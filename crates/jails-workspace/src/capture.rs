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
    MigrationRecord, ProjectFacts, ProjectPath, RenderedTree, SnapshotPreconditions,
    WorkspaceSnapshot,
};
use jails_model::AppModel;
use jails_support::codec::{hex, sha256};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

mod observe;

use observe::{
    build_artifact_id, declared_dependencies, junit_version, spring_boot_version,
    template_overrides,
};
pub use observe::{observe_build_system, observe_spring_boot};

const MANAGED_ROOT: &str = ".jails/generated";
pub(crate) const MIGRATION_ROOT: &str = "src/main/resources/db/migration";
const READER_MAIN_ROOT: &str = "src/main/java";
const READER_TEST_ROOT: &str = "src/test/java";
pub(crate) const COMPILER_LOCK: &str = ".jails/compiler.lock.json";
const COMPILER_LOCK_SCHEMA_V1: &str = "jails.compiler-lock.v1";
const COMPILER_LOCK_SCHEMA_V2: &str = "jails.compiler-lock.v2";
const COMPILER_LOCK_SCHEMA_V3: &str = "jails.compiler-lock.v3";

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

pub fn capture(
    root: &Path,
    model_path: &Path,
    model_source: &[u8],
    model: AppModel,
) -> Result<WorkspaceSnapshot, String> {
    capture_with_reader_paths(root, model_path, model_source, model, &[])
}

pub fn capture_with_reader_paths(
    root: &Path,
    model_path: &Path,
    model_source: &[u8],
    model: AppModel,
    reader_paths: &[ProjectPath],
) -> Result<WorkspaceSnapshot, String> {
    let trees = ReaderTrees::of(&model);
    capture_model_state(
        root,
        model_path,
        model_source,
        model,
        reader_paths,
        true,
        trees,
    )
}

