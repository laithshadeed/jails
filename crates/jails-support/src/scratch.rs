//! A scratch directory nothing else can be handed, and that cleans up after
//! itself.
//!
//! ## Why this is not four lines of `create_dir_all`
//!
//! It was, and the four lines were wrong in both halves. The name came from a
//! process id plus a nanosecond timestamp, which is not unique: two threads in
//! one process read the same nanosecond, and `create_dir_all` -- whose whole
//! contract is that an existing directory is *success* -- happily handed both
//! of them the same tree. The second `jails g cli Admin` then failed with
//! "already exists" over the first one's files. It reproduced about once in
//! five full-workspace runs.
//!
//! [`tempfile`] creates the directory atomically with OS randomness in the
//! name, so exclusivity is the filesystem's guarantee rather than a hope about
//! clock resolution.
//!
//! ## The three rules
//!
//! - **Never claim a directory that already exists.** `reserve` cannot: it asks
//!   the OS for a fresh one. A caller reaching for `create_dir_all` to "make
//!   sure" the scratch root is there has reintroduced the bug above.
//! - **`Drop` removes only what `tempfile` returned**, never a path assembled
//!   by hand, so a guard can never delete a directory it did not create.
//! - **Cleanup failure is reported on the explicit path.** `Drop` cannot return
//!   an error, so a success path calls [`ScratchDir::close`] and gets one. A
//!   panic still cleans up through `Drop`, silently, which is the right
//!   trade when the process is already failing.

use crate::Result;
use std::path::{Path, PathBuf};

/// An exclusively created directory, removed when this value is dropped.
#[derive(Debug)]
pub struct ScratchDir {
    /// `None` once the directory has been closed or persisted, so `Drop` has
    /// nothing left to remove and cannot double-free it.
    inner: Option<tempfile::TempDir>,
}

impl ScratchDir {
    /// A fresh directory under `parent`, named `<prefix>` plus OS randomness.
    ///
    /// `parent` must already exist; this creates the scratch tree, not the
    /// place to put it.
    pub fn reserve(parent: &Path, prefix: &str) -> Result<Self> {
        let inner = tempfile::Builder::new()
            .prefix(&format!("{prefix}-"))
            .tempdir_in(parent)
            .map_err(|error| {
                format!(
                    "failed to create a scratch directory under {}: {error}",
                    parent.display()
                )
            })?;
        Ok(Self { inner: Some(inner) })
    }

    /// A fresh directory under the machine's temp directory.
    pub fn in_temp(prefix: &str) -> Result<Self> {
        Self::reserve(&std::env::temp_dir(), prefix)
    }

    /// Where the scratch tree is. The only thing this type exposes about it.
    pub fn path(&self) -> &Path {
        self.inner
            .as_ref()
            .expect("a ScratchDir is only consumed by close/keep, both of which take self")
            .path()
    }

    /// Remove it now, and say so if that fails.
    ///
    /// The reason this exists at all: `Drop` swallows the error, and a scratch
    /// tree that could not be removed is a disk leak nobody is told about.
    /// Success paths call this; failure paths let `Drop` do its best.
    pub fn close(mut self) -> Result<()> {
        let Some(inner) = self.inner.take() else {
            return Ok(());
        };
        let path = inner.path().to_path_buf();
        inner.close().map_err(|error| {
            format!(
                "failed to remove scratch directory {}: {error}",
                path.display()
            )
        })
    }

    /// Give up ownership and keep the directory, for recovery storage.
    pub fn keep(mut self) -> PathBuf {
        self.inner
            .take()
            .expect("a ScratchDir is only consumed by close/keep, both of which take self")
            .keep()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// The property the old pid-plus-timestamp naming did not have.
    #[test]
    fn two_reservations_with_one_prefix_are_different_directories() {
        let first = ScratchDir::in_temp("jails-scratch-test").unwrap();
        let second = ScratchDir::in_temp("jails-scratch-test").unwrap();
        assert_ne!(first.path(), second.path());
        assert!(first.path().is_dir() && second.path().is_dir());
        first.close().unwrap();
        second.close().unwrap();
    }

    /// A directory that is already there is never adopted, and never removed.
    /// `create_dir_all` treated "it exists" as success, which is exactly how
    /// one test came to be handed another's tree.
    #[test]
    fn an_existing_directory_is_neither_reused_nor_removed() {
        let parent = ScratchDir::in_temp("jails-scratch-parent").unwrap();
        let occupied = parent.path().join("jails-scratch-test-taken");
        fs::create_dir(&occupied).unwrap();
        let sentinel = occupied.join("evidence.txt");
        fs::write(&sentinel, b"not yours").unwrap();

        for _ in 0..8 {
            let fresh = ScratchDir::reserve(parent.path(), "jails-scratch-test").unwrap();
            assert_ne!(fresh.path(), occupied.as_path());
            fresh.close().unwrap();
        }
        assert_eq!(fs::read(&sentinel).unwrap(), b"not yours");
        parent.close().unwrap();
    }

    /// A panic inside the guarded scope still cleans up -- silently, through
    /// `Drop`, because there is nobody left to report to.
    #[test]
    fn a_panic_inside_the_scope_still_removes_the_tree() {
        let parent = ScratchDir::in_temp("jails-scratch-panic").unwrap();
        let leaked = std::panic::catch_unwind({
            let parent = parent.path().to_path_buf();
            move || {
                let guard = ScratchDir::reserve(&parent, "jails-scratch-test").unwrap();
                let path = guard.path().to_path_buf();
                assert!(path.is_dir());
                panic!("{}", path.display());
            }
        })
        .unwrap_err();
        let path = leaked
            .downcast_ref::<String>()
            .expect("the panic payload carries the path");
        assert!(!Path::new(path).exists(), "{path} outlived its guard");
        parent.close().unwrap();
    }

    /// Removal failure reaches the caller on the explicit path. `Drop` cannot
    /// return one, which is the whole reason `close` exists.
    #[test]
    fn a_failed_removal_is_reported_rather_than_swallowed() {
        let guard = ScratchDir::in_temp("jails-scratch-close").unwrap();
        // Remove it out from under the guard, so `close` finds nothing there.
        fs::remove_dir_all(guard.path()).unwrap();
        let error = guard.close().unwrap_err();
        assert!(
            error.contains("failed to remove scratch directory"),
            "{error}"
        );
    }

    /// `keep` hands the directory over, so `Drop` must not take it back.
    #[test]
    fn keeping_a_directory_survives_the_guard() {
        let parent = ScratchDir::in_temp("jails-scratch-keep").unwrap();
        let kept = ScratchDir::reserve(parent.path(), "jails-scratch-test")
            .unwrap()
            .keep();
        assert!(kept.is_dir(), "{} was removed by Drop", kept.display());
        parent.close().unwrap();
    }
}
