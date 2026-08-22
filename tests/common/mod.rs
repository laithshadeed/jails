#![allow(dead_code)]

pub mod scenarios;

use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Output, Stdio};
use std::sync::{Condvar, Mutex};

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
/// `examples/DOGFOOD.md` records the day 15,288 of them filled `/tmp` and
/// Maven failed with `Disk quota exceeded` mid-gate. It happened again on
/// 2026-08-22, which is why the sweep below exists: **keep the evidence from
/// this run, take out the ones nobody is looking at any more.**
pub fn temp_dir(label: &str) -> PathBuf {
    sweep_stale_fixtures();
    let dir = std::env::temp_dir().join(format!(
        "jails-e2e-{label}-{}-{:?}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// How old a fixture has to be before it is rubbish rather than evidence.
///
/// Well clear of a full sweep: the longest single test here is minutes, so a
/// directory this old belongs to a run that finished long ago. Anything
/// younger is left alone, including every fixture a *concurrent* run is
/// using.
const FIXTURE_LIFETIME: std::time::Duration = std::time::Duration::from_secs(60 * 60);

/// Remove `jails-*` fixtures older than `FIXTURE_LIFETIME`, once per process.
///
/// Opportunistic on purpose: it never fails a test. A directory that cannot
/// be read or removed -- one another user owns, one a running process holds
/// -- is skipped, because a cleaner that can break a test run is worse than
/// a full disk you can `rm`.
fn sweep_stale_fixtures() {
    use std::sync::Once;
    static SWEPT: Once = Once::new();
    SWEPT.call_once(|| {
        let Ok(entries) = fs::read_dir(std::env::temp_dir()) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            // Everything this project creates, not just this file's
            // fixtures: each module's unit tests have their own `scratch()`
            // label, so the leftovers are `jails-e2e-*`, `jails-run-test-*`,
            // `jails-new-test-*`, `jails-project-*` and half a dozen more.
            // Sweeping only the two prefixes I first thought of left 3,166
            // directories behind and 13 GB still gone.
            if !name.starts_with("jails-") {
                continue;
            }
            let stale = fs::metadata(&path)
                .and_then(|m| m.modified())
                .map(|when| when.elapsed().is_ok_and(|age| age > FIXTURE_LIFETIME))
                .unwrap_or(false);
            if stale {
                fs::remove_dir_all(&path).ok();
            }
        }
    });
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
fn set_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).unwrap();
}

pub fn read_log(log: &Path) -> String {
    fs::read_to_string(log).unwrap_or_default()
}

/// Set `JAILS_REQUIRE_TOOLCHAIN=1` to turn every "skipping: ..." into a
/// failure.
///
/// The real-toolchain tests self-skip when Maven, a new enough JDK or Docker
/// is missing, which is right on a laptop and wrong in CI: the suite reports
/// green while the one tier that answers "does this produce a project that
/// compiles?" has not run, and nothing in the output says so.
///
/// So the default stays permissive and CI opts in. A run with this set either
/// exercises the tier or fails naming what was missing -- it never passes
/// quietly.
#[track_caller]
pub fn skip(reason: &str) {
    if std::env::var_os("JAILS_REQUIRE_TOOLCHAIN").is_some_and(|v| v != "0") {
        panic!("JAILS_REQUIRE_TOOLCHAIN is set, but this test cannot run: {reason}");
    }
    eprintln!("skipping: {reason}");
}

pub fn real_mvn_available() -> bool {
    real_path_dirs().any(|dir| dir.join("mvn").is_file())
}

pub fn real_java_available() -> bool {
    real_path_dirs().any(|dir| dir.join("java").is_file())
        && real_path_dirs().any(|dir| dir.join("javac").is_file())
}

/// Whether the `javac` on PATH understands the release jails generates for
/// (`pom::TARGET_RELEASE`). Presence of a JDK is not enough: a JDK older than
/// the target rejects `--release N` outright. Tests that really compile
/// generated code skip on this rather than hiding the reason in a javac
/// failure; required CI sets `JAILS_REQUIRE_TOOLCHAIN=1` so it cannot skip.
pub fn real_java_supports_target_release() -> bool {
    Command::new("javac")
        .arg(format!("--release={TARGET_RELEASE}"))
        .arg("-version")
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

/// Testcontainers (and the real `add db` Spring contextLoads check) need a
/// running daemon, not just a binary on PATH.
pub fn real_docker_available() -> bool {
    Command::new("docker")
        .args(["info"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Kept in step with `pom::TARGET_RELEASE` by
/// `target_release_matches_the_binary` in tests/cli.rs -- the integration
/// tests compile against the binary, not the library, so the constant cannot
/// simply be imported.
pub const TARGET_RELEASE: &str = "25";

fn real_path_dirs() -> impl Iterator<Item = PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .collect::<Vec<_>>()
        .into_iter()
}

/// The real PATH with mvnd removed for generated-project builds.
///
/// mvnd currently fails intermittently when this suite drives many projects
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
    ToolchainCommand::new(cmd)
}

/// A real Maven process with test output tuned for a parent Rust harness.
///
/// Spring and Kafka otherwise emit tens of thousands of INFO lines which
/// libtest retains until the test completes. Warnings and all Maven failures
/// remain visible; only successful-framework chatter is suppressed.
pub fn real_maven_cmd(cwd: &Path, path: &str) -> ToolchainCommand {
    let mut cmd = Command::new("mvn");
    cmd.current_dir(cwd);
    cmd.env("PATH", path);
    cmd.env(
        "MAVEN_ARGS",
        "-ntp -Dspring.main.banner-mode=off -Dlogging.level.root=WARN",
    );
    ToolchainCommand::new(cmd)
}

/// A process which may enter Maven, javac, Surefire or Testcontainers.
///
/// Libtest defaults to one worker per CPU. A generated-project test then
/// starts Maven, which starts javac and another JVM, and some of those JVMs
/// start containers. Letting sixteen such trees run at once made each
/// otherwise seven-second build take 40--75 seconds and eventually made a
/// Kafka container exit during startup. Four process trees keep this machine
/// busy without turning parallelism into contention. Pure Rust tests and
/// fake-toolchain commands do not use this wrapper and remain fully parallel.
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
        let _permit = ToolchainPermit::acquire();
        self.inner.status()
    }

    pub fn output(&mut self) -> io::Result<Output> {
        let _permit = ToolchainPermit::acquire();
        self.inner.output()
    }
}

const MAX_TOOLCHAIN_PROCESSES: usize = 4;
static TOOLCHAIN_PROCESSES: (Mutex<usize>, Condvar) = (Mutex::new(0), Condvar::new());

struct ToolchainPermit;

impl ToolchainPermit {
    fn acquire() -> Self {
        let (active, available) = &TOOLCHAIN_PROCESSES;
        let mut count = active
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        while *count >= MAX_TOOLCHAIN_PROCESSES {
            count = available
                .wait(count)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
        *count += 1;
        Self
    }
}

impl Drop for ToolchainPermit {
    fn drop(&mut self) {
        let (active, available) = &TOOLCHAIN_PROCESSES;
        let mut count = active
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *count -= 1;
        available.notify_one();
    }
}

/// A super-simple, hand-written Spring Boot project (pinned versions, JDK
/// 26) -- deliberately not fetched from start.spring.io, so the
/// "does scaffold produce a project that compiles" check never depends on
/// that external service.
pub fn write_spring_fixture(root: &Path) {
    let pkg_dir = root.join("src/main/java/com/example/demo");
    fs::create_dir_all(&pkg_dir).unwrap();
    fs::write(root.join("pom.xml"), SPRING_FIXTURE_POM).unwrap();
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
        <java.version>25</java.version>
    </properties>
    <dependencies>
        <dependency>
            <groupId>org.springframework.boot</groupId>
            <artifactId>spring-boot-starter-webmvc</artifactId>
        </dependency>
        <dependency>
            <groupId>org.springframework.boot</groupId>
            <artifactId>spring-boot-starter-webmvc-test</artifactId>
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
            // and every test that only ever *inspected* this fixture passed
            // while nothing had ever built it. That is plan.md 8.8 in
            // miniature, and `add format` -- which shells out to Maven -- is
            // what finally noticed.
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
