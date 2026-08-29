use crate::ProjectPath;
use jails_model::AppModel;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum FileKind {
    JavaMain,
    JavaTest,
    Resource,
    HttpCollection,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum FileMode {
    Regular,
    Executable,
}

/// Declaration order is the order generated roots are rendered into a build
/// file, so it is `Ord` and the variants are written in the order a reader
/// expects to meet them.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum JavaSourceSet {
    Main,
    Test,
    MainResources,
    TestResources,
}

/// One generated root and the source set it joins.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct MavenSourceRoot {
    pub source_set: JavaSourceSet,
    pub path: ProjectPath,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum BuildFeature {
    Coverage,
    IntegrationTests,
    /// Spotless plus a pinned formatter.
    ///
    /// Keyed by what it does rather than by a plugin coordinate, for the
    /// reason the other two are: `spotless-maven-plugin` is not a name Gradle
    /// resolves, and keying by it files a Gradle project's claim under a
    /// plugin it does not have.
    Formatting,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Provenance {
    /// Stable identity of this one emitted file, used for merge history.
    pub artifact_id: String,
    /// Optional ownership-transfer boundary shared by one or more artifacts.
    /// When absent, the artifact itself is the boundary for compatibility.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ejection_id: Option<String>,
    pub ejectable: bool,
    pub semantic_ids: BTreeSet<String>,
    pub compiler_pass: String,
}

impl Provenance {
    pub fn ejection_target(&self) -> &str {
        self.ejection_id
            .as_deref()
            .unwrap_or(self.artifact_id.as_str())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RenderedFile {
    pub kind: FileKind,
    pub mode: FileMode,
    pub bytes: Vec<u8>,
    pub provenance: Provenance,
}

/// One compiler-owned facet embedded in an otherwise reader-owned document.
///
/// The accepted projection stores only these bytes, never the whole document:
/// unrelated YAML/XML/properties remain reader state, while this exact slice
/// is the BASE for the same three-way merge used by generated source files.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RenderedReaderFacet {
    pub path: ProjectPath,
    pub kind: ReaderFacetKind,
    pub bytes: Vec<u8>,
    pub provenance: Provenance,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ReaderFacetKind {
    ComposeService {
        service: String,
        marker: String,
    },
    /// A compiler-owned project file outside the generated source tree.
    /// Accepted bytes are its merge BASE; the live file remains hand-editable.
    ManagedFile {
        mode: FileMode,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RenderedTree {
    pub root: ProjectPath,
    pub files: BTreeMap<ProjectPath, RenderedFile>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub reader_facets: BTreeMap<String, RenderedReaderFacet>,
}

impl RenderedTree {
    pub fn new(root: ProjectPath) -> Self {
        Self {
            root,
            files: BTreeMap::new(),
            reader_facets: BTreeMap::new(),
        }
    }

    pub fn insert(&mut self, path: ProjectPath, file: RenderedFile) -> Result<(), String> {
        if !path.is_within(&self.root) {
            return Err(format!(
                "managed artifact `{path}` is outside managed root `{}`",
                self.root
            ));
        }
        if self.files.insert(path.clone(), file).is_some() {
            return Err(format!("two compiler units emit `{path}`"));
        }
        Ok(())
    }

    pub fn insert_reader_facet(
        &mut self,
        id: String,
        facet: RenderedReaderFacet,
    ) -> Result<(), String> {
        if self.reader_facets.insert(id.clone(), facet).is_some() {
            return Err(format!("duplicate rendered reader facet `{id}`"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RenderedMigration {
    pub logical_name: String,
    pub bytes: Vec<u8>,
    pub semantic_ids: BTreeSet<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct BuildDependency {
    pub group: String,
    pub artifact: String,
    pub version: Option<String>,
    pub scope: jails_model::DependencyScope,
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct PropertyEntry {
    pub key: String,
    pub value: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum DocumentIntent {
    /// Every generated root Maven must compile, as one intent.
    ///
    /// One intent rather than one per source set, because they land in one
    /// `<plugin>`. A block per source set meant a full
    /// `org.codehaus.mojo:build-helper-maven-plugin` declaration per set, and
    /// a project with a main and a test root declared that plugin twice in one
    /// `<plugins>`: Maven merges the executions, so both roots do compile, but
    /// it warns `must be unique but found duplicate declaration` on every
    /// build. A warning that alarming on every build is how readers learn to
    /// stop reading warnings.
    EnsureMavenSourceRoots {
        roots: Vec<MavenSourceRoot>,
    },
    EnsureGradleSourceRoot {
        path: ProjectPath,
        source_set: JavaSourceSet,
    },
    ReconcileDependencies {
        dependencies: Vec<BuildDependency>,
    },
    ReconcileBuildFeatures {
        features: BTreeSet<BuildFeature>,
    },
    ReconcileProperties {
        path: ProjectPath,
        previous: Vec<PropertyEntry>,
        desired: Vec<PropertyEntry>,
    },
    EjectFile {
        source: ProjectPath,
        path: ProjectPath,
        bytes: Vec<u8>,
        semantic_ids: BTreeSet<String>,
    },
    /// Transfer an existing reader-owned Java unit into the canonical managed
    /// tree while retaining its exact live bytes as the reader's merge delta.
    AdoptJava {
        source: ProjectPath,
        path: ProjectPath,
        /// Exact bytes last rendered by the legacy generator. These are the
        /// one-time BASE for moving live reader edits onto canonical output.
        base: Vec<u8>,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EffectIntent {
    pub id: String,
    pub kind: String,
    pub arguments: BTreeMap<String, String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DiagnosticSeverity {
    Warning,
    Error,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CompilerDiagnostic {
    pub severity: DiagnosticSeverity,
    pub code: String,
    pub semantic_id: Option<String>,
    pub message: String,
    pub fix: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SemanticPlan {
    pub model_nodes: usize,
    pub managed_files: usize,
    pub migrations: usize,
    pub reader_document_intents: usize,
    pub effects: usize,
}

/// Pure compiler output. Paths and bytes are desired state, not operations.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PlanDraft {
    pub next_model: AppModel,
    /// Exact prior compiler projection used as the three-way merge base.
    pub baseline: RenderedTree,
    pub generated: RenderedTree,
    pub migrations: Vec<RenderedMigration>,
    pub reader_document_intents: Vec<DocumentIntent>,
    pub follow_up_effects: Vec<EffectIntent>,
    pub summary: SemanticPlan,
    pub diagnostics: Vec<CompilerDiagnostic>,
}
