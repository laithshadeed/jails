//! Which tests can the last edit possibly have broken?
//!
//! `plan.md` §10.2 step 3. The index is a reverse-dependency map built from
//! the constant pools already sitting in `target/` -- see [`crate::classfile`]
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
//! component scan, a resource file, a `application.properties` key. That is
//! why `jails check` stays `mvn clean verify` and why this is opt-in.

use crate::classfile;
use jails_support::process::{self, CommandSpec, Diagnostics, OutputMode};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};

pub(crate) enum Selection {
    /// These test classes, by fully qualified name.
    Tests(Vec<String>),
    /// Everything, because of this.
    Everything(String),
    /// Nothing changed that any test can see.
    Nothing,
}

/// The test classes reachable from what has changed in the working tree.
pub(crate) fn select(root: &Path, debug: bool) -> Selection {
    let changed = match changed_sources(root, debug) {
        Ok(changed) if changed.is_empty() => return Selection::Nothing,
        Ok(changed) => changed,
        Err(reason) => return Selection::Everything(reason),
    };

    let main = root.join("target/classes");
    let tests = root.join("target/test-classes");
    let mut graph = Graph::default();
    graph.absorb(&main, false);
    graph.absorb(&tests, true);
    if graph.owners.is_empty() {
        return Selection::Everything("nothing is compiled yet".into());
    }

    let mut seeds = BTreeSet::new();
    for source in &changed {
        match seed_classes(&graph, source) {
            Some(classes) => seeds.extend(classes),
            // A changed source with no class of its own is the case that must
            // widen rather than narrow: a brand new file, or one whose class
            // has not been compiled yet.
            None => {
                return Selection::Everything(format!(
                    "{} has no compiled class, so what depends on it is unknown",
                    source.display()
                ));
            }
        }
    }
    Selection::Tests(graph.tests_reachable_from(seeds))
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
    fn absorb(&mut self, dir: &Path, is_test: bool) {
        for (name, bytes) in class_files(dir) {
            self.owners.insert(name.clone(), is_test);
            if let Some(types) = classfile::referenced_types(&bytes) {
                for referenced in types {
                    if referenced != name {
                        self.referrers
                            .entry(referenced)
                            .or_default()
                            .insert(name.clone());
                    }
                }
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
fn class_files(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut found = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(path) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&path) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "class")
                && let (Ok(relative), Ok(bytes)) = (path.strip_prefix(dir), std::fs::read(&path))
            {
                let name = relative
                    .to_string_lossy()
                    .replace('\\', "/")
                    .trim_end_matches(".class")
                    .to_string();
                found.push((name, bytes));
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
fn changed_sources(root: &Path, debug: bool) -> Result<Vec<PathBuf>, String> {
    let spec = CommandSpec::new("git")
        .args(["status", "--porcelain", "--untracked-files=all", "--"])
        .arg("src/main/java")
        .arg("src/test/java")
        .current_dir(root)
        .output(OutputMode::Capture);
    let done = process::run(&spec, Diagnostics::from_flag(debug))
        .map_err(|_| "git is not available, so what changed is unknown".to_string())?;
    if !done.status.success() {
        return Err("this is not a git repository, so what changed is unknown".into());
    }
    Ok(done
        .stdout_string()
        .lines()
        .filter_map(parse_porcelain)
        .filter(|path| path.extension().is_some_and(|ext| ext == "java"))
        .map(|path| root.join(path))
        .collect())
}

/// The path out of one `git status --porcelain` line.
///
/// The rename form is `XY old -> new`, and it is the new name that has a class
/// file; taking the old one would seed the graph with a name that no longer
/// exists and select nothing.
fn parse_porcelain(line: &str) -> Option<PathBuf> {
    let rest = line.get(3..)?.trim();
    let path = rest.rsplit(" -> ").next()?;
    let path = path.trim_matches('"');
    (!path.is_empty()).then(|| PathBuf::from(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_porcelain_line_yields_its_path_and_a_rename_yields_the_new_one() {
        assert_eq!(
            parse_porcelain(" M src/main/java/A.java"),
            Some(PathBuf::from("src/main/java/A.java"))
        );
        assert_eq!(
            parse_porcelain("?? src/test/java/B.java"),
            Some(PathBuf::from("src/test/java/B.java"))
        );
        assert_eq!(
            parse_porcelain("R  src/main/java/Old.java -> src/main/java/New.java"),
            Some(PathBuf::from("src/main/java/New.java"))
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
}
