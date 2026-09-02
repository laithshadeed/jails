#![allow(dead_code)]

pub mod parallel;
pub mod scenarios;

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs;
use std::fs::File;
use std::io;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Output, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

pub fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_jails")
}

/// A fresh, isolated scratch directory under the OS temp dir -- real
/// filesystem, but never the actual project checkout, so tests can't step
/// on each other or on this repo.
///
/// Nothing removes these at the end of a test, deliberately: when a
/// real-toolchain test fails, the generated project is the evidence, and a
/// `Drop` guard would delete it before anyone could look. The cost is that
/// they accumulate -- a full sweep leaves a Maven project per cell -- and
/// enough of them fill `/tmp`, at which point Maven fails mid-gate with
/// `Disk quota exceeded`. The sweep below is the answer: **keep the evidence
/// from this run, take out the ones nobody is looking at any more.**
///
/// A scratch tree no other test can be handed: `tempfile` creates the
/// directory atomically with OS randomness in the name, so exclusivity is the
/// filesystem's guarantee rather than a hope about clock resolution -- a pid
/// plus a nanosecond timestamp created with `create_dir_all` is exclusive in
/// neither half. The guard is leaked on purpose: these fixtures outlive the test
/// so a failure can be inspected, and `sweep_stale_fixtures` is what collects
/// them an hour later.
pub fn temp_dir(label: &str) -> PathBuf {
    sweep_stale_fixtures();
    match tempfile::Builder::new()
        .prefix(&format!("jails-e2e-{label}-"))
        .tempdir()
    {
        Ok(fixture) => fixture.keep(),
        Err(error) => panic!("{}", scratch_failure(label, &error)),
    }
}

/// What to say when a fixture directory cannot be created.
///
/// `.expect("failed to create a scratch directory")` is a sentence about
/// jails, and a full `/tmp` produces hundreds of them in tests that are
/// working perfectly; a harness that names the wrong subject costs an
/// afternoon before anyone runs `df`.
///
/// So the disk is named when the disk is the cause, and only then: a
/// permission error or a missing `TMPDIR` is a different failure and claiming
/// it is a full disk is the same defect facing the other way. The count is
/// the actionable half -- these fixtures are kept deliberately so a failed
/// test can be inspected, so the sweep's budget is what is holding them, not
/// a leak.
fn scratch_failure(label: &str, error: &io::Error) -> String {
    let temp = std::env::temp_dir();
    let mut message = format!(
        "could not create the `{label}` test fixture under {}: {error}",
        temp.display()
    );
    if matches!(
        error.kind(),
        io::ErrorKind::StorageFull | io::ErrorKind::QuotaExceeded
    ) {
        let fixtures = fs::read_dir(&temp)
            .map(|entries| {
                entries
                    .flatten()
                    .filter(|entry| entry.file_name().to_string_lossy().starts_with("jails-"))
                    .count()
            })
            .unwrap_or_default();
        message.push_str(&format!(
            "\n  This is the machine and not the tree: {} has no room left. \
             {fixtures} jails fixtures are in it, kept on purpose so a failed test \
             can be inspected, and swept once they are past {} minutes or over a \
             budget of {FIXTURE_BUDGET}.\n  fix: rm -rf {}/jails-*",
            temp.display(),
            FIXTURE_LIFETIME.as_secs() / 60,
            temp.display(),
        ));
    }
    message
}

/// Whether the suite should print what every subprocess cost.
///
/// **Empty means off**, which is not pedantry: CI passes this through an
/// expression that yields `''` when the dispatch input is false, and a bare
/// `is_some()` reads a set-but-empty variable as "on" -- so every ordinary run
/// would print thousands of profile lines nobody asked for.
pub fn profiling() -> bool {
    std::env::var_os("JAILS_TEST_PROFILE").is_some_and(|value| !value.is_empty())
}

/// A persistent generated-project directory keyed to this integration-test
/// executable. Maven may reuse unchanged javac output across `cargo test`
/// invocations, while every Java test is still executed on every invocation.
/// Any product or harness change rebuilds this executable and invalidates the
/// generated tree before it can be trusted.
pub fn cached_toolchain_dir(label: &str) -> (PathBuf, bool) {
    cached_toolchain_dir_with_salt(label, "")
}

/// This process's claim on every persistent fixture it is using.
///
/// **The claim is `flock`, and it is held until the process exits.** Both
/// halves are the fix rather than the implementation: a claim taken and
/// dropped at the end of the decision would leave the tree unlocked before
/// the first test read a byte of it, because every caller parks the directory
/// in a `OnceLock` and uses it for the lifetime of the binary. And `flock`
/// rather than a marker file, because the kernel releases it however the
/// holder dies -- a gate killed mid-Maven leaves no claim anybody has to
/// clear by hand.
static FIXTURE_CLAIMS: Mutex<Vec<File>> = Mutex::new(Vec::new());

/// Where this process may build `label`, given who else is using it.
///
/// The shared fixture is the point -- it is what lets Maven reuse javac
/// output across `cargo test` invocations -- so it is what a run asks for
/// first. What it must not do is *share* it: two gate runs reading the same
/// stamp, with whichever decides to rebuild calling `remove_dir_all` on a
/// tree the other is running Maven inside, fail as `capabilities::`
/// assertions that read exactly like capability regressions.
///
/// A second run gets a private copy instead of waiting. Waiting would be
/// correct too, but the wait is the whole `cli` binary -- minutes -- and a
/// gate that has stopped for that long is indistinguishable from one that
/// has hung. A cold build is a cost the second run can see and the first run
/// does not pay.
fn claim_fixture(cache: &Path, label: &str) -> PathBuf {
    // Reported through the claim rather than swallowed: a cache directory
    // that does not exist refuses every claim, which reads as contention.
    let _ = fs::create_dir_all(cache);
    if claim(cache, label) {
        return cache.join(label);
    }
    let private = format!("{label}.pid{}", std::process::id());
    eprintln!(
        "[jails-tests] another run holds the `{label}` fixture, so this one builds \
         `{private}` from cold. Concurrent gate runs are safe; they are not free."
    );
    claim(cache, &private);
    cache.join(private)
}

/// Take this process's claim on one fixture, or report that somebody holds it.
///
/// The lock file sits *beside* the tree and never inside it. A rebuild
/// removes the tree, and a lock file removed while a holder has it open is a
/// lock the next opener cannot see -- so the claim would be granted twice and
/// prove nothing.
fn claim(cache: &Path, name: &str) -> bool {
    let Ok(file) = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(cache.join(format!("{name}.lock")))
    else {
        return false;
    };
    // `WouldBlock` is the only refusal that means somebody holds this. The
    // other one this binary is exposed to is `EINTR`: it reaps thousands of
    // spawned `jails` processes, so `SIGCHLD` arrives constantly and `fs2`
    // surfaces an interrupted `flock` as an ordinary error. Reading that as
    // contention would send an ordinary single run to a private tree and a
    // cold build for no reason at all.
    for _ in 0..16 {
        match fs2::FileExt::try_lock_exclusive(&file) {
            Ok(()) => {
                FIXTURE_CLAIMS
                    .lock()
                    .unwrap_or_else(|holder| holder.into_inner())
                    .push(file);
                return true;
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => return false,
            Err(_) => continue,
        }
    }
    false
}

/// A persistent toolchain directory whose validity also depends on harness
/// inputs which are compiled into this integration-test binary rather than
/// the `jails` executable itself (for example proof-application manifests).
pub fn cached_toolchain_dir_with_salt(label: &str, salt: &str) -> (PathBuf, bool) {
    const CACHE_SCHEMA: u32 = 1;
    let root = claim_fixture(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/jails-e2e-cache"),
        label,
    );
    let executable = PathBuf::from(env!("CARGO_BIN_EXE_jails"));
    let metadata = fs::metadata(executable).unwrap();
    let modified = metadata
        .modified()
        .unwrap()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let stamp = if salt.is_empty() {
        // Preserve the existing stamp for callers which depend only on the
        // product executable, so adding salted caches does not cold-rebuild
        // every unrelated persistent fixture.
        format!("{CACHE_SCHEMA}:{}:{modified}\n", metadata.len())
    } else {
        format!(
            "{CACHE_SCHEMA}:{}:{modified}:{:016x}\n",
            metadata.len(),
            stable_cache_salt(salt)
        )
    };
    let marker = root.join(".jails-generated-stamp");
    if root.join(".jails-generated-ready").is_file()
        && fs::read_to_string(&marker).is_ok_and(|existing| existing == stamp)
    {
        return (root, false);
    }
    if root.exists() {
        fs::remove_dir_all(&root).unwrap();
    }
    fs::create_dir_all(&root).unwrap();
    fs::write(marker, stamp).unwrap();
    (root, true)
}

fn stable_cache_salt(value: &str) -> u64 {
    // FNV-1a is sufficient here: this is cache invalidation, not an identity
    // or security boundary, and its explicit algorithm stays stable across
    // Rust releases unlike an implementation-selected standard hasher.
    value.bytes().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    })
}

pub fn mark_toolchain_dir_generated(root: &Path) {
    fs::write(root.join(".jails-generated-ready"), "ready\n").unwrap();
}

/// How old a fixture has to be before it is rubbish rather than evidence.
///
/// Well clear of a full sweep: the longest single test here is minutes, so a
/// directory this old belongs to a run that finished long ago. Anything
/// younger is left alone, including every fixture a *concurrent* run is
/// using.
const FIXTURE_LIFETIME: std::time::Duration = std::time::Duration::from_secs(60 * 60);

/// How many fixtures may sit in the temporary directory before the oldest are
/// collected regardless of age.
///
/// **Age alone is the wrong bound.** Every fixture is `keep()`d so a failure
/// can be inspected, so several back-to-back runs inside `FIXTURE_LIFETIME`
/// fill `/tmp` with nothing stale by the age rule, and the next run collapses
/// with `No space left on device` in tests that are working.
///
/// A count rather than a byte budget: the sweep already stats each entry, and
/// a recursive size walk of ten thousand trees on every test binary's start
/// would cost more than the space it reclaims. At the measured size per
/// fixture this budget is about 2 GB, which leaves the last run whole on a
/// 16 GB `/tmp` with room for the next two.
const FIXTURE_BUDGET: usize = 3000;

/// The age below which a fixture is never collected, whatever the budget says.
///
/// The budget must not become a way to delete a *concurrent* run's fixtures,
/// which is the one thing `FIXTURE_LIFETIME`'s comment promises. Ten minutes
/// is longer than any single test here and far shorter than the hour the age
/// rule waits, so an over-budget sweep still only reaches finished work.
const FIXTURE_FLOOR: std::time::Duration = std::time::Duration::from_secs(10 * 60);

/// Remove `jails-*` fixtures older than `FIXTURE_LIFETIME`, once per process.
///
/// Opportunistic on purpose: it never fails a test. A directory that cannot
/// be read or removed -- one another user owns, one a running process holds
/// -- is skipped, because a cleaner that can break a test run is worse than
/// a full disk you can `rm`.
/// What a sweep collects, given every `jails-*` entry and its age.
///
/// Pure so the policy can be tested: the rules are an age cutoff, a floor
/// that protects a concurrent run, and a budget that counts *every* survivor
/// while only ever deleting from those above the floor.
fn fixtures_to_collect<P: Clone>(entries: &[(std::time::Duration, P)], budget: usize) -> Vec<P> {
    let mut collected = Vec::new();
    let mut eligible = Vec::new();
    let mut kept = 0usize;
    for (age, path) in entries {
        if *age > FIXTURE_LIFETIME {
            collected.push(path.clone());
            continue;
        }
        // Everything still here counts against the budget, including the
        // fixtures too young to be collected -- otherwise a run whose own
        // output is most of the corpus would read as under budget and the
        // disk would fill anyway.
        kept += 1;
        if *age > FIXTURE_FLOOR {
            eligible.push((*age, path.clone()));
        }
    }
    // Oldest first, and only down to the budget. Nothing below the floor was
    // ever a candidate, so a concurrent run's fixtures cannot be reached
    // however far over budget this is.
    let over = kept.saturating_sub(budget).min(eligible.len());
    if over > 0 {
        eligible.sort_unstable_by_key(|(age, _)| std::cmp::Reverse(*age));
        collected.extend(eligible.into_iter().take(over).map(|(_, path)| path));
    }
    collected
}

