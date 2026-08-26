//! The prepared transaction: exactly what will happen, decided before
//! anything happens.
//!
//! ## What "exact" buys
//!
//! plan.md §R3.1: *"There is no lazy callback/body in a prepared value."*
//! Every byte that will be written is present in `objects`, every file that
//! will be touched names both the image it expects to find and the image it
//! will leave, and every fact the plan depended on is an
//! `InputPrecondition`. That is what makes crash recovery possible at all —
//! a recovered journal can finish the work without re-deriving anything, and
//! it can *refuse* when the project has moved underneath it.
//!
//! ## Why a replace and a delete guard their preimage
//!
//! `GuardedImage` is not validation ceremony. It is the difference between
//! "write these bytes" and "write these bytes over exactly the ones I read".
//! Without it, a commit that started before the user edited a file would
//! overwrite that edit and report success, which is the failure the whole
//! transaction exists to prevent.
//!
//! Every operation names a `ProjectPath`, which refuses `.jails/` by
//! construction: machine state is the executor's, and a plan that could name a
//! path inside it would be able to rewrite the record of what it was doing.

use crate::Result;
use crate::operation::OperationIdentityV1;
use crate::tool::PreparationContextFingerprint;
use jails_protocol::conflict::{FileImage, FileMode};
use jails_protocol::effect::PostCommitEffect;
use jails_protocol::identity::{ObjectId, ObjectRef, OperationId, ProjectPath, TransactionId};
use jails_protocol::resource::ResourceOwner;
use jails_protocol::snapshot::InputPrecondition;
use jails_support::codec::{self, Codec, Decoder, Encoder, ordered};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

/// This crate's one format version.
pub(crate) const FORMAT: u32 = 1;

/// The bytes and mode a file operation expects to find.
///
/// Unlike [`FileImage`] this cannot be absent: a replace or a delete is about
/// a file that is there, and "guard against absence" is a create.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GuardedImage {
    pub object: ObjectRef,
    pub mode: FileMode,
}

impl Codec for GuardedImage {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.object.encode(encoder)?;
        self.mode.encode(encoder)?;
        Ok(())
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(Self {
            object: ObjectRef::decode(decoder)?,
            mode: FileMode::decode(decoder)?,
        })
    }
}

/// One file's transition, with the guard that makes it safe.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FileOp {
    Create {
        path: ProjectPath,
        after: ObjectRef,
        mode: FileMode,
        contributors: BTreeSet<ResourceOwner>,
    },
    Replace {
        path: ProjectPath,
        before: GuardedImage,
        after: ObjectRef,
        mode: FileMode,
        contributors: BTreeSet<ResourceOwner>,
    },
    Delete {
        path: ProjectPath,
        before: GuardedImage,
        contributors: BTreeSet<ResourceOwner>,
    },
}

impl FileOp {
    pub fn target(&self) -> &ProjectPath {
        match self {
            Self::Create { path, .. } | Self::Replace { path, .. } | Self::Delete { path, .. } => {
                path
            }
        }
    }

    pub fn contributors(&self) -> &BTreeSet<ResourceOwner> {
        match self {
            Self::Create { contributors, .. }
            | Self::Replace { contributors, .. }
            | Self::Delete { contributors, .. } => contributors,
        }
    }

    /// The object this operation will write, if it writes one.
    pub fn after(&self) -> Option<ObjectRef> {
        match self {
            Self::Create { after, .. } | Self::Replace { after, .. } => Some(*after),
            Self::Delete { .. } => None,
        }
    }

