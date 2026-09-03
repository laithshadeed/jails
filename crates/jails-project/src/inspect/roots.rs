//! Where a project's Java lives.
//!
//! Its own module because it is its own secret. Nothing here reads Java; it
//! answers which directories hold it, and what to call them in a report that
//! found nothing.

use std::path::{Path, PathBuf};

/// The Java source root of one source set, with the relative name to print
/// for it.
///
/// One tree per set: managed output lives beside the reader's own sources
/// under `src/`, so a route jails emitted and a route the reader wrote are
/// found by the same scan. The label travels with the path so nothing
/// downstream needs the root again.
pub fn source_roots(root: &Path, set: SourceSet) -> Vec<(PathBuf, &'static str)> {
    let reader = match set {
        SourceSet::Main => "src/main/java",
        SourceSet::Test => "src/test/java",
    };
    vec![(root.join(reader), reader)]
}

#[derive(Clone, Copy)]
pub enum SourceSet {
    Main,
    Test,
}

/// Every `.java` file under the roots that exist, in root order.
pub(crate) fn source_files_in(roots: &[(PathBuf, &'static str)]) -> Vec<PathBuf> {
    roots
        .iter()
        .flat_map(|(path, _)| crate::java::source_files(path))
        .collect()
}

/// Name the roots that were walked, so an empty report says where it looked.
pub fn scanned(roots: &[(PathBuf, &'static str)]) -> String {
    roots
        .iter()
        .map(|(_, label)| *label)
        .collect::<Vec<_>>()
        .join(" and ")
}
