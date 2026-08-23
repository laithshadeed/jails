//! Everything planning is allowed to know, captured once.
//!
//! ## The property this exists for
//!
//! plan.md §R2: *"Planning must stop learning by mutating or rereading the
//! live tree."* The evidence it cites is in this repository:
//! `app.rs::project_at` deliberately reloads the project after every applied
//! capability, because the previous step already rewrote the POM. That is
//! correct for an imperative loop and it is proof the loop cannot plan one
//! atomic manifest — a plan whose inputs change while it is being made is not
//! a plan, it is a sequence of guesses.
//!
//! So a snapshot is taken once, and **a read of anything it does not declare
//! is an error rather than a filesystem access**. That single rule is what
//! makes two runs produce the same plan, and what lets a later stale check
//! know exactly which facts a decision rested on.
//!
//! ## Absent is a fact, not a gap
//!
//! A file that is not there is recorded as [`InputPrecondition::Absent`], not
//! omitted. The distinction is load-bearing twice over: it is how an
//! undeclared read stays distinguishable from a declared miss, and it is how a
//! file appearing later invalidates a plan that depended on its absence.
//! §R2's gate says it directly — *"absent ledger is distinct from an empty
//! ledger file"*.
//!
//! Directories carry a listing hash for the same reason. A plan that enumerated
//! a directory depends on **which entries were there**, so a new sibling has to
//! be able to invalidate it.

use crate::Result;
use crate::conflict::FileMode;
use crate::entity::ExternalPathId;
use crate::identity::{ObjectId, ProjectPath, TemplateId, TransactionId};
use jails_support::codec::{self, Decoder, Encoder, ordered};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

/// One file's captured bytes and identity.
///
/// The bytes are shared rather than copied per reader: a snapshot is read many
/// times by many planners and a POM is not small.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SnapshotFile {
    pub bytes: Arc<[u8]>,
    pub sha256: ObjectId,
    pub len: u64,
    pub mode: FileMode,
}

impl SnapshotFile {
    /// Capture bytes, deriving the identity from them so the two cannot
    /// disagree.
    pub fn capture(bytes: Vec<u8>, mode: FileMode) -> Self {
        let sha256 = ObjectId::from_bytes(codec::sha256(&bytes));
        let len = bytes.len() as u64;
        Self {
            bytes: Arc::from(bytes),
            sha256,
            len,
            mode,
        }
    }
}

/// A source outside the project that a plan may depend on.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub enum ExternalInputId {
    AppManifest,
    UserTemplate(TemplateId),
    CasesBrief { path_id: ExternalPathId },
}

/// One pre-schema-2 machine file.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub enum LegacySourcePath {
    Schema1Ledger,
    AppState,
    IntentFiles { name: LegacyFileName },
    ModelFiles { name: LegacyFileName },
    GlobalFiles,
    VersionFile,
}

/// One UTF-8 component ending `.files`.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct LegacyFileName(String);

