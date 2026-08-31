//! The only canonical project writer.
//!
//! It takes a verified `PlanBundle` and nothing else: locks `.jails/apply.lock`,
//! rechecks every captured precondition against the tree as it is *now*,
//! applies the plan's typed operations in order, and verifies the exact
//! after-images it published. There is no replanning here and no path where a
//! desired byte is computed — `simplify-sol.md`'s "apply never replans" is
//! enforced by this module having no compiler and no model to reach for.
//!
//! **It converges rather than rolls back**, which is the deliberate trade the
//! legacy kernel does not make: no journal, no preimage, no recovery command.
//! A crashed run may leave a temporarily mixed but individually valid tree,
//! and the next identical generation repairs it deterministically. Every
//! `continue` in here is that property — an operation whose after-image is
//! already on disk is skipped, so a re-run reports zero written and zero
//! deleted rather than rewriting a tree it agrees with.
//!
//! `crates/jails-workspace/tests/crash.rs` proves it at nine named instants,
//! in-process and in a child that `abort()`s. The aborting half is not
//! ceremony: it found `sweep_staged`'s absence, where a temporary left by a
//! crash between staging and rename wedged the project permanently.
//!
//! The precondition check has three deliberate escape hatches, and they are
//! what makes convergence possible at all: a file already carrying its desired
//! content passes, an absent file that the plan removes passes, and a
//! directory that holds exactly its captured contents plus this plan's own new
//! migrations passes. Without them the second run of an interrupted plan would
//! refuse over its own half-finished work.

use crate::verify_bundle;
use fs2::FileExt as _;
use jails_contracts::{
    ContentDigest, FileMode, FilePrecondition, PlanBundle, PlannedOperation, ProjectPath,
    TreeManifest,
};
use jails_support::codec::{hex, sha256};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write as _;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Execution {
    pub schema: String,
    pub plan_digest: ContentDigest,
    pub operations: usize,
    pub files_written: usize,
    pub files_deleted: usize,
}

pub fn execute(root: &Path, bundle: &PlanBundle) -> Result<Execution, String> {
    verify_bundle(bundle)?;
    let jails = root.join(".jails");
    std::fs::create_dir_all(&jails)
        .map_err(|error| format!("could not create {}: {error}", jails.display()))?;
    let lock_path = jails.join("apply.lock");
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)
        .map_err(|error| format!("could not open {}: {error}", lock_path.display()))?;
    lock.lock_exclusive()
        .map_err(|error| format!("could not lock {}: {error}", lock_path.display()))?;
    crate::fault::trip(crate::fault::point::AFTER_LOCK)?;
    sweep_staged(root, bundle)?;
    verify_preconditions(root, bundle)?;
    crate::fault::trip(crate::fault::point::AFTER_PRECONDITIONS)?;
    preflight_writable(root, bundle)?;

    let mut written = 0_usize;
    let mut deleted = 0_usize;
    for operation in &bundle.plan.operations {
        match operation {
            PlannedOperation::PublishMergedTree {
                root: managed_root,
                after,
                ..
            } => {
                let tree = bundle
                    .trees
                    .get(after)
                    .ok_or_else(|| format!("plan references missing tree `{}`", after.as_str()))?;
                crate::fault::trip(crate::fault::point::BEFORE_TREE)?;
                let counts = publish_merged_tree(root, managed_root, tree, bundle)?;
                crate::fault::trip(crate::fault::point::AFTER_TREE)?;
                written += counts.0;
                deleted += counts.1;
            }
            PlannedOperation::ReplaceModelFile { path, after, .. }
            | PlannedOperation::ReplaceStateFile { path, after, .. }
            | PlannedOperation::PatchReaderFile { path, after, .. }
            | PlannedOperation::AppendMigration { path, after } => {
                let actual = actual_file(root, path)?;
                if actual
                    .as_ref()
                    .is_some_and(|actual| actual.digest == after.blob && actual.mode == after.mode)
                {
                    continue;
                }
                let bytes = bundle.blobs.get(&after.blob).ok_or_else(|| {
                    format!(
                        "model after-image references missing blob `{}`",
                        after.blob.as_str()
                    )
                })?;
                write_atomic(root, path, bytes, after.mode)?;
                written += 1;
            }
            PlannedOperation::RemoveReaderFile { path, .. } => {
                let absolute = root.join(path.as_str());
                crate::fault::trip(crate::fault::point::BEFORE_REMOVE)?;
                match std::fs::remove_file(&absolute) {
                    Ok(()) => {
                        crate::fault::trip(crate::fault::point::AFTER_REMOVE)?;
                        deleted += 1;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => {
                        return Err(format!("could not delete {}: {error}", absolute.display()));
                    }
                }
            }
        }
    }
    crate::fault::trip(crate::fault::point::BEFORE_VERIFY)?;
    verify_after(root, bundle)?;
    Ok(Execution {
        schema: "jails.execution.v1".to_string(),
        plan_digest: bundle.plan.digest.clone(),
        operations: bundle.plan.operations.len(),
        files_written: written,
        files_deleted: deleted,
    })
}