fn sweep_stale_fixtures() {
    use std::sync::Once;
    static SWEPT: Once = Once::new();
    SWEPT.call_once(|| {
        let Ok(entries) = fs::read_dir(std::env::temp_dir()) else {
            return;
        };
        let mut found = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            // Everything this project creates, not just this file's
            // fixtures: each module's unit tests have their own `scratch()`
            // label, so the leftovers are `jails-e2e-*`, `jails-run-test-*`,
            // `jails-new-test-*`, `jails-project-*` and half a dozen more.
            if !name.starts_with("jails-") {
                continue;
            }
            let Ok(age) = fs::metadata(&path)
                .and_then(|m| m.modified())
                .map(|when| when.elapsed().unwrap_or_default())
            else {
                continue;
            };
            found.push((age, path));
        }
        for path in fixtures_to_collect(&found, FIXTURE_BUDGET) {
            fs::remove_dir_all(&path).ok();
        }
    });
}

#[cfg(test)]
mod scratch_failure_tests {
    use super::*;

    /// A full disk names the disk rather than jails.
    #[test]
    fn a_full_disk_names_the_disk_and_says_what_to_do() {
        let message = scratch_failure("new-cli", &io::Error::from(io::ErrorKind::StorageFull));
        assert!(
            message.contains("This is the machine and not the tree"),
            "a full disk must not read as a jails defect: {message}"
        );
        assert!(
            message.contains("fix: rm -rf"),
            "every refusal carries a fix line: {message}"
        );
    }

    /// The same defect facing the other way. A fixture that blames the disk
    /// for a permission error sends the reader to `df` and there is nothing
    /// there to find.
    #[test]
    fn a_failure_that_is_not_the_disk_does_not_claim_it_is() {
        let message = scratch_failure("new-cli", &io::Error::from(io::ErrorKind::PermissionDenied));
        assert!(
            !message.contains("no room left"),
            "only a storage error may be reported as one: {message}"
        );
        assert!(
            message.contains("new-cli"),
            "the fixture that could not be created is the one useful fact: {message}"
        );
    }
}

#[cfg(test)]
mod fixture_claim_tests {
    use super::*;

    /// The second run must not be handed the tree the first one is building
    /// in.
    ///
    /// `flock` treats two opens of one file independently even inside a
    /// single process, so one process can play both runs here -- which is
    /// what makes the property testable at all without launching a gate.
    #[test]
    fn a_fixture_another_run_holds_is_not_shared_with_it() {
        let cache = temp_dir("fixture-claim");
        assert!(
            claim(&cache, "toolbox"),
            "the first claim should be granted"
        );
        assert!(
            !claim(&cache, "toolbox"),
            "a fixture somebody holds must be refused, not shared"
        );
    }

    #[test]
    fn a_second_run_builds_its_own_copy_rather_than_the_one_in_use() {
        let cache = temp_dir("fixture-fallback");
        let first = claim_fixture(&cache, "toolbox");
        let second = claim_fixture(&cache, "toolbox");
        assert_eq!(
            first,
            cache.join("toolbox"),
            "the first run gets the shared fixture, which is the whole point of it"
        );
        assert_ne!(
            second, first,
            "a claimed fixture must not be handed to a second run"
        );
    }

    /// The lock file has to survive the rebuild that empties the tree, or the
    /// claim is granted twice: `remove_dir_all` on the fixture would take the
    /// lock file with it and the next opener would create a fresh one.
    #[test]
    fn a_rebuild_of_the_tree_does_not_release_the_claim() {
        let cache = temp_dir("fixture-rebuild");
        let root = claim_fixture(&cache, "toolbox");
        fs::create_dir_all(&root).unwrap();
        fs::remove_dir_all(&root).unwrap();
        assert!(
            !claim(&cache, "toolbox"),
            "emptying the fixture must not release the claim on it"
        );
    }
}

#[cfg(test)]
mod sweep_tests {
    use super::*;
    use std::time::Duration;

    /// Ages in seconds, oldest last, labelled by position.
    fn aged(seconds: &[u64]) -> Vec<(Duration, usize)> {
        seconds
            .iter()
            .enumerate()
            .map(|(index, secs)| (Duration::from_secs(*secs), index))
            .collect()
    }

    const YOUNG: u64 = 30;
    const MIDDLE: u64 = 20 * 60;

    #[test]
    fn anything_past_its_lifetime_goes_whatever_the_budget_says() {
        let entries = aged(&[FIXTURE_LIFETIME.as_secs() + 1, YOUNG, MIDDLE]);
        assert_eq!(fixtures_to_collect(&entries, 99), vec![0]);
    }

    #[test]
    fn a_corpus_inside_the_budget_is_left_alone() {
        let entries = aged(&[MIDDLE, MIDDLE, YOUNG]);
        assert!(fixtures_to_collect(&entries, 3).is_empty());
    }

    /// The case the age rule alone cannot answer: several suite runs inside
    /// `FIXTURE_LIFETIME` leave nothing stale and no space.
    #[test]
    fn over_budget_collects_the_oldest_first_and_only_the_excess() {
        let entries = aged(&[MIDDLE + 3, MIDDLE + 1, MIDDLE + 4, MIDDLE + 2]);
        // Four survivors against a budget of two: the two oldest go, and they
        // are chosen by age rather than by the order the directory listed them.
        assert_eq!(fixtures_to_collect(&entries, 2), vec![2, 0]);
    }

    /// Young fixtures count against the budget even though they can never be
    /// the ones collected -- otherwise a run whose own output is most of the
    /// corpus reads as under budget and the disk fills anyway.
    #[test]
    fn fixtures_below_the_floor_still_count_against_the_budget() {
        let entries = aged(&[MIDDLE, YOUNG, YOUNG, YOUNG]);
        assert_eq!(fixtures_to_collect(&entries, 3), vec![0]);
    }

    /// The budget must never reach a concurrent run's fixtures, which is what
    /// `FIXTURE_LIFETIME`'s comment promises and what makes the sweep safe to
    /// run from thirty-three binaries at once.
    #[test]
    fn nothing_below_the_floor_is_collected_however_far_over_budget() {
        let entries = aged(&[YOUNG; 40]);
        assert!(fixtures_to_collect(&entries, 1).is_empty());
    }
}

/// Build a `jails` invocation rooted at `cwd`. When `fake_maven_dir` is
/// set, PATH is replaced with just that directory -- not prepended to the
/// real PATH -- so a real mvn/mvnd installed on this machine can never be
/// found ahead of (or instead of) the fake one from write_fake_maven().
pub fn jails_cmd(cwd: &Path, fake_maven_dir: Option<&Path>) -> Command {
    let mut cmd = Command::new(bin());
    cmd.current_dir(cwd);
    if let Some(dir) = fake_maven_dir {
        cmd.env("PATH", dir);
    }
    cmd
}

/// Writes fake executables named after each of `names` (e.g. "mvn",
/// "mvnd") that append their argv to `log` and exit 0 -- a mock Maven for
/// tests that only care what jails would have run, not what a real build
/// does.
pub fn write_fake_maven(dir: &Path, names: &[&str], log: &Path) {
    fs::create_dir_all(dir).unwrap();
    for name in names {
        let script = dir.join(name);
        fs::write(
            &script,
            format!(
                "#!/bin/sh\necho \"$0 $*\" >> \"{}\"\nexit 0\n",
                log.display()
            ),
        )
        .unwrap();
        set_executable(&script);
    }
}

#[cfg(unix)]
pub fn set_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).unwrap();
}

