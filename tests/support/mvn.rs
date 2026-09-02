//! `mvn`, memoised: the same build, proven once per distinct input.
//!
//! **A generated project is a function of the model, so its proof is a
//! function of its bytes.** A real-toolchain test proves that one exact tree
//! compiles and passes its own tests under one toolchain. Running Maven again
//! over byte-identical inputs proves nothing the last green run did not, and
//! costs a JVM booting a Spring context, which is where the suite's wall clock
//! goes. So this wrapper keys every run on the project tree (everything but
//! `target/` and the build tool's own state), the argv, the environment Maven
//! and Spring read, and the identity of the `mvn` and `java` that would run,
//! and replays a recorded green run when the key has been proven.
//!
//! **A hit is a run, not a shortcut.** What is replayed is everything the run
//! left behind: the exit status, stdout and stderr, the after-image of
//! `target/` (so a later `jails test --fast` over the compiled classes, or a
//! report reader, sees what a real run leaves), every file the run changed
//! outside it (Spotless formatting source), and any output file the argv
//! named by absolute path. Only a successful run is recorded, so a hit can
//! never turn a failing proof green; a miss runs Maven exactly as before.
//!
//! Built as the cargo example named `mvn`, so a `--debug` line the product
//! prints still reads `.../mvn compile`. The harness points `JAILS_MAVEN` and
//! its own Maven calls here; without `JAILS_PROOF_CACHE` in the environment it
//! is a plain pass-through. `JAILS_PROOF_CACHE_MAVEN` names the real command.
//! Not production code: it writes files, which `jails_support::apply` is the
//! only production module allowed to do, and it lives under `tests/`.

use jails_support::{hex, sha256};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

const SCHEMA: &str = "jails-proof-cache.v2";
const CWD_PLACEHOLDER: &str = "@JAILS_PROOF_CACHE_CWD@";
/// Directories a build tool owns or that carry no input: their bytes are not
/// the project's, and `target/` is replayed whole rather than keyed.
const NOT_AN_INPUT: &[&str] = &[
    "target",
    "build",
    ".git",
    ".gradle",
    ".idea",
    "node_modules",
];
/// Environment Maven, the JVM, Spring and Testcontainers read. `PATH` is not
/// here: which `mvn` and `java` it resolves to is keyed instead. Nor is
/// `JAILS_*`: those are the harness's and the product's switches, and none
/// of them reaches the build.
const KEYED_ENV_PREFIXES: &[&str] = &[
    "MAVEN_",
    "JAVA_",
    "JDK_",
    "SPRING_",
    "TESTCONTAINERS_",
    "DOCKER_",
];

fn main() -> ExitCode {
    let argv: Vec<OsString> = std::env::args_os().skip(1).collect();
    let real = std::env::var_os("JAILS_PROOF_CACHE_MAVEN")
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| OsString::from("mvn"));
    let Some(cache) = std::env::var_os("JAILS_PROOF_CACHE")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
    else {
        return pass_through(&real, &argv);
    };
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(error) => {
            eprintln!("jails-proof-cache: no working directory ({error}); running mvn");
            return pass_through(&real, &argv);
        }
    };
    let before = tree(&cwd);
    let run = Run {
        argv: &argv,
        real: &real,
        cwd: &cwd,
        before: &before,
    };
    let key = key(&run);
    let entry = cache.join("entries").join(&key);
    if entry.join("meta.json").is_file() {
        match replay(&entry, &cwd) {
            Ok(code) => return code,
            Err(error) => {
                eprintln!("jails-proof-cache: replaying {key} failed ({error}); running mvn");
            }
        }
    }
    note_miss(&cache, &key, &run);
    let (code, stdout, stderr) = match execute(&real, &argv) {
        Ok(run) => run,
        Err(error) => {
            eprintln!(
                "jails-proof-cache: could not run {}: {error}",
                real.to_string_lossy()
            );
            return ExitCode::FAILURE;
        }
    };
    if code == 0
        && let Err(error) = record(&cache, &key, &run, &stdout, &stderr)
    {
        eprintln!("jails-proof-cache: could not record {key} ({error}); the run stands");
    }
    exit_code(code)
}

