//! The only canonical project writer.
//!
//! It takes a verified `PlanBundle` and nothing else: locks `.jails/apply.lock`,
//! rechecks every captured precondition against the tree as it is *now*,
//! applies the plan's typed operations in order, and verifies the exact
//! after-images it published. There is no replanning here and no path where a
//! desired byte is computed -- apply never replans, and that is enforced by
//! this module having no compiler and no model to reach for.
//!
//! **It converges rather than rolls back**: no journal, no preimage, no
//! recovery command.
//! A crashed run may leave a temporarily mixed but individually valid tree,
//! and the next identical generation repairs it deterministically. Every
//! `continue` in here is that property — an operation whose after-image is
//! already on disk is skipped, so a re-run reports zero written and zero
//! deleted rather than rewriting a tree it agrees with.
//!
//! `tests/crash.rs` proves it at every point in `fault::POINTS`, in-process
//! and in a child that `abort()`s. The aborting half is what reaches
//! `sweep_staged`: a temporary left by a crash between staging and rename
//! would otherwise wedge the project permanently.
//!
//! The precondition check has three deliberate escape hatches, and they are
//! what makes convergence possible at all: a file already carrying its desired
//! content passes, an absent file that the plan removes passes, and a
//! directory that holds exactly its captured contents plus this plan's own new
//! migrations passes. Without them the second run of an interrupted plan would
//! refuse over its own half-finished work.

use crate::capture::refused_io;
use crate::verify_bundle;
use fs2::FileExt as _;
use jails_contracts::{
    ContentDigest, FileMode, FilePrecondition, PlanBundle, PlannedOperation, ProjectPath,
    TreeManifest,
};
use jails_model::Diagnostic;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write as _;
use std::path::Path;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Execution {
    pub schema: String,
    pub plan_digest: ContentDigest,
    pub operations: usize,
    pub files_written: usize,
    pub files_deleted: usize,
}

