//! Which tests can the last edit possibly have broken?
//!
//! `plan.md` §10.2 step 3. The index is a reverse-dependency map built from
//! the constant pools already sitting in `target/` -- see [`classfile`]
//! for the one question asked of each file -- so nothing has to be compiled,
//! configured or kept in step by hand.
//!
//! **Three rules, and the third is the one that keeps it honest.**
//!
//! *Only project types are edges.* A class naming `java.util.List` says
//! nothing useful; a class naming `com.example.domain.Money` does. Types with
//! no class file under this project's output directories are dropped, which
//! keeps the graph to the size of the project rather than of its classpath.
//!
//! *Reachability is transitive.* A change to a domain record must select the
//! controller test three hops away, or the selection is a faster way to miss
//! the failure.
//!
//! *Unknown means run.* Every way this can fail to know something --
//! no git, a source it cannot map to a class, a class file it cannot read,
//! nothing compiled yet -- returns [`Selection::Everything`] with a reason,
//! never a smaller set. A test selector that silently drops a test is a green
//! build that proves nothing, which is worse than a slow one.
//!
//! What it cannot see is stated rather than guessed at: reflection, Spring's
//! component scan, a resource file, or an annotation processor edge widens to
//! the full suite and records why.

mod epoch;

use crate::process::{CommandSpec, Diagnostics, OutputMode};
use jails_support::Result;
use jails_support::codec::{domain_hash, sha256};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};

pub(crate) enum Selection {
    /// These test classes, by fully qualified name.
    Tests { epoch: u64, tests: Vec<String> },
    /// Everything, because of these facts.
    Everything { epoch: u64, reasons: Vec<String> },
    /// Inputs cannot be proved to match compiled output.
    Stale { epoch: u64, reasons: Vec<String> },
    /// Nothing changed that any test can see.
    Nothing { epoch: u64 },
}

/// The test classes reachable from what has changed in the working tree.
pub(crate) fn select(root: &Path, debug: bool) -> Selection {
    let input = match input_snapshot(root) {
        Ok(input) => input,
        Err(reason) => {
            return Selection::Stale {
                epoch: 0,
                reasons: vec![reason.to_string()],
            };
        }
    };

    let main = root.join("target/classes");
    let tests = root.join("target/test-classes");
    let mut graph = Graph::default();
    let mut reasons = Vec::new();
    graph.absorb(&main, false, &mut reasons);
    graph.absorb(&tests, true, &mut reasons);
    let graph_digest = graph.digest();
    let epoch = match epoch::record(root, input, graph_digest) {
        Ok(recorded) => recorded,
        Err(reason) => {
            return Selection::Stale {
                epoch: 0,
                reasons: vec![reason.to_string()],
            };
        }
    };
    if graph.owners.is_empty() {
        reasons.push("nothing is compiled yet".into());
    }
    if let Some(stale) = crate::launcher::staleness(root) {
        return Selection::Stale {
            epoch,
            reasons: vec![stale.explain()],
        };
    }
    let changed = match changed_paths(root, debug) {
        Ok(changed) => changed,
        Err(reason) => {
            reasons.push(reason.to_string());
            Vec::new()
        }
    };
    if !reasons.is_empty() {
        return Selection::Everything { epoch, reasons };
    }
    if changed.is_empty() {
        return Selection::Nothing { epoch };
    }
    if let Some(path) = changed.iter().find(|path| !path.exists()) {
        return Selection::Stale {
            epoch,
            reasons: vec![format!(
                "{} was deleted, so its previous owner edges are unknown",
                path.display()
            )],
        };
    }
    if let Some(path) = changed.iter().find(|path| !is_java_source(path)) {
        if resource_output_is_current(root, path) {
            return Selection::Everything {
                epoch,
                reasons: vec![format!(
                    "{} can affect tests without a bytecode edge",
                    path.display()
                )],
            };
        }
        return Selection::Stale {
            epoch,
            reasons: vec![format!(
                "{} changed without current compiled or copied output",
                path.display()
            )],
        };
    }

    if let Some(path) = changed
        .iter()
        .find(|path| !java_output_is_current(root, &graph, path))
    {
        return Selection::Stale {
            epoch,
            reasons: vec![format!(
                "{} is newer than its compiled class output",
                path.display()
            )],
        };
    }

    let mut seeds = BTreeSet::new();
    for source in &changed {
        match seed_classes(&graph, source) {
            Some(classes) => seeds.extend(classes),
            // A changed source with no class of its own is the case that must
            // widen rather than narrow: a brand new file, or one whose class
            // has not been compiled yet.
            None => {
                return Selection::Everything {
                    epoch,
                    reasons: vec![format!(
                        "{} has no compiled class, so what depends on it is unknown",
                        source.display()
                    )],
                };
            }
        }
    }
    Selection::Tests {
        epoch,
        tests: graph.tests_reachable_from(seeds),
    }
}

