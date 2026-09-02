//! The exact reviewed transition: `Plan`, its operations, and the
//! content-addressed `PlanBundle` that carries their bytes.
//!
//! **This is the document confirmation is about.** Preview, export,
//! confirmation and apply all refer to one digest and apply never replans,
//! which means every fact the executor
//! needs has to be *in here* rather than re-derivable. Hence the shape: a
//! `Plan` carries preconditions, a semantic summary and a list of typed
//! operations, and a `PlanBundle` adds the `trees` and `blobs` those
//! operations name by digest.
//!
//! Bytes live in `blobs` and are referenced by `ContentDigest` from every
//! other position — an operation names an after-image, never carries one. Two
//! operations writing the same content therefore share one entry, and the
//! plan cannot disagree with itself about what a digest means.
//!
//! The operation set is closed on purpose. `PublishMergedTree` is the managed
//! tree, `ReplaceModelFile`/`ReplaceStateFile` are jails' own, `AppendMigration`
//! is forward-only history, and `PatchReaderFile`/`RemoveReaderFile` are the
//! two ways anything reader-owned may move — each with a captured before-image.
//! A new way to touch a project is a new variant here, which is a compile
//! error in the executor until it is handled, rather than a new caller
//! somewhere writing a file.

use crate::{
    ContentDigest, EffectIntent, FileKind, FileMode, ProjectPath, SemanticPlan,
    SnapshotPreconditions,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// What the reader asked for, as the plan records it.
///
/// The source edit is already in the plan: `ReplaceModelFile` carries the
/// model file's before- and after-image. What the source cannot say is the
/// [`jails_model::Evolution`] -- the one-shot policies about how the accepted
/// schema reaches the next one -- so that is what the input records, and two
/// mutations that edit the source identically but mean different things have
/// different digests.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PlanInput {
    pub schema: String,
    pub bytes: Vec<u8>,
}

impl PlanInput {
    const SCHEMA: &str = "jails.plan-input.v1";

    /// No request: the model is recompiled as it stands (`sync`, `repair`).
    pub fn reconcile() -> Self {
        Self::of(br#"{"kind":"reconcile"}"#.to_vec())
    }

    /// The model's first compile, on a project that had none.
    pub fn init_model() -> Self {
        Self::of(br#"{"kind":"init-model"}"#.to_vec())
    }

    /// A mutation: the edited source is in the plan, and this is what it
    /// could not say.
    pub fn evolution(evolution: &jails_model::Evolution) -> Result<Self, String> {
        #[derive(Serialize)]
        struct Input<'a> {
            kind: &'static str,
            evolution: &'a jails_model::Evolution,
        }
        serde_json::to_vec(&Input {
            kind: "mutation",
            evolution,
        })
        .map(Self::of)
        .map_err(|error| format!("could not encode the plan input: {error}"))
    }

    fn of(bytes: Vec<u8>) -> Self {
        Self {
            schema: Self::SCHEMA.to_string(),
            bytes,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelFileUpdate {
    pub path: ProjectPath,
    pub bytes: Vec<u8>,
    /// Authoring sources this update replaces, retired in the same plan.
    ///
    /// **A model source that stops being the model is reader-owned source.**
    /// The one caller is `model upgrade`, which writes `.jails/model.jdl` and
    /// retires `.jails/model.toml` in the *same* exact plan, because two
    /// editable model sources is the state the contracts forbid. It is a
    /// `RemoveReaderFile` rather than a new operation because that is exactly
    /// what a retired source is, and a seventh operation for one caller would
    /// widen the vocabulary every executor and verifier has to cover.
    pub retire: Vec<ProjectPath>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FileImageRef {
    pub blob: ContentDigest,
    pub len: u64,
    pub mode: FileMode,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TreeEntry {
    pub kind: FileKind,
    pub mode: FileMode,
    pub blob: ContentDigest,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct TreeManifest {
    pub entries: BTreeMap<ProjectPath, TreeEntry>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum PlannedOperation {
    ReplaceModelFile {
        path: ProjectPath,
        before: Option<FileImageRef>,
        after: FileImageRef,
    },
    /// Publish the already-reconciled BASE/OURS/THEIRS result. The tree is
    /// not the raw compiler projection: reader edits have been merged before
    /// this operation can exist.
    PublishMergedTree {
        root: ProjectPath,
        before: Option<ContentDigest>,
        after: ContentDigest,
    },
    AppendMigration {
        path: ProjectPath,
        after: FileImageRef,
    },
    ReplaceStateFile {
        path: ProjectPath,
        before: Option<FileImageRef>,
        after: FileImageRef,
    },
    PatchReaderFile {
        path: ProjectPath,
        before: Option<FileImageRef>,
        after: FileImageRef,
    },
    RemoveReaderFile {
        path: ProjectPath,
        before: FileImageRef,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Plan {
    pub schema: String,
    pub id: String,
    pub compiler: String,
    pub base: SnapshotPreconditions,
    pub input: PlanInput,
    pub summary: SemanticPlan,
    pub operations: Vec<PlannedOperation>,
    pub follow_up_effects: Vec<EffectIntent>,
    pub digest: ContentDigest,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PlanBundle {
    pub schema: String,
    pub plan: Plan,
    pub trees: BTreeMap<ContentDigest, TreeManifest>,
    pub blobs: BTreeMap<ContentDigest, Vec<u8>>,
}
