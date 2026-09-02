//! `WorkspaceSnapshot` — every external fact, captured once.
//!
//! **The second contract, and the one the compiler's purity rests on.** The
//! compiler may not read the filesystem, the environment or a subprocess; it
//! reads this. So anything the compiler needs to know about the world outside
//! the model has to appear here as a value — the build system, the Java
//! release, the Boot version, the declared dependencies, the reader's layer
//! renames, the captured file bytes, the migration history, the external type
//! index. A fact that is missing is not a fact the compiler asks for later; it
//! is a fact the compiler cannot use.
//!
//! That is why the type is wide and boring. Each field is a question a pass
//! cannot answer for itself, answered at the boundary instead; `maven_wrapper`
//! is the clearest example.
//!
//! `SnapshotPreconditions` is the other half — not what the compiler read, but
//! what the executor must find unchanged. `ContentDigest` is how both halves
//! name content, and it too has one constructor that refuses rather than
//! repairs, for `path.rs`'s reason: these are keys.

use crate::{Layout, ProjectPath, RenderedTree};
use jails_model::{AppModel, BuildSystem};
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

/// Facts observed once at the capture boundary.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProjectFacts {
    pub build_system: BuildSystem,
    pub java_release: u16,
    pub spring_boot: Option<String>,
    pub base_package: String,
    pub dependencies: BTreeSet<String>,
    /// Whether the project ships a Maven wrapper.
    ///
    /// Observed the way `build_system` is -- by asking the filesystem once at
    /// the capture boundary -- because the generated CI workflow and Dockerfile
    /// have to invoke the build the way the project actually offers it.
    /// `./mvnw` on a project without one fails at the first step; `mvn` on a
    /// project with one silently uses whatever Maven the runner happens to
    /// have, which is the version drift the wrapper exists to prevent.
    #[serde(default)]
    pub maven_wrapper: bool,
    /// The reader's layer renames, from `jails.toml`.
    ///
    /// `#[serde(default)]` so a snapshot without the field decodes and a
    /// project that renamed nothing serializes without it -- the defaults are
    /// the names the compiler uses anyway.
    #[serde(default)]
    pub layout: Layout,
    /// The JUnit version this project declares, if it declares one.
    ///
    /// **`test --fast` needs it and cannot guess it.** It is the version
    /// `junit-platform-console` must be declared at, which is *not* always the
    /// version the project declares: JUnit 5 paired jupiter `5.y.z` with
    /// platform `1.y.z`, and from JUnit 6 `junit-bom` gives them one number.
    /// Getting it wrong either fails to resolve or dies at run time with a
    /// `NoSuchMethodError` wrapped in "versions not properly aligned". Under a
    /// Spring Boot parent or an imported `junit-bom` something else manages
    /// it and this stays `None`; on a plain build it is the only thing that
    /// makes the capability declarable at all.
    #[serde(default)]
    pub junit: Option<String>,
    /// The identity this project's build declares for itself.
    ///
    /// **A consumer group is not a directory name.** It is a durable identity
    /// in the broker, so naming it after whatever the checkout happens to be
    /// called gives two clones of one service two different groups -- and both
    /// then receive every message instead of splitting the work. The model's
    /// application name is derived from the directory when a model is seeded
    /// beside an existing build, which is exactly the case this corrects.
    ///
    /// Maven's `<artifactId>` and Gradle's `rootProject.name`; `None` when the
    /// build states neither, and the caller falls back to the model.
    #[serde(default)]
    pub artifact_id: Option<String>,
    /// Every coordinate the build file declares, jails' own block included.
    ///
    /// `dependencies` is the *reader's* set: capture strips the marked block
    /// jails writes before reading, because a compiler that saw its own
    /// block would decline to declare what it had declared and empty it.
    /// That is the right question for reconciliation and the wrong one for
    /// `doctor`, which asks what is on the classpath -- and a starter `add
    /// db` put in jails' block is on the classpath. One reader, two sets.
    #[serde(default)]
    pub build_dependencies: BTreeSet<String>,
    /// The Maven reactor this project belongs to, and what it inherits.
    #[serde(default)]
    pub reactor: Reactor,
    /// The class the packaged artifact starts, when the build names one:
    /// `<mainClass>` on Maven, `bootJar { mainClass }` on Gradle.
    ///
    /// `None` when it names none. A Boot project leaves the plugin to find
    /// the entry point, and `g cli` retargets only a stub jails wrote; `jails
    /// run` starts this class first because a project with two dispatchers
    /// has two `main` methods and a source walk picks whichever it reaches.
    #[serde(default)]
    pub main_class: Option<String>,
}

