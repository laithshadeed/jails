use crate::{Layout, ProjectPath, RenderedTree};
use jails_model::AppModel;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// A content identity captured before planning.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct ContentDigest(String);

impl ContentDigest {
    pub fn parse(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        let Some(hex) = value.strip_prefix("sha256:") else {
            return Err("content digest must start with `sha256:`".to_string());
        };
        if hex.len() != 64 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err("content digest must contain 64 hexadecimal digits".to_string());
        }
        Ok(Self(format!("sha256:{}", hex.to_ascii_lowercase())))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct VersionedModel {
    pub model: AppModel,
    pub source_digest: Option<ContentDigest>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum BuildSystem {
    Maven,
    Gradle,
    Unknown,
}

/// Facts observed once at the capture boundary.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProjectFacts {
    pub build_system: BuildSystem,
    pub java_release: u16,
    pub spring_boot: Option<String>,
    pub base_package: String,
    pub dependencies: BTreeSet<String>,
    /// The reader's layer renames, from `jails.toml`.
    ///
    /// `#[serde(default)]` so a snapshot written before this field existed
    /// still decodes, and so a project that renamed nothing serializes exactly
    /// as it did -- the defaults are the names the compiler already used.
    #[serde(default)]
    pub layout: Layout,
}

impl ProjectFacts {
    pub fn detached(model: &AppModel) -> Self {
        Self {
            build_system: BuildSystem::Unknown,
            java_release: model.project.java_release,
            spring_boot: None,
            base_package: model.project.base_package.clone(),
            dependencies: BTreeSet::new(),
            layout: Layout::default(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ExternalType {
    pub qualified_name: String,
    pub capabilities: BTreeSet<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ExternalTypeIndex {
    pub types: BTreeMap<String, ExternalType>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MigrationRecord {
    pub version: String,
    pub path: ProjectPath,
    pub digest: ContentDigest,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct MigrationHistory {
    pub records: Vec<MigrationRecord>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct OwnedPatchState {
    pub documents: BTreeMap<ProjectPath, ContentDigest>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum FilePrecondition {
    Missing,
    Present {
        digest: ContentDigest,
        executable: bool,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum DirectoryPrecondition {
    Missing,
    Present { digest: ContentDigest },
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct SnapshotPreconditions {
    pub files: BTreeMap<ProjectPath, FilePrecondition>,
    pub directories: BTreeMap<ProjectPath, DirectoryPrecondition>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CapturedFile {
    pub bytes: Vec<u8>,
    pub executable: bool,
}

/// The only workspace observation a pure compiler may consume.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct WorkspaceSnapshot {
    pub model: VersionedModel,
    /// Last model whose compiler projection was successfully materialized.
    /// It reproduces merge bases; it is not desired-state authority.
    pub accepted_model: Option<AppModel>,
    /// Exact compiler projection accepted with `accepted_model`.
    ///
    /// Generic three-way merge cannot reconstruct this from the model after
    /// an emitter upgrade. Keeping exactly one projection is the irreducible
    /// merge state; it is neither desired-state authority nor history.
    pub accepted_projection: Option<RenderedTree>,
    pub accepted_compiler: Option<String>,
    pub project: ProjectFacts,
    pub external_types: ExternalTypeIndex,
    pub migration_history: MigrationHistory,
    pub owned_patches: OwnedPatchState,
    pub preconditions: SnapshotPreconditions,
    pub files: BTreeMap<ProjectPath, CapturedFile>,
}

impl WorkspaceSnapshot {
    pub fn detached(model: AppModel) -> Self {
        let project = ProjectFacts::detached(&model);
        Self {
            model: VersionedModel {
                model,
                source_digest: None,
            },
            accepted_model: None,
            accepted_projection: None,
            accepted_compiler: None,
            project,
            external_types: ExternalTypeIndex::default(),
            migration_history: MigrationHistory::default(),
            owned_patches: OwnedPatchState::default(),
            preconditions: SnapshotPreconditions::default(),
            files: BTreeMap::new(),
        }
    }
}
