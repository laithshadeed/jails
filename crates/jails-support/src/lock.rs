//! One mutation at a time, and one effect at a time.
//!
//! ## Why an advisory file lock and not a PID file
//!
//! Process ids are reused, so a stale PID file naming a dead process can
//! name a live unrelated one -- and cleaning up a stale file is itself a race
//! between the reader that decided it was stale and the writer that just
//! took it. `flock` has neither problem: the kernel releases it when the
//! holder exits, however it exits.
//!
//! ## Why the lock file is never deleted
//!
//! The lock is on an *inode*, not on a name. Deleting and recreating the file
//! gives the next acquirer a different inode, which means two processes can
//! hold "the lock" at once — each on a file the other cannot see. So the file
//! is created once and kept, and the device/inode of the open handle is
//! compared against the path after acquisition to catch exactly that swap.
//!
//! ## Why contention never waits
//!
//! An invisible wait is indistinguishable from a hang. The second run reports
//! who holds the lock and exits 1, which is a thing a person can act on.

use crate::Result;
use fs2::FileExt;
use std::fs::File;
use std::io::{Read, Seek, Write};
use std::path::{Path, PathBuf};

/// A held lock. Releasing happens when this is dropped, or when the process
/// exits — including when it is killed.
#[derive(Debug)]
pub struct Lock {
    file: File,
    path: PathBuf,
}

/// Why a lock could not be taken.
#[derive(Debug)]
pub enum Contention {
    /// Somebody else holds it. The string is their diagnostic content,
    /// read best-effort — it may be empty or stale, and it is never trusted
    /// for anything but a message.
    Held(String),
    /// Something about the lock file itself is wrong.
    Refused(String),
}

impl std::fmt::Display for Contention {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Held(who) if who.trim().is_empty() => {
                write!(f, "another jails mutation holds the lock")
            }
            Self::Held(who) => write!(f, "another jails mutation holds the lock ({})", who.trim()),
            Self::Refused(why) => f.write_str(why),
        }
    }
}

impl Lock {
    /// Take the lock, or say who has it. Never waits.
    pub fn acquire(path: &Path, description: &str) -> std::result::Result<Self, Contention> {
        let file = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .map_err(|error| {
                Contention::Refused(format!("could not open {}: {error}", path.display()))
            })?;
        set_private(&file, path).map_err(|failure| Contention::Refused(failure.to_string()))?;

        contend(&file, path)?;

        // The lock is on an inode. If the path now names a different one,
        // somebody replaced the file and a second holder is possible.
        same_entry(&file, path).map_err(|failure| Contention::Refused(failure.to_string()))?;

        let mut held = Self {
            file,
            path: path.to_path_buf(),
        };
        held.describe(description)
            .map_err(|failure| Contention::Refused(failure.to_string()))?;
        Ok(held)
    }

    /// Lock an existing directory without creating a lock-file artifact in it.
    ///
    /// This is for publication into a directory that is not itself a jails
    /// project yet. The directory is the stable inode shared by all competing
    /// publishers, so it has the same no-unlink safety property as a retained
    /// lock file without leaving bookkeeping in the user's parent directory.
    pub fn acquire_directory(path: &Path) -> std::result::Result<Self, Contention> {
        let file = File::open(path).map_err(|error| {
            Contention::Refused(format!("could not open {}: {error}", path.display()))
        })?;
        contend(&file, path)?;
        same_entry(&file, path).map_err(|failure| Contention::Refused(failure.to_string()))?;
        Ok(Self {
            file,
            path: path.to_path_buf(),
        })
    }