impl LegacyFileName {
    pub fn parse(text: &str) -> Result<Self> {
        if !text.ends_with(".files") {
            return Err(format!("`{text}` is not a legacy `.files` component"));
        }
        if text.contains('/') || text.contains('\\') || text.contains("..") {
            return Err(format!("`{text}` is a path, not one component"));
        }
        if text.len() <= ".files".len() {
            return Err("a legacy component has no stem".to_string());
        }
        Ok(Self(text.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LegacyDirectoryKind {
    Intents,
    Models,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LegacyDirectoryState {
    Absent,
    Present {
        entries: Vec<LegacyFileName>,
        entries_sha256: ObjectId,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MachineRootPresence {
    Absent,
    Present,
}

/// One fact a plan depended on, and the exact shape it was in.
///
/// The tags are fixed by §R3.1 and may never be reused for another meaning:
/// they reach a recovered journal, and a tag that changed meaning between
/// versions would make an old record decode as a plausible wrong answer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InputPrecondition {
    Absent {
        path: ProjectPath,
    },
    File {
        path: ProjectPath,
        sha256: ObjectId,
        len: u64,
        mode: FileMode,
    },
    Directory {
        path: ProjectPath,
        entries: Vec<ProjectPath>,
        entries_sha256: ObjectId,
    },
    ExternalAbsent {
        id: ExternalInputId,
    },
    ExternalFile {
        id: ExternalInputId,
        sha256: ObjectId,
        len: u64,
    },
    LegacyAbsent {
        path: LegacySourcePath,
    },
    LegacyFile {
        path: LegacySourcePath,
        sha256: ObjectId,
        len: u64,
        mode: FileMode,
    },
    LegacyDirectory {
        kind: LegacyDirectoryKind,
        state: LegacyDirectoryState,
    },
    MachineRoot {
        presence: MachineRootPresence,
    },
    MachineReceipt {
        transaction: TransactionId,
        generation: u64,
        record_checksum: ObjectId,
    },
}

impl InputPrecondition {
    /// The fixed wire tag. §R3.1 numbers these; the gaps are the
    /// machine-object and machine-receipt-directory variants R4 adds.
    pub fn tag(&self) -> u8 {
        match self {
            Self::Absent { .. } => 0,
            Self::File { .. } => 1,
            Self::Directory { .. } => 2,
            Self::ExternalAbsent { .. } => 3,
            Self::ExternalFile { .. } => 4,
            Self::LegacyAbsent { .. } => 5,
            Self::LegacyFile { .. } => 6,
            Self::LegacyDirectory { .. } => 7,
            Self::MachineReceipt { .. } => 9,
            Self::MachineRoot { .. } => 11,
        }
    }

    /// The canonical sort key, so a read set has one encoding.
    fn order_key(&self) -> (u8, String) {
        let detail = match self {
            Self::Absent { path } | Self::File { path, .. } | Self::Directory { path, .. } => {
                path.as_str().to_string()
            }
            Self::ExternalAbsent { id } | Self::ExternalFile { id, .. } => format!("{id:?}"),
            Self::LegacyAbsent { path } | Self::LegacyFile { path, .. } => format!("{path:?}"),
            Self::LegacyDirectory { kind, .. } => format!("{kind:?}"),
            Self::MachineReceipt { transaction, .. } => transaction.to_hex(),
            Self::MachineRoot { .. } => String::new(),
        };
        (self.tag(), detail)
    }
}

/// Every fact one plan rested on, in canonical order.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ReadSet {
    inputs: Vec<InputPrecondition>,
}

impl ReadSet {
    /// Sorts and rejects a duplicate subject, so one set has one encoding.
    pub fn new(mut inputs: Vec<InputPrecondition>) -> Result<Self> {
        inputs.sort_by_key(|input| input.order_key());
        let mut previous: Option<(u8, String)> = None;
        for input in &inputs {
            let key = input.order_key();
            ordered(previous.as_ref(), &key)?;
            previous = Some(key);
        }
        Ok(Self { inputs })
    }

    pub fn inputs(&self) -> &[InputPrecondition] {
        &self.inputs
    }

    pub fn is_empty(&self) -> bool {
        self.inputs.is_empty()
    }
}

/// The one canonical root a snapshot was taken at.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CanonicalRoot(String);

impl CanonicalRoot {
    pub fn new(path: &str) -> Result<Self> {
        if !path.starts_with('/') {
            return Err(format!(
                "`{path}` is not a canonical absolute root.\n       fix: resolve it before \
                 taking a snapshot, so two runs from different directories agree."
            ));
        }
        Ok(Self(path.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Everything planning may know about one project, captured once.
#[derive(Clone, Debug)]
pub struct ProjectSnapshot {
    root: CanonicalRoot,
    files: BTreeMap<ProjectPath, SnapshotFile>,
    absences: BTreeSet<ProjectPath>,
    directories: BTreeMap<ProjectPath, Vec<ProjectPath>>,
}

/// What a declared read found.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Captured<'a> {
    Present(&'a SnapshotFile),
    /// Declared, and it was not there. A fact, not a gap.
    Absent,
}

impl ProjectSnapshot {
    pub fn new(
        root: CanonicalRoot,
        files: BTreeMap<ProjectPath, SnapshotFile>,
        absences: BTreeSet<ProjectPath>,
        directories: BTreeMap<ProjectPath, Vec<ProjectPath>>,
    ) -> Result<Self> {
        // A path cannot be both present and absent: one of the two readings
        // would silently win, and which one is an implementation detail.
        for path in &absences {
            if files.contains_key(path) {
                return Err(format!("`{path}` is captured as both present and absent"));
            }
        }
        Ok(Self {
            root,
            files,
            absences,
            directories,
        })
    }

    pub fn root(&self) -> &CanonicalRoot {
        &self.root
    }

    /// Read a declared file.
    ///
    /// **An undeclared read is an error**, and that is the whole discipline: a
    /// planner that could reach past the snapshot would make decisions on
    /// facts nothing recorded, and the stale check at commit would have
    /// nothing to compare. The message names the path so the fix — declare it
    /// — is obvious.
    pub fn read(&self, path: &ProjectPath) -> Result<Captured<'_>> {
        if let Some(file) = self.files.get(path) {
            return Ok(Captured::Present(file));
        }
        if self.absences.contains(path) {
            return Ok(Captured::Absent);
        }
        Err(format!(
            "`{path}` was not captured, so planning may not read it.\n       fix: declare it in \
             the read set. Reaching past the snapshot would decide on a fact nothing recorded, \
             and the commit-time staleness check would have nothing to compare."
        ))
    }

    /// List a declared directory. Undeclared is an error for the same reason.
    pub fn list(&self, path: &ProjectPath) -> Result<&[ProjectPath]> {
        self.directories
            .get(path)
            .map(Vec::as_slice)
            .ok_or_else(|| {
                format!(
                    "directory `{path}` was not enumerated, so planning may not list it.\n       \
                 fix: declare it in the read set."
                )
            })
    }

    /// The complete read set this snapshot justifies.
    ///
    /// Every captured fact appears, including every absence: a plan that
    /// depended on a file *not* being there must be invalidated when one
    /// appears.
    pub fn read_set(&self) -> Result<ReadSet> {
        let mut inputs = Vec::new();
        for (path, file) in &self.files {
            inputs.push(InputPrecondition::File {
                path: path.clone(),
                sha256: file.sha256,
                len: file.len,
                mode: file.mode,
            });
        }
        for path in &self.absences {
            inputs.push(InputPrecondition::Absent { path: path.clone() });
        }
        for (path, entries) in &self.directories {
            inputs.push(InputPrecondition::Directory {
                path: path.clone(),
                entries: entries.clone(),
                entries_sha256: directory_digest(entries)?,
            });
        }
        ReadSet::new(inputs)
    }
}

/// `SHA256("JAILS-DIRECTORY-1" || encode(entries))`. The empty directory
/// hashes a real value rather than being indistinguishable from an absent one.
pub fn directory_digest(entries: &[ProjectPath]) -> Result<ObjectId> {
    let mut encoder = Encoder::new();
    encoder.count(entries.len())?;
    let mut previous: Option<&ProjectPath> = None;
    for entry in entries {
        ordered(previous, entry)?;
        previous = Some(entry);
        entry.encode(&mut encoder)?;
    }
    Ok(ObjectId::from_bytes(codec::domain_hash(
        "JAILS-DIRECTORY-1",
        &encoder.finish()?,
    )))
}

/// `SHA256("JAILS-SNAPSHOT-1" || encode(read set))`.
pub fn snapshot_digest(read_set: &ReadSet) -> Result<ObjectId> {
    let mut encoder = Encoder::new();
    encoder.count(read_set.inputs().len())?;
    for input in read_set.inputs() {
        encoder.tag(input.tag());
        match input {
            InputPrecondition::Absent { path } => path.encode(&mut encoder)?,
            InputPrecondition::File {
                path,
                sha256,
                len,
                mode,
            } => {
                path.encode(&mut encoder)?;
                sha256.encode(&mut encoder);
                encoder.u64(*len);
                encoder.u32(mode.bits());
            }
            InputPrecondition::Directory {
                path,
                entries_sha256,
                ..
            } => {
                path.encode(&mut encoder)?;
                entries_sha256.encode(&mut encoder);
            }
            other => {
                // The remaining variants reach the wire with R4's journal; for
                // now their debug projection is stable and distinct, which is
                // enough for a snapshot digest that never leaves memory.
                encoder.string(&format!("{other:?}"))?;
            }
        }
    }
    Ok(ObjectId::from_bytes(codec::domain_hash(
        "JAILS-SNAPSHOT-1",
        &encoder.finish()?,
    )))
}

/// Decoder counterpart for the two variants that already reach the wire.
pub fn decode_precondition(decoder: &mut Decoder<'_>) -> Result<InputPrecondition> {
    Ok(match decoder.tag()? {
        0 => InputPrecondition::Absent {
            path: ProjectPath::decode(decoder)?,
        },
        1 => InputPrecondition::File {
            path: ProjectPath::decode(decoder)?,
            sha256: ObjectId::decode(decoder)?,
            len: decoder.u64()?,
            mode: FileMode::new(decoder.u32()?)?,
        },
        other => return Err(format!("unknown input precondition tag {other}")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(text: &str) -> ProjectPath {
        ProjectPath::parse(text).unwrap()
    }

    fn mode() -> FileMode {
        FileMode::new(0o644).unwrap()
    }

    fn snapshot() -> ProjectSnapshot {
        ProjectSnapshot::new(
            CanonicalRoot::new("/srv/demo").unwrap(),
            BTreeMap::from([(
                path("pom.xml"),
                SnapshotFile::capture(b"<project/>".to_vec(), mode()),
            )]),
            BTreeSet::from([path("compose.yaml")]),
            BTreeMap::from([(path("src/main/java"), vec![path("src/main/java/App.java")])]),
        )
        .unwrap()
    }

    /// The whole discipline. A planner that could reach past the snapshot
    /// would decide on facts nothing recorded, and the commit-time staleness
    /// check would have nothing to compare.
    #[test]
    fn an_undeclared_read_is_an_error_not_a_filesystem_access() {
        let snapshot = snapshot();
        let error = snapshot.read(&path("src/main/java/App.java")).unwrap_err();
        assert!(error.contains("was not captured"), "{error}");
        assert!(error.contains("fix: declare it"), "{error}");

        let error = snapshot.list(&path("src/test/java")).unwrap_err();
        assert!(error.contains("not enumerated"), "{error}");
    }

    /// §R2's gate, directly: an absent file is a *fact*, distinguishable from
    /// one nobody declared.
    #[test]
    fn a_declared_absence_is_a_fact_and_not_a_gap() {
        let snapshot = snapshot();
        assert_eq!(
            snapshot.read(&path("compose.yaml")).unwrap(),
            Captured::Absent
        );
        assert!(snapshot.read(&path("never-mentioned.yaml")).is_err());
    }

    #[test]
    fn a_declared_file_reads_back_with_its_identity() {
        let snapshot = snapshot();
        match snapshot.read(&path("pom.xml")).unwrap() {
            Captured::Present(file) => {
                assert_eq!(&*file.bytes, b"<project/>");
                assert_eq!(file.len, 10);
                assert_eq!(
                    file.sha256,
                    ObjectId::from_bytes(codec::sha256(b"<project/>"))
                );
            }
            other => panic!("{other:?}"),
        }
    }

    /// One of the two readings would silently win, and which one is an
    /// implementation detail.
    #[test]
    fn a_path_cannot_be_both_present_and_absent() {
        let error = ProjectSnapshot::new(
            CanonicalRoot::new("/srv/demo").unwrap(),
            BTreeMap::from([(path("pom.xml"), SnapshotFile::capture(vec![], mode()))]),
            BTreeSet::from([path("pom.xml")]),
            BTreeMap::new(),
        )
        .unwrap_err();
        assert!(error.contains("both present and absent"), "{error}");
    }

    /// Two runs from different directories must agree, so the root is resolved
    /// before a snapshot is taken.
    #[test]
    fn a_snapshot_root_is_canonical() {
        assert!(CanonicalRoot::new("/srv/demo").is_ok());
        let error = CanonicalRoot::new("demo").unwrap_err();
        assert!(error.contains("canonical absolute root"), "{error}");
    }

    /// A plan that depended on a file *not* being there must be invalidated
    /// when one appears, so every absence is in the read set.
    #[test]
    fn the_read_set_carries_absences_and_directory_listings() {
        let read_set = snapshot().read_set().unwrap();
        let tags: Vec<u8> = read_set.inputs().iter().map(|input| input.tag()).collect();
        assert!(tags.contains(&0), "an absence is a precondition");
        assert!(tags.contains(&1), "so is a file");
        assert!(tags.contains(&2), "so is a directory listing");
        assert_eq!(read_set.inputs().len(), 3);
    }

    /// A new sibling has to be able to invalidate a plan that enumerated the
    /// directory.
    #[test]
    fn a_directory_digest_changes_when_an_entry_appears() {
        let before = directory_digest(&[path("a/x.java")]).unwrap();
        let after = directory_digest(&[path("a/x.java"), path("a/y.java")]).unwrap();
        assert_ne!(before, after);

        // The empty directory hashes a real value rather than being
        // indistinguishable from an absent one.
        let empty = directory_digest(&[]).unwrap();
        assert_ne!(empty, before);
    }

    /// An unsorted or duplicated listing refuses, so one directory has one
    /// digest.
    #[test]
    fn a_directory_listing_must_be_canonical() {
        assert!(directory_digest(&[path("a/y.java"), path("a/x.java")]).is_err());
        assert!(directory_digest(&[path("a/x.java"), path("a/x.java")]).is_err());
    }

    /// Two snapshots of the same facts hash the same, whatever order the
    /// inputs were discovered in.
    #[test]
    fn the_snapshot_digest_is_order_independent() {
        let one = ReadSet::new(vec![
            InputPrecondition::Absent {
                path: path("b.txt"),
            },
            InputPrecondition::File {
                path: path("a.txt"),
                sha256: ObjectId::from_bytes(codec::sha256(b"a")),
                len: 1,
                mode: mode(),
            },
        ])
        .unwrap();
        let other = ReadSet::new(vec![
            InputPrecondition::File {
                path: path("a.txt"),
                sha256: ObjectId::from_bytes(codec::sha256(b"a")),
                len: 1,
                mode: mode(),
            },
            InputPrecondition::Absent {
                path: path("b.txt"),
            },
        ])
        .unwrap();
        assert_eq!(one, other);
        assert_eq!(
            snapshot_digest(&one).unwrap(),
            snapshot_digest(&other).unwrap()
        );
    }

    /// A changed byte changes the digest, which is what a staleness check
    /// rests on.
    #[test]
    fn a_changed_input_changes_the_snapshot_digest() {
        let base = snapshot().read_set().unwrap();
        let changed = ProjectSnapshot::new(
            CanonicalRoot::new("/srv/demo").unwrap(),
            BTreeMap::from([(
                path("pom.xml"),
                SnapshotFile::capture(b"<project></project>".to_vec(), mode()),
            )]),
            BTreeSet::from([path("compose.yaml")]),
            BTreeMap::from([(path("src/main/java"), vec![path("src/main/java/App.java")])]),
        )
        .unwrap()
        .read_set()
        .unwrap();
        assert_ne!(
            snapshot_digest(&base).unwrap(),
            snapshot_digest(&changed).unwrap()
        );
    }

    /// The same subject twice would give one set two encodings.
    #[test]
    fn a_read_set_refuses_a_duplicate_subject() {
        let duplicated = ReadSet::new(vec![
            InputPrecondition::Absent {
                path: path("a.txt"),
            },
            InputPrecondition::Absent {
                path: path("a.txt"),
            },
        ]);
        assert!(duplicated.unwrap_err().contains("duplicate key"));
    }

    /// A precondition that already reaches the wire round-trips.
    #[test]
    fn a_file_precondition_round_trips() {
        let input = InputPrecondition::File {
            path: path("pom.xml"),
            sha256: ObjectId::from_bytes(codec::sha256(b"x")),
            len: 1,
            mode: mode(),
        };
        let mut encoder = Encoder::new();
        encoder.tag(input.tag());
        match &input {
            InputPrecondition::File {
                path,
                sha256,
                len,
                mode,
            } => {
                path.encode(&mut encoder).unwrap();
                sha256.encode(&mut encoder);
                encoder.u64(*len);
                encoder.u32(mode.bits());
            }
            _ => unreachable!(),
        }
        let bytes = encoder.finish().unwrap();
        let mut decoder = Decoder::new(&bytes).unwrap();
        assert_eq!(decode_precondition(&mut decoder).unwrap(), input);
        decoder.finish().unwrap();
    }

    #[test]
    fn a_legacy_component_is_one_dot_files_name() {
        assert!(LegacyFileName::parse("record-note-abc.files").is_ok());
        for bad in ["", ".files", "record.txt", "a/b.files", "../x.files"] {
            assert!(LegacyFileName::parse(bad).is_err(), "{bad}");
        }
    }
}
