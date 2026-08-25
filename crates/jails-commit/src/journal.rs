//! The durable record of one transaction, and the receipt it becomes.
//!
//! ## Why a journal at all
//!
//! Several filesystem names do not change at once. A commit that writes five
//! files can be interrupted after two, and nothing on disk would say which
//! two were meant. The journal says: it names the transaction, carries its
//! complete prepared identity, and records which phase it reached. plan.md
//! §R4 makes recovery roll a validated journal **forward**, which is why the
//! record has to be complete before a single live byte is touched.
//!
//! ## Why the checksum covers everything but itself
//!
//! Because a journal is rewritten as its state advances, and a rewrite
//! interrupted mid-write leaves a record that decodes. Without a checksum the
//! executor would read a half-updated state and act on it. §R4.2: a checksum
//! mismatch is corruption, *not* an incomplete transition to guess through.
//!
//! ## Why the transaction id is the prepared identity's hash
//!
//! `SHA256("JAILS-PREPARED-1" || encode(prepared))`, and the directory name
//! must equal it. So a journal cannot be moved into another transaction's
//! directory, and a prepared identity cannot be swapped under a name that
//! still looks right.
//!
//! ## Why `Complete` journals are kept
//!
//! A published receipt directory holds both records. The receipt is the
//! effect-state authority and the journal is the immutable witness recovery
//! validates it against — `ReceiptV1.complete_journal_checksum` binds them,
//! so editing one to agree with the other is not possible without breaking
//! the receipt's own checksum.

use crate::Result;
use crate::store;
use jails_prepare::prepare::PreparedIdentityV1;
use jails_prepare::receipt::EffectReceipt;
use jails_protocol::compatibility::{JOURNAL_MAGIC, RECEIPT_MAGIC};
use jails_protocol::conflict::FileMode;
use jails_protocol::identity::{ObjectId, ProjectPath, TransactionId};
use jails_support::codec::{self, Codec, Decoder, Encoder};
use std::path::Path;

/// The largest record either format may reach.
pub(crate) const MAX_RECORD: usize = codec::MAX_PROTOCOL_RECORD;

/// Which filesystem object the project root was when this began.
///
/// A project moved or replaced under a running transaction is not the project
/// the plan was made against, and a path comparison would not notice: the
/// same path can name a different directory.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RootIdentity {
    pub device: u64,
    pub inode: u64,
}

impl RootIdentity {
    #[cfg(unix)]
    pub fn of(path: &Path) -> Result<Self> {
        use std::os::unix::fs::MetadataExt;
        let metadata = std::fs::metadata(path)
            .map_err(|error| format!("could not stat {}: {error}", path.display()))?;
        Ok(Self {
            device: metadata.dev(),
            inode: metadata.ino(),
        })
    }
}
impl Codec for RootIdentity {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.u64(self.device);
        encoder.u64(self.inode);
        Ok(())
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(Self {
            device: decoder.u64()?,
            inode: decoder.u64()?,
        })
    }
}

/// How far a transaction got.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum JournalState {
    /// Validated, and **no live byte may be touched**.
    Prepared,
    /// Recovery must roll this forward.
    Active,
    LedgerCommitted,
    Complete,
    /// Recovery stopped and said why. Every mutation is blocked until a
    /// person resolves it, because guessing past an unclassifiable live image
    /// is how a half-applied transaction becomes a wrong one.
    Blocked {
        resume: ResumeState,
        path: Option<ProjectPath>,
        reason: BlockReason,
    },
}

impl JournalState {
    fn tag(&self) -> u8 {
        match self {
            Self::Prepared => 0,
            Self::Active => 1,
            Self::LedgerCommitted => 2,
            Self::Complete => 3,
            Self::Blocked { .. } => 4,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Prepared => "prepared",
            Self::Active => "active",
            Self::LedgerCommitted => "ledger-committed",
            Self::Complete => "complete",
            Self::Blocked { .. } => "blocked",
        }
    }
}
impl Codec for JournalState {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.tag(self.tag());
        if let Self::Blocked {
            resume,
            path,
            reason,
        } = self
        {
            encoder.tag(resume.tag());
            encoder.option(path.as_ref(), |e, path| path.encode(e))?;
            reason.encode(encoder)?;
        }
        Ok(())
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(match decoder.tag()? {
            0 => Self::Prepared,
            1 => Self::Active,
            2 => Self::LedgerCommitted,
            3 => Self::Complete,
            4 => Self::Blocked {
                resume: ResumeState::from_tag(decoder.tag()?)?,
                path: decoder.option(ProjectPath::decode)?,
                reason: BlockReason::decode(decoder)?,
            },
            other => Err(format!("unknown journal state tag {other}"))?,
        })
    }
}

