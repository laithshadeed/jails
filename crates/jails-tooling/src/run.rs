use crate::compose;
use crate::generate::find_project_root;
use jails_support::Result;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// One PATH lookup, in `process`. `run.rs`, `compose.rs` and `project.rs`
/// each had their own copy, which is how the mvnd naming drifted between
/// them.
/// The project root, refusing early if Maven is not what builds it.
///
/// Every command in this module shells out to `mvn`, so the check and the
/// lookup are one step: eight call sites each doing `find_project_root` then
/// `require_maven_at` is eight chances to add a ninth that forgets.
fn maven_root(command: &str) -> Result<PathBuf> {
    let root = find_project_root()?;
    crate::build::require_maven_at(&root, command)?;
    Ok(root)
}

mod filter;
use filter::*;

pub fn find_on_path(bin: &str) -> bool {
    crate::process::on_path(bin)
}

/// Run a command with our stdio, failing on a non-zero exit.
///
/// A thin adapter over the one executor: the callers here build a
/// `std::process::Command` directly, and converting all of them at once would
/// be a large diff for no behaviour change. What matters is that the printing,
/// spawning and exit-status handling happen in one place -- the executor
/// prints *and then runs*, which is the property that was violated where each
/// site decided for itself.
pub fn run_inherited(mut cmd: Command, debug: bool) -> Result<()> {
    let is_maven = is_maven_program(cmd.get_program());
    if is_maven {
        forced_color(&mut cmd);
    }
    let mut spec = crate::process::CommandSpec::new(cmd.get_program())
        .args(cmd.get_args())
        .output(if is_maven {
            crate::process::OutputMode::Tee
        } else {
            crate::process::OutputMode::Inherit
        });
    if let Some(dir) = cmd.get_current_dir() {
        spec = spec.current_dir(dir);
    }
    for (key, value) in cmd.get_envs() {
        if let Some(value) = value {
            spec = spec.env(key, value);
        }
    }
    let done = crate::process::run(&spec, crate::process::Diagnostics::from_flag(debug))?;
    if done.status.success() {
        return Ok(());
    }
    if is_maven {
        let mut log = done.stdout_string();
        log.push_str(&String::from_utf8_lossy(&done.stderr));
        report_maven_failure(&log);
    }
    Err(format!(
        "{} exited with {}",
        cmd.get_program().to_string_lossy(),
        done.status
    ))
}

fn is_maven_program(program: &std::ffi::OsStr) -> bool {
    Path::new(program)
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            matches!(
                name.to_ascii_lowercase().as_str(),
                "mvn" | "mvnw" | "mvnd" | "mvn.cmd" | "mvnw.cmd" | "mvnd.cmd"
            )
        })
}