/// One line per miss in `misses.log`, so a run that should have replayed
/// and did not can be traced to the key that changed: the entry keeps the
/// text the key was hashed from, and two of them diff to the input that moved.
fn note_miss(cache: &Path, key: &str, run: &Run<'_>) {
    let _ = fs::create_dir_all(cache);
    if let Ok(mut log) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(cache.join("misses.log"))
    {
        let argv = run
            .argv
            .iter()
            .map(|arg| arg.to_string_lossy())
            .collect::<Vec<_>>()
            .join(" ");
        let _ = writeln!(log, "{key}\t{}\t{argv}", run.cwd.display());
    }
}

fn pass_through(real: &OsStr, argv: &[OsString]) -> ExitCode {
    match Command::new(real).args(argv).status() {
        Ok(status) => exit_code(status.code().unwrap_or(1)),
        Err(error) => {
            eprintln!(
                "jails-proof-cache: could not run {}: {error}",
                real.to_string_lossy()
            );
            ExitCode::FAILURE
        }
    }
}

fn exit_code(code: i32) -> ExitCode {
    ExitCode::from(u8::try_from(code).unwrap_or(1))
}

/// Every input file below `root`, by project-relative path: whether it is
/// executable and the digest of its bytes.
fn tree(root: &Path) -> BTreeMap<String, (bool, String)> {
    let mut files = BTreeMap::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let name = entry.file_name().to_string_lossy().into_owned();
            if kind.is_dir() {
                if name == ".jails" && dir == root {
                    // Maven reads the generated source root and nothing
                    // else under `.jails/`: the model spells the project
                    // name the directory gave it, and the lock carries the
                    // model's digest, so keying them would make every scratch
                    // directory a different proof of the same Java.
                    let generated = path.join("generated");
                    if generated.is_dir() {
                        pending.push(generated);
                    }
                } else if !(NOT_AN_INPUT.contains(&name.as_str())
                    || name.starts_with(".jails-staged-"))
                {
                    pending.push(path);
                }
            } else if kind.is_file()
                && let Ok(bytes) = fs::read(&path)
                && let Ok(relative) = path.strip_prefix(root)
            {
                files.insert(
                    relative.to_string_lossy().replace('\\', "/"),
                    (is_executable(&path), hex(&sha256(&bytes))),
                );
            }
        }
    }
    files
}

fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::metadata(path).is_ok_and(|meta| meta.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        false
    }
}

/// One Maven invocation as the cache sees it: what was asked, of which
/// Maven, where, over which input bytes.
struct Run<'a> {
    argv: &'a [OsString],
    real: &'a OsStr,
    cwd: &'a Path,
    before: &'a BTreeMap<String, (bool, String)>,
}

fn key(run: &Run<'_>) -> String {
    hex(&sha256(key_text(run).as_bytes()))
}

/// What the key hashes, kept beside every entry as `key.txt`.
fn key_text(run: &Run<'_>) -> String {
    let Run {
        argv,
        real,
        cwd,
        before,
    } = *run;
    let mut body = format!("{SCHEMA}\n");
    for arg in argv {
        // `-Dmdep.outputFile=<project>/target/…` names the directory and
        // `-Dspring.datasource.url=jdbc:postgresql://127.0.0.1:36993/…` the
        // port a service was started on; the same run in another scratch
        // directory, against the same service on another port, is the same
        // proof. Which service it was is `JAILS_PROOF_CACHE_KEY`'s to say.
        let arg = String::from_utf8_lossy(&placeholder(arg.as_encoded_bytes(), cwd)).into_owned();
        body.push_str(&format!("arg\t{}\n", without_loopback_ports(&arg)));
    }
    if let Ok(extra) = std::env::var("JAILS_PROOF_CACHE_KEY") {
        body.push_str(&format!("key\t{extra}\n"));
    }
    let mut env: Vec<(String, String)> = std::env::vars()
        .filter(|(name, _)| {
            KEYED_ENV_PREFIXES
                .iter()
                .any(|prefix| name.starts_with(prefix))
        })
        .collect();
    env.sort();
    for (name, value) in env {
        body.push_str(&format!("env\t{name}\t{value}\n"));
    }
    body.push_str(&format!("mvn\t{}\n", tool_identity(real)));
    let java = std::env::var_os("JAVA_HOME")
        .filter(|home| !home.is_empty())
        .map_or_else(
            || OsString::from("java"),
            |home| Path::new(&home).join("bin").join("java").into_os_string(),
        );
    body.push_str(&format!("java\t{}\n", tool_identity(&java)));
    for (path, (executable, digest)) in before {
        body.push_str(&format!(
            "file\t{path}\t{}\t{digest}\n",
            u8::from(*executable)
        ));
    }
    body
}

