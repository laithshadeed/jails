//! Where a project's Java lives.
//!
//! Its own module because it is its own secret, and because `inspect.rs`
//! became the largest module in the workspace the moment this arrived in it.
//! Nothing here reads Java; it answers which directories hold it, and what to
//! call them in a report that found nothing.

use std::path::{Path, PathBuf};

/// The Java source roots this project actually has, each with the relative
/// name to print for it.
///
/// `bugs.md` B56's finding generalises: *a route jails emitted and cannot see
/// is worse than a gap, because the reader cannot tell an unlisted route from
/// an absent one.* Reproducible output moved to `.jails/generated` for the
/// canonical path, so `jails routes` and `jails beans` answered "No routes
/// found under src/main/java" about a project whose every controller jails had
/// just written, and `jails stats` reported a domain of zero files.
///
/// Present-only, reader's tree first. A project with no generated tree gets
/// exactly the old scan and exactly the old message.
///
/// The label travels with the path so nothing downstream needs the root again:
/// this is the one function here that re-derives a fact from a primitive, and
/// the `abstract.md` §7 ladder counts it.
pub fn source_roots(root: &Path, set: SourceSet) -> Vec<(PathBuf, &'static str)> {
    let (reader, generated) = match set {
        SourceSet::Main => ("src/main/java", GENERATED_MAIN_JAVA),
        SourceSet::Test => ("src/test/java", GENERATED_TEST_JAVA),
    };
    let mut roots = vec![(root.join(reader), reader)];
    let path = root.join(generated);
    if path.is_dir() {
        roots.push((path, generated));
    }
    roots
}

#[derive(Clone, Copy)]
pub enum SourceSet {
    Main,
    Test,
}

/// Every `.java` file under the roots that exist, in root order.
pub fn source_files_in(roots: &[(PathBuf, &'static str)]) -> Vec<PathBuf> {
    roots
        .iter()
        .flat_map(|(path, _)| crate::java::source_files(path))
        .collect()
}

/// Name the roots that were walked, so an empty report says where it looked.
///
/// Printing `src/main/java` while also scanning `.jails/generated/main/java`
/// would be the same defect one layer up: a reader who has just generated a
/// controller needs to know the generated tree was searched and came back
/// empty, not that it was never opened.
pub fn scanned(roots: &[(PathBuf, &'static str)]) -> String {
    roots
        .iter()
        .map(|(_, label)| *label)
        .collect::<Vec<_>>()
        .join(" and ")
}

/// Where the compiler writes. A literal rather than an import because
/// `jails-project` does not depend on the canonical ladder, and this is the
/// one fact about it a reader-facing report needs.
const GENERATED_MAIN_JAVA: &str = ".jails/generated/main/java";
const GENERATED_TEST_JAVA: &str = ".jails/generated/test/java";
