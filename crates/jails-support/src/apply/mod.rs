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
//! helper" is a convention instead of a boundary. `tests/architecture/`
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
use std::io::Write;
use std::path::{Path, PathBuf};

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

/// Write a file jails renders once from an authority outside the model.
///
/// **A named category, not a loophole.** Every managed artifact goes through
/// `jails-workspace::execute`, which is the only canonical project writer:
/// it locks, rechecks preconditions and publishes exact after-images, and the
/// model is what tells it which files exist. These do not have a model entry
/// and never will -- their authority is somewhere else entirely, and nothing
/// reconciles them because there is nothing to reconcile against:
///
/// - `jails adopt` writes a `[layout]` table in `jails.toml`. That is
///   *configuration jails reads*, which is the opposite of a thing jails owns
///   and would later take away.
/// - `jails modernize` bumps versions in the reader's build file.
/// - `jails contract emit --out` projects a document out of source that
///   already exists.
/// - `jails sql generate` renders adapters for queries declared in the
///   reader's `.sql` manifest.
///
/// Re-running is how each is refreshed, which is exactly why a transaction
/// would buy nothing: there is no accepted state for the next run to diverge
/// from. The name is deliberately long for [`put_outside_project`]'s reason --
/// nothing that writes a *managed* artifact should reach one by accident.
pub fn put_one_shot(path: impl AsRef<Path>, contents: impl AsRef<str>) -> Result<()> {
    put(path, contents)
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

/// Atomically publish a private binary artifact outside project authority.
///
/// Prepared plans are the first caller: writing one is explicitly requested,
/// but it is not a project operation and must never appear in the transaction
/// it describes. The temporary file is user-only from creation, synced before
/// rename, and the parent is synced after publication.
pub fn put_outside_project_private_atomic(
    path: impl AsRef<Path>,
    contents: impl AsRef<[u8]>,
) -> Result<()> {
    use std::io::Write as _;

    let (path, contents) = (path.as_ref(), contents.as_ref());
    let parent = path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|error| {
        format!(
            "failed to create private artifact directory `{}`: {error}.\n       \
             fix: choose a writable destination directory.",
            parent.display()
        )
    })?;
    let file_name = path.file_name().ok_or_else(|| {
        format!(
            "private artifact path `{}` has no file name.\n       \
             fix: choose a path ending in a file name.",
            path.display()
        )
    })?;
    let mut suffix = 0u32;
    let mut temporary;
    let mut file = loop {
        temporary = parent.join(format!(
            ".{}.{}.{}.tmp",
            file_name.to_string_lossy(),
            std::process::id(),
            suffix
        ));
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&temporary) {
            Ok(file) => break file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                suffix = suffix.checked_add(1).ok_or(concat!(
                    "too many private artifact temporary files.\n       ",
                    "fix: remove stale temporary files beside the destination."
                ))?;
            }
            Err(error) => {
                return Err(format!(
                    "failed to create private artifact temporary file `{}`: {error}.\n       \
                     fix: choose a writable destination and retry.",
                    temporary.display()
                )
                .into());
            }
        }
    };
    let result = (|| -> Result<()> {
        file.write_all(contents).map_err(|error| {
            format!(
                "failed to write private artifact `{}`: {error}.\n       \
                 fix: free disk space or choose another destination.",
                path.display()
            )
        })?;
        file.sync_all().map_err(|error| {
            format!(
                "failed to sync private artifact `{}`: {error}.\n       \
                 fix: choose a destination on a writable local filesystem.",
                path.display()
            )
        })?;
        fs::rename(&temporary, path).map_err(|error| {
            format!(
                "failed to publish private artifact `{}`: {error}.\n       \
                 fix: choose a destination on the same writable filesystem.",
                path.display()
            )
        })?;
        #[cfg(unix)]
        fs::File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| {
                format!(
                    "failed to sync private artifact directory `{}`: {error}.\n       \
                     fix: choose a destination on a writable local filesystem.",
                    parent.display()
                )
            })?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
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

/// Create a directory outside any project, on the machine.
///
/// [`put_outside_project`]'s counterpart, and long for the same reason: the
/// caller is a cache or a config directory under the user's home, and nothing
/// that edits a *project* should reach for it.
pub fn ensure_directory_outside_project(path: impl AsRef<Path>) -> Result<()> {
    ensure_directory(path)
}

/// Create the user-only directory for disposable daemon process state.
///
/// `.jails/run` is explicitly not project authority: it holds sockets,
/// authentication cookies, and generated process sources that disappear when
/// the daemon stops. Keeping this path checked and its permissions here makes
/// that exception narrower than a general write inside `.jails`.
pub fn ensure_runtime_directory(project: &Path) -> Result<PathBuf> {
    let jails = project.join(".jails");
    let run = project.join(".jails/run");
    for path in [&jails, &run] {
        match path.symlink_metadata() {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(format!(
                    "{} is a symlink and cannot hold authenticated process state\n       fix: replace it with a directory owned by this project",
                    path.display()
                )
                .into());
            }
            Ok(metadata) if !metadata.is_dir() => {
                return Err(format!(
                    "{} is not a directory and cannot hold process state\n       fix: move it aside and retry",
                    path.display()
                )
                .into());
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!(
                    "failed to inspect {}: {error}\n       fix: restore access to the project's `.jails` directory and retry",
                    path.display()
                )
                .into());
            }
        }
    }
    ensure_directory(&run)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&run, fs::Permissions::from_mode(0o700))
            .map_err(|error| format!("failed to protect {}: {error}", run.display()))?;
    }
    Ok(run)
}

