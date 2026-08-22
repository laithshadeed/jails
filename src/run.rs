use crate::Result;
use crate::compose;
use crate::generate::find_project_root;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// One PATH lookup, in `process`. `run.rs`, `compose.rs` and `project.rs`
/// each had their own copy, which is how the mvnd naming drifted between
/// them.
pub(crate) fn find_on_path(bin: &str) -> bool {
    crate::process::on_path(bin)
}

/// The name mvnd is installed under. On Windows it ships as `mvnd.cmd`, so
/// probing for a bare `mvnd` there finds nothing and silently falls back to
/// `mvn`.
///
/// This is the whole reason there is one resolver: `project.rs` had its own
/// copy that got this right while this one got it wrong, so on Windows
/// `jails about` reported a Maven command that `jails test` would not have
/// used -- and this path would then have tried to execute a name that is not
/// on disk.
fn mvnd_binary() -> &'static str {
    if cfg!(windows) { "mvnd.cmd" } else { "mvnd" }
}

/// Prefer the project's wrapper so its Maven version is reproducible. A
/// project without one keeps the fast mvnd/system-Maven fallback.
///
/// The one place this is decided. `project.rs` reports it, `run.rs` executes
/// it, and the two disagreeing is how you get a tool that describes a build
/// it does not run.
pub(crate) fn maven_binary(root: &Path) -> PathBuf {
    let wrapper = root.join(if cfg!(windows) { "mvnw.cmd" } else { "mvnw" });
    if wrapper.is_file() {
        return wrapper;
    }
    if find_on_path(mvnd_binary()) {
        PathBuf::from(mvnd_binary())
    } else {
        PathBuf::from("mvn")
    }
}

/// Run a command with our stdio, failing on a non-zero exit.
///
/// A thin adapter over the one executor: the callers here build a
/// `std::process::Command` directly, and converting all of them at once would
/// be a large diff for no behaviour change. What matters is that the printing,
/// spawning and exit-status handling happen in one place -- the executor
/// prints *and then runs*, which is the property that was violated where each
/// site decided for itself.
pub(crate) fn run_inherited(cmd: Command, debug: bool) -> Result<()> {
    let mut spec = crate::process::CommandSpec::new(cmd.get_program())
        .args(cmd.get_args())
        .output(crate::process::OutputMode::Inherit);
    if let Some(dir) = cmd.get_current_dir() {
        spec = spec.current_dir(dir);
    }
    for (key, value) in cmd.get_envs() {
        if let Some(value) = value {
            spec = spec.env(key, value);
        }
    }
    crate::process::run_checked(&spec, crate::process::Diagnostics::from_flag(debug)).map(|_| ())
}

