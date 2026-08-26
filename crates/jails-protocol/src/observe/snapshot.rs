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
use crate::identity::{ObjectId, ObjectRef, ProjectPath, TemplateId, TemplateKey, TransactionId};
use crate::provenance::TemplateOrigin;
use jails_support::codec::{self, Codec, Decoder, Encoder, ordered};
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

impl Codec for ExternalInputId {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        match self {
            Self::AppManifest => {
                encoder.tag(0);
                Ok(())
            }
            Self::UserTemplate(id) => {
                encoder.tag(1);
                id.encode(encoder)
            }
            Self::CasesBrief { path_id } => {
                encoder.tag(2);
                path_id.encode(encoder)?;
                Ok(())
            }
        }
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(match decoder.tag()? {
            0 => Self::AppManifest,
            1 => Self::UserTemplate(TemplateId::decode(decoder)?),
            2 => Self::CasesBrief {
                path_id: ExternalPathId::decode(decoder)?,
            },
            other => Err(format!("unknown external input tag {other}"))?,
        })
    }
}

impl Codec for MachineRootPresence {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.tag(match self {
            Self::Absent => 0,
            Self::Present => 1,
        });
        Ok(())
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(match decoder.tag()? {
            0 => Self::Absent,
            1 => Self::Present,
            other => Err(format!("unknown machine root presence tag {other}"))?,
        })
    }
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
    /// The fixed wire tag. The gaps are variants that have been and gone:
    /// 5-7 were pre-schema-2 machine state, back when a first commit migrated
    /// a store this binary no longer reads. A tag is never reused, so a number
    /// that meant one thing cannot come to mean another.
    pub fn tag(&self) -> u8 {
        match self {
            Self::Absent { .. } => 0,
            Self::File { .. } => 1,
            Self::Directory { .. } => 2,
            Self::ExternalAbsent { .. } => 3,
            Self::ExternalFile { .. } => 4,
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
            Self::MachineReceipt { transaction, .. } => transaction.to_hex(),
            Self::MachineRoot { .. } => String::new(),
        };
        (self.tag(), detail)
    }
}

impl Codec for InputPrecondition {
    /// Tag and body. Every variant, because this is the encoder the snapshot
    /// digest, the prepared identity and the journal all use.
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.tag(self.tag());
        match self {
            Self::Absent { path } => path.encode(encoder)?,
            Self::File {
                path,
                sha256,
                len,
                mode,
            } => {
                path.encode(encoder)?;
                sha256.encode(encoder)?;
                encoder.u64(*len);
                encoder.u32(mode.bits());
            }
            Self::Directory {
                path,
                entries,
                entries_sha256,
            } => {
                path.encode(encoder)?;
                // A *sorted list*, not a set: the entries are a `Vec` whose
                // order is the wire order, so `Encoder::set`'s bound does not
                // apply. Same reason `conflict::encode_paths` survives.
                encoder.count(entries.len())?;
                let mut previous: Option<&ProjectPath> = None;
                for entry in entries {
                    ordered(previous, entry)?;
                    previous = Some(entry);
                    entry.encode(encoder)?;
                }
                entries_sha256.encode(encoder)?;
            }
            Self::ExternalAbsent { id } => id.encode(encoder)?,
            Self::ExternalFile { id, sha256, len } => {
                id.encode(encoder)?;
                sha256.encode(encoder)?;
                encoder.u64(*len);
            }
            Self::MachineReceipt {
                transaction,
                generation,
                record_checksum,
            } => {
                transaction.encode(encoder)?;
                encoder.u64(*generation);
                record_checksum.encode(encoder)?;
            }
            Self::MachineRoot { presence } => presence.encode(encoder)?,
        }
        Ok(())
    }

    /// Decode one precondition. Every variant, because a prepared identity puts
    /// the whole read set on the wire and a recovered journal has to reconstruct
    /// exactly what the plan rested on.
    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(match decoder.tag()? {
            0 => Self::Absent {
                path: ProjectPath::decode(decoder)?,
            },
            1 => Self::File {
                path: ProjectPath::decode(decoder)?,
                sha256: ObjectId::decode(decoder)?,
                len: decoder.u64()?,
                mode: FileMode::new(decoder.u32()?)?,
            },
            2 => {
                let path = ProjectPath::decode(decoder)?;
                let count = decoder.count()?;
                let mut entries = Vec::new();
                let mut previous: Option<ProjectPath> = None;
                for _ in 0..count {
                    let entry = ProjectPath::decode(decoder)?;
                    ordered(previous.as_ref(), &entry)?;
                    previous = Some(entry.clone());
                    entries.push(entry);
                }
                Self::Directory {
                    path,
                    entries,
                    entries_sha256: ObjectId::decode(decoder)?,
                }
            }
            3 => Self::ExternalAbsent {
                id: ExternalInputId::decode(decoder)?,
            },
            4 => Self::ExternalFile {
                id: ExternalInputId::decode(decoder)?,
                sha256: ObjectId::decode(decoder)?,
                len: decoder.u64()?,
            },
            9 => Self::MachineReceipt {
                transaction: TransactionId::decode(decoder)?,
                generation: decoder.u64()?,
                record_checksum: ObjectId::decode(decoder)?,
            },
            11 => Self::MachineRoot {
                presence: MachineRootPresence::decode(decoder)?,
            },
            other => return Err(format!("unknown input precondition tag {other}").into()),
        })
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
            )
            .into());
        }
        Ok(Self(path.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One template as the snapshot froze it.
///
/// §R2.1 freezes built-in/project/user origin **once**. A planner that
/// re-resolved a template would let the same run render one file from a
/// built-in and another from an override that appeared in between, and the
/// only record of which was used would be the bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedTemplate {
    pub id: TemplateId,
    pub origin: TemplateOrigin,
    pub source: Arc<str>,
    pub source_object: ObjectRef,
    pub required_keys: BTreeSet<TemplateKey>,
}

impl ResolvedTemplate {
    /// Freeze one template, deriving its object identity from its own bytes.
    pub fn capture(
        id: TemplateId,
        origin: TemplateOrigin,
        source: &str,
        required_keys: BTreeSet<TemplateKey>,
    ) -> Self {
        Self {
            id,
            origin,
            source: Arc::from(source),
            source_object: ObjectRef::new(
                ObjectId::from_bytes(codec::sha256(source.as_bytes())),
                source.len() as u64,
            ),
            required_keys,
        }
    }
}

/// Every template this run may render, resolved once.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TemplateStore {
    templates: BTreeMap<TemplateId, ResolvedTemplate>,
}

