//! A new project is absent or complete, never half-written.
//!
//! plan.md §R6.5. `jails new` is the one mutation with no project to lock and
//! no ledger to journal: there is nothing there yet. So the guarantee the rest
//! of the tool gets from the transaction protocol has to be bought a different
//! way, and the cheapest sound way is publication by rename.
//!
//! Everything the command writes — the downloaded starter zip, the pom, the
//! sources, `mise.toml`, `AGENTS.md`, the seeded manifest and whatever
//! `app apply` generates from it — lands in a scratch directory *beside* the
//! destination. The destination itself is created by one `rename`. A curl that
//! 404s, a template that refuses, a `^C` in the middle of an `app apply`: each
//! leaves the destination absent, which is a state the user can act on. The
//! failure this replaces left a directory containing a pom and no sources,
//! which reads exactly like a project.
//!
//! Two details are load-bearing:
//!
//! - **The scratch tree is a sibling, not `/tmp`.** `rename` is atomic only
//!   within one filesystem, and `/tmp` is frequently a different one. Reserving
//!   beside the destination is what makes the last step a rename rather than a
//!   copy.
//! - **The lock is on the parent directory, not on the destination.** A lock
//!   file inside the destination would have to be created before the thing it
//!   guards exists, and would then be part of the project it published. So two
//!   `jails new` runs in one directory serialise on `<parent>/.jails-new.lock`,
//!   which is also what makes the "already exists" check meaningful: it is
//!   rechecked under the lock, so it cannot be overtaken between the check and
//!   the rename.

use jails_support::Result;
use jails_support::lock::Lock;
use jails_support::scratch::ScratchDir;
use std::path::{Path, PathBuf};

/// The name of the advisory lock two concurrent `jails new` runs contend on.
pub(crate) const LOCK_FILE: &str = ".jails-new.lock";

/// The staging tree, and the only way to write into it.
///
/// **This exists so a claim can be checked rather than believed.** `jails new`
/// writes ~33 files with no project to lock and no ledger to journal, which the
/// R6.4 gate counted as mutations that bypass the executor -- and they are not.
/// Every one lands inside a reserved scratch that is published by a single
/// `rename` or discarded entire, which is the same guarantee the executor
/// gives, bought the way this module's header describes.
///
/// What was missing was any way to *say* that. `root: &Path` is a path like any
/// other, so nothing distinguished a write into the staging tree from a write
/// into a live project, and the gate could only assume the worst. A `Tree` can
/// be obtained from a [`Publication`] and nowhere else, so a function that takes
/// one is a function that cannot reach a published project. `pending.md` §5.
///
/// The verbs are `apply`'s, unchanged, and deliberately not the full set: a
/// staging tree is jails' own, created empty moments earlier, so there is
/// nothing to preserve and nothing to merge.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Tree<'a> {
    root: &'a Path,
}

impl<'a> Tree<'a> {
    /// A `Tree` over a plain path, for the tests that build one directly.
    #[cfg(test)]
    pub(crate) fn at(root: &'a Path) -> Self {
        Self { root }
    }

    /// The staging root. For the few callers that must read a path rather than
    /// write one -- `git init`, a pom re-read, a `.gitkeep` probe.
    pub(crate) fn root(&self) -> &Path {
        self.root
    }

    /// A path inside this tree.
    pub(crate) fn join(&self, relative: impl AsRef<Path>) -> PathBuf {
        self.root.join(relative)
    }

    pub(crate) fn put(&self, relative: impl AsRef<Path>, contents: impl AsRef<str>) -> Result<()> {
        jails_support::apply::put(self.join(relative), contents)
    }

    pub(crate) fn put_named(
        &self,
        relative: impl AsRef<Path>,
        contents: impl AsRef<str>,
        label: &str,
    ) -> Result<()> {
        jails_support::apply::put_named(self.join(relative), contents, label)
    }

    /// The same verbs for a caller that already holds an absolute path.
    ///
    /// **Containment is checked, not assumed.** A relative path cannot escape
    /// this tree; an absolute one can, and half of `new`'s writes arrive as
    /// absolute paths because the source and test directories are computed once
    /// from the package name and joined many times. So the check happens here:
    /// a write outside the staging tree is a refusal rather than a write, which
    /// turns "`new` only ever writes into a reserved scratch" from a claim into
    /// something the program enforces.
    pub(crate) fn put_at(&self, path: &Path, contents: impl AsRef<str>) -> Result<()> {
        self.inside(path)?;
        jails_support::apply::put(path, contents)
    }

    pub(crate) fn put_named_at(
        &self,
        path: &Path,
        contents: impl AsRef<str>,
        label: &str,
    ) -> Result<()> {
        self.inside(path)?;
        jails_support::apply::put_named(path, contents, label)
    }

    pub(crate) fn put_executable_at(&self, path: &Path, contents: impl AsRef<str>) -> Result<()> {
        self.inside(path)?;
        jails_support::apply::put_executable(path, contents)
    }

    pub(crate) fn ensure_directory_at(&self, path: &Path) -> Result<()> {
        self.inside(path)?;
        jails_support::apply::ensure_directory(path)
    }

    pub(crate) fn remove_at(&self, path: &Path) -> Result<()> {
        self.inside(path)?;
        jails_support::apply::remove(path)
    }

    fn inside(&self, path: &Path) -> Result<()> {
        if path.starts_with(self.root) {
            return Ok(());
        }
        Err(format!(
            "{} is outside the tree `jails new` reserved ({}), so writing it would leave \
             bytes behind that publication cannot take back.\n       fix: this is a bug in \
             `jails new`, not something a project can cause -- please report the command.",
            path.display(),
            self.root.display()
        )
        .into())
    }
}

