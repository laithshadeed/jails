//! Content-addressed Maven/Gradle runtime classpath resolution.
//!
//! `command` on every entry point is what the reader typed -- `run`,
//! `console`, `runner` -- so a Gradle build that cannot answer for its
//! classpath is refused in that command's name (`launcher::gradle_report`).

use super::super::RunCompile;
use crate::project::Project;
use jails_support::Result;
use jails_support::{domain_hash, hex};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const CACHE: &str = "runtime-classpath-v2";

pub(super) struct Resolved {
    pub entries: Vec<PathBuf>,
    pub main_class: String,
}

pub(super) fn refresh(project: &Project, command: &str, debug: bool) -> Result<()> {
    resolve(project, RunCompile::Build, command, debug).map(|_| ())
}

pub(super) fn output_id(project: &Project, command: &str, debug: bool) -> String {
    let mut paths = outputs(project, command, debug)
        .into_iter()
        .flatten()
        .flat_map(|output| walk(&output))
        .collect::<Vec<_>>();
    paths.sort();
    let mut bytes = Vec::new();
    for path in paths {
        bytes.extend_from_slice(path.to_string_lossy().as_bytes());
        bytes.push(0);
        if let Ok(content) = fs::read(path) {
            bytes.extend_from_slice(&domain_hash("JAILS-RUNTIME-OUTPUT-1", &content));
        }
    }
    hex(&domain_hash("JAILS-RUNTIME-OUTPUT-SET-1", &bytes))
}

pub(super) fn resolve(
    project: &Project,
    compile: RunCompile,
    command: &str,
    debug: bool,
) -> Result<Resolved> {
    Ok(Resolved {
        entries: resolve_entries(project, compile, command, debug)?,
        main_class: main_class(project)?,
    })
}

pub(super) fn resolve_entries(
    project: &Project,
    compile: RunCompile,
    command: &str,
    debug: bool,
) -> Result<Vec<PathBuf>> {
    let outputs = outputs(project, command, debug)?;
    let prior_cache = read_cache(project);
    let cache_mismatch = prior_cache
        .as_ref()
        .is_some_and(|cache| !cache.current(project, &outputs));
    let cached = prior_cache.filter(|cache| cache.current(project, &outputs));
    if debug {
        eprintln!(
            "jails: runtime cache current={}, mismatch={}, outputs_current={}",
            cached.is_some(),
            cache_mismatch,
            outputs_current(project, &outputs)
        );
    }
    let must_compile = match compile {
        RunCompile::Ide => {
            return Err(
                "`--compile ide` requires a negotiated editor output epoch\n       fix: connect the editor session, or use `--compile auto`"
                    .into(),
            );
        }
        RunCompile::Build => true,
        RunCompile::Auto => {
            cache_mismatch || (cached.is_none() && !outputs_current(project, &outputs))
        }
        RunCompile::None => {
            if cache_mismatch || (cached.is_none() && !outputs_current(project, &outputs)) {
                return Err(
                    "application classes are stale and `--compile none` forbids repair\n       fix: run `jails build` or `jails run --compile build` once, or choose `--compile auto`"
                        .into(),
                );
            }
            false
        }
    };
    if must_compile {
        compile_outputs(project, debug)?;
    }
    let dependencies = match read_cache(project).filter(|cache| cache.current(project, &outputs)) {
        Some(cache) if !must_compile => cache.entries,
        _ => resolve_dependencies(project, command, debug)?,
    };
    let entries = canonical_classpath(outputs, dependencies)?;
    write_cache(project, command, debug, &entries)?;
    Ok(entries)
}

fn compile_outputs(project: &Project, debug: bool) -> Result<()> {
    match project.build() {
        crate::build::Build::Maven => {
            let mut command = Command::new(crate::maven::binary(project.root()));
            command.arg("compile").current_dir(project.root());
            super::super::run_inherited(command, debug)
        }
        crate::build::Build::Gradle => {
            super::super::gradlew::tasks(project.root(), &["classes"], debug)
        }
        other => Err(format!(
            "jails cannot compile a {} application\n       fix: add a supported Maven or Gradle build",
            other.name()
        )
        .into()),
    }
}