pub fn execute(root: &Path, bundle: &PlanBundle) -> Result<Execution, Diagnostic> {
    verify_bundle(bundle)?;
    let jails = root.join(".jails");
    std::fs::create_dir_all(&jails).map_err(|error| refused_io("create", &jails, error))?;
    let lock_path = jails.join("apply.lock");
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)
        .map_err(|error| refused_io("open", &lock_path, error))?;
    lock.lock_exclusive()
        .map_err(|error| refused_io("lock", &lock_path, error))?;
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
                before,
                after,
            } => {
                let tree = bundle.trees.get(after).ok_or_else(|| missing_tree(after))?;
                let previous = before
                    .as_ref()
                    .map(|before| bundle.trees.get(before).ok_or_else(|| missing_tree(before)))
                    .transpose()?;
                crate::fault::trip(crate::fault::point::BEFORE_TREE)?;
                let counts = publish_merged_tree(root, managed_root, previous, tree, bundle)?;
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
                    Diagnostic::without_a_fix(
                        "workspace-after-image-blob-missing",
                        format!("$.blobs.{}", after.blob.as_str()),
                        format!(
                            "model after-image references missing blob `{}`",
                            after.blob.as_str()
                        ),
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
                        return Err(refused_io("delete", &absolute, error));
                    }
                }
            }
        }
    }
    crate::fault::trip(crate::fault::point::BEFORE_VERIFY)?;
    verify_after(root, bundle)?;
    // **After the transaction, not before it.** A reader who commits `.jails/`
    // is committing the model and the lock; `apply.lock` and `run/` are a mutex
    // and a daemon's scratch, and neither belongs in a diff. Writing it here
    // rather than beside `create_dir_all` above keeps the property that a
    // refused plan writes nothing -- the marker appears on the first apply that
    // actually wrote something, and every later one leaves it alone.
    jails_support::apply::mark_state_scratch(root).map_err(|error| {
        Diagnostic::new(
            "workspace-state-not-marked-scratch",
            ".jails/.gitignore",
            error.to_string(),
            "make `.jails` writable, or write the file by hand with `apply.lock` and `run/` in it",
        )
    })?;
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
/// The operation list is applied in order, so an unwritable *later*
/// destination would leave the earlier ones published: a read-only migration
/// directory would take `resource field add` from "add a column" to a managed
/// tree whose insert names a column no migration creates, with the lock still
/// describing the tree before it -- so `doctor` calls jails' own output a
/// reader edit and reports all clear. The transition converges when it is run
/// again, but nobody knows to run it again.
///
/// One probe per directory, staged and dropped, because the question is
/// exactly the one `write_atomic` asks: not "what do the mode bits say", which
/// answers about the wrong subject on every unusual filesystem, but "can this
/// process create a file here right now".
fn preflight_writable(root: &Path, bundle: &PlanBundle) -> Result<(), Diagnostic> {
    fn parent_of(path: &ProjectPath) -> Option<std::path::PathBuf> {
        std::path::Path::new(path.as_str())
            .parent()
            .map(std::path::Path::to_path_buf)
    }
    let mut parents = BTreeSet::new();
    for path in desired_files(bundle)?.keys() {
        parents.extend(parent_of(path));
    }
    // A deletion needs its parent writable too, but a parent that is already
    // gone -- the converged retry after a deletion emptied it -- is not
    // recreated for the probe.
    for path in removed_files(bundle)? {
        parents.extend(parent_of(&path).filter(|parent| root.join(parent).is_dir()));
    }
    for parent in parents {
        let absolute = root.join(&parent);
        std::fs::create_dir_all(&absolute)
            .map_err(|error| refused_io("create", &absolute, error))?;
        tempfile::Builder::new()
            .prefix(STAGED_PREFIX)
            .tempfile_in(&absolute)
            .map_err(|error| {
                Diagnostic::new(
                    "workspace-directory-not-writable",
                    absolute.display().to_string(),
                    format!("could not stage into {}: {error}", absolute.display()),
                    "make the directory writable, then run the same command again -- nothing was written",
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
/// An injected `Err` unwinds, so the staged `NamedTempFile`'s guard removes
/// it; an `abort()` or a lost machine does not, and the file stays. A staged
/// file is written beside its destination, so the sweep reads the parent of
/// every path the plan publishes -- the only directories a dead run of this
/// plan could have staged into -- and nothing else: managed files sit beside
/// the reader's own, and a walk of `src/` would be a walk of theirs. Left in
/// place, a stray temporary in a source root is a file the build reads as
/// theirs and the next capture cannot tell from one.
///
/// It runs under the lock, so no other run has a staged file in flight here:
/// anything matching is a dead run's, never a live one's.
fn sweep_staged(root: &Path, bundle: &PlanBundle) -> Result<(), Diagnostic> {
    let mut parents = BTreeSet::new();
    for path in desired_files(bundle)?.keys() {
        parents.extend(root.join(path.as_str()).parent().map(Path::to_path_buf));
    }
    for parent in parents {
        sweep_staged_in(&parent)?;
    }
    Ok(())
}

fn sweep_staged_in(directory: &Path) -> Result<(), Diagnostic> {
    if !directory.exists() {
        return Ok(());
    }
    for entry in
        std::fs::read_dir(directory).map_err(|error| refused_io("read", directory, error))?
    {
        let path = entry
            .map_err(|error| refused_io("read", directory, error))?
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

fn remove_staged(path: &Path) -> Result<(), Diagnostic> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(refused_io("clear staged", path, error)),
    }
}

/// The one fix for every stale precondition.
///
/// **A plan is a transition out of a project state that no longer exists**,
/// and there is nothing to repair: the answer is to plan again against the
/// tree as it is now. The three refusals below used to say "stale exact plan"
/// and name no next step, which reads as corruption rather than as the race
/// it usually is. Three things produce this condition -- another `jails` run
/// finishing first, an editor saving between planning and applying, and a
/// plan imported with `--plan-in` that was reviewed somewhere else -- and all
/// three take the same action, so the line names them together.
const RUN_IT_AGAIN: &str = "run the command again so it plans against the project as it is now; \
                            another jails run or an editor may have changed it, and a plan \
                            imported with `--plan-in` has to have been exported from this \
                            project";

fn verify_preconditions(root: &Path, bundle: &PlanBundle) -> Result<(), Diagnostic> {
    let desired = desired_files(bundle)?;
    let removed = removed_files(bundle)?;
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
        return Err(Diagnostic::new(
            "workspace-precondition-stale",
            path.to_string(),
            format!(
                "`{path}` no longer matches what this plan was reviewed against -- {observed}. \
                 Nothing was written."
            ),
            RUN_IT_AGAIN,
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
        return Err(Diagnostic::new(
            "workspace-precondition-path-appeared",
            path.to_string(),
            format!(
                "the managed path `{path}` appeared after this plan was reviewed. Nothing was \
                 written."
            ),
            RUN_IT_AGAIN,
        ));
    }
    for (path, expected) in &bundle.plan.base.directories {
        let actual = crate::capture::observe_directory(root, path)?;
        if actual == *expected || directory_is_plan_prefix(root, path, bundle)? {
            continue;
        }
        return Err(Diagnostic::new(
            "workspace-precondition-directory-stale",
            path.to_string(),
            format!(
                "the directory `{path}` no longer matches what this plan was reviewed against. \
                 Nothing was written."
            ),
            RUN_IT_AGAIN,
        ));
    }
    Ok(())
}

fn directory_is_plan_prefix(
    root: &Path,
    directory: &ProjectPath,
    bundle: &PlanBundle,
) -> Result<bool, Diagnostic> {
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

/// Publish the managed set: every entry of `desired`, then the removal of
/// every path `previous` held that `desired` does not.
///
/// **The set is the plan's, never a directory's.** Managed files sit beside
/// the reader's own under `src/`, so what to delete is answered by the two
/// trees in the bundle -- what the accepted projection owned and what the
/// reconciled one does -- and a reader file beside a managed one is never
/// read, let alone removed. A directory a deletion leaves empty goes with it,
/// down to the source-set root, which stays.
fn publish_merged_tree(
    root: &Path,
    managed_root: &ProjectPath,
    previous: Option<&TreeManifest>,
    desired: &TreeManifest,
    bundle: &PlanBundle,
) -> Result<(usize, usize), Diagnostic> {
    let mut written = 0_usize;
    for (path, entry) in &desired.entries {
        if !path.is_within(managed_root) {
            return Err(Diagnostic::without_a_fix(
                "workspace-tree-entry-escaped",
                path.to_string(),
                format!("tree entry `{path}` escaped managed root `{managed_root}`"),
            ));
        }
        let bytes = bundle.blobs.get(&entry.blob).ok_or_else(|| {
            Diagnostic::without_a_fix(
                "workspace-tree-entry-blob-missing",
                path.to_string(),
                format!("tree entry `{path}` references a missing blob"),
            )
        })?;
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

    let mut deleted = 0_usize;
    for path in retired_paths(previous, desired) {
        let absolute = root.join(path.as_str());
        crate::fault::trip(crate::fault::point::BEFORE_REMOVE)?;
        match std::fs::remove_file(&absolute) {
            Ok(()) => {
                crate::fault::trip(crate::fault::point::AFTER_REMOVE)?;
                deleted += 1;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(refused_io("delete", &absolute, error));
            }
        }
        remove_empty_ancestors(root, &path)?;
    }
    Ok((written, deleted))
}

/// The paths a previous tree held and the desired one does not.
fn retired_paths(previous: Option<&TreeManifest>, desired: &TreeManifest) -> Vec<ProjectPath> {
    previous
        .into_iter()
        .flat_map(|tree| tree.entries.keys())
        .filter(|path| !desired.entries.contains_key(*path))
        .cloned()
        .collect()
}

fn verify_after(root: &Path, bundle: &PlanBundle) -> Result<(), Diagnostic> {
    let desired = desired_files(bundle)?;
    for (path, expected) in &desired {
        let actual = actual_file(root, path)?;
        if !actual_matches_desired(actual.as_ref(), Some(expected)) {
            return Err(Diagnostic::without_a_fix(
                "workspace-after-image-not-published",
                path.to_string(),
                format!("executor did not publish exact after-image `{path}`"),
            ));
        }
    }
    for path in removed_files(bundle)? {
        if actual_file(root, &path)?.is_some() {
            return Err(Diagnostic::without_a_fix(
                "workspace-removal-incomplete",
                path.to_string(),
                format!("executor did not remove `{path}`"),
            ));
        }
    }
    Ok(())
}

#[derive(Clone)]
struct DesiredFile {
    digest: ContentDigest,
    mode: FileMode,
}

fn desired_files(bundle: &PlanBundle) -> Result<BTreeMap<ProjectPath, DesiredFile>, Diagnostic> {
    let mut files = BTreeMap::new();
    for operation in &bundle.plan.operations {
        match operation {
            PlannedOperation::PublishMergedTree { after, .. } => {
                let tree = bundle.trees.get(after).ok_or_else(|| missing_tree(after))?;
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

/// Every path this plan removes: reader files it retires and managed files
/// the accepted projection held that the reconciled one does not.
fn removed_files(bundle: &PlanBundle) -> Result<BTreeSet<ProjectPath>, Diagnostic> {
    let mut removed = BTreeSet::new();
    for operation in &bundle.plan.operations {
        match operation {
            PlannedOperation::RemoveReaderFile { path, .. } => {
                removed.insert(path.clone());
            }
            PlannedOperation::PublishMergedTree { before, after, .. } => {
                let desired = bundle.trees.get(after).ok_or_else(|| missing_tree(after))?;
                let previous = before
                    .as_ref()
                    .map(|before| bundle.trees.get(before).ok_or_else(|| missing_tree(before)))
                    .transpose()?;
                removed.extend(retired_paths(previous, desired));
            }
            _ => {}
        }
    }
    Ok(removed)
}

struct ActualFile {
    digest: ContentDigest,
    mode: FileMode,
}

fn actual_file(root: &Path, path: &ProjectPath) -> Result<Option<ActualFile>, Diagnostic> {
    let absolute = root.join(path.as_str());
    let metadata = match std::fs::symlink_metadata(&absolute) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(refused_io("inspect", &absolute, error)),
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(not_a_regular_file(&absolute));
    }
    let bytes = std::fs::read(&absolute).map_err(|error| refused_io("read", &absolute, error))?;
    Ok(Some(ActualFile {
        digest: crate::materialize::digest(&bytes)?,
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
) -> Result<(), Diagnostic> {
    let destination = root.join(path.as_str());
    let parent = destination.parent().ok_or_else(|| {
        Diagnostic::without_a_fix(
            "workspace-managed-path-has-no-parent",
            path.to_string(),
            format!("managed path `{path}` has no parent"),
        )
    })?;
    std::fs::create_dir_all(parent).map_err(|error| refused_io("create", parent, error))?;
    let mut staged = tempfile::Builder::new()
        .prefix(STAGED_PREFIX)
        .tempfile_in(parent)
        .map_err(|error| refused_io("stage beside", &destination, error))?;
    staged
        .write_all(bytes)
        .and_then(|()| staged.flush())
        .map_err(|error| refused_io("stage", &destination, error))?;
    set_mode(staged.path(), mode)?;
    staged
        .as_file()
        .sync_all()
        .map_err(|error| refused_io("sync staged", &destination, error))?;
    crate::fault::trip(crate::fault::point::BEFORE_FILE)?;
    staged
        .persist(&destination)
        .map_err(|error| refused_io("publish staged", &destination, error.error))?;
    crate::fault::trip(crate::fault::point::AFTER_FILE)?;
    Ok(())
}

/// Remove the directories a deleted file leaves empty, up to the root that
/// stays.
///
/// A package directory whose last managed file went is jails' to clean; the
/// source-set root above it (`src/main/java`, `src/test/resources`) and
/// `.jails` itself are the project's shape and stay, empty or not. Anything
/// non-empty ends the ascent, so a reader file beside the deleted one keeps
/// its directory.
fn remove_empty_ancestors(root: &Path, path: &ProjectPath) -> Result<(), Diagnostic> {
    let mut relative = Path::new(path.as_str()).parent();
    while let Some(directory) = relative {
        let text = directory.to_string_lossy();
        if text.is_empty() || keeps_empty_directory(&text) {
            break;
        }
        let absolute = root.join(directory);
        match std::fs::remove_dir(&absolute) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) if error.kind() == std::io::ErrorKind::DirectoryNotEmpty => break,
            Err(error) => {
                return Err(refused_io("remove empty directory", &absolute, error));
            }
        }
        relative = directory.parent();
    }
    Ok(())
}

/// The directories a deletion never removes: `.jails` and the source-set
/// roots -- `src`, `src/<set>` and `src/<set>/<kind>`.
fn keeps_empty_directory(relative: &str) -> bool {
    relative == ".jails"
        || (relative.split('/').count() <= 3 && (relative == "src" || relative.starts_with("src/")))
}

/// Every regular file under one directory, for a directory precondition.
fn existing_files(
    root: &Path,
    directory: &ProjectPath,
) -> Result<BTreeSet<ProjectPath>, Diagnostic> {
    let mut files = BTreeSet::new();
    let absolute = root.join(directory.as_str());
    if absolute.exists() {
        walk_files(root, &absolute, &mut files)?;
    }
    Ok(files)
}

fn walk_files(
    root: &Path,
    directory: &Path,
    files: &mut BTreeSet<ProjectPath>,
) -> Result<(), Diagnostic> {
    for entry in
        std::fs::read_dir(directory).map_err(|error| refused_io("read", directory, error))?
    {
        let path = entry
            .map_err(|error| refused_io("read", directory, error))?
            .path();
        let metadata = std::fs::symlink_metadata(&path)
            .map_err(|error| refused_io("inspect", &path, error))?;
        if metadata.is_dir() && !metadata.file_type().is_symlink() {
            walk_files(root, &path, files)?;
        } else if metadata.is_file() && !metadata.file_type().is_symlink() {
            let relative = path.strip_prefix(root).map_err(|_| {
                Diagnostic::without_a_fix(
                    "workspace-path-escaped-root",
                    path.display().to_string(),
                    format!("{} escaped project root", path.display()),
                )
            })?;
            files.insert(crate::capture::project_path(path_text(relative))?);
        } else {
            return Err(not_a_regular_file(&path));
        }
    }
    Ok(())
}

/// A plan naming a tree its own bundle does not carry, from any of the five
/// places that resolve one.
fn missing_tree(id: &ContentDigest) -> Diagnostic {
    Diagnostic::without_a_fix(
        "workspace-plan-tree-missing",
        format!("$.trees.{}", id.as_str()),
        format!("plan references missing tree `{}`", id.as_str()),
    )
}

/// A path the executor has to read or write that is a directory, a symlink or
/// a device. One site, because it is one refusal reached from two walks.
fn not_a_regular_file(at: &Path) -> Diagnostic {
    Diagnostic::without_a_fix(
        "workspace-not-a-regular-file",
        at.display().to_string(),
        format!("`{}` is not a regular file", at.display()),
    )
}

fn path_text(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
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
fn set_mode(path: &Path, mode: FileMode) -> Result<(), Diagnostic> {
    use std::os::unix::fs::PermissionsExt as _;
    let bits = if mode == FileMode::Executable {
        0o755
    } else {
        0o644
    };
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(bits))
        .map_err(|error| refused_io("set mode on", path, error))
}

#[cfg(not(unix))]
fn set_mode(_path: &Path, _mode: FileMode) -> Result<(), Diagnostic> {
    Ok(())
}
