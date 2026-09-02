//! The exact reviewed transition: `Plan`, its operations, and the
//! content-addressed `PlanBundle` that carries their bytes.
//!
//! **This is the document confirmation is about.** `simplify-sol.md`'s fifth
//! contract is that preview, export, confirmation and apply all refer to one
//! digest and that apply never replans, which means every fact the executor
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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CanonicalModelPatch {
    pub schema: String,
    pub bytes: Vec<u8>,
}

impl CanonicalModelPatch {
    pub fn reconcile() -> Self {
        Self {
            schema: "jails.model-patch.v1".to_string(),
            bytes: br#"{"kind":"reconcile"}"#.to_vec(),
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
    /// The one caller is the upgrade that moves a project off
    /// `.jails/model.toml`: writing `.jails/model.jdl` without retiring the
    /// TOML in the *same* exact plan leaves two editable model sources, which
    /// is the state `docs/00-contracts.md` forbids and the whole reason the
    /// upgrade exists. It is a `RemoveReaderFile` rather than a new operation
    /// because that is exactly what it is once it is no longer the model, and
    /// inventing a seventh operation for one caller would widen the vocabulary
    /// every executor and verifier has to cover.
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
    pub input: CanonicalModelPatch,
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
