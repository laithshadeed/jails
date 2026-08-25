use crate::compose;
use crate::generate::find_project_root;
use jails_support::Result;
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
    if matches!(crate::build::detect(&root), crate::build::Build::Gradle) {
        return Err(format!(
            "`jails {command}` drives Maven, and this project is built by Gradle.\n       \
             jails reads and splices `build.gradle` -- `add`, `generate`, `doctor` and the \
             rest work here -- but it does not drive this command's Gradle equivalent \
             yet.\n       fix: run it through the wrapper, `./gradlew <task>`."
        )
        .into());
    }
    Ok(root)
}

/// The project root for a command that can drive either build.
///
/// Separate from [`maven_root`] on purpose. A command that shells out to `mvn`
/// and one that knows both tools are different commands, and collapsing them
/// is how `jails test` came to run Maven against a Gradle project and fail
/// with a POM error -- which is worse than the refusal it replaced, because a
/// refusal says what to do instead.
fn either_root(command: &str) -> Result<(PathBuf, crate::build::Build)> {
    let root = find_project_root()?;
    crate::build::require_maven_at(&root, command)?;
    let build = crate::build::detect(&root);
    Ok((root, build))
}

mod filter;
mod fingerprint;
mod gradlew;
mod isolation;
mod test_execution;
mod test_plan;
mod watch;
use filter::*;

/// Run a command with our stdio, failing on a non-zero exit.
///
/// A thin adapter over the one executor: the callers here build a
/// `std::process::Command` directly, and converting all of them at once would
/// be a large diff for no behaviour change. What matters is that the printing,
/// spawning and exit-status handling happen in one place -- the executor
/// prints *and then runs*, which is the property that was violated where each
/// site decided for itself.
pub(crate) fn run_inherited(mut cmd: Command, debug: bool) -> Result<()> {
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
    )
    .into())
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
        return Err(format!("{program} exited with {status}").into());
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
    Err(jails_support::Failure::Reported)
}

/// Force colour back on for a piped child. Maven and Spring Boot both turn
/// it off when stdout is not a terminal. Both Maven's diagnostic tee and
/// `run_watched` pipe, so every Maven entry point must pass through here.
fn forced_color(cmd: &mut Command) {
    cmd.arg("-Dstyle.color=always")
        .arg("-Dspring-boot.run.jvmArguments=-Dspring.output.ansi.enabled=always");
}

/// What `jails test` was asked for beyond the filter.
#[derive(Clone, Debug)]
pub struct TestOptions {
    pub scope: jails_protocol::testing::TestScope,
    pub compile: jails_protocol::testing::TestCompilePolicy,
    pub engine: jails_protocol::testing::TestEnginePolicy,
    pub watch: bool,
    pub affected: bool,
    pub failed: bool,
    pub tags: Vec<String>,
    pub fail_fast: bool,
    pub slowest: Option<usize>,
    /// Report the run as JSON instead of Maven's own output.
    pub json: bool,
    /// One-release compatibility spelling for `--engine auto`.
    pub fast: bool,
    pub until_fail: bool,
    pub repeat: usize,
    pub timeout: Option<String>,
    pub database_schema: bool,
    pub explain_selection: bool,
}

pub fn validate_test_options(options: &TestOptions) -> Result<()> {
    test_plan::validate_runtime_options(options)
}

pub fn test(requested: &[String], options: TestOptions, debug: bool) -> Result<()> {
    validate_test_options(&options)?;
    if options.watch {
        return test_execution::test_watch(requested, options, debug);
    }
    if options.until_fail {
        let mut once = options;
        once.until_fail = false;
        loop {
            test_once(requested, once.clone(), debug)?;
        }
    }
    let mut once = options;
    let repeat = once.repeat;
    once.repeat = 1;
    for iteration in 0..repeat {
        if repeat > 1 {
            println!("test run {}/{}", iteration + 1, repeat);
        }
        test_once(requested, once.clone(), debug)?;
    }
    Ok(())
}

fn test_once(requested: &[String], options: TestOptions, debug: bool) -> Result<()> {
    test_once_with_fallback(requested, options, debug, None)
}