fn resolve_dependencies(project: &Project, command: &str, debug: bool) -> Result<Vec<PathBuf>> {
    match project.build() {
        crate::build::Build::Maven => {
            let pom = fs::read_to_string(project.root().join("pom.xml")).unwrap_or_default();
            if !pom.contains("<dependency>") {
                return Ok(Vec::new());
            }
            let target = project.root().join("target/jails-runtime-classpath");
            // **Reused while `pom.xml` has not moved, for the reason
            // `launcher.rs` gives about the test classpath:
            // `dependency:build-classpath` is itself a Maven run.** Resolving
            // it unconditionally would make every `jails runner` and `jails
            // console` pay a full Maven round trip before doing any work.
            //
            // The pom is the only thing that can change the answer, so
            // comparing its mtime against the cache's is the cheapest question
            // that answers correctly. A missing pom leaves the cache
            // authoritative; a foreign build never reaches here.
            if !crate::launcher::is_fresh(&target, &project.root().join("pom.xml")) {
                let mut command = Command::new(crate::maven::binary(project.root()));
                command
                    .arg("-q")
                    .arg("dependency:build-classpath")
                    .arg(format!("-Dmdep.outputFile={}", target.display()))
                    .arg("-DincludeScope=runtime")
                    .current_dir(project.root());
                super::super::run_inherited(command, debug)?;
            }
            let text = fs::read_to_string(&target)
                .map_err(|error| format!("failed to read Maven runtime classpath: {error}"))?;
            Ok(std::env::split_paths(text.trim()).collect())
        }
        // The same answer the warm test engine reads, from the same cached
        // Gradle invocation: `configurations.runtimeClasspath`, resolved by
        // the build itself.
        crate::build::Build::Gradle => {
            Ok(crate::launcher::gradle_report(project, command, debug)?.runtime)
        }
        other => Err(format!(
            "jails cannot resolve a {} runtime classpath\n       fix: use a supported Maven or Gradle build",
            other.name()
        )
        .into()),
    }
}

/// The main output directories: one under Maven, and under Gradle the
/// classes directories followed by the resources directory, as the build
/// stated them rather than as the convention would have them.
fn outputs(project: &Project, command: &str, debug: bool) -> Result<Vec<PathBuf>> {
    match project.build() {
        crate::build::Build::Maven => Ok(vec![project.root().join("target/classes")]),
        crate::build::Build::Gradle => {
            let report = crate::launcher::gradle_report(project, command, debug)?;
            let mut outputs = report.main_classes;
            outputs.extend(report.main_resources);
            Ok(outputs)
        }
        other => Err(format!(
            "jails cannot locate {} application output\n       fix: use a supported Maven or Gradle build",
            other.name()
        )
        .into()),
    }
}

fn outputs_current(project: &Project, outputs: &[PathBuf]) -> bool {
    // Gradle lists the classes directories first and the resources
    // directory last (`outputs`); a single-language project has exactly two.
    let (Some(classes), Some(resources)) = (outputs.first(), outputs.last()) else {
        return false;
    };
    if !matches!(
        project.build(),
        crate::build::Build::Maven | crate::build::Build::Gradle
    ) {
        return false;
    }
    tree_current(
        &project.root().join("src/main/java"),
        classes,
        Some("java"),
        Some("class"),
    ) && tree_current(
        &project.root().join("src/main/resources"),
        resources,
        None,
        None,
    ) && build_inputs(project)
        .into_iter()
        .all(|input| newer_than(&input, classes))
}

fn tree_current(
    source: &Path,
    output: &Path,
    source_extension: Option<&str>,
    output_extension: Option<&str>,
) -> bool {
    let mut stack = vec![source.to_path_buf()];
    while let Some(directory) = stack.pop() {
        let Ok(entries) = fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if source_extension
                .is_some_and(|extension| path.extension().is_none_or(|found| found != extension))
            {
                continue;
            }
            let Ok(relative) = path.strip_prefix(source) else {
                return false;
            };
            let mut compiled = output.join(relative);
            if let Some(extension) = output_extension {
                compiled.set_extension(extension);
            }
            if !compiled.is_file() || !newer_than(&path, &compiled) {
                return false;
            }
        }
    }
    output.is_dir()
}

fn newer_than(source: &Path, output: &Path) -> bool {
    let Ok(source_time) = fs::metadata(source).and_then(|metadata| metadata.modified()) else {
        return false;
    };
    let output_time = if output.is_dir() {
        newest_file(output)
    } else {
        fs::metadata(output)
            .and_then(|metadata| metadata.modified())
            .ok()
    };
    output_time.is_some_and(|time| time >= source_time)
}