pub fn read_log(log: &Path) -> String {
    fs::read_to_string(log).unwrap_or_default()
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct MavenReportSummary {
    pub reports: usize,
    pub tests: usize,
    pub failures: usize,
    pub errors: usize,
    pub skipped: usize,
}

impl MavenReportSummary {
    pub fn add(&mut self, other: Self) {
        self.reports += other.reports;
        self.tests += other.tests;
        self.failures += other.failures;
        self.errors += other.errors;
        self.skipped += other.skipped;
    }
}

/// Totals from Maven's per-class XML reports, excluding summary metadata.
///
/// Failsafe and Surefire use the same `testsuite` attributes. Reading those
/// reports is the coverage gate for the real generated projects: a successful
/// Maven exit alone also describes a run that selected or skipped nothing.
pub fn maven_report_summary(root: &Path, report_dir: &str) -> MavenReportSummary {
    let reports = root.join("target").join(report_dir);
    xml_test_report_summary(&reports)
}

/// Totals from the JUnit XML directory emitted by a build tool.
///
/// Maven and Gradle use the same `testsuite` attributes. Keeping one reader
/// makes the example-manifest gates prove collected and executed tests rather
/// than treating a zero-test green build as sufficient.
pub fn xml_test_report_summary(reports: &Path) -> MavenReportSummary {
    let mut summary = MavenReportSummary::default();
    for entry in fs::read_dir(reports)
        .unwrap_or_else(|error| panic!("could not read {}: {error}", reports.display()))
        .flatten()
    {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with("TEST-") || !name.ends_with(".xml") {
            continue;
        }
        let xml = fs::read_to_string(entry.path()).unwrap();
        let suite = xml
            .lines()
            .find(|line| line.contains("<testsuite"))
            .unwrap_or_else(|| panic!("{} has no testsuite element", entry.path().display()));
        summary.reports += 1;
        summary.tests += xml_attribute(suite, "tests", &entry.path());
        summary.failures += xml_attribute(suite, "failures", &entry.path());
        summary.errors += xml_attribute(suite, "errors", &entry.path());
        summary.skipped += xml_attribute(suite, "skipped", &entry.path());
    }
    summary
}

fn xml_attribute(line: &str, name: &str, report: &Path) -> usize {
    let prefix = format!("{name}=\"");
    line.split_once(&prefix)
        .and_then(|(_, rest)| rest.split_once('"'))
        .and_then(|(value, _)| value.parse().ok())
        .unwrap_or_else(|| panic!("{} has no numeric {name} attribute", report.display()))
}

mod toolchain;

// Not every test crate that includes this harness uses every gate helper,
// and this file is included by several of them.
#[allow(unused_imports)]
pub use toolchain::{skip, skip_unsupported_environment, toolchain_enabled};

pub fn real_mvn_available() -> bool {
    toolchain_enabled() && real_path_dirs().any(|dir| dir.join("mvn").is_file())
}

pub fn real_java_available() -> bool {
    toolchain_enabled()
        && real_path_dirs().any(|dir| dir.join("java").is_file())
        && real_path_dirs().any(|dir| dir.join("javac").is_file())
}

/// Whether the `javac` on PATH understands the release jails generates for
/// (`release::TARGET_RELEASE`). Presence of a JDK is not enough: a JDK older than
/// the target rejects `--release N` outright. Tests that really compile
/// generated code skip on this rather than hiding the reason in a javac
/// failure.
///
/// Answers `false` whenever the tier is switched off, so the probe is never
/// the thing that decides whether the expensive suite runs -- see
/// [`toolchain_enabled`].
pub fn real_java_supports_target_release() -> bool {
    toolchain_enabled()
        && Command::new("javac")
            .arg(format!("--release={TARGET_RELEASE}"))
            .arg("-version")
            .output()
            .map(|out| out.status.success())
            .unwrap_or(false)
}

/// Testcontainers (and the real `add db` Spring contextLoads check) need a
/// running daemon, not just a binary on PATH.
pub fn real_docker_available() -> bool {
    toolchain_enabled()
        && Command::new("docker")
            .args(["info"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
}

/// The real constant, not a copy of it: `pom` is a library crate, so
/// integration tests read it rather than keeping one fact in two places.
pub use jails_spec::release::TARGET_RELEASE;

fn real_path_dirs() -> impl Iterator<Item = PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .collect::<Vec<_>>()
        .into_iter()
}

/// The real PATH with mvnd removed for generated-project builds.
///
/// mvnd fails intermittently when this suite drives many projects
/// concurrently. These checks are about the projects Maven receives, so keep
/// them on the stable Maven executable and test mvnd command selection with
/// the isolated fake-toolchain tests below.
pub fn real_path_without_mvnd() -> String {
    let dirs = real_path_dirs()
        .filter(|dir| !dir.join("mvnd").is_file())
        .collect::<Vec<_>>();
    std::env::join_paths(dirs)
        .unwrap()
        .to_string_lossy()
        .into_owned()
}

pub fn jails_cmd_with_path(cwd: &Path, path: &str) -> ToolchainCommand {
    let mut cmd = Command::new(bin());
    cmd.current_dir(cwd);
    cmd.env("PATH", path);
    cmd.env("MAVEN_ARGS", REAL_MAVEN_ARGS);
    cmd.env("JAVA_TOOL_OPTIONS", REAL_JAVA_TOOL_OPTIONS);
    ToolchainCommand::new(cmd)
}

/// Settings for real generated-project Maven runs.
///
/// Most Spring contexts in the proof applications do not exercise messaging.
/// Starting their Kafka listeners makes them reconnect to the production
/// Compose address until the context shuts down. The real messaging IT opts
/// back in explicitly and supplies a broker through `@ServiceConnection`, so
/// this removes accidental localhost traffic without omitting that test.
const REAL_MAVEN_ARGS: &str = "-ntp -DforkCount=0 -Dspring.main.banner-mode=off \
    -Dlogging.level.root=WARN -Dspring.kafka.listener.auto-startup=false \
    -Dspring.datasource.hikari.maximum-pool-size=2 \
    -Dspring.datasource.hikari.minimum-idle=0";

// **JUnit class-level parallelism is refused here, measured.** Generated
// tests share a database, ports and fixture files, so running their classes
// concurrently does not overlap work, it manufactures contention and retries,
// and the gate goes from green to slower with failures. The app suite gets
// away with it because it declares the mode explicitly per service and holds
// parallelism at two. Do not lift that setting to the general case without
// re-measuring.

/// Startup policy for the deliberately short-lived JVMs in this test suite.
///
/// These processes compile a small generated project, run a small test set,
/// and exit. Spending CPU and memory on a throughput collector and fully
/// optimised tiered compilation costs more than it can repay in that lifetime.
const REAL_JAVA_TOOL_OPTIONS: &str = "-XX:+UseSerialGC -XX:TieredStopAtLevel=1";

/// A real Maven process with test output tuned for a parent Rust harness.
///
/// Spring and Kafka otherwise emit tens of thousands of INFO lines which
/// libtest retains until the test completes. Warnings and all Maven failures
/// remain visible; only successful-framework chatter is suppressed.
pub fn real_maven_cmd(cwd: &Path, path: &str) -> ToolchainCommand {
    let mut cmd = Command::new("mvn");
    cmd.current_dir(cwd);
    cmd.env("PATH", path);
    cmd.env("MAVEN_ARGS", REAL_MAVEN_ARGS);
    cmd.env("JAVA_TOOL_OPTIONS", real_java_tool_options());
    ToolchainCommand::new(cmd)
}

/// `javac` with this project's dependencies already on the classpath.
pub fn real_javac_cmd(cwd: &Path, path: &str) -> ToolchainCommand {
    let mut cmd = Command::new("javac");
    cmd.current_dir(cwd);
    cmd.env("PATH", path);
    cmd.env("JAVA_TOOL_OPTIONS", real_java_tool_options());
    ToolchainCommand::new(cmd)
}

/// This project's dependency classpath, resolved once per distinct `pom.xml`.
///
/// **Resolution is the expensive half and it is identical across projects that
/// share a pom**, which most generated fixtures do. Keyed on the pom's bytes
/// rather than on the directory, so the first test to need a given shape pays
/// `dependency:build-classpath` and every later one reads a file.
///
/// Cached under `target/` for the reason every other ledger here is: it is a
/// derived value, never an input to a result, so a stale or missing entry
/// costs a re-resolve and changes no verdict.
fn dependency_classpath(root: &Path, path: &str) -> Option<String> {
    let pom = fs::read(root.join("pom.xml")).ok()?;
    let key = format!("{:016x}", stable_cache_salt(&String::from_utf8_lossy(&pom)));
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/jails-test-classpath");
    fs::create_dir_all(&directory).ok()?;
    let cached = directory.join(format!("{key}.txt"));
    if let Ok(existing) = fs::read_to_string(&cached)
        && !existing.trim().is_empty()
    {
        return Some(existing);
    }

    // **Unique per call, not per process.** Keyed on the pid alone, two
    // threads of one test binary resolving the same pom write the same
    // staging path: one truncates the file the other is about to read, and
    // the loser hands `javac` an empty classpath.
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let unique = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let scratch = directory.join(format!("{key}.{}.{unique}.tmp", std::process::id()));
    let resolved = real_maven_cmd(root, path)
        .args([
            "-q",
            "-B",
            "org.apache.maven.plugins:maven-dependency-plugin:3.6.1:build-classpath",
            &format!("-Dmdep.outputFile={}", scratch.display()),
        ])
        .output()
        .ok()?;
    if !resolved.status.success() {
        let _ = fs::remove_file(&scratch);
        return None;
    }
    let classpath = fs::read_to_string(&scratch).ok()?;
    let _ = fs::rename(&scratch, &cached);
    let _ = fs::remove_file(&scratch);
    Some(classpath)
}

/// Every `.java` file the project's main compilation would see.
fn main_sources(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for tree in ["src/main/java", ".jails/generated/main/java"] {
        let mut stack = vec![root.join(tree)];
        while let Some(directory) = stack.pop() {
            let Ok(entries) = fs::read_dir(&directory) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|ext| ext == "java") {
                    found.push(path);
                }
            }
        }
    }
    found
}

/// Prove the generated main sources compile, **without paying for Maven**.
///
/// Maven's own startup is over a second before any work, against a JVM start
/// of a few milliseconds, and the question here -- *does this compile* -- is
/// one `javac` answers directly. The dependency resolution is hoisted into
/// [`dependency_classpath`], where it happens once per distinct pom and is
/// then a file read. What is left is the compiler.
///
/// **It refuses rather than skipping when the classpath cannot be resolved.**
/// A compile check that quietly becomes a no-op is the failure this tier
/// exists to prevent, and it is the one this helper could most easily
/// introduce.
pub fn assert_main_sources_compile(root: &Path, path: &str, what: &str) {
    let classpath = dependency_classpath(root, path)
        .unwrap_or_else(|| panic!("{what}: could not resolve the dependency classpath"));
    let sources = main_sources(root);
    assert!(!sources.is_empty(), "{what}: no main sources to compile");

    // **`target/classes`, where Maven would have put them.** Tests downstream
    // assert on specific `.class` files through [`compiled_class`], and a
    // helper that quietly moved the output would turn those into assertions
    // about a path nothing writes.
    let classes = root.join("target/classes");
    fs::create_dir_all(&classes).unwrap();
    let compiled = real_javac_cmd(root, path)
        .args(["--release", TARGET_RELEASE])
        .args(["-classpath", classpath.trim()])
        .args(["-d", &classes.display().to_string()])
        .args(sources.iter().map(|source| source.display().to_string()))
        .output()
        .unwrap();
    assert!(
        compiled.status.success(),
        "{what} did not compile:\n{}\n{}",
        String::from_utf8_lossy(&compiled.stdout),
        String::from_utf8_lossy(&compiled.stderr)
    );
}

/// The JVM startup policy, overridable for measurement.
///
/// `JAILS_JAVA_TOOL_OPTIONS` replaces it wholesale. The right flags depend on
/// how long these JVMs actually live, so the choice has to stay measurable
/// rather than remembered.
fn real_java_tool_options() -> String {
    std::env::var("JAILS_JAVA_TOOL_OPTIONS").unwrap_or_else(|_| REAL_JAVA_TOOL_OPTIONS.to_string())
}

/// A Docker-compatible command which shares the generated-project process
/// budget with Maven, javac, Surefire and Testcontainers.
pub fn real_docker_cmd(cwd: &Path) -> ToolchainCommand {
    let mut cmd = Command::new("docker");
    cmd.current_dir(cwd);
    ToolchainCommand::new(cmd)
}

/// Substitutions for the base images a generated `Dockerfile` names, as
/// `from=replacement` pairs separated by commas.
///
/// **This exists for a sandbox that re-terminates TLS, and it is the only
/// shape that reaches BuildKit.** A generated Dockerfile opens with `# syntax=
/// docker/dockerfile:1`, and that external frontend resolves every `FROM`
/// against the *registry* -- so a locally retagged image is invisible to it,
/// however the tag is arranged, measured rather than assumed.
/// `--build-context <name>=docker-image://<image>` is what the frontend does
/// honour.
///
/// Empty for everybody else, so the gate builds exactly what jails wrote. The
/// substitution is deliberately not derived from anything jails knows: an
/// image that trusts a proxy CA is a fact about the machine, and inferring one
/// would let the gate quietly stop testing the base image the template names.
pub fn oci_base_substitutions() -> Vec<(String, String)> {
    std::env::var("JAILS_OCI_BASE_IMAGES")
        .unwrap_or_default()
        .split(',')
        .filter_map(|pair| pair.split_once('='))
        .map(|(from, to)| (from.trim().to_string(), to.trim().to_string()))
        .filter(|(from, to)| !from.is_empty() && !to.is_empty())
        .collect()
}

/// PostgreSQL and Kafka shared by the complete generated-application gate.
///
/// The generated Testcontainers beans remain enabled by default. This harness
/// opts out of those per-Maven-JVM beans and gives every application its own
/// database on one suite-scoped PostgreSQL plus one suite-scoped Kafka broker.
/// The exact containers are removed by `Drop`, including during unwinding.
pub struct AppSuiteServices {
    postgres: ContainerGuard,
    kafka: ContainerGuard,
}

#[derive(Clone, Copy)]
pub struct AppSuiteEndpoints {
    postgres_port: u16,
    kafka_port: u16,
}

impl AppSuiteEndpoints {
    pub fn configure_maven(&self, command: &mut ToolchainCommand, app_name: &str) {
        command.args([
            "-Djails.testcontainers.postgres.enabled=false".to_string(),
            "-Djails.testcontainers.kafka.enabled=false".to_string(),
            format!(
                "-Dspring.datasource.url=jdbc:postgresql://127.0.0.1:{}/{}",
                self.postgres_port,
                database_name(app_name)
            ),
            "-Dspring.datasource.username=postgres".to_string(),
            "-Dspring.datasource.password=postgres".to_string(),
            "-Dspring.datasource.hikari.maximum-pool-size=2".to_string(),
            "-Dspring.datasource.hikari.minimum-idle=0".to_string(),
            format!(
                "-Dspring.kafka.bootstrap-servers=127.0.0.1:{}",
                self.kafka_port
            ),
            format!("-Dspring.kafka.consumer.group-id=jails-{app_name}"),
        ]);
    }
}

pub fn configure_app_unit_maven(command: &mut ToolchainCommand, app_name: &str) {
    command.args([
        "-Djails.testcontainers.postgres.enabled=false".to_string(),
        "-Djails.testcontainers.kafka.enabled=false".to_string(),
        format!(
            "-Dspring.datasource.url=jdbc:h2:mem:jails_{};MODE=PostgreSQL;DB_CLOSE_DELAY=-1",
            app_name.replace('-', "_")
        ),
        "-Dspring.datasource.username=sa".to_string(),
        "-Dspring.datasource.password=".to_string(),
        "-Dspring.datasource.hikari.connection-test-query=SELECT 1".to_string(),
        "-Dspring.datasource.hikari.connection-init-sql=SELECT 1".to_string(),
        "-Dspring.flyway.enabled=false".to_string(),
        "-Dspring.kafka.bootstrap-servers=127.0.0.1:1".to_string(),
        "-Djunit.jupiter.execution.parallel.enabled=true".to_string(),
        "-Djunit.jupiter.execution.parallel.mode.default=same_thread".to_string(),
        "-Djunit.jupiter.execution.parallel.mode.classes.default=concurrent".to_string(),
        "-Djunit.jupiter.execution.parallel.config.strategy=fixed".to_string(),
        "-Djunit.jupiter.execution.parallel.config.fixed.parallelism=2".to_string(),
    ]);
}

