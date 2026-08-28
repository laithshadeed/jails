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
