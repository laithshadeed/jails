//! The one place jails looks at a project, and the only place it may.
//!
//! `WorkspaceSnapshot` captures every external fact once, and this module is
//! where it is filled in: the build system, the release, the Boot version,
//! the declared dependencies, the layer renames, the bytes of every file the
//! plan could touch, the migration history, and the *accepted* state from
//! `.jails/compiler.lock.json`. Everything above this line is a pure function
//! of what comes out of here, which is what makes the compiler's determinism
//! checkable at all.
//!
//! **Capture must be over-inclusive, and the failure when it is not is
//! silent.** An exact plan may only touch a path it captured a before-image
//! for, so a path this module did not read is a path the executor will refuse
//! to write — not with a bug report, but with a plan that quietly omits it.
//! That is why the reader roots are walked whole and why callers hand in extra
//! `reader_paths` for files a particular mutation might reach.
//!
//! **The lock is read, never repaired.** Three schema versions decode -- v1
//! has a model, v2 adds the compiler version and the accepted projection, v3
//! seals the published migrations -- and an envelope this binary cannot read
//! is an error rather than an absence: treating unreadable state as empty
//! would offer to regenerate a project's whole contents. The accepted
//! projection is BASE for every later three-way merge, so losing it does not
//! lose a file, it loses every hand edit in one.

use jails_contracts::{
    CapturedFile, ContentDigest, DirectoryPrecondition, FilePrecondition, Layout, MigrationHistory,
    MigrationRecord, ProjectPath, RenderedTree, SnapshotPreconditions, WorkspaceSnapshot,
};
use jails_model::{AppModel, Diagnostic};
use jails_support::{hex, sha256};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

mod observe;

use observe::template_overrides;
pub use observe::{facts, observe_build_system, observe_spring_boot};

pub const MIGRATION_ROOT: &str = "src/main/resources/db/migration";
const READER_MAIN_ROOT: &str = "src/main/java";
const READER_TEST_ROOT: &str = "src/test/java";
pub const COMPILER_LOCK: &str = ".jails/compiler.lock.json";

/// A canonical project path this phase built, or the diagnostic that it is
/// not one.
///
/// **The three constructors below are the capture/apply phase's shared ones**,
/// and they live here rather than beside the executor because `jails-project`
/// is the lower half: `jails-workspace` re-exports this module as
/// `crate::capture`, so one spelling of `workspace-path-invalid`,
/// `workspace-digest-invalid` and `workspace-io` serves both halves. Two
/// spellings would be two codes for one refusal, which is the thing the code
/// namespace exists to prevent.
pub fn project_path(value: impl Into<String>) -> Result<ProjectPath, Diagnostic> {
    let value = value.into();
    ProjectPath::parse(value.clone())
        .map_err(|message| Diagnostic::without_a_fix("workspace-path-invalid", value, message))
}

/// The content address of some bytes, in the one spelling the plan uses.
pub fn digest(bytes: &[u8]) -> Result<ContentDigest, Diagnostic> {
    ContentDigest::parse(format!("sha256:{}", hex(&sha256(bytes)))).map_err(|message| {
        Diagnostic::without_a_fix("workspace-digest-invalid", "$.blobs", message)
    })
}

/// Any filesystem call the capture or the executor could not complete.
///
/// One code for the family, not one per verb: `could not read`, `could not
/// inspect`, `could not stage beside` are one refusal -- the operating system
/// said no -- and the verb stays in the sentence, so every one of those
/// messages keeps its bytes. No `fix:`, because the next step is whatever the
/// errno says and inventing one would be advice jails cannot stand behind.
pub fn refused_io(verb: &str, at: &Path, error: impl std::fmt::Display) -> Diagnostic {
    Diagnostic::without_a_fix(
        "workspace-io",
        at.display().to_string(),
        format!("could not {verb} {}: {error}", at.display()),
    )
}

/// The compiler lock could not be decoded, verified or believed. One code
/// per question, each reached from every schema version that asks it.
/// `jails.toml` names a layer that does not exist. One site for the two
/// readers of it: the capture, and the `ProjectFacts` every command resolves.
pub(crate) fn layout_invalid(message: String) -> Diagnostic {
    Diagnostic::without_a_fix("workspace-layout-invalid", "jails.toml", message)
}