    fn tag(&self) -> u8 {
        match self {
            Self::Create { .. } => 0,
            Self::Replace { .. } => 1,
            Self::Delete { .. } => 2,
        }
    }
}
impl Codec for FileOp {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.tag(self.tag());
        match self {
            Self::Create {
                path,
                after,
                mode,
                contributors,
            } => {
                path.encode(encoder)?;
                after.encode(encoder)?;
                mode.encode(encoder)?;
                encoder.set(contributors)
            }
            Self::Replace {
                path,
                before,
                after,
                mode,
                contributors,
            } => {
                path.encode(encoder)?;
                before.encode(encoder)?;
                after.encode(encoder)?;
                mode.encode(encoder)?;
                encoder.set(contributors)
            }
            Self::Delete {
                path,
                before,
                contributors,
            } => {
                path.encode(encoder)?;
                before.encode(encoder)?;
                encoder.set(contributors)
            }
        }
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(match decoder.tag()? {
            0 => Self::Create {
                path: ProjectPath::decode(decoder)?,
                after: ObjectRef::decode(decoder)?,
                mode: FileMode::decode(decoder)?,
                contributors: decoder.set()?,
            },
            1 => Self::Replace {
                path: ProjectPath::decode(decoder)?,
                before: GuardedImage::decode(decoder)?,
                after: ObjectRef::decode(decoder)?,
                mode: FileMode::decode(decoder)?,
                contributors: decoder.set()?,
            },
            2 => Self::Delete {
                path: ProjectPath::decode(decoder)?,
                before: GuardedImage::decode(decoder)?,
                contributors: decoder.set()?,
            },
            other => Err(format!("unknown file operation tag {other}"))?,
        })
    }
}

/// A directory this transaction creates.
///
/// There is no removal. An empty directory left behind is untidy; a removed
/// one the user had put something in is data loss, and the two are told apart
/// only by a listing that may be stale by the time it is acted on.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum DirectoryOp {
    Create { path: ProjectPath },
}

impl DirectoryOp {
    pub fn path(&self) -> &ProjectPath {
        match self {
            Self::Create { path } => path,
        }
    }
}
impl Codec for DirectoryOp {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.tag(0);
        self.path().encode(encoder)
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        match decoder.tag()? {
            0 => Ok(Self::Create {
                path: ProjectPath::decode(decoder)?,
            }),
            other => Err(format!("unknown directory operation tag {other}").into()),
        }
    }
}

/// What kind of transition this is.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PreparedKind {
    Apply,
    /// A merge that could not be resolved. `paths` are the files left carrying
    /// markers, and they are the reason no effect is emitted: the postimage
    /// does not exist yet.
    Conflict {
        paths: Vec<ProjectPath>,
    },
    Finalise {
        origin: OperationId,
    },
    Abort {
        origin: OperationId,
    },
}

impl PreparedKind {
    fn tag(&self) -> u8 {
        match self {
            Self::Apply => 0,
            Self::Conflict { .. } => 1,
            Self::Finalise { .. } => 2,
            Self::Abort { .. } => 3,
        }
    }
}
impl Codec for PreparedKind {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.tag(self.tag());
        match self {
            Self::Apply => Ok(()),
            Self::Conflict { paths } => {
                // A sorted list rather than a set, so the `set` bound does
                // not apply -- see `snapshot.rs` for the same case.
                encoder.count(paths.len())?;
                let mut previous: Option<&ProjectPath> = None;
                for path in paths {
                    ordered(previous, path)?;
                    previous = Some(path);
                    path.encode(encoder)?;
                }
                Ok(())
            }
            Self::Finalise { origin } | Self::Abort { origin } => {
                origin.encode(encoder)?;
                Ok(())
            }
        }
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(match decoder.tag()? {
            0 => Self::Apply,
            1 => {
                let count = decoder.count()?;
                let mut paths = Vec::new();
                let mut previous: Option<ProjectPath> = None;
                for _ in 0..count {
                    let path = ProjectPath::decode(decoder)?;
                    ordered(previous.as_ref(), &path)?;
                    previous = Some(path.clone());
                    paths.push(path);
                }
                Self::Conflict { paths }
            }
            2 => Self::Finalise {
                origin: OperationId::decode(decoder)?,
            },
            3 => Self::Abort {
                origin: OperationId::decode(decoder)?,
            },
            other => Err(format!("unknown prepared kind tag {other}"))?,
        })
    }
}

/// Everything about a prepared transaction that decides its identity.
///
/// The bodies are *not* here: `object_manifest` names them by id and length.
/// Two preparations that produced the same bytes therefore have the same
/// transaction id however they arrived at them.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparedIdentityV1 {
    pub operation_identity: OperationIdentityV1,
    pub operation_id: OperationId,
    pub preparation: PreparationContextFingerprint,
    pub input_preconditions: Vec<InputPrecondition>,
    pub operations: Vec<FileOp>,
    pub directories: Vec<DirectoryOp>,
    pub ledger_before: FileImage,
    pub ledger_after: FileImage,
    pub object_manifest: Vec<ObjectRef>,
    pub post_commit: Vec<PostCommitEffect>,
    pub kind: PreparedKind,
}

