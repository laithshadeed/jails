//! The only module that writes.
//!
//! Every other module *plans*: it returns a `Change`, a `Vec<Artifact>`, a
//! spliced `String`. This one is where a plan becomes bytes on disk, and it is
//! the only place `fs::write` is allowed to appear.
//!
//! ## Why one module rather than a rule everybody remembers
//!
//! `write_new_file` looked like the single choke point and was not.
//! `src/add.rs` wrote a capability file with a bare `fs::write`, straight past
//! the collision check, and `plan.md` §11 names the consequence exactly: a
//! ledger hung off `write_new_file` alone has a hole precisely where a
//! capability updates a file it previously wrote. That is not a bug anybody
//! introduced carelessly -- it is what happens when "route writes through the
//! helper" is a convention instead of a boundary. `tests/architecture.rs`
//! makes it a boundary by failing when `fs::write` appears anywhere else.
//!
//! ## The four ways to write, and why they are different
//!
//! The distinction is *what the caller believes about the file already there*,
//! because that belief is the whole difference between a safe generator and
//! one that eats work:
//!
//! - [`create`] -- it must not exist. A generator refusing to overwrite is the
//!   property `g scaffold` and `g record` are built on.
//! - [`replace`] -- it exists and jails owns it. Used where a capability
//!   rewrites a file it wrote itself.
//! - [`put`] -- it may or may not exist and the new content already accounts
//!   for whatever was there. Every splice lands here: `pom.xml`,
//!   `compose.yaml`, `application.properties`, `jails.toml`. The *merge* is
//!   the caller's job and happens before this module sees anything, which is
//!   what keeps those byte-preserving splices reviewable.
//! - [`put_outside_project`] -- a machine-level file, deliberately named so it
//!   cannot be reached by accident. `jails setup` writes
//!   `~/.testcontainers.properties`; nothing else should.
//!
//! A caller that cannot say which of the four it means does not yet know what
//! it is doing, which is the point of making it choose.

use crate::Result;
use std::fs;
use std::path::Path;

/// Create the parent directory chain for a file about to be written.
fn ensure_parent(path: &Path) -> Result<()> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    if parent.as_os_str().is_empty() {
        return Ok(());
    }
    fs::create_dir_all(parent)
        .map_err(|error| format!("failed to create {}: {error}", parent.display()))
}

/// Write a file that must not already exist.
///
/// The refusal is the feature: `g scaffold` on a resource that is already
/// there stops rather than replacing hand-edited code, and `g field` prints a
/// snippet instead of clobbering. Callers that have already run their own
/// collision check still come through here, because the check and the write
/// racing is a different bug from the check being absent.
pub fn create(path: impl AsRef<Path>, contents: impl AsRef<str>) -> Result<()> {
    let (path, contents) = (path.as_ref(), contents.as_ref());
    if path.exists() {
        return Err(format!(
            "{} already exists; refusing to overwrite it",
            path.display()
        ));
    }
    ensure_parent(path)?;
    fs::write(path, contents)
        .map_err(|error| format!("failed to write {}: {error}", path.display()))
}

/// Replace a file jails previously wrote.
///
/// Distinct from [`put`] only in what it says about intent: this one is for a
/// capability rewriting its own output, where finding the file absent is a
/// surprise worth not hiding.
pub fn replace(path: impl AsRef<Path>, contents: impl AsRef<str>) -> Result<()> {
    let (path, contents) = (path.as_ref(), contents.as_ref());
    ensure_parent(path)?;
    fs::write(path, contents)
        .map_err(|error| format!("failed to write {}: {error}", path.display()))
}

/// Write content that already accounts for whatever was there.
///
/// This is where every splice lands. `pom.xml`, `compose.yaml`,
/// `application.properties` and `jails.toml` are files people edit, and the
/// rule that protects them -- an edit must be surgical and leave every other
/// byte alone -- is enforced *before* this call, by the module that owns the
/// format. By the time bytes reach here the merge is a decision already taken.
pub fn put(path: impl AsRef<Path>, contents: impl AsRef<str>) -> Result<()> {
    let (path, contents) = (path.as_ref(), contents.as_ref());
    ensure_parent(path)?;
    fs::write(path, contents)
        .map_err(|error| format!("failed to write {}: {error}", path.display()))
}

