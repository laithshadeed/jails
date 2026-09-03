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
//! read "not stated here" as *unknown* rather than as *absent*. That is why
//! it refuses to grow into a parser.

use crate::documents::pom;
use jails_contracts::{BuildSystem, Layout, ProjectFacts, Reactor, ReactorModule};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Every fact about a project the build files state, read once.
///
/// **The one place a build file is asked what it says.** `capture` takes
/// these and replaces the two the model is authority for -- the Java release
/// and the base package -- and the layout, which it reads out of the captured
/// `jails.toml` rather than off disk a second time; `Project` takes them as
/// they are, because the commands that resolve one run on projects that may
/// have no model at all. A `jails.toml` that names a layer that does not
/// exist is refused here, as it is everywhere.
pub fn facts(root: &Path) -> Result<ProjectFacts, jails_model::Diagnostic> {
    let layout = match std::fs::read_to_string(root.join("jails.toml")) {
        Ok(source) => Layout::parse(&source).map_err(super::layout_invalid)?,
        Err(_) => Layout::default(),
    };
    Ok(observed(root, layout))
}

/// [`facts`] with the layout already decided by the caller.
pub(super) fn observed(root: &Path, layout: Layout) -> ProjectFacts {
    let build_system = observe_build_system(root);
    let reactor = reactor(root, build_system);
    ProjectFacts {
        build_system,
        // A build that states no release is read as targeting the floor:
        // generated code compiles on every release jails supports, and the
        // model's own release replaces this in every capture.
        java_release: reactor
            .java_release
            .unwrap_or(jails_model::JAVA_RELEASE_FLOOR),
        spring_boot: spring_boot_version(root, build_system),
        base_package: crate::spec::base_package(root).unwrap_or_default(),
        dependencies: declared_dependencies(root, build_system),
        maven_wrapper: root.join("mvnw").is_file(),
        layout,
        junit: junit_version(root, build_system),
        artifact_id: build_artifact_id(root, build_system),
        build_dependencies: build_dependencies(root, build_system),
        reactor,
        main_class: main_class(root, build_system),
    }
}

/// The entry point the build names, in whichever dialect it is written.
fn main_class(root: &Path, build_system: BuildSystem) -> Option<String> {
    let source = std::fs::read_to_string(build_file(root, build_system)?).ok()?;
    match build_system {
        BuildSystem::Maven => pom::main_class(&source).map(str::to_string),
        BuildSystem::Gradle => crate::gradle::main_class(&source),
        BuildSystem::Unknown => None,
    }
}

/// The reactor walk: which aggregator owns this module, what it inherits on
/// the way up, and every module the aggregator declares.
///
/// Paths are canonicalized before they are compared, because `<module>`
/// entries are written relative and a module is inside its reactor only once
/// both are spelled absolutely. The walk stops at the filesystem root, and a
/// pom it cannot read is a pom that aggregates nothing.
fn reactor(root: &Path, build_system: BuildSystem) -> Reactor {
    match build_system {
        BuildSystem::Maven => maven_reactor(root),
        BuildSystem::Gradle => {
            let text = build_file(root, build_system)
                .and_then(|path| std::fs::read_to_string(path).ok())
                .unwrap_or_default();
            Reactor {
                root: String::new(),
                artifact_id: build_artifact_id(root, build_system),
                java_release: crate::gradle::release_level(&text)
                    .and_then(|r| u16::try_from(r).ok()),
                spring_boot: crate::gradle::is_spring_boot(&text),
                modules: Vec::new(),
            }
        }
        BuildSystem::Unknown => Reactor::default(),
    }
}

