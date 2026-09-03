//! `jails test --fast`: run already-compiled tests without starting the build
//! tool.
//!
//! `mvn test` on one method is almost none of it the test: it is Maven
//! resolving a reactor, reading a pom, running a lifecycle and forking
//! Surefire; `gradle test` configures a project and forks a worker. When the
//! classes are already compiled — the inner loop, where you re-run one test
//! to read its output again — none of that has to happen. JUnit ships a
//! launcher that takes a classpath and a selector, and that is all this is.
//!
//! ## Soundness, which is the whole design
//!
//! **Compiling nothing is unsound**, and running stale classes silently is the
//! worst outcome a test runner can produce: green over code that no longer
//! exists. So this compares the newest `.java` under `src/` against the newest
//! `.class` under the build's output tree, and when a source is newer it says
//! so and hands the run back to the build tool. The rule, in one place:
//! **every fast path falls back loudly.**
//!
//! `jails check` stays `mvn clean verify` and is not touched by any of this.
//!
//! ## Where the classpath comes from, and why the answer is cached
//!
//! Maven answers `dependency:build-classpath`, itself a Maven run, so doing
//! it every time would give back the saving. It is written to
//! `target/jails-test-classpath` and reused while `pom.xml` has not changed
//! since — the pom is the only thing that can alter it, and comparing
//! mtimes is the cheapest question that answers correctly.
//!
//! Gradle has no such goal, and the exact answer — the resolved
//! `testRuntimeClasspath` and each source set's output directories — is the
//! build's own. So jails' marked block registers a task that prints it
//! ([`gradle::classpath_task`]), this module invokes the task and caches what
//! it said under `.jails/run/` against every file that feeds it, and a build
//! without the task is refused by name rather than read for a layout.

use crate::gradle::{self, ClasspathReport};
use crate::project::Project;
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
                "nothing is compiled yet, so there is nothing to run fast".to_string()
            }
            TooStale::SourceIsNewer(path) => format!(
                "{} is newer than the compiled classes, and running the old ones would be \
                 green over code that no longer exists",
                path.display()
            ),
        }
    }

    /// The same fact in a clause, for a reason line that already says the
    /// outputs are stale and needs only to name *which* file made them so.
    ///
    /// A reader who is told the outputs are stale and not which source did it
    /// has to go and find the file themselves, and the one they reach for
    /// first is the one they just edited -- which, after a `resource field
    /// add`, is the model rather than the seven files it rewrote.
    pub fn summary(&self) -> String {
        match self {
            TooStale::NothingCompiled => "nothing is compiled yet".to_string(),
            TooStale::SourceIsNewer(path) => {
                format!("{} is newer than the compiled classes", path.display())
            }
        }
    }
}

/// Where a build writes what it compiles: `target/` under Maven, `build/`
/// under Gradle.
///
/// The staleness question is answered at this grain and no finer: the newest
/// `.class` anywhere under the tree against the newest `.java` under `src/`.
/// The exact output directories come from the build itself
/// ([`OutputLayout`]); a reader who relocated Gradle's `buildDirectory` gets
/// a `NothingCompiled` refusal naming the way out rather than a run over
/// whatever happened to be here.
pub(crate) fn output_root(build: crate::build::Build) -> &'static str {
    match build {
        crate::build::Build::Gradle => "build",
        _ => "target",
    }
}

