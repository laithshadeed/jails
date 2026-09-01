//! Every fact this project's build file states, read once.
//!
//! **Split out by the secret the whole module shares: it parses a build file,
//! and nothing else here does.** The capture beside it walks trees and records
//! preconditions; this answers a fixed list of questions -- which build tool,
//! which Spring Boot, which JUnit, which dependencies -- by looking at one
//! document.
//!
//! **It is recognition, not understanding.** jails never resolves a build, and
//! reading one artifact name out of a script is not resolving it: an
//! unparseable file yields nothing rather than a guess, and every caller must
//! read "not stated here" as *unknown* rather than as *absent*. That is the
//! same bar `jails-spec::build` clears for the question of which build tool a
//! directory uses, and the reason both refuse to grow into parsers.

use jails_contracts::BuildSystem;
use std::collections::BTreeSet;
use std::path::Path;

/// Which build language this module uses, observed from its build files.
///
/// **Public because a second reader of the same two filenames is a second
/// answer.** `jails model upgrade` needs the `build` axis before any plan
/// exists, and `jdl-sol.md` §22 says an unsupported build language aborts the
/// upgrade rather than being guessed -- so it needs exactly this function's
/// `Unknown`, not a fresh pair of `is_file` calls that could disagree with the
/// snapshot the very next command captures.
pub fn observe_build_system(root: &Path) -> BuildSystem {
    match (
        root.join("pom.xml").is_file(),
        root.join("build.gradle").is_file() || root.join("build.gradle.kts").is_file(),
    ) {
        (true, false) => BuildSystem::Maven,
        (false, true) => BuildSystem::Gradle,
        _ => BuildSystem::Unknown,
    }
}

/// This module's Spring Boot version, or `None` for a plain project.
///
/// Public for the same reason [`observe_build_system`] is: the `platform` axis
/// an upgrade materializes has to be the axis the next capture will observe.
pub fn observe_spring_boot(root: &Path, build_system: BuildSystem) -> Option<String> {
    spring_boot_version(root, build_system)
}

pub(super) fn spring_boot_version(root: &Path, build_system: BuildSystem) -> Option<String> {
    let path = match build_system {
        BuildSystem::Maven => root.join("pom.xml"),
        BuildSystem::Gradle if root.join("build.gradle.kts").is_file() => {
            root.join("build.gradle.kts")
        }
        BuildSystem::Gradle => root.join("build.gradle"),
        BuildSystem::Unknown => return None,
    };
    let source = std::fs::read_to_string(path).ok()?;
    match build_system {
        BuildSystem::Maven => {
            let parent = between(&source, "<parent>", "</parent>")?;
            if !parent.contains("<groupId>org.springframework.boot</groupId>")
                || !parent.contains("<artifactId>spring-boot-starter-parent</artifactId>")
            {
                return None;
            }
            between(parent, "<version>", "</version>")
                .map(str::trim)
                .map(str::to_string)
        }
        BuildSystem::Gradle => gradle_spring_boot_version(&source),
        BuildSystem::Unknown => None,
    }
}

/// The JUnit version this project declares, read off its build.
///
/// **One number, and it is the project's rather than jails'.** `test --fast`
/// runs the console launcher against the already-compiled classes, and the
/// launcher has to be the same JUnit as the tests: JUnit 6's BOM constrains
/// every artifact to one version, while JUnit 5 paired jupiter `5.y.z` with
/// platform `1.y.z`. Under a Spring Boot parent the version is managed and
/// nothing here needs to answer; on a plain build this is what makes the
/// capability declarable instead of refused.
/// Every `group:artifact` this project's build file declares.
///
/// **Observed once, here, because the compiler may not read a build file.**
/// What it is for is the pair of questions the model cannot answer on its own:
/// whether an artifact the compiler would otherwise splice is already the
/// reader's, and whether a type the generated Java names -- `JdbcClient`, from
/// `spring-jdbc` -- is on the classpath at all. A project that declares
/// `spring-boot-starter-data-jdbc` and no `db` capability still has JDBC, and
/// a generator that decides otherwise emits an in-memory bean into a project
/// with a database.
///
/// Coordinates only: no versions, no scopes, no resolution. jails does not
/// understand a build, and reading one artifact name out of it is not
/// understanding one -- which is why an unparseable build yields an empty set
/// rather than a guess, and every caller must treat "not declared here" as
/// "unknown" rather than as "absent".
pub(super) fn declared_dependencies(root: &Path, build_system: BuildSystem) -> BTreeSet<String> {
    let Some(path) = build_file(root, build_system) else {
        return BTreeSet::new();
    };
    let Ok(source) = std::fs::read_to_string(path) else {
        return BTreeSet::new();
    };
    // **jails' own block is not the reader's**, and reading it back as though
    // it were would make the compiler drop every dependency it had just
    // written: the next compile would see them declared already, decline to
    // declare them, and empty the block it owns.
    let marker = crate::documents::DEPENDENCY_MARKER;
    let (open, close) = match build_system {
        BuildSystem::Gradle => (format!("// {marker}"), format!("// /{marker}")),
        _ => (format!("<!-- {marker} -->"), format!("<!-- /{marker} -->")),
    };
    let source = match (source.find(&open), source.find(&close)) {
        (Some(start), Some(end)) if end > start => {
            format!("{}{}", &source[..start], &source[end + close.len()..])
        }
        _ => source,
    };
    match build_system {
        BuildSystem::Maven => {
            let mut found = BTreeSet::new();
            let mut rest = source.as_str();
            while let Some(at) = rest.find("<dependency>") {
                let Some(block) = between(&rest[at..], "<dependency>", "</dependency>") else {
                    break;
                };
                if let (Some(group), Some(artifact)) = (
                    between(block, "<groupId>", "</groupId>"),
                    between(block, "<artifactId>", "</artifactId>"),
                ) {
                    found.insert(format!("{}:{}", group.trim(), artifact.trim()));
                }
                rest = &rest[at + "<dependency>".len()..];
            }
            found
        }
        // A Gradle script states each one as a quoted coordinate on its own
        // line, with or without a version.
        BuildSystem::Gradle => source
            .lines()
            // The coordinate is the quoted run, whatever configuration name
            // precedes it: `implementation '...'`, `testImplementation("...")`.
            .filter_map(|line| {
                let at = line.find(['\'', '"'])?;
                quoted(&line[at..])
            })
            .filter_map(|coordinate| {
                let mut parts = coordinate.split(':');
                let group = parts.next()?;
                let artifact = parts.next()?;
                (!group.is_empty() && !artifact.is_empty()).then(|| format!("{group}:{artifact}"))
            })
            .collect(),
        BuildSystem::Unknown => BTreeSet::new(),
    }
}