/// A reserved destination and the scratch tree standing in for it.
#[derive(Debug)]
pub(crate) struct Publication {
    destination: PathBuf,
    staging: PathBuf,
    scratch: ScratchDir,
    /// Held for as long as the publication is unfinished. Dropping it releases
    /// the parent directory to the next `jails new`, which is why it is named
    /// rather than discarded.
    _lock: Lock,
}

impl Publication {
    /// Take the parent lock, confirm the destination is still absent, and
    /// reserve a scratch tree beside it.
    pub(crate) fn reserve(destination: &Path) -> Result<Self> {
        let destination = std::path::absolute(destination)
            .map_err(|error| format!("could not resolve {}: {error}", destination.display()))?;
        let parent = destination
            .parent()
            .ok_or_else(|| format!("{} has no parent directory", destination.display()))?
            .to_path_buf();
        let name = destination
            .file_name()
            .ok_or_else(|| {
                format!(
                    "{} does not name a directory to create",
                    destination.display()
                )
            })?
            .to_os_string();
        if !parent.is_dir() {
            return Err(format!(
                "{} does not exist, so there is nowhere to create {}.\n       \
                 fix: create the parent directory first, or run the command from one that exists.",
                parent.display(),
                name.to_string_lossy()
            )
            .into());
        }

        let lock = Lock::acquire(&parent.join(LOCK_FILE), "jails new").map_err(|contention| {
            format!(
                "cannot create a project in {}: {contention}",
                parent.display()
            )
        })?;

        // Rechecked under the lock. Checked before it too, by the caller, so
        // the common refusal costs no lock file -- but only this one is
        // authority, because between an unlocked check and the rename another
        // run can have published the same name.
        if destination.symlink_metadata().is_ok() {
            return Err(jails_support::Failure::Told(already_exists(&destination)));
        }

        let scratch = ScratchDir::reserve(&parent, ".jails-new")?;
        let staging = scratch.path().join(&name);
        jails_support::apply::ensure_directory(&staging)?;
        Ok(Self {
            destination,
            staging,
            scratch,
            _lock: lock,
        })
    }

    /// This publication's tree, as the only thing `jails new` may write to.
    pub(crate) fn tree(&self) -> Tree<'_> {
        Tree {
            root: &self.staging,
        }
    }

    /// The directory `root()` sits in.
    ///
    /// Exposed for exactly one caller: start.spring.io's archive wraps itself
    /// in a `<name>/` folder (that is what `baseDir` means), so the unpack
    /// destination is the directory that folder should land *in*, not the
    /// project root itself. Downloads land here too, so the zip shares a
    /// filesystem with the project and is swept by the same scratch tree.
    pub(crate) fn enclosure(&self) -> &Path {
        self.scratch.path()
    }

    /// Make the project real, and remove what is left of the scratch tree.
    pub(crate) fn publish(self) -> Result<PathBuf> {
        let Self {
            destination,
            staging,
            scratch,
            _lock,
        } = self;
        jails_support::apply::publish_tree(&staging, &destination)?;
        scratch.close()?;
        Ok(destination)
    }
}

/// The one spelling of this refusal, so the pre-lock check and the recheck
/// under the lock cannot tell the reader two different stories.
pub(crate) fn already_exists(destination: &Path) -> String {
    format!(
        "{} already exists.\n       \
         fix: pick another name, or remove it first. jails will not write into a directory it \
         did not create.",
        destination.display()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reserved_destination_stays_absent_until_it_is_published() {
        let parent = ScratchDir::in_temp("publish-absent").unwrap();
        let destination = parent.path().join("demo");
        let publication = Publication::reserve(&destination).unwrap();
        jails_support::apply::put(publication.tree().root().join("pom.xml"), "<project/>").unwrap();

        assert!(
            !destination.exists(),
            "the destination is absent while the project is being built"
        );
        publication.publish().unwrap();
        assert!(destination.join("pom.xml").is_file());
    }

    #[test]
    fn an_abandoned_publication_leaves_nothing_behind() {
        let parent = ScratchDir::in_temp("publish-abandoned").unwrap();
        let destination = parent.path().join("demo");
        {
            let publication = Publication::reserve(&destination).unwrap();
            jails_support::apply::put(publication.tree().root().join("pom.xml"), "<project/>")
                .unwrap();
        }
        assert!(!destination.exists());
        let leftovers: Vec<_> = std::fs::read_dir(parent.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name != LOCK_FILE)
            .collect();
        assert!(
            leftovers.is_empty(),
            "a dropped publication removes its scratch tree, found {leftovers:?}"
        );
    }

    #[test]
    fn a_second_run_in_the_same_directory_is_refused_rather_than_queued() {
        let parent = ScratchDir::in_temp("publish-contended").unwrap();
        let held = Publication::reserve(&parent.path().join("first")).unwrap();
        let message = Publication::reserve(&parent.path().join("second")).unwrap_err();
        assert!(
            message.contains("holds the lock"),
            "contention names the holder, got {message}"
        );
        drop(held);
        Publication::reserve(&parent.path().join("second")).unwrap();
    }

    #[test]
    fn an_existing_destination_is_refused_under_the_lock() {
        let parent = ScratchDir::in_temp("publish-existing").unwrap();
        let destination = parent.path().join("demo");
        jails_support::apply::ensure_directory(&destination).unwrap();
        let message = Publication::reserve(&destination).unwrap_err();
        assert!(message.contains("already exists"), "got {message}");
    }

    #[test]
    fn a_missing_parent_is_named_rather_than_created() {
        let parent = ScratchDir::in_temp("publish-missing-parent").unwrap();
        let message = Publication::reserve(&parent.path().join("absent/demo")).unwrap_err();
        assert!(
            message.contains("does not exist"),
            "the refusal names the missing parent, got {message}"
        );
    }
}