/// Refuse before the first write if any destination cannot be written.
///
/// **`bugs.md` B18, closed at the only point that can close it.** The
/// operation list is applied in order, so an unwritable *later* destination
/// used to leave the earlier ones published: a read-only migration directory
/// took `resource field add` from "add a column" to a managed tree whose
/// insert names a column no migration creates, with the lock still describing
/// the tree before it -- so `doctor` called jails' own output a reader edit
/// and reported all clear. The transition converges when it is run again, but
/// nobody knows to run it again.
///
/// One probe per directory, staged and dropped, because the question is
/// exactly the one `write_atomic` asks: not "what do the mode bits say", which
/// answers about the wrong subject on every unusual filesystem, but "can this
/// process create a file here right now".
fn preflight_writable(root: &Path, bundle: &PlanBundle) -> Result<(), String> {
    fn parent_of(path: &ProjectPath) -> Option<std::path::PathBuf> {
        std::path::Path::new(path.as_str())
            .parent()
            .map(std::path::Path::to_path_buf)
    }
    let mut parents = BTreeSet::new();
    for operation in &bundle.plan.operations {
        match operation {
            PlannedOperation::PublishMergedTree {
                root: managed_root,
                after,
                ..
            } => {
                let Some(tree) = bundle.trees.get(after) else {
                    continue;
                };
                for path in tree.entries.keys() {
                    parents.extend(parent_of(&managed_root.join(path.as_str())?));
                }
                parents.insert(std::path::PathBuf::from(managed_root.as_str()));
            }
            PlannedOperation::ReplaceModelFile { path, .. }
            | PlannedOperation::ReplaceStateFile { path, .. }
            | PlannedOperation::PatchReaderFile { path, .. }
            | PlannedOperation::AppendMigration { path, .. }
            | PlannedOperation::RemoveReaderFile { path, .. } => {
                parents.extend(parent_of(path));
            }
        }
    }
    for parent in parents {
        let absolute = root.join(&parent);
        std::fs::create_dir_all(&absolute)
            .map_err(|error| format!("could not create {}: {error}", absolute.display()))?;
        tempfile::Builder::new()
            .prefix(STAGED_PREFIX)
            .tempfile_in(&absolute)
            .map_err(|error| {
                format!(
                    "could not stage into {}: {error}\n       fix: make the directory writable, then run the same command again -- nothing was written",
                    absolute.display()
                )
            })?;
    }
    Ok(())
}

/// The prefix [`write_atomic`] stages under, and the reason it is not
/// `tempfile`'s default `.tmp`.
///
/// A staged file is jails', and after a crash between staging and rename it is
/// the only thing in a project that looks like a reader's file but is not.
/// Nothing else may recognise one, so it says whose it is.
const STAGED_PREFIX: &str = ".jails-staged-";

