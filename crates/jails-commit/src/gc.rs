//! Deleting objects nothing points at, and nothing else.
//!
//! ## Why mark and sweep rather than an age check
//!
//! An object is shared. The bytes a generated file was rendered from may also
//! be the base of another file, the source of a template, or a preimage a
//! retained receipt needs to explain what it did. Deleting by age would
//! delete a base a project has had for a year and still measures its edits
//! against — and the symptom is not a missing file, it is a merge against
//! nothing that silently reports the user's whole file as a change.
//!
//! ## Why verification happens before any deletion
//!
//! plan.md §R5.5: *"Verify each object before deleting any."* A store with
//! one corrupt object is a store whose reachability jails cannot compute — a
//! record it cannot read may name roots it cannot see. So a corrupt object, a
//! symlink or an unreadable shard aborts the whole cycle with nothing
//! deleted, rather than pruning around the damage.
//!
//! ## Why a failure here is a warning and not a failed commit
//!
//! Garbage collection runs after a successful commit. The project change is
//! durable; the only cost of an aborted cycle is disk. Reporting it as a
//! failed commit would tell somebody to retry work that has already happened.
//!
//! ## Except that nothing calls it
//!
//! Closing this crate's API (`pending.md` §7.2) is what said so: with
//! `dead_code = "deny"`, [`sweep`] and everything it needs -- `roots_of`,
//! `promote_receipts`, `store::list_objects`, `store::is_object_name` -- are
//! reached from nothing. **No commit collects anything, so `.jails/objects`
//! only grows.** That is not a correctness bug (an unreachable object is
//! inert) and it is not a small one either: every rendered body, every base
//! and every preimage a project has ever had is still on disk.
//!
//! The module is complete and unit-tested. What is missing is the one call at
//! the end of a successful commit, plus the decision about where its warnings
//! go -- which is the paragraph above, already written.

use crate::store;
use jails_protocol::identity::ObjectId;
use std::collections::BTreeSet;
use std::path::Path;

/// What one cycle did.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Swept {
    pub kept: usize,
    pub deleted: Vec<ObjectId>,
}

/// What stopped a cycle. Never a claim that the commit failed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Warning {
    /// The store holds something that is not a verifiable object.
    Unreadable(String),
    /// A record names an object the store does not have. That is a dangling
    /// reference, and deleting *anything* while one exists could remove the
    /// bytes that would have explained it.
    MissingRoot(ObjectId),
}

impl std::fmt::Display for Warning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unreadable(why) => write!(
                f,
                "object cleanup did nothing: {why}.\n       fix: the store holds something it \
                 cannot verify, and pruning around damage risks deleting a base a project still \
                 measures against."
            ),
            Self::MissingRoot(id) => write!(
                f,
                "object cleanup did nothing: {id} is referenced and absent.\n       fix: a \
                 dangling reference means reachability cannot be computed, so nothing is \
                 deleted."
            ),
        }
    }
}

/// Delete every verified object no root reaches.
///
/// `roots` is the complete closure the caller computed from the ledger, the
/// pending candidate, every valid journal and every retained receipt. This
/// function does not derive it: an incomplete root set computed here would be
/// indistinguishable from an object legitimately becoming garbage.
pub fn sweep(objects: &Path, roots: &BTreeSet<ObjectId>) -> std::result::Result<Swept, Warning> {
    let present = store::list_objects(objects).map_err(Warning::Unreadable)?;
    let held: BTreeSet<ObjectId> = present.iter().copied().collect();

    // A root the store does not hold means the reachability graph is
    // incomplete, and an incomplete graph cannot justify a deletion.
    for root in roots {
        if !held.contains(root) {
            return Err(Warning::MissingRoot(*root));
        }
    }

    // Every object is verified before any is deleted.
    for id in &present {
        store::read_object(objects, id).map_err(Warning::Unreadable)?;
    }

    let mut swept = Swept::default();
    for id in present {
        if roots.contains(&id) {
            swept.kept += 1;
            continue;
        }
        let path = store::object_path(objects, &id);
        std::fs::remove_file(&path).map_err(|error| {
            Warning::Unreadable(format!("could not remove {}: {error}", path.display()))
        })?;
        if let Some(shard) = path.parent() {
            let _ = store::sync_dir(shard);
        }
        swept.deleted.push(id);
    }
    Ok(swept)
}

/// The object closure of one record, for building a root set.
///
/// A helper rather than a method, because the records that carry object
/// references live in three crates and each of them would otherwise grow its
/// own idea of what "reachable" means.
pub fn roots_of<'a>(refs: impl IntoIterator<Item = &'a ObjectId>) -> BTreeSet<ObjectId> {
    refs.into_iter().copied().collect()
}