/// Capture for a plan that is about to *change* the model.
///
/// **The reader trees are chosen from the model the patch produces, not the
/// one on disk.** Which of them a plan needs is a question about the intended
/// state -- `add db` has to splice `@Import(TestcontainersConfig.class)` into
/// the reader's `@SpringBootTest` classes, and the very command that
/// introduces `db` is the one whose pre-patch model does not have it.
///
/// Asking the pre-patch model does half the work in silence: `add db` on a
/// Spring project generates the config and adds the starter, splices nothing,
/// and leaves `mvn verify` red on the `contextLoads` test the project ships
/// with. A later `jails sync` -- whose model *is* the intended one -- quietly
/// repairs it, which is why a test has to assert after each command rather
/// than after two.
pub fn capture_planned(
    root: &Path,
    model_path: &Path,
    model_source: &[u8],
    model: AppModel,
    intended: &AppModel,
    reader_paths: &[ProjectPath],
) -> Result<WorkspaceSnapshot, String> {
    let trees = ReaderTrees::of(intended);
    capture_model_state(
        root,
        model_path,
        model_source,
        model,
        reader_paths,
        true,
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

/// Capture a project before its one-way canonical model is published.
///
/// The supplied source is compiler input, but the model path itself is a
/// missing-file precondition so a concurrently created model makes the exact
/// import plan stale rather than being overwritten.
pub fn capture_import(
    root: &Path,
    model_path: &Path,
    model_source: &[u8],
    model: AppModel,
    reader_paths: &[ProjectPath],
) -> Result<WorkspaceSnapshot, String> {
    let trees = ReaderTrees::of(&model);
    capture_model_state(
        root,
        model_path,
        model_source,
        model,
        reader_paths,
        false,
        trees,
    )
}

fn capture_model_state(
    root: &Path,
    model_path: &Path,
    model_source: &[u8],
    model: AppModel,
    reader_paths: &[ProjectPath],
    model_present: bool,
    trees: ReaderTrees,
) -> Result<WorkspaceSnapshot, String> {
    let relative_model = if model_path.is_absolute() {
        model_path
            .strip_prefix(root)
            .map_err(|_| "model path is outside the project root".to_string())?
    } else {
        model_path
    };
    let model_path = ProjectPath::parse(path_text(relative_model))?;
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

    let managed = root.join(MANAGED_ROOT);
    if managed.exists() {
        capture_tree(root, &managed, &mut files, &mut preconditions)?;
    }
    // **The accepted model is read before the reader trees are chosen**,
    // because retiring a capability edits the same tree that installing it
    // did. `remove db` produces an intended model with no `db` in it, and a
    // plan built from that alone reads no test tree -- so the
    // `@Import(TestcontainersConfig.class)` it spliced has no before-image,
    // cannot be named in the plan, and stays behind importing a class the same
    // transition deletes. The lock is already in `files` from the managed walk
    // above, so this costs a decode, not a second traversal.
    capture_optional_file(root, COMPILER_LOCK, &mut files, &mut preconditions)?;
    let accepted = accepted_compiler_state(&files)?;
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
    // [`ReaderTrees`] and `capture_planned`.
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
    let build_system = observe_build_system(root);
    // Read from the capture, not from disk a second time: the whole point of
    // the snapshot is that an external fact is observed once, and a layout read
    // separately could disagree with the precondition recorded above.
    let layout = match files.get(&ProjectPath::parse("jails.toml")?) {
        Some(captured) => Layout::parse(&String::from_utf8_lossy(&captured.bytes))?,
        None => Layout::default(),
    };
    let project = ProjectFacts {
        build_system,
        java_release: model.project.java_release,
        spring_boot: spring_boot_version(root, build_system),
        base_package: model.project.base_package.clone(),
        dependencies: declared_dependencies(root, build_system),
        maven_wrapper: root.join("mvnw").is_file(),
        layout,
        junit: junit_version(root, build_system),
        artifact_id: build_artifact_id(root, build_system),
    };
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
    snapshot.external_types = index_reader_types(&files);
    snapshot.files = files;
    Ok(snapshot)
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
) -> jails_contracts::ExternalTypeIndex {
    let mut index = jails_contracts::ExternalTypeIndex::default();
    for (path, file) in files {
        if !path.as_str().ends_with(".java") {
            continue;
        }
        // **The reader's sources only.** The managed tree is what the plan is
        // about to change, so a type that is in it now may not be in it after
        // -- counting one would let `destroy enum Status` succeed while an
        // entity still declares a `Status` field, leaving a record that no
        // longer compiles. What the model declares is asked of the model.
        if path.as_str().starts_with(".jails/") {
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
) -> Result<MigrationHistory, String> {
    let migration_root = ProjectPath::parse(MIGRATION_ROOT)?;
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
        .map_err(|error| format!("could not inspect {}: {error}", absolute.display()))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(format!("`{MIGRATION_ROOT}` is not a regular directory"));
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
                Ok::<_, String>(MigrationRecord {
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
) -> Result<Option<AcceptedCompilerState>, String> {
    files
        .get(&ProjectPath::parse(COMPILER_LOCK)?)
        .map(|file| decode_compiler_lock(&file.bytes))
        .transpose()
}

/// The decoder, reachable from a test in a sibling module.
///
/// An *older* lock must still decode, and the v1 arm is where that is
/// asserted -- a schema branch nothing exercises is one that has already
/// rotted without saying so.
#[cfg(test)]
pub(crate) fn decode_compiler_lock_for_test(bytes: &[u8]) -> Result<(), String> {
    decode_compiler_lock(bytes).map(|_| ())
}

fn decode_compiler_lock(bytes: &[u8]) -> Result<AcceptedCompilerState, String> {
    let header: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|error| format!("could not decode `{COMPILER_LOCK}`: {error}"))?;
    let schema = header
        .get("schema")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    match schema {
        COMPILER_LOCK_SCHEMA_V1 => {
            let lock: CompilerLockV1 = serde_json::from_value(header)
                .map_err(|error| format!("could not decode `{COMPILER_LOCK}`: {error}"))?;
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
            let lock: CompilerLockV2 = serde_json::from_value(header)
                .map_err(|error| format!("could not decode `{COMPILER_LOCK}`: {error}"))?;
            verify_model(&lock.model, &lock.model_digest)?;
            let projection = serde_json::to_vec(&lock.projection)
                .map_err(|error| format!("could not verify `{COMPILER_LOCK}`: {error}"))?;
            if digest(&projection)? != lock.projection_digest {
                return Err(format!(
                    "compiler lock `{COMPILER_LOCK}` does not match its accepted projection\n       fix: restore a known-good lock; do not infer merge bases from generated source"
                ));
            }
            Ok(AcceptedCompilerState {
                model: lock.model,
                projection: Some(lock.projection),
                compiler: Some(lock.compiler),
                migrations: BTreeMap::new(),
                migration_bytes: BTreeMap::new(),
            })
        }
        COMPILER_LOCK_SCHEMA_V3 => {
            let lock: CompilerLockV3 = serde_json::from_value(header)
                .map_err(|error| format!("could not decode `{COMPILER_LOCK}`: {error}"))?;
            verify_model(&lock.model, &lock.model_digest)?;
            let projection = serde_json::to_vec(&lock.projection)
                .map_err(|error| format!("could not verify `{COMPILER_LOCK}`: {error}"))?;
            if digest(&projection)? != lock.projection_digest {
                return Err(format!(
                    "compiler lock `{COMPILER_LOCK}` does not match its accepted projection\n       fix: restore a known-good lock; do not infer merge bases from generated source"
                ));
            }
            Ok(AcceptedCompilerState {
                model: lock.model,
                projection: Some(lock.projection),
                compiler: Some(lock.compiler),
                migrations: lock.migrations,
                migration_bytes: lock.migration_bytes,
            })
        }
        other => Err(format!(
            "unsupported compiler lock `{other}`\n       fix: regenerate `{COMPILER_LOCK}` with this version of jails"
        )),
    }
}

fn verify_model(model: &AppModel, expected: &ContentDigest) -> Result<(), String> {
    let actual = digest(
        &model
            .canonical_json()
            .map_err(|error| format!("could not verify `{COMPILER_LOCK}`: {error}"))?,
    )?;
    if &actual != expected {
        return Err(format!(
            "compiler lock `{COMPILER_LOCK}` does not match its accepted model\n       fix: restore a known-good lock; do not infer merge bases from generated source"
        ));
    }
    Ok(())
}

pub(crate) fn observe_directory(
    root: &Path,
    path: &ProjectPath,
) -> Result<DirectoryPrecondition, String> {
    let absolute = root.join(path.as_str());
    let metadata = match std::fs::symlink_metadata(&absolute) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(DirectoryPrecondition::Missing);
        }
        Err(error) => return Err(format!("could not inspect {}: {error}", absolute.display())),
    };
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(format!("`{}` is not a regular directory", path.as_str()));
    }
    let mut files = BTreeMap::new();
    let mut ignored = SnapshotPreconditions::default();
    capture_tree(root, &absolute, &mut files, &mut ignored)?;
    directory_precondition_from_files(&files, path)
}

fn directory_precondition_from_files(
    files: &BTreeMap<ProjectPath, CapturedFile>,
    root: &ProjectPath,
) -> Result<DirectoryPrecondition, String> {
    let entries = files
        .iter()
        .filter(|(path, _)| path.is_within(root))
        .map(|(path, file)| Ok::<_, String>((path.as_str(), digest(&file.bytes)?, file.executable)))
        .collect::<Result<Vec<_>, _>>()?;
    let encoded = serde_json::to_vec(&entries)
        .map_err(|error| format!("could not encode directory precondition: {error}"))?;
    Ok(DirectoryPrecondition::Present {
        digest: digest(&encoded)?,
    })
}

fn capture_optional_file(
    root: &Path,
    relative: &str,
    files: &mut BTreeMap<ProjectPath, CapturedFile>,
    preconditions: &mut SnapshotPreconditions,
) -> Result<(), String> {
    let path = root.join(relative);
    let metadata = match std::fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            preconditions
                .files
                .insert(ProjectPath::parse(relative)?, FilePrecondition::Missing);
            return Ok(());
        }
        Err(error) => return Err(format!("could not inspect {}: {error}", path.display())),
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(format!("`{relative}` is not a regular reader file"));
    }
    let bytes = std::fs::read(&path)
        .map_err(|error| format!("could not read {}: {error}", path.display()))?;
    let path = ProjectPath::parse(relative)?;
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
) -> Result<(), String> {
    let entries = std::fs::read_dir(directory)
        .map_err(|error| format!("could not read {}: {error}", directory.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| {
            format!(
                "could not read an entry under {}: {error}",
                directory.display()
            )
        })?;
        let path = entry.path();
        let metadata = std::fs::symlink_metadata(&path)
            .map_err(|error| format!("could not inspect {}: {error}", path.display()))?;
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "managed output `{}` is a symlink; replace it with a regular file",
                path.display()
            ));
        }
        if metadata.is_dir() {
            capture_tree(root, &path, files, preconditions)?;
            continue;
        }
        if !metadata.is_file() {
            return Err(format!(
                "managed output `{}` is not a regular file",
                path.display()
            ));
        }
        let bytes = std::fs::read(&path)
            .map_err(|error| format!("could not read {}: {error}", path.display()))?;
        let relative = path
            .strip_prefix(root)
            .map_err(|_| format!("{} escaped the project root", path.display()))?;
        let project_path = ProjectPath::parse(path_text(relative))?;
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

fn digest(bytes: &[u8]) -> Result<ContentDigest, String> {
    ContentDigest::parse(format!("sha256:{}", hex(&sha256(bytes))))
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

    const MODEL: &str = r#"
schema = "jails.model.v1"

[project]
id = "project_notes"
name = "Notes"
base_package = "com.example.notes"
java_release = 26
dialect = "postgresql"
"#;

    #[test]
    fn v1_lock_remains_a_one_way_upgrade_input() {
        let model = jails_model::parse_toml(MODEL).unwrap();
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
    }

    #[test]
    fn v2_lock_refuses_a_projection_that_does_not_match_its_digest() {
        let model = jails_model::parse_toml(MODEL).unwrap();
        let model_digest = digest(&model.canonical_json().unwrap()).unwrap();
        let projection = RenderedTree::new(ProjectPath::parse(MANAGED_ROOT).unwrap());
        let projection_digest = digest(&serde_json::to_vec(&projection).unwrap()).unwrap();
        let mut damaged = projection;
        damaged.root = ProjectPath::parse(".jails/not-generated").unwrap();
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
        assert!(error.contains("accepted projection"), "{error}");
    }
}