impl PreparedIdentityV1 {
    /// The authenticated project state observed before this transaction.
    pub fn state_before(&self) -> &FileImage {
        &self.ledger_before
    }

    /// `SHA256("JAILS-PREPARED-1" || encode(self))`.
    ///
    /// §R4.2 names this prefix, and a journal's directory name and stored
    /// transaction must both equal it — so the id is the prepared identity
    /// and nothing else can be substituted for it.
    pub fn transaction_id(&self) -> Result<TransactionId> {
        let mut encoder = Encoder::new();
        self.encode(&mut encoder)?;
        Ok(TransactionId::from_bytes(codec::domain_hash(
            "JAILS-PREPARED-1",
            &encoder.finish()?,
        )))
    }

    /// The checks that hold for a prepared identity however it arrived.
    pub fn validate(&self) -> Result<()> {
        self.operation_identity.semantics.agrees_with(&self.kind)?;
        if self.operation_id != self.operation_identity.operation_id()? {
            return Err(
                jails_support::Failure::Told("the operation id does not hash its own identity; this record was assembled                  from two different operations"
                    .to_string()),
            );
        }
        let mut targets = BTreeSet::new();
        for operation in &self.operations {
            if !targets.insert(operation.target().clone()) {
                return Err(format!(
                    "{} carries two operations in one transaction",
                    operation.target()
                )
                .into());
            }
        }
        let mut previous: Option<&ObjectRef> = None;
        for object in &self.object_manifest {
            ordered(previous, object)?;
            previous = Some(object);
        }
        Ok(())
    }
}
impl Codec for PreparedIdentityV1 {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.u32(FORMAT);
        self.operation_identity.encode(encoder)?;
        self.operation_id.encode(encoder)?;
        self.preparation.encode(encoder)?;
        encoder.count(self.input_preconditions.len())?;
        for precondition in &self.input_preconditions {
            precondition.encode(encoder)?;
        }
        encoder.count(self.operations.len())?;
        for operation in &self.operations {
            operation.encode(encoder)?;
        }
        encoder.count(self.directories.len())?;
        for directory in &self.directories {
            directory.encode(encoder)?;
        }
        self.ledger_before.encode(encoder)?;
        self.ledger_after.encode(encoder)?;
        encoder.count(self.object_manifest.len())?;
        for object in &self.object_manifest {
            object.encode(encoder)?;
        }
        encoder.count(self.post_commit.len())?;
        for effect in &self.post_commit {
            effect.encode(encoder)?;
        }
        self.kind.encode(encoder)
    }

    /// Decode one prepared identity.
    ///
    /// A journal recovered after a crash comes back through here, and it goes
    /// through the same constructors the live path used — a decoder with its
    /// own idea of a valid value is a second validator, and two validators
    /// drift.
    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        let format = decoder.u32()?;
        if format != FORMAT {
            return Err(format!("prepared identity format {format} is not {FORMAT}").into());
        }
        let operation_identity = OperationIdentityV1::decode(decoder)?;
        let operation_id = OperationId::decode(decoder)?;
        let preparation = PreparationContextFingerprint::decode(decoder)?;
        let count = decoder.count()?;
        let mut input_preconditions = Vec::new();
        for _ in 0..count {
            input_preconditions.push(InputPrecondition::decode(decoder)?);
        }
        let count = decoder.count()?;
        let mut operations = Vec::new();
        for _ in 0..count {
            operations.push(FileOp::decode(decoder)?);
        }
        let count = decoder.count()?;
        let mut directories = Vec::new();
        for _ in 0..count {
            directories.push(DirectoryOp::decode(decoder)?);
        }
        let ledger_before = FileImage::decode(decoder)?;
        let ledger_after = FileImage::decode(decoder)?;
        let count = decoder.count()?;
        let mut object_manifest = Vec::new();
        for _ in 0..count {
            object_manifest.push(ObjectRef::decode(decoder)?);
        }
        let count = decoder.count()?;
        let mut post_commit = Vec::new();
        for _ in 0..count {
            post_commit.push(PostCommitEffect::decode(decoder)?);
        }
        let kind = PreparedKind::decode(decoder)?;
        let identity = Self {
            operation_identity,
            operation_id,
            preparation,
            input_preconditions,
            operations,
            directories,
            ledger_before,
            ledger_after,
            object_manifest,
            post_commit,
            kind,
        };
        identity.validate()?;
        Ok(identity)
    }
}