impl AppSuiteServices {
    pub fn start(
        app_names: &[&str],
        launched: std::sync::mpsc::Sender<AppSuiteEndpoints>,
        postgres_ready: std::sync::mpsc::Sender<()>,
    ) -> Self {
        sweep_orphaned_suite_containers();
        let nonce = format!(
            "{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let [postgres_port, kafka_port] = reserve_loopback_ports(2)[..] else {
            unreachable!("two ports were requested")
        };
        let endpoints = AppSuiteEndpoints {
            postgres_port,
            kafka_port,
        };
        let databases = app_names
            .iter()
            .map(|name| database_name(name))
            .collect::<Vec<_>>();
        let postgres_nonce = nonce.clone();
        let kafka_nonce = nonce;
        let launch_barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
        let (postgres, kafka) = std::thread::scope(|scope| {
            let postgres_barrier = std::sync::Arc::clone(&launch_barrier);
            let postgres = scope.spawn(move || {
                let postgres = ContainerGuard::start(
                    format!("jails-suite-postgres-{postgres_nonce}"),
                    &[
                        "-p".to_string(),
                        format!("127.0.0.1:{postgres_port}:5432"),
                        "-e".to_string(),
                        "POSTGRES_USER=postgres".to_string(),
                        "-e".to_string(),
                        "POSTGRES_PASSWORD=postgres".to_string(),
                        "postgres:17-alpine".to_string(),
                    ],
                );
                postgres_barrier.wait();
                wait_for_postgres(&postgres.name);
                for database in databases {
                    let deadline = Instant::now() + Duration::from_secs(5);
                    loop {
                        let status = real_docker_cmd(Path::new("."))
                            .args([
                                "exec",
                                &postgres.name,
                                "createdb",
                                "-U",
                                "postgres",
                                &database,
                            ])
                            .status()
                            .unwrap();
                        if status.success() {
                            break;
                        }
                        assert!(
                            Instant::now() < deadline,
                            "could not create suite database {database}"
                        );
                        std::thread::sleep(Duration::from_millis(100));
                    }
                }
                postgres_ready.send(()).unwrap();
                postgres
            });
            let kafka_barrier = std::sync::Arc::clone(&launch_barrier);
            let kafka = scope.spawn(move || {
                let kafka = ContainerGuard::start(
                    format!("jails-suite-kafka-{kafka_nonce}"),
                    &[
                        "-p".to_string(),
                        format!("127.0.0.1:{kafka_port}:9092"),
                        "-e".to_string(),
                        "KAFKA_NODE_ID=1".to_string(),
                        "-e".to_string(),
                        // The image defaults to a 1 GiB initial heap. This
                        // suite exchanges only a handful of records, so that
                        // reservation adds startup and GC pressure without
                        // exercising a production-relevant capacity boundary.
                        "KAFKA_HEAP_OPTS=-Xms128m -Xmx256m".to_string(),
                        "-e".to_string(),
                        "KAFKA_PROCESS_ROLES=broker,controller".to_string(),
                        "-e".to_string(),
                        "KAFKA_LISTENERS=PLAINTEXT://:9092,CONTROLLER://:9093".to_string(),
                        "-e".to_string(),
                        format!(
                            "KAFKA_ADVERTISED_LISTENERS=PLAINTEXT://127.0.0.1:{kafka_port}"
                        ),
                        "-e".to_string(),
                        "KAFKA_CONTROLLER_LISTENER_NAMES=CONTROLLER".to_string(),
                        "-e".to_string(),
                        "KAFKA_LISTENER_SECURITY_PROTOCOL_MAP=CONTROLLER:PLAINTEXT,PLAINTEXT:PLAINTEXT"
                            .to_string(),
                        "-e".to_string(),
                        "KAFKA_CONTROLLER_QUORUM_VOTERS=1@localhost:9093".to_string(),
                        "-e".to_string(),
                        "KAFKA_OFFSETS_TOPIC_REPLICATION_FACTOR=1".to_string(),
                        "-e".to_string(),
                        "KAFKA_GROUP_INITIAL_REBALANCE_DELAY_MS=0".to_string(),
                        "-e".to_string(),
                        "KAFKA_TRANSACTION_STATE_LOG_REPLICATION_FACTOR=1".to_string(),
                        "-e".to_string(),
                        "KAFKA_TRANSACTION_STATE_LOG_MIN_ISR=1".to_string(),
                        "apache/kafka:4.1.0".to_string(),
                    ],
                );
                kafka_barrier.wait();
                wait_for_log(&kafka.name, "Kafka Server started", Duration::from_secs(45));
                kafka
            });
            launch_barrier.wait();
            launched.send(endpoints).unwrap();
            (postgres.join().unwrap(), kafka.join().unwrap())
        });

        Self { postgres, kafka }
    }
}

impl Drop for AppSuiteServices {
    fn drop(&mut self) {
        // Kafka and PostgreSQL can each take several seconds to stop. They are
        // independent exact-name removals, so reap them concurrently and wait
        // for both; the suite leaves no service behind without serialising two
        // shutdown grace periods onto its critical path.
        std::thread::scope(|scope| {
            scope.spawn(|| self.postgres.remove());
            scope.spawn(|| self.kafka.remove());
        });
    }
}