fn lock_undecodable(error: impl std::fmt::Display) -> Diagnostic {
    Diagnostic::without_a_fix(
        "workspace-lock-undecodable",
        COMPILER_LOCK,
        format!("could not decode `{COMPILER_LOCK}`: {error}"),
    )
}

fn lock_unverifiable(error: impl std::fmt::Display) -> Diagnostic {
    Diagnostic::without_a_fix(
        "workspace-lock-unverifiable",
        COMPILER_LOCK,
        format!("could not verify `{COMPILER_LOCK}`: {error}"),
    )
}

fn lock_projection_mismatch() -> Diagnostic {
    Diagnostic::new(
        "workspace-lock-projection-mismatch",
        COMPILER_LOCK,
        format!("compiler lock `{COMPILER_LOCK}` does not match its accepted projection"),
        "restore a known-good lock; do not infer merge bases from generated source",
    )
}
const COMPILER_LOCK_SCHEMA_V1: &str = "jails.compiler-lock.v1";
const COMPILER_LOCK_SCHEMA_V2: &str = "jails.compiler-lock.v2";
const COMPILER_LOCK_SCHEMA_V3: &str = "jails.compiler-lock.v3";
/// v3's fields with the projection's bytes stored as text.
const COMPILER_LOCK_SCHEMA_V4: &str = "jails.compiler-lock.v4";

#[derive(Deserialize)]
struct CompilerLockV1 {
    model_digest: ContentDigest,
    model: AppModel,
}

#[derive(Deserialize)]
struct CompilerLockV2 {
    compiler: String,
    model_digest: ContentDigest,
    model: AppModel,
    projection_digest: ContentDigest,
    projection: RenderedTree,
}

/// v2 plus the seal on published schema history.
///
/// The migration map is what makes append-only checkable: `migration_history`
/// is read fresh from the tree, so it agrees with whatever the file says now,
/// and only a recorded digest can say the file changed after it was published.
#[derive(Deserialize)]
struct CompilerLockV3 {
    compiler: String,
    model_digest: ContentDigest,
    model: AppModel,
    projection_digest: ContentDigest,
    projection: RenderedTree,
    #[serde(default)]
    migrations: BTreeMap<ProjectPath, ContentDigest>,
    /// The published bytes, so an edited migration can be put back.
    ///
    /// `#[serde(default)]` rather than a fourth lock schema: a lock written
    /// before this existed still decodes and still verifies, and the only
    /// thing it cannot do is restore a migration -- which is exactly what it
    /// could not do before either.
    #[serde(default)]
    migration_bytes: BTreeMap<ProjectPath, Vec<u8>>,
}

#[derive(Debug)]
struct AcceptedCompilerState {
    model: AppModel,
    projection: Option<RenderedTree>,
    compiler: Option<String>,
    migrations: BTreeMap<ProjectPath, ContentDigest>,
    migration_bytes: BTreeMap<ProjectPath, Vec<u8>>,
}

/// Whether the model file is expected on disk when the plan runs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModelFile {
    /// Read the file as it is: present, with its bytes as the precondition,
    /// or absent, for a project that has no model yet and compiles the seed
    /// `model init` would write.
    Observed,
    /// The file must not exist: `model init` publishes the first model, and a
    /// concurrently created one makes the plan stale rather than overwritten.
    Absent,
}

/// Capture a project's external facts, once, for one compilation.
///
/// `model` is the model on disk; `intended` is the model the plan will
/// compile when it differs -- **the reader trees are chosen from the
/// intended model, not the one on disk**. Which trees a plan needs is a
/// question about the intended state: `add db` has to splice
/// `@Import(TestcontainersConfig.class)` into the reader's `@SpringBootTest`
/// classes, and the very command that introduces `db` is the one whose
/// current model does not have it. Asking the current model does half the
/// work in silence: `add db` generates the config and adds the starter,
/// splices nothing, and leaves `mvn verify` red on the `contextLoads` test
/// the project ships with; a later `jails sync` -- whose model *is* the
/// intended one -- quietly repairs it, which is why a test has to assert
/// after each command rather than after two.
///
/// `reader_paths` are the reader-owned files the plan may edit beyond the
/// trees the model implies; pass `&[]` when the caller has none.
pub fn capture(
    root: &Path,
    model_path: &Path,
    model_source: &[u8],
    model: AppModel,
    intended: Option<&AppModel>,
    reader_paths: &[ProjectPath],
    model_file: ModelFile,
) -> Result<WorkspaceSnapshot, Diagnostic> {
    let trees = ReaderTrees::of(intended.unwrap_or(&model));
    capture_model_state(
        root,
        model_path,
        model_source,
        model,
        reader_paths,
        model_file == ModelFile::Observed,
        trees,
    )
}