/// The complete prepared transaction, bodies included.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparedChange {
    pub operation_identity: OperationIdentityV1,
    pub operation_id: OperationId,
    pub transaction_id: TransactionId,
    pub preparation: PreparationContextFingerprint,
    pub input_preconditions: Vec<InputPrecondition>,
    pub operations: Vec<FileOp>,
    pub directories: Vec<DirectoryOp>,
    pub ledger_before: FileImage,
    pub ledger_after: FileImage,
    pub objects: BTreeMap<ObjectId, Arc<[u8]>>,
    pub post_commit: Vec<PostCommitEffect>,
    pub kind: PreparedKind,
}

impl PreparedChange {
    /// Encode the complete prepared transaction for a portable plan.
    ///
    /// Journals deliberately persist the identity and object bodies
    /// separately. A portable plan has no object store to fall back to, so it
    /// carries both in one closed record and validates every content address
    /// again when decoded.
    pub fn portable_bytes(&self) -> Result<Vec<u8>> {
        let mut encoder = Encoder::new();
        self.encode(&mut encoder)?;
        encoder.finish()
    }

    /// Decode a complete portable transaction and reject trailing or corrupt
    /// data before the executor sees it.
    pub fn from_portable_bytes(bytes: &[u8]) -> Result<Self> {
        let mut decoder = Decoder::new(bytes)?;
        let change = Self::decode(&mut decoder)?;
        decoder.finish()?;
        Ok(change)
    }

    /// Everything that must hold before this value may be executed.
    ///
    /// Each check is here rather than at a construction site because a plan
    /// arrives from three places — built now, decoded from a journal after a
    /// crash, or reread during recovery — and a rule enforced at only one of
    /// them is a rule the other two can violate.
    pub fn validate(&self) -> Result<()> {
        self.operation_identity.semantics.agrees_with(&self.kind)?;

        if self.operation_id != self.operation_identity.operation_id()? {
            return Err(jails_support::Failure::Told(
                "the operation id does not hash its own identity; this plan was assembled from \
                 two different operations"
                    .to_string(),
            ));
        }

        // One operation per target. Two would make the order decide the
        // result, and the order is not part of the identity.
        let mut targets = BTreeSet::new();
        for operation in &self.operations {
            if !targets.insert(operation.target().clone()) {
                return Err(format!(
                    "{} carries two operations in one transaction",
                    operation.target()
                )
                .into());
            }
        }

        // Every byte that will be written is present. §R3.1: no lazy body.
        for operation in &self.operations {
            if let Some(after) = operation.after() {
                match self.objects.get(&after.id) {
                    None => {
                        return Err(format!(
                            "{} writes an object that is not in the bundle.\n       fix: a \
                             prepared value carries every byte it will write, so recovery can \
                             finish without re-deriving anything.",
                            operation.target()
                        )
                        .into());
                    }
                    Some(bytes) if bytes.len() as u64 != after.len => {
                        return Err(format!(
                            "{} names an object of {} bytes and carries {}",
                            operation.target(),
                            after.len,
                            bytes.len()
                        )
                        .into());
                    }
                    Some(_) => {}
                }
            }
        }
        for (id, bytes) in &self.objects {
            let actual = ObjectId::from_bytes(codec::sha256(bytes));
            if &actual != id {
                return Err(format!(
                    "object {id} does not hash its own bytes; the bundle is corrupt"
                )
                .into());
            }
        }

        self.validate_effects()?;

        if self.transaction_id != self.identity()?.transaction_id()? {
            return Err(jails_support::Failure::Told(
                "the transaction id does not hash its own prepared identity".to_string(),
            ));
        }
        Ok(())
    }

    /// §R3.1 permits at most one aggregate effect, and a conflict emits none:
    /// the resolved postimage does not exist yet, so an executable descriptor
    /// would have to invent it.
    fn validate_effects(&self) -> Result<()> {
        if self.post_commit.len() > 1 {
            return Err(format!(
                "{} post-commit effects; V1 permits at most one aggregate effect",
                self.post_commit.len()
            )
            .into());
        }
        if !self.post_commit.is_empty() && !matches!(self.kind, PreparedKind::Apply) {
            return Err(jails_support::Failure::Told(
                "only an apply carries an executable effect.\n       fix: a conflict freezes the \
                 intent and waits for the resolved postimage; an abort discards it."
                    .to_string(),
            ));
        }
        Ok(())
    }