/// Remove suite containers whose owning test process no longer exists.
///
/// [`ContainerGuard`]'s `Drop` is the ordinary path and it is reliable for
/// every ordinary ending, panics included. It cannot run when the process is
/// killed outright -- `SIGKILL` from the OOM killer, a `docker kill` of the
/// runner, a hard `^C`. Left to accumulate the containers exhaust Docker's
/// address pool and unrelated container tests begin failing, with nothing
/// pointing at the run that actually caused it.
///
/// So ownership is recoverable rather than merely promised: every suite
/// container carries its creator's pid in its name, and a pid that is gone is
/// a container nobody will ever reap. Sweeping at start-up rather than at exit
/// is deliberate -- an exit-time sweep is one more thing a kill can skip,
/// while a start-time sweep repairs whatever the last run could not.
///
/// It never touches a container whose owner is alive, so concurrent runs of
/// the suite do not reap each other.
fn sweep_orphaned_suite_containers() {
    // `ps --format '{{.Names}}'` rather than `-q` plus an `inspect` each: it is
    // one subprocess instead of one per container, and this exact spelling
    // behaves identically on docker and on podman's shim.
    let listed = Command::new("docker")
        .args([
            "ps",
            "-a",
            "--filter",
            "name=jails-suite-",
            "--format",
            "{{.Names}}",
        ])
        .stderr(Stdio::null())
        .output();
    let Ok(listed) = listed else { return };
    if !listed.status.success() {
        return;
    }
    for name in String::from_utf8_lossy(&listed.stdout).lines() {
        let name = name.trim();
        let Some(pid) = suite_container_pid(name) else {
            continue;
        };
        if Path::new(&format!("/proc/{pid}")).exists() {
            continue;
        }
        eprintln!("sweeping orphaned suite container {name} (pid {pid} is gone)");
        let _ = Command::new("docker")
            .args(["kill", name])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

/// The pid embedded in a suite container's name, if this is one of ours.
///
/// `jails-suite-<service>-<pid>-<nanos>`, so the pid is the second field from
/// the end. It is a separate function because it is the only part of the sweep
/// that can be *wrong* rather than merely fail: everything else either finds a
/// live process or kills a container this suite named, while a parser that
/// reads the wrong field returns a plausible pid belonging to something else
/// entirely -- and the consequence of that is killing a stranger's container.
/// The prefix check is what keeps a foreign name from ever reaching the digit
/// parse.
fn suite_container_pid(name: &str) -> Option<u32> {
    name.strip_prefix("jails-suite-")?
        .rsplit('-')
        .nth(1)?
        .parse()
        .ok()
}

#[cfg(test)]
mod sweep_container_tests {
    use super::suite_container_pid;

    #[test]
    fn the_pid_is_read_from_a_real_suite_container_name() {
        assert_eq!(
            suite_container_pid("jails-suite-kafka-2355472-1788267293555018168"),
            Some(2355472)
        );
        assert_eq!(
            suite_container_pid("jails-suite-postgres-2355472-1788267293555018168"),
            Some(2355472)
        );
    }

    #[test]
    fn a_container_this_suite_did_not_name_is_never_claimed() {
        // The whole point of the prefix: `docker ps --filter name=` matches a
        // *substring*, so a reader's own container can appear in the listing.
        // Reaping one because its name happens to end in two numeric fields
        // would be the sweep causing exactly the damage it exists to undo.
        for foreign in [
            "postgres-2355472-1788267293555018168",
            "my-jails-suite-kafka-1-2",
            "jails-generated-app-123-456",
        ] {
            assert_eq!(suite_container_pid(foreign), None, "claimed {foreign}");
        }
    }

    #[test]
    fn a_name_without_a_numeric_pid_field_is_left_alone() {
        for malformed in [
            "jails-suite-kafka",
            "jails-suite-kafka-notapid-1788267293555018168",
            "jails-suite-",
        ] {
            assert_eq!(suite_container_pid(malformed), None, "claimed {malformed}");
        }
    }
}

struct ContainerGuard {
    name: String,
}

impl ContainerGuard {
    fn start(name: String, args: &[String]) -> Self {
        let mut command = real_docker_cmd(Path::new("."));
        command.args(["run", "-d", "--rm", "--name", &name]);
        command.args(args);
        let output = command.infrastructure_start_output().unwrap();
        assert!(
            output.status.success(),
            "could not start {name}: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        Self { name }
    }

    fn remove(&mut self) {
        if self.name.is_empty() {
            return;
        }
        let name = std::mem::take(&mut self.name);
        let _ = Command::new("docker")
            .args(["kill", &name])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

impl Drop for ContainerGuard {
    fn drop(&mut self) {
        self.remove();
    }
}

/// A loopback port with something listening on it, held open by the returned
/// listener.
///
/// A command that starts compose services then waits for PostgreSQL to accept
/// connections -- `jails run --services start` -- polls for thirty seconds
/// before giving up. Against a fake `docker` that starts no container the
/// poll can only ever time out, so a test asking the narrow question *did
/// compose go up before Spring* would pay the entire budget for an answer it
/// was not asking about. Shortening the production budget would be the wrong
/// fix: how long to wait for a database is a real decision. So the fixture
/// stops lying instead. A fake `docker` that reports success is claiming a
/// server is up, and this makes that claim true enough for the probe that
/// checks it -- a *better* model of the case under test, not a weaker one --
/// and leaves the readiness wait itself covered by the tests that are about
/// it.
///
/// A listening socket completes the handshake from its backlog with no
/// `accept()` call, so nothing here has to serve anything. Hold the listener
/// for as long as the port must answer: dropping it closes the socket.
pub fn listening_loopback_port() -> (TcpListener, u16) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("could not bind a loopback port");
    let port = listener.local_addr().unwrap().port();
    (listener, port)
}

/// `count` distinct free loopback ports, for handing to containers.
///
/// **All of them are held at once, then released together**, and that is the
/// whole reason this takes a count instead of being called in a loop. Asking
/// the kernel for an ephemeral port means binding port 0, reading what you
/// got, and closing -- so a second call made *after* the first has closed can
/// be handed the very same port back. Reserving PostgreSQL's port and then
/// Kafka's that way buys the confusing kind of failure: two containers are
/// told to publish on one port, the
/// second `docker run` fails to bind, and the suite reports it as a broker
/// that would not start.
///
/// Holding every listener until all of them are chosen makes that impossible,
/// because the kernel will not hand out a port it currently has bound.
///
/// What it cannot close is the window between this returning and the
/// container binding: another process on the machine can still take the port
/// in between. Nothing short of letting the container choose its own port
/// fixes that, and it is a far smaller window than the one above -- this is
/// the standard reservation trick, with its one real footgun removed.
pub fn reserve_loopback_ports(count: usize) -> Vec<u16> {
    let held: Vec<TcpListener> = (0..count)
        .map(|_| TcpListener::bind(("127.0.0.1", 0)).expect("could not reserve a loopback port"))
        .collect();
    held.iter()
        .map(|listener| listener.local_addr().unwrap().port())
        .collect()
    // `held` drops here: every port is chosen before any is released.
}

fn database_name(app_name: &str) -> String {
    format!("jails_{}", app_name.replace('-', "_"))
}

fn wait_for_postgres(container: &str) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let ready = Command::new("docker")
            .args(["exec", container, "pg_isready", "-U", "postgres"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success());
        if ready {
            return;
        }
        assert!(Instant::now() < deadline, "PostgreSQL did not become ready");
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn wait_for_log(container: &str, marker: &str, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    loop {
        let output = Command::new("docker").args(["logs", container]).output();
        if output.is_ok_and(|output| {
            String::from_utf8_lossy(&output.stdout).contains(marker)
                || String::from_utf8_lossy(&output.stderr).contains(marker)
        }) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "{container} did not become ready"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// A process which may enter Maven, javac, Surefire or Testcontainers.
///
/// Libtest defaults to one worker per CPU. A generated-project test then
/// starts Maven, which starts javac and another JVM, and some of those JVMs
/// start containers. One such tree per core multiplies every build's time
/// several-fold and can make a Kafka container exit during startup, so the
/// number of process trees is bounded separately and derived from the
/// machine. Pure Rust tests and fake-toolchain commands do not use this
/// wrapper and remain fully parallel.
pub struct ToolchainCommand {
    inner: Command,
}

impl ToolchainCommand {
    fn new(inner: Command) -> Self {
        Self { inner }
    }

    pub fn arg<S: AsRef<OsStr>>(&mut self, arg: S) -> &mut Self {
        self.inner.arg(arg);
        self
    }

    pub fn args<I, S>(&mut self, args: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.inner.args(args);
        self
    }

    pub fn status(&mut self) -> io::Result<ExitStatus> {
        let description = profile_command_description(&self.inner);
        let queued_at = Instant::now();
        let _permit = TOOLCHAIN_PROCESSES.acquire(max_toolchain_processes());
        let queue_time = queued_at.elapsed();
        let started_at = Instant::now();
        let result = self.inner.status();
        let run_time = started_at.elapsed();
        record_subprocess(&self.inner, queue_time, run_time);
        report_profiled_command(description, "status", queue_time, run_time);
        result
    }

    pub fn output(&mut self) -> io::Result<Output> {
        self.output_with_permit(&TOOLCHAIN_PROCESSES, max_toolchain_processes())
    }

    /// Run the short `docker run` phase without waiting behind Maven jobs.
    ///
    /// This is deliberately private and used only by `ContainerGuard::start`;
    /// readiness probes and every other Docker command keep using the ordinary
    /// toolchain pool.
    fn infrastructure_start_output(&mut self) -> io::Result<Output> {
        self.output_with_permit(
            &INFRASTRUCTURE_START_PROCESSES,
            MAX_INFRASTRUCTURE_START_PROCESSES,
        )
    }

    fn output_with_permit(&mut self, pool: &PermitPool, maximum: usize) -> io::Result<Output> {
        let description = profile_command_description(&self.inner);
        let queued_at = Instant::now();
        let permit = pool.acquire(maximum);
        let queue_time = queued_at.elapsed();
        let started_at = Instant::now();
        let result = self.inner.output();
        let run_time = started_at.elapsed();
        drop(permit);
        record_subprocess(&self.inner, queue_time, run_time);
        report_profiled_command(description, "output", queue_time, run_time);
        result
    }
}

fn profile_command_description(command: &Command) -> Option<String> {
    profiling().then(|| {
        let _ = test_profile_epoch();
        let cwd = command
            .get_current_dir()
            .map_or_else(|| ".".into(), |path| path.display().to_string());
        let program = command.get_program().to_string_lossy();
        let args = command
            .get_args()
            .map(|arg| arg.to_string_lossy())
            .collect::<Vec<_>>()
            .join(" ");
        format!("cwd={cwd} command={program} {args}")
    })
}

fn report_profiled_command(
    description: Option<String>,
    operation: &str,
    queue_time: Duration,
    run_time: Duration,
) {
    if let Some(description) = description {
        let end_ms = test_profile_epoch().elapsed().as_millis();
        let run_ms = run_time.as_millis();
        let queue_ms = queue_time.as_millis();
        let run_start_ms = end_ms.saturating_sub(run_ms);
        let start_ms = run_start_ms.saturating_sub(queue_ms);
        eprintln!(
            "JAILS_TEST_PROFILE operation={operation} start_ms={start_ms} run_start_ms={run_start_ms} end_ms={end_ms} queue_ms={queue_ms} run_ms={run_ms} {description}"
        );
    }
}

/// What every toolchain subprocess cost, recorded whatever `JAILS_TEST_PROFILE`
/// says.
///
/// The per-subprocess lines behind that variable answer *which command was
/// slow* and cost a few thousand lines of output, so they stay opt-in. This
/// answers a different question -- *where does the wall clock go* -- in four
/// numbers, and it has to be on by default or it is not there on the run that
/// raises the question. A measurement that needs a specially dispatched run
/// is a measurement nobody takes.
///
/// Bucketed by the program's own file stem rather than by a table of tools:
/// a stem jails has never heard of is still a subprocess whose seconds are on
/// the wall clock, and naming the buckets in advance is how one goes missing.
#[derive(Default)]
struct SubprocessTotals {
    /// The end of the last subprocess, against the profile epoch. Divided into
    /// the summed run time this gives mean concurrency, which is the number
    /// that separates "the machine is saturated" from "the schedule has gaps".
    span_ms: u128,
    queue_ms: u128,
    by_tool: BTreeMap<String, (u64, u128)>,
}

fn record_subprocess(command: &Command, queue_time: Duration, run_time: Duration) {
    static TOTALS: OnceLock<Mutex<SubprocessTotals>> = OnceLock::new();
    let tool = Path::new(command.get_program())
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let end_ms = test_profile_epoch().elapsed().as_millis();

    let Ok(mut totals) = TOTALS.get_or_init(Default::default).lock() else {
        return;
    };
    totals.span_ms = totals.span_ms.max(end_ms);
    totals.queue_ms += queue_time.as_millis();
    let entry = totals.by_tool.entry(tool).or_default();
    entry.0 += 1;
    entry.1 += run_time.as_millis();
    write_subprocess_totals(&totals);
}

/// Rewritten whole on every subprocess, because a test binary has no exit hook
/// that libtest will run. The file is a few hundred bytes against subprocesses
/// that are seconds long, so the cost does not register; the alternative is an
/// append-only log that has to be parsed back, for a summary this size.
fn write_subprocess_totals(totals: &SubprocessTotals) {
    let Some(name) = std::env::current_exe().ok().and_then(|exe| {
        exe.file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
    }) else {
        return;
    };
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/jails-test-profile");
    if fs::create_dir_all(&directory).is_err() {
        return;
    }
    let mut body = format!(
        "span_ms\t{}\nqueue_ms\t{}\n",
        totals.span_ms, totals.queue_ms
    );
    for (tool, (count, run_ms)) in &totals.by_tool {
        body.push_str(&format!("tool\t{tool}\t{count}\t{run_ms}\n"));
    }
    // Staged and renamed for `CostLedger`'s reason: the aggregator may read
    // while a binary is still running, and a half-written file would be a
    // silently wrong summary rather than a visible failure.
    let path = directory.join(format!("{name}.tsv"));
    let staging = directory.join(format!("{name}.{}.tmp", std::process::id()));
    if fs::write(&staging, body).is_ok() {
        let _ = fs::rename(&staging, &path);
    }
    let _ = fs::remove_file(&staging);
}

fn test_profile_epoch() -> &'static Instant {
    static EPOCH: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    EPOCH.get_or_init(Instant::now)
}

/// How many Maven/JDK processes may run at once, when nothing says otherwise.
///
/// Derived from the machine rather than a constant, which is wrong in both
/// directions: it throttles a 16-core machine and oversubscribes a 4-core one.
/// Six is the floor because that is the number the suite is tuned against,
/// and twelve the ceiling because these are JVMs -- Surefire forks again
/// underneath each one, so the limit is memory and disk rather than cores.
/// Three quarters of the cores reaches the ceiling on a 16-core machine, where
/// the permit queue measures zero, and leaves a four-core runner on the floor;
/// past twelve there is no queue left to drain, measured.
///
/// `JAILS_TEST_MAX_TOOLCHAIN_PROCESSES` overrides it.
fn default_max_toolchain_processes() -> usize {
    std::thread::available_parallelism()
        .map(|cores| (cores.get() * 3 / 4).clamp(6, 12))
        .unwrap_or(6)
}
const MAX_INFRASTRUCTURE_START_PROCESSES: usize = 2;
static TOOLCHAIN_PROCESSES: PermitPool = PermitPool::new("toolchain");
static INFRASTRUCTURE_START_PROCESSES: PermitPool = PermitPool::new("infrastructure");

fn max_toolchain_processes() -> usize {
    static MAXIMUM: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *MAXIMUM.get_or_init(|| {
        std::env::var("JAILS_TEST_MAX_TOOLCHAIN_PROCESSES")
            .ok()
            .and_then(|value| value.parse().ok())
            .filter(|value| *value > 0)
            .unwrap_or_else(default_max_toolchain_processes)
    })
}

/// A budget of concurrent toolchain processes, shared across **processes**.
///
/// An in-process budget -- a `Mutex` and a `Condvar` -- is the whole
/// machine's budget only while one binary runs at a time: a second shell
/// running `cargo test` while the first still is silently doubles the
/// machine's JVM count. This one does not, whatever launches the suite.
///
/// `flock` is the budget, one lock file per permit under `target/`. Three
/// properties are why it is a file lock rather than anything cleverer:
///
/// - **The kernel releases it however the holder dies.** A test that panics,
///   a binary killed by `--test-threads` teardown, a `^C` -- none of them can
///   leak a permit. A counter in a file would need a crash-safe decrement,
///   which is the PID-file problem `jails-support`'s `lock.rs` rejects for the
///   same reason.
/// - **It is per workspace.** The slots live under `target/`, so two
///   checkouts do not share a budget and a `cargo clean` cannot corrupt one --
///   it just makes the next acquirer create the files again.
/// - **`O_CLOEXEC` is std's default.** `lock.rs` records the trap: a lock
///   lives on the open file description, `fork` duplicates it, and a child
///   spawned while the parent holds one keeps it alive until `exec`. Every
///   `File` std opens is close-on-exec, so the Maven process this permit
///   exists to throttle cannot inherit the permit that admitted it.
///
/// Acquisition polls rather than blocking in the kernel. `flock` has no
/// "wait for any of these six", and a permit is held for the length of a
/// Maven run, so a 25 ms poll is far below the noise of what it is gating.
struct PermitPool {
    slots: std::sync::OnceLock<PathBuf>,
    name: &'static str,
}

/// How long to wait before rescanning the slots. Two orders of magnitude
/// below the ~7s Maven run this gates, and far above a `fork`/`exec` window.
const PERMIT_POLL: std::time::Duration = std::time::Duration::from_millis(25);

impl PermitPool {
    const fn new(name: &'static str) -> Self {
        Self {
            slots: std::sync::OnceLock::new(),
            name,
        }
    }

    /// The directory holding this pool's slot files, created once.
    ///
    /// `target/` and not `env::temp_dir()`: the budget is a property of this
    /// workspace, and a stale directory here is harmless -- the files carry no
    /// state, only locks, so anything left behind is reused rather than
    /// repaired.
    fn directory(&self) -> &Path {
        self.slots.get_or_init(|| {
            let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("target/jails-test-permits")
                .join(self.name);
            // Reported through the slot refusals rather than swallowed: a
            // pool whose directory does not exist refuses every slot with
            // `NotFound`, which reads as contention and is not.
            let _ = fs::create_dir_all(&root);
            root
        })
    }

    fn acquire(&self, maximum: usize) -> ProcessPermit {
        let directory = self.directory();
        loop {
            for slot in 0..maximum {
                let path = directory.join(format!("{slot}.lock"));
                let Ok(file) = fs::OpenOptions::new()
                    .create(true)
                    .truncate(false)
                    .write(true)
                    .open(&path)
                else {
                    // A pool that cannot create its slots must not deadlock the
                    // suite: an unthrottled run is slow, a hung one is broken.
                    return ProcessPermit { _file: None };
                };
                if fs2::FileExt::try_lock_exclusive(&file).is_ok() {
                    return ProcessPermit { _file: Some(file) };
                }
            }
            std::thread::sleep(PERMIT_POLL);
        }
    }

    #[cfg(test)]
    fn try_acquire(&self, maximum: usize) -> Option<ProcessPermit> {
        self.try_acquire_reporting(maximum).0
    }

    /// The same attempt, plus why each slot was refused.
    ///
    /// "Returned `None`" is not enough to tell a slot that was locked from a
    /// slot that could not be opened. The reason travels with the failure so
    /// the panic names it.
    #[cfg(test)]
    fn try_acquire_reporting(&self, maximum: usize) -> (Option<ProcessPermit>, Vec<String>) {
        let directory = self.directory();
        let mut refusals = Vec::new();
        for slot in 0..maximum {
            let path = directory.join(format!("{slot}.lock"));
            let file = match fs::OpenOptions::new()
                .create(true)
                .truncate(false)
                .write(true)
                .open(&path)
            {
                Ok(file) => file,
                Err(error) => {
                    refusals.push(format!(
                        "slot {slot}: could not open {}: {error}",
                        path.display()
                    ));
                    continue;
                }
            };
            // `WouldBlock` is the only refusal that means "somebody holds
            // this". Anything else is the call failing, and the one this
            // binary is exposed to is `EINTR`: it reaps thousands of spawned
            // `jails` processes, so `SIGCHLD` arrives constantly, and `fs2`
            // surfaces an interrupted `flock` as an ordinary error. Reading
            // that as contention makes a pool refuse its own slots under load.
            let mut attempts = 0;
            loop {
                match fs2::FileExt::try_lock_exclusive(&file) {
                    Ok(()) => return (Some(ProcessPermit { _file: Some(file) }), refusals),
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        refusals.push(format!("slot {slot}: held"));
                        break;
                    }
                    Err(_) if attempts < 16 => attempts += 1,
                    Err(error) => {
                        refusals.push(format!(
                            "slot {slot}: {error} ({:?}), still failing after {attempts} retries",
                            error.kind()
                        ));
                        break;
                    }
                }
            }
        }
        (None, refusals)
    }
}

/// One permit. The lock is released when the `File` closes, which the kernel
/// also does if this process dies holding it.
struct ProcessPermit {
    _file: Option<File>,
}

/// A super-simple, hand-written Spring Boot project (pinned versions, JDK
/// 26) -- deliberately not fetched from start.spring.io, so the
/// "does scaffold produce a project that compiles" check never depends on
/// that external service.
/// Where the compiler puts a file that a test names by an older path.
///
/// **A closed list.** Each row is a documented divergence: ports live in
/// `repository` rather than `app`, an operation's typed command in
/// `application/commands` rather than `service`, and the two repository
/// adapters in sub-packages under `adapters` that say which one they are.
/// Exact paths are pinned by the golden trees; this exists so a test about a
/// file's *contents* is not also a second, weaker assertion about its package.
const MOVED_PACKAGES: &[(&str, &[&str])] = &[
    (
        "src/main/java/com/example/demo/app/",
        &["com/example/demo/repository/"],
    ),
    (
        "src/main/java/com/example/demo/service/",
        &[
            "com/example/demo/application/commands/",
            "com/example/demo/application/queries/",
            "com/example/demo/application/transitions/",
            // A `Storing...UseCase` names the implementation rather than the
            // port, and the implementation is the operation's JDBC adapter.
            "com/example/demo/adapters/jdbc/",
        ],
    ),
    (
        "src/main/java/com/example/demo/adapters/",
        &[
            "com/example/demo/adapters/jdbc/",
            "com/example/demo/adapters/memory/",
            "com/example/demo/adapters/http/",
            // An operation's JDBC adapter is not filed with the adapters: the
            // compiler emits one typed class per operation and puts it with
            // the other operations rather than with the driver code.
            "com/example/demo/application/commands/",
            "com/example/demo/application/queries/",
            "com/example/demo/application/transitions/",
        ],
    ),
    (
        "src/main/java/com/example/demo/web/",
        &["com/example/demo/adapters/http/"],
    ),
    (
        // An event is a domain fact, so the compiler puts it with the domain
        // rather than with the transport that happens to carry it.
        "src/main/java/com/example/demo/messaging/",
        &["com/example/demo/domain/events/"],
    ),
    (
        "src/main/java/com/example/demo/jobs/",
        &["com/example/demo/jobs/"],
    ),
    (
        "src/test/java/com/example/demo/adapters/",
        &[
            "com/example/demo/adapters/jdbc/",
            "com/example/demo/adapters/memory/",
            "com/example/demo/adapters/http/",
        ],
    ),
    (
        "src/test/java/com/example/demo/web/",
        &["com/example/demo/adapters/http/"],
    ),
];

/// Where javac put the class for a generated source, in whichever package the
/// compiler puts that source.
///
/// **The `.class` is wherever the `.java` is**, so an assertion about
/// compiled output goes through the same table as one about source. Named with
/// the source path it is the output of -- `src/main/java/.../Foo.java` -- so
/// the caller states the thing they generated rather than an output layout
/// they would then have to keep in step with the build tool.
pub fn compiled_class(root: &Path, relative: &str) -> PathBuf {
    let resolved = generated_relative(root, relative);
    let (output, rest) = match resolved.split_once("/main/java/") {
        Some((_, rest)) => ("target/classes", rest.to_string()),
        None => match resolved.split_once("/test/java/") {
            Some((_, rest)) => ("target/test-classes", rest.to_string()),
            None => return root.join(relative),
        },
    };
    root.join(output).join(rest.replace(".java", ".class"))
}

/// The same resolution, as a path relative to the project root.
///
/// For an assertion about *reported* text rather than about bytes: `g field`
/// names each companion it regenerated, and the name it prints is the path the
/// compiler wrote rather than an older spelling.
pub fn generated_relative(root: &Path, relative: &str) -> String {
    generated(root, relative)
        .strip_prefix(root)
        .unwrap_or(Path::new(relative))
        .to_string_lossy()
        .replace('\\', "/")
}

/// A generated source path, in whichever tree this project keeps it.
///
/// **One helper rather than a sweep through three hundred assertions.** The
/// compiler renders into `.jails/generated/{main,test}/java`, the reader's own
/// sources stay under `src/`, and a test that asserts on a file jails wrote
/// should not have to know which of the two it is looking at -- that is the
/// projection's business, not the assertion's.
///
/// Falls back to the `src/` spelling when neither exists, so an absence check
/// reads as an absence rather than as a path that was never going to be there.
pub fn generated(root: &Path, relative: &str) -> PathBuf {
    let managed = match relative.strip_prefix("src/") {
        Some(rest) => root.join(".jails/generated").join(rest),
        None => root.join(".jails/generated").join(relative),
    };
    if managed.exists() {
        return managed;
    }
    // **The package may have moved, and only these moves count.** The
    // canonical layout differs from the older spelling in a closed set of
    // places, listed here so the divergence is written down once rather than
    // rediscovered at three hundred assertions -- and so a file that turns up
    // somewhere *unexpected* is still a failure. A basename search would
    // accept any location, which is exactly the check these tests are for.
    //
    // An older package name can map to more than one canonical package --
    // `service` splits into commands, queries and transitions by what the
    // operation *is* -- so each row lists its candidates and the basename
    // still has to match exactly.
    let tree = if relative.starts_with("src/test/") {
        ".jails/generated/test/java"
    } else {
        ".jails/generated/main/java"
    };
    for (from, candidates) in MOVED_PACKAGES {
        let Some(rest) = relative.strip_prefix(from) else {
            continue;
        };
        for to in *candidates {
            for name in renamed_kinds(rest) {
                let moved = root.join(tree).join(format!("{to}{name}"));
                if moved.exists() {
                    return moved;
                }
            }
        }
    }
    root.join(relative)
}

/// The same file under the type name the compiler gives its kind.
///
/// **The suffix moved with the package**, and for the same reason: an
/// operation is a command, a query or a transition, and the compiler names the
/// type after which one it is rather than after the word `jails g` happened to
/// be typed with. `RenameUseCase` in `service` is `RenameTransition` in
/// `application/transitions`.
///
/// Every candidate is still an exact path -- the basename has to match one of
/// these spellings in one of that row's packages -- so a file that turns up
/// somewhere unexpected is a failure rather than a match, which is what these
/// assertions exist to check. The original spelling comes first, so a name
/// that did not move is answered without consulting the table at all.
fn renamed_kinds(relative: &str) -> Vec<String> {
    const RENAMED: &[(&str, &[&str])] = &[
        ("UseCase", &["Command", "Transition", "Query"]),
        ("QueryController", &["Controller"]),
        ("UseCaseController", &["Controller"]),
    ];
    // **The prefix moves the same way the suffix does**, and each of these is
    // one older name for one canonical file rather than a pattern. `Storing`
    // marks the *implementation* of a use case -- the class that writes the
    // row -- and the compiler calls that the operation's JDBC adapter, so the
    // prefix is replaced rather than dropped. `Jdbc` is dropped where the
    // compiler does not distinguish an implementation by its technology.
    //
    // Applied once rather than recursively, which is the part that matters:
    // chaining them would turn `StoringPlaceOrderUseCase` into the bare
    // `PlaceOrderCommand` as well, and that name is the *port* -- a different
    // file, in a different package, which an assertion about the
    // implementation would then match instead.
    const PREFIXED: &[(&str, &str)] = &[("Jdbc", ""), ("Storing", "Jdbc"), ("Resolving", "Jdbc")];
    // **A durable job's unit of work is its queue.** A test may call the
    // record `...Work`; the compiler names it after what holds it, which is
    // the class a reader looks for when a job is not draining.
    const SUFFIXED: &[(&str, &str)] = &[("Work", "Queue")];
    let Some(stem) = relative.strip_suffix(".java") else {
        return vec![relative.to_string()];
    };
    let (directory, base) = match stem.rsplit_once('/') {
        Some((directory, base)) => (format!("{directory}/"), base),
        None => (String::new(), stem),
    };
    let mut stems = vec![base.to_string()];
    for (from, to) in PREFIXED {
        if let Some(rest) = base.strip_prefix(*from) {
            stems.push(format!("{to}{rest}"));
        }
    }
    for (from, to) in SUFFIXED {
        if let Some(head) = base.strip_suffix(*from) {
            stems.push(format!("{head}{to}"));
        }
    }
    let mut names = Vec::new();
    for stem in stems {
        names.push(format!("{directory}{stem}.java"));
        // **A companion test is renamed by whatever renamed its subject.**
        // `OpenTicketsQueryController` is `OpenTicketsController`, so its
        // test is too -- and a table listing both spellings of every row
        // would go stale on the first kind that gains one. The trailing `Test`
        // is lifted off, the rename applied to the type it names, and the
        // suffix put back.
        for (subject, tail) in [
            (stem.as_str(), ""),
            (stem.trim_end_matches("Test"), "Test"),
            (stem.trim_end_matches("IT"), "IT"),
        ] {
            if !tail.is_empty() && subject == stem {
                continue;
            }
            for (from, candidates) in RENAMED {
                let Some(head) = subject.strip_suffix(from) else {
                    continue;
                };
                names.extend(
                    candidates
                        .iter()
                        .map(|to| format!("{directory}{head}{to}{tail}.java")),
                );
            }
        }
    }
    names
}

/// Read a generated file, and say what *is* there when it is not.
///
/// **A bare `.unwrap()` on a missing generated path says `NotFound` and
/// nothing else**, which is the least useful failure in this suite: the
/// question is always whether the file moved, was renamed, or was never
/// written, and answering it meant re-running the command by hand. Listing the
/// managed tree turns each of those into a one-glance answer, and costs
/// nothing on the passing path.
pub fn read_generated(root: &Path, relative: &str) -> String {
    let path = generated(root, relative);
    match fs::read_to_string(&path) {
        Ok(source) => source,
        Err(error) => panic!(
            "could not read generated `{relative}` ({error}).\nThe managed tree holds:\n{}",
            managed_listing(root)
        ),
    }
}

/// Every path under `.jails/generated`, one per line.
pub fn managed_listing(root: &Path) -> String {
    fn walk(dir: &Path, base: &Path, into: &mut Vec<String>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, base, into);
            } else if let Ok(rest) = path.strip_prefix(base) {
                into.push(format!("  {}", rest.display()));
            }
        }
    }
    let base = root.join(".jails/generated");
    let mut found = Vec::new();
    walk(&base, &base, &mut found);
    found.sort();
    if found.is_empty() {
        return "  (nothing -- this project has no managed tree)".to_string();
    }
    found.join("\n")
}

/// What `jails resource status --output json` says about one resource.
///
/// **The canonical lifecycle record, read through the command that reports
/// it.** A project keeps three facts -- which Java type, which table, which
/// migrations -- in the model and the migration directory, and this is the
/// one place that answers from them.
/// Going through the product rather than the files is deliberate: a test that
/// re-derives the answer can disagree with what a reader is told.
pub fn resource_status(root: &Path, selector: &str) -> serde_json::Value {
    let output = Command::new(bin())
        .current_dir(root)
        .args(["resource", "status", selector, "--output", "json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "resource status {selector}: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "resource status {selector} is not JSON ({error}): {}",
            String::from_utf8_lossy(&output.stdout)
        )
    })
}

/// Give a fixture the model every mutating command needs.
///
/// **The on-ramp, run explicitly.** Any mutation initialises a project that has
/// none, so most tests never call this -- but a test whose *first* command is a
/// dry run does, because `--pretend` must not write and there is nothing to
/// plan against until the model exists. The refusal says exactly this; these
/// tests are about what comes after it.
pub fn become_canonical(root: &Path) {
    let output = Command::new(bin())
        .current_dir(root)
        .args(["model", "init"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "could not initialise the model: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Declare PostgreSQL storage, the way a reader would.
///
/// **A declaration, not a directory.** The compiler asks the *model* whether
/// the project has storage, so an empty `db/migration` directory says nothing
/// and no DDL is emitted. The declaration also brings the JDBC adapters and
/// the Testcontainers wiring the tests go on to assert about.
///
/// `--no-start` throughout: nothing here wants a container, and starting one
/// per test would put a docker invocation on the critical path.
pub fn declare_storage(root: &Path) {
    let output = Command::new(bin())
        .current_dir(root)
        .args(["add", "db", "--no-start"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "could not declare storage: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

pub fn write_spring_fixture(root: &Path) {
    let pkg_dir = root.join("src/main/java/com/example/demo");
    fs::create_dir_all(&pkg_dir).unwrap();
    fs::write(
        root.join("pom.xml"),
        SPRING_FIXTURE_POM.replace("{TARGET_RELEASE}", TARGET_RELEASE),
    )
    .unwrap();
    fs::write(
        pkg_dir.join("DemoApplication.java"),
        SPRING_FIXTURE_APPLICATION,
    )
    .unwrap();
    let test_dir = root.join("src/test/java/com/example/demo");
    fs::create_dir_all(&test_dir).unwrap();
    fs::write(
        test_dir.join("DemoApplicationTests.java"),
        SPRING_FIXTURE_TESTS,
    )
    .unwrap();
}

/// A Boot 2.7 project, for the tests that have to prove a classic
/// `MockMvc` template compiles.
///
/// The generated companion tests are written against `MockMvcTester`, which
/// is Spring Framework 6.2 (Boot 3.4), and carry a classic form for an older
/// project -- `jails new --gradle --boot 2.7.18` exists so those projects can
/// be worked in. A template written and not exercised is a template nobody
/// has proved compiles; this fixture is what exercises them.
///
/// Boot 2.7.18 on Java 17, resolved from Maven Central like every other pinned
/// version here, and it runs under this machine's JDK 26.
pub fn write_spring2_fixture(root: &Path) {
    let pkg_dir = root.join("src/main/java/com/example/demo");
    fs::create_dir_all(&pkg_dir).unwrap();
    fs::write(root.join("pom.xml"), SPRING2_FIXTURE_POM).unwrap();
    fs::write(
        pkg_dir.join("DemoApplication.java"),
        SPRING_FIXTURE_APPLICATION,
    )
    .unwrap();
    let test_dir = root.join("src/test/java/com/example/demo");
    fs::create_dir_all(&test_dir).unwrap();
    fs::write(
        test_dir.join("DemoApplicationTests.java"),
        SPRING_FIXTURE_TESTS,
    )
    .unwrap();
}

/// Pinned, and `spring-boot-starter-web` rather than `-webmvc`: the module was
/// renamed in Boot 4, and naming the Boot 4 spelling here would be the fixture
/// describing a project this version cannot build.
const SPRING2_FIXTURE_POM: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0">
    <modelVersion>4.0.0</modelVersion>
    <parent>
        <groupId>org.springframework.boot</groupId>
        <artifactId>spring-boot-starter-parent</artifactId>
        <version>2.7.18</version>
        <relativePath/>
    </parent>
    <groupId>com.example</groupId>
    <artifactId>demo</artifactId>
    <version>0.0.1-SNAPSHOT</version>
    <properties>
        <!-- 21, not 17: jails generates Java 21+ code and refuses below it,
             and `examples/minicom-spring/` pairs Boot 2.7.18 with 21 for the
             same reason. Boot 2.7 predates 21 but does not object to it. -->
        <java.version>21</java.version>
    </properties>
    <dependencies>
        <dependency>
            <groupId>org.springframework.boot</groupId>
            <artifactId>spring-boot-starter-web</artifactId>
        </dependency>
        <dependency>
            <groupId>org.springframework.boot</groupId>
            <artifactId>spring-boot-starter-test</artifactId>
            <scope>test</scope>
        </dependency>
    </dependencies>
    <build>
        <plugins>
            <plugin>
                <groupId>org.springframework.boot</groupId>
                <artifactId>spring-boot-maven-plugin</artifactId>
            </plugin>
        </plugins>
    </build>
</project>
"#;

/// A Spring project shaped exactly like the one `jails new --offline` writes.
///
/// It declares nothing `new` does not -- in particular not
/// `spring-boot-starter-webmvc-test`: `add security` generates a
/// `@WebMvcTest`, Boot 4 moved that class into a module
/// `spring-boot-starter-test` does not bring in, and a fixture that supplies
/// what the tool is supposed to supply proves nothing about the tool.
const SPRING_FIXTURE_POM: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0">
    <modelVersion>4.0.0</modelVersion>
    <parent>
        <groupId>org.springframework.boot</groupId>
        <artifactId>spring-boot-starter-parent</artifactId>
        <version>4.1.0</version>
        <relativePath/>
    </parent>
    <groupId>com.example</groupId>
    <artifactId>demo</artifactId>
    <version>0.0.1-SNAPSHOT</version>
    <properties>
        <java.version>{TARGET_RELEASE}</java.version>
    </properties>
    <dependencies>
        <dependency>
            <groupId>org.springframework.boot</groupId>
            <artifactId>spring-boot-starter-webmvc</artifactId>
        </dependency>
        <dependency>
            <groupId>org.springframework.boot</groupId>
            <artifactId>spring-boot-starter-test</artifactId>
            <scope>test</scope>
        </dependency>
    </dependencies>
    <build>
        <plugins>
            <plugin>
                <groupId>org.springframework.boot</groupId>
                <artifactId>spring-boot-maven-plugin</artifactId>
            </plugin>
        </plugins>
    </build>
</project>
"#;

const SPRING_FIXTURE_APPLICATION: &str = r#"package com.example.demo;

import org.springframework.boot.SpringApplication;
import org.springframework.boot.autoconfigure.SpringBootApplication;

@SpringBootApplication
public class DemoApplication {
    public static void main(String[] args) {
        SpringApplication.run(DemoApplication.class, args);
    }
}
"#;

const SPRING_FIXTURE_TESTS: &str = r#"package com.example.demo;

import org.junit.jupiter.api.Test;
import org.springframework.boot.test.context.SpringBootTest;

@SpringBootTest
class DemoApplicationTests {

    @Test
    void contextLoads() {}
}
"#;

/// Whether the project's own record names something.
///
/// **The record is the model plus the managed tree**: the declaration, and
/// the files it owns. A check that searched a bookkeeping file the project
/// does not have would answer "no" about everything, and a check that cannot
/// fail is a check that is not there.
pub fn ledger_mentions(root: &std::path::Path, needle: &str) -> bool {
    let model = std::fs::read_to_string(root.join(".jails/model.jdl")).unwrap_or_default();
    // A canonical project keeps its bookkeeping in three places, and which one
    // holds a given fact is the compiler's business rather than a test's: the
    // model states the declaration, the managed listing states what was
    // written, and the lock states the accepted projection -- including the
    // digest of every migration and the jails that produced them.
    let lock = std::fs::read_to_string(root.join(".jails/compiler.lock.json")).unwrap_or_default();
    model.contains(needle) || lock.contains(needle) || managed_listing(root).contains(needle)
}

/// A minimal plain-Maven project: a pom with a release level and JUnit, and
/// one class so `base_package` can find the tree. Shared with the golden
/// target, which needs the same starting point every time or the snapshots
/// are not comparable.
pub fn write_plain_fixture(root: &Path) {
    let pkg_dir = root.join("src/main/java/com/example/demo");
    std::fs::create_dir_all(&pkg_dir).unwrap();
    std::fs::write(
        root.join("pom.xml"),
        format!(
            // `modelVersion` and the project's own `version` are not
            // decoration: without them Maven refuses to read the POM at all,
            // and a test that only ever *inspects* this fixture passes
            // without ever building it.
            "<project>\n    <modelVersion>4.0.0</modelVersion>\n    <groupId>com.example</groupId>\n    <artifactId>demo</artifactId>\n    <version>0.1.0</version>\n    <properties>\n        <maven.compiler.release>{TARGET_RELEASE}</maven.compiler.release>\n    </properties>\n    <dependencies>\n        <dependency>\n            <groupId>org.junit.jupiter</groupId>\n            <artifactId>junit-jupiter</artifactId>\n            <version>5.11.4</version>\n            <scope>test</scope>\n        </dependency>\n    </dependencies>\n</project>\n"
        ),
    )
    .unwrap();
    std::fs::write(
        pkg_dir.join("DemoApplication.java"),
        "package com.example.demo;\n\npublic class DemoApplication {}\n",
    )
    .unwrap();
}

/// Which build the adopted fixture is.
///
/// A flavour rather than two fixtures: the reader's classes, packages and
/// directory names are the same foreignness in either case, and the thing that
/// differs is what jails *reads off the build file* -- which Boot version, and
/// therefore which repository wiring, which MockMvc form, which webmvc-test
/// module. Two copies would drift on the half that is not the point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Adopted {
    /// Plain Maven, JUnit only. Cheap to build.
    Plain,
    /// Spring Boot, with the reader's own `@SpringBootApplication`.
    Spring,
}

/// The reader's own files in [`write_adopted_fixture`], as (path, body).
///
/// Public so a caller can assert they come back byte-identical: that is the
/// property an adopted project is for, and the one easiest to break without
/// noticing.
pub const ADOPTED_READER_FILES: [(&str, &str); 4] = [
    (
        "OrdersService.java",
        "package net.acme.legacy;\n\npublic final class OrdersService {\n    public String name() {\n        return \"orders\";\n    }\n}\n",
    ),
    (
        "domain/Money.java",
        "package net.acme.legacy.domain;\n\npublic record Money(long minor, String currency) {\n    public Money {\n        if (minor < 0) {\n            throw new IllegalArgumentException(\"minor must not be negative\");\n        }\n    }\n}\n",
    ),
    (
        "persistence/OrderStore.java",
        "package net.acme.legacy.persistence;\n\nimport java.util.List;\n\npublic interface OrderStore {\n    List<String> ids();\n}\n",
    ),
    (
        "web/OrderEndpoint.java",
        "package net.acme.legacy.web;\n\npublic final class OrderEndpoint {\n    public String route() {\n        return \"/orders\";\n    }\n}\n",
    ),
];

/// The one reader file only the Spring flavour has.
///
/// It is the reader's, not jails': an adopted Spring project has an entry
/// point somebody else wrote, and `base_package()` finds the package from it.
/// Listing it here rather than beside the fixture is what puts it in
/// [`adopted_reader_bytes`], so it is held byte-for-byte like the rest.
pub const ADOPTED_SPRING_FILES: [(&str, &str); 1] = [(
    "OrdersApplication.java",
    "package net.acme.legacy;\n\nimport org.springframework.boot.SpringApplication;\nimport org.springframework.boot.autoconfigure.SpringBootApplication;\n\n@SpringBootApplication\npublic class OrdersApplication {\n    public static void main(String[] args) {\n        SpringApplication.run(OrdersApplication.class, args);\n    }\n}\n",
)];

/// Every reader file this flavour writes.
pub fn adopted_files(flavour: Adopted) -> Vec<(&'static str, &'static str)> {
    let mut files: Vec<_> = ADOPTED_READER_FILES.to_vec();
    if flavour == Adopted::Spring {
        files.extend(ADOPTED_SPRING_FILES);
    }
    files
}

/// Where [`ADOPTED_READER_FILES`] live, relative to the project root.
pub fn adopted_base(root: &Path) -> PathBuf {
    root.join("src/main/java/net/acme/legacy")
}

/// A Maven project jails did not write.
///
/// Every manifest in `examples/proof-policy.tsv` is jails' own output, and a
/// generator can be perfectly correct about its own layout and wrong about
/// somebody else's.
///
/// Deliberately foreign in every respect a generator might assume: its own
/// groupId and artifactId, a package root that is not `com.example.demo`, a
/// `persistence` directory where jails would have written `adapters`, and
/// classes with bodies rather than stubs.
///
/// [`Adopted::Plain`] depends on JUnit alone, so a real build of it is cheap.
/// [`Adopted::Spring`] is the case where being foreign costs something: jails
/// reads the *reader's* pom to decide repository wiring, the MockMvc form and
/// the webmvc-test module, so a Spring project it did not create is where a
/// wrong reading shows up as Java that does not compile.
pub fn write_adopted_fixture(root: &Path, flavour: Adopted) {
    fs::create_dir_all(root).unwrap();
    fs::write(
        root.join("pom.xml"),
        match flavour {
            Adopted::Spring => ADOPTED_SPRING_POM.replace("{TARGET_RELEASE}", TARGET_RELEASE),
            Adopted::Plain => format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0">
  <modelVersion>4.0.0</modelVersion>
  <groupId>net.acme.legacy</groupId>
  <artifactId>orders-service</artifactId>
  <version>2.4.1</version>
  <properties>
    <maven.compiler.release>{TARGET_RELEASE}</maven.compiler.release>
    <project.build.sourceEncoding>UTF-8</project.build.sourceEncoding>
  </properties>
  <dependencies>
    <dependency>
      <groupId>org.junit.jupiter</groupId>
      <artifactId>junit-jupiter</artifactId>
      <version>6.1.1</version>
      <scope>test</scope>
    </dependency>
  </dependencies>
</project>
"#
            ),
        },
    )
    .unwrap();
    let base = adopted_base(root);
    for (relative, body) in adopted_files(flavour) {
        let at = base.join(relative);
        fs::create_dir_all(at.parent().unwrap()).unwrap();
        fs::write(&at, body).unwrap();
    }
}

/// The Spring flavour's build file: the reader's own coordinates under the
/// same pinned Boot parent every other Spring fixture here uses.
///
/// Pinned rather than fetched for the reason `write_spring_fixture` is, and it
/// declares nothing jails is supposed to declare -- `webmvc-test` in
/// particular is left out, because a fixture that supplies what the tool must
/// supply hides exactly the defect these tests exist to find.
const ADOPTED_SPRING_POM: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0">
  <modelVersion>4.0.0</modelVersion>
  <parent>
    <groupId>org.springframework.boot</groupId>
    <artifactId>spring-boot-starter-parent</artifactId>
    <version>4.1.0</version>
    <relativePath/>
  </parent>
  <groupId>net.acme.legacy</groupId>
  <artifactId>orders-service</artifactId>
  <version>2.4.1</version>
  <properties>
    <java.version>{TARGET_RELEASE}</java.version>
  </properties>
  <dependencies>
    <dependency>
      <groupId>org.springframework.boot</groupId>
      <artifactId>spring-boot-starter-webmvc</artifactId>
    </dependency>
    <dependency>
      <groupId>org.springframework.boot</groupId>
      <artifactId>spring-boot-starter-test</artifactId>
      <scope>test</scope>
    </dependency>
  </dependencies>
</project>
"#;

/// Every reader file's current bytes, in [`adopted_files`] order.
///
/// `unwrap` rather than a skip-if-missing: a reader file that has *vanished*
/// is the loudest way this can fail, and reading it as "nothing to compare"
/// would report the deletion as preservation.
pub fn adopted_reader_bytes(root: &Path, flavour: Adopted) -> Vec<String> {
    let base = adopted_base(root);
    adopted_files(flavour)
        .iter()
        .map(|(relative, _)| fs::read_to_string(base.join(relative)).unwrap())
        .collect()
}

#[cfg(test)]
mod permit_pool_tests {
    use super::{
        INFRASTRUCTURE_START_PROCESSES, MAX_INFRASTRUCTURE_START_PROCESSES, PermitPool,
        TOOLCHAIN_PROCESSES,
    };

    /// A pool whose budget belongs to *this process and this test alone*.
    ///
    /// `tests/common/mod.rs` is compiled into every integration-test binary,
    /// so this module runs in every one of them -- and since the budget is a
    /// `flock` on named files, a fixed name would make those copies contend
    /// with each other for the very slots they are asserting about. A
    /// production pool wants exactly this sharing; a test *about* the sharing
    /// has to own its own budget, so the name carries a per-process token.
    ///
    /// A pid alone is not that. Pids are recycled, so a binary started later
    /// is routinely handed the pid of one that has already exited -- and a
    /// slot lock outlives the process that took it whenever a forked child
    /// inherited the descriptor and has not reached `exec` yet. These binaries
    /// spawn thousands of `jails` processes, so that window is hit, and it
    /// looks like a second acquire failing under full-suite load and passing
    /// alone.
    ///
    /// The token is per *process*, not per call, because
    /// `a_budget_is_shared_by_every_pool_of_the_same_name` needs two pools
    /// built from one label to be the same pool. Same label, same directory;
    /// different run, different directory, whatever the kernel does with pids.
    fn pool(label: &str) -> PermitPool {
        static TOKEN: std::sync::OnceLock<String> = std::sync::OnceLock::new();
        let token = TOKEN.get_or_init(|| {
            // **Nanoseconds since the epoch, not within the second.** The
            // binaries start together and pids are recycled, so a pid
            // plus a sub-second reading collides often enough to be seen: two
            // live processes then share one named budget, and the assertions
            // below -- which are about *this* process holding and releasing a
            // permit -- fail on a permit somebody else took. It reads exactly
            // like a bug in the pool and is a bug in the label.
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|since| since.as_nanos())
                .unwrap_or_default();
            format!("{}-{nanos}", std::process::id())
        });
        let name = format!("test-{label}-{token}");
        PermitPool::new(Box::leak(name.into_boxed_str()))
    }

    #[test]
    fn infrastructure_start_pool_has_two_reusable_permits() {
        assert_eq!(MAX_INFRASTRUCTURE_START_PROCESSES, 2);
        let pool = pool("reusable");
        let (first, refusals) = pool.try_acquire_reporting(MAX_INFRASTRUCTURE_START_PROCESSES);
        let first = first.unwrap_or_else(|| {
            panic!(
                "the first of two permits was refused in a directory this process \
                 created for itself; slot by slot: {}",
                refusals.join("; ")
            )
        });
        let (second, refusals) = pool.try_acquire_reporting(MAX_INFRASTRUCTURE_START_PROCESSES);
        let second = second.unwrap_or_else(|| {
            panic!(
                "the second of two permits was refused; slot by slot: {}",
                refusals.join("; ")
            )
        });

        assert!(
            pool.try_acquire(MAX_INFRASTRUCTURE_START_PROCESSES)
                .is_none()
        );
        drop(first);
        // **Bounded, because the release can lag under full-suite load.**
        // With every binary reaping thousands of `jails` children, a slot
        // this process has just closed can still report as held for a few
        // milliseconds. The cause is not established, so this waits rather
        // than claiming to explain it:
        // what the test is for is that a permit comes back at all, and a
        // release that takes a few milliseconds to become visible to a fresh
        // descriptor still satisfies that. It is written as a loop with a
        // named ceiling so a release that never lands still fails.
        let mut refusals = Vec::new();
        let mut replacement = None;
        for _ in 0..64 {
            let (acquired, reported) =
                pool.try_acquire_reporting(MAX_INFRASTRUCTURE_START_PROCESSES);
            if acquired.is_some() {
                replacement = acquired;
                break;
            }
            refusals = reported;
            std::thread::sleep(super::PERMIT_POLL);
        }
        let replacement = replacement.unwrap_or_else(|| {
            panic!(
                "a released permit never became reusable; slot by slot: {}",
                refusals.join("; ")
            )
        });

        drop(second);
        drop(replacement);
        assert!(
            pool.try_acquire(MAX_INFRASTRUCTURE_START_PROCESSES)
                .is_some()
        );
    }

    #[test]
    fn infrastructure_start_pool_is_separate_from_toolchain_pool() {
        assert!(!std::ptr::eq(
            &TOOLCHAIN_PROCESSES,
            &INFRASTRUCTURE_START_PROCESSES
        ));

        let toolchain = pool("separate-toolchain");
        let infrastructure = pool("separate-infrastructure");
        let _toolchain_permit = toolchain.acquire(1);

        assert!(toolchain.try_acquire(1).is_none());
        assert!(infrastructure.try_acquire(1).is_some());
    }

    /// The property a process-shared budget exists for.
    ///
    /// Two pools built independently under one name are what two *processes*
    /// are: each opens the slot files for itself, so each has its own open
    /// file description, and `flock` contends between them exactly as it does
    /// across a `fork`. If this passes in one process it holds across
    /// every binary, whatever launches them.
    #[test]
    fn a_budget_is_shared_by_every_pool_of_the_same_name() {
        let one = pool("shared-budget");
        let two = pool("shared-budget");

        let held = one.try_acquire(1).expect("the only permit");
        assert!(
            two.try_acquire(1).is_none(),
            "a second holder of the same named budget took a permit that was already out"
        );

        drop(held);
        // **Report the refusal, do not summarise it.** "The permit was not
        // released back to the shared budget" is one of three things a `None`
        // can mean here and the only one that is a bug in the pool: the slot
        // may equally have failed to *open* (fd exhaustion under a loaded
        // suite) or failed to lock for a reason that is not contention, and a
        // summary sends the reader after a release path that was never
        // involved.
        let (permit, refusals) = two.try_acquire_reporting(1);
        assert!(
            permit.is_some(),
            "the shared budget refused a permit after its only holder was dropped: {}",
            if refusals.is_empty() {
                "no slot reported a reason".to_string()
            } else {
                refusals.join("; ")
            }
        );
    }
}
