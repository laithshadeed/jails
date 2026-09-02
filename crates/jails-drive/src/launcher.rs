//! `jails test --fast`: run already-compiled tests without starting Maven.
//!
//! `mvn test` on one method is almost none of it the test: it is Maven
//! resolving a reactor, reading a pom, running a lifecycle and forking
//! Surefire. When the classes are already
//! compiled — the inner loop, where you re-run one test to read its output
//! again — none of that has to happen. JUnit ships a launcher that takes a
//! classpath and a selector, and that is all this is.
//!
//! ## Soundness, which is the whole design
//!
//! **Compiling nothing is unsound**, and running stale classes silently is the
//! worst outcome a test runner can produce: green over code that no longer
//! exists. So this compares the newest `.java` under `src/` against the newest
//! `.class` under `target/`, and when a source is newer it says so and hands
//! the run back to `mvn`. The rule, in one place: **every fast path falls
//! back loudly.**
//!
//! `jails check` stays `mvn clean verify` and is not touched by any of this.
//!
//! ## Why the classpath is cached and the cache is dated
//!
//! `mvn dependency:build-classpath` is itself a Maven run, so doing it every
//! time would give back the saving. It is written to
//! `target/jails-test-classpath` and reused while `pom.xml` has not changed
//! since — the pom is the only thing that can alter it, and comparing
//! mtimes is the cheapest question that answers correctly.

use crate::run;
use jails_support::Result;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

/// Why the fast path could not be taken. `None` means it can.
pub(crate) enum TooStale {
    /// Nothing has been compiled here yet.
    NothingCompiled,
    /// A source file is newer than the newest class file.
    SourceIsNewer(PathBuf),
}

impl TooStale {
    pub fn explain(&self) -> String {
        match self {
            TooStale::NothingCompiled => {
                "nothing is compiled in target/ yet, so there is nothing to run fast".to_string()
            }
            TooStale::SourceIsNewer(path) => format!(
                "{} is newer than the compiled classes, and running the old ones would be \
                 green over code that no longer exists",
                path.display()
            ),
        }
    }
}

/// Whether the compiled classes can be trusted for this run.
pub(crate) fn staleness(root: &Path) -> Option<TooStale> {
    // `?` here would mean "no class files, so nothing is stale", which is
    // exactly backwards: the launcher would then run against an empty
    // classpath and report that nothing failed.
    let Some(newest_class) = newest_with_extension(&root.join("target"), "class") else {
        return Some(TooStale::NothingCompiled);
    };
    // A source newer than *every* class means the last compile predates the
    // edit. Comparing against the newest class rather than per-file is the
    // blunt answer, and blunt in the safe direction: it can refuse a fast run
    // that would have been fine, never permit one that would not.
    let newest_source = newest_with_extension(&root.join("src"), "java");
    match newest_source {
        Some((path, at)) if at > newest_class.1 => Some(TooStale::SourceIsNewer(path)),
        _ => None,
    }
}

/// Returns `None` when the tree holds no such file at all.
fn newest_with_extension(dir: &Path, extension: &str) -> Option<(PathBuf, SystemTime)> {
    let mut newest: Option<(PathBuf, SystemTime)> = None;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().is_none_or(|ext| ext != extension) {
                continue;
            }
            let Ok(at) = entry.metadata().and_then(|meta| meta.modified()) else {
                continue;
            };
            if newest.as_ref().is_none_or(|(_, best)| at > *best) {
                newest = Some((path, at));
            }
        }
    }
    newest
}

/// A project's test classpath, with its two halves kept apart.
///
/// They are separated because `testd` needs them separated and a single joined
/// string cannot be taken back apart reliably (a dependency path may contain
/// the substring `target/classes`). The daemon holds `dependencies` on its own
/// classpath -- loaded once, stays warm -- and hands `outputs` to JUnit as
/// `--class-path`, which loads them into a fresh child loader per run. Put the
/// outputs in both and the parent-first delegation returns the **stale** class
/// every time, silently: the run is fresh in name only.
pub(crate) struct TestClasspath {
    /// `target/classes` and `target/test-classes`: what a recompile changes.
    pub outputs: Vec<PathBuf>,
    /// The resolved third-party jars: what a pom change changes.
    pub dependencies: Vec<PathBuf>,
}

/// The test classpath, from cache when the pom has not moved since.
pub(crate) fn test_classpath(root: &Path, debug: bool) -> Result<TestClasspath> {
    let cache = root.join("target/jails-test-classpath");
    if !is_fresh(&cache, &root.join("pom.xml")) {
        let mut mvn = Command::new(crate::maven::binary(root));
        mvn.args([
            "-q",
            "dependency:build-classpath",
            &format!("-Dmdep.outputFile={}", cache.display()),
            "-DincludeScope=test",
        ])
        .current_dir(root);
        run::run_inherited(mvn, debug)?;
    }

    let mut dependencies = Vec::new();
    if let Ok(deps) = std::fs::read_to_string(&cache) {
        let deps = deps.trim();
        if !deps.is_empty() {
            dependencies.extend(std::env::split_paths(deps));
        }
    }
    Ok(TestClasspath {
        outputs: vec![
            root.join("target/classes"),
            root.join("target/test-classes"),
        ],
        dependencies,
    })
}