/// Delete the debris a run that died between staging and rename left behind.
///
/// **Found by `tests/crash.rs`, and only by its aborting half.** An injected
/// `Err` unwinds, so the staged `NamedTempFile`'s guard removes it; an
/// `abort()` does not, and the file stays. `verify_preconditions` then reads
/// it as an unmanaged file that appeared inside the managed tree and refuses
/// -- permanently, because nothing removes it and every later plan refuses
/// the same way. A project wedged by its own temporary file is the exact
/// opposite of "the next identical generation repairs it deterministically",
/// which is what this executor trades rollback away for.
///
/// It runs under the lock, so no other run has a staged file in flight here:
/// anything matching is a dead run's, never a live one's.
fn sweep_staged(root: &Path, bundle: &PlanBundle) -> Result<(), String> {
    let managed = managed_roots(bundle);
    for managed_root in &managed {
        let absolute = root.join(managed_root.as_str());
        if absolute.exists() {
            sweep_staged_under(&absolute)?;
        }
    }
    for path in desired_files(bundle)?.keys() {
        if managed.iter().any(|managed| path.is_within(managed)) {
            continue;
        }
        if let Some(parent) = root.join(path.as_str()).parent() {
            sweep_staged_in(parent)?;
        }
    }
    Ok(())
}

fn sweep_staged_under(directory: &Path) -> Result<(), String> {
    for entry in std::fs::read_dir(directory)
        .map_err(|error| format!("could not read {}: {error}", directory.display()))?
    {
        let path = entry
            .map_err(|error| format!("could not read {}: {error}", directory.display()))?
            .path();
        if path.is_dir() {
            sweep_staged_under(&path)?;
        } else if is_staged(&path) {
            remove_staged(&path)?;
        }
    }
    Ok(())
}

fn sweep_staged_in(directory: &Path) -> Result<(), String> {
    if !directory.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(directory)
        .map_err(|error| format!("could not read {}: {error}", directory.display()))?
    {
        let path = entry
            .map_err(|error| format!("could not read {}: {error}", directory.display()))?
            .path();
        if path.is_file() && is_staged(&path) {
            remove_staged(&path)?;
        }
    }
    Ok(())
}

fn is_staged(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with(STAGED_PREFIX))
}

fn remove_staged(path: &Path) -> Result<(), String> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!(
            "could not clear staged {}: {error}",
            path.display()
        )),
    }
}

fn verify_preconditions(root: &Path, bundle: &PlanBundle) -> Result<(), String> {
    let desired = desired_files(bundle)?;
    let removed = removed_files(bundle);
    let managed_roots = managed_roots(bundle);
    for (path, expected) in &bundle.plan.base.files {
        let actual = actual_file(root, path)?;
        if actual_matches_precondition(actual.as_ref(), expected) {
            continue;
        }
        if actual.is_none() && removed.contains(path) {
            continue;
        }
        if actual_matches_desired(actual.as_ref(), desired.get(path)) {
            continue;
        }
        // **Say which way it diverged.** "no longer matches" is true of a file
        // that appeared, one that vanished and one that was edited, and the
        // three have different causes -- a concurrent command, a deletion, a
        // hand edit between plan and apply. Naming the direction is what turns
        // this from a puzzle into a sentence.
        let observed = match (&actual, expected) {
            (None, FilePrecondition::Present { .. }) => "it is gone",
            (Some(_), FilePrecondition::Missing) => "it was created after the plan",
            _ => "its bytes changed after the plan",
        };
        return Err(format!(
            "stale exact plan: `{path}` no longer matches its captured precondition -- {observed}"
        ));
    }
    for (path, expected) in &desired {
        if bundle.plan.base.files.contains_key(path) {
            continue;
        }
        let actual = actual_file(root, path)?;
        if actual.is_none() || actual_matches_desired(actual.as_ref(), Some(expected)) {
            continue;
        }
        return Err(format!(
            "stale exact plan: new managed path `{path}` appeared after planning"
        ));
    }
    for managed_root in &managed_roots {
        for actual in existing_files(root, managed_root)? {
            if !bundle.plan.base.files.contains_key(&actual) && !desired.contains_key(&actual) {
                return Err(format!(
                    "stale exact plan: unmanaged file `{actual}` appeared inside the managed tree"
                ));
            }
        }
    }
    for (path, expected) in &bundle.plan.base.directories {
        let actual = crate::capture::observe_directory(root, path)?;
        if actual == *expected || directory_is_plan_prefix(root, path, bundle)? {
            continue;
        }
        return Err(format!(
            "stale exact plan: directory `{path}` no longer matches its captured precondition"
        ));
    }
    Ok(())
}

