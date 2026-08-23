//! Where machine state lives, and the rules for putting bytes there.
//!
//! ## Why the layout is fixed rather than derived
//!
//! plan.md §R4.1 fixes it because recovery has to find these paths *without*
//! the value that created them. A crashed run leaves a transaction directory
//! and nothing else; if its location depended on configuration, a recovery
//! that read the configuration differently would not find it.
//!
//! ## Why an object write is so careful
//!
//! `create_new`, write, `sync_all`, reread and verify, then fsync the
//! containing directories. Each step answers a failure that has happened to
//! somebody: `create_new` because two runs may address the same content and
//! the second must not truncate the first mid-read; `sync_all` because a
//! crash after a buffered write leaves a file of the right length full of
//! zeroes; the reread because a short write reports success on some
//! filesystems; and the directory fsync because a synced file whose directory
//! entry was not synced is a file that does not exist after a power loss.
//!
//! Finding an object already there is fine — content addresses are stable —
//! but only after checking its length and hash. An existing file at the right
//! name proves nothing about its bytes.

use crate::Result;
use jails_protocol::identity::{ObjectId, TransactionId};
use jails_support::codec::sha256;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

/// The machine root inside a project.
pub const MACHINE_ROOT: &str = ".jails";

/// Where machine state for one project lives.
#[derive(Clone, Debug)]
pub struct Store {
    root: PathBuf,
}