/// Which reader-owned source trees a model's plan may need to edit.
///
/// Whole trees rather than named files because the anchors are found by
/// *shape*: the dispatcher `g command` registers into, and the
/// `@SpringBootTest` classes `add db` imports the container config into. Which
/// file has that shape is an observation, so the tree is the read set.
///
/// Conditional because the cost is real: every captured file is a
/// precondition, so capturing either tree unconditionally would make an edit
/// to any unrelated source invalidate a reviewed plan.
#[derive(Clone, Copy, Debug)]
struct ReaderTrees {
    main: bool,
    test: bool,
}

impl ReaderTrees {
    fn of(model: &AppModel) -> Self {
        Self {
            main: model.components.values().any(|component| {
                matches!(
                    component.kind,
                    jails_model::ComponentKind::Command | jails_model::ComponentKind::Cli
                )
            })
            // **A field naming a type the reader owns is a question only
            // their sources can answer**, and the compiler refuses when
            // nothing declares it -- so the tree has to be read before that
            // refusal can be right. Only when there is such a field: reading
            // every Java file on every `g enum` would be a cost paid for a
            // question nobody asked. A qualified name is somebody else's
            // package and is not this project's to declare.
                || model.entities.values().any(|entity| {
                    entity.active
                        && entity.fields.iter().any(|field| {
                            matches!(&field.ty, jails_model::TypeRef::External(name)
                                if !name.contains('.'))
                        })
                }),
            test: model
                .capabilities
                .values()
                .any(|capability| capability.kind == "db"),
        }
    }

    /// Every tree either model needs.
    ///
    /// Installing a capability and retiring one edit the same reader files, so
    /// the read set is the union of what the accepted model wanted and what
    /// the intended one wants -- narrowing to the intended model alone leaves
    /// `remove db`'s splice behind.
    fn union(self, other: Self) -> Self {
        Self {
            main: self.main || other.main,
            test: self.test || other.test,
        }
    }
}