/// `127.0.0.1:<port>` and `localhost:<port>` with the port blanked.
fn without_loopback_ports(arg: &str) -> String {
    let mut out = String::with_capacity(arg.len());
    let mut rest = arg;
    while let Some(at) = ["127.0.0.1:", "localhost:"]
        .iter()
        .filter_map(|host| rest.find(host).map(|index| (index, host.len())))
        .min()
    {
        let (index, host_len) = at;
        let after = &rest[index + host_len..];
        let digits = after.bytes().take_while(u8::is_ascii_digit).count();
        out.push_str(&rest[..index + host_len]);
        if digits > 0 {
            out.push_str("<port>");
        }
        rest = &after[digits..];
    }
    out.push_str(rest);
    out
}

/// Which binary a name resolves to and what it is: the resolved path (a
/// version-managed install carries its version in the path), its length and
/// its modification time. `mvn --version` and `java -version` would answer
/// more exactly and cost a JVM start each, which is what this exists to avoid.
fn tool_identity(name: &OsStr) -> String {
    let resolved = resolve(name);
    let meta = resolved.as_deref().and_then(|path| fs::metadata(path).ok());
    format!(
        "{}\t{}\t{}",
        resolved.as_deref().map_or_else(
            || name.to_string_lossy().into_owned(),
            |path| path.display().to_string()
        ),
        meta.as_ref().map_or(0, fs::Metadata::len),
        meta.and_then(|meta| meta.modified().ok())
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |since| since.as_nanos())
    )
}

fn resolve(name: &OsStr) -> Option<PathBuf> {
    let candidate = Path::new(name);
    if candidate.components().count() > 1 {
        return candidate.is_file().then(|| candidate.to_path_buf());
    }
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|dir| dir.join(name))
        .find(|path| path.is_file())
}

/// Run the real Maven, streaming its output as it arrives and keeping a copy.
fn execute(real: &OsStr, argv: &[OsString]) -> io::Result<(i32, Vec<u8>, Vec<u8>)> {
    let mut child = Command::new(real)
        .args(argv)
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let stdout = child.stdout.take().expect("piped");
    let stderr = child.stderr.take().expect("piped");
    let out = std::thread::spawn(move || tee(stdout, io::stdout()));
    let err = std::thread::spawn(move || tee(stderr, io::stderr()));
    let status = child.wait()?;
    let stdout = out.join().unwrap_or_default();
    let stderr = err.join().unwrap_or_default();
    Ok((status.code().unwrap_or(1), stdout, stderr))
}

fn tee(mut from: impl Read, mut to: impl Write) -> Vec<u8> {
    let mut kept = Vec::new();
    let mut chunk = [0u8; 8192];
    while let Ok(read) = from.read(&mut chunk)
        && read > 0
    {
        let _ = to.write_all(&chunk[..read]);
        let _ = to.flush();
        kept.extend_from_slice(&chunk[..read]);
    }
    kept
}

/// Absolute paths the argv names as outputs (`-Dmdep.outputFile=/…`) that
/// lie outside the project: written by the run, so replayed with it.
fn named_outputs(argv: &[OsString], cwd: &Path) -> Vec<PathBuf> {
    argv.iter()
        .filter_map(|arg| arg.to_str())
        .filter_map(|arg| arg.split_once('=').map(|(_, value)| PathBuf::from(value)))
        .filter(|path| path.is_absolute() && !path.starts_with(cwd))
        .collect()
}

