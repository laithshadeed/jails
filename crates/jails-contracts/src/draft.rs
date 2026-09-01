//! `PlanDraft` — what the compiler decided, before anything knows about disk.
//!
//! The compiler's output is *semantic desire*: a `RenderedTree` of managed
//! files, the migrations to append, the reader-file intents, the build
//! dependencies and features, the properties, and a `SemanticPlan` summary. It
//! is deliberately not a plan. There are no digests here, no before-images and
//! no preconditions, because those are facts about a workspace and the
//! compiler has none — `jails-workspace::materialize` turns this into the
//! exact `Plan` by pairing it with a snapshot.
//!
//! Two shapes here carry more weight than their size suggests.
//!
//! `Provenance` rides on every rendered file and is what makes regeneration a
//! merge rather than an overwrite: `artifact_id` is the identity a BASE/THEIRS
//! pair is matched by, so a file that *moves* is still the same artifact and
//! its hand edits follow it. `ejection_id` is the separate question of what
//! transfers together, and `ejectable` says whether transfer is offered at all
//! — managed ABI is not.
//!
//! `BuildFeature` is keyed by what a plugin *does*, never by its coordinate,
//! because `jacoco-maven-plugin` is not a name Gradle resolves and keying by
//! it filed a Gradle project's claim under a plugin it does not have. Adding a
//! variant is a compile error in the Gradle backend until that side exists,
//! which is what replaced a run-time refusal for an unrecognised plugin.

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
    /// The artifact is on this project's classpath and is not passed on to
    /// anything that depends on it.
    ///
    /// Maven spells that `<optional>true</optional>`, and Boot's own starters
    /// mark `spring-boot-docker-compose` and devtools that way -- Spring
    /// Initializr copies them, so a generated pom that omits it differs from
    /// the one the same choices produce on start.spring.io. **Gradle needs
    /// nothing**: `implementation` and `runtimeOnly` are already non-transitive
    /// for a consumer's compile classpath, which is exactly what the Maven flag
    /// buys, so its renderer reads this field and has nothing to add. It is
    /// carried on the dependency rather than decided inside the Maven adapter
    /// because which dependencies are optional is a fact about the capability.
    pub optional: bool,
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
    /// Put `@Import(<class>.class)` on every `@SpringBootTest` the project
    /// already has.
    ///
    /// **The target set is not named here, deliberately.** The compiler cannot
    /// enumerate `src/test/java`, and it must not: an observation belongs at
    /// the capture boundary. So the snapshot carries those files and the
    /// materializer picks the ones that are `@SpringBootTest` classes, which
    /// makes the plan exact -- every file it touches has a captured
    /// before-image, and a test edited after review makes the plan stale
    /// rather than being overwritten.
    ///
    /// It exists because the moment `spring-boot-starter-jdbc` lands in the
    /// build, JDBC auto-configuration demands a `DataSource` for *every*
    /// `@SpringBootTest` -- including the `contextLoads` test that shipped
    /// with the project and never touches a database.
    ReconcileSpringTestImport {
        /// The `@TestConfiguration` class to import.
        class: String,
        /// Its package, so a test in another one gets the import statement.
        package: String,
        /// Whether the model still wants it.
        ///
        /// **`false` is why this reconciles rather than ensures.** The splice
        /// is an edit to a file the reader owns, so `remove db` has to take it
        /// back out -- a `@SpringBootTest` left importing a
        /// `TestcontainersConfig` that no longer exists does not compile. The
        /// annotation names the class, so the inverse is exact and needs no
        /// marker; what it needs is for the compiler to keep emitting the
        /// intent after the capability is gone, which an *ensure* would not.
        wanted: bool,
    },
    /// Register a generated command in the project's CLI dispatcher.
    ///
    /// The dispatcher is found by *shape* rather than by filename — the
    /// registry type plus the `return commands;` anchor — so both `App.java`
    /// from `new-cli` and a `<Name>Cli.java` from `g cli` are found, and a
    /// class that merely happens to be called `App` is not. Like
    /// [`Self::EnsureSpringTestImport`] it names no path: which file is the
    /// dispatcher is an observation, so the snapshot carries the candidates.
    ///
    /// Nothing is written when there is no dispatcher, or when there is more
    /// than one: the generated command's Javadoc already carries the line to
    /// paste, and splicing into a file jails cannot uniquely identify is worse
    /// than saying nothing.
    EnsureCommandRegistration {
        /// The generated command's simple class name.
        class: String,
        /// Its package, so a dispatcher elsewhere gets the import statement.
        package: String,
    },
    /// Point the packaged jar at a `cli` component this model declares.
    ///
    /// The compiler decides whether it *may* — see `emit_component::cli` — so
    /// this intent existing at all means the claim is jails' to make.
    SetMavenMainClass {
        class: String,
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
