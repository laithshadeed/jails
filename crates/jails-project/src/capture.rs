//! One immutable reading of a project, taken once.
//!
//! plan.md §R2: a request is planned against *"one immutable capture of a
//! project"*. Everything downstream — projection, preparation, the executor's
//! precondition check — compares against this value rather than against the
//! filesystem, which is what makes a plan describe a project that actually
//! existed at a moment rather than one assembled from readings taken at
//! different times.
//!
//! ## Declared reads, and why an absence is a fact
//!
//! Nothing here walks the tree looking for interesting files. The caller
//! declares which paths it intends to read, and each one comes back present or
//! absent — both recorded. That is the whole difference between "the plan did
//! not see a `compose.yaml`" and "there was no `compose.yaml`": the first is a
//! gap the executor cannot check, the second is a precondition it can.
//!
//! ## Why a directory listing is part of it
//!
//! `g migration` allocates the next serial number by looking at what is
//! already in the migrations directory. That reading has to be part of the
//! same capture as everything else, or two runs can both allocate `V3`.
//!
//! ## What it refuses
//!
//! A symlink, and a declared path that turns out to be a directory. jails
//! writes files and directories it owns; a symlink in the middle of a declared
//! read means the bytes it captured and the bytes it would later replace can
//! be two different files, and there is no honest snapshot of that.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::model::Project;
use crate::projection::ProjectedProject;
use jails_protocol::conflict::FileMode;
use jails_protocol::identity::{Package, ProjectPath};
use jails_protocol::snapshot::{CanonicalRoot, ProjectSnapshot, SnapshotFile};
use jails_support::Result;

/// What the caller intends to read.
///
/// Deliberately a value rather than four arguments: the declaration *is* the
/// thing a reader of a plan wants to see, and building it in one place is what
/// stops a planner reaching for a fact nobody declared.
#[derive(Clone, Debug, Default)]
pub struct ReadDeclaration {
    files: BTreeSet<ProjectPath>,
    directories: BTreeSet<ProjectPath>,
}

impl ReadDeclaration {
    pub fn new() -> Self {
        Self::default()
    }

    /// Declare one file. Absence is a legitimate outcome, not an error.
    pub fn file(mut self, path: ProjectPath) -> Self {
        self.files.insert(path);
        self
    }

    /// Declare a directory listing.
    pub fn directory(mut self, path: ProjectPath) -> Self {
        self.directories.insert(path);
        self
    }

    pub fn files(&self) -> impl Iterator<Item = &ProjectPath> {
        self.files.iter()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty() && self.directories.is_empty()
    }
}

/// Take the capture.
///
/// Takes the *resolved* root, not a path to resolve: two runs started from
/// different directories have to agree on what they captured, and resolving
/// once at the boundary is what makes that true. [`canonical_root`] is that
/// boundary.
pub fn capture(root: &CanonicalRoot, declaration: &ReadDeclaration) -> Result<ProjectSnapshot> {
    let at = Path::new(root.as_str());
    let mut files = BTreeMap::new();
    let mut absences = BTreeSet::new();
    for path in &declaration.files {
        match read_file(&resolve(at, path), path)? {
            Some(file) => {
                files.insert(path.clone(), file);
            }
            None => {
                absences.insert(path.clone());
            }
        }
    }
    let mut directories = BTreeMap::new();
    for path in &declaration.directories {
        directories.insert(path.clone(), list(&resolve(at, path), path)?);
    }
    ProjectSnapshot::new(root.clone(), files, absences, directories)
}

/// Everything a capability plan is allowed to look at.
///
/// One list, in one place, because a read set assembled per capability is a
/// read set that differs between the plan and the recheck. The four files here
/// are the format owners a capability contributes to: the POM, the compose
/// file, the application properties and the manifest that records what is
/// installed.
pub fn capability_reads() -> Result<ReadDeclaration> {
    Ok(ReadDeclaration::new()
        .file(ProjectPath::parse("pom.xml")?)
        // Both build files, always. Which one a project has is not this
        // function's to decide -- it declares what *may* be read, and a
        // projection can only overlay a path its snapshot captured. Declaring
        // only the one that exists would make the read set depend on the
        // project, and two runs of one command would then guard different
        // preconditions.
        .file(ProjectPath::parse(crate::gradle::FILE)?)
        .file(ProjectPath::parse("compose.yaml")?)
        .file(ProjectPath::parse(
            "src/main/resources/application.properties",
        )?)
        // The test overlay, for the same reason both build files are here: a
        // projection can only overlay a path its snapshot captured, and
        // declaring it only when it exists would make the read set depend on
        // the project -- so `jails set --tests` on a project that has never
        // had one would guard a different precondition from the same command
        // run again.
        .file(ProjectPath::parse(
            "src/test/resources/config/application.properties",
        )?)
        .file(ProjectPath::parse("jails.toml")?))
}