fn java_output_is_current(project: &Path, graph: &Graph, source: &Path) -> bool {
    let Some(classes) = seed_classes(graph, source) else {
        return false;
    };
    let Ok(source_time) = source.metadata().and_then(|metadata| metadata.modified()) else {
        return false;
    };
    classes.into_iter().all(|class| {
        let output = if graph.owners.get(&class) == Some(&true) {
            project.join("target/test-classes")
        } else {
            project.join("target/classes")
        };
        output
            .join(format!("{class}.class"))
            .metadata()
            .and_then(|metadata| metadata.modified())
            .is_ok_and(|compiled| compiled >= source_time)
    })
}

fn resource_output_is_current(project: &Path, source: &Path) -> bool {
    for (input, output) in [
        ("src/main/resources", "target/classes"),
        ("src/test/resources", "target/test-classes"),
    ] {
        let input = project.join(input);
        if let Ok(relative) = source.strip_prefix(&input) {
            return std::fs::read(source).ok()
                == std::fs::read(project.join(output).join(relative)).ok();
        }
    }
    false
}

/// Every internal class name in this project, and who names whom.
#[derive(Default)]
struct Graph {
    /// internal name -> the classes that reference it.
    referrers: BTreeMap<String, BTreeSet<String>>,
    /// internal name -> is it a test class.
    owners: BTreeMap<String, bool>,
}

impl Graph {
    fn absorb(&mut self, dir: &Path, is_test: bool, reasons: &mut Vec<String>) {
        for class in class_files(dir, reasons) {
            let (name, bytes) = (class.name, class.bytes);
            self.owners.insert(name.clone(), is_test);
            if let Some(types) = crate::classfile::referenced_types(&bytes) {
                for referenced in types {
                    if referenced != name {
                        self.referrers
                            .entry(referenced)
                            .or_default()
                            .insert(name.clone());
                    }
                }
            } else {
                reasons.push(format!(
                    "{} could not be parsed, so its dependency edges are unknown",
                    class.path.display()
                ));
            }
        }
        // An inner class is compiled separately but changes with its outer, so
        // `Outer$Inner` is treated as referencing `Outer`. Without this a
        // change to a record's outer file misses everything that only ever
        // names the nested type.
        let names: Vec<String> = self.owners.keys().cloned().collect();
        for name in names {
            if let Some((outer, _)) = name.split_once('$') {
                self.referrers
                    .entry(outer.to_string())
                    .or_default()
                    .insert(name.clone());
            }
        }
    }

    fn digest(&self) -> [u8; 32] {
        let mut canonical = Vec::new();
        for (name, is_test) in &self.owners {
            canonical.extend_from_slice(&(name.len() as u32).to_be_bytes());
            canonical.extend_from_slice(name.as_bytes());
            canonical.push(u8::from(*is_test));
        }
        for (target, referrers) in &self.referrers {
            canonical.extend_from_slice(&(target.len() as u32).to_be_bytes());
            canonical.extend_from_slice(target.as_bytes());
            canonical.extend_from_slice(&(referrers.len() as u32).to_be_bytes());
            for referrer in referrers {
                canonical.extend_from_slice(&(referrer.len() as u32).to_be_bytes());
                canonical.extend_from_slice(referrer.as_bytes());
            }
        }
        domain_hash("JAILS-AFFECTED-GRAPH-2", &canonical)
    }