/// Run a command, echoing its output live while keeping a copy, and treat
/// a *successful* exit with fatal output in it as a failure.
///
/// `run_inherited` cannot do this: it hands the child our stdio and never
/// sees a byte. That is fine for `build` and `test`, where Maven's exit code
/// is the truth. It is not fine for `run`: spring-boot-devtools runs `main`
/// on its own thread, catches the startup exception there, and lets Maven
/// print BUILD SUCCESS over a dead application -- so `jails run` reported
/// success for an app that never came up.
///
/// Piping costs the child its terminal, and a program that cannot see a
/// terminal turns colour off, so the caller passes `color_args` to force it
/// back on. Only stdout and stderr are piped; stdin stays inherited, so an
/// interactive program still reads the keyboard.
fn run_watched(mut cmd: Command, debug: bool) -> Result<()> {
    use std::io::Read as _;

    cmd.stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    if debug {
        crate::debug_cmd(&cmd);
    }
    let program = cmd.get_program().to_string_lossy().to_string();
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("failed to run {program}: {e}"))?;

    // stderr on its own thread: reading the two pipes in sequence would
    // deadlock the moment the child filled the one we are not reading.
    let stderr = child.stderr.take();
    let collector = std::thread::spawn(move || {
        let mut captured = String::new();
        if let Some(mut stderr) = stderr {
            let mut chunk = [0u8; 4096];
            while let Ok(n) = stderr.read(&mut chunk) {
                if n == 0 {
                    break;
                }
                let text = String::from_utf8_lossy(&chunk[..n]);
                eprint!("{text}");
                captured.push_str(&text);
            }
        }
        captured
    });

    let mut log = String::new();
    if let Some(mut stdout) = child.stdout.take() {
        let mut chunk = [0u8; 4096];
        while let Ok(n) = stdout.read(&mut chunk) {
            if n == 0 {
                break;
            }
            let text = String::from_utf8_lossy(&chunk[..n]);
            print!("{text}");
            std::io::Write::flush(&mut std::io::stdout()).ok();
            log.push_str(&text);
        }
    }
    let status = child
        .wait()
        .map_err(|e| format!("failed to wait for {program}: {e}"))?;
    if let Ok(errors) = collector.join() {
        log.push_str(&errors);
    }

    if !status.success() {
        return Err(format!("{program} exited with {status}"));
    }
    if !crate::why::looks_fatal(&log) {
        return Ok(());
    }

    println!();
    println!("jails: the application failed to start, even though {program} reported success.");
    println!("(spring-boot-devtools runs main on its own thread and swallows the exception.)");
    println!();
    if crate::why::report(&log) == 0 {
        println!("jails does not recognise this failure. `jails doctor` checks everything that");
        println!("has to be true before the app can start.");
    }
    // The report above is the message; main.rs prints nothing for an empty
    // error and just sets the exit code.
    Err(String::new())
}

/// Force colour back on for a piped child. Maven and Spring Boot both turn
/// it off when stdout is not a terminal, and `run_watched` always pipes.
fn forced_color(cmd: &mut Command) {
    cmd.arg("-Dstyle.color=always")
        .arg("-Dspring-boot.run.jvmArguments=-Dspring.output.ansi.enabled=always");
}

/// Split `Class#method` into its two halves. Anything with no `#` is all
/// class.
fn split_method(filter: &str) -> (&str, Option<&str>) {
    match filter.split_once('#') {
        Some((class, method)) => (class, Some(method)),
        None => (filter, None),
    }
}

/// `Payout` -> `PayoutTest`, `Payout#settles` -> `PayoutTest#settles`.
///
/// The suffix belongs to the class alone. Appending it to the whole filter
/// produced `Payout#settlesTest`, a method nothing declares, and Surefire
/// then failed the build for a filter jails itself had corrupted.
fn expand_filter(filter: &str) -> String {
    let (class, method) = split_method(filter);
    let expanded = if class.ends_with("Test")
        || class.ends_with("Tests")
        || class.ends_with("IT")
        || class.contains('*')
    {
        class.to_string()
    } else {
        format!("{class}Test")
    };
    match method {
        Some(method) => format!("{expanded}#{method}"),
        None => expanded,
    }
}

pub fn test(filter: Option<&str>, debug: bool) -> Result<()> {
    let root = find_project_root()?;
    let mut cmd = Command::new(maven_binary(&root));
    if let Some(f) = filter {
        let test_name = expand_filter(f);
        // Decided on the *class*, not on the whole filter. `PayoutIT#settles`
        // ends in `settles`, so routing on the finished string sent an
        // integration test to Surefire, which does not run `*IT` -- Maven
        // reported success having executed nothing. Splitting first is what
        // makes both halves right.
        let (class, _) = split_method(&test_name);
        if class.ends_with("IT") {
            cmd.arg("verify").arg(format!("-Dit.test={test_name}"));
        } else {
            cmd.arg("test").arg(format!("-Dtest={test_name}"));
        }
        // Without this, a filter that matches nothing is a *build failure*
        // with a stack trace rather than "no tests ran" -- and jails' own
        // routing above can hand Surefire a filter that legitimately matches
        // nothing when the project holds both kinds. The payments team keeps
        // this as tribal knowledge; it belongs in the tool.
        cmd.arg("-Dsurefire.failIfNoSpecifiedTests=false");
        cmd.arg("-Dfailsafe.failIfNoSpecifiedTests=false");
    } else {
        cmd.arg("test");
    }
    cmd.current_dir(&root);
    run_inherited(cmd, debug)
}