    /// Replace the diagnostic content. Never authority for anything.
    fn describe(&mut self, description: &str) -> Result<()> {
        let content = format!("pid {}\n{description}\n", std::process::id());
        Ok(self
            .file
            .set_len(0)
            .and_then(|()| self.file.rewind())
            .and_then(|()| self.file.write_all(content.as_bytes()))
            .and_then(|()| self.file.sync_all())
            .map_err(|error| format!("could not describe {}: {error}", self.path.display()))?)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// How long a `EWOULDBLOCK` has to persist before it counts as contention.
///
/// Not a wait for the lock, and not a retry loop around a busy resource: this
/// is how long it takes a *released* lock to actually look released when the
/// process is also spawning children. See `contend`.
const SETTLE: std::time::Duration = std::time::Duration::from_millis(500);
const SETTLE_STEP: std::time::Duration = std::time::Duration::from_millis(2);

/// Take the lock, and say what happened in the caller's vocabulary.
///
/// `flock` reports three different things through one `Err`: somebody else
/// holds it (`EWOULDBLOCK`), the call was interrupted by a signal (`EINTR`),
/// and something is wrong with the file or the filesystem. Reporting all
/// three as "another jails mutation holds the lock" sends the reader looking
/// for a process that does not exist -- and `EINTR` is real here, because
/// jails spawns child processes and their `SIGCHLD` arrives on whichever
/// thread the kernel picks. An interrupted attempt is retried, since nothing
/// about it says the lock is taken.
///
/// ## Why `EWOULDBLOCK` is not immediately believed either
///
/// A lock lives on the *open file description*, and `fork` duplicates every
/// one of them. So while jails is starting a child — a formatter, Maven, a
/// compose command — the child briefly holds a copy of whatever the parent has
/// open, until `exec` drops it through `O_CLOEXEC`. In that window a lock the
/// holder has already released still reads as held. The window is not
/// bounded by one `fork`/`exec`: it is bounded by how long the child takes to
/// *reach* `exec`, which on a loaded machine is a scheduling question, and
/// one `fork` captures *every* lock the process holds at that instant, not
/// just the one being released.
///
/// `SETTLE` is measured against that window, with room for a child that waits
/// for a CPU, and is still far shorter than anything a person reads as a hang,
/// so it separates the two cases without reintroducing an invisible wait.
/// Real contention costs the caller the settle window and then reports who
/// holds it; it never waits for the holder to finish.
///
/// It is a threshold rather than a decision because the two cases are not
/// distinguishable portably. Linux can tell them apart -- `/proc/locks` names
/// the holding pid against the device and inode the acquirer already has from
/// `same_entry` -- and that would replace the guess with an answer, at the
/// cost of a Linux-only branch in a production error path.
fn contend(file: &File, path: &Path) -> std::result::Result<(), Contention> {
    let deadline = std::time::Instant::now() + SETTLE;
    loop {
        match file.try_lock_exclusive() {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                if std::time::Instant::now() >= deadline {
                    return Err(Contention::Held(read_best_effort(path)));
                }
                std::thread::sleep(SETTLE_STEP);
            }
            Err(error) => {
                return Err(Contention::Refused(format!(
                    "could not lock {}: {error}",
                    path.display()
                )));
            }
        }
    }
}

fn read_best_effort(path: &Path) -> String {
    let mut content = String::new();
    if let Ok(mut file) = File::open(path) {
        let _ = file.read_to_string(&mut content);
    }
    content
}

#[cfg(unix)]
fn set_private(file: &File, path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    Ok(file
        .set_permissions(std::fs::Permissions::from_mode(0o600))
        .map_err(|error| format!("could not secure {}: {error}", path.display()))?)
}

/// The open handle and the path must still be the same entry, and it must not
/// be a symlink.
#[cfg(unix)]
fn same_entry(file: &File, path: &Path) -> Result<()> {
    use std::os::unix::fs::MetadataExt;
    let held = file
        .metadata()
        .map_err(|error| format!("could not stat the lock handle: {error}"))?;
    let named = std::fs::symlink_metadata(path)
        .map_err(|error| format!("could not stat {}: {error}", path.display()))?;
    if named.is_symlink() {
        return Err(format!(
            "{} is a symlink.\n       fix: replace it with a real file -- a symlink lets two \
             runs lock two different inodes and both believe they hold it.",
            path.display()
        )
        .into());
    }
    if (held.dev(), held.ino()) != (named.dev(), named.ino()) {
        return Err(format!(
            "{} changed while it was being locked.\n       fix: retry -- the lock is on an inode, not a \
             name; a replaced file means two holders are possible. Retry.",
            path.display()
        )
        .into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scratch::ScratchDir;

    #[test]
    fn a_second_acquirer_is_told_who_holds_it_and_does_not_wait() {
        let scratch = ScratchDir::in_temp("jails-lock").unwrap();
        let path = scratch.path().join("lock");
        let held = Lock::acquire(&path, "jails app apply").unwrap();

        let started = std::time::Instant::now();
        let error = Lock::acquire(&path, "jails add db").unwrap_err();
        assert!(
            started.elapsed() < std::time::Duration::from_secs(1),
            "acquisition waited instead of reporting"
        );
        let message = error.to_string();
        assert!(message.contains("another jails mutation"), "{message}");
        assert!(message.contains("jails app apply"), "{message}");

        drop(held);
        Lock::acquire(&path, "jails add db").unwrap();
        scratch.close().unwrap();
    }

    /// The lock is on an inode. A replaced file means two runs can hold two
    /// different inodes and both believe they hold the lock.
    #[test]
    fn a_lock_file_replaced_during_acquisition_is_refused() {
        let scratch = ScratchDir::in_temp("jails-lock").unwrap();
        let path = scratch.path().join("lock");
        std::fs::write(&path, "").unwrap();

        // Simulate the swap by locking the handle to a file that is no longer
        // at the path.
        let held = Lock::acquire(&path, "one").unwrap();
        std::fs::remove_file(&path).unwrap();
        std::fs::write(&path, "").unwrap();
        let error = same_entry(&held.file, &path).unwrap_err();
        assert!(
            error.contains("changed while it was being locked"),
            "{error}"
        );
        scratch.close().unwrap();
    }

    #[test]
    fn a_symlink_where_the_lock_belongs_is_refused() {
        let scratch = ScratchDir::in_temp("jails-lock").unwrap();
        let real = scratch.path().join("real");
        std::fs::write(&real, "").unwrap();
        let link = scratch.path().join("lock");
        std::os::unix::fs::symlink(&real, &link).unwrap();

        let error = Lock::acquire(&link, "one").unwrap_err();
        assert!(error.to_string().contains("symlink"), "{error}");
        scratch.close().unwrap();
    }

    /// The kernel releases the lock when the holder exits, however it exits —
    /// which is the whole reason this is not a PID file.
    #[test]
    fn the_lock_is_released_when_its_holder_is_dropped() {
        let scratch = ScratchDir::in_temp("jails-lock").unwrap();
        let path = scratch.path().join("lock");
        {
            let _held = Lock::acquire(&path, "one").unwrap();
        }
        Lock::acquire(&path, "two").unwrap();
        scratch.close().unwrap();
    }

    #[test]
    fn an_existing_directory_can_be_locked_without_creating_a_file() {
        let scratch = ScratchDir::in_temp("jails-directory-lock").unwrap();
        let before: Vec<_> = std::fs::read_dir(scratch.path()).unwrap().collect();
        let held = Lock::acquire_directory(scratch.path()).unwrap();

        let error = Lock::acquire_directory(scratch.path()).unwrap_err();
        assert!(matches!(error, Contention::Held(_)), "{error}");
        assert_eq!(
            std::fs::read_dir(scratch.path()).unwrap().count(),
            before.len()
        );

        drop(held);
        Lock::acquire_directory(scratch.path()).unwrap();
    }

    /// Contention is reported, not waited out.
    ///
    /// The settle window in `contend` buys a released lock time to look
    /// released; it must never turn into a queue. The holder here never lets
    /// go, so queuing would mean never answering.
    ///
    /// Bounded against `SETTLE` rather than a constant of its own: a bare
    /// number silently becomes the exact value of `SETTLE` when that moves,
    /// and turns a property into a coin flip.
    #[test]
    fn a_held_lock_is_refused_promptly_rather_than_queued_behind_its_holder() {
        let scratch = ScratchDir::in_temp("jails-lock-prompt").unwrap();
        let path = scratch.path().join("lock");
        let _held = Lock::acquire(&path, "one").unwrap();

        let started = std::time::Instant::now();
        let error = Lock::acquire(&path, "two").unwrap_err();
        let waited = started.elapsed();

        assert!(matches!(error, Contention::Held(_)), "{error}");
        assert!(
            waited < SETTLE * 2,
            "refusing took {waited:?} against a {SETTLE:?} settle window, \
             which is a queue rather than an answer"
        );
        scratch.close().unwrap();
    }

    #[test]
    fn the_lock_file_is_private_to_its_owner() {
        use std::os::unix::fs::PermissionsExt;
        let scratch = ScratchDir::in_temp("jails-lock").unwrap();
        let path = scratch.path().join("lock");
        let _held = Lock::acquire(&path, "one").unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        scratch.close().unwrap();
    }
}