/// This project's build script, whichever dialect it is written in.
fn build_file(root: &Path, build_system: BuildSystem) -> Option<std::path::PathBuf> {
    match build_system {
        BuildSystem::Maven => Some(root.join("pom.xml")),
        BuildSystem::Gradle if root.join("build.gradle.kts").is_file() => {
            Some(root.join("build.gradle.kts"))
        }
        BuildSystem::Gradle => Some(root.join("build.gradle")),
        BuildSystem::Unknown => None,
    }
}

pub(super) fn junit_version(root: &Path, build_system: BuildSystem) -> Option<String> {
    let path = match build_system {
        BuildSystem::Maven => root.join("pom.xml"),
        BuildSystem::Gradle if root.join("build.gradle.kts").is_file() => {
            root.join("build.gradle.kts")
        }
        BuildSystem::Gradle => root.join("build.gradle"),
        BuildSystem::Unknown => return None,
    };
    let source = std::fs::read_to_string(path).ok()?;
    match build_system {
        BuildSystem::Maven => {
            let mut rest = source.as_str();
            while let Some(at) = rest.find("<artifactId>junit-") {
                let block_start = rest[..at].rfind("<dependency>")?;
                let block = between(&rest[block_start..], "<dependency>", "</dependency>")?;
                if let Some(version) = between(block, "<version>", "</version>") {
                    return Some(version.trim().to_string());
                }
                rest = &rest[at + 1..];
            }
            None
        }
        // A Gradle project states it as a coordinate on one line.
        BuildSystem::Gradle => source
            .lines()
            .find(|line| line.contains("org.junit.jupiter:junit-jupiter"))
            .and_then(|line| line.rsplit_once(':'))
            .and_then(|(_, version)| quoted(version.trim())),
        BuildSystem::Unknown => None,
    }
}

fn gradle_spring_boot_version(source: &str) -> Option<String> {
    for marker in [
        "id(\"org.springframework.boot\")",
        "id 'org.springframework.boot'",
    ] {
        let Some(start) = source.find(marker) else {
            continue;
        };
        let declaration = source[start..].lines().next()?;
        let version = declaration.split_once("version")?.1.trim();
        return quoted(version);
    }
    legacy_gradle_spring_boot_version(source)
}

/// The pre-`plugins {}` spelling, which is what a Boot 2 project has.
///
/// `buildscript { dependencies { classpath
/// "org.springframework.boot:spring-boot-gradle-plugin:2.7.18" } }` plus
/// `apply plugin: 'org.springframework.boot'`. **Both halves are required**,
/// because the classpath entry alone says the plugin is resolvable and not
/// that it is applied -- and this is the value that decides whether the
/// compiler renders Spring adapters at all.
///
/// Reading it is not guessing: the coordinate states the version exactly, and
/// without it a Boot 2 Gradle project looked to the compiler like no Spring
/// project at all -- so `add db` refused with "requires a captured Spring Boot
/// project" instead of naming the module the version is missing.
fn legacy_gradle_spring_boot_version(source: &str) -> Option<String> {
    const COORDINATE: &str = "org.springframework.boot:spring-boot-gradle-plugin:";
    if !source.contains("apply plugin: 'org.springframework.boot'")
        && !source.contains("apply plugin: \"org.springframework.boot\"")
    {
        return None;
    }
    let start = source.find(COORDINATE)? + COORDINATE.len();
    let version: String = source[start..]
        .chars()
        .take_while(|character| character.is_ascii_digit() || *character == '.')
        .collect();
    (!version.is_empty()).then_some(version)
}

fn between<'a>(source: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let source = source.split_once(start)?.1;
    source.split_once(end).map(|(value, _)| value)
}

fn quoted(source: &str) -> Option<String> {
    let quote = source.chars().next()?;
    if !matches!(quote, '\'' | '"') {
        return None;
    }
    let value = &source[quote.len_utf8()..];
    value
        .split_once(quote)
        .map(|(version, _)| version.to_string())
}