/// Promote every object a retained receipt holds locally into the durable
/// store, before anything is deleted.
///
/// §R5.1's prepass: a retained receipt's local copy may be pruned only after
/// the matching global object is verified. If any promotion fails, the whole
/// cycle reports and deletes nothing.
pub fn promote_receipts(receipts: &Path, durable: &Path) -> std::result::Result<usize, Warning> {
    let entries = match std::fs::read_dir(receipts) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(Warning::Unreadable(error.to_string())),
    };
    let mut promoted = 0;
    for entry in entries {
        let entry = entry.map_err(|error| Warning::Unreadable(error.to_string()))?;
        let local = entry.path().join("objects");
        if !local.is_dir() {
            continue;
        }
        let ids = store::list_objects(&local).map_err(Warning::Unreadable)?;
        promoted += store::promote(&local, durable, &ids).map_err(Warning::Unreadable)?;
    }
    Ok(promoted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use jails_support::codec::sha256;
    use jails_support::scratch::ScratchDir;

    fn id_of(bytes: &[u8]) -> ObjectId {
        ObjectId::from_bytes(sha256(bytes))
    }

    fn store_with(bodies: &[&[u8]]) -> (ScratchDir, std::path::PathBuf) {
        let scratch = ScratchDir::in_temp("jails-gc").unwrap();
        let objects = scratch.path().join("objects");
        for body in bodies {
            store::put_object(&objects, &id_of(body), body).unwrap();
        }
        (scratch, objects)
    }

    #[test]
    fn an_unreachable_object_is_deleted_and_a_root_is_kept() {
        let (scratch, objects) = store_with(&[b"kept", b"garbage"]);
        let roots = roots_of([&id_of(b"kept")]);
        let swept = sweep(&objects, &roots).unwrap();
        assert_eq!(swept.kept, 1);
        assert_eq!(swept.deleted, vec![id_of(b"garbage")]);
        assert!(store::read_object(&objects, &id_of(b"kept")).is_ok());
        scratch.close().unwrap();
    }

    /// The symptom of deleting a live base is not a missing file — it is a
    /// merge against nothing that reports the user's whole file as a change.
    #[test]
    fn every_root_survives_a_cycle() {
        let (scratch, objects) = store_with(&[b"base", b"template", b"context"]);
        let roots = roots_of([&id_of(b"base"), &id_of(b"template"), &id_of(b"context")]);
        let swept = sweep(&objects, &roots).unwrap();
        assert_eq!(swept.kept, 3);
        assert!(swept.deleted.is_empty());
        scratch.close().unwrap();
    }

    /// Pruning around damage risks deleting a base a project still measures
    /// against, so a corrupt object stops the whole cycle.
    #[test]
    fn a_corrupt_object_aborts_the_cycle_with_nothing_deleted() {
        let (scratch, objects) = store_with(&[b"kept", b"garbage"]);
        let path = store::object_path(&objects, &id_of(b"kept"));
        std::fs::write(&path, b"tampered").unwrap();

        let error = sweep(&objects, &BTreeSet::new()).unwrap_err();
        assert!(matches!(error, Warning::Unreadable(_)), "{error:?}");
        assert!(
            store::object_path(&objects, &id_of(b"garbage")).exists(),
            "an object was deleted despite the abort"
        );
        scratch.close().unwrap();
    }

    /// A dangling reference means reachability cannot be computed, and an
    /// incomplete graph cannot justify a deletion.
    #[test]
    fn a_root_the_store_does_not_hold_stops_the_cycle() {
        let (scratch, objects) = store_with(&[b"garbage"]);
        let error = sweep(&objects, &roots_of([&id_of(b"missing")])).unwrap_err();
        assert!(matches!(error, Warning::MissingRoot(_)), "{error:?}");
        assert!(store::object_path(&objects, &id_of(b"garbage")).exists());
        scratch.close().unwrap();
    }

    /// A retained receipt's local copy may be pruned only after the matching
    /// global object is verified.
    #[test]
    fn receipt_objects_are_promoted_before_anything_is_swept() {
        let scratch = ScratchDir::in_temp("jails-gc").unwrap();
        let receipts = scratch.path().join("receipts");
        let durable = scratch.path().join("objects");
        let local = receipts.join("a".repeat(64)).join("objects");
        store::put_object(&local, &id_of(b"preimage"), b"preimage").unwrap();

        assert_eq!(promote_receipts(&receipts, &durable).unwrap(), 1);
        assert_eq!(
            store::read_object(&durable, &id_of(b"preimage")).unwrap(),
            b"preimage"
        );
        scratch.close().unwrap();
    }

    /// Reporting an aborted cycle as a failed commit would tell somebody to
    /// retry work that has already happened.
    #[test]
    fn every_warning_says_what_was_not_done_and_why() {
        for warning in [
            Warning::Unreadable("a shard could not be read".to_string()),
            Warning::MissingRoot(id_of(b"x")),
        ] {
            let text = warning.to_string();
            assert!(text.contains("did nothing"), "{text}");
            assert!(text.contains("fix:"), "{text}");
        }
    }
}