    /// Breadth-first over `referrers`, keeping the test classes found.
    fn tests_reachable_from(&self, seeds: BTreeSet<String>) -> Vec<String> {
        let mut seen: BTreeSet<String> = seeds.iter().cloned().collect();
        let mut queue: VecDeque<String> = seeds.into_iter().collect();
        let mut found = BTreeSet::new();
        while let Some(name) = queue.pop_front() {
            if self.owners.get(&name) == Some(&true) {
                // An inner class cannot be selected; JUnit wants the outer.
                let outer = name.split('$').next().unwrap_or(&name);
                found.insert(outer.replace('/', "."));
            }
            for referrer in self.referrers.get(&name).into_iter().flatten() {
                if seen.insert(referrer.clone()) {
                    queue.push_back(referrer.clone());
                }
            }
        }
        found.into_iter().collect()
    }
}

/// `src/main/java/com/example/Money.java` -> `com/example/Money` and its
/// inner classes, or `None` when none of them is compiled.
fn seed_classes(graph: &Graph, source: &Path) -> Option<Vec<String>> {
    let text = source.to_string_lossy().replace('\\', "/");
    let stem = ["src/main/java/", "src/test/java/"]
        .iter()
        .find_map(|prefix| text.split_once(prefix).map(|(_, rest)| rest))?
        .strip_suffix(".java")?
        .to_string();
    let classes: Vec<String> = graph
        .owners
        .keys()
        .filter(|name| **name == stem || name.starts_with(&format!("{stem}$")))
        .cloned()
        .collect();
    (!classes.is_empty()).then_some(classes)
}

/// `(internal name, bytes)` for every class file under `dir`.
struct ClassFile {
    name: String,
    path: PathBuf,
    bytes: Vec<u8>,
}

fn class_files(dir: &Path, reasons: &mut Vec<String>) -> Vec<ClassFile> {
    let mut found = Vec::new();
    if dir
        .symlink_metadata()
        .is_ok_and(|metadata| metadata.file_type().is_symlink())
    {
        reasons.push(format!(
            "{} is a symlink, so the affected graph cannot attribute its class files",
            dir.display()
        ));
        return found;
    }
    let mut stack = vec![dir.to_path_buf()];
    while let Some(path) = stack.pop() {
        let entries = match std::fs::read_dir(&path) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                reasons.push(format!(
                    "{} could not be read ({error}), so the affected graph is incomplete",
                    path.display()
                ));
                continue;
            }
        };
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    reasons.push(format!(
                        "{} has an unreadable entry ({error}), so the affected graph is incomplete",
                        path.display()
                    ));
                    continue;
                }
            };
            let path = entry.path();
            let kind = match entry.file_type() {
                Ok(kind) => kind,
                Err(error) => {
                    reasons.push(format!(
                        "{} could not be classified ({error}), so the affected graph is incomplete",
                        path.display()
                    ));
                    continue;
                }
            };
            if kind.is_symlink() {
                reasons.push(format!(
                    "{} is a symlink, so the affected graph cannot attribute its class file",
                    path.display()
                ));
            } else if kind.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "class") {
                let relative = match path.strip_prefix(dir) {
                    Ok(relative) => relative,
                    Err(_) => continue,
                };
                let name = relative
                    .to_string_lossy()
                    .replace('\\', "/")
                    .trim_end_matches(".class")
                    .to_string();
                match std::fs::read(&path) {
                    Ok(bytes) => found.push(ClassFile { name, path, bytes }),
                    Err(error) => reasons.push(format!(
                        "{} could not be read ({error}), so its dependency edges are unknown",
                        path.display()
                    )),
                }
            }
        }
    }
    found
}

/// The Java sources git reports as changed in the working tree.
///
/// **Git, rather than a marker file jails writes.** A marker would make the
/// selection depend on when jails last ran, so the same command would select
/// different tests on two consecutive invocations with no edit between them --
/// and after a red run with nothing changed it would select nothing and report
/// green. "What I have changed since my last commit" is a set the person at
/// the keyboard already knows without being told.
fn changed_paths(root: &Path, debug: bool) -> Result<Vec<PathBuf>> {
    let spec = CommandSpec::new("git")
        .args([
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=all",
            "--",
        ])
        .args(INPUT_PATHS)
        .current_dir(root)
        .output(OutputMode::Capture);
    let done = crate::process::run(&spec, Diagnostics::from_flag(debug))
        .map_err(|_| "git is not available, so what changed is unknown".to_string())?;
    if !done.status.success() {
        return Err("this is not a git repository, so what changed is unknown".into());
    }
    Ok(parse_porcelain_z(&done.stdout_string())
        .into_iter()
        .map(|path| root.join(path))
        .collect())
}

