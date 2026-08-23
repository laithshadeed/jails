//! `jails testd` -- run tests against a resident JVM.
//!
//! **The measurement this exists for.** `plan.md` §19.1 found `jails test
//! --fast` and `mvnd` both sitting at ~0.6 s for one test method, with a cold
//! `java` process making up what is left in each. §19.2 then measured where
//! that goes: the *first* JUnit session in a JVM costs 464 ms against 20 ms
//! warm (758 ms against 83 ms on a 151-class project). A daemon pays it once.
//!
//! **What it deliberately does not do.** §10.2's sketch had the daemon hold a
//! `JavaCompiler` and compile in-process. §19.5's measurement removed the
//! reason: the editor's language server already writes `target/classes` on
//! save, so the compile is happening anyway, by something that has the whole
//! project's model rather than one changed file. The daemon runs what is on
//! disk and [`launcher::staleness`] refuses when a source is newer -- the same
//! gate `--fast` uses, and one fewer thing to be subtly wrong about, since
//! §10.2 itself records that compiling only the changed file is unsound.
//!
//! **Why the classpath is split in two.** The daemon's own classpath carries
//! the dependencies and nothing else; the project's `target/classes` and
//! `target/test-classes` are handed to JUnit as `--class-path`, which builds a
//! child loader for them per run and closes it afterwards. Freshness therefore
//! comes from JUnit rather than from code here. Putting the outputs on the
//! daemon's classpath as well would look harmless and be the worst possible
//! bug: parent-first delegation would serve the stale class on every run, so
//! the daemon would report green over code that no longer exists.
//!
//! **It is a Java program, not a jails jar.** The daemon source is a template
//! compiled by `java`'s single-file source launcher at start-up, and nothing
//! about it enters the project: no dependency, no artifact, no import. The
//! scope bar is about generated projects depending on jails, and this does not
//! cross it.

use crate::Result;
use crate::affected;
use crate::build;
use crate::launcher;
use crate::model::Project;
use crate::process::CommandSpec;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Ends a response. Cannot occur in JUnit's output, which is text.
const END: u8 = 4;

/// How long a daemon waits for work before exiting.
///
/// A daemon that outlives the session it was started for is a JVM holding half
/// a gigabyte for a directory nobody has open. Thirty minutes is longer than
/// any edit-test gap and far shorter than a working day.
const IDLE_SECONDS: u64 = 1800;

/// How long to wait for a daemon to bind its socket before giving up on it.
const START_TIMEOUT: Duration = Duration::from_secs(90);

pub(crate) enum Action {
    /// Run the tests this filter selects, starting a daemon if needed.
    Run(Option<String>),
    /// Run only the tests reachable from what has changed in the working tree.
    Affected,
    /// Stop this project's daemon, if one is running.
    Stop,
    /// Say whether one is running, and where.
    Status,
}

pub(crate) fn testd(action: Action, debug: bool) -> Result<()> {
    let project = Project::discover()?;
    build::require_maven(project.build(), "testd")?;
    let root = project.root();
    let socket = socket_path(root)?;

    match action {
        Action::Stop => {
            if request(&socket, &["STOP".into()]).is_ok() {
                println!("testd: stopped");
            } else {
                println!("testd: not running");
            }
            Ok(())
        }
        Action::Status => {
            match request(&socket, &["PING".into()]) {
                Ok(_) => println!("testd: running ({})", socket.display()),
                Err(_) => println!("testd: not running"),
            }
            Ok(())
        }
        Action::Run(filter) => run(&project, &socket, Wanted::Filter(filter), debug),
        Action::Affected => run(&project, &socket, Wanted::Affected, debug),
    }
}

/// What this run should execute.
enum Wanted {
    /// A `jails test`-style filter, or everything when it is `None`.
    Filter(Option<String>),
    /// Whatever the working tree's changes can reach.
    Affected,
}

fn run(project: &Project, socket: &Path, wanted: Wanted, debug: bool) -> Result<()> {
    let root = project.root();

    // The same refusal `--fast` makes, for the same reason: a run over classes
    // older than their sources is green about code that is gone.
    if let Some(stale) = launcher::staleness(root) {
        return Err(format!(
            "testd not taken: {}\n  fix: jails test",
            stale.explain()
        ));
    }
    // Decided before the daemon is touched: `--affected` can conclude there is
    // nothing to run, and starting a JVM to be told that is pure waste.
    let selectors = match &wanted {
        Wanted::Filter(None) => {
            // Nothing selected means "everything the output directories hold",
            // which is what `--class-path` already names -- so the scan is
            // over exactly the classpath JUnit was handed, and no wider.
            vec!["--scan-class-path".to_string()]
        }
        Wanted::Filter(Some(filter)) => match launcher::fully_qualified(root, filter) {
            Some(name) => launcher::selectors(Some(&name)),
            None => {
                return Err(format!(
                    "testd not taken: could not resolve `{filter}` to a fully qualified name.\n  \
                     fix: jails test {filter}"
                ));
            }
        },
        Wanted::Affected => match affected::select(root, debug) {
            affected::Selection::Nothing => {
                println!("testd: nothing has changed under src/main/java or src/test/java");
                return Ok(());
            }
            affected::Selection::Everything(reason) => {
                // Loud, and it says which of the unknowns it hit. A selector
                // that silently widened would be indistinguishable from one
                // that was simply not selecting.
                println!("testd: running everything -- {reason}");
                vec!["--scan-class-path".to_string()]
            }
            affected::Selection::Tests(tests) => {
                println!(
                    "testd: {} test class(es) reachable from the working tree's changes",
                    tests.len()
                );
                tests
                    .iter()
                    .map(|name| format!("--select-class={name}"))
                    .collect()
            }
        },
    };

    let classpath = launcher::test_classpath(root, debug)?;
    ensure_running(project, socket, &classpath, debug)?;

    let mut message = vec!["RUN".to_string()];
    message.extend(selectors);
    message.push("--details=testfeed".into());

    let (output, code) = request(socket, &message)?;
    print!("{output}");
    std::io::stdout().flush().ok();
    if code == 0 {
        Ok(())
    } else {
        // An empty Err: JUnit has already printed the failures, and a second
        // `jails: ` line over them says nothing. Same convention as `doctor`.
        Err(String::new())
    }
}