fn directory_is_plan_prefix(
    root: &Path,
    directory: &ProjectPath,
    bundle: &PlanBundle,
) -> Result<bool, String> {
    let actual = existing_files(root, directory)?;
    let base = bundle
        .plan
        .base
        .files
        .iter()
        .filter_map(|(path, precondition)| {
            (path.is_within(directory) && matches!(precondition, FilePrecondition::Present { .. }))
                .then_some(path.clone())
        })
        .collect::<BTreeSet<_>>();
    let planned = bundle
        .plan
        .operations
        .iter()
        .filter_map(|operation| match operation {
            PlannedOperation::AppendMigration { path, .. } if path.is_within(directory) => {
                Some(path.clone())
            }
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    if !base.is_subset(&actual)
        || !actual
            .iter()
            .all(|path| base.contains(path) || planned.contains(path))
    {
        return Ok(false);
    }
    let desired = desired_files(bundle)?;
    for path in &actual {
        let observed = actual_file(root, path)?;
        if let Some(expected) = desired.get(path) {
            if !actual_matches_desired(observed.as_ref(), Some(expected)) {
                return Ok(false);
            }
        } else if let Some(expected) = bundle.plan.base.files.get(path) {
            if !actual_matches_precondition(observed.as_ref(), expected) {
                return Ok(false);
            }
        } else {
            return Ok(false);
        }
    }
    Ok(true)
}

fn publish_merged_tree(
    root: &Path,
    managed_root: &ProjectPath,
    desired: &TreeManifest,
    bundle: &PlanBundle,
) -> Result<(usize, usize), String> {
    let mut written = 0_usize;
    for (path, entry) in &desired.entries {
        if !path.is_within(managed_root) {
            return Err(format!(
                "tree entry `{path}` escaped managed root `{managed_root}`"
            ));
        }
        let bytes = bundle
            .blobs
            .get(&entry.blob)
            .ok_or_else(|| format!("tree entry `{path}` references a missing blob"))?;
        let actual = actual_file(root, path)?;
        if actual
            .as_ref()
            .is_some_and(|actual| actual.digest == entry.blob && actual.mode == entry.mode)
        {
            continue;
        }
        write_atomic(root, path, bytes, entry.mode)?;
        written += 1;
    }

    let desired_paths = desired.entries.keys().cloned().collect::<BTreeSet<_>>();
    let mut deleted = 0_usize;
    for path in existing_files(root, managed_root)? {
        if desired_paths.contains(&path) {
            continue;
        }
        let absolute = root.join(path.as_str());
        crate::fault::trip(crate::fault::point::BEFORE_REMOVE)?;
        std::fs::remove_file(&absolute)
            .map_err(|error| format!("could not delete {}: {error}", absolute.display()))?;
        crate::fault::trip(crate::fault::point::AFTER_REMOVE)?;
        deleted += 1;
    }
    remove_empty_directories(&root.join(managed_root.as_str()))?;
    Ok((written, deleted))
}

fn verify_after(root: &Path, bundle: &PlanBundle) -> Result<(), String> {
    let desired = desired_files(bundle)?;
    for (path, expected) in &desired {
        let actual = actual_file(root, path)?;
        if !actual_matches_desired(actual.as_ref(), Some(expected)) {
            return Err(format!(
                "executor did not publish exact after-image `{path}`"
            ));
        }
    }
    for managed_root in managed_roots(bundle) {
        let actual = existing_files(root, &managed_root)?;
        let expected = desired
            .keys()
            .filter(|path| path.is_within(&managed_root))
            .cloned()
            .collect::<BTreeSet<_>>();
        if actual != expected {
            return Err(format!(
                "executor did not converge managed tree `{managed_root}`"
            ));
        }
    }
    for path in removed_files(bundle) {
        if actual_file(root, &path)?.is_some() {
            return Err(format!("executor did not remove reader source `{path}`"));
        }
    }
    Ok(())
}

#[derive(Clone)]
struct DesiredFile {
    digest: ContentDigest,
    mode: FileMode,
}

fn desired_files(bundle: &PlanBundle) -> Result<BTreeMap<ProjectPath, DesiredFile>, String> {
    let mut files = BTreeMap::new();
    for operation in &bundle.plan.operations {
        match operation {
            PlannedOperation::PublishMergedTree { after, .. } => {
                let tree = bundle
                    .trees
                    .get(after)
                    .ok_or_else(|| format!("plan references missing tree `{}`", after.as_str()))?;
                for (path, entry) in &tree.entries {
                    files.insert(
                        path.clone(),
                        DesiredFile {
                            digest: entry.blob.clone(),
                            mode: entry.mode,
                        },
                    );
                }
            }
            PlannedOperation::ReplaceModelFile { path, after, .. }
            | PlannedOperation::ReplaceStateFile { path, after, .. }
            | PlannedOperation::AppendMigration { path, after }
            | PlannedOperation::PatchReaderFile { path, after, .. } => {
                files.insert(
                    path.clone(),
                    DesiredFile {
                        digest: after.blob.clone(),
                        mode: after.mode,
                    },
                );
            }
            PlannedOperation::RemoveReaderFile { .. } => {}
        }
    }
    Ok(files)
}

fn removed_files(bundle: &PlanBundle) -> BTreeSet<ProjectPath> {
    bundle
        .plan
        .operations
        .iter()
        .filter_map(|operation| match operation {
            PlannedOperation::RemoveReaderFile { path, .. } => Some(path.clone()),
            _ => None,
        })
        .collect()
}

fn managed_roots(bundle: &PlanBundle) -> Vec<ProjectPath> {
    bundle
        .plan
        .operations
        .iter()
        .filter_map(|operation| match operation {
            PlannedOperation::PublishMergedTree { root, .. } => Some(root.clone()),
            _ => None,
        })
        .collect()
}

struct ActualFile {
    digest: ContentDigest,
    mode: FileMode,
}

fn actual_file(root: &Path, path: &ProjectPath) -> Result<Option<ActualFile>, String> {
    let absolute = root.join(path.as_str());
    let metadata = match std::fs::symlink_metadata(&absolute) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("could not inspect {}: {error}", absolute.display())),
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(format!("`{}` is not a regular file", absolute.display()));
    }
    let bytes = std::fs::read(&absolute)
        .map_err(|error| format!("could not read {}: {error}", absolute.display()))?;
    Ok(Some(ActualFile {
        digest: digest(&bytes)?,
        mode: mode(&metadata),
    }))
}

