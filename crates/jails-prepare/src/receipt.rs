//! What actually happened, as a value.
//!
//! A receipt is the prepared change's counterpart: the plan says what will
//! happen, the receipt says what did. They are separate types rather than one
//! with a status flag because a receipt records *both* images of every path —
//! a plan's `before` is a guard that might not have held, and a receipt's is
//! what was found.
//!
//! `AppliedReceipt` is the stable API projection. plan.md §R3.4 keeps the
//! durable `ReceiptV1` internal and never serialises it through a command
//! result, so the shape people script against can change on its own schedule
//! from the shape crash recovery depends on.

use crate::Result;
use jails_protocol::conflict::FileImage;
use jails_protocol::effect::{EffectId, EffectState, PostCommitEffect};
use jails_protocol::identity::{ObjectId, OperationId, ProjectPath, TransactionId};
use jails_protocol::resource::ResourceOwner;
use std::collections::BTreeSet;

/// How a transition ended.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApplyOutcome {
    Applied,
    Conflicted,
    Finalised,
    Aborted,
}

impl ApplyOutcome {
    pub fn label(self) -> &'static str {
        match self {
            Self::Applied => "applied",
            Self::Conflicted => "conflicted",
            Self::Finalised => "finalised",
            Self::Aborted => "aborted",
        }
    }
}

/// One path's transition, with both images.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileReceipt {
    pub path: ProjectPath,
    pub before: FileImage,
    pub after: FileImage,
    pub contributors: BTreeSet<ResourceOwner>,
}

/// One directory that was created.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirectoryReceipt {
    pub path: ProjectPath,
}

/// One post-commit effect and where it got to.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EffectReceipt {
    pub id: EffectId,
    pub effect: PostCommitEffect,
    pub state: EffectState,
}

/// The whole transition, as it happened.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppliedReceipt {
    pub operation_id: OperationId,
    pub transaction_id: TransactionId,
    pub operation_digest: ObjectId,
    pub prepared_after: ObjectId,
    pub files: Vec<FileReceipt>,
    pub directories: Vec<DirectoryReceipt>,
    pub ledger_before: FileImage,
    pub ledger_after: FileImage,
    pub outcome: ApplyOutcome,
    pub post_commit: Vec<EffectReceipt>,
}

impl AppliedReceipt {
    /// One receipt per path, for the same reason a plan carries one operation
    /// per path: two rows would make the order decide what is believed to
    /// have happened.
    pub fn validate(&self) -> Result<()> {
        let mut seen = BTreeSet::new();
        for file in &self.files {
            if !seen.insert(&file.path) {
                return Err(format!("{} appears twice in one receipt", file.path).into());
            }
            if file.before == file.after {
                return Err(format!(
                    "{} records the same image before and after; a receipt records what \
                     changed",
                    file.path
                )
                .into());
            }
        }
        let mut directories = BTreeSet::new();
        for directory in &self.directories {
            if !directories.insert(&directory.path) {
                return Err(format!(
                    "{} appears twice in one directory receipt.\n       fix: publish each created directory exactly once.",
                    directory.path
                )
                .into());
            }
            if seen.contains(&directory.path) {
                return Err(format!(
                    "{} is both a directory and file receipt.\n       fix: refuse the path-kind collision before commit.",
                    directory.path
                )
                .into());
            }
        }
        let mut effects = BTreeSet::new();
        for effect in &self.post_commit {
            if !effects.insert(effect.id) {
                return Err(format!("effect {} appears twice in one receipt", effect.id).into());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jails_protocol::identity::ObjectRef;
    use jails_support::codec::sha256;

    fn path(text: &str) -> ProjectPath {
        ProjectPath::parse(text).unwrap()
    }

    fn present(seed: &str) -> FileImage {
        FileImage::Present {
            object: ObjectRef::new(
                jails_protocol::identity::ObjectId::from_bytes(sha256(seed.as_bytes())),
                seed.len() as u64,
            ),
            mode: jails_protocol::conflict::FileMode::new(0o644).unwrap(),
        }
    }

    fn receipt(files: Vec<FileReceipt>) -> AppliedReceipt {
        AppliedReceipt {
            operation_id: OperationId::from_bytes(sha256(b"op")),
            transaction_id: TransactionId::from_bytes(sha256(b"tx")),
            operation_digest: ObjectId::from_bytes(sha256(b"ops")),
            prepared_after: ObjectId::from_bytes(sha256(b"after")),
            files,
            directories: Vec::new(),
            ledger_before: FileImage::Absent,
            ledger_after: present("ledger"),
            outcome: ApplyOutcome::Applied,
            post_commit: Vec::new(),
        }
    }

    #[test]
    fn a_receipt_records_both_images_of_every_path() {
        let one = receipt(vec![FileReceipt {
            path: path("pom.xml"),
            before: FileImage::Absent,
            after: present("<project/>"),
            contributors: BTreeSet::new(),
        }]);
        one.validate().unwrap();
        assert_eq!(one.files[0].before, FileImage::Absent);
    }

    #[test]
    fn one_path_twice_in_a_receipt_is_refused() {
        let row = FileReceipt {
            path: path("pom.xml"),
            before: FileImage::Absent,
            after: present("<project/>"),
            contributors: BTreeSet::new(),
        };
        let error = receipt(vec![row.clone(), row]).validate().unwrap_err();
        assert!(error.contains("appears twice"), "{error}");
    }

    #[test]
    fn one_directory_twice_in_a_receipt_is_refused() {
        let mut receipt = receipt(Vec::new());
        let directory = DirectoryReceipt {
            path: path("src/main/java"),
        };
        receipt.directories = vec![directory.clone(), directory];
        let error = receipt.validate().unwrap_err();
        assert!(error.contains("directory receipt"), "{error}");
    }

    /// A receipt records what changed. A row whose two images are equal is a
    /// claim that something happened where nothing did.
    #[test]
    fn a_row_whose_images_are_equal_is_refused() {
        let error = receipt(vec![FileReceipt {
            path: path("pom.xml"),
            before: present("<project/>"),
            after: present("<project/>"),
            contributors: BTreeSet::new(),
        }])
        .validate()
        .unwrap_err();
        assert!(error.contains("same image"), "{error}");
    }
}