    /// This change's identity: the same value with bodies replaced by refs.
    pub fn identity(&self) -> Result<PreparedIdentityV1> {
        Ok(PreparedIdentityV1 {
            operation_identity: self.operation_identity.clone(),
            operation_id: self.operation_id,
            preparation: self.preparation.clone(),
            input_preconditions: self.input_preconditions.clone(),
            operations: self.operations.clone(),
            directories: self.directories.clone(),
            ledger_before: self.ledger_before,
            ledger_after: self.ledger_after,
            object_manifest: self
                .objects
                .iter()
                .map(|(id, bytes)| ObjectRef::new(*id, bytes.len() as u64))
                .collect(),
            post_commit: self.post_commit.clone(),
            kind: self.kind.clone(),
        })
    }

    /// A change with nothing to do. R3's empty-effect rule: a true semantic,
    /// file and ledger no-op is a legitimate outcome, not a bug to paper over.
    pub fn is_no_op(&self) -> bool {
        self.operations.is_empty()
            && self.directories.is_empty()
            && self.post_commit.is_empty()
            && self.ledger_before == self.ledger_after
    }
}

impl Codec for PreparedChange {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.validate()?;
        encoder.u32(FORMAT);
        self.identity()?.encode(encoder)?;
        self.transaction_id.encode(encoder)?;
        encoder.count(self.objects.len())?;
        let mut previous: Option<&ObjectId> = None;
        for (id, bytes) in &self.objects {
            ordered(previous, id)?;
            previous = Some(id);
            id.encode(encoder)?;
            encoder.object(bytes, codec::DEFAULT_MAX_OBJECT_BYTES)?;
        }
        Ok(())
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        let format = decoder.u32()?;
        if format != FORMAT {
            return Err(format!(
                "portable prepared change format {format} is not {FORMAT}.\n       \
                 fix: re-export the plan with this jails version."
            )
            .into());
        }
        let identity = PreparedIdentityV1::decode(decoder)?;
        let transaction_id = TransactionId::decode(decoder)?;
        let count = decoder.count()?;
        let mut objects = BTreeMap::new();
        for _ in 0..count {
            let id = ObjectId::decode(decoder)?;
            ordered(objects.last_key_value().map(|(last, _)| last), &id)?;
            let bytes: Arc<[u8]> = Arc::from(decoder.object(codec::DEFAULT_MAX_OBJECT_BYTES)?);
            objects.insert(id, bytes);
        }
        let change = Self {
            operation_identity: identity.operation_identity,
            operation_id: identity.operation_id,
            transaction_id,
            preparation: identity.preparation,
            input_preconditions: identity.input_preconditions,
            operations: identity.operations,
            directories: identity.directories,
            ledger_before: identity.ledger_before,
            ledger_after: identity.ledger_after,
            objects,
            post_commit: identity.post_commit,
            kind: identity.kind,
        };
        change.validate()?;
        if change.identity()?.object_manifest != identity.object_manifest {
            return Err(concat!(
                "portable prepared change object manifest does not match its bodies.\n       ",
                "fix: discard the corrupt plan and export it again."
            )
            .into());
        }
        Ok(change)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::operation::{ApplySemantics, OperationSemanticsV1};
    use crate::tool::tests::identity;
    use crate::tool::{OperationContextFingerprint, OperationToolFingerprint, ToolArgTemplate};
    use jails_protocol::entity::{EntityId, IntentId, Recipe};
    use jails_protocol::identity::{Name, Package};
    use jails_protocol::plan::{LedgerIntent, PlannedSubject};
    use jails_support::codec::sha256;

    pub(crate) fn path(text: &str) -> ProjectPath {
        ProjectPath::parse(text).unwrap()
    }

    fn mode() -> FileMode {
        FileMode::new(0o644).unwrap()
    }

    fn owner(name: &str) -> ResourceOwner {
        ResourceOwner::Entity(EntityId::Intent(IntentId::new(
            Recipe::Record,
            Name::parse(name).unwrap(),
            Package::parse("com.example.demo.domain").unwrap(),
        )))
    }