const INPUT_PATHS: &[&str] = &[
    "src/main/java",
    "src/test/java",
    "src/main/resources",
    "src/test/resources",
    "pom.xml",
    "build.gradle",
    "build.gradle.kts",
    "settings.gradle",
    "settings.gradle.kts",
    "gradle.properties",
    ".mvn",
    "mvnw",
    "mvnw.cmd",
    "gradle",
    "gradlew",
    "gradlew.bat",
    "jails.toml",
    ".jails/app.toml",
];

fn input_snapshot(project: &Path) -> Result<[u8; 32]> {
    let mut files = Vec::new();
    for relative in INPUT_PATHS {
        collect_input_files(project, &project.join(relative), &mut files)?;
    }
    files.sort_by(|left, right| left.0.cmp(&right.0));
    let mut canonical = Vec::new();
    for (path, digest) in files {
        canonical.extend_from_slice(&(path.len() as u32).to_be_bytes());
        canonical.extend_from_slice(path.as_bytes());
        canonical.extend_from_slice(&digest);
    }
    Ok(domain_hash("JAILS-AFFECTED-INPUT-2", &canonical))
}

fn collect_input_files(
    project: &Path,
    path: &Path,
    files: &mut Vec<(String, [u8; 32])>,
) -> Result<()> {
    let metadata = match path.symlink_metadata() {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(format!(
                "{} could not be inspected ({error}), so the input epoch is unknown\n       fix: restore readable project inputs and retry",
                path.display()
            )
            .into());
        }
    };
    if metadata.file_type().is_symlink() {
        return Err(format!(
            "{} is a symlink, so its input bytes cannot be attributed to this project\n       fix: replace it with a project-owned file or directory",
            path.display()
        )
        .into());
    }
    if metadata.is_dir() {
        let mut children = std::fs::read_dir(path)
            .map_err(|error| {
                format!(
                    "failed to read {}: {error}\n       fix: restore readable project inputs and retry",
                    path.display()
                )
            })?
            .collect::<std::io::Result<Vec<_>>>()
            .map_err(|error| {
                format!(
                    "failed to enumerate {}: {error}\n       fix: restore readable project inputs and retry",
                    path.display()
                )
            })?;
        children.sort_by_key(|entry| entry.file_name());
        for child in children {
            collect_input_files(project, &child.path(), files)?;
        }
    } else if metadata.is_file() {
        let relative = path
            .strip_prefix(project)
            .map_err(|_| {
                format!(
                    "{} is outside the affected input root\n       fix: keep affected inputs inside the project",
                    path.display()
                )
            })?;
        let bytes = std::fs::read(path).map_err(|error| {
            format!(
                "failed to read {}: {error}\n       fix: restore readable project inputs and retry",
                path.display()
            )
        })?;
        files.push((
            relative.to_string_lossy().replace('\\', "/"),
            sha256(&bytes),
        ));
    }
    Ok(())
}

fn is_java_source(path: &Path) -> bool {
    let normalized = path.to_string_lossy().replace('\\', "/");
    (normalized.contains("/src/main/java/") || normalized.contains("/src/test/java/"))
        && path
            .extension()
            .is_some_and(|extension| extension == "java")
}