/// Whether the compiled classes can be trusted for this run.
pub(crate) fn staleness(root: &Path, build: crate::build::Build) -> Option<TooStale> {
    // `?` here would mean "no class files, so nothing is stale", which is
    // exactly backwards: the launcher would then run against an empty
    // classpath and report that nothing failed.
    let Some(newest_class) = newest_with_extension(&root.join(output_root(build)), "class") else {
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

/// The directories a build writes, as the build states them.
///
/// Maven's is fixed -- classes and resources share `target/classes`, tests
/// share `target/test-classes` -- and Gradle's is whatever
/// [`gradle::classpath_task`] printed, which keeps them apart and may hold
/// one classes directory per compiled language. Kept as four lists rather
/// than two paths so `affected` can ask *which* directory holds a class and
/// the daemon can snapshot every one of them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OutputLayout {
    pub main_classes: Vec<PathBuf>,
    pub main_resources: Vec<PathBuf>,
    pub test_classes: Vec<PathBuf>,
    pub test_resources: Vec<PathBuf>,
}

impl OutputLayout {
    fn maven(project: &Project) -> Self {
        let classes = project.root().join("target/classes");
        let tests = project.root().join("target/test-classes");
        Self {
            main_classes: vec![classes.clone()],
            main_resources: vec![classes],
            test_classes: vec![tests.clone()],
            test_resources: vec![tests],
        }
    }

    fn gradle(report: &ClasspathReport) -> Self {
        Self {
            main_classes: report.main_classes.clone(),
            main_resources: report.main_resources.clone(),
            test_classes: report.test_classes.clone(),
            test_resources: report.test_resources.clone(),
        }
    }

    /// Every output directory once, main before test, classes before
    /// resources: the order JUnit's `--class-path` receives them in.
    pub fn all(&self) -> Vec<PathBuf> {
        let mut seen = Vec::new();
        for path in self
            .main_classes
            .iter()
            .chain(&self.main_resources)
            .chain(&self.test_classes)
            .chain(&self.test_resources)
        {
            if !seen.contains(path) {
                seen.push(path.clone());
            }
        }
        seen
    }

    /// The directories a class of this kind compiles into.
    pub fn classes(&self, test: bool) -> &[PathBuf] {
        if test {
            &self.test_classes
        } else {
            &self.main_classes
        }
    }

    /// The directories a resource tree is copied into.
    pub fn resources(&self, test: bool) -> &[PathBuf] {
        if test {
            &self.test_resources
        } else {
            &self.main_resources
        }
    }
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
    /// What a recompile changes: every directory in [`Self::layout`], once.
    pub outputs: Vec<PathBuf>,
    /// The resolved third-party jars: what a build-file change changes.
    pub dependencies: Vec<PathBuf>,
    /// The same outputs, by what each directory holds.
    pub layout: OutputLayout,
}

/// The test classpath, from cache when the build's inputs have not moved.
///
/// `command` is the one the reader typed, for the refusal a Gradle build
/// without jails' task gets.
pub(crate) fn test_classpath(
    project: &Project,
    command: &str,
    debug: bool,
) -> Result<TestClasspath> {
    match project.build() {
        crate::build::Build::Maven => maven_test_classpath(project, debug),
        crate::build::Build::Gradle => {
            let report = gradle_report(project, command, debug)?;
            let layout = OutputLayout::gradle(&report);
            Ok(TestClasspath {
                outputs: layout.all(),
                dependencies: report.test_runtime,
                layout,
            })
        }
        other => {
            crate::build::require_maven(other, command)?;
            Err(format!(
                "`jails {command}` cannot resolve a classpath for a project built by {}\n       \
                 fix: add a Maven `pom.xml` or a Groovy `build.gradle`",
                other.name()
            )
            .into())
        }
    }
}

/// The output directories alone, for a caller that has to know where classes
/// go before it can decide whether to ask for the classpath at all.
///
/// On Maven that is a fixed layout and costs nothing; on Gradle it is the
/// same answer as [`test_classpath`], from the same cache.
pub(crate) fn output_layout(project: &Project, command: &str, debug: bool) -> Result<OutputLayout> {
    match project.build() {
        crate::build::Build::Maven => Ok(OutputLayout::maven(project)),
        _ => Ok(test_classpath(project, command, debug)?.layout),
    }
}

fn maven_test_classpath(project: &Project, debug: bool) -> Result<TestClasspath> {
    let root = project.root();
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
    let layout = OutputLayout::maven(project);
    Ok(TestClasspath {
        outputs: layout.all(),
        dependencies,
        layout,
    })
}

/// The files that can change what Gradle answers, so the cached answer is
/// believed only while none of them has moved.
///
/// Maven's list is one file, the pom; Gradle spreads the same facts over a
/// build script, a settings script, `gradle.properties` and a version
/// catalog, and a cache keyed on the build script alone would serve a stale
/// classpath after a catalog bump. A file that is absent is not an input.
const GRADLE_CLASSPATH_INPUTS: &[&str] = &[
    "build.gradle",
    "build.gradle.kts",
    "settings.gradle",
    "settings.gradle.kts",
    "gradle.properties",
    "gradle/libs.versions.toml",
    "gradle/wrapper/gradle-wrapper.properties",
];

/// Where the last answer from [`gradle::CLASSPATH_TASK`] is kept: under the
/// project's runtime state rather than under `build/`, which Gradle's own
/// `clean` empties and a reader may have relocated.
const GRADLE_CLASSPATH_CACHE: &str = ".jails/run/gradle-classpath";

/// The cache's first line: the canonical root the cached answer was for.
const ROOT_LINE: &str = "root=";

/// Ask the build for its classpaths and output directories, or reuse the
/// last answer while nothing that feeds it has changed.
///
/// **A build without the task is refused, never read for a layout.** The
/// task is registered in the `// jails:dependencies` block the model renders,
/// so the way out is the command that declares a dependency, and the refusal
/// says so. The daemon and the runtime launcher both come through here, which
/// is what keeps one Gradle invocation answering both questions.
pub(crate) fn gradle_report(
    project: &Project,
    command: &str,
    debug: bool,
) -> Result<ClasspathReport> {
    if !gradle::declares_classpath_task(project.build_file()) {
        return Err(gradle::missing_classpath_task(command).into());
    }
    let root = project.root();
    let cache = root.join(GRADLE_CLASSPATH_CACHE);
    // The answer is absolute paths, so it belongs to the directory it was
    // given in: a project moved or copied elsewhere carries a cache whose
    // every input is older than it and whose every path points at the old
    // place. The first line names the root the answer was for.
    let here = format!(
        "{ROOT_LINE}{}",
        root.canonicalize()
            .unwrap_or_else(|_| root.to_path_buf())
            .display()
    );
    let fresh = GRADLE_CLASSPATH_INPUTS
        .iter()
        .all(|input| is_fresh(&cache, &root.join(input)))
        && std::fs::read_to_string(&cache)
            .is_ok_and(|cached| cached.lines().next() == Some(here.as_str()));
    let stdout = if fresh {
        std::fs::read_to_string(&cache)
            .map_err(|error| format!("failed to read {}: {error}", cache.display()))?
    } else {
        let spec = crate::process::CommandSpec::new(run::gradlew::binary(root))
            .args(["-q", gradle::CLASSPATH_TASK])
            .current_dir(root)
            .output(crate::process::OutputMode::Capture);
        let done = crate::process::run(&spec, crate::process::Diagnostics::from_flag(debug))?;
        if !done.status.success() {
            let mut log = done.stdout_string();
            log.push_str(&String::from_utf8_lossy(&done.stderr));
            return Err(format!(
                "Gradle could not answer `{}`:\n{}       fix: run the task through the wrapper \
                 to see the build's own diagnostic",
                gradle::CLASSPATH_TASK,
                indent(&log)
            )
            .into());
        }
        let stdout = format!("{here}\n{}", done.stdout_string());
        // Parsed before it is cached, so a run that printed nothing usable is
        // a refusal now and not a blank answer served on every later command.
        gradle::parse_classpath_report(&stdout)?;
        jails_support::apply::put_runtime_state(root, &cache, stdout.as_bytes())?;
        stdout
    };
    let mut report = gradle::parse_classpath_report(&stdout)?;
    for list in [
        &mut report.main_classes,
        &mut report.main_resources,
        &mut report.test_classes,
        &mut report.test_resources,
        &mut report.runtime,
        &mut report.test_runtime,
    ] {
        // The daemon relativises every output against the canonical project
        // root, so the directories have to be canonical too. One that does not
        // exist yet -- the resources directory of a project with no resources
        // -- is kept as printed: nothing is under it to snapshot.
        for path in list.iter_mut() {
            if let Ok(canonical) = path.canonicalize() {
                *path = canonical;
            }
        }
    }
    Ok(report)
}

fn indent(text: &str) -> String {
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| format!("  {line}\n"))
        .collect()
}