impl TemplateStore {
    pub fn new(templates: Vec<ResolvedTemplate>) -> Result<Self> {
        let mut map = BTreeMap::new();
        for template in templates {
            if map.insert(template.id.clone(), template.clone()).is_some() {
                return Err(format!(
                    "template `{}` was resolved twice; its origin is frozen once per run",
                    template.id
                )
                .into());
            }
        }
        Ok(Self { templates: map })
    }

    /// An unresolved template is an error for the same reason an undeclared
    /// read is: the run would render from bytes nothing recorded.
    pub fn resolve(&self, id: &TemplateId) -> Result<&ResolvedTemplate> {
        Ok(self.templates.get(id).ok_or_else(|| {
            format!(
                "template `{id}` was not resolved, so this run may not render it.\n       fix: \
                 declare it in the snapshot; its origin is frozen once so two files cannot \
                 render from two different versions of it."
            )
        })?)
    }

    pub fn iter(&self) -> impl Iterator<Item = &ResolvedTemplate> {
        self.templates.values()
    }
}

/// Everything planning may know about one project, captured once.
#[derive(Clone, Debug)]
pub struct ProjectSnapshot {
    root: CanonicalRoot,
    files: BTreeMap<ProjectPath, SnapshotFile>,
    absences: BTreeSet<ProjectPath>,
    directories: BTreeMap<ProjectPath, Vec<ProjectPath>>,
    directory_absences: BTreeSet<ProjectPath>,
}

/// What a declared read found.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Captured<'a> {
    Present(&'a SnapshotFile),
    /// Declared, and it was not there. A fact, not a gap.
    Absent,
}

/// The no-follow filesystem fact captured for a directory precondition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DirectoryFact<'a> {
    Missing,
    Directory(&'a [ProjectPath]),
    NonDirectory,
}

impl ProjectSnapshot {
    pub fn new(
        root: CanonicalRoot,
        files: BTreeMap<ProjectPath, SnapshotFile>,
        absences: BTreeSet<ProjectPath>,
        directories: BTreeMap<ProjectPath, Vec<ProjectPath>>,
    ) -> Result<Self> {
        Self::with_directory_absences(root, files, absences, directories, BTreeSet::new())
    }