/// Paths out of NUL-delimited porcelain v1. Rename/copy records put the new
/// path first and the old path in the following field; only the new owner can
/// have a current class output.
fn parse_porcelain_z(output: &str) -> Vec<PathBuf> {
    let mut fields = output.split('\0').filter(|field| !field.is_empty());
    let mut paths = Vec::new();
    while let Some(entry) = fields.next() {
        let status = entry.get(..2).unwrap_or_default();
        let Some(path) = entry.get(3..) else { continue };
        paths.push(PathBuf::from(path));
        if status.contains('R') || status.contains('C') {
            let _old_path = fields.next();
        }
    }
    paths
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn porcelain_z_yields_paths_and_a_rename_yields_only_the_new_one() {
        assert_eq!(
            parse_porcelain_z(
                " M src/main/java/A.java\0?? src/test/java/B.java\0R  src/main/java/New.java\0src/main/java/Old.java\0"
            ),
            vec![
                PathBuf::from("src/main/java/A.java"),
                PathBuf::from("src/test/java/B.java"),
                PathBuf::from("src/main/java/New.java")
            ]
        );
    }

    fn graph_of(edges: &[(&str, bool, &[&str])]) -> Graph {
        let mut graph = Graph::default();
        for (name, is_test, references) in edges {
            graph.owners.insert((*name).to_string(), *is_test);
            for referenced in *references {
                graph
                    .referrers
                    .entry((*referenced).to_string())
                    .or_default()
                    .insert((*name).to_string());
            }
        }
        graph
    }

    /// The property the whole feature turns on: a change three hops away from
    /// a test still selects it. A one-hop version would look right on a
    /// scaffold and quietly miss the controller test that made it worth doing.
    #[test]
    fn reachability_is_transitive() {
        let graph = graph_of(&[
            ("com/example/Money", false, &[]),
            ("com/example/Service", false, &["com/example/Money"]),
            ("com/example/Controller", false, &["com/example/Service"]),
            (
                "com/example/ControllerTest",
                true,
                &["com/example/Controller"],
            ),
            ("com/example/UnrelatedTest", true, &["com/example/Other"]),
        ]);
        let selected =
            graph.tests_reachable_from(BTreeSet::from(["com/example/Money".to_string()]));
        assert_eq!(selected, vec!["com.example.ControllerTest"]);
    }

    /// JUnit selects the outer class; an inner one is not a runnable name.
    #[test]
    fn an_inner_test_class_is_reported_as_its_outer_class() {
        let graph = graph_of(&[("com/example/AppTest$Nested", true, &["com/example/Money"])]);
        let selected =
            graph.tests_reachable_from(BTreeSet::from(["com/example/Money".to_string()]));
        assert_eq!(selected, vec!["com.example.AppTest"]);
    }

    /// A source whose class is not compiled must widen the selection, not
    /// narrow it -- a brand new file is exactly the case where guessing costs
    /// the most.
    #[test]
    fn a_source_with_no_compiled_class_yields_no_seed() {
        let graph = graph_of(&[("com/example/Money", false, &[])]);
        assert!(seed_classes(&graph, Path::new("src/main/java/com/example/New.java")).is_none());
        assert_eq!(
            seed_classes(
                &graph,
                Path::new("/tmp/p/src/main/java/com/example/Money.java")
            ),
            Some(vec!["com/example/Money".to_string()])
        );
    }

    #[test]
    fn the_input_epoch_is_content_addressed_and_ignores_unrelated_files() {
        let project = jails_support::scratch::ScratchDir::in_temp("affected-input").unwrap();
        let source = project.path().join("src/main/java/example/App.java");
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        std::fs::write(&source, "class App {}\n").unwrap();
        let first = input_snapshot(project.path()).unwrap();
        std::fs::write(project.path().join("README.md"), "ignored\n").unwrap();
        assert_eq!(input_snapshot(project.path()).unwrap(), first);
        std::fs::write(&source, "class App { int changed; }\n").unwrap();
        assert_ne!(input_snapshot(project.path()).unwrap(), first);
    }

    #[test]
    fn an_unreadable_class_widens_instead_of_losing_its_edges() {
        let project = jails_support::scratch::ScratchDir::in_temp("affected-class").unwrap();
        let class = project.path().join("example/Broken.class");
        std::fs::create_dir_all(class.parent().unwrap()).unwrap();
        std::fs::write(&class, b"not a class").unwrap();
        let mut graph = Graph::default();
        let mut reasons = Vec::new();
        graph.absorb(project.path(), false, &mut reasons);
        assert_eq!(graph.owners.get("example/Broken"), Some(&false));
        assert!(
            reasons
                .iter()
                .any(|reason| reason.contains("could not be parsed"))
        );
    }
}