fn maven_reactor(root: &Path) -> Reactor {
    let module = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let read = |dir: &Path| std::fs::read_to_string(dir.join("pom.xml")).ok();
    // The highest ancestor whose `<modules>` reaches this one.
    let mut reactor = module.clone();
    for ancestor in module.ancestors().skip(1) {
        let Some(pom) = read(ancestor) else {
            continue;
        };
        let aggregates = pom::module_paths(&pom).into_iter().any(|declared| {
            std::fs::canonicalize(ancestor.join(declared))
                .is_ok_and(|declared| module.starts_with(declared))
        });
        if aggregates {
            reactor = ancestor.to_path_buf();
        }
    }
    // What the module inherits: the nearest stated release, and Boot's
    // dependency management from any pom between here and the reactor.
    let mut java_release = None;
    let mut spring_boot = false;
    for dir in module
        .ancestors()
        .take_while(|dir| dir.starts_with(&reactor))
    {
        let Some(pom) = read(dir) else {
            continue;
        };
        if java_release.is_none() {
            java_release = pom::release_level(&pom).and_then(|r| u16::try_from(r).ok());
        }
        spring_boot |= pom::is_spring_boot(&pom);
    }
    let mut modules = Vec::new();
    let mut seen = BTreeSet::new();
    collect_modules(&reactor, &reactor, &mut seen, &mut modules);
    let depth = module
        .strip_prefix(&reactor)
        .map(|relative| relative.components().count())
        .unwrap_or(0);
    Reactor {
        root: vec![".."; depth].join("/"),
        artifact_id: read(&reactor).and_then(|pom| pom::artifact_id(&pom)),
        java_release,
        spring_boot,
        modules,
    }
}

fn collect_modules(
    reactor: &Path,
    aggregator: &Path,
    seen: &mut BTreeSet<PathBuf>,
    modules: &mut Vec<ReactorModule>,
) {
    let Ok(pom) = std::fs::read_to_string(aggregator.join("pom.xml")) else {
        return;
    };
    for declared in pom::module_paths(&pom) {
        let Ok(dir) = std::fs::canonicalize(aggregator.join(&declared)) else {
            continue;
        };
        let Ok(child) = std::fs::read_to_string(dir.join("pom.xml")) else {
            continue;
        };
        if dir == reactor || !seen.insert(dir.clone()) {
            continue;
        }
        modules.push(ReactorModule {
            path: dir
                .strip_prefix(reactor)
                .unwrap_or(&dir)
                .to_string_lossy()
                .replace('\\', "/"),
            artifact_id: pom::artifact_id(&child),
        });
        collect_modules(reactor, &dir, seen, modules);
    }
}

/// Every coordinate the build file declares, jails' own block included.
///
/// [`declared_dependencies`] strips that block because the compiler must not
/// read its own writing back as the reader's; `doctor` asks what is on the
/// classpath, and jails' block is.
fn build_dependencies(root: &Path, build_system: BuildSystem) -> BTreeSet<String> {
    let Some(path) = build_file(root, build_system) else {
        return BTreeSet::new();
    };
    let Ok(source) = std::fs::read_to_string(path) else {
        return BTreeSet::new();
    };
    coordinates(&source, build_system)
}

/// Which build language this module uses, observed from its build files.
///
/// **Public because a second reader of the same two filenames is a second
/// answer.** `jails model upgrade` needs the `build` axis before any plan
/// exists, and JDL v1 §22 says an unsupported build language aborts the
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
        BuildSystem::Maven => pom::parent_spring_boot_version(&source).map(str::to_string),
        BuildSystem::Gradle => gradle_spring_boot_version(&source),
        BuildSystem::Unknown => None,
    }
}

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
    coordinates(&source, build_system)
}