fn capture_model_state(
    root: &Path,
    model_path: &Path,
    model_source: &[u8],
    model: AppModel,
    reader_paths: &[ProjectPath],
    model_present: bool,
    trees: ReaderTrees,
) -> Result<WorkspaceSnapshot, Diagnostic> {
    let relative_model = if model_path.is_absolute() {
        model_path.strip_prefix(root).map_err(|_| {
            Diagnostic::without_a_fix(
                "workspace-model-path-outside-root",
                model_path.display().to_string(),
                "model path is outside the project root",
            )
        })?
    } else {
        model_path
    };
    let model_path = project_path(path_text(relative_model))?;
    let model_digest = digest(model_source)?;
    let mut files = BTreeMap::new();
    // **Observed, not asserted.** The caller passes the model *source*, which
    // is not the same question as whether the file is on disk: a project with
    // no model reads as the seed `model init` would write, so a frontend hands
    // over bytes for a file that may not exist yet. Taking the caller's word
    // for it would record a `Present` precondition over nothing, and the
    // executor would then refuse its own plan with "it is gone" -- a stale
    // plan against a file jails is about to write.
    let model_present = model_present && root.join(model_path.as_str()).is_file();
    let model_precondition = if model_present {
        files.insert(
            model_path.clone(),
            CapturedFile {
                bytes: model_source.to_vec(),
                executable: false,
            },
        );
        FilePrecondition::Present {
            digest: model_digest.clone(),
            executable: false,
        }
    } else {
        FilePrecondition::Missing
    };
    let mut preconditions = SnapshotPreconditions {
        files: BTreeMap::from([(model_path, model_precondition)]),
        directories: BTreeMap::new(),
    };

    // **The lock is read first, and the paths it names are the managed
    // set.** Managed output lives beside the reader's own files under `src/`,
    // and nothing about a path says whose it is: a file is jails' because the
    // accepted projection names it. So every path the projection names is
    // observed -- present with its bytes, which are OURS for the merge, or
    // missing, which is a deletion the materializer answers -- and no
    // directory is walked for it.
    //
    // **The accepted model is read before the reader trees are chosen**,
    // because retiring a capability edits the same tree that installing it
    // did. `remove db` produces an intended model with no `db` in it, and a
    // plan built from that alone reads no test tree -- so the
    // `@Import(TestcontainersConfig.class)` it spliced has no before-image,
    // cannot be named in the plan, and stays behind importing a class the same
    // transition deletes.
    capture_optional_file(root, COMPILER_LOCK, &mut files, &mut preconditions)?;
    let accepted = accepted_compiler_state(&files)?;
    let managed = accepted
        .as_ref()
        .and_then(|state| state.projection.as_ref())
        .map(|projection| projection.files.keys().cloned().collect::<BTreeSet<_>>())
        .unwrap_or_default();
    for path in &managed {
        capture_optional_file(root, path.as_str(), &mut files, &mut preconditions)?;
    }
    let trees = match accepted.as_ref() {
        Some(state) => trees.union(ReaderTrees::of(&state.model)),
        None => trees,
    };
    // **The reader's own sources, when the model's plan may edit them.**
    //
    // The dispatcher `g command` registers into and the `@SpringBootTest`
    // classes the container `@Import` targets are found by shape, and which
    // files have that shape is an observation -- the compiler cannot
    // enumerate a directory and must not try. Capturing them here makes the
    // plan exact: every reader file the plan edits has a before-image, so one
    // edited after review makes the plan stale rather than being silently
    // overwritten.
    //
    // Which trees, and why the *intended* model decides it, is
    // [`ReaderTrees`] and [`capture`].
    let reader_main = root.join(READER_MAIN_ROOT);
    if trees.main && reader_main.exists() {
        capture_tree(root, &reader_main, &mut files, &mut preconditions)?;
    }
    let reader_tests = root.join(READER_TEST_ROOT);
    if trees.test && reader_tests.exists() {
        capture_tree(root, &reader_tests, &mut files, &mut preconditions)?;
    }
    for reader_file in [
        "jails.toml",
        "pom.xml",
        "build.gradle",
        "build.gradle.kts",
        "src/main/resources/application.properties",
        "src/test/resources/config/application.properties",
        "compose.yaml",
        "compose.yml",
        "docker-compose.yml",
        "docker-compose.yaml",
    ] {
        capture_optional_file(root, reader_file, &mut files, &mut preconditions)?;
    }
    for reader_path in reader_paths {
        capture_optional_file(root, reader_path.as_str(), &mut files, &mut preconditions)?;
    }
    // Read from the capture, not from disk a second time: the whole point of
    // the snapshot is that an external fact is observed once, and a layout read
    // separately could disagree with the precondition recorded above.
    let layout = match files.get(&project_path("jails.toml")?) {
        Some(captured) => {
            Layout::parse(&String::from_utf8_lossy(&captured.bytes)).map_err(layout_invalid)?
        }
        None => Layout::default(),
    };
    // **The model is desired-state authority** for the two facts it states:
    // the build file's release and the source tree's package are what the
    // reader has, the model's are what the compiler emits for.
    let mut project = observe::observed(root, layout);
    project.java_release = model.project.java_release;
    project.base_package = model.project.base_package.clone();
    let accepted_reader_paths = accepted
        .as_ref()
        .and_then(|state| state.projection.as_ref())
        .into_iter()
        .flat_map(|projection| projection.reader_facets.values())
        .map(|facet| facet.path.clone())
        .collect::<BTreeSet<_>>();
    for path in accepted_reader_paths {
        capture_optional_file(root, path.as_str(), &mut files, &mut preconditions)?;
    }
    let mut snapshot = WorkspaceSnapshot::detached(model);
    snapshot.model.source_digest = Some(model_digest);
    if let Some(accepted) = accepted {
        snapshot.accepted_model = Some(accepted.model);
        snapshot.accepted_projection = accepted.projection;
        snapshot.accepted_compiler = accepted.compiler;
        snapshot.accepted_migrations = accepted.migrations;
        snapshot.accepted_migration_bytes = accepted.migration_bytes;
    }
    snapshot.template_overrides = template_overrides(root);
    snapshot.project = project;
    snapshot.migration_history = capture_migration_history(root, &mut files, &mut preconditions)?;
    snapshot.preconditions = preconditions;
    snapshot.external_types = index_reader_types(&files, &managed);
    snapshot.files = files;
    Ok(snapshot)
}