/// Capture a project and open a projection over it.
///
/// The two steps belong together: a projection is *defined* as an overlay on
/// one capture, and a caller that could pair a projection with a different
/// snapshot than the one it was opened on would be planning against two
/// readings of the same project.
pub fn projected(
    project: &Project,
    declaration: &ReadDeclaration,
) -> Result<(std::sync::Arc<ProjectSnapshot>, ProjectedProject)> {
    let root = canonical_root(project.root())?;
    let snapshot = std::sync::Arc::new(capture(&root, declaration)?);
    let projection = ProjectedProject::new(
        snapshot.clone(),
        project.build(),
        Package::parse(project.base())?,
        // A project whose POM states no release is planned against the one
        // jails targets. Guessing lower would silently generate for a language
        // level the project never asked for; there is no third option, because
        // a renderer has to be told something. `TARGET_RELEASE` is the
        // spelling that goes into a POM, so it is parsed rather than restated.
        match project.java_release() {
            Some(release) => release,
            None => crate::pom::TARGET_RELEASE.parse::<u32>().map_err(|_| {
                format!(
                    "jails' own target release `{}` is not a number",
                    crate::pom::TARGET_RELEASE
                )
            })?,
        },
        Some(project.flavor()),
    );
    Ok((snapshot, projection))
}

/// The root as one string two runs from different directories agree on.
pub fn canonical_root(root: &Path) -> Result<CanonicalRoot> {
    let resolved = std::fs::canonicalize(root)
        .map_err(|error| format!("failed to resolve {}: {error}", root.display()))?;
    let text = resolved
        .to_str()
        .ok_or_else(|| format!("{} is not valid UTF-8", resolved.display()))?;
    CanonicalRoot::new(text)
}

fn resolve(at: &Path, path: &ProjectPath) -> PathBuf {
    at.join(path.as_str())
}

fn read_file(at: &Path, path: &ProjectPath) -> Result<Option<SnapshotFile>> {
    let metadata = match at.symlink_metadata() {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("failed to read {path}: {error}")),
    };
    if metadata.file_type().is_symlink() {
        return Err(format!(
            "{path} is a symlink.\n       fix: jails snapshots the bytes it will later replace, \
             and a link means those can be two different files. Replace it with the file itself, \
             or keep it outside the project."
        ));
    }
    if metadata.is_dir() {
        return Err(format!(
            "{path} is a directory, and the plan declared it as a file"
        ));
    }
    let bytes = std::fs::read(at).map_err(|error| format!("failed to read {path}: {error}"))?;
    Ok(Some(SnapshotFile::capture(bytes, mode_of(&metadata)?)))
}

/// The entries of one directory, by name, in one order.
///
/// Sorted, because `read_dir` order is the filesystem's and two captures of an
/// unchanged directory must be the same value. An absent directory lists as
/// empty rather than failing: "nothing has been generated yet" is the ordinary
/// state of a migrations directory.
/// One directory listing, as the snapshot records it.
///
/// Public because the commit-time recheck has to produce the *same* list from
/// the same directory -- §R4.3 step 2 compares its digest against the one the
/// plan captured, and two enumerations that sorted differently would report a
/// change nobody made.
pub fn list_directory(at: &Path, path: &ProjectPath) -> Result<Vec<ProjectPath>> {
    list(at, path)
}

fn list(at: &Path, path: &ProjectPath) -> Result<Vec<ProjectPath>> {
    let entries = match std::fs::read_dir(at) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("failed to list {path}: {error}")),
    };
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| format!("failed to list {path}: {error}"))?;
        let name = entry.file_name();
        let name = name
            .to_str()
            .ok_or_else(|| format!("{path} contains a name that is not valid UTF-8"))?;
        names.push(ProjectPath::parse(&format!("{path}/{name}"))?);
    }
    names.sort();
    Ok(names)
}

#[cfg(unix)]
fn mode_of(metadata: &std::fs::Metadata) -> Result<FileMode> {
    use std::os::unix::fs::PermissionsExt;
    FileMode::new(metadata.permissions().mode() & FileMode::PERMITTED)
}