fn record(cache: &Path, key: &str, run: &Run<'_>, stdout: &[u8], stderr: &[u8]) -> io::Result<()> {
    let Run {
        argv, cwd, before, ..
    } = *run;
    let entries = cache.join("entries");
    fs::create_dir_all(&entries)?;
    let staging = entries.join(format!("{key}.tmp{}", std::process::id()));
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging)?;
    let result = (|| -> io::Result<()> {
        fs::write(staging.join("key.txt"), key_text(run))?;
        fs::write(staging.join("stdout"), placeholder(stdout, cwd))?;
        fs::write(staging.join("stderr"), placeholder(stderr, cwd))?;
        let after = tree(cwd);
        let mut files = Vec::new();
        for (path, (executable, digest)) in &after {
            if before
                .get(path)
                .is_some_and(|(e, d)| e == executable && d == digest)
            {
                continue;
            }
            let copy = staging.join("files").join(path);
            if let Some(parent) = copy.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(cwd.join(path), &copy)?;
            files.push(json!([path, executable]));
        }
        let deleted: Vec<Value> = before
            .keys()
            .filter(|path| !after.contains_key(*path))
            .map(|path| json!(path))
            .collect();
        let target = cwd.join("target").is_dir();
        if target {
            // Rewritten into a staging copy first, then archived: a jar
            // passes through untouched, a report loses the directory name.
            copy_tree(&cwd.join("target"), &staging.join("target"), &|bytes| {
                placeholder(bytes, cwd)
            })?;
            let archived = Command::new("tar")
                .args(["--zstd", "-cf"])
                .arg(staging.join("target.tar.zst"))
                .arg("-C")
                .arg(&staging)
                .arg("target")
                .status()?;
            if !archived.success() {
                return Err(io::Error::other("tar could not archive target/"));
            }
            fs::remove_dir_all(staging.join("target"))?;
        }
        let mut outputs = Vec::new();
        for (index, path) in named_outputs(argv, cwd).into_iter().enumerate() {
            if path.is_file() {
                let copy = staging.join("outputs").join(index.to_string());
                fs::create_dir_all(staging.join("outputs"))?;
                fs::copy(&path, &copy)?;
                outputs.push(json!([path.display().to_string(), index]));
            }
        }
        let meta = json!({
            "schema": SCHEMA,
            "key": key,
            "argv": argv.iter().map(|arg| arg.to_string_lossy()).collect::<Vec<_>>(),
            "code": 0,
            "files": files,
            "deleted": deleted,
            "target": target,
            "outputs": outputs,
        });
        // Written last: an entry is complete exactly when its meta exists,
        // and the rename below publishes the whole directory at once.
        fs::write(staging.join("meta.json"), serde_json::to_vec_pretty(&meta)?)?;
        Ok(())
    })();
    match result {
        Ok(()) => {
            let entry = entries.join(key);
            if fs::rename(&staging, &entry).is_err() {
                // Another process proved the same key first; theirs stands.
                let _ = fs::remove_dir_all(&staging);
            }
            Ok(())
        }
        Err(error) => {
            let _ = fs::remove_dir_all(&staging);
            Err(error)
        }
    }
}