fn test_once_with_fallback(
    requested: &[String],
    mut options: TestOptions,
    debug: bool,
    fallback_reason: Option<String>,
) -> Result<()> {
    let (root, build) = either_root("test")?;
    let mut execution_requested = requested.to_vec();
    if options.failed {
        let failures = match build {
            crate::build::Build::Gradle => crate::reports::failed_patterns(&root),
            _ => crate::reports::failed_selectors(&root),
        };
        if failures.is_empty() && execution_requested.is_empty() {
            println!(
                "no failures recorded. Reports are read from target/surefire-reports, \
                 target/failsafe-reports and build/test-results/."
            );
            println!("Nothing to rerun -- run `jails test` first, or drop --failed.");
            return Ok(());
        }
        if !failures.is_empty() {
            if execution_requested.is_empty() {
                println!(
                    "rerunning {} failed test(s) from the last run",
                    failures.len()
                );
            } else {
                println!(
                    "adding {} failed test(s) from the last run to the requested selection",
                    failures.len()
                );
            }
            execution_requested.extend(failures);
            execution_requested.sort();
            execution_requested.dedup();
        }
        options.failed = false;
    }
    let requested = execution_requested.as_slice();
    let compiled_outputs_current =
        build == crate::build::Build::Maven && crate::launcher::staleness(&root).is_none();
    let plan = test_plan::plan(build, requested, &options, compiled_outputs_current)?;
    if options.explain_selection || options.fast {
        test_plan::explain(&plan);
    }
    if plan
        .partitions
        .iter()
        .any(|partition| partition.engine == jails_protocol::testing::TestEngine::TestdV2)
    {
        return test_execution::run_warm(requested, &options, debug);
    }
    if options.affected {
        if options.engine == jails_protocol::testing::TestEnginePolicy::Build {
            println!(
                "test selection widened to the full {:?} scope: the build engine has no safe \
                 affected-test graph",
                options.scope
            );
        } else {
            return test_execution::run_warm(requested, &options, debug);
        }
    }
    if build == crate::build::Build::Gradle {
        return gradlew::test(&root, requested, options, fallback_reason, debug);
    }
    let root = maven_root("test")?;

    let filter = match requested {
        [] => None,
        [one] => Some(one.as_str()),
        _ => {
            let context = test_execution::MavenTestContext {
                project: &root,
                options: &options,
                fallback_reason: fallback_reason.as_deref(),
                debug,
            };
            return test_execution::test_many_maven(&context, requested);
        }
    };

    // `--failed` is a filter jails computes rather than one the reader types,
    // so it is resolved first and then follows exactly the same path.
    let from_reports;
    let filter = if options.failed {
        let failures = crate::reports::failed_selectors(&root);
        if failures.is_empty() {
            println!(
                "no failures recorded. Reports are read from target/surefire-reports, \
                 target/failsafe-reports and build/test-results/."
            );
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
        // The console launcher is already on the test classpath: `main.rs`
        // installs it as an owned entity before calling this, so a reader can
        // take it back out with `jails remove fast-test`. It used to be
        // spliced from here, which recorded nothing and left no way to undo
        // it -- `pending.md` §7.7's one real project write outside a
        // transaction.
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
        match options.scope {
            jails_protocol::testing::TestScope::Unit => {
                cmd.arg("test");
            }
            jails_protocol::testing::TestScope::Integration => {
                cmd.arg("verify").arg("-Dsurefire.skip=true");
            }
            jails_protocol::testing::TestScope::All => {
                cmd.arg("verify");
            }
        }
    }
    if !options.tags.is_empty() {
        cmd.arg(format!("-Dgroups={}", options.tags.join(",")));
    }
    if options.fail_fast {
        // One failing class is enough to stop: the point of the flag is the
        // *first* failure, and Surefire counts classes, not methods.
        cmd.arg("-Dsurefire.skipAfterFailureCount=1");
        cmd.arg("-Dfailsafe.skipAfterFailureCount=1");
    }
    cmd.current_dir(&root);

    if options.json {
        if options.timeout.is_some() {
            return Err(
                "`--timeout --json` needs the v2 captured-result executor\n       fix: omit one \
                 option until that executor is active"
                    .into(),
            );
        }
        // Maven's own output would sit in front of the JSON and make it
        // unparseable, so it is captured and dropped. The report is read from
        // Surefire's XML afterwards -- the same source `--failed` and
        // `--slowest` already use, so the three cannot disagree about what ran.
        let captured = cmd
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()
            .map_err(|error| format!("failed to run Maven: {error}"))?;
        let report = crate::reports::normalized(
            &root,
            jails_protocol::testing::TestEngine::Maven,
            options.scope,
            requested,
            captured.status.success(),
            fallback_reason,
        )?;
        return crate::reports::render(&report, true, options.slowest);
    }

    let outcome = match options.timeout.as_deref() {
        Some(timeout) => test_execution::run_inherited_timeout(
            cmd,
            debug,
            std::time::Duration::from_secs(test_plan::parse_duration(timeout)?),
        ),
        None => run_inherited(cmd, debug),
    };

    if outcome.is_err() {
        crate::reports::rerun_line(&root, rerun_hint.as_deref());
    }
    let report = crate::reports::normalized(
        &root,
        jails_protocol::testing::TestEngine::Maven,
        options.scope,
        requested,
        outcome.is_ok(),
        fallback_reason,
    )?;
    crate::reports::render(&report, false, options.slowest)
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
    let (root, build) = either_root("build")?;
    if build == crate::build::Build::Gradle {
        // `assemble` rather than `build`: Gradle's `build` runs the tests too,
        // and `jails build` is the one that does not. `jails check` is where
        // tests belong, and a command that quietly did more than its name says
        // is how a slow feedback loop gets blamed on the wrong thing.
        return gradlew::tasks(&root, &["assemble"], debug);
    }
    let mut cmd = Command::new(crate::maven::binary(&root));
    cmd.arg("package").current_dir(&root);
    run_inherited(cmd, debug)
}

pub fn clean(debug: bool) -> Result<()> {
    let (root, build) = either_root("clean")?;
    if build == crate::build::Build::Gradle {
        return gradlew::tasks(&root, &["clean"], debug);
    }
    let mut cmd = Command::new(crate::maven::binary(&root));
    cmd.arg("clean").current_dir(&root);
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
    let (root, build) = either_root("check")?;
    if build == crate::build::Build::Gradle {
        // `clean` first for the same reason Maven gets it: an incremental
        // compile does not delete stale classes, so a removed test still runs
        // from the output directory and fails the check for a file that is no
        // longer in the tree.
        return gradlew::tasks(&root, &["clean", "check"], debug);
    }
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

/// The same escape hatch for a Gradle build.
///
/// A sibling rather than a branch inside `mvn`, because the two are different
/// commands: the arguments are a *Maven* command line in one and a *Gradle*
/// one in the other, and a single name taking either would make a muscle-memory
/// `jails mvn -DskipTests` silently mean something else on the wrong project.
pub fn gradle(args: &[String], debug: bool) -> Result<()> {
    let root = find_project_root()?;
    if crate::build::detect(&root) != crate::build::Build::Gradle {
        return Err(jails_support::Failure::Told(
            "`jails gradle` needs a Gradle project.\n       fix: this one is not built by \
             Gradle -- `jails mvn` is the escape hatch for a Maven build."
                .to_string(),
        ));
    }
    let borrowed: Vec<&str> = args.iter().map(String::as_str).collect();
    gradlew::tasks(&root, &borrowed, debug)
}

/// Spawns `spring-boot:run` once and, on every change to a .java source
/// file, re-runs `mvn compile`. spring-boot-devtools (if on the
/// classpath) watches target/classes itself and restarts the already-
/// running JVM -- jails never kills/restarts the app process, just keeps
/// target/classes fresh. Without devtools this recompiles for nothing, so
/// that's checked upfront.
pub fn watch(debug: bool) -> Result<()> {
    let (root, build) = either_root("watch")?;
    if build == crate::build::Build::Gradle {
        compose::up(&root, &[], debug);
        // Gradle's own continuous mode rather than devtools: `--continuous`
        // re-runs the task when an input changes, which is the same loop from
        // the reader's side and needs nothing added to the build. devtools is
        // still honoured if the project has it -- the two compose, because one
        // rebuilds and the other restarts.
        return gradlew::tasks(&root, &["--continuous", "bootRun"], debug);
    }
    let root = maven_root("watch")?;
    compose::up(&root, &[], debug);
    let pom = fs::read_to_string(root.join("pom.xml"))
        .map_err(|e| format!("failed to read pom.xml: {e}"))?;
    if !pom.contains("org.springframework.boot") {
        return Err(jails_support::Failure::Told(
            "--watch only supports Spring Boot projects".to_string(),
        ));
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

    let mut seen = fingerprint::fingerprint(&root);
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

        let now = fingerprint::fingerprint(&root);
        let changes = fingerprint::changes_between(&seen, &now, &root);
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

/// `args` is everything after `--`, forwarded verbatim to the program. A tool
/// that scaffolds CLI projects has to be able to *run* one with arguments, or
/// the edit loop drops out to raw `mvn` the moment the program takes input.
pub fn run(no_build: bool, args: &[String], debug: bool) -> Result<()> {
    let (root, build) = either_root("run")?;
    if build == crate::build::Build::Gradle {
        compose::up(&root, &[], debug);
        let mut tasks = vec!["bootRun".to_string()];
        if !args.is_empty() {
            // The Boot Gradle plugin forks a JVM, so argv reaches the
            // application through a property rather than as arguments to
            // Gradle -- the same shape `spring-boot:run` needs and for the
            // same reason.
            tasks.push(format!("--args={}", args.join(" ")));
        }
        let borrowed: Vec<&str> = tasks.iter().map(String::as_str).collect();
        return gradlew::tasks(&root, &borrowed, debug);
    }
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
        ).into());
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
    Ok(entries
        .flatten()
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|ext| ext == "jar"))
        .ok_or_else(|| {
            "no jar under target/ -- run `jails build` or `jails run` (without --no-build) first"
                .to_string()
        })?)
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