fn actual_matches_precondition(actual: Option<&ActualFile>, expected: &FilePrecondition) -> bool {
    match (actual, expected) {
        (None, FilePrecondition::Missing) => true,
        (Some(actual), FilePrecondition::Present { digest, executable }) => {
            actual.digest == *digest && (actual.mode == FileMode::Executable) == *executable
        }
        _ => false,
    }
}

fn actual_matches_desired(actual: Option<&ActualFile>, expected: Option<&DesiredFile>) -> bool {
    match (actual, expected) {
        (None, None) => true,
        (Some(actual), Some(expected)) => {
            actual.digest == expected.digest && actual.mode == expected.mode
        }
        _ => false,
    }
}

fn write_atomic(
    root: &Path,
    path: &ProjectPath,
    bytes: &[u8],
    mode: FileMode,
) -> Result<(), String> {
    let destination = root.join(path.as_str());
    let parent = destination
        .parent()
        .ok_or_else(|| format!("managed path `{path}` has no parent"))?;
    std::fs::create_dir_all(parent)
        .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
    let mut staged = tempfile::Builder::new()
        .prefix(STAGED_PREFIX)
        .tempfile_in(parent)
        .map_err(|error| format!("could not stage beside {}: {error}", destination.display()))?;
    staged
        .write_all(bytes)
        .and_then(|()| staged.flush())
        .map_err(|error| format!("could not stage {}: {error}", destination.display()))?;
    set_mode(staged.path(), mode)?;
    staged
        .as_file()
        .sync_all()
        .map_err(|error| format!("could not sync staged {}: {error}", destination.display()))?;
    crate::fault::trip(crate::fault::point::BEFORE_FILE)?;
    staged.persist(&destination).map_err(|error| {
        format!(
            "could not publish staged {}: {}",
            destination.display(),
            error.error
        )
    })?;
    crate::fault::trip(crate::fault::point::AFTER_FILE)?;
    Ok(())
}