/// Observe the paths a rendered tree wants that the capture has not seen.
///
/// **The one observation the lock cannot make.** The managed set is the
/// accepted projection's paths, and the next render can want a path the lock
/// does not own -- a new entity's record, say. Whether the reader already has
/// a file there is a fact about the tree, so it is observed here, once, after
/// the compiler has said which paths it wants: a present file becomes OURS
/// and the materializer refuses the collision by name, and an absent one
/// becomes the `Missing` precondition that makes a later appearance stale
/// rather than overwritten. Paths the capture already answered for are left
/// as they were, so this widens the snapshot and never changes it.
pub fn observe_rendered_paths<'a>(
    root: &Path,
    snapshot: &mut WorkspaceSnapshot,
    paths: impl IntoIterator<Item = &'a ProjectPath>,
) -> Result<(), Diagnostic> {
    for path in paths {
        if snapshot.preconditions.files.contains_key(path) {
            continue;
        }
        capture_optional_file(
            root,
            path.as_str(),
            &mut snapshot.files,
            &mut snapshot.preconditions,
        )?;
    }
    Ok(())
}

/// Every path the accepted projection names: the managed set, for a command
/// that walks the reader's sources without capturing them.
///
/// Managed files sit beside the reader's own under `src/`, so a textual
/// rename or a stranded-reference report walking the tree has to be told
/// which files are jails' -- and the lock is the only thing that knows. No
/// lock is no managed file; an unreadable one is an error, as in [`capture`].
pub fn managed_paths(root: &Path) -> Result<BTreeSet<ProjectPath>, Diagnostic> {
    let mut files = BTreeMap::new();
    let mut preconditions = SnapshotPreconditions::default();
    capture_optional_file(root, COMPILER_LOCK, &mut files, &mut preconditions)?;
    Ok(accepted_compiler_state(&files)?
        .and_then(|state| state.projection)
        .map(|projection| projection.files.into_keys().collect())
        .unwrap_or_default())
}

/// Every Java type the reader's own sources declare, by simple name.
///
/// A capitalised field type is a type this project owns, and this is what
/// checks that it does: otherwise `g scaffold Book author:Author` emits a
/// record naming `Author`, and the project stops compiling on a file the
/// reader never wrote -- the exact failure the tool exists to remove. The
/// compiler cannot look at the filesystem, so the answer is observed once
/// here, like every other external fact.
///
/// Read off the declaration line rather than the path, because a checkout's
/// directories do not always match its packages, with line comments stripped
/// so a type named inside one is not mistaken for one that exists.
fn index_reader_types(
    files: &BTreeMap<ProjectPath, CapturedFile>,
    managed: &BTreeSet<ProjectPath>,
) -> jails_contracts::ExternalTypeIndex {
    let mut index = jails_contracts::ExternalTypeIndex::default();
    for (path, file) in files {
        if !path.as_str().ends_with(".java") {
            continue;
        }
        // **The reader's sources only.** The managed files are what the plan
        // is about to change, so a type that is in one now may not be in it
        // after -- counting one would let `destroy enum Status` succeed while
        // an entity still declares a `Status` field, leaving a record that no
        // longer compiles. What the model declares is asked of the model, and
        // which files are managed is asked of the lock.
        if managed.contains(path) {
            continue;
        }
        let Ok(source) = std::str::from_utf8(&file.bytes) else {
            continue;
        };
        let package = declared_package(source);
        for name in declared_types(source) {
            let qualified = if package.is_empty() {
                name.clone()
            } else {
                format!("{package}.{name}")
            };
            index
                .types
                .entry(name)
                .or_insert(jails_contracts::ExternalType {
                    qualified_name: qualified,
                    capabilities: BTreeSet::new(),
                });
        }
    }
    index
}

/// The `package` this source declares, or empty for the default package.
fn declared_package(source: &str) -> String {
    source
        .lines()
        .find_map(|line| {
            line.trim()
                .strip_prefix("package ")?
                .trim()
                .strip_suffix(';')
                .map(|name| name.trim().to_string())
        })
        .unwrap_or_default()
}