#[cfg(not(unix))]
fn mode_of(_metadata: &std::fs::Metadata) -> Result<FileMode> {
    // Nothing to read, so nothing is claimed: 0o644 is what the executor
    // writes and what it will verify.
    FileMode::new(0o644)
}

#[cfg(test)]
mod tests {
    use super::*;
    use jails_support::scratch::ScratchDir;

    fn path(text: &str) -> ProjectPath {
        ProjectPath::parse(text).unwrap()
    }

    fn project() -> ScratchDir {
        let scratch = ScratchDir::in_temp("capture").unwrap();
        jails_support::apply::put(scratch.path().join("pom.xml"), "<project/>\n").unwrap();
        scratch
    }

    #[test]
    fn a_declared_file_that_is_not_there_is_recorded_as_absent() {
        let scratch = project();
        let snapshot = capture(
            &canonical_root(scratch.path()).unwrap(),
            &ReadDeclaration::new()
                .file(path("pom.xml"))
                .file(path("compose.yaml")),
        )
        .unwrap();
        assert!(matches!(
            snapshot.read(&path("pom.xml")).unwrap(),
            jails_protocol::snapshot::Captured::Present(_)
        ));
        assert_eq!(
            snapshot.read(&path("compose.yaml")).unwrap(),
            jails_protocol::snapshot::Captured::Absent,
            "an absence the plan declared is a fact it can be held to"
        );
    }

    #[test]
    fn an_undeclared_path_is_neither_present_nor_absent() {
        let scratch = project();
        let snapshot = capture(
            &canonical_root(scratch.path()).unwrap(),
            &ReadDeclaration::new().file(path("pom.xml")),
        )
        .unwrap();
        let error = snapshot.read(&path("compose.yaml")).unwrap_err();
        assert!(error.contains("was not captured"), "{error}");
    }

    #[test]
    fn a_directory_lists_in_one_order_whatever_the_filesystem_says() {
        let scratch = project();
        for name in ["V2__b.sql", "V1__a.sql", "V10__c.sql"] {
            jails_support::apply::put(
                scratch
                    .path()
                    .join("src/main/resources/db/migration")
                    .join(name),
                "select 1;\n",
            )
            .unwrap();
        }
        let listed = capture(
            &canonical_root(scratch.path()).unwrap(),
            &ReadDeclaration::new().directory(path("src/main/resources/db/migration")),
        )
        .unwrap();
        let entries = listed
            .list(&path("src/main/resources/db/migration"))
            .unwrap();
        let names: Vec<&str> = entries.iter().map(ProjectPath::as_str).collect();
        assert_eq!(
            names,
            [
                "src/main/resources/db/migration/V10__c.sql",
                "src/main/resources/db/migration/V1__a.sql",
                "src/main/resources/db/migration/V2__b.sql",
            ]
        );
    }

    #[test]
    fn a_directory_that_does_not_exist_yet_lists_as_empty() {
        let scratch = project();
        let snapshot = capture(
            &canonical_root(scratch.path()).unwrap(),
            &ReadDeclaration::new().directory(path("src/main/resources/db/migration")),
        )
        .unwrap();
        assert!(
            snapshot
                .list(&path("src/main/resources/db/migration"))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn a_symlink_in_a_declared_read_is_refused() {
        let scratch = project();
        std::os::unix::fs::symlink(
            scratch.path().join("pom.xml"),
            scratch.path().join("link.xml"),
        )
        .unwrap();
        let error = capture(
            &canonical_root(scratch.path()).unwrap(),
            &ReadDeclaration::new().file(path("link.xml")),
        )
        .unwrap_err();
        assert!(error.contains("symlink"), "{error}");
        assert!(error.contains("fix:"), "{error}");
    }

    #[test]
    fn the_captured_bytes_carry_their_own_identity() {
        let scratch = project();
        let snapshot = capture(
            &canonical_root(scratch.path()).unwrap(),
            &ReadDeclaration::new().file(path("pom.xml")),
        )
        .unwrap();
        let jails_protocol::snapshot::Captured::Present(file) =
            snapshot.read(&path("pom.xml")).unwrap()
        else {
            panic!("the pom was declared and is there");
        };
        assert_eq!(file.len, "<project/>\n".len() as u64);
        assert_eq!(
            file.sha256,
            jails_protocol::identity::ObjectId::from_bytes(jails_support::codec::sha256(
                b"<project/>\n"
            ))
        );
    }
}