/// The Maven reactor a project belongs to, as the build files state it.
///
/// A module three directories below its aggregator inherits the reactor's
/// Java release and Boot management without stating either, so the facts a
/// reader is told about a module are the facts of the walk up to the reactor,
/// not of one file. A standalone project is its own reactor. On Gradle the
/// reactor is the project itself and `modules` is empty: Gradle's
/// multi-project model is `settings.gradle`'s `include` lines, a different
/// shape that is not read, and an empty list means *not read* rather than
/// *none*.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Reactor {
    /// The reactor root relative to the project root: empty for a standalone
    /// project, `..` for a module one level below its aggregator. Relative,
    /// so a snapshot carries no machine path.
    pub root: String,
    /// The reactor's own `<artifactId>`.
    pub artifact_id: Option<String>,
    /// The nearest Java release stated between the project and its reactor,
    /// and `None` when no build file on that path states one.
    ///
    /// **The build's answer, not the model's.** [`ProjectFacts::java_release`]
    /// is what the compiler emits for and comes from the model; this is what
    /// `doctor` compares the JDK against and what a seed model adopts.
    pub java_release: Option<u16>,
    /// Whether Spring Boot's dependency management reaches this project: a
    /// Boot parent or an imported Boot BOM in any build file between it and
    /// its reactor, or the Boot plugin on Gradle.
    pub spring_boot: bool,
    /// Every module the reactor aggregates, transitively, in declaration
    /// order and without the reactor itself.
    pub modules: Vec<ReactorModule>,
}

/// One module a reactor aggregates.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReactorModule {
    /// Relative to the reactor root.
    pub path: String,
    pub artifact_id: Option<String>,
}

impl ProjectFacts {
    pub fn detached(model: &AppModel) -> Self {
        Self {
            build_system: BuildSystem::Unknown,
            java_release: model.project.java_release,
            spring_boot: None,
            base_package: model.project.base_package.clone(),
            dependencies: BTreeSet::new(),
            maven_wrapper: false,
            layout: Layout::default(),
            junit: None,
            artifact_id: None,
            build_dependencies: BTreeSet::new(),
            reactor: Reactor::default(),
            main_class: None,
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
    /// Digest of every migration the executor has published, as accepted.
    ///
    /// **Schema history is append-only, and this is what makes that
    /// checkable.** A migration already applied to a database cannot be
    /// rewritten -- Flyway refuses on the checksum, and a database that ran
    /// the old text is not described by the new one -- so an edit is a fault
    /// rather than a preserved reader edit. Nothing else here can see it:
    /// `migration_history` is read fresh from the tree on every capture, so it
    /// agrees with whatever the file says now.
    pub accepted_migrations: BTreeMap<ProjectPath, ContentDigest>,
    /// The exact bytes of every migration the executor has published.
    ///
    /// **A digest can only say a migration changed; the bytes can put it
    /// back.** An edited migration that a database has already run is the one
    /// file a reader cannot fix by regenerating: the compiler derives a
    /// migration from a model *diff*, and the diff that produced this one is
    /// history. Flyway refuses on the checksum until the file matches what
    /// ran, so `resource repair` needs the original and nothing else has it.
    ///
    /// `#[serde(default)]` so a lock without the field decodes; repair then
    /// reports that migration as unrecoverable rather than claiming to have
    /// restored it.
    #[serde(default)]
    pub accepted_migration_bytes: BTreeMap<ProjectPath, Vec<u8>>,
    pub project: ProjectFacts,
    pub external_types: ExternalTypeIndex,
    pub migration_history: MigrationHistory,
    pub owned_patches: OwnedPatchState,
    pub preconditions: SnapshotPreconditions,
    pub files: BTreeMap<ProjectPath, CapturedFile>,
    /// Reader-authored replacements for jails' own Java templates.
    ///
    /// Observed at the capture boundary because the compiler may not read the
    /// filesystem, and `#[serde(default)]` because a snapshot without the
    /// field is a snapshot with none.
    #[serde(default)]
    pub template_overrides: crate::TemplateOverrides,
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
            accepted_migrations: BTreeMap::new(),
            accepted_migration_bytes: BTreeMap::new(),
            project,
            external_types: ExternalTypeIndex::default(),
            migration_history: MigrationHistory::default(),
            owned_patches: OwnedPatchState::default(),
            template_overrides: crate::TemplateOverrides::default(),
            preconditions: SnapshotPreconditions::default(),
            files: BTreeMap::new(),
        }
    }
}