/// Atomically publish one user-only file directly beneath `.jails/run`.
pub fn put_runtime_state(project: &Path, path: &Path, contents: &[u8]) -> Result<()> {
    let run = project.join(".jails/run");
    runtime_child(&run, path)?;
    let run = ensure_runtime_directory(project)?;
    let unique = format!("tmp.{}.{}", std::process::id(), monotonic_nonce());
    let temp = run.join(unique);
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let publish = (|| -> Result<()> {
        let mut file = options
            .open(&temp)
            .map_err(|error| format!("failed to create {}: {error}", temp.display()))?;
        file.write_all(contents)
            .map_err(|error| format!("failed to write {}: {error}", temp.display()))?;
        file.sync_all()
            .map_err(|error| format!("failed to sync {}: {error}", temp.display()))?;
        fs::rename(&temp, path)
            .map_err(|error| format!("failed to publish {}: {error}", path.display()).into())
    })();
    if publish.is_err() {
        let _ = fs::remove_file(&temp);
    }
    publish?;
    sync_directory(&run)
}

/// Remove one socket or file directly beneath `.jails/run`; absence succeeds.
pub fn remove_runtime_state(project: &Path, path: &Path) -> Result<()> {
    let jails = project.join(".jails");
    let run = project.join(".jails/run");
    runtime_child(&run, path)?;
    for authority in [&jails, &run] {
        match authority.symlink_metadata() {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(format!(
                    "{} is a symlink and cannot hold authenticated process state\n       fix: replace it with a directory owned by this project",
                    authority.display()
                )
                .into());
            }
            Ok(metadata) if !metadata.is_dir() => {
                return Err(format!(
                    "{} is not a directory and cannot hold process state\n       fix: move it aside and retry",
                    authority.display()
                )
                .into());
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => {
                return Err(format!(
                    "failed to inspect {}: {error}\n       fix: restore access to the project's `.jails` directory and retry",
                    authority.display()
                )
                .into());
            }
        }
    }
    remove(path)
}

fn runtime_child(run: &Path, path: &Path) -> Result<()> {
    if path.parent() == Some(run) && path.file_name().is_some() {
        return Ok(());
    }
    Err(format!(
        "{} is not a direct child of {} and cannot be process state\n       fix: keep daemon state directly beneath `.jails/run`",
        path.display(),
        run.display()
    )
    .into())
}

fn monotonic_nonce() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// The build tool's output directory, which is not the project.
///
/// **Derived output is deliberately outside the transaction.** `target/` and
/// `build/` are Maven's and Gradle's, nothing in the ledger claims a byte of
/// them, and a transition that rewrote one would be claiming ownership of
/// something jails does not own. `dispatch::drop_compiled_shadows` says the
/// same thing from the other end: the compiled shadow of a deleted source has
/// to go, and it goes *after* the commit rather than inside it.
///
/// So these two verbs exist to make that claim checkable instead of implied.
/// A `remove` on a path under `target/` and a `remove` on a path under `src/`
/// are the same call today, which is why the R6.4 gate had to count both;
/// naming the first separately is what lets it stop counting, and the refusal
/// below is what stops the name being a lie. `pending.md` §7.7.
fn derived(path: &Path) -> Result<()> {
    if path
        .components()
        .any(|part| matches!(part.as_os_str().to_str(), Some("target" | "build")))
    {
        return Ok(());
    }
    Err(format!(
        "{} is not build output, so it may not be written outside a transaction.\n       \
         fix: this is a bug in jails, not something a project can cause -- please report the \
         command.",
        path.display()
    )
    .into())
}

/// Remove a file the build tool derived, once its source is gone.
pub fn remove_derived(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    derived(path)?;
    remove(path)
}