fn replay(entry: &Path, cwd: &Path) -> io::Result<ExitCode> {
    let meta: Value = serde_json::from_slice(&fs::read(entry.join("meta.json"))?)?;
    if meta["schema"] != SCHEMA {
        return Err(io::Error::other("unknown entry schema"));
    }
    let stdout = fs::read(entry.join("stdout"))?;
    let stderr = fs::read(entry.join("stderr"))?;
    let target = cwd.join("target");
    if target.exists() {
        fs::remove_dir_all(&target)?;
    }
    if meta["target"] == true {
        // Extracted beside the entry and copied in with today's dates, so a
        // staleness check comparing the classes against freshly written
        // sources sees a build newer than its inputs, as a real run leaves.
        let unpacked = entry.join(format!("unpack.{}", std::process::id()));
        fs::create_dir_all(&unpacked)?;
        let extracted = Command::new("tar")
            .args(["--zstd", "-xf"])
            .arg(entry.join("target.tar.zst"))
            .arg("-C")
            .arg(&unpacked)
            .status()?;
        if !extracted.success() {
            let _ = fs::remove_dir_all(&unpacked);
            return Err(io::Error::other("tar could not restore target/"));
        }
        let copied = copy_tree(&unpacked.join("target"), &target, &|bytes| {
            restore(bytes, cwd)
        });
        let _ = fs::remove_dir_all(&unpacked);
        copied?;
    }
    for file in meta["files"].as_array().into_iter().flatten() {
        let (Some(path), Some(executable)) = (file[0].as_str(), file[1].as_bool()) else {
            continue;
        };
        let destination = cwd.join(path);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(entry.join("files").join(path), &destination)?;
        set_executable(&destination, executable)?;
    }
    for path in meta["deleted"].as_array().into_iter().flatten() {
        if let Some(path) = path.as_str() {
            let _ = fs::remove_file(cwd.join(path));
        }
    }
    for output in meta["outputs"].as_array().into_iter().flatten() {
        let (Some(path), Some(index)) = (output[0].as_str(), output[1].as_u64()) else {
            continue;
        };
        if let Some(parent) = Path::new(path).parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(entry.join("outputs").join(index.to_string()), path)?;
    }
    io::stdout().write_all(&restore(&stdout, cwd))?;
    io::stderr().write_all(&restore(&stderr, cwd))?;
    let code = i32::try_from(meta["code"].as_i64().unwrap_or(0)).unwrap_or(1);
    Ok(exit_code(code))
}

/// `from` copied under `to`, every file's bytes passed through `rewrite`.
///
/// The build tool's own output is part of what a run leaves, and some of it
/// spells the project directory -- a surefire report records `user.dir` --
/// so it is stored with the placeholder and restored with the real path,
/// like stdout. A file that is not text passes through unchanged, because
/// the placeholder is only ever substituted into what parsed as UTF-8.
fn copy_tree(from: &Path, to: &Path, rewrite: &dyn Fn(&[u8]) -> Vec<u8>) -> io::Result<()> {
    let mut pending = vec![from.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let destination = to.join(dir.strip_prefix(from).expect("below from"));
        fs::create_dir_all(&destination)?;
        for entry in fs::read_dir(&dir)?.flatten() {
            let path = entry.path();
            let kind = entry.file_type()?;
            if kind.is_dir() {
                pending.push(path);
            } else if kind.is_file() {
                let bytes = fs::read(&path)?;
                let bytes = if std::str::from_utf8(&bytes).is_ok() {
                    rewrite(&bytes)
                } else {
                    bytes
                };
                let copy = destination.join(entry.file_name());
                fs::write(&copy, bytes)?;
                set_executable(&copy, is_executable(&path))?;
            }
        }
    }
    Ok(())
}

fn set_executable(path: &Path, executable: bool) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(path)?.permissions();
        let mode = permissions.mode();
        permissions.set_mode(if executable {
            mode | 0o111
        } else {
            mode & !0o111
        });
        fs::set_permissions(path, permissions)
    }
    #[cfg(not(unix))]
    {
        let _ = (path, executable);
        Ok(())
    }
}

/// The recorded output with every spelling of the project directory replaced,
/// so a run recorded in one scratch directory replays in another.
fn placeholder(output: &[u8], cwd: &Path) -> Vec<u8> {
    let mut text = String::from_utf8_lossy(output).into_owned();
    for spelling in spellings(cwd) {
        text = text.replace(&spelling, CWD_PLACEHOLDER);
    }
    text.into_bytes()
}

fn restore(output: &[u8], cwd: &Path) -> Vec<u8> {
    String::from_utf8_lossy(output)
        .replace(CWD_PLACEHOLDER, &cwd.display().to_string())
        .into_bytes()
}

fn spellings(cwd: &Path) -> Vec<String> {
    let mut spellings = vec![cwd.display().to_string()];
    if let Ok(canonical) = cwd.canonicalize() {
        let canonical = canonical.display().to_string();
        if !spellings.contains(&canonical) {
            spellings.push(canonical);
        }
    }
    // Longest first, so a prefix never eats the longer spelling's tail.
    spellings.sort_by_key(|spelling| std::cmp::Reverse(spelling.len()));
    spellings
}