/// Every type name this source declares, nested ones included.
///
/// **Deliberately over-collecting, and that is the safe direction.** The one
/// consumer refuses a field whose type nothing declares, so a name picked up
/// from a string literal costs a refusal that does not happen; a name *missed*
/// would refuse a project that compiles. Line comments are stripped because
/// they only ever add, and nothing else is worth a parser here -- this
/// answers "does this name exist", not "what is its shape".
fn declared_types(source: &str) -> Vec<String> {
    const KEYWORDS: [&str; 4] = ["class ", "record ", "interface ", "enum "];
    let mut found = Vec::new();
    for line in source.lines() {
        let line = line.split("//").next().unwrap_or(line).trim();
        for keyword in KEYWORDS {
            let mut rest = line;
            while let Some(at) = rest.find(keyword) {
                let before_is_word = rest[..at]
                    .chars()
                    .next_back()
                    .is_some_and(|last| last.is_alphanumeric() || last == '_' || last == '.');
                rest = &rest[at + keyword.len()..];
                if before_is_word {
                    continue;
                }
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                if !name.is_empty() {
                    found.push(name);
                }
            }
        }
    }
    found
}

fn capture_migration_history(
    root: &Path,
    files: &mut BTreeMap<ProjectPath, CapturedFile>,
    preconditions: &mut SnapshotPreconditions,
) -> Result<MigrationHistory, Diagnostic> {
    let migration_root = project_path(MIGRATION_ROOT)?;
    let absolute = root.join(MIGRATION_ROOT);
    if !absolute.exists() {
        preconditions
            .directories
            .insert(migration_root, DirectoryPrecondition::Missing);
        return Ok(MigrationHistory {
            records: Vec::new(),
        });
    }
    let metadata = std::fs::symlink_metadata(&absolute)
        .map_err(|error| refused_io("inspect", &absolute, error))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(Diagnostic::without_a_fix(
            "workspace-migration-root-not-a-directory",
            MIGRATION_ROOT,
            format!("`{MIGRATION_ROOT}` is not a regular directory"),
        ));
    }
    capture_tree(root, &absolute, files, preconditions)?;
    let directory = directory_precondition_from_files(files, &migration_root)?;
    preconditions
        .directories
        .insert(migration_root.clone(), directory);

    let mut records = files
        .iter()
        .filter(|(path, _)| path.is_within(&migration_root))
        .filter_map(|(path, file)| {
            let name = path.as_str().rsplit('/').next()?;
            let version = name.strip_prefix('V')?.split_once("__")?.0;
            name.ends_with(".sql").then(|| {
                Ok::<_, Diagnostic>(MigrationRecord {
                    version: version.to_string(),
                    path: path.clone(),
                    digest: digest(&file.bytes)?,
                })
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    records.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(MigrationHistory { records })
}

fn accepted_compiler_state(
    files: &BTreeMap<ProjectPath, CapturedFile>,
) -> Result<Option<AcceptedCompilerState>, Diagnostic> {
    files
        .get(&project_path(COMPILER_LOCK)?)
        .map(|file| decode_compiler_lock(&file.bytes))
        .transpose()
}

fn decode_compiler_lock(bytes: &[u8]) -> Result<AcceptedCompilerState, Diagnostic> {
    let mut header: serde_json::Value = serde_json::from_slice(bytes).map_err(lock_undecodable)?;
    // **Back to the shape the types decode from, whatever the file holds.**
    // A v4 lock stores a generated file's bytes as text; every earlier one
    // stores an array and has nothing to expand. Either way what the
    // verification below digests is the one form `serde` derives, so a lock
    // from any release is checked under one rule.
    jails_contracts::lock_bytes::expand(&mut header);
    let schema = header
        .get("schema")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    match schema {
        COMPILER_LOCK_SCHEMA_V1 => {
            let lock: CompilerLockV1 = serde_json::from_value(header).map_err(lock_undecodable)?;
            verify_model(&lock.model, &lock.model_digest)?;
            Ok(AcceptedCompilerState {
                model: lock.model,
                projection: None,
                compiler: None,
                migrations: BTreeMap::new(),
                migration_bytes: BTreeMap::new(),
            })
        }
        COMPILER_LOCK_SCHEMA_V2 => {
            let lock: CompilerLockV2 = serde_json::from_value(header).map_err(lock_undecodable)?;
            verify_model(&lock.model, &lock.model_digest)?;
            let projection = serde_json::to_vec(&lock.projection).map_err(lock_unverifiable)?;
            if digest(&projection)? != lock.projection_digest {
                return Err(lock_projection_mismatch());
            }
            Ok(AcceptedCompilerState {
                model: lock.model,
                projection: Some(lock.projection),
                compiler: Some(lock.compiler),
                migrations: BTreeMap::new(),
                migration_bytes: BTreeMap::new(),
            })
        }
        COMPILER_LOCK_SCHEMA_V3 | COMPILER_LOCK_SCHEMA_V4 => {
            let lock: CompilerLockV3 = serde_json::from_value(header).map_err(lock_undecodable)?;
            verify_model(&lock.model, &lock.model_digest)?;
            let projection = serde_json::to_vec(&lock.projection).map_err(lock_unverifiable)?;
            if digest(&projection)? != lock.projection_digest {
                return Err(lock_projection_mismatch());
            }
            Ok(AcceptedCompilerState {
                model: lock.model,
                projection: Some(lock.projection),
                compiler: Some(lock.compiler),
                migrations: lock.migrations,
                migration_bytes: lock.migration_bytes,
            })
        }
        other => Err(Diagnostic::new(
            "workspace-lock-schema-unsupported",
            COMPILER_LOCK,
            format!("unsupported compiler lock `{other}`"),
            format!("regenerate `{COMPILER_LOCK}` with this version of jails"),
        )),
    }
}

fn verify_model(model: &AppModel, expected: &ContentDigest) -> Result<(), Diagnostic> {
    let actual = digest(&model.canonical_json().map_err(lock_unverifiable)?)?;
    if &actual != expected {
        return Err(Diagnostic::new(
            "workspace-lock-model-mismatch",
            COMPILER_LOCK,
            format!("compiler lock `{COMPILER_LOCK}` does not match its accepted model"),
            "restore a known-good lock; do not infer merge bases from generated source",
        ));
    }
    Ok(())
}

pub fn observe_directory(
    root: &Path,
    path: &ProjectPath,
) -> Result<DirectoryPrecondition, Diagnostic> {
    let absolute = root.join(path.as_str());
    let metadata = match std::fs::symlink_metadata(&absolute) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(DirectoryPrecondition::Missing);
        }
        Err(error) => return Err(refused_io("inspect", &absolute, error)),
    };
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(Diagnostic::without_a_fix(
            "workspace-not-a-regular-directory",
            path.to_string(),
            format!("`{}` is not a regular directory", path.as_str()),
        ));
    }
    let mut files = BTreeMap::new();
    let mut ignored = SnapshotPreconditions::default();
    capture_tree(root, &absolute, &mut files, &mut ignored)?;
    directory_precondition_from_files(&files, path)
}