pub fn build(debug: bool) -> Result<()> {
    let root = find_project_root()?;
    let mut cmd = Command::new(maven_binary(&root));
    cmd.arg("package").current_dir(&root);
    run_inherited(cmd, debug)
}

pub fn clean(debug: bool) -> Result<()> {
    let root = find_project_root()?;
    let mut cmd = Command::new(maven_binary(&root));
    cmd.arg("clean").current_dir(&root);
    run_inherited(cmd, debug)
}

/// Reformat in place. Spotless is a plugin, not a dependency, so an
/// unconfigured project fails with a Maven stack trace about an unknown
/// prefix -- checking first turns that into one actionable line.
pub fn fmt(debug: bool) -> Result<()> {
    let root = find_project_root()?;
    require_spotless(&root)?;
    let mut cmd = Command::new(maven_binary(&root));
    cmd.args(["spotless:apply"]).current_dir(&root);
    run_inherited(cmd, debug)
}

/// Reformat quietly, for `add format` to call the moment it installs the
/// plugin. A formatter has an opinion about line wrapping that no amount of
/// careful templating can predict, so the only way to leave the project
/// passing its own `verify` is to actually run it once.
///
/// Best-effort: a project without Maven on PATH is not a reason to fail the
/// capability, it just means the first `jails fmt` has work to do.
pub fn fmt_quietly(root: &std::path::Path) -> bool {
    Command::new(maven_binary(root))
        .args(["-q", "spotless:apply"])
        .current_dir(root)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Everything the build has to say: format check, compile, tests. `verify`
/// rather than `test` because that is the phase `add format` binds to.
/// `clean` first: Maven's incremental compile does not delete stale `.class`
/// files, so a removed test (or a renamed record) would still run from
/// `target/` and fail the check for a file that is no longer in the tree.
pub fn check(debug: bool) -> Result<()> {
    let root = find_project_root()?;
    let mut cmd = Command::new(maven_binary(&root));
    cmd.args(["clean", "verify"]).current_dir(&root);
    run_inherited(cmd, debug)
}

/// Escape hatch for Maven features jails should not duplicate. Arguments are
/// forwarded exactly; the project wrapper is still preferred.
pub fn mvn(args: &[String], debug: bool) -> Result<()> {
    let root = find_project_root()?;
    let mut cmd = Command::new(maven_binary(&root));
    cmd.args(args).current_dir(&root);
    run_inherited(cmd, debug)
}

fn require_spotless(root: &std::path::Path) -> Result<()> {
    let pom = fs::read_to_string(root.join("pom.xml"))
        .map_err(|e| format!("failed to read pom.xml: {e}"))?;
    if pom.contains("spotless-maven-plugin") {
        return Ok(());
    }
    Err("this project has no formatter configured -- run `jails add format` first".to_string())
}

/// Spawns `spring-boot:run` once and, on every change to a .java source
/// file, re-runs `mvn compile`. spring-boot-devtools (if on the
/// classpath) watches target/classes itself and restarts the already-
/// running JVM -- jails never kills/restarts the app process, just keeps
/// target/classes fresh. Without devtools this recompiles for nothing, so
/// that's checked upfront.
pub fn watch(debug: bool) -> Result<()> {
    let root = find_project_root()?;
    compose::up(&root, &[], debug);
    let pom = fs::read_to_string(root.join("pom.xml"))
        .map_err(|e| format!("failed to read pom.xml: {e}"))?;
    if !pom.contains("org.springframework.boot") {
        return Err("--watch only supports Spring Boot projects".to_string());
    }
    if !pom.contains("devtools") {
        eprintln!(
            "jails: spring-boot-devtools not found in pom.xml -- recompiles won't trigger a restart. Add it: jails new --deps web,devtools"
        );
    }

    let mut run_cmd = Command::new(maven_binary(&root));
    run_cmd.arg("spring-boot:run").current_dir(&root);
    // The same treatment `jails run` gets, and for the same reason:
    // `mvn spring-boot:run` exits 0 over an application that never started,
    // because devtools runs `main` on its own thread and catches the
    // exception there. Watching a dead application and reporting nothing is
    // the worst version of that bug, since the reader is *sitting there*
    // waiting for it to come up.
    forced_color(&mut run_cmd);
    let (finished, when_it_exits) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = finished.send(run_watched(run_cmd, debug));
    });

    let mut seen = fingerprint(&root);
    println!(
        "jails: watching {} for changes (Ctrl-C to stop)",
        root.display()
    );

    loop {
        std::thread::sleep(std::time::Duration::from_millis(750));

        match when_it_exits.try_recv() {
            // `run_watched` has already printed the log and, for a fatal
            // startup, the `why` explanation of it.
            Ok(result) => return result,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => return Ok(()),
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
        }

        let now = fingerprint(&root);
        let changes = changes_between(&seen, &now, &root);
        if changes.is_empty() {
            continue;
        }
        seen = now;
        for change in &changes {
            println!("jails: {change}");
        }
        println!("jails: recompiling...");
        let mut compile = Command::new(maven_binary(&root));
        compile.arg("compile").current_dir(&root);
        if debug {
            crate::debug_cmd(&compile);
        }
        match compile.status() {
            Ok(s) if s.success() => {
                println!("jails: recompiled -- devtools should restart shortly")
            }
            Ok(s) => eprintln!("jails: recompile failed ({s})"),
            Err(e) => eprintln!("jails: failed to run compile: {e}"),
        }
    }
}