impl Store {
    /// The store under a project root. Creates nothing.
    pub fn at(project_root: &Path) -> Self {
        Self {
            root: project_root.join(MACHINE_ROOT),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn lock_path(&self) -> PathBuf {
        self.root.join("lock")
    }

    pub fn effects_lock_path(&self) -> PathBuf {
        self.root.join("effects.lock")
    }

    pub fn transactions(&self) -> PathBuf {
        self.root.join("transactions")
    }

    pub fn receipts(&self) -> PathBuf {
        self.root.join("receipts")
    }

    pub fn objects(&self) -> PathBuf {
        self.root.join("objects")
    }

    pub fn transaction(&self, id: &TransactionId) -> PathBuf {
        self.transactions().join(id.to_hex())
    }

    pub fn receipt(&self, id: &TransactionId) -> PathBuf {
        self.receipts().join(id.to_hex())
    }

    /// Create the fixed subdirectories. Called only under the lock.
    pub fn create_subdirectories(&self) -> Result<()> {
        for directory in [self.transactions(), self.receipts(), self.objects()] {
            create_private_dir(&directory)?;
        }
        Ok(())
    }
}

/// `objects/sha256/<first-two>/<remaining-62>`.
///
/// Sharded because a flat directory of a hundred thousand entries is slow to
/// list on every filesystem and pathological on some.
pub fn object_path(objects: &Path, id: &ObjectId) -> PathBuf {
    let hex = id.to_hex();
    objects.join("sha256").join(&hex[..2]).join(&hex[2..])
}

/// Write one object, or verify the one already there.
pub fn put_object(objects: &Path, id: &ObjectId, bytes: &[u8]) -> Result<PathBuf> {
    let actual = ObjectId::from_bytes(sha256(bytes));
    if &actual != id {
        return Err(format!(
            "object {id} does not hash its own bytes; it hashes to {actual}"
        ));
    }
    let path = object_path(objects, id);
    let parent = path.parent().expect("object paths have parents");
    create_private_dir(parent)?;

    match File::options().write(true).create_new(true).open(&path) {
        Ok(mut file) => {
            file.write_all(bytes)
                .map_err(|error| format!("could not write {}: {error}", path.display()))?;
            file.sync_all()
                .map_err(|error| format!("could not sync {}: {error}", path.display()))?;
            drop(file);
            verify(&path, id, bytes.len() as u64)?;
            sync_dir(parent)?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            // A content address is stable, so an object already there is
            // ordinary. Its *name* proves nothing about its bytes, though.
            verify(&path, id, bytes.len() as u64)?;
        }
        Err(error) => {
            return Err(format!("could not create {}: {error}", path.display()));
        }
    }
    Ok(path)
}

/// Reread and check length and hash.
fn verify(path: &Path, id: &ObjectId, len: u64) -> Result<()> {
    let bytes = std::fs::read(path)
        .map_err(|error| format!("could not reread {}: {error}", path.display()))?;
    if bytes.len() as u64 != len {
        return Err(format!(
            "{} is {} bytes and should be {len}",
            path.display(),
            bytes.len()
        ));
    }
    if &ObjectId::from_bytes(sha256(&bytes)) != id {
        return Err(format!(
            "{} does not hash to {id}.\n       fix: the object store is corrupt; the bytes at \
             a content address are not the bytes it names.",
            path.display()
        ));
    }
    Ok(())
}

/// Read one object back, checking it against its own name.
pub fn read_object(objects: &Path, id: &ObjectId) -> Result<Vec<u8>> {
    let path = object_path(objects, id);
    let bytes = std::fs::read(&path)
        .map_err(|error| format!("could not read {}: {error}", path.display()))?;
    if &ObjectId::from_bytes(sha256(&bytes)) != id {
        return Err(format!(
            "{} does not hash to {id}; the object store is corrupt",
            path.display()
        ));
    }
    Ok(bytes)
}

/// A directory only this user can read. Machine state includes prepared
/// bytes for files the project may keep private.
pub fn create_private_dir(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent()
        && !parent.exists()
    {
        create_private_dir(parent)?;
    }
    match std::fs::create_dir(path) {
        Ok(()) => {}
        // A racing creator is fine; the mode is set below either way.
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => {
            return Err(format!("could not create {}: {error}", path.display()));
        }
    }
    set_private(path, 0o700)
}

#[cfg(unix)]
fn set_private(path: &Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|error| format!("could not stat {}: {error}", path.display()))?;
    if metadata.is_symlink() {
        return Err(format!(
            "{} is a symlink.\n       fix: machine state must be a real directory; a symlink \
             here points somewhere this transaction never validated.",
            path.display()
        ));
    }
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
        .map_err(|error| format!("could not set the mode of {}: {error}", path.display()))
}

/// fsync a directory, so an entry created in it survives a power loss.
pub fn sync_dir(path: &Path) -> Result<()> {
    File::open(path)
        .and_then(|dir| dir.sync_all())
        .map_err(|error| format!("could not sync {}: {error}", path.display()))
}

/// Every device a publication will cross, refused before activation.
///
/// A rename across a device boundary is not atomic — it is a copy — so a
/// receipt published onto another filesystem could be seen half-written. Path
/// ancestry does not answer this: a nested mount looks like an ordinary
/// subdirectory.
pub fn same_device(one: &Path, other: &Path) -> Result<bool> {
    Ok(device_of(one)? == device_of(other)?)
}

#[cfg(unix)]
fn device_of(path: &Path) -> Result<u64> {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(path)
        .map(|metadata| metadata.dev())
        .map_err(|error| format!("could not stat {}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use jails_support::scratch::ScratchDir;

    fn objects() -> (ScratchDir, PathBuf) {
        let scratch = ScratchDir::in_temp("jails-objects").unwrap();
        let objects = scratch.path().join("objects");
        (scratch, objects)
    }

    fn id_of(bytes: &[u8]) -> ObjectId {
        ObjectId::from_bytes(sha256(bytes))
    }

    #[test]
    fn an_object_round_trips_through_its_content_address() {
        let (scratch, objects) = objects();
        let id = id_of(b"hello");
        put_object(&objects, &id, b"hello").unwrap();
        assert_eq!(read_object(&objects, &id).unwrap(), b"hello");
        scratch.close().unwrap();
    }

    /// Two runs may address the same content, and the second must not
    /// truncate the first mid-read.
    #[test]
    fn writing_an_object_twice_is_accepted_after_verification() {
        let (scratch, objects) = objects();
        let id = id_of(b"hello");
        put_object(&objects, &id, b"hello").unwrap();
        put_object(&objects, &id, b"hello").unwrap();
        scratch.close().unwrap();
    }

    /// A file at the right name proves nothing about its bytes.
    #[test]
    fn an_existing_object_with_the_wrong_bytes_is_corruption() {
        let (scratch, objects) = objects();
        let id = id_of(b"hello");
        let path = object_path(&objects, &id);
        create_private_dir(path.parent().unwrap()).unwrap();
        // Same length, different bytes: only the content address catches it.
        std::fs::write(&path, b"world").unwrap();
        let error = put_object(&objects, &id, b"hello").unwrap_err();
        assert!(error.contains("does not hash to"), "{error}");
        scratch.close().unwrap();
    }

    #[test]
    fn an_object_whose_id_does_not_match_its_bytes_is_refused_before_any_write() {
        let (scratch, objects) = objects();
        let error = put_object(&objects, &id_of(b"hello"), b"goodbye").unwrap_err();
        assert!(error.contains("does not hash its own bytes"), "{error}");
        assert!(!objects.exists(), "a refused write created the store");
        scratch.close().unwrap();
    }

    /// Sharded because a flat directory of a hundred thousand entries is slow
    /// on every filesystem and pathological on some.
    #[test]
    fn object_paths_are_sharded_by_the_first_two_hex_characters() {
        let id = id_of(b"hello");
        let hex = id.to_hex();
        let path = object_path(Path::new("/tmp/objects"), &id);
        assert!(path.ends_with(format!("sha256/{}/{}", &hex[..2], &hex[2..])));
    }

    #[test]
    fn the_layout_is_the_one_recovery_looks_for() {
        let store = Store::at(Path::new("/srv/demo"));
        assert!(store.lock_path().ends_with(".jails/lock"));
        assert!(store.effects_lock_path().ends_with(".jails/effects.lock"));
        assert!(store.transactions().ends_with(".jails/transactions"));
        assert!(store.receipts().ends_with(".jails/receipts"));
    }

    /// Machine state carries prepared bytes for files the project may keep
    /// private, so its directories are not world-readable.
    #[test]
    fn machine_directories_are_private_to_their_owner() {
        use std::os::unix::fs::PermissionsExt;
        let scratch = ScratchDir::in_temp("jails-store").unwrap();
        let store = Store::at(scratch.path());
        create_private_dir(store.root()).unwrap();
        store.create_subdirectories().unwrap();
        for directory in [store.transactions(), store.receipts(), store.objects()] {
            let mode = std::fs::metadata(&directory).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o700, "{}", directory.display());
        }
        scratch.close().unwrap();
    }

    /// A symlink here points somewhere this transaction never validated.
    #[test]
    fn a_symlink_where_a_machine_directory_belongs_is_refused() {
        let scratch = ScratchDir::in_temp("jails-store").unwrap();
        let target = scratch.path().join("elsewhere");
        std::fs::create_dir(&target).unwrap();
        let link = scratch.path().join(".jails");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let error = create_private_dir(&link).unwrap_err();
        assert!(error.contains("symlink"), "{error}");
        scratch.close().unwrap();
    }
}
