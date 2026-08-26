//! Canonical identities for one prepared filesystem transition.
//!
//! These digests deliberately derive from the already prepared value. A
//! preview, portable plan and applied receipt therefore cannot each invent a
//! subtly different account of the same operation sequence or after-state.

use crate::Result;
use crate::prepare::{FileOp, PreparedChange};
use jails_protocol::identity::ObjectId;
use jails_protocol::snapshot::CanonicalRoot;
use jails_support::codec::{self, Codec, Encoder};

/// The digest of the public operation sequence, including truthful directory
/// creates before file operations.
pub fn operations(change: &PreparedChange) -> Result<ObjectId> {
    let mut encoder = Encoder::new();
    encode_operations(&mut encoder, change)?;
    Ok(ObjectId::from_bytes(codec::domain_hash(
        "JAILS-PREPARED-OPERATIONS-1",
        &encoder.finish()?,
    )))
}

/// The verification identity of the complete prepared after-state.
pub fn digest(root: &CanonicalRoot, change: &PreparedChange) -> Result<ObjectId> {
    let mut encoder = Encoder::new();
    encoder.string(root.as_str())?;
    let observed_generation = change
        .operation_identity
        .proposed_generation
        .checked_sub(1)
        .ok_or_else(|| {
            jails_support::Failure::Told(concat!(
                "a prepared change proposes generation zero.\n       ",
                "fix: prepare it against an observed generation before computing its after-state."
            )
            .to_string())
        })?;
    encoder.u64(observed_generation);
    encode_operations(&mut encoder, change)?;
    change.ledger_before.encode(&mut encoder)?;
    change.ledger_after.encode(&mut encoder)?;
    encoder.count(change.post_commit.len())?;
    for effect in &change.post_commit {
        effect.encode(&mut encoder)?;
    }
    Ok(ObjectId::from_bytes(codec::domain_hash(
        "JAILS-PREPARED-AFTER-1",
        &encoder.finish()?,
    )))
}

fn encode_operations(encoder: &mut Encoder, change: &PreparedChange) -> Result<()> {
    encoder.count(change.directories.len() + change.operations.len())?;
    for directory in &change.directories {
        encoder.tag(0);
        directory.path().encode(encoder)?;
        encoder.option::<ObjectId>(None, |one, id| id.encode(one))?;
        encoder.option::<ObjectId>(None, |one, id| id.encode(one))?;
        encoder.option::<jails_protocol::conflict::FileMode>(None, |one, mode| mode.encode(one))?;
        encoder.count(0)?;
    }
    for operation in &change.operations {
        encoder.tag(1);
        operation.target().encode(encoder)?;
        let (before, after, mode) = operation_fields(operation);
        encoder.option(before.as_ref(), |one, id| id.encode(one))?;
        encoder.option(after.as_ref(), |one, id| id.encode(one))?;
        encoder.option(mode.as_ref(), |one, mode| mode.encode(one))?;
        encoder.set(operation.contributors())?;
    }
    Ok(())
}

fn operation_fields(
    operation: &FileOp,
) -> (
    Option<ObjectId>,
    Option<ObjectId>,
    Option<jails_protocol::conflict::FileMode>,
) {
    match operation {
        FileOp::Create { after, mode, .. } => (None, Some(after.id), Some(*mode)),
        FileOp::Replace {
            before,
            after,
            mode,
            ..
        } => (Some(before.object.id), Some(after.id), Some(*mode)),
        FileOp::Delete { before, .. } => (Some(before.object.id), None, Some(before.mode)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prepare::tests::{change_with, create};

    #[test]
    fn equal_prepared_values_have_equal_operation_digests() {
        let one = change_with(vec![create("src/App.java", b"class App {}\n")]);
        let two = change_with(vec![create("src/App.java", b"class App {}\n")]);
        assert_eq!(operations(&one).unwrap(), operations(&two).unwrap());
    }

    #[test]
    fn prepared_after_binds_root_generation_and_ordered_operations() {
        let root = CanonicalRoot::new("/workspace/project").unwrap();
        let change = change_with(vec![
            create("src/Zebra.java", b"class Zebra {}\n"),
            create("src/Apple.java", b"class Apple {}\n"),
        ]);
        let expected = digest(&root, &change).unwrap();

        let reversed_input = change_with(vec![
            create("src/Apple.java", b"class Apple {}\n"),
            create("src/Zebra.java", b"class Zebra {}\n"),
        ]);
        assert_eq!(expected, digest(&root, &reversed_input).unwrap());

        let other_root = CanonicalRoot::new("/workspace/other").unwrap();
        assert_ne!(expected, digest(&other_root, &change).unwrap());

        let mut other_generation = change.clone();
        other_generation.operation_identity.proposed_generation += 1;
        assert_ne!(expected, digest(&root, &other_generation).unwrap());

        let other_operation = change_with(vec![create("src/App.java", b"class App {}\n")]);
        assert_ne!(expected, digest(&root, &other_operation).unwrap());
    }
}