/// What every watched file looked like at one moment: path -> mtime.
///
/// A map, not a high-water mark. The mtime *maximum* the watcher used before
/// could only answer "has anything got newer", which gets three cases wrong,
/// all of them ordinary: it cannot name the file that changed, a **deletion**
/// lowers nothing so it goes unnoticed, and `git checkout` of an older
/// revision moves mtimes backwards -- the exact moment a reader most wants a
/// restart. Comparing maps with `!=` catches all three.
///
/// The watched set is the whole project, not just `.java`: a template, a
/// migration, `application.properties`, `pom.xml`, `compose.yaml` and
/// `jails.toml` all change what a running application does, and a watcher
/// that ignores them makes the reader wonder why their change did nothing.
fn fingerprint(root: &Path) -> BTreeMap<PathBuf, std::time::SystemTime> {
    let mut found = BTreeMap::new();
    for dir in [
        "src/main/java",
        "src/main/resources",
        "src/test/java",
        "src/test/resources",
    ] {
        collect_mtimes(&root.join(dir), &mut found);
    }
    for file in ["pom.xml", "compose.yaml", "jails.toml"] {
        let path = root.join(file);
        if let Ok(modified) = fs::metadata(&path).and_then(|m| m.modified()) {
            found.insert(path, modified);
        }
    }
    found
}

fn collect_mtimes(dir: &Path, out: &mut BTreeMap<PathBuf, std::time::SystemTime>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // Build output is a *consequence* of a change, not one.
            if path.file_name().is_some_and(|n| n == "target") {
                continue;
            }
            collect_mtimes(&path, out);
        } else if let Ok(modified) = fs::metadata(&path).and_then(|m| m.modified()) {
            out.insert(path, modified);
        }
    }
}

