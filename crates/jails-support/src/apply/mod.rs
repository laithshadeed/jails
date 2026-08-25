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
//!
//! ## Removing, and why it took so long to get a verb
//!
//! For a long time this module could only write, so every caller that had to
//! *remove* something reached for `fs::remove_file` directly — twenty-nine
//! sites across generators, capabilities, `rename` and `app apply`. plan.md
//! §R6.4 is what forced the count to be taken: the gate banned one spelling
//! and read green while a dozen other calls mutated projects through other
//! names.
//!
//! The verbs below close that. They are not conveniences; each carries the
//! same kind of belief the writing verbs do:
//!
//! - [`remove`] — jails wrote this file and is taking it back. Absence is
//!   success, because `destroy` after a manual delete is not an error.
//! - [`ensure_directory`] — the parent chain for something about to be
//!   written, made explicit at the call site rather than implied.
//!
//! What is deliberately *not* here is a recursive delete of anything a user
//! might own. A caller that wants a tree gone has to say so path by path.
//!
//! ## Six verbs that used to be here
//!
//! `put_bytes`, `move_file`, `copy_into_scratch`, `remove_managed_tree`,
//! `remove_managed_directory` and `atomically` are gone. They were the V1
//! spellings of moving, copying and rewriting `.jails/` bookkeeping, and each
//! of those is the executor's job now: `jails-commit`'s `activate` moves the
//! bytes and its `store` owns the ledger. Nothing had called any of them for
//! some time, and nothing said so, because `pub` on a library item tells the
//! compiler another crate might — which is the whole reason this crate's API
//! is closed to `pub(crate)` by default.

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
    Ok(fs::create_dir_all(parent)
        .map_err(|error| format!("failed to create {}: {error}", parent.display()))?)
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
        )
        .into());
    }
    ensure_parent(path)?;
    Ok(fs::write(path, contents)
        .map_err(|error| format!("failed to write {}: {error}", path.display()))?)
}

/// Replace a file jails previously wrote.
///
/// Distinct from [`put`] only in what it says about intent: this one is for a
/// capability rewriting its own output, where finding the file absent is a
/// surprise worth not hiding.
pub fn replace(path: impl AsRef<Path>, contents: impl AsRef<str>) -> Result<()> {
    let (path, contents) = (path.as_ref(), contents.as_ref());
    ensure_parent(path)?;
    Ok(fs::write(path, contents)
        .map_err(|error| format!("failed to write {}: {error}", path.display()))?)
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
    Ok(fs::write(path, contents)
        .map_err(|error| format!("failed to write {}: {error}", path.display()))?)
}

