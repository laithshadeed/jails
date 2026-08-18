use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_jails")
}

/// A fresh, isolated scratch directory under the OS temp dir -- real
/// filesystem, but never the actual project checkout, so tests can't step
/// on each other or on this repo.
pub fn temp_dir(label: &str) -> PathBuf {
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

pub fn real_mvn_available() -> bool {
    real_path_dirs().any(|dir| dir.join("mvn").is_file())
}

pub fn real_java_available() -> bool {
    real_path_dirs().any(|dir| dir.join("java").is_file())
        && real_path_dirs().any(|dir| dir.join("javac").is_file())
}

/// Whether the `javac` on PATH understands the release jails generates for
/// (`pom::TARGET_RELEASE`). Presence of a JDK is not enough: a JDK older than
/// the target rejects `--release N` outright, which is the normal state of
/// the world in the months before a new Java GA. Tests that really compile
/// generated code skip on this rather than going red until the toolchain
/// catches up -- see mise.toml for how 27 is provided here.
pub fn real_java_supports_target_release() -> bool {
    Command::new("javac")
        .arg(format!("--release={TARGET_RELEASE}"))
        .arg("-version")
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

/// Kept in step with `pom::TARGET_RELEASE` by
/// `target_release_matches_the_binary` in tests/cli.rs -- the integration
/// tests compile against the binary, not the library, so the constant cannot
/// simply be imported.
pub const TARGET_RELEASE: &str = "27";

fn real_path_dirs() -> impl Iterator<Item = PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .collect::<Vec<_>>()
        .into_iter()
}

/// The real PATH, minus any directory that has an `mvnd` on it. mvnd is
/// preferred by default (see run.rs) and stays real -- nothing here is
/// mocked -- but this machine hits a known mvnd daemon flake (native
/// -library extraction bug against this JDK), so the "does it really
/// compile" tests pin to plain mvn instead of depending on mvnd's daemon
/// being healthy. `uname`/`dirname`/etc. that mvn's own launcher script
/// shells out to need the rest of the real PATH to stay intact.
pub fn real_path_without_mvnd() -> String {
    let filtered: Vec<PathBuf> = real_path_dirs()
        .filter(|dir| !dir.join("mvnd").is_file())
        .collect();
    std::env::join_paths(filtered)
        .unwrap()
        .to_string_lossy()
        .into_owned()
}

pub fn jails_cmd_with_path(cwd: &Path, path: &str) -> Command {
    let mut cmd = Command::new(bin());
    cmd.current_dir(cwd);
    cmd.env("PATH", path);
    cmd
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
        <java.version>26</java.version>
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
