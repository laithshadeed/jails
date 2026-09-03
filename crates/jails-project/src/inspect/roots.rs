//! Where a project's Java lives.
//!
//! Its own module because it is its own secret. Nothing here reads Java; it
//! answers which directories hold it, and what to call them in a report that
//! found nothing.
//!
//! **One answer to "where is the source."** Every scanner in the tool asks
//! [`input_roots`] -- the affected index, the watch fingerprint, the Kafka
//! topic scan, `jails lint`, the editor handshake -- so a root that moves
//! moves for all of them at once. Five separate lists is how one of them came
//! to be told the generated tree did not exist while the other four could see
//! it.

use std::path::{Path, PathBuf};

/// Every directory this project's own inputs live in.
///
/// The list is the standard Maven and Gradle one, and it is a function rather
/// than a constant on purpose: managed output lives beside the reader's own
/// sources under `src/` today, so the answer is uniform, but *uniform* is not
/// the same as *asked once*. A build that states its own source set -- a
/// Gradle `sourceSets` block naming another directory -- has one place to be
/// read into, and every scanner picks the change up together.
///
/// Ordered main before test, Java before resources, because the editor
/// handshake prints them in that order and an editor diffing two handshakes
/// should see a reordering only when a root actually moved.
pub fn input_roots(root: &Path) -> Vec<InputRoot> {
    [
        (SourceSet::Main, RootKind::Java, "src/main/java"),
        (SourceSet::Test, RootKind::Java, "src/test/java"),
        (SourceSet::Main, RootKind::Resources, "src/main/resources"),
        (SourceSet::Test, RootKind::Resources, "src/test/resources"),
    ]
    .into_iter()
    .map(|(set, kind, relative)| InputRoot {
        path: root.join(relative),
        relative,
        set,
        kind,
    })
    .collect()
}

/// One directory of project inputs.
///
/// The project-relative spelling travels with the absolute path because every
/// consumer needs both: the path to walk, and the relative name to print, to
/// hand git as a pathspec, or to strip a changed file against.
pub struct InputRoot {
    pub path: PathBuf,
    pub relative: &'static str,
    pub set: SourceSet,
    pub kind: RootKind,
}

impl InputRoot {
    /// The name an editor is told this root by: `main-java`, `test-resources`.
    pub fn label(&self) -> &'static str {
        match (self.set, self.kind) {
            (SourceSet::Main, RootKind::Java) => "main-java",
            (SourceSet::Test, RootKind::Java) => "test-java",
            (SourceSet::Main, RootKind::Resources) => "main-resources",
            (SourceSet::Test, RootKind::Resources) => "test-resources",
        }
    }
}

/// What a root holds: compiled sources, or files copied to the output as they
/// are. The distinction is the one every consumer makes -- a changed `.java`
/// has a bytecode edge to follow and a changed resource does not.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RootKind {
    Java,
    Resources,
}

/// The Java source root of one source set, with the relative name to print
/// for it.
///
/// The Java projection of [`input_roots`]: one tree per set, because managed
/// output lives beside the reader's own sources under `src/`, so a route jails
/// emitted and a route the reader wrote are found by the same scan. The label
/// travels with the path so nothing downstream needs the root again.
pub fn source_roots(root: &Path, set: SourceSet) -> Vec<(PathBuf, &'static str)> {
    input_roots(root)
        .into_iter()
        .filter(|input| input.set == set && input.kind == RootKind::Java)
        .map(|input| (input.path, input.relative))
        .collect()
}

#[derive(Clone, Copy, PartialEq, Eq)]
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
pub fn scanned(roots: &[(PathBuf, &'static str)]) -> String {
    roots
        .iter()
        .map(|(_, label)| *label)
        .collect::<Vec<_>>()
        .join(" and ")
}