fn directory_precondition_from_files(
    files: &BTreeMap<ProjectPath, CapturedFile>,
    root: &ProjectPath,
) -> Result<DirectoryPrecondition, Diagnostic> {
    let entries = files
        .iter()
        .filter(|(path, _)| path.is_within(root))
        .map(|(path, file)| {
            Ok::<_, Diagnostic>((path.as_str(), digest(&file.bytes)?, file.executable))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let encoded = serde_json::to_vec(&entries).map_err(|error| {
        Diagnostic::without_a_fix(
            "workspace-directory-precondition-encoding",
            root.to_string(),
            format!("could not encode directory precondition: {error}"),
        )
    })?;
    Ok(DirectoryPrecondition::Present {
        digest: digest(&encoded)?,
    })
}

fn capture_optional_file(
    root: &Path,
    relative: &str,
    files: &mut BTreeMap<ProjectPath, CapturedFile>,
    preconditions: &mut SnapshotPreconditions,
) -> Result<(), Diagnostic> {
    let path = root.join(relative);
    let metadata = match std::fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            preconditions
                .files
                .insert(project_path(relative)?, FilePrecondition::Missing);
            return Ok(());
        }
        Err(error) => return Err(refused_io("inspect", &path, error)),
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(Diagnostic::without_a_fix(
            "workspace-reader-file-not-regular",
            relative,
            format!("`{relative}` is not a regular reader file"),
        ));
    }
    let bytes = std::fs::read(&path).map_err(|error| refused_io("read", &path, error))?;
    let path = project_path(relative)?;
    let executable = executable(&metadata);
    preconditions.files.insert(
        path.clone(),
        FilePrecondition::Present {
            digest: digest(&bytes)?,
            executable,
        },
    );
    files.insert(path, CapturedFile { bytes, executable });
    Ok(())
}