    /// A create of `body` at `at`, with the object it will write.
    pub(crate) fn create(at: &str, body: &[u8]) -> (FileOp, Vec<u8>) {
        let id = ObjectId::from_bytes(sha256(body));
        (
            FileOp::Create {
                path: path(at),
                after: ObjectRef::new(id, body.len() as u64),
                mode: mode(),
                contributors: BTreeSet::from([owner("Note")]),
            },
            body.to_vec(),
        )
    }

    fn semantics() -> OperationSemanticsV1 {
        OperationSemanticsV1::Apply(Box::new(ApplySemantics {
            subject: PlannedSubject::AdoptLayout,
            ledger_intent: LedgerIntent {
                generation_before: 3,
                entities_after: Vec::new(),
                one_shots_after: Vec::new(),
                resources_after: Vec::new(),
                entities_removed: Vec::new(),
            },
        }))
    }

    fn operation_identity(semantics: OperationSemanticsV1) -> OperationIdentityV1 {
        OperationIdentityV1 {
            snapshot: ObjectId::from_bytes(sha256(b"snapshot")),
            operation_context: OperationContextFingerprint {
                tools: vec![OperationToolFingerprint {
                    identity: identity("spotless"),
                    args: vec![
                        ToolArgTemplate::Literal("spotless:apply".to_string()),
                        ToolArgTemplate::OperationLabel {
                            prefix: "jails-".to_string(),
                            hex_chars: 12,
                        },
                    ],
                }],
            },
            invocation: None,
            proposed_generation: 4,
            semantics,
        }
    }

    /// A complete, self-consistent prepared change over the given operations.
    pub(crate) fn change_with(parts: Vec<(FileOp, Vec<u8>)>) -> PreparedChange {
        assemble(operation_identity(semantics()), PreparedKind::Apply, parts)
    }

    fn assemble(
        operation_identity: OperationIdentityV1,
        kind: PreparedKind,
        parts: Vec<(FileOp, Vec<u8>)>,
    ) -> PreparedChange {
        let mut objects = BTreeMap::new();
        let mut operations = Vec::new();
        for (operation, body) in parts {
            if let Some(after) = operation.after() {
                objects.insert(after.id, Arc::from(body.into_boxed_slice()));
            }
            operations.push(operation);
        }
        operations.sort_by(|a, b| a.target().cmp(b.target()));
        let operation_id = operation_identity.operation_id().unwrap();
        let mut change = PreparedChange {
            operation_identity,
            operation_id,
            transaction_id: TransactionId::from_bytes([0; 32]),
            preparation: PreparationContextFingerprint::default(),
            input_preconditions: Vec::new(),
            operations,
            directories: Vec::new(),
            ledger_before: FileImage::Absent,
            ledger_after: FileImage::Absent,
            objects,
            post_commit: Vec::new(),
            kind,
        };
        change.transaction_id = change.identity().unwrap().transaction_id().unwrap();
        change
    }

    #[test]
    fn a_self_consistent_change_validates() {
        change_with(vec![create("pom.xml", b"<project/>")])
            .validate()
            .unwrap();
    }