/// Create a directory under the build tool's output, for something jails is
/// about to ask the build tool to write there.
pub fn ensure_derived_directory(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    derived(path)?;
    ensure_directory(path)
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
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
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

    #[test]
    fn runtime_state_is_private_atomic_and_directly_beneath_run() {
        let project = scratch();
        let path = project.join(".jails/run/testd-v2.meta");
        put_runtime_state(&project, &path, b"first\n").unwrap();
        put_runtime_state(&project, &path, b"second\n").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"second\n");

        #[cfg(unix)]
        {
            assert_eq!(
                fs::metadata(project.join(".jails/run"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o700
            );
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }

        remove_runtime_state(&project, &path).unwrap();
        assert!(!path.exists());
        remove_runtime_state(&project, &path).unwrap();
    }

    #[test]
    fn runtime_state_refuses_nested_and_external_paths() {
        let project = scratch();
        let nested = project.join(".jails/run/nested/state");
        let external = project.join("state");
        assert!(put_runtime_state(&project, &nested, b"no").is_err());
        assert!(put_runtime_state(&project, &external, b"no").is_err());
        assert!(!nested.exists());
        assert!(!external.exists());
    }

    #[cfg(unix)]
    #[test]
    fn runtime_state_refuses_a_symlinked_authority_directory() {
        use std::os::unix::fs::symlink;

        let project = scratch();
        let outside = scratch();
        symlink(&outside, project.join(".jails")).unwrap();
        let path = project.join(".jails/run/testd-v2.meta");
        let error = put_runtime_state(&project, &path, b"secret").unwrap_err();
        assert!(error.contains("is a symlink"), "{error}");
        assert!(!outside.join("run/testd-v2.meta").exists());
    }

    #[cfg(unix)]
    #[test]
    fn runtime_cleanup_refuses_to_follow_a_symlinked_authority_directory() {
        use std::os::unix::fs::symlink;

        let project = scratch();
        let outside = scratch();
        fs::write(outside.join("testd-v2.meta"), b"keep").unwrap();
        fs::create_dir(project.join(".jails")).unwrap();
        symlink(&outside, project.join(".jails/run")).unwrap();
        let path = project.join(".jails/run/testd-v2.meta");
        let error = remove_runtime_state(&project, &path).unwrap_err();
        assert!(error.contains("is a symlink"), "{error}");
        assert_eq!(fs::read(outside.join("testd-v2.meta")).unwrap(), b"keep");
    }
}

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
/// into a live project, and the gate could only assume the worst. A function
/// that takes a `Tree` is a function that cannot reach a published project --
/// `Tree::inside` refuses a path outside it. `pending.md` §5.
///
/// It lives here rather than beside `jails new` because the generators write
/// into it too: `generate::write_new_file` is called only from the publication
/// path, and taking a `Tree` is what says so in the signature rather than in a
/// comment. `pending.md` §7.7.
///
/// The verbs are `apply`'s, unchanged, and deliberately not the full set: a
/// staging tree is jails' own, created empty moments earlier, so there is
/// nothing to preserve and nothing to merge.
#[derive(Clone, Copy, Debug)]
pub struct Tree<'a> {
    root: &'a Path,
}

impl<'a> Tree<'a> {
    /// A `Tree` over a directory the caller has just reserved for itself.
    pub fn at(root: &'a Path) -> Self {
        Self { root }
    }

    /// The staging root. For the few callers that must read a path rather than
    /// write one -- `git init`, a pom re-read, a `.gitkeep` probe.
    pub fn root(&self) -> &Path {
        self.root
    }

    /// A path inside this tree.
    pub fn join(&self, relative: impl AsRef<Path>) -> PathBuf {
        self.root.join(relative)
    }

    pub fn put(&self, relative: impl AsRef<Path>, contents: impl AsRef<str>) -> Result<()> {
        put(self.join(relative), contents)
    }

    pub fn put_named(
        &self,
        relative: impl AsRef<Path>,
        contents: impl AsRef<str>,
        label: &str,
    ) -> Result<()> {
        put_named(self.join(relative), contents, label)
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
    pub fn put_at(&self, path: &Path, contents: impl AsRef<str>) -> Result<()> {
        self.inside(path)?;
        put(path, contents)
    }

    /// The one verb whose meaning is not "jails owns this": the file must not
    /// already exist. A staging tree is created empty, so this can only fail
    /// when one generator writes over another's output.
    pub fn create_at(&self, path: &Path, contents: impl AsRef<str>) -> Result<()> {
        self.inside(path)?;
        create(path, contents)
    }

    pub fn put_named_at(&self, path: &Path, contents: impl AsRef<str>, label: &str) -> Result<()> {
        self.inside(path)?;
        put_named(path, contents, label)
    }

    pub fn put_executable_at(&self, path: &Path, contents: impl AsRef<str>) -> Result<()> {
        self.inside(path)?;
        put_executable(path, contents)
    }

    pub fn ensure_directory_at(&self, path: &Path) -> Result<()> {
        self.inside(path)?;
        ensure_directory(path)
    }

    pub fn remove_at(&self, path: &Path) -> Result<()> {
        self.inside(path)?;
        remove(path)
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