/// What moved between two fingerprints, as lines a reader can act on.
fn changes_between(
    before: &BTreeMap<PathBuf, std::time::SystemTime>,
    after: &BTreeMap<PathBuf, std::time::SystemTime>,
    root: &Path,
) -> Vec<String> {
    let relative = |path: &Path| {
        path.strip_prefix(root)
            .unwrap_or(path)
            .display()
            .to_string()
    };
    let mut changes = Vec::new();
    for (path, when) in after {
        match before.get(path) {
            None => changes.push(format!("added   {}", relative(path))),
            // `!=`, not `>`: `git checkout` of an older revision moves an
            // mtime backwards, and that is still a change.
            Some(previous) if previous != when => {
                changes.push(format!("changed {}", relative(path)))
            }
            Some(_) => {}
        }
    }
    for path in before.keys() {
        if !after.contains_key(path) {
            changes.push(format!("deleted {}", relative(path)));
        }
    }
    changes
}

/// `args` is everything after `--`, forwarded verbatim to the program. A tool
/// that scaffolds CLI projects has to be able to *run* one with arguments, or
/// the edit loop drops out to raw `mvn` the moment the program takes input.
pub fn run(no_build: bool, args: &[String], debug: bool) -> Result<()> {
    let root = find_project_root()?;
    compose::up(&root, &[], debug);
    let pom = fs::read_to_string(root.join("pom.xml"))
        .map_err(|e| format!("failed to read pom.xml: {e}"))?;

    if pom.contains("org.springframework.boot") {
        if no_build {
            let jar = find_built_jar(&root)?;
            let mut run = Command::new("java");
            run.args(["-jar"]).arg(&jar).args(args).current_dir(&root);
            return run_inherited(run, debug);
        }
        let mut cmd = Command::new(maven_binary(&root));
        cmd.arg("spring-boot:run").current_dir(&root);
        // spring-boot:run forks a JVM, so argv cannot simply be appended: the
        // plugin takes them as one space-joined property instead.
        if !args.is_empty() {
            cmd.arg(format!("-Dspring-boot.run.arguments={}", args.join(" ")));
        }
        forced_color(&mut cmd);
        return run_watched(cmd, debug);
    }

    let (pkg, class_name) = find_main_class(&root)?;
    let fqcn = if pkg.is_empty() {
        class_name
    } else {
        format!("{pkg}.{class_name}")
    };

    if !no_build {
        let mut compile = Command::new(maven_binary(&root));
        compile.arg("compile").current_dir(&root);
        run_inherited(compile, debug)?;
    } else if !root
        .join("target/classes")
        .join(fqcn.replace('.', "/"))
        .with_extension("class")
        .is_file()
    {
        return Err(format!(
            "target/classes has no compiled {fqcn} -- run `jails build` or `jails run` (without --no-build) first"
        ));
    }

    let mut run = Command::new("java");
    run.args(["-cp", "target/classes", &fqcn])
        .args(args)
        .current_dir(&root);
    run_inherited(run, debug)
}

/// Picks a jar out of target/ for --no-build's Spring Boot path. Excludes
/// spring-boot-maven-plugin's *.jar.original (its extension() is
/// "original", not "jar", so a plain "jar" filter already skips it).
fn find_built_jar(root: &Path) -> Result<PathBuf> {
    let target = root.join("target");
    let entries = fs::read_dir(&target).map_err(|_| {
        "no target/ directory -- run `jails build` or `jails run` (without --no-build) first"
            .to_string()
    })?;
    entries
        .flatten()
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|ext| ext == "jar"))
        .ok_or_else(|| {
            "no jar under target/ -- run `jails build` or `jails run` (without --no-build) first"
                .to_string()
        })
}

/// Find the file with `static void main` under src/main/java and return
/// its (package, class name) so the caller can build the FQCN.
fn find_main_class(root: &Path) -> Result<(String, String)> {
    let src_root = root.join("src/main/java");
    let file = search_main_file(&src_root)
        .ok_or_else(|| "no file with `static void main` found under src/main/java".to_string())?;
    let contents =
        fs::read_to_string(&file).map_err(|e| format!("failed to read {}: {e}", file.display()))?;
    let pkg = contents
        .lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix("package ")?.trim().strip_suffix(';'))
        .unwrap_or("")
        .to_string();
    let class_name = file
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .ok_or_else(|| format!("could not determine class name for {}", file.display()))?;
    Ok((pkg, class_name))
}