    pub fn with_directory_absences(
        root: CanonicalRoot,
        files: BTreeMap<ProjectPath, SnapshotFile>,
        absences: BTreeSet<ProjectPath>,
        directories: BTreeMap<ProjectPath, Vec<ProjectPath>>,
        directory_absences: BTreeSet<ProjectPath>,
    ) -> Result<Self> {
        // A path cannot be both present and absent: one of the two readings
        // would silently win, and which one is an implementation detail.
        for path in &absences {
            if files.contains_key(path) {
                return Err(format!("`{path}` is captured as both present and absent").into());
            }
        }
        for path in directories.keys() {
            if files.contains_key(path)
                || absences.contains(path)
                || directory_absences.contains(path)
            {
                return Err(format!(
                    "`{path}` has contradictory captured filesystem facts.\n       \
                     fix: declare the path in exactly one observed state."
                )
                .into());
            }
        }
        for path in &directory_absences {
            if files.contains_key(path) {
                return Err(format!(
                    "`{path}` has contradictory captured filesystem facts.\n       \
                     fix: declare the path in exactly one observed state."
                )
                .into());
            }
        }
        Ok(Self {
            root,
            files,
            absences,
            directories,
            directory_absences,
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
    /// nothing to compare.
    ///
    /// **The message is written for the reader, not the author.** "Declare it
    /// in the read set" is an instruction to whoever is editing the route --
    /// there is no `--read-set` and nothing a user can do with it -- and it
    /// was reached on the *recovery* path, which is the worst place to hand
    /// somebody a sentence about jails' internals. It says what it is now: a
    /// bug in the command, with the path that exposed it and something to try
    /// meanwhile.
    pub fn read(&self, path: &ProjectPath) -> Result<Captured<'_>> {
        if let Some(file) = self.files.get(path) {
            return Ok(Captured::Present(file));
        }
        if self.absences.contains(path) {
            return Ok(Captured::Absent);
        }
        Err(format!(
            "this command planned against `{path}` without observing it first, which is a bug in \
             jails rather than in your project -- nothing was written.\n       fix: report the \
             command and that path. `jails resource status` and `jails doctor` are read-only and \
             still work meanwhile."
        )
        .into())
    }

    /// List a declared directory. Undeclared is an error for the same reason.
    pub fn list(&self, path: &ProjectPath) -> Result<&[ProjectPath]> {
        if self.directory_absences.contains(path) {
            return Ok(&[]);
        }
        Ok(self
            .directories
            .get(path)
            .map(Vec::as_slice)
            .ok_or_else(|| {
                format!(
                    "directory `{path}` was not enumerated, so planning may not list it.\n       \
                 fix: declare it in the read set."
                )
            })?)
    }

    /// Read a directory fact that was explicitly captured without following
    /// symlinks. A file at the path is returned as a collision so preparation
    /// can refuse before commit.
    pub fn directory_fact(&self, path: &ProjectPath) -> Result<DirectoryFact<'_>> {
        if let Some(entries) = self.directories.get(path) {
            return Ok(DirectoryFact::Directory(entries));
        }
        if self.directory_absences.contains(path) {
            return Ok(DirectoryFact::Missing);
        }
        if self.files.contains_key(path) {
            return Ok(DirectoryFact::NonDirectory);
        }
        Err(format!(
            "directory `{path}` was not captured, so preparation cannot decide whether creating it is observable.\n       \
             fix: declare it in the directory read set."
        )
        .into())
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
        for path in self.absences.union(&self.directory_absences) {
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
///
/// Through [`InputPrecondition::encode`], not a second hand-written pass: a
/// digest computed by one encoder and a journal written by another would let
/// two runs agree on the hash and disagree on what it covered.
pub fn snapshot_digest(read_set: &ReadSet) -> Result<ObjectId> {
    let mut encoder = Encoder::new();
    encoder.count(read_set.inputs().len())?;
    for input in read_set.inputs() {
        input.encode(&mut encoder)?;
    }
    Ok(ObjectId::from_bytes(codec::domain_hash(
        "JAILS-SNAPSHOT-1",
        &encoder.finish()?,
    )))
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
        assert!(error.contains("without observing it first"), "{error}");
        assert!(error.contains("a bug in jails"), "{error}");
        assert!(error.contains("fix: report the command"), "{error}");

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
                sha256.encode(&mut encoder).unwrap();
                encoder.u64(*len);
                encoder.u32(mode.bits());
            }
            _ => unreachable!(),
        }
        let bytes = encoder.finish().unwrap();
        let mut decoder = Decoder::new(&bytes).unwrap();
        assert_eq!(InputPrecondition::decode(&mut decoder).unwrap(), input);
        decoder.finish().unwrap();
    }
}