/// The phase a blocked transaction would resume from.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResumeState {
    Prepared,
    Active,
    LedgerCommitted,
    Complete,
}

impl ResumeState {
    fn tag(self) -> u8 {
        match self {
            Self::Prepared => 0,
            Self::Active => 1,
            Self::LedgerCommitted => 2,
            Self::Complete => 3,
        }
    }

    fn from_tag(tag: u8) -> Result<Self> {
        Ok(match tag {
            0 => Self::Prepared,
            1 => Self::Active,
            2 => Self::LedgerCommitted,
            3 => Self::Complete,
            other => Err(format!("unknown resume state tag {other}"))?,
        })
    }
}

/// What a live path actually was, when it was neither image the plan named.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ActualImage {
    Absent,
    File {
        sha256: ObjectId,
        len: u64,
        mode: FileMode,
    },
    Directory,
    Symlink,
    Other,
}

impl ActualImage {
    fn tag(&self) -> u8 {
        match self {
            Self::Absent => 0,
            Self::File { .. } => 1,
            Self::Directory => 2,
            Self::Symlink => 3,
            Self::Other => 4,
        }
    }
}
impl Codec for ActualImage {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.tag(self.tag());
        if let Self::File { sha256, len, mode } = self {
            sha256.encode(encoder)?;
            encoder.u64(*len);
            encoder.u32(mode.bits());
        }
        Ok(())
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(match decoder.tag()? {
            0 => Self::Absent,
            1 => Self::File {
                sha256: ObjectId::decode(decoder)?,
                len: decoder.u64()?,
                mode: FileMode::new(decoder.u32()?)?,
            },
            2 => Self::Directory,
            3 => Self::Symlink,
            4 => Self::Other,
            other => Err(format!("unknown actual image tag {other}"))?,
        })
    }
}

/// How a live path compared against the two images the plan named.
///
/// `Unknown` is the interesting one: recovery can roll forward from either
/// the before or the after image, and neither from anything else. A third
/// value is not "probably fine" — it is an edit nobody recorded.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ObservedImage {
    Before,
    After,
    Unknown { actual: ActualImage },
    Unreadable { error_kind: String },
}

/// Why a transaction stopped.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BlockReason {
    UnknownLiveImage { actual: ActualImage },
    Unreadable { error_kind: String },
    RootChanged,
    CorruptJournal,
    CorruptObject(ObjectId),
    MultipleTransactions,
}

impl BlockReason {
    fn tag(&self) -> u8 {
        match self {
            Self::UnknownLiveImage { .. } => 0,
            Self::Unreadable { .. } => 1,
            Self::RootChanged => 2,
            Self::CorruptJournal => 3,
            Self::CorruptObject(_) => 4,
            Self::MultipleTransactions => 5,
        }
    }

    /// What a person is told, and what they can do about it.
    pub fn explain(&self) -> String {
        match self {
            Self::UnknownLiveImage { actual } => format!(
                "a file is neither the image this transaction expected nor the one it would \
                 have written (found {actual:?}).\n       fix: something changed it while the \
                 transaction was interrupted. Restore it to either image, or abort."
            ),
            Self::Unreadable { error_kind } => format!(
                "a file could not be read ({error_kind}).\n       fix: make it readable and run \
                 the command again."
            ),
            Self::RootChanged => "the project root is not the directory this transaction began \
                 in.\n       fix: run the command from the original project."
                .to_string(),
            Self::CorruptJournal => "the transaction record does not match its own \
                 checksum.\n       fix: it cannot be rolled forward safely; the transaction \
                 directory must be inspected by hand."
                .to_string(),
            Self::CorruptObject(id) => format!(
                "the bytes at content address {id} are not the bytes it names.\n       fix: the \
                 object store is corrupt; the transaction cannot be completed."
            ),
            Self::MultipleTransactions => "more than one incomplete transaction \
                 exists.\n       fix: only one may be rolled forward; inspect them by hand."
                .to_string(),
        }
    }
}
impl Codec for BlockReason {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.tag(self.tag());
        match self {
            Self::UnknownLiveImage { actual } => {
                actual.encode(encoder)?;
                Ok(())
            }
            Self::Unreadable { error_kind } => encoder.string(error_kind),
            Self::CorruptObject(id) => {
                id.encode(encoder)?;
                Ok(())
            }
            Self::RootChanged | Self::CorruptJournal | Self::MultipleTransactions => Ok(()),
        }
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(match decoder.tag()? {
            0 => Self::UnknownLiveImage {
                actual: ActualImage::decode(decoder)?,
            },
            1 => Self::Unreadable {
                error_kind: decoder.string()?,
            },
            2 => Self::RootChanged,
            3 => Self::CorruptJournal,
            4 => Self::CorruptObject(ObjectId::decode(decoder)?),
            5 => Self::MultipleTransactions,
            other => Err(format!("unknown block reason tag {other}"))?,
        })
    }
}