    #[test]
    fn prepared_bundle_matches_the_protocol_golden() {
        let change = change_with(vec![create(
            "src/main/java/com/example/Note.java",
            b"package com.example;\n\npublic record Note(String title) {}\n",
        )]);
        let identity = change.identity().unwrap();
        let mut encoder = Encoder::new();
        identity.encode(&mut encoder).unwrap();
        let identity_bytes = encoder.finish().unwrap();
        let mut actual = format!(
            "operation = {}\ntransaction = {}\nprepared_identity_hex = {}\n",
            change.operation_id,
            change.transaction_id,
            hex(&identity_bytes)
        );
        for (id, bytes) in &change.objects {
            actual.push_str(&format!("object = {id} {} {}\n", bytes.len(), hex(bytes)));
        }
        let expected = include_str!("../../../tests/protocol-golden/prepared-bundle.txt");
        assert_eq!(actual, expected);
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    /// §R3.1: no lazy body. A recovered journal has to be able to finish the
    /// work without re-deriving anything.
    #[test]
    fn an_operation_whose_bytes_are_missing_is_refused() {
        let mut change = change_with(vec![create("pom.xml", b"<project/>")]);
        change.objects.clear();
        let error = change.validate().unwrap_err();
        assert!(error.contains("not in the bundle"), "{error}");
    }

    #[test]
    fn an_object_that_does_not_hash_its_own_bytes_is_refused() {
        let mut change = change_with(vec![create("pom.xml", b"<project/>")]);
        let id = *change.objects.keys().next().unwrap();
        // Same length, different bytes: the length check passes and only
        // the content address catches it.
        change.objects.insert(id, Arc::from(b"<tampered>".to_vec()));
        let error = change.validate().unwrap_err();
        assert!(error.contains("does not hash its own bytes"), "{error}");
    }

    /// Two operations on one path would make the order decide the result, and
    /// the order is not part of the identity.
    #[test]
    fn two_operations_on_one_path_are_refused() {
        let mut change = change_with(vec![create("pom.xml", b"<project/>")]);
        let duplicate = change.operations[0].clone();
        change.operations.push(duplicate);
        change.transaction_id = change.identity().unwrap().transaction_id().unwrap();
        assert!(change.validate().unwrap_err().contains("two operations"));
    }

    #[test]
    fn a_tampered_operation_id_is_refused() {
        let mut change = change_with(vec![create("pom.xml", b"<project/>")]);
        change.operation_id = OperationId::from_bytes(sha256(b"someone else"));
        let error = change.validate().unwrap_err();
        assert!(error.contains("two different operations"), "{error}");
    }

    #[test]
    fn a_tampered_transaction_id_is_refused() {
        let mut change = change_with(vec![create("pom.xml", b"<project/>")]);
        change.transaction_id = TransactionId::from_bytes(sha256(b"elsewhere"));
        let error = change.validate().unwrap_err();
        assert!(error.contains("does not hash its own"), "{error}");
    }

    /// The identity covers the bytes by reference, so changing what a file
    /// will contain changes the transaction.
    #[test]
    fn different_bytes_are_a_different_transaction() {
        let one = change_with(vec![create("pom.xml", b"<project/>")]);
        let other = change_with(vec![create("pom.xml", b"<project></project>")]);
        assert_ne!(one.transaction_id, other.transaction_id);
    }

    /// A conflict cannot carry an executable effect: the resolved postimage
    /// does not exist yet, so the descriptor would have to invent it.
    #[test]
    fn a_conflict_carries_no_executable_effect() {
        let mut change = assemble(
            operation_identity(semantics()),
            PreparedKind::Conflict {
                paths: vec![path("pom.xml")],
            },
            vec![create("pom.xml", b"<project/>")],
        );
        change.post_commit.push(compose_effect());
        change.transaction_id = change.identity().unwrap().transaction_id().unwrap();
        let error = change.validate().unwrap_err();
        assert!(error.contains("only an apply"), "{error}");
    }

    #[test]
    fn more_than_one_aggregate_effect_is_refused() {
        let mut change = change_with(vec![create("pom.xml", b"<project/>")]);
        change.post_commit.push(compose_effect());
        change.post_commit.push(compose_effect());
        change.transaction_id = change.identity().unwrap().transaction_id().unwrap();
        assert!(change.validate().unwrap_err().contains("at most one"));
    }

    fn compose_effect() -> PostCommitEffect {
        PostCommitEffect::ComposeReconcile {
            compose_output: path("compose.yaml"),
            before_document: None,
            after_document: None,
            prior_managed_services: BTreeMap::new(),
            desired_services: BTreeMap::new(),
            stop_services: BTreeSet::new(),
        }
    }

    /// A plan whose file operations abort while its semantics apply is not a
    /// transition anything can execute.
    #[test]
    fn a_kind_and_semantics_that_disagree_are_refused() {
        let change = assemble(
            operation_identity(semantics()),
            PreparedKind::Abort {
                origin: OperationId::from_bytes(sha256(b"origin")),
            },
            Vec::new(),
        );
        let error = change.validate().unwrap_err();
        assert!(error.contains("different transitions"), "{error}");
    }

    #[test]
    fn a_change_with_nothing_to_do_is_a_no_op() {
        assert!(change_with(Vec::new()).is_no_op());
        assert!(!change_with(vec![create("pom.xml", b"<project/>")]).is_no_op());
    }
}