/// Whether a cached classpath answer can still be believed.
///
/// **One owner for every caller** -- the test classpath, the runtime
/// classpath and the Gradle answer -- so the predicate cannot drift into
/// copies.
///
/// **An empty file is not a cached answer.** `dependency:build-classpath`
/// creates its output before it has resolved anything, so a Maven run that
/// dies partway leaves a blank cache with a fresh mtime, and mtime alone reads
/// that as "the classpath is empty". Nothing then re-resolves it: `jails
/// console` and `jails runner` launch with `target/classes` and no
/// dependencies, and fail at the first library class with nothing in the
/// message pointing at `target/`. Re-running the build tool for a genuinely
/// empty classpath costs one round trip; believing one costs a wrong answer
/// that looks like the project's fault. The runtime caller cannot even reach
/// that case -- it returns early when the pom declares no dependency at all.
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
        // No such input to compare against: the cache is all there is, and a
        // foreign build never reaches here anyway.
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
    let (class, method) = crate::testing::split_selector(filter);
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
    use crate::build::Build;
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
        assert!(matches!(
            staleness(&root, Build::Maven),
            Some(TooStale::NothingCompiled)
        ));
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

        match staleness(&root, Build::Maven) {
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

        assert!(staleness(&root, Build::Maven).is_none());
    }

    /// Gradle compiles into `build/`, and a class under `target/` is not
    /// evidence that a Gradle project compiled -- nor the other way round.
    #[test]
    fn staleness_reads_the_output_tree_of_the_build_that_wrote_it() {
        let root = scratch("gradle-tree");
        fs::create_dir_all(root.join("build/classes/java/main")).unwrap();
        fs::create_dir_all(root.join("src/main/java")).unwrap();
        fs::write(root.join("src/main/java/A.java"), "class A {}").unwrap();
        let class = root.join("build/classes/java/main/A.class");
        fs::write(&class, "").unwrap();
        filetime_bump(&class);

        assert!(staleness(&root, Build::Gradle).is_none());
        assert!(matches!(
            staleness(&root, Build::Maven),
            Some(TooStale::NothingCompiled)
        ));
    }

    /// Set an mtime a second into the future, so the comparison cannot depend
    /// on filesystem timestamp granularity.
    fn filetime_bump(path: &Path) {
        let now = SystemTime::now() + std::time::Duration::from_secs(2);
        let file = fs::OpenOptions::new().write(true).open(path).unwrap();
        file.set_modified(now).unwrap();
    }

    /// Maven's four output roles are two directories; the classpath JUnit
    /// receives lists each once, main first.
    #[test]
    fn the_maven_layout_hands_junit_two_directories_once_each() {
        let root = scratch("maven-layout");
        fs::write(root.join("pom.xml"), "<project/>").unwrap();
        let layout = OutputLayout::maven(&Project::inspect(&root).unwrap());
        assert_eq!(
            layout.all(),
            vec![
                root.join("target/classes"),
                root.join("target/test-classes")
            ]
        );
        assert_eq!(layout.classes(true), &[root.join("target/test-classes")]);
        assert_eq!(layout.resources(false), &[root.join("target/classes")]);
    }

    #[test]
    fn a_gradle_answer_keeps_classes_and_resources_apart() {
        let report = gradle::parse_classpath_report(
            "jails.classpath.main-classes=/p/build/classes/java/main\n\
             jails.classpath.main-resources=/p/build/resources/main\n\
             jails.classpath.test-classes=/p/build/classes/java/test\n\
             jails.classpath.test-resources=/p/build/resources/test\n\
             jails.classpath.runtime=/m2/a.jar\n\
             jails.classpath.test-runtime=/m2/a.jar\n\
             jails.classpath.test-runtime=/m2/junit.jar\n",
        )
        .unwrap();
        let layout = OutputLayout::gradle(&report);
        assert_eq!(layout.all().len(), 4);
        assert_eq!(
            layout.classes(false),
            &[PathBuf::from("/p/build/classes/java/main")]
        );
        assert_eq!(
            layout.resources(true),
            &[PathBuf::from("/p/build/resources/test")]
        );
        assert_eq!(report.test_runtime.len(), 2);
    }

    /// A Gradle build that never registered the task is refused with the
    /// command that writes it, and no Gradle is started to find that out.
    #[test]
    fn a_gradle_build_without_the_task_is_refused_by_name() {
        let root = scratch("gradle-no-task");
        fs::write(root.join("build.gradle"), "plugins { id 'java' }\n").unwrap();
        fs::create_dir_all(root.join("src/main/java/com/example")).unwrap();
        fs::write(
            root.join("src/main/java/com/example/App.java"),
            "package com.example;\nclass App {}\n",
        )
        .unwrap();
        let project = Project::inspect(&root).unwrap();
        let error = test_classpath(&project, "test --engine warm", false)
            .err()
            .expect("a build without the task must refuse")
            .to_string();
        assert!(error.contains("jailsClasspath"), "{error}");
        assert!(error.contains("`jails test --fast`"), "{error}");
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