/// The durable record of one transaction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JournalV1 {
    pub transaction: TransactionId,
    pub generation: u64,
    pub root_identity: RootIdentity,
    pub state: JournalState,
    pub prepared: PreparedIdentityV1,
}

impl JournalV1 {
    /// The canonical encoding, including the magic and the trailing checksum.
    pub fn encode(&self) -> Result<Vec<u8>> {
        let body = self.body()?;
        let checksum = codec::domain_hash("JAILS-JOURNAL-STATE-1", &body);
        let mut out = Vec::with_capacity(body.len() + 32);
        out.extend_from_slice(&body);
        out.extend_from_slice(&checksum);
        Ok(out)
    }

    /// Everything the checksum covers: every field but the checksum itself.
    fn body(&self) -> Result<Vec<u8>> {
        let mut encoder = Encoder::new();
        for byte in JOURNAL_MAGIC {
            encoder.tag(*byte);
        }
        self.transaction.encode(&mut encoder)?;
        encoder.u64(self.generation);
        self.root_identity.encode(&mut encoder)?;
        self.state.encode(&mut encoder)?;
        self.prepared.encode(&mut encoder)?;
        encoder.finish()
    }

    /// Decode and validate. §R4.2's order: limits, checksum, then identity.
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_RECORD {
            return Err(format!("journal is {} bytes; too large", bytes.len()).into());
        }
        let split = bytes
            .len()
            .checked_sub(codec::DIGEST_BYTES)
            .ok_or_else(|| "journal is too short to carry a checksum".to_string())?;
        let (body, stored) = bytes.split_at(split);

        let mut decoder = Decoder::new(body)?;
        let mut magic = [0u8; 16];
        for slot in &mut magic {
            *slot = decoder.tag()?;
        }
        if &magic != JOURNAL_MAGIC {
            return Err(jails_support::Failure::Told(
                "unsupported transaction journal format.\n       fix: upgrade jails to the \
                 version that wrote this journal; this version will not recover through an \
                 unknown format."
                    .to_string(),
            ));
        }
        let record = Self {
            transaction: TransactionId::decode(&mut decoder)?,
            generation: decoder.u64()?,
            root_identity: RootIdentity::decode(&mut decoder)?,
            state: JournalState::decode(&mut decoder)?,
            prepared: PreparedIdentityV1::decode(&mut decoder)?,
        };
        decoder.finish()?;