fn existing_files(root: &Path, managed: &ProjectPath) -> Result<BTreeSet<ProjectPath>, String> {
    let mut files = BTreeSet::new();
    let absolute = root.join(managed.as_str());
    if absolute.exists() {
        walk_files(root, &absolute, &mut files)?;
    }
    Ok(files)
}

fn walk_files(
    root: &Path,
    directory: &Path,
    files: &mut BTreeSet<ProjectPath>,
) -> Result<(), String> {
    for entry in std::fs::read_dir(directory)
        .map_err(|error| format!("could not read {}: {error}", directory.display()))?
    {
        let path = entry
            .map_err(|error| format!("could not read {}: {error}", directory.display()))?
            .path();
        let metadata = std::fs::symlink_metadata(&path)
            .map_err(|error| format!("could not inspect {}: {error}", path.display()))?;
        if metadata.is_dir() && !metadata.file_type().is_symlink() {
            walk_files(root, &path, files)?;
        } else if metadata.is_file() && !metadata.file_type().is_symlink() {
            let relative = path
                .strip_prefix(root)
                .map_err(|_| format!("{} escaped project root", path.display()))?;
            files.insert(ProjectPath::parse(path_text(relative))?);
        } else {
            return Err(format!(
                "managed output `{}` is not a regular file",
                path.display()
            ));
        }
    }
    Ok(())
}

fn remove_empty_directories(root: &Path) -> Result<(), String> {
    if !root.is_dir() {
        return Ok(());
    }
    let mut directories = Vec::new();
    collect_directories(root, &mut directories)?;
    directories.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    for directory in directories {
        match std::fs::remove_dir(&directory) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::DirectoryNotEmpty => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!(
                    "could not remove empty directory {}: {error}",
                    directory.display()
                ));
            }
        }
    }
    Ok(())
}

fn collect_directories(directory: &Path, directories: &mut Vec<PathBuf>) -> Result<(), String> {
    directories.push(directory.to_path_buf());
    for entry in std::fs::read_dir(directory)
        .map_err(|error| format!("could not read {}: {error}", directory.display()))?
    {
        let path = entry
            .map_err(|error| format!("could not read {}: {error}", directory.display()))?
            .path();
        if path.is_dir() {
            collect_directories(&path, directories)?;
        }
    }
    Ok(())
}

fn path_text(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn digest(bytes: &[u8]) -> Result<ContentDigest, String> {
    ContentDigest::parse(format!("sha256:{}", hex(&sha256(bytes))))
}

#[cfg(unix)]
fn mode(metadata: &std::fs::Metadata) -> FileMode {
    use std::os::unix::fs::PermissionsExt as _;
    if metadata.permissions().mode() & 0o111 == 0 {
        FileMode::Regular
    } else {
        FileMode::Executable
    }
}

#[cfg(not(unix))]
fn mode(_metadata: &std::fs::Metadata) -> FileMode {
    FileMode::Regular
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: FileMode) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt as _;
    let bits = if mode == FileMode::Executable {
        0o755
    } else {
        0o644
    };
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(bits))
        .map_err(|error| format!("could not set mode on {}: {error}", path.display()))
}

#[cfg(not(unix))]
fn set_mode(_path: &Path, _mode: FileMode) -> Result<(), String> {
    Ok(())
}