fn newest_file(directory: &Path) -> Option<std::time::SystemTime> {
    let mut newest = None;
    let mut stack = vec![directory.to_path_buf()];
    while let Some(current) = stack.pop() {
        for entry in fs::read_dir(current).ok()?.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if let Ok(time) = entry.metadata().and_then(|metadata| metadata.modified()) {
                newest = Some(newest.map_or(time, |prior| std::cmp::max(prior, time)));
            }
        }
    }
    newest
}

fn canonical_classpath(outputs: Vec<PathBuf>, dependencies: Vec<PathBuf>) -> Result<Vec<PathBuf>> {
    let mut seen = BTreeSet::new();
    let mut paths = Vec::new();
    for path in outputs.into_iter().chain(dependencies) {
        if !path.exists() {
            continue;
        }
        let canonical = path.canonicalize().map_err(|error| {
            format!(
                "failed to canonicalize runtime classpath {}: {error}",
                path.display()
            )
        })?;
        if seen.insert(canonical.clone()) {
            paths.push(canonical);
        }
    }
    Ok(paths)
}

fn main_class(project: &Project) -> Result<String> {
    if let Some(main) = &project.facts().main_class {
        return Ok(main.clone());
    }
    let (package, class) = super::super::find_main_class(project.root())?;
    Ok(if package.is_empty() {
        class
    } else {
        format!("{package}.{class}")
    })
}

#[derive(Debug)]
struct Cache {
    snapshot: String,
    entries: Vec<PathBuf>,
}

impl Cache {
    fn current(&self, project: &Project, outputs: &[PathBuf]) -> bool {
        self.entries.iter().all(|entry| entry.exists())
            && self.snapshot == snapshot(project, outputs, &self.entries)
    }
}

fn read_cache(project: &Project) -> Option<Cache> {
    let text = fs::read_to_string(project.root().join(".jails/run").join(CACHE)).ok()?;
    let mut lines = text.lines();
    if lines.next()? != "version=2" {
        return None;
    }
    if lines.next()? != format!("root={}", root_id(project)) {
        return None;
    }
    let snapshot = lines.next()?.strip_prefix("snapshot=")?.to_string();
    let entries = lines
        .map(|line| line.strip_prefix("entry=").map(PathBuf::from))
        .collect::<Option<Vec<_>>>()?;
    Some(Cache { snapshot, entries })
}

fn write_cache(project: &Project, command: &str, debug: bool, entries: &[PathBuf]) -> Result<()> {
    let outputs = outputs(project, command, debug)?;
    let mut text = format!(
        "version=2\nroot={}\nsnapshot={}\n",
        root_id(project),
        snapshot(project, &outputs, entries)
    );
    for entry in entries {
        let value = entry.to_string_lossy();
        if value.contains(['\n', '\r']) {
            return Err("runtime classpath contains a newline\n       fix: move the dependency cache to ordinary paths".into());
        }
        text.push_str("entry=");
        text.push_str(&value);
        text.push('\n');
    }
    let path = project.root().join(".jails/run").join(CACHE);
    jails_support::apply::put_runtime_state(project.root(), &path, text.as_bytes())
}

fn root_id(project: &Project) -> String {
    let canonical = project
        .root()
        .canonicalize()
        .unwrap_or_else(|_| project.root().to_path_buf());
    hex(&domain_hash(
        "JAILS-RUNTIME-CLASSPATH-ROOT-1",
        canonical.to_string_lossy().as_bytes(),
    ))
}