/// Connect, or start a daemon and wait for it to bind.
fn ensure_running(
    project: &Project,
    socket: &Path,
    classpath: &launcher::TestClasspath,
    debug: bool,
) -> Result<()> {
    let root = project.root();
    if request(socket, &["PING".into()]).is_ok() {
        if !pom_moved_since(socket, &root.join("pom.xml")) {
            return Ok(());
        }
        // A daemon holds its classpath from the moment it started, so one that
        // predates a `jails add` is running against the dependencies the
        // project had *before* the capability was installed. The tests then
        // fail on a missing class with nothing to connect it to the add --
        // which is worse than a slow run, so the daemon is replaced rather
        // than reused.
        println!("testd: the pom changed, restarting the daemon");
        let _ = request(socket, &["STOP".into()]);
    }
    let source = daemon_source()?;
    let outputs = std::env::join_paths(&classpath.outputs)
        .map_err(|error| format!("failed to join classpath: {error}"))?;
    let dependencies = std::env::join_paths(&classpath.dependencies)
        .map_err(|error| format!("failed to join classpath: {error}"))?;

    // The console launcher is what runs the tests, and it must be the
    // project's own JUnit version -- see `launcher::console_version` for the
    // `NoSuchMethodError` a guessed pin produces. `jails test --fast` splices
    // it, so the two paths share one dependency rather than each having their
    // own idea of which JUnit this is.
    if !project.pom().contains("junit-platform-console") {
        return Err(
            "testd needs junit-platform-console on the test classpath.\n  \
             fix: jails test --fast (it splices the dependency, pinned to this project's JUnit)"
                .into(),
        );
    }

    let spec = CommandSpec::new("java")
        .arg("-cp")
        .arg(&dependencies)
        .arg(&source)
        .arg(socket)
        .arg(IDLE_SECONDS.to_string())
        .arg(&outputs)
        .current_dir(root);
    let mut child = crate::process::spawn(&spec, crate::process::Diagnostics::from_flag(debug))?;

    // Wait for the socket, not for a fixed delay: the first start compiles the
    // daemon source and warms the JUnit engine, which is exactly the cost this
    // whole command exists to move off the critical path -- but only once.
    let started = Instant::now();
    while started.elapsed() < START_TIMEOUT {
        if request(socket, &["PING".into()]).is_ok() {
            return Ok(());
        }
        if let Ok(Some(status)) = child.try_wait() {
            let mut stderr = String::new();
            if let Some(mut pipe) = child.stderr.take() {
                pipe.read_to_string(&mut stderr).ok();
            }
            return Err(format!(
                "testd: the daemon exited with {status} before it was ready.\n{}  \
                 fix: jails test --fast",
                indent(&stderr)
            ));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let _ = child.kill();
    Err("testd: the daemon did not become ready in time.\n  fix: jails test --fast".into())
}

/// Whether the pom has changed since the daemon bound this socket.
///
/// The socket is created at start-up and deleted at exit, so its mtime is the
/// daemon's own start time -- no state file, and nothing to leave behind.
///
/// Takes the pom rather than the root because the pom is what it reads; a
/// `root: &Path` here would be `abstract.md` §8.0's containment-as-parameter
/// in miniature, and the caller already knows where the pom is.
fn pom_moved_since(socket: &Path, pom: &Path) -> bool {
    let Ok(started) = std::fs::metadata(socket).and_then(|meta| meta.modified()) else {
        return false;
    };
    match std::fs::metadata(pom).and_then(|meta| meta.modified()) {
        Ok(changed) => changed > started,
        Err(_) => false,
    }
}

fn indent(text: &str) -> String {
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| format!("  {line}\n"))
        .collect()
}

/// One request, one response. Returns the output and JUnit's exit code.
fn request(socket: &Path, message: &[String]) -> Result<(String, i32)> {
    let mut stream =
        UnixStream::connect(socket).map_err(|error| format!("testd: not running ({error})"))?;
    let mut payload = message.join("\n");
    payload.push_str("\n\n");
    stream
        .write_all(payload.as_bytes())
        .map_err(|error| format!("testd: could not send the request ({error})"))?;
    let mut buffer = Vec::new();
    stream
        .read_to_end(&mut buffer)
        .map_err(|error| format!("testd: could not read the reply ({error})"))?;
    split_reply(&buffer)
}

/// Split a reply at its terminator into output and exit code.
fn split_reply(buffer: &[u8]) -> Result<(String, i32)> {
    let end = buffer
        .iter()
        .rposition(|byte| *byte == END)
        .ok_or_else(|| "testd: the reply was truncated".to_string())?;
    let output = String::from_utf8_lossy(&buffer[..end]).into_owned();
    let code = String::from_utf8_lossy(&buffer[end + 1..])
        .trim()
        .parse()
        .map_err(|_| "testd: the reply had no exit code".to_string())?;
    Ok((output, code))
}

/// The daemon source on disk, written from the template if it is not there.
///
/// Keyed by jails' own version so an upgraded jails cannot talk to a daemon
/// built from the previous source -- the failure that would cause is a
/// protocol mismatch, which reads as a hang rather than as an error.
fn daemon_source() -> Result<PathBuf> {
    const SOURCE: &str = include_str!("../templates/testd/JailsTestDaemon.java");
    let dir = cache_dir()?.join(env!("CARGO_PKG_VERSION"));
    std::fs::create_dir_all(&dir)
        .map_err(|error| format!("failed to create {}: {error}", dir.display()))?;
    let path = dir.join("JailsTestDaemon.java");
    if std::fs::read_to_string(&path).ok().as_deref() != Some(SOURCE) {
        crate::apply::put_outside_project(&path, SOURCE)?;
    }
    Ok(path)
}

fn cache_dir() -> Result<PathBuf> {
    if let Some(dir) = std::env::var_os("XDG_CACHE_HOME") {
        return Ok(PathBuf::from(dir).join("jails/testd"));
    }
    std::env::var_os("HOME")
        .map(|home| PathBuf::from(home).join(".cache/jails/testd"))
        .ok_or_else(|| "testd: no HOME to put the daemon's socket under".to_string())
}

/// One socket per project, named by the project rather than by its path.
///
/// The path is hashed because a unix socket address is capped at ~104 bytes on
/// some platforms and a project path can easily exceed it -- a limit that
/// shows up as an unexplained bind failure. The directory name is kept in
/// front of the hash so `ls` in the cache says which project a daemon is for.
fn socket_path(root: &Path) -> Result<PathBuf> {
    let canonical = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let name = canonical
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "project".into());
    let name: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .take(24)
        .collect();
    Ok(cache_dir()?.join(format!("{name}-{:016x}.sock", fnv1a(canonical.as_os_str()))))
}