/// Whether a cached `dependency:build-classpath` answer can still be believed.
///
/// **One owner for both callers** -- this one and the runtime classpath's --
/// so the predicate cannot drift into two copies.
///
/// **An empty file is not a cached answer.** `dependency:build-classpath`
/// creates its output before it has resolved anything, so a Maven run that
/// dies partway leaves a blank cache with a fresh mtime, and mtime alone reads
/// that as "the classpath is empty". Nothing then re-resolves it: `jails
/// console` and `jails runner` launch with `target/classes` and no
/// dependencies, and fail at the first library class with nothing in the
/// message pointing at `target/`. Re-running Maven for a genuinely empty
/// classpath costs one round trip; believing one costs a wrong answer that
/// looks like the project's fault. The runtime caller cannot even reach that
/// case -- it returns early when the pom declares no dependency at all.
pub(crate) fn is_fresh(cache: &Path, source: &Path) -> bool {
    match std::fs::read_to_string(cache) {
        Ok(text) if text.trim().is_empty() => return false,
        Ok(_) => {}
        Err(_) => return false,
    }
    let Ok(cached) = std::fs::metadata(cache).and_then(|meta| meta.modified()) else {
        return false;
    };
    match std::fs::metadata(source).and_then(|meta| meta.modified()) {
        Ok(changed) => cached >= changed,
        // No pom to compare against: the cache is all there is, and a foreign
        // build never reaches here anyway.
        Err(_) => true,
    }
}

/// `NoteTest` -> `com.example.demo.domain.NoteTest`.
///
/// The launcher selects by fully qualified name; jails' filters are bare class
/// names, because that is what a person types and what Surefire accepts. The
/// package is read off the file rather than guessed, for the same reason
/// `base_package` reads it rather than being configured.
pub(crate) fn fully_qualified(root: &Path, filter: &str) -> Option<String> {
    let (class, method) = match filter.split_once('#') {
        Some((class, method)) => (class, Some(method)),
        None => (filter, None),
    };
    if class.contains('.') {
        return Some(filter.to_string());
    }
    let file = format!("{class}.java");
    let mut stack = vec![root.join("src/test/java"), root.join("src/main/java")];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.file_name().is_some_and(|name| name == file.as_str()) {
                let source = std::fs::read_to_string(&path).ok()?;
                let package = source
                    .lines()
                    .find_map(|line| line.trim().strip_prefix("package "))?
                    .trim_end_matches(';')
                    .trim();
                let qualified = format!("{package}.{class}");
                return Some(match method {
                    Some(method) => format!("{qualified}#{method}"),
                    None => qualified,
                });
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch(tag: &str) -> PathBuf {
        jails_support::scratch::ScratchDir::in_temp(&format!("jails-fast-{tag}"))
            .unwrap()
            .keep()
    }

    /// The bug this pins: "no class files" must not read as "nothing is
    /// stale". The launcher would then run against an empty classpath and
    /// report that nothing failed.
    #[test]
    fn a_project_with_nothing_compiled_is_refused_rather_than_run() {
        let root = scratch("empty");
        fs::create_dir_all(root.join("src/main/java")).unwrap();
        fs::write(root.join("src/main/java/A.java"), "class A {}").unwrap();
        assert!(matches!(staleness(&root), Some(TooStale::NothingCompiled)));
    }

    /// The one that matters: green over code that no longer exists.
    #[test]
    fn a_source_newer_than_the_classes_refuses_the_fast_path() {
        let root = scratch("stale");
        fs::create_dir_all(root.join("target/classes")).unwrap();
        fs::create_dir_all(root.join("src/main/java")).unwrap();
        fs::write(root.join("target/classes/A.class"), "").unwrap();
        // Written second, so its mtime is at or after the class file's. Some
        // filesystems have coarse timestamps, so nudge it explicitly.
        let source = root.join("src/main/java/A.java");
        fs::write(&source, "class A {}").unwrap();
        filetime_bump(&source);

        match staleness(&root) {
            Some(TooStale::SourceIsNewer(path)) => assert_eq!(path, source),
            other => panic!("expected a staleness refusal, got {:?}", other.is_none()),
        }
    }

    #[test]
    fn classes_newer_than_every_source_take_the_fast_path() {
        let root = scratch("fresh");
        fs::create_dir_all(root.join("target/classes")).unwrap();
        fs::create_dir_all(root.join("src/main/java")).unwrap();
        fs::write(root.join("src/main/java/A.java"), "class A {}").unwrap();
        let class = root.join("target/classes/A.class");
        fs::write(&class, "").unwrap();
        filetime_bump(&class);

        assert!(staleness(&root).is_none());
    }

    /// Set an mtime a second into the future, so the comparison cannot depend
    /// on filesystem timestamp granularity.
    fn filetime_bump(path: &Path) {
        let now = SystemTime::now() + std::time::Duration::from_secs(2);
        let file = fs::OpenOptions::new().write(true).open(path).unwrap();
        file.set_modified(now).unwrap();
    }

    #[test]
    fn a_bare_class_name_is_qualified_from_the_file_that_declares_it() {
        let root = scratch("fqn");
        let dir = root.join("src/test/java/com/example/demo/domain");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("NoteTest.java"),
            "package com.example.demo.domain;\n\nclass NoteTest {}\n",
        )
        .unwrap();

        assert_eq!(
            fully_qualified(&root, "NoteTest").as_deref(),
            Some("com.example.demo.domain.NoteTest")
        );
        assert_eq!(
            fully_qualified(&root, "NoteTest#renders").as_deref(),
            Some("com.example.demo.domain.NoteTest#renders")
        );
        // Already qualified: left alone rather than searched for.
        assert_eq!(
            fully_qualified(&root, "com.other.Thing").as_deref(),
            Some("com.other.Thing")
        );
        assert_eq!(fully_qualified(&root, "Missing"), None);
    }
}