fn capture_tree(
    root: &Path,
    directory: &Path,
    files: &mut BTreeMap<ProjectPath, CapturedFile>,
    preconditions: &mut SnapshotPreconditions,
) -> Result<(), Diagnostic> {
    let entries =
        std::fs::read_dir(directory).map_err(|error| refused_io("read", directory, error))?;
    for entry in entries {
        let entry = entry.map_err(|error| refused_io("read an entry under", directory, error))?;
        let path = entry.path();
        let metadata = std::fs::symlink_metadata(&path)
            .map_err(|error| refused_io("inspect", &path, error))?;
        if metadata.file_type().is_symlink() {
            return Err(Diagnostic::without_a_fix(
                "workspace-managed-output-symlink",
                path.display().to_string(),
                format!(
                    "managed output `{}` is a symlink; replace it with a regular file",
                    path.display()
                ),
            ));
        }
        if metadata.is_dir() {
            capture_tree(root, &path, files, preconditions)?;
            continue;
        }
        if !metadata.is_file() {
            return Err(Diagnostic::without_a_fix(
                "workspace-managed-output-not-regular",
                path.display().to_string(),
                format!("managed output `{}` is not a regular file", path.display()),
            ));
        }
        let bytes = std::fs::read(&path).map_err(|error| refused_io("read", &path, error))?;
        let relative = path.strip_prefix(root).map_err(|_| {
            Diagnostic::without_a_fix(
                "workspace-capture-path-escaped-root",
                path.display().to_string(),
                format!("{} escaped the project root", path.display()),
            )
        })?;
        let project_path = project_path(path_text(relative))?;
        let executable = executable(&metadata);
        preconditions.files.insert(
            project_path.clone(),
            FilePrecondition::Present {
                digest: digest(&bytes)?,
                executable,
            },
        );
        files.insert(project_path, CapturedFile { bytes, executable });
    }
    Ok(())
}

fn path_text(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(unix)]
fn executable(metadata: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt as _;
    metadata.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn executable(_metadata: &std::fs::Metadata) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// An app block and nothing else: the lock tests need a model, not a tree.
    const MODEL: &str = "jdl 1\n\napp Notes @id(project_notes) {\n  pkg com.example.notes\n  \
         java 26\n  platform spring\n  build maven\n  storage none\n}\n";

    #[test]
    fn v1_lock_remains_a_one_way_upgrade_input() {
        let model = jails_model::parse_jdl(MODEL).unwrap();
        let model_digest = digest(&model.canonical_json().unwrap()).unwrap();
        let bytes = serde_json::to_vec(&json!({
            "schema": COMPILER_LOCK_SCHEMA_V1,
            "model_digest": model_digest,
            "model": model,
        }))
        .unwrap();

        let accepted = decode_compiler_lock(&bytes).unwrap();
        assert!(accepted.projection.is_none());
        assert!(accepted.compiler.is_none());

        // ... and a lock whose model does not match its digest is refused
        // rather than trusted, which is the property the digest is for.
        let mut tampered: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        tampered["model_digest"] =
            json!("sha256:0000000000000000000000000000000000000000000000000000000000000000");
        let error = decode_compiler_lock(&serde_json::to_vec(&tampered).unwrap())
            .expect_err("a lock that disagrees with its own digest must refuse");
        assert!(error.to_string().contains("compiler.lock"), "{error}");
    }

    #[test]
    fn v2_lock_refuses_a_projection_that_does_not_match_its_digest() {
        let model = jails_model::parse_jdl(MODEL).unwrap();
        let model_digest = digest(&model.canonical_json().unwrap()).unwrap();
        let projection = RenderedTree::new();
        let projection_digest = digest(&serde_json::to_vec(&projection).unwrap()).unwrap();
        let mut damaged = projection;
        damaged.files.insert(
            project_path("src/main/java/Damaged.java").unwrap(),
            jails_contracts::RenderedFile {
                kind: jails_contracts::FileKind::JavaMain,
                mode: jails_contracts::FileMode::Regular,
                bytes: Vec::new(),
                provenance: jails_contracts::Provenance {
                    artifact_id: "art_damaged".to_string(),
                    semantic_ids: BTreeSet::new(),
                    ejection_id: None,
                    ejectable: false,
                    compiler_pass: String::new(),
                },
            },
        );
        let bytes = serde_json::to_vec(&json!({
            "schema": COMPILER_LOCK_SCHEMA_V2,
            "compiler": "0.1.0",
            "model_digest": model_digest,
            "model": model,
            "projection_digest": projection_digest,
            "projection": damaged,
        }))
        .unwrap();

        let error = decode_compiler_lock(&bytes).unwrap_err();
        assert!(error.to_string().contains("accepted projection"), "{error}");
    }
}