/// FNV-1a. jails' dependencies are clap and clap_complete, and a hash used
/// only to name a socket does not justify a third.
fn fnv1a(value: &std::ffi::OsStr) -> u64 {
    use std::os::unix::ffi::OsStrExt;
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reply_splits_into_output_and_exit_code() {
        let mut buffer = b"two tests passed\n".to_vec();
        buffer.push(END);
        buffer.extend_from_slice(b"0\n");
        assert_eq!(
            split_reply(&buffer).unwrap(),
            ("two tests passed\n".to_string(), 0)
        );
    }

    /// The terminator is searched for from the *end*, so output that somehow
    /// contained one still yields the real exit code rather than a parse
    /// failure that would be reported as a broken daemon.
    #[test]
    fn a_terminator_inside_the_output_does_not_confuse_the_split() {
        let mut buffer = b"weird \x04 output\n".to_vec();
        buffer.push(END);
        buffer.extend_from_slice(b"1\n");
        let (output, code) = split_reply(&buffer).unwrap();
        assert_eq!(code, 1);
        assert!(output.starts_with("weird"));
    }

    #[test]
    fn a_truncated_reply_is_an_error_rather_than_a_silent_success() {
        assert!(split_reply(b"no terminator").is_err());
    }

    /// Two projects must never share a daemon: the classpath differs, so a
    /// shared one would run one project's tests against the other's jars.
    #[test]
    fn each_project_gets_its_own_socket() {
        let a = socket_path(Path::new("/tmp/jails-testd-a")).unwrap();
        let b = socket_path(Path::new("/tmp/jails-testd-b")).unwrap();
        assert_ne!(a, b);
        assert_eq!(a, socket_path(Path::new("/tmp/jails-testd-a")).unwrap());
    }

    /// A socket address is capped at ~104 bytes on some platforms, and the
    /// failure is an unexplained bind error rather than a message about names.
    #[test]
    fn a_deeply_nested_project_still_gets_a_short_socket_name() {
        let deep = PathBuf::from("/tmp").join("x".repeat(200)).join("service");
        let socket = socket_path(&deep).unwrap();
        assert!(socket.file_name().unwrap().len() < 60);
    }
}