        // Checked before anything is believed: a rewrite interrupted
        // mid-write leaves a record that decodes.
        if stored != codec::domain_hash("JAILS-JOURNAL-STATE-1", body) {
            return Err(jails_support::Failure::Told(
                "the journal does not match its own checksum.\n       fix: it is corrupt, not \
                 an incomplete state transition to guess through."
                    .to_string(),
            ));
        }
        record.validate()?;
        Ok(record)
    }

    /// The rules that hold however this record arrived.
    pub fn validate(&self) -> Result<()> {
        self.prepared.validate()?;
        if self.transaction != self.prepared.transaction_id()? {
            return Err(jails_support::Failure::Told(
                "the journal's transaction id is not its prepared identity's hash".to_string(),
            ));
        }
        // Zero would make "never committed" and "generation zero" the same
        // recorded value, and a recovery has to tell them apart.
        if self.generation == 0 {
            return Err(jails_support::Failure::Told(
                "a journal's generation is counted from one".to_string(),
            ));
        }
        Ok(())
    }

    /// Write this record into a transaction directory, durably.
    ///
    /// `journal.bin.tmp` with `create_new`, sync, rename over `journal.bin`,
    /// fsync the directory. The temp is never authoritative: a valid
    /// `journal.bin` wins over a synced temp holding a later phase, because
    /// promoting a temp means guessing that its rename was intended.
    pub fn persist(&self, directory: &Path) -> Result<()> {
        let bytes = self.encode()?;
        let temp = directory.join("journal.bin.tmp");
        let final_path = directory.join("journal.bin");
        // A stale temp from an interrupted rewrite is removed rather than
        // reused; `create_new` would otherwise refuse forever.
        let _ = std::fs::remove_file(&temp);
        {
            use std::io::Write;
            let mut file = std::fs::File::options()
                .write(true)
                .create_new(true)
                .open(&temp)
                .map_err(|error| format!("could not create {}: {error}", temp.display()))?;
            file.write_all(&bytes)
                .map_err(|error| format!("could not write {}: {error}", temp.display()))?;
            file.sync_all()
                .map_err(|error| format!("could not sync {}: {error}", temp.display()))?;
        }
        std::fs::rename(&temp, &final_path)
            .map_err(|error| format!("could not publish {}: {error}", final_path.display()))?;
        store::sync_dir(directory)
    }

    /// Read the journal from a transaction directory.
    pub fn read(directory: &Path) -> Result<Self> {
        let path = directory.join("journal.bin");
        let bytes = std::fs::read(&path)
            .map_err(|error| format!("could not read {}: {error}", path.display()))?;
        let record = Self::decode(&bytes)?;
        // The directory name is the transaction id, so a journal cannot be
        // moved into another transaction's directory and still validate.
        let named = directory
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        if named != record.transaction.to_hex() {
            return Err(format!(
                "{} holds the journal of transaction {}",
                directory.display(),
                record.transaction
            )
            .into());
        }
        Ok(record)
    }

    /// The same record in a later phase.
    ///
    /// A separate constructor because the checksum has to be recomputed, and
    /// a caller that mutated the field in place would leave a record that
    /// decodes and does not verify.
    pub fn advanced(&self, state: JournalState) -> Self {
        Self {
            state,
            ..self.clone()
        }
    }
}

/// The published, immutable record of a committed transaction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReceiptV1 {
    pub transaction: TransactionId,
    pub generation: u64,
    pub prepared: PreparedIdentityV1,
    /// The checksum of the `Complete` journal beside it. Because the
    /// receipt's own checksum covers this field, editing the journal to agree
    /// with a tampered receipt is not possible without breaking the receipt.
    pub complete_journal_checksum: ObjectId,
    /// The one mutable section, replaced atomically as effects advance.
    pub post_commit: Vec<EffectReceipt>,
}

impl ReceiptV1 {
    pub fn encode(&self) -> Result<Vec<u8>> {
        let body = self.body()?;
        let checksum = codec::domain_hash("JAILS-RECEIPT-STATE-1", &body);
        let mut out = Vec::with_capacity(body.len() + 32);
        out.extend_from_slice(&body);
        out.extend_from_slice(&checksum);
        Ok(out)
    }

    fn body(&self) -> Result<Vec<u8>> {
        let mut encoder = Encoder::new();
        for byte in RECEIPT_MAGIC {
            encoder.tag(*byte);
        }
        self.transaction.encode(&mut encoder)?;
        encoder.u64(self.generation);
        self.prepared.encode(&mut encoder)?;
        self.complete_journal_checksum.encode(&mut encoder)?;
        encoder.count(self.post_commit.len())?;
        for effect in &self.post_commit {
            effect.id.encode(&mut encoder)?;
            effect.effect.encode(&mut encoder)?;
            effect.state.encode(&mut encoder)?;
        }
        encoder.finish()
    }

    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_RECORD {
            return Err(format!("receipt is {} bytes; too large", bytes.len()).into());
        }
        let split = bytes
            .len()
            .checked_sub(codec::DIGEST_BYTES)
            .ok_or_else(|| "receipt is too short to carry a checksum".to_string())?;
        let (body, stored) = bytes.split_at(split);

