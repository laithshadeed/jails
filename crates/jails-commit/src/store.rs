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
///
/// plan.md §R5.1's protocol: a unique same-shard temporary with `create_new`,
/// synced and reread, then hard-linked to the final absent name and the
/// temporary unlinked. The link is the atomic no-replace step — writing
/// directly at the final name would leave a partially written file under a
/// content address for as long as the write took, and a concurrent reader
/// would see bytes that do not hash to their own name.
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

    if path.exists() {
        // A content address is stable, so an object already there is
        // ordinary. Its *name* proves nothing about its bytes, though.
        verify(&path, id, bytes.len() as u64)?;
        return Ok(path);
    }

    // Same shard, so the link is within one directory and one filesystem.
    let temporary = parent.join(format!(
        ".{}.{}.tmp",
        &id.to_hex()[2..10],
        std::process::id()
    ));
    let _ = std::fs::remove_file(&temporary);
    {
        let mut file = File::options()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| format!("could not create {}: {error}", temporary.display()))?;
        file.write_all(bytes)
            .map_err(|error| format!("could not write {}: {error}", temporary.display()))?;
        file.sync_all()
            .map_err(|error| format!("could not sync {}: {error}", temporary.display()))?;
    }
    verify(&temporary, id, bytes.len() as u64)?;

    match std::fs::hard_link(&temporary, &path) {
        Ok(()) => {}
        // Somebody else won the race to the same content address. Their bytes
        // are checked, not assumed.
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            verify(&path, id, bytes.len() as u64)?;
        }
        Err(error) => {
            let _ = std::fs::remove_file(&temporary);
            return Err(format!("could not link {}: {error}", path.display()));
        }
    }
    let _ = std::fs::remove_file(&temporary);
    sync_dir(parent)?;
    sync_dir(objects)?;
    Ok(path)
}

/// Whether a name under an object store is one this store would have written.
///
/// Checked rather than assumed because a store is a directory anyone can put
/// a file in, and a name that is not a content address is not an object —
/// reading it as one would mean trusting bytes nothing verified.
pub fn is_object_name(shard: &str, rest: &str) -> bool {
    fn lower_hex(text: &str, len: usize) -> bool {
        text.len() == len
            && text
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    }
    lower_hex(shard, 2) && lower_hex(rest, 62)
}

/// Copy every object reachable from one store into another, verifying each.
///
/// §R5.1: promotion happens **before** the ledger that references the
/// objects, so a committed store can never point at bytes that only exist
/// inside a transaction directory somebody later cleans up.
pub fn promote(from: &Path, into: &Path, ids: &[ObjectId]) -> Result<usize> {
    let mut promoted = 0;
    for id in ids {
        let bytes = read_object(from, id)?;
        put_object(into, id, &bytes)?;
        promoted += 1;
    }
    Ok(promoted)
}

/// Every object a store holds, by id.
pub fn list_objects(objects: &Path) -> Result<Vec<ObjectId>> {
    let sha256 = objects.join("sha256");
    let shards = match std::fs::read_dir(&sha256) {
        Ok(shards) => shards,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("could not read {}: {error}", sha256.display())),
    };
    let mut out = Vec::new();
    for shard in shards {
        let shard = shard.map_err(|error| format!("could not read a shard: {error}"))?;
        let shard_name = shard.file_name().to_string_lossy().to_string();
        let entries = std::fs::read_dir(shard.path())
            .map_err(|error| format!("could not read {}: {error}", shard.path().display()))?;
        for entry in entries {
            let entry = entry.map_err(|error| format!("could not read an object: {error}"))?;
            let name = entry.file_name().to_string_lossy().to_string();
            // A temporary or anything else that is not a content address is
            // not an object, and reading it as one would be trusting bytes
            // nothing verified.
            if !is_object_name(&shard_name, &name) {
                continue;
            }
            out.push(ObjectId::parse_hex(&format!("{shard_name}{name}"))?);
        }
    }
    out.sort();
    Ok(out)
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

    /// The link is the atomic no-replace step. Writing at the final name
    /// would leave a partially written file under a content address, and a
    /// concurrent reader would see bytes that do not hash to their own name.
    #[test]
    fn no_temporary_survives_a_successful_write() {
        let (scratch, objects) = objects();
        let id = id_of(b"hello");
        let path = put_object(&objects, &id, b"hello").unwrap();
        let shard = path.parent().unwrap();
        let leftovers: Vec<_> = std::fs::read_dir(shard)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .filter(|name| name.starts_with('.'))
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
        scratch.close().unwrap();
    }

    /// A store is a directory anyone can put a file in. A name that is not a
    /// content address is not an object.
    #[test]
    fn only_a_content_address_is_read_as_an_object() {
        assert!(is_object_name("ab", &"c".repeat(62)));
        assert!(!is_object_name("AB", &"c".repeat(62)), "uppercase");
        assert!(!is_object_name("ab", &"c".repeat(61)), "wrong length");
        assert!(!is_object_name("zz", &"c".repeat(62)), "not hex");
        assert!(!is_object_name("ab", ".tmp"), "a temporary");
    }

    #[test]
    fn listing_a_store_returns_exactly_its_objects() {
        let (scratch, objects) = objects();
        put_object(&objects, &id_of(b"one"), b"one").unwrap();
        put_object(&objects, &id_of(b"two"), b"two").unwrap();
        // A stray file in a shard is not an object and must not be listed.
        let shard = object_path(&objects, &id_of(b"one"));
        std::fs::write(shard.parent().unwrap().join("notes.txt"), b"hi").unwrap();

        let listed = list_objects(&objects).unwrap();
        assert_eq!(listed, {
            let mut expected = vec![id_of(b"one"), id_of(b"two")];
            expected.sort();
            expected
        });
        scratch.close().unwrap();
    }

    /// Promotion happens before the store that references the objects, so a
    /// committed ledger can never point at bytes that live only inside a
    /// transaction directory somebody later cleans up.
    #[test]
    fn promotion_copies_and_verifies_every_named_object() {
        let scratch = ScratchDir::in_temp("jails-promote").unwrap();
        let from = scratch.path().join("transaction/objects");
        let into = scratch.path().join("durable/objects");
        put_object(&from, &id_of(b"one"), b"one").unwrap();
        put_object(&from, &id_of(b"two"), b"two").unwrap();

        assert_eq!(
            promote(&from, &into, &[id_of(b"one"), id_of(b"two")]).unwrap(),
            2
        );
        assert_eq!(read_object(&into, &id_of(b"one")).unwrap(), b"one");
        // Idempotent: promoting again verifies rather than rewrites.
        assert_eq!(promote(&from, &into, &[id_of(b"one")]).unwrap(), 1);
        scratch.close().unwrap();
    }

    /// A corrupt object is never "repaired" from whatever is nearby: the
    /// promotion refuses and the durable store stays as it was.
    #[test]
    fn promoting_a_corrupt_object_refuses_rather_than_repairing_it() {
        let scratch = ScratchDir::in_temp("jails-promote").unwrap();
        let from = scratch.path().join("transaction/objects");
        let into = scratch.path().join("durable/objects");
        let id = id_of(b"one");
        let path = object_path(&from, &id);
        create_private_dir(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"two").unwrap();

        let error = promote(&from, &into, &[id]).unwrap_err();
        assert!(error.contains("corrupt"), "{error}");
        assert!(list_objects(&into).unwrap().is_empty());
        scratch.close().unwrap();
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