/// Write bytes rather than text.
///
/// The 3-way merge produces a file that may not be valid UTF-8 and may carry
/// conflict markers: it is whatever `git merge-file` decided, and re-encoding
/// it through `String` would be jails editing a merge result it did not make.
pub fn put_bytes(path: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> Result<()> {
    let path = path.as_ref();
    ensure_parent(path)?;
    fs::write(path, contents)
        .map_err(|error| format!("failed to write {}: {error}", path.display()))
}

/// The same, reported under a short name rather than an absolute path.
///
/// `failed to write pom.xml` is what a person needs; the absolute path of a
/// file in the directory they are standing in is noise.
pub fn put_named(path: impl AsRef<Path>, contents: impl AsRef<str>, label: &str) -> Result<()> {
    let (path, contents) = (path.as_ref(), contents.as_ref());
    ensure_parent(path)?;
    fs::write(path, contents).map_err(|error| format!("failed to write {label}: {error}"))
}

/// Write a file outside the project, on the machine.
///
/// Deliberately long: `jails setup` writes `~/.testcontainers.properties`, and
/// nothing that edits a *project* should ever reach for this by accident.
pub fn put_outside_project(path: impl AsRef<Path>, contents: impl AsRef<str>) -> Result<()> {
    let (path, contents) = (path.as_ref(), contents.as_ref());
    ensure_parent(path)?;
    fs::write(path, contents)
        .map_err(|error| format!("failed to write {}: {error}", path.display()))
}

/// Write via a temporary file and a rename.
///
/// For the bookkeeping under `.jails/`, where a half-written ledger is worse
/// than an absent one: an interrupted `app apply` has to be resumable, and it
/// can only resume from a file that is either the old one or the new one.
pub fn atomically(path: impl AsRef<Path>, contents: impl AsRef<str>) -> Result<()> {
    let (path, contents) = (path.as_ref(), contents.as_ref());
    let parent = path
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", path.display()))?;
    ensure_parent(path)?;
    let temporary = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name().unwrap_or_default().to_string_lossy(),
        std::process::id()
    ));
    fs::write(&temporary, contents)
        .map_err(|error| format!("failed to write {}: {error}", temporary.display()))?;
    fs::rename(&temporary, path).map_err(|error| {
        format!(
            "failed to replace {} with {}: {error}",
            path.display(),
            temporary.display()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);

    fn scratch() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "jails-apply-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn create_refuses_an_existing_file_and_never_touches_it() {
        let dir = scratch();
        let path = dir.join("Thing.java");
        create(&path, "first\n").unwrap();
        let error = create(&path, "second\n").unwrap_err();
        assert!(error.contains("already exists"), "{error}");
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "first\n",
            "a refused write must leave the original byte-for-byte"
        );
    }

    #[test]
    fn create_makes_the_parent_directories_it_needs() {
        let dir = scratch();
        let path = dir.join("src/main/java/com/example/Thing.java");
        create(&path, "body\n").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "body\n");
    }

    #[test]
    fn put_replaces_because_the_merge_already_happened() {
        let dir = scratch();
        let path = dir.join("pom.xml");
        put(&path, "<project/>\n").unwrap();
        put(&path, "<project><dependencies/></project>\n").unwrap();
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "<project><dependencies/></project>\n"
        );
    }

    #[test]
    fn put_named_reports_the_short_name_a_person_recognises() {
        // A directory where the file should be is the easiest way to make the
        // write fail without depending on permissions.
        let dir = scratch();
        let path = dir.join("pom.xml");
        fs::create_dir_all(&path).unwrap();
        let error = put_named(&path, "<project/>", "pom.xml").unwrap_err();
        assert!(error.starts_with("failed to write pom.xml:"), "{error}");
    }

    #[test]
    fn atomically_leaves_no_temporary_behind() {
        let dir = scratch();
        let path = dir.join(".jails/files");
        atomically(&path, "a\nb\n").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "a\nb\n");
        let leftovers: Vec<_> = fs::read_dir(dir.join(".jails"))
            .unwrap()
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
    }
}