/// Every `group:artifact` a build script's text declares.
fn coordinates(source: &str, build_system: BuildSystem) -> BTreeSet<String> {
    match build_system {
        BuildSystem::Maven => pom::dependency_coordinates(source),
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

/// The identity the build declares for itself, if it declares one.
///
/// Maven states it under the project element and [`pom::artifact_id`] skips
/// the parent's, since a Boot project's first one belongs to
/// `spring-boot-starter-parent`.
/// Gradle states it as `rootProject.name` in `settings.gradle`, and a project
/// with no settings file falls back to nothing rather than to the directory,
/// which is the value this exists to stop using.
pub(super) fn build_artifact_id(root: &Path, build_system: BuildSystem) -> Option<String> {
    match build_system {
        BuildSystem::Maven => {
            pom::artifact_id(&std::fs::read_to_string(root.join("pom.xml")).ok()?)
        }
        BuildSystem::Gradle => {
            for name in ["settings.gradle", "settings.gradle.kts"] {
                let Ok(source) = std::fs::read_to_string(root.join(name)) else {
                    continue;
                };
                for line in source.lines() {
                    let Some((key, value)) = line.split_once('=') else {
                        continue;
                    };
                    if key.trim() != "rootProject.name" {
                        continue;
                    }
                    let value = value.trim().trim_matches(['\'', '"']).trim();
                    if !value.is_empty() {
                        return Some(value.to_string());
                    }
                }
            }
            None
        }
        BuildSystem::Unknown => None,
    }
}

/// Every template this workspace overrides, machine tier first.
///
/// **The later insert wins, which is the documented order**: a project's
/// override beats the machine's, because the file in the repository is the
/// one a colleague can see. Names are relative to the override directory --
/// exactly how the built-ins are named -- so `.jails/templates/generate/
/// command_test.java` replaces `templates/generate/command_test.java`.
///
/// A file that is not UTF-8 is skipped rather than refused: it cannot be a
/// Java template, and refusing here would make an unrelated stray file break
/// every command in the project.
pub(super) fn template_overrides(root: &Path) -> jails_contracts::TemplateOverrides {
    let mut found = jails_contracts::TemplateOverrides::default();
    for base in [machine_templates(), Some(root.join(".jails/templates"))]
        .into_iter()
        .flatten()
    {
        collect_overrides(&base, &base, &mut found);
    }
    found
}

fn machine_templates() -> Option<std::path::PathBuf> {
    let base = match std::env::var_os("XDG_CONFIG_HOME") {
        Some(value) if !value.is_empty() => std::path::PathBuf::from(value),
        _ => std::path::PathBuf::from(std::env::var_os("HOME")?).join(".config"),
    };
    Some(base.join("jails/templates"))
}

fn collect_overrides(base: &Path, dir: &Path, found: &mut jails_contracts::TemplateOverrides) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_overrides(base, &path, found);
            continue;
        }
        let (Ok(text), Ok(relative)) = (std::fs::read_to_string(&path), path.strip_prefix(base))
        else {
            continue;
        };
        found.files.insert(
            relative.to_string_lossy().replace('\\', "/"),
            jails_contracts::TemplateOverride {
                origin: path.to_string_lossy().replace('\\', "/"),
                text,
            },
        );
    }
}

/// The version `junit-platform-console` must be declared at, or `None` when
/// something already manages it.
///
/// **This is not the version the project declares, and pinning that is the
/// bug it exists to stop.** Confirmed in `deps/junit-framework` rather than
/// from memory: `junit-bom` constrains every mavenized artifact to the single
/// root `version` from JUnit 6 on, so jupiter and platform share one number
/// there -- but JUnit 5 paired jupiter `5.y.z` with platform `1.y.z`, and a
/// console pinned at `5.11.4` does not resolve at all. Under a Spring Boot
/// parent or an imported `junit-bom` the version is managed and a redundant
/// pin is the *other* half of the same failure: it holds the launcher still
/// while the BOM moves the engine, which dies at run time with a
/// `NoSuchMethodError` wrapped in "versions not properly aligned".
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
    if source.contains("junit-bom") {
        return None;
    }
    let declared = match build_system {
        BuildSystem::Maven => pom::junit_jupiter_version(&source)?.to_string(),
        // A Gradle project states it as a coordinate on one line.
        BuildSystem::Gradle => source
            .lines()
            .find(|line| line.contains("org.junit.jupiter:junit-jupiter"))
            .and_then(|line| line.rsplit_once(':'))
            .and_then(|(_, version)| quoted(version.trim()))?,
        BuildSystem::Unknown => return None,
    };
    console_version(&declared)
}

/// JUnit's own versioning scheme, as one function.
fn console_version(declared: &str) -> Option<String> {
    let mut parts = declared.split('.');
    let major: u32 = parts.next().and_then(|part| part.parse().ok())?;
    if major >= 6 {
        return Some(declared.to_string());
    }
    let rest: Vec<&str> = parts.collect();
    if rest.is_empty() {
        return None;
    }
    Some(format!("1.{}", rest.join(".")))
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
/// without it a Boot 2 Gradle project looks to the compiler like no Spring
/// project at all -- so `add db` refuses with "requires a captured Spring Boot
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