/// Prefer a jails CLI dispatcher when the project has one.
///
/// `generate cli` adds a second `static void main` to a project that already
/// has `App.java`, and picking whichever the directory walk reached first
/// would make `jails run` a coin toss -- usually landing on the Hello World
/// stub that ignores argv entirely. The dispatcher is the one that routes
/// arguments, so it wins.
fn search_main_file(dir: &Path) -> Option<PathBuf> {
    dispatcher_main_file(dir).or_else(|| any_main_file(dir))
}

fn dispatcher_main_file(dir: &Path) -> Option<PathBuf> {
    let entries = fs::read_dir(dir).ok()?;
    let mut found = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(nested) = dispatcher_main_file(&path) {
                found.push(nested);
            }
        } else if path.extension().is_some_and(|ext| ext == "java") {
            let dispatches = fs::read_to_string(&path)
                .map(|s| s.contains("static void main") && crate::generate::is_dispatcher(&s))
                .unwrap_or(false);
            if dispatches {
                found.push(path);
            }
        }
    }
    // More than one is not a preference jails can express for the user.
    found.sort();
    (found.len() == 1).then(|| found.remove(0))
}

fn any_main_file(dir: &Path) -> Option<PathBuf> {
    let entries = fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = any_main_file(&path) {
                return Some(found);
            }
        } else if path.extension().is_some_and(|ext| ext == "java") {
            if let Ok(contents) = fs::read_to_string(&path) {
                if contents.contains("static void main") {
                    return Some(path);
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "jails-run-test-{label}-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_watcher_notices_every_kind_of_change_and_names_the_file() {
        let root = scratch("fingerprint");
        let java = root.join("src/main/java/com/example");
        let resources = root.join("src/main/resources");
        fs::create_dir_all(&java).unwrap();
        fs::create_dir_all(&resources).unwrap();
        fs::write(java.join("App.java"), "x").unwrap();
        fs::write(resources.join("application.properties"), "a=1").unwrap();
        fs::write(root.join("pom.xml"), "<project/>").unwrap();

        let before = fingerprint(&root);
        assert_eq!(before.len(), 3, "{before:?}");
        assert!(changes_between(&before, &before, &root).is_empty());

        // A resource is a change: it decides what the running application
        // does just as much as a class does.
        std::thread::sleep(std::time::Duration::from_millis(20));
        fs::write(resources.join("application.properties"), "a=2").unwrap();
        let changed = fingerprint(&root);
        assert_eq!(
            changes_between(&before, &changed, &root),
            vec!["changed src/main/resources/application.properties"]
        );

        // A new file, and a deleted one -- which the old high-water mark
        // could not see at all, since removing a file lowers nothing.
        fs::write(java.join("Extra.java"), "x").unwrap();
        fs::remove_file(java.join("App.java")).unwrap();
        let after = fingerprint(&root);
        let changes = changes_between(&changed, &after, &root);
        assert!(
            changes.contains(&"added   src/main/java/com/example/Extra.java".to_string()),
            "{changes:?}"
        );
        assert!(
            changes.contains(&"deleted src/main/java/com/example/App.java".to_string()),
            "{changes:?}"
        );
    }

    #[test]
    fn an_mtime_that_moves_backwards_is_still_a_change() {
        // `git checkout` of an older revision does exactly this, and it is
        // the moment a reader most wants a restart.
        let root = scratch("fingerprint-backwards");
        let java = root.join("src/main/java");
        fs::create_dir_all(&java).unwrap();
        fs::write(java.join("App.java"), "x").unwrap();

        let before = fingerprint(&root);
        let mut older = before.clone();
        let path = java.join("App.java");
        older.insert(
            path,
            before.values().next().unwrap().checked_sub(std::time::Duration::from_secs(60)).unwrap(),
        );
        assert_eq!(
            changes_between(&older, &before, &root),
            vec!["changed src/main/java/App.java"]
        );
        assert_eq!(
            changes_between(&before, &older, &root),
            vec!["changed src/main/java/App.java"],
            "a change is a change in either direction"
        );
    }

    #[test]
    fn build_output_is_not_a_change() {
        let root = scratch("fingerprint-target");
        let java = root.join("src/main/java");
        fs::create_dir_all(java.join("target")).unwrap();
        fs::write(java.join("App.java"), "x").unwrap();
        fs::write(java.join("target/App.class"), "compiled").unwrap();
        assert_eq!(fingerprint(&root).len(), 1);
    }

    #[test]
    fn find_main_class_extracts_package_and_class_name() {
        let root = scratch("main-class");
        let src = root.join("src/main/java/com/example/app");
        fs::create_dir_all(&src).unwrap();
        fs::write(
            src.join("Cli.java"),
            "package com.example.app;\n\npublic class Cli {\n    public static void main(String[] args) {}\n}\n",
        )
        .unwrap();

        let (pkg, class_name) = find_main_class(&root).unwrap();
        assert_eq!(pkg, "com.example.app");
        assert_eq!(class_name, "Cli");
    }

    #[test]
    fn find_main_class_ignores_files_without_a_main_method() {
        let root = scratch("no-main-class");
        let src = root.join("src/main/java/com/example/app");
        fs::create_dir_all(&src).unwrap();
        fs::write(
            src.join("Helper.java"),
            "package com.example.app;\n\nclass Helper {}\n",
        )
        .unwrap();

        assert!(find_main_class(&root).is_err());
    }

    #[test]
    fn find_main_class_handles_default_package() {
        let root = scratch("default-package");
        let src = root.join("src/main/java");
        fs::create_dir_all(&src).unwrap();
        fs::write(
            src.join("Cli.java"),
            "public class Cli {\n    public static void main(String[] args) {}\n}\n",
        )
        .unwrap();

        let (pkg, class_name) = find_main_class(&root).unwrap();
        assert_eq!(pkg, "");
        assert_eq!(class_name, "Cli");
    }
}

#[cfg(test)]
mod maven_resolution_tests {
    use super::*;

    /// mvnd ships as `mvnd.cmd` on Windows. `run.rs` probed for a bare
    /// `mvnd` while `project.rs` probed for `mvnd.cmd`, so on Windows the
    /// command `jails about` reported was not the one `jails test` would run
    /// -- and this side would have tried to execute a name not on disk.
    #[test]
    fn the_mvnd_binary_carries_its_platform_extension() {
        if cfg!(windows) {
            assert_eq!(mvnd_binary(), "mvnd.cmd");
        } else {
            assert_eq!(mvnd_binary(), "mvnd");
        }
    }

    /// The wrapper wins over anything on PATH, so a project's pinned Maven
    /// version is what runs.
    #[test]
    fn the_project_wrapper_is_preferred_over_path() {
        let dir = std::env::temp_dir().join(format!(
            "jails-maven-binary-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();

        // No wrapper: falls back to something on PATH, never to a wrapper path.
        assert!(!maven_binary(&dir).starts_with(&dir));

        let wrapper = dir.join(if cfg!(windows) { "mvnw.cmd" } else { "mvnw" });
        std::fs::write(&wrapper, "#!/bin/sh\n").unwrap();
        assert_eq!(maven_binary(&dir), wrapper);

        std::fs::remove_dir_all(&dir).ok();
    }

    /// `about` must report the command that will actually be executed.
    #[test]
    fn about_and_run_resolve_the_same_maven() {
        let root = std::env::temp_dir();
        assert_eq!(
            crate::project::maven_command_for_tests(&root),
            maven_binary(&root)
        );
    }
}