fn report_maven_failure(log: &str) {
    println!();
    println!("jails: Maven failed; diagnosing the captured failure:");
    println!();
    if crate::why::report(log) == 0 {
        println!("jails does not recognise this Maven failure yet.");
        println!("Run `jails doctor`, then inspect the final `Caused by:` line above.");
    }
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
        jails_support::debug_cmd(&cmd);
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
        report_maven_failure(&log);
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
/// it off when stdout is not a terminal. Both Maven's diagnostic tee and
/// `run_watched` pipe, so every Maven entry point must pass through here.
fn forced_color(cmd: &mut Command) {
    cmd.arg("-Dstyle.color=always")
        .arg("-Dspring-boot.run.jvmArguments=-Dspring.output.ansi.enabled=always");
}

/// What `jails test` was asked for beyond the filter.
#[derive(Default, Clone, Copy)]
pub struct TestOptions {
    pub failed: bool,
    pub fail_fast: bool,
    pub slowest: Option<usize>,
    /// Report the run as JSON instead of Maven's own output.
    pub json: bool,
    /// Skip Maven entirely and run the compiled classes (`plan.md` §10.2).
    pub fast: bool,
}

pub fn test(filter: Option<&str>, options: TestOptions, debug: bool) -> Result<()> {
    let root = maven_root("test")?;

    // `--failed` is a filter jails computes rather than one the reader types,
    // so it is resolved first and then follows exactly the same path.
    let from_reports;
    let filter = if options.failed {
        let failures = crate::surefire::failed_selectors(&root);
        if failures.is_empty() {
            println!("no failures recorded in target/surefire-reports or target/failsafe-reports.");
            println!("Nothing to rerun -- run `jails test` first, or drop --failed.");
            return Ok(());
        }
        println!(
            "rerunning {} failed test(s) from the last run",
            failures.len()
        );
        from_reports = failures.join(",");
        Some(from_reports.as_str())
    } else {
        filter
    };

    // The fast path, and every way out of it. `plan.md` §10.2's rule is that a
    // fast path falls back *loudly*: the failure this prevents is a green run
    // over classes that no longer match the source, which is worse than any
    // slowness.
    if options.fast {
        // One Maven run, the first time: the launcher class has to be on the
        // test classpath, and jails supplies what it needs to run rather than
        // handing the reader a `ClassNotFoundException` for a line they did
        // not write. Idempotent, so every later `--fast` skips it.
        ensure_console_launcher(&root, debug)?;
        match fast_path_refusal(&root, &options) {
            Some(reason) => {
                println!("--fast not taken: {reason}");
                println!("Running the full Maven path instead.");
            }
            None => {
                let resolved = filter
                    .map(|f| resolve_filter(&root, f))
                    .transpose()?
                    .and_then(|f| crate::launcher::fully_qualified(&root, &f));
                if filter.is_some() && resolved.is_none() {
                    println!(
                        "--fast not taken: could not resolve `{}` to a fully qualified name.",
                        filter.unwrap_or_default()
                    );
                    println!("Running the full Maven path instead.");
                } else {
                    return crate::launcher::run_fast(&root, resolved.as_deref(), debug);
                }
            }
        }
    }

    let mut cmd = Command::new(crate::maven::binary(&root));
    let mut rerun_hint: Option<String> = None;
    if let Some(f) = filter {
        let resolved = resolve_filter(&root, f)?;
        let test_name = expand_filter(&resolved);
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
        rerun_hint = Some(test_name);
    } else {
        cmd.arg("test");
    }
    if options.fail_fast {
        // One failing class is enough to stop: the point of the flag is the
        // *first* failure, and Surefire counts classes, not methods.
        cmd.arg("-Dsurefire.skipAfterFailureCount=1");
        cmd.arg("-Dfailsafe.skipAfterFailureCount=1");
    }
    cmd.current_dir(&root);

    if options.json {
        // Maven's own output would sit in front of the JSON and make it
        // unparseable, so it is captured and dropped. The report is read from
        // Surefire's XML afterwards -- the same source `--failed` and
        // `--slowest` already use, so the three cannot disagree about what ran.
        let captured = cmd
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()
            .map_err(|error| format!("failed to run Maven: {error}"))?;
        return report_json(&root, captured.status.success());
    }

    let outcome = run_inherited(cmd, debug);

    if let Some(count) = options.slowest {
        report_slowest(&root, count);
    }
    if outcome.is_err() {
        report_rerun_line(&root, rerun_hint.as_deref());
    }
    outcome
}

/// The finished run, as data.
///
/// `passed` is the build's own verdict rather than "no failed cases": a build
/// can fail before a single test runs -- a compile error, a missing dependency
/// -- and an empty failure list would then read as success. The `cases` array
/// says what actually executed, which is the other half a consumer needs to
/// tell "all green" from "nothing ran".
fn report_json(root: &Path, passed: bool) -> Result<()> {
    let cases = crate::surefire::cases(root);
    let rows: Vec<String> = cases
        .iter()
        .map(|case| {
            format!(
                "    {{\"class\": {}, \"method\": {}, \"seconds\": {:.3}, \"failed\": {}, \
                 \"selector\": {}}}",
                crate::json::string(&case.class),
                crate::json::string(&case.method),
                case.seconds,
                case.failed,
                crate::json::string(&case.selector())
            )
        })
        .collect();
    let failed = cases.iter().filter(|case| case.failed).count();
    println!(
        "{{\n  \"schema_version\": 1,\n  \"passed\": {passed},\n  \"total\": {},\n  \
         \"failed\": {failed},\n  \"cases\": [\n{}\n  ]\n}}",
        cases.len(),
        rows.join(",\n")
    );
    if passed { Ok(()) } else { Err(String::new()) }
}

/// After a failing run, the command that reruns just what broke.
///
/// Copied from Rails, which prints a runnable `bin/rails test path:LINE`
/// rather than making you assemble one. The plan credited
/// `--only-failures` to Rails; that is RSpec's, and **the copy-pasteable
/// line is the part worth having** (plan.md §7).
fn report_rerun_line(root: &Path, already_filtered: Option<&str>) {
    let failures = crate::surefire::failed_selectors(root);
    println!();
    match failures.len() {
        0 => {
            // Nothing in the reports: the build failed before any test ran,
            // or Maven itself did. Repeating the filter is still useful and
            // pretending to know which test failed is not.
            if let Some(filter) = already_filtered {
                println!("jails: rerun with  jails test '{filter}'");
            }
        }
        1 => println!("jails: rerun with  jails test '{}'", failures[0]),
        n => {
            println!("jails: {n} test(s) failed. Rerun just those with  jails test --failed");
            for selector in failures.iter().take(5) {
                println!("         {selector}");
            }
            if n > 5 {
                println!("         ... and {} more", n - 5);
            }
        }
    }
}

/// The slowest tests of the run that just finished.
///
/// Read from the reports rather than timed here: Maven already measured
/// each one, and a wall-clock number jails invented would include its own
/// startup.
fn report_slowest(root: &Path, count: usize) {
    let slowest = crate::surefire::slowest(root, count);
    if slowest.is_empty() {
        println!();
        println!("jails: no test reports to read -- nothing ran, or the build failed first.");
        return;
    }
    println!();
    println!("slowest {} test(s):", slowest.len());
    for case in slowest {
        println!("  {:>8.2}s  {}", case.seconds, case.selector());
    }
}

/// Turn whatever the reader typed into something Surefire understands.
///
/// Only one shape needs work: `path/to/FooTest.java:42`, which JUnit cannot
/// resolve itself -- Jupiter has no `FileSelector` -- so an editor
/// keybinding has nothing to send unless jails does it.
/// Put JUnit's console launcher on the test classpath, once.
fn ensure_console_launcher(root: &Path, debug: bool) -> Result<()> {
    let pom = crate::pom::read(root)?;
    if pom.contains("junit-platform-console") {
        return Ok(());
    }
    let version = match crate::junit::console_version(&pom) {
        crate::junit::ConsoleVersion::Managed => None,
        // Leaked deliberately: `pom::Dependency` is a compile-time constant
        // everywhere else, this one string is derived once per process, and it
        // has to outlive the splice. Threading a lifetime through forty const
        // declarations to avoid one CLI-lifetime allocation is the worse trade.
        crate::junit::ConsoleVersion::Pinned(version) => Some(&*version.leak()),
        crate::junit::ConsoleVersion::Unknown => {
            return Err(
                "this project declares no JUnit version, so jails cannot align the console \
                 launcher with it.\n       \
                 A mismatched launcher resolves fine and then dies with NoSuchMethodError.\n       \
                 fix: declare org.junit.jupiter:junit-jupiter (or import junit-bom), then \
                 retry --fast."
                    .to_string(),
            );
        }
    };
    let dependency = crate::pom::Dependency {
        group_id: "org.junit.platform",
        artifact_id: "junit-platform-console",
        version,
        scope: Some("test"),
        optional: false,
    };
    let Some(updated) = crate::pom::add_dependency(&pom, &dependency)? else {
        return Ok(());
    };
    crate::apply::put_named(root.join("pom.xml"), updated, "pom.xml")?;
    println!(
        "added {}:{} (test scope) -- `--fast` runs JUnit's console launcher directly",
        dependency.group_id, dependency.artifact_id
    );
    let _ = debug;
    Ok(())
}

/// Why `--fast` cannot be used for this run, if it cannot.
///
/// `--json` and `--slowest` read Surefire's XML, which the console launcher
/// does not write. Producing an empty report would be worse than declining:
/// the three ways jails reports a run all read one source, and that is what
/// stops them disagreeing about what ran.
fn fast_path_refusal(root: &Path, options: &TestOptions) -> Option<String> {
    if options.json {
        return Some(
            "--json reads Surefire's XML, which the console launcher does not write".into(),
        );
    }
    if options.slowest.is_some() {
        return Some(
            "--slowest reads Surefire's XML, which the console launcher does not write".into(),
        );
    }
    if options.fail_fast {
        return Some("--fail-fast is a Surefire setting".into());
    }
    crate::launcher::staleness(root).map(|stale| stale.explain())
}

pub fn build(debug: bool) -> Result<()> {
    let root = maven_root("build")?;
    let mut cmd = Command::new(crate::maven::binary(&root));
    cmd.arg("package").current_dir(&root);
    run_inherited(cmd, debug)
}

pub fn clean(debug: bool) -> Result<()> {
    let root = maven_root("clean")?;
    let mut cmd = Command::new(crate::maven::binary(&root));
    cmd.arg("clean").current_dir(&root);
    run_inherited(cmd, debug)
}

/// Reformat in place. Spotless is a plugin, not a dependency, so an
/// unconfigured project fails with a Maven stack trace about an unknown
/// prefix -- checking first turns that into one actionable line.
pub fn fmt(debug: bool) -> Result<()> {
    let root = maven_root("fmt")?;
    require_spotless(&root)?;
    let mut cmd = Command::new(crate::maven::binary(&root));
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
/// Everything the build has to say: format check, compile, tests. `verify`
/// rather than `test` because that is the phase `add format` binds to.
/// `clean` first: Maven's incremental compile does not delete stale `.class`
/// files, so a removed test (or a renamed record) would still run from
/// `target/` and fail the check for a file that is no longer in the tree.
pub fn check(debug: bool) -> Result<()> {
    let root = maven_root("check")?;
    let mut cmd = Command::new(crate::maven::binary(&root));
    cmd.args(["clean", "verify"]).current_dir(&root);
    run_inherited(cmd, debug)
}

/// Escape hatch for Maven features jails should not duplicate. Arguments are
/// forwarded exactly; the project wrapper is still preferred.
pub fn mvn(args: &[String], debug: bool) -> Result<()> {
    let root = maven_root("mvn")?;
    let mut cmd = Command::new(crate::maven::binary(&root));
    cmd.args(args).current_dir(&root);
    run_inherited(cmd, debug)
}

fn require_spotless(root: &Path) -> Result<()> {
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
    let root = maven_root("watch")?;
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

    let mut run_cmd = Command::new(crate::maven::binary(&root));
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
        let mut compile = Command::new(crate::maven::binary(&root));
        compile.arg("compile").current_dir(&root);
        if debug {
            jails_support::debug_cmd(&compile);
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
    let root = maven_root("run")?;
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
        let mut cmd = Command::new(crate::maven::binary(&root));
        cmd.arg("spring-boot:run").current_dir(&root);
        // spring-boot:run forks a JVM, so argv cannot simply be appended: the
        // plugin takes them as one space-joined property instead.
        if !args.is_empty() {
            cmd.arg(format!("-Dspring-boot.run.arguments={}", args.join(" ")));
        }
        forced_color(&mut cmd);
        return run_watched(cmd, debug);
    }

    // The POM's `<mainClass>` first, because that is the entry point the
    // packaged jar has, and `jails run` claiming to run the application while
    // starting a *different* class is a defect rather than a convenience: a
    // project whose manifest generated `LedgerCli` had `reconcile` registered
    // there, and both `java -jar` and `jails run` started the `App` stub,
    // which answers only `help`. Searching source is the fallback for a POM
    // that declares none -- it cannot be the primary, because a project with
    // two dispatchers has two `main` methods and a walk picks whichever it
    // reaches first.
    let fqcn = match crate::pom::main_class(&pom) {
        Some(declared) => declared.to_string(),
        None => {
            let (pkg, class_name) = find_main_class(&root)?;
            if pkg.is_empty() {
                class_name
            } else {
                format!("{pkg}.{class_name}")
            }
        }
    };

    if !no_build {
        let mut compile = Command::new(crate::maven::binary(&root));
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
        } else if path.extension().is_some_and(|ext| ext == "java")
            && let Ok(contents) = fs::read_to_string(&path)
            && contents.contains("static void main")
        {
            return Some(path);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(label: &str) -> PathBuf {
        jails_support::scratch::ScratchDir::in_temp(&format!("jails-run-test-{label}"))
            .unwrap()
            .keep()
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
            before
                .values()
                .next()
                .unwrap()
                .checked_sub(std::time::Duration::from_secs(60))
                .unwrap(),
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
mod filter_tests {
    use super::*;

    const SOURCE: &str = r#"package com.example.demo;

import org.junit.jupiter.api.Test;

class ResultTest {

    /** Javadoc mentioning @Test, which is not an annotation. */
    void helper() {}

    @Test
    void completesAnItem() {
        assertThat(item.complete()).isTrue();
    }

    @org.junit.jupiter.api.Test
    void writtenFullyQualified() {
        assertThat(item.cost()).isZero();
    }

    @Nested
    class WhenDeclined {

        @Test
        void keepsTheOriginalAmount() {
        assertThat(item.amount()).isEqualTo(1L);
        }
    }
}
"#;

    fn line_of(needle: &str) -> usize {
        SOURCE
            .lines()
            .position(|l| l.contains(needle))
            .expect("needle is in the fixture")
            + 1
    }

    #[test]
    fn a_line_inside_a_test_resolves_to_that_test() {
        assert_eq!(
            enclosing_test_method(SOURCE, line_of("item.complete()")),
            Some("completesAnItem".to_string())
        );
        // The declaration line itself counts as inside it.
        assert_eq!(
            enclosing_test_method(SOURCE, line_of("void completesAnItem")),
            Some("completesAnItem".to_string())
        );
    }

    #[test]
    fn a_fully_qualified_test_annotation_counts() {
        // Jails' own generated ITs carry fully qualified annotations, and
        // matching `@Test` as a prefix missed every one of them.
        assert_eq!(
            enclosing_test_method(SOURCE, line_of("item.cost()")),
            Some("writtenFullyQualified".to_string())
        );
    }

    #[test]
    fn a_method_with_no_test_annotation_is_not_a_test() {
        // `helper` is preceded by Javadoc containing the word @Test, which
        // is exactly what `crate::java::blanked` exists to stop being read as one.
        assert_eq!(enclosing_test_method(SOURCE, line_of("void helper")), None);
    }

    #[test]
    fn a_line_above_every_test_resolves_to_nothing_rather_than_guessing() {
        assert_eq!(enclosing_test_method(SOURCE, 1), None);
    }

    #[test]
    fn a_nested_class_is_addressed_the_way_junit_addresses_it() {
        let path = Path::new("src/test/java/com/example/demo/PayoutTest.java");
        assert_eq!(enclosing_class(SOURCE, path), "PayoutTest$WhenDeclined");
        // A file with no nested type is just its stem.
        let flat = "package com.example.demo;\n\nclass PayoutTest {\n}\n";
        assert_eq!(enclosing_class(flat, path), "PayoutTest");
    }

    #[test]
    fn only_a_path_ending_in_java_with_a_line_is_a_file_selector() {
        assert!(split_file_line("PayoutTest.java:42").is_some());
        assert!(split_file_line("Payout#settles").is_none());
        assert!(split_file_line("PayoutTest").is_none());
        // A class name with a colon but no line number is not one either.
        assert!(split_file_line("PayoutTest.java:nope").is_none());
    }

    #[test]
    fn the_class_suffix_is_applied_to_the_class_half_only() {
        assert_eq!(expand_filter("Payout"), "PayoutTest");
        assert_eq!(expand_filter("Payout#settles"), "PayoutTest#settles");
        assert_eq!(expand_filter("PayoutTest#settles"), "PayoutTest#settles");
        assert_eq!(expand_filter("PayoutIT#settles"), "PayoutIT#settles");
        // A nested selector already carries its own suffix.
        assert_eq!(
            expand_filter("PayoutTest$WhenDeclined#keeps"),
            "PayoutTest$WhenDeclined#keeps"
        );
    }
}