fn snapshot(project: &Project, outputs: &[PathBuf], entries: &[PathBuf]) -> String {
    let mut paths = build_inputs(project);
    paths.extend(walk(&project.root().join("src/main")));
    for output in outputs {
        paths.extend(walk(output));
    }
    paths.sort();
    paths.dedup();
    let mut bytes = Vec::new();
    for path in paths {
        bytes.extend_from_slice(b"project\0");
        bytes.extend_from_slice(
            path.strip_prefix(project.root())
                .unwrap_or(&path)
                .to_string_lossy()
                .as_bytes(),
        );
        bytes.push(0);
        match fs::read(&path) {
            Ok(content) => {
                bytes.extend_from_slice(&(content.len() as u64).to_be_bytes());
                bytes.extend_from_slice(&domain_hash("JAILS-RUNTIME-FILE-1", &content));
            }
            Err(_) => bytes.extend_from_slice(&u64::MAX.to_be_bytes()),
        }
    }
    for (index, path) in entries.iter().filter(|path| path.is_file()).enumerate() {
        bytes.extend_from_slice(b"classpath\0");
        bytes.extend_from_slice(&(index as u64).to_be_bytes());
        bytes.extend_from_slice(
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .as_bytes(),
        );
        bytes.push(0);
        match fs::read(path) {
            Ok(content) => {
                bytes.extend_from_slice(&(content.len() as u64).to_be_bytes());
                bytes.extend_from_slice(&domain_hash("JAILS-RUNTIME-DEPENDENCY-2", &content));
            }
            Err(_) => bytes.extend_from_slice(&u64::MAX.to_be_bytes()),
        }
    }
    bytes.extend_from_slice(b"release\0");
    bytes.extend_from_slice(&project.java_release().unwrap_or_default().to_be_bytes());
    hex(&domain_hash("JAILS-RUNTIME-SNAPSHOT-2", &bytes))
}

fn build_inputs(project: &Project) -> Vec<PathBuf> {
    [
        "pom.xml",
        "build.gradle",
        "build.gradle.kts",
        "settings.gradle",
        "settings.gradle.kts",
        "gradle.properties",
        "gradle/libs.versions.toml",
        "gradle/wrapper/gradle-wrapper.properties",
        ".mvn/maven.config",
        ".mvn/wrapper/maven-wrapper.properties",
    ]
    .into_iter()
    .map(|path| project.root().join(path))
    .filter(|path| path.is_file())
    .collect()
}

fn walk(directory: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut stack = vec![directory.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = fs::read_dir(current) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.is_file() {
                files.push(path);
            }
        }
    }
    files
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_snapshot_invalidates_when_source_bytes_change() {
        let scratch =
            jails_support::scratch::ScratchDir::in_temp("runtime-classpath-cache").unwrap();
        let root = scratch.path();
        fs::write(root.join("pom.xml"), "<project/>").unwrap();
        let source = root.join("src/main/java/com/example/App.java");
        let class = root.join("target/classes/com/example/App.class");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::create_dir_all(class.parent().unwrap()).unwrap();
        fs::write(&source, "class App {}\n").unwrap();
        fs::write(&class, "compiled").unwrap();
        let project = Project::inspect(root).unwrap();
        let entries =
            canonical_classpath(outputs(&project, "run", false).unwrap(), vec![]).unwrap();
        write_cache(&project, "run", false, &entries).unwrap();
        assert!(
            read_cache(&project)
                .unwrap()
                .current(&project, &outputs(&project, "run", false).unwrap())
        );
        fs::write(&source, "class App { int changed; }\n").unwrap();
        assert!(
            !read_cache(&project)
                .unwrap()
                .current(&project, &outputs(&project, "run", false).unwrap())
        );
    }

    #[test]
    fn semantic_snapshot_does_not_depend_on_checkout_or_dependency_cache_prefixes() {
        let scratch = jails_support::scratch::ScratchDir::in_temp("runtime-host-prefix").unwrap();
        let mut snapshots = Vec::new();
        for name in ["first", "second"] {
            let root = scratch.path().join(name).join("project");
            let source = root.join("src/main/java/com/example/App.java");
            let class = root.join("target/classes/com/example/App.class");
            let dependency = scratch
                .path()
                .join(name)
                .join("cache/repository/example-1.0.jar");
            fs::create_dir_all(source.parent().unwrap()).unwrap();
            fs::create_dir_all(class.parent().unwrap()).unwrap();
            fs::create_dir_all(dependency.parent().unwrap()).unwrap();
            fs::write(root.join("pom.xml"), "<project/>").unwrap();
            fs::write(&source, "class App {}\n").unwrap();
            fs::write(&class, "compiled").unwrap();
            fs::write(&dependency, "same artifact bytes").unwrap();
            let project = Project::inspect(&root).unwrap();
            snapshots.push(snapshot(
                &project,
                &outputs(&project, "run", false).unwrap(),
                &[
                    class.parent().unwrap().parent().unwrap().to_path_buf(),
                    dependency,
                ],
            ));
        }
        assert_eq!(snapshots[0], snapshots[1]);
    }
}