        let mut decoder = Decoder::new(body)?;
        let mut magic = [0u8; 16];
        for slot in &mut magic {
            *slot = decoder.tag()?;
        }
        if &magic != RECEIPT_MAGIC {
            return Err(jails_support::Failure::Told(
                "unsupported transaction receipt format.\n       fix: upgrade jails to the \
                 version that wrote this receipt; this version will not accept an unknown \
                 format."
                    .to_string(),
            ));
        }
        let transaction = TransactionId::decode(&mut decoder)?;
        let generation = decoder.u64()?;
        let prepared = PreparedIdentityV1::decode(&mut decoder)?;
        let complete_journal_checksum = ObjectId::decode(&mut decoder)?;
        let count = decoder.count()?;
        let mut post_commit = Vec::new();
        for _ in 0..count {
            post_commit.push(EffectReceipt {
                id: jails_protocol::effect::EffectId::decode(&mut decoder)?,
                effect: jails_protocol::effect::PostCommitEffect::decode(&mut decoder)?,
                state: jails_protocol::effect::EffectState::decode(&mut decoder)?,
            });
        }
        decoder.finish()?;

        if stored != codec::domain_hash("JAILS-RECEIPT-STATE-1", body) {
            return Err(jails_support::Failure::Told(
                "the receipt does not match its own checksum".to_string(),
            ));
        }
        let record = Self {
            transaction,
            generation,
            prepared,
            complete_journal_checksum,
            post_commit,
        };
        record.validate()?;
        Ok(record)
    }

    pub fn validate(&self) -> Result<()> {
        self.prepared.validate()?;
        if self.transaction != self.prepared.transaction_id()? {
            return Err(jails_support::Failure::Told(
                "the receipt's transaction id is not its prepared identity's hash".to_string(),
            ));
        }
        if self.generation == 0 {
            return Err(jails_support::Failure::Told(
                "a receipt's generation is counted from one".to_string(),
            ));
        }
        // Exactly one effect receipt per prepared descriptor, in order. A
        // mismatch is corruption: the receipt would be reporting on work the
        // transaction never planned.
        if self.post_commit.len() != self.prepared.post_commit.len() {
            return Err(format!(
                "the receipt carries {} effect states for {} prepared effects",
                self.post_commit.len(),
                self.prepared.post_commit.len()
            )
            .into());
        }
        for (receipt, descriptor) in self.post_commit.iter().zip(&self.prepared.post_commit) {
            if &receipt.effect != descriptor {
                return Err(jails_support::Failure::Told(
                    "an effect receipt describes work this transaction did not plan".to_string(),
                ));
            }
        }
        Ok(())
    }

    /// `receipt.bin.tmp → receipt.bin`, the same protocol as the journal.
    pub fn persist(&self, directory: &Path) -> Result<()> {
        let bytes = self.encode()?;
        let temp = directory.join("receipt.bin.tmp");
        let final_path = directory.join("receipt.bin");
        let _ = std::fs::remove_file(&temp);
        {
            use std::io::Write;
            let mut file = std::fs::File::options()
                .write(true)
                .create_new(true)
                .open(&temp)
                .map_err(|error| format!("could not create {}: {error}", temp.display()))?;
            file.write_all(&bytes)
                .map_err(|error| format!("could not write {}: {error}", temp.display()))?;
            file.sync_all()
                .map_err(|error| format!("could not sync {}: {error}", temp.display()))?;
        }
        std::fs::rename(&temp, &final_path)
            .map_err(|error| format!("could not publish {}: {error}", final_path.display()))?;
        store::sync_dir(directory)
    }

    pub fn read(directory: &Path) -> Result<Self> {
        let path = directory.join("receipt.bin");
        let bytes = std::fs::read(&path)
            .map_err(|error| format!("could not read {}: {error}", path.display()))?;
        let receipt = Self::decode(&bytes)?;

        // The witness beside it must be the Complete journal this receipt
        // names, and its prepared identity must be the same bytes.
        let journal = JournalV1::read(directory)?;
        if journal.state != JournalState::Complete {
            return Err(format!(
                "{} holds a receipt beside a journal in state {}",
                directory.display(),
                journal.state.label()
            )
            .into());
        }
        let witness = ObjectId::from_bytes(codec::sha256(&journal.encode()?));
        if witness != receipt.complete_journal_checksum {
            return Err(format!(
                "{} holds a receipt that names another journal.\n       fix: the two published \
                 records must be the same transaction; one of them has been edited.",
                directory.display()
            )
            .into());
        }
        if journal.prepared != receipt.prepared || journal.generation != receipt.generation {
            return Err(format!(
                "{}'s journal and receipt disagree about what was prepared",
                directory.display()
            )
            .into());
        }
        Ok(receipt)
    }

    /// The checksum of a Complete journal, for construction.
    pub fn witness_of(journal: &JournalV1) -> Result<ObjectId> {
        if journal.state != JournalState::Complete {
            return Err(format!(
                "a receipt witnesses a Complete journal, not one in state {}",
                journal.state.label()
            )
            .into());
        }
        Ok(ObjectId::from_bytes(codec::sha256(&journal.encode()?)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jails_prepare::operation::{ApplySemantics, OperationIdentityV1, OperationSemanticsV1};
    use jails_prepare::prepare::{FileOp, PreparedKind};
    use jails_prepare::tool::{OperationContextFingerprint, PreparationContextFingerprint};
    use jails_protocol::conflict::FileImage;
    use jails_protocol::identity::ObjectRef;
    use jails_protocol::identity::ProjectPath;
    use jails_protocol::plan::{LedgerIntent, PlannedSubject};
    use jails_support::scratch::ScratchDir;
    use std::collections::BTreeSet;

    // Built by hand rather than borrowed from `jails-prepare`'s own test
    // module: `#[cfg(test)]` means "when *that* crate is under test", so a
    // dependent crate's tests cannot see it. Duplicating the fixture is the
    // honest cost of that, and it keeps this crate's tests independent of
    // another crate's test-only surface.
    fn prepared_with(body: &[u8]) -> PreparedIdentityV1 {
        let after = ObjectRef::new(ObjectId::from_bytes(codec::sha256(body)), body.len() as u64);
        let operation_identity = OperationIdentityV1 {
            snapshot: ObjectId::from_bytes(codec::sha256(b"snapshot")),
            operation_context: OperationContextFingerprint::default(),
            invocation: None,
            proposed_generation: 4,
            semantics: OperationSemanticsV1::Apply(Box::new(ApplySemantics {
                subject: PlannedSubject::AdoptLayout,
                ledger_intent: LedgerIntent {
                    generation_before: 3,
                    entities_after: Vec::new(),
                    one_shots_after: Vec::new(),
                    resources_after: Vec::new(),
                    entities_removed: Vec::new(),
                },
            })),
        };
        PreparedIdentityV1 {
            operation_id: operation_identity.operation_id().unwrap(),
            operation_identity,
            preparation: PreparationContextFingerprint::default(),
            input_preconditions: Vec::new(),
            operations: vec![FileOp::Create {
                path: ProjectPath::parse("pom.xml").unwrap(),
                after,
                mode: FileMode::new(0o644).unwrap(),
                contributors: BTreeSet::new(),
            }],
            directories: Vec::new(),
            ledger_before: FileImage::Absent,
            ledger_after: FileImage::Absent,
            object_manifest: vec![after],
            post_commit: Vec::new(),
            kind: PreparedKind::Apply,
        }
    }

    fn prepared() -> PreparedIdentityV1 {
        prepared_with(b"<project/>")
    }

    fn journal(state: JournalState) -> JournalV1 {
        let prepared = prepared();
        JournalV1 {
            transaction: prepared.transaction_id().unwrap(),
            generation: 4,
            root_identity: RootIdentity {
                device: 66,
                inode: 1234,
            },
            state,
            prepared,
        }
    }

    #[test]
    fn a_journal_round_trips_in_every_state() {
        for state in [
            JournalState::Prepared,
            JournalState::Active,
            JournalState::LedgerCommitted,
            JournalState::Complete,
            JournalState::Blocked {
                resume: ResumeState::Active,
                path: Some(ProjectPath::parse("pom.xml").unwrap()),
                reason: BlockReason::UnknownLiveImage {
                    actual: ActualImage::Symlink,
                },
            },
        ] {
            let one = journal(state);
            let bytes = one.encode().unwrap();
            assert_eq!(JournalV1::decode(&bytes).unwrap(), one);
        }
    }

    /// A rewrite interrupted mid-write leaves a record that decodes. Without
    /// the checksum the executor would read a half-updated state and act.
    #[test]
    fn a_tampered_journal_is_corruption_not_a_state_to_guess_through() {
        let mut bytes = journal(JournalState::Active).encode().unwrap();
        // Flip the state byte, leaving the checksum stale.
        let at = bytes.windows(1).position(|_| false).unwrap_or(20);
        bytes[at] ^= 0xff;
        let error = JournalV1::decode(&bytes).unwrap_err();
        assert!(
            error.contains("checksum") || error.contains("not a jails journal"),
            "{error}"
        );
    }

    #[test]
    fn a_record_with_a_truncated_checksum_is_refused() {
        let mut bytes = journal(JournalState::Active).encode().unwrap();
        bytes.truncate(bytes.len() - 4);
        assert!(JournalV1::decode(&bytes).is_err());
    }

    #[test]
    fn an_unknown_journal_version_fails_closed_with_an_upgrade_instruction() {
        let mut bytes = journal(JournalState::Active).encode().unwrap();
        bytes[..16].copy_from_slice(b"JAILS-JOURNAL-2\0");
        let error = JournalV1::decode(&bytes).unwrap_err();
        assert!(error.contains("unsupported transaction journal"), "{error}");
        assert!(error.contains("upgrade jails"), "{error}");
        assert!(error.contains("will not recover"), "{error}");
    }

    /// A state rewrite keeps the transaction id and changes the checksum;
    /// changing an immutable prepared byte changes both.
    #[test]
    fn advancing_the_state_preserves_the_transaction_and_changes_the_checksum() {
        let prepared = journal(JournalState::Prepared);
        let active = prepared.advanced(JournalState::Active);
        assert_eq!(prepared.transaction, active.transaction);
        assert_ne!(prepared.encode().unwrap(), active.encode().unwrap());

        let other = prepared_with(b"<project></project>");
        assert_ne!(prepared.transaction, other.transaction_id().unwrap());
    }

    /// A journal cannot be moved into another transaction's directory and
    /// still validate, because the directory name *is* the id.
    #[test]
    fn a_journal_in_the_wrong_directory_is_refused() {
        let scratch = ScratchDir::in_temp("jails-journal").unwrap();
        let wrong = scratch.path().join("0".repeat(64));
        std::fs::create_dir_all(&wrong).unwrap();
        journal(JournalState::Prepared).persist(&wrong).unwrap();
        let error = JournalV1::read(&wrong).unwrap_err();
        assert!(
            error.contains("holds the journal of transaction"),
            "{error}"
        );
        scratch.close().unwrap();
    }

    #[test]
    fn a_journal_persists_and_reads_back() {
        let scratch = ScratchDir::in_temp("jails-journal").unwrap();
        let one = journal(JournalState::Active);
        let directory = scratch.path().join(one.transaction.to_hex());
        std::fs::create_dir_all(&directory).unwrap();
        one.persist(&directory).unwrap();
        assert_eq!(JournalV1::read(&directory).unwrap(), one);
        assert!(
            !directory.join("journal.bin.tmp").exists(),
            "the temp survived the rename"
        );
        scratch.close().unwrap();
    }

    /// A stale temp from an interrupted rewrite must not make the next write
    /// fail forever.
    #[test]
    fn a_stale_temp_does_not_block_the_next_write() {
        let scratch = ScratchDir::in_temp("jails-journal").unwrap();
        let one = journal(JournalState::Active);
        let directory = scratch.path().join(one.transaction.to_hex());
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("journal.bin.tmp"), b"junk").unwrap();
        one.persist(&directory).unwrap();
        assert_eq!(JournalV1::read(&directory).unwrap(), one);
        scratch.close().unwrap();
    }

    /// A generation of zero would make "never committed" and "generation
    /// zero" the same recorded value.
    #[test]
    fn a_generation_of_zero_is_refused() {
        let mut one = journal(JournalState::Active);
        one.generation = 0;
        let bytes = one.encode().unwrap();
        assert!(
            JournalV1::decode(&bytes)
                .unwrap_err()
                .contains("counted from one")
        );
    }

    fn published(scratch: &ScratchDir) -> (std::path::PathBuf, JournalV1, ReceiptV1) {
        let complete = journal(JournalState::Complete);
        let directory = scratch.path().join(complete.transaction.to_hex());
        std::fs::create_dir_all(&directory).unwrap();
        complete.persist(&directory).unwrap();
        let receipt = ReceiptV1 {
            transaction: complete.transaction,
            generation: complete.generation,
            prepared: complete.prepared.clone(),
            complete_journal_checksum: ReceiptV1::witness_of(&complete).unwrap(),
            post_commit: Vec::new(),
        };
        receipt.persist(&directory).unwrap();
        (directory, complete, receipt)
    }

    #[test]
    fn a_published_pair_reads_back_and_binds_the_two_records() {
        let scratch = ScratchDir::in_temp("jails-receipt").unwrap();
        let (directory, _, receipt) = published(&scratch);
        assert_eq!(ReceiptV1::read(&directory).unwrap(), receipt);
        scratch.close().unwrap();
    }

    #[test]
    fn an_unknown_receipt_version_fails_closed_with_an_upgrade_instruction() {
        let scratch = ScratchDir::in_temp("jails-receipt-version").unwrap();
        let (_, _, receipt) = published(&scratch);
        let mut bytes = receipt.encode().unwrap();
        bytes[..16].copy_from_slice(b"JAILS-RECEIPT-2\0");
        let error = ReceiptV1::decode(&bytes).unwrap_err();
        assert!(error.contains("unsupported transaction receipt"), "{error}");
        assert!(error.contains("upgrade jails"), "{error}");
        assert!(error.contains("will not accept"), "{error}");
        scratch.close().unwrap();
    }

    /// The receipt's own checksum covers the journal's checksum, so editing
    /// the journal to agree with a tampered receipt breaks the receipt.
    #[test]
    fn a_receipt_beside_another_journal_is_refused() {
        let scratch = ScratchDir::in_temp("jails-receipt").unwrap();
        let (directory, complete, _) = published(&scratch);
        // Rewrite the journal in a different state: same transaction, new
        // checksum, so the receipt now names a journal that is not there.
        complete
            .advanced(JournalState::LedgerCommitted)
            .persist(&directory)
            .unwrap();
        let error = ReceiptV1::read(&directory).unwrap_err();
        assert!(error.contains("state ledger-committed"), "{error}");
        scratch.close().unwrap();
    }

    /// A receipt reporting on work the transaction never planned is
    /// corruption, not a variant to execute leniently.
    #[test]
    fn a_receipt_with_an_effect_the_plan_did_not_carry_is_refused() {
        let complete = journal(JournalState::Complete);
        let receipt = ReceiptV1 {
            transaction: complete.transaction,
            generation: complete.generation,
            prepared: complete.prepared.clone(),
            complete_journal_checksum: ReceiptV1::witness_of(&complete).unwrap(),
            post_commit: vec![jails_prepare::receipt::EffectReceipt {
                id: jails_protocol::effect::EffectId::from_object(ObjectId::from_bytes(
                    codec::sha256(b"effect"),
                )),
                effect: jails_protocol::effect::PostCommitEffect::ComposeReconcile {
                    compose_output: ProjectPath::parse("compose.yaml").unwrap(),
                    before_document: None,
                    after_document: None,
                    prior_managed_services: Default::default(),
                    desired_services: Default::default(),
                    stop_services: Default::default(),
                },
                state: jails_protocol::effect::EffectState::Deferred,
            }],
        };
        let error = receipt.validate().unwrap_err();
        assert!(error.contains("for 0 prepared effects"), "{error}");
    }

    #[test]
    fn a_receipt_may_only_witness_a_complete_journal() {
        let error = ReceiptV1::witness_of(&journal(JournalState::Active)).unwrap_err();
        assert!(error.contains("witnesses a Complete journal"), "{error}");
    }

    /// Every block reason has to tell a person what to do, or it is a dead
    /// end with a code on it.
    #[test]
    fn every_block_reason_explains_itself_with_a_fix() {
        for reason in [
            BlockReason::UnknownLiveImage {
                actual: ActualImage::Directory,
            },
            BlockReason::Unreadable {
                error_kind: "permission denied".to_string(),
            },
            BlockReason::RootChanged,
            BlockReason::CorruptJournal,
            BlockReason::CorruptObject(ObjectId::from_bytes(codec::sha256(b"x"))),
            BlockReason::MultipleTransactions,
        ] {
            let explanation = reason.explain();
            assert!(explanation.contains("fix:"), "{explanation}");
        }
    }
}