/// Write a file that has to be runnable, not merely present.
///
/// A sixth verb rather than a `chmod` at the call site, and for the reason
/// every other verb here exists: what the caller believes about the result.
/// `gradlew` is not a file the project *has*, it is the command the project
/// *is run by* -- `run.rs` prefers it over `gradle` on PATH precisely because
/// it pins the build's own Gradle version, so a `gradlew` written without the
/// executable bit is a wrapper that exists, is found, and cannot be executed.
/// That failure surfaces as "permission denied" from a shell, several steps
/// away from the write that caused it.
///
/// The mode is only applied on Unix. Windows has no permission bit to set and
/// runs `gradlew.bat` instead, so there is nothing to do and nothing to fail.
pub fn put_executable(path: impl AsRef<Path>, contents: impl AsRef<str>) -> Result<()> {
    let (path, contents) = (path.as_ref(), contents.as_ref());
    ensure_parent(path)?;
    fs::write(path, contents)
        .map_err(|error| format!("failed to write {}: {error}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))
            .map_err(|error| format!("failed to make {} executable: {error}", path.display()))?;
    }
    Ok(())
}

/// The same, reported under a short name rather than an absolute path.
///
/// `failed to write pom.xml` is what a person needs; the absolute path of a
/// file in the directory they are standing in is noise.
pub fn put_named(path: impl AsRef<Path>, contents: impl AsRef<str>, label: &str) -> Result<()> {
    let (path, contents) = (path.as_ref(), contents.as_ref());
    ensure_parent(path)?;
    Ok(fs::write(path, contents).map_err(|error| format!("failed to write {label}: {error}"))?)
}

/// Write a file outside the project, on the machine.
///
/// Deliberately long: `jails setup` writes `~/.testcontainers.properties`, and
/// nothing that edits a *project* should ever reach for this by accident.
pub fn put_outside_project(path: impl AsRef<Path>, contents: impl AsRef<str>) -> Result<()> {
    let (path, contents) = (path.as_ref(), contents.as_ref());
    ensure_parent(path)?;
    Ok(fs::write(path, contents)
        .map_err(|error| format!("failed to write {}: {error}", path.display()))?)
}

/// Write a file into a scratch tree jails owns for the duration of one run.
///
/// A verb of its own rather than a general byte-write, for the same reason
/// `put_outside_project` exists: the caller's belief about what is there is
/// different. A scratch tree is jails' own, created empty moments earlier and
/// removed when the run ends, so there is nothing to preserve and nothing to
/// collide with — and nothing that edits a *project* should reach for this.
pub fn put_in_scratch(path: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> Result<()> {
    let path = path.as_ref();
    ensure_parent(path)?;
    Ok(fs::write(path, contents)
        .map_err(|error| format!("failed to write {}: {error}", path.display()))?)
}

/// Remove a file jails wrote.
///
/// An absent file is success. `destroy` after somebody deleted the file by
/// hand is not an error — the requested state is "not there", and it is not
/// there. Reporting a failure would make the ordinary cleanup path noisy and
/// teach people to ignore it.
pub fn remove(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("failed to remove {}: {error}", path.display()).into()),
    }
}

/// Create a directory and its parents.
///
/// Explicit at the call site rather than implied by a write, because a
/// generator that creates a package directory and then fails still created
/// it, and a reader tracing what a command touched should see that.
pub fn ensure_directory(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    Ok(fs::create_dir_all(path)
        .map_err(|error| format!("failed to create {}: {error}", path.display()))?)
}

/// Publish a completed tree by renaming it onto a destination that must be
/// absent, then flushing the directory entry that now names it.
///
/// This is what makes a new project "absent or complete" (plan.md §R6.5).
/// Everything a `jails new` writes goes into a scratch sibling of the
/// destination, so a download that fails, a template that refuses or a
/// killed process leaves nothing behind for the reader to distinguish from a
/// project — and the one step that makes it real is a rename, which the
/// kernel either performs or does not.
///
/// `from` and `to` must share a filesystem, which the caller guarantees by
/// reserving the scratch tree beside the destination rather than in `/tmp`.
/// A cross-device rename is reported rather than papered over with a copy:
/// a copy is not atomic, and silently downgrading to one would give back the
/// half-written tree this exists to prevent.
pub fn publish_tree(from: impl AsRef<Path>, to: impl AsRef<Path>) -> Result<()> {
    let (from, to) = (from.as_ref(), to.as_ref());
    if to.symlink_metadata().is_ok() {
        return Err(format!("{} already exists", to.display()).into());
    }
    fs::rename(from, to).map_err(|error| {
        format!(
            "failed to publish {} as {}: {error}",
            from.display(),
            to.display()
        )
    })?;
    let Some(parent) = to.parent() else {
        return Ok(());
    };
    sync_directory(parent)
}

/// Flush a directory's own entries, so a name that was just created or
/// renamed survives a crash rather than only the bytes it points at.
pub(crate) fn sync_directory(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    Ok(std::fs::File::open(path)
        .and_then(|dir| dir.sync_all())
        .map_err(|error| format!("failed to flush {}: {error}", path.display()))?)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn scratch() -> std::path::PathBuf {
        crate::scratch::ScratchDir::in_temp("jails-apply")
            .unwrap()
            .keep()
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
}
