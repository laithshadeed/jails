//! `.jails/compiler.lock.json`: what the last applied plan accepted.
//!
//! **The reading half, and `materialize::lock` is the writing one.** What a
//! lock is made of is one subject -- five schema versions, the model and the
//! projection each sealed by its own digest, the published migrations -- and
//! it is not the subject of the module that captures a project's external
//! facts. Splitting it is what keeps either readable.
//!
//! **Every version still decodes.** A lock is a file in somebody's
//! repository, so a release that could not read the one before it would ask
//! every project to regenerate before it could do anything; the arms below
//! are that promise, and the digest each version is checked under is the same
//! one -- a digest of the form `serde` derives from `RenderedTree`,
//! recomputed from whatever the file held.
//!
//! **v5 holds the merge base beside the lock rather than inside it.** Every
//! managed file's accepted bytes used to sit in this JSON, which made it 1.38x
//! the source tree it described and one opaque blob to git. They are files
//! under `.jails/base` now: byte-identical to what they describe, so git
//! stores no new object for them, a base diff is per file, and a merge is a
//! merge.

use super::{COMPILER_LOCK, CapturedFile, capture_optional_file, digest, project_path};
use jails_contracts::{ContentDigest, ProjectPath, RenderedTree};
use jails_model::{AppModel, Diagnostic};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;

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
        format!("compiler lock `{COMPILER_LOCK}` does not match the generated tree it accepted"),
        "restore a known-good lock; do not infer merge bases from generated source",
    )
}
const COMPILER_LOCK_SCHEMA_V1: &str = "jails.compiler-lock.v1";
const COMPILER_LOCK_SCHEMA_V2: &str = "jails.compiler-lock.v2";
const COMPILER_LOCK_SCHEMA_V3: &str = "jails.compiler-lock.v3";
/// v3's fields with the projection's bytes stored as text.
const COMPILER_LOCK_SCHEMA_V4: &str = "jails.compiler-lock.v4";
/// v4 with the merge base moved out of the lock and into a tree of files.
const COMPILER_LOCK_SCHEMA_V5: &str = "jails.compiler-lock.v5";

/// Where the merge base lives, one file per managed path.
pub const BASE_ROOT: &str = ".jails/base";

/// v4's fields with `projection` replaced by a manifest of what is under
/// [`BASE_ROOT`].
#[derive(Debug, Deserialize)]
struct CompilerLockV5 {
    compiler: String,
    model_digest: ContentDigest,
    model: AppModel,
    projection_digest: ContentDigest,
    base: BaseManifest,
    #[serde(default)]
    migrations: BTreeMap<ProjectPath, ContentDigest>,
    #[serde(default, deserialize_with = "migration_bytes")]
    migration_bytes: BTreeMap<ProjectPath, Vec<u8>>,
}

#[derive(Debug, Deserialize)]
struct BaseManifest {
    files: BTreeMap<ProjectPath, BaseEntry>,
    #[serde(default)]
    reader_facets: BTreeMap<String, jails_contracts::RenderedReaderFacet>,
}

#[derive(Debug, Deserialize)]
struct BaseEntry {
    kind: jails_contracts::FileKind,
    mode: jails_contracts::FileMode,
    provenance: jails_contracts::Provenance,
    digest: ContentDigest,
}

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
    #[serde(default, deserialize_with = "migration_bytes")]
    migration_bytes: BTreeMap<ProjectPath, Vec<u8>>,
}

/// The migrations map, whichever shape the lock stores its values in.
///
/// The same reason as [`jails_contracts::bytes_field`]: the values are file
/// bytes and the lock writes them as text.
fn migration_bytes<'de, D>(deserializer: D) -> Result<BTreeMap<ProjectPath, Vec<u8>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(serde::Deserialize)]
    struct Entry(#[serde(deserialize_with = "jails_contracts::bytes_field::deserialize")] Vec<u8>);

    let raw = BTreeMap::<ProjectPath, Entry>::deserialize(deserializer)?;
    Ok(raw
        .into_iter()
        .map(|(path, Entry(bytes))| (path, bytes))
        .collect())
}

#[derive(Debug)]
pub(super) struct AcceptedCompilerState {
    pub(super) model: AppModel,
    pub(super) projection: Option<RenderedTree>,
    pub(super) compiler: Option<String>,
    pub(super) migrations: BTreeMap<ProjectPath, ContentDigest>,
    pub(super) migration_bytes: BTreeMap<ProjectPath, Vec<u8>>,
}

pub(super) fn accepted_compiler_state(
    root: &Path,
    files: &mut BTreeMap<ProjectPath, CapturedFile>,
    preconditions: &mut jails_contracts::SnapshotPreconditions,
) -> Result<Option<AcceptedCompilerState>, Diagnostic> {
    let Some(lock) = files.get(&project_path(COMPILER_LOCK)?) else {
        return Ok(None);
    };
    let bytes = lock.bytes.clone();
    match decode_compiler_lock(&bytes)? {
        Decoded::Whole(state) => Ok(Some(state)),
        // **The base is beside the lock, not inside it.** Every path the
        // manifest names is observed the way every other managed path is --
        // present with its bytes or missing -- so the merge base is part of
        // the plan's preconditions and a base file edited between plan and
        // apply is a stale plan rather than a silently different merge.
        Decoded::WithBase(lock) => {
            let mut projection = RenderedTree {
                files: BTreeMap::new(),
                reader_facets: lock.base.reader_facets,
            };
            for (path, entry) in lock.base.files {
                let stored = base_path(&path)?;
                capture_optional_file(root, stored.as_str(), files, preconditions)?;
                let Some(captured) = files.get(&stored) else {
                    return Err(base_file_missing(&stored));
                };
                if digest(&captured.bytes)? != entry.digest {
                    return Err(base_file_changed(&stored));
                }
                projection.files.insert(
                    path,
                    jails_contracts::RenderedFile {
                        kind: entry.kind,
                        mode: entry.mode,
                        bytes: captured.bytes.clone(),
                        provenance: entry.provenance,
                    },
                );
            }
            let encoded = serde_json::to_vec(&projection).map_err(lock_unverifiable)?;
            if digest(&encoded)? != lock.projection_digest {
                return Err(lock_projection_mismatch());
            }
            Ok(Some(AcceptedCompilerState {
                model: lock.model,
                projection: Some(projection),
                compiler: Some(lock.compiler),
                migrations: lock.migrations,
                migration_bytes: lock.migration_bytes,
            }))
        }
    }
}

/// `src/main/java/X.java` under the base root.
pub fn base_path(path: &ProjectPath) -> Result<ProjectPath, Diagnostic> {
    project_path(format!("{BASE_ROOT}/{path}"))
}

fn base_file_missing(path: &ProjectPath) -> Diagnostic {
    Diagnostic::new(
        "workspace-merge-base-incomplete",
        path.to_string(),
        format!("`{COMPILER_LOCK}` names `{path}` as part of the merge base and it is not there"),
        "restore it from version control, or delete the lock and `jails sync` to accept the tree as it stands",
    )
}

fn base_file_changed(path: &ProjectPath) -> Diagnostic {
    Diagnostic::new(
        "workspace-merge-base-edited",
        path.to_string(),
        format!("`{path}` is not the merge base `{COMPILER_LOCK}` accepted"),
        "restore it from version control; the merge base is what the next generation diffs against, and editing it changes what a merge means",
    )
}

/// What a lock decoded to: everything, or everything but the bytes.
#[derive(Debug)]
enum Decoded {
    Whole(AcceptedCompilerState),
    WithBase(CompilerLockV5),
}

fn decode_compiler_lock(bytes: &[u8]) -> Result<Decoded, Diagnostic> {
    let header: serde_json::Value = serde_json::from_slice(bytes).map_err(lock_undecodable)?;
    // **Either shape decodes straight into the type.** A v4 lock stores a
    // generated file's bytes as text and every earlier one stores an array of
    // integers; `jails_contracts::bytes_field` reads both, so neither is
    // rewritten into the other on the way in. What the verification below
    // digests is still the one form `serde` derives, so a lock from any
    // release is checked under one rule.
    let schema = header
        .get("schema")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    match schema {
        COMPILER_LOCK_SCHEMA_V1 => {
            let lock: CompilerLockV1 = serde_json::from_value(header).map_err(lock_undecodable)?;
            verify_model(&lock.model, &lock.model_digest)?;
            Ok(Decoded::Whole(AcceptedCompilerState {
                model: lock.model,
                projection: None,
                compiler: None,
                migrations: BTreeMap::new(),
                migration_bytes: BTreeMap::new(),
            }))
        }
        COMPILER_LOCK_SCHEMA_V2 => {
            let lock: CompilerLockV2 = serde_json::from_value(header).map_err(lock_undecodable)?;
            verify_model(&lock.model, &lock.model_digest)?;
            let projection = serde_json::to_vec(&lock.projection).map_err(lock_unverifiable)?;
            if digest(&projection)? != lock.projection_digest {
                return Err(lock_projection_mismatch());
            }
            Ok(Decoded::Whole(AcceptedCompilerState {
                model: lock.model,
                projection: Some(lock.projection),
                compiler: Some(lock.compiler),
                migrations: BTreeMap::new(),
                migration_bytes: BTreeMap::new(),
            }))
        }
        COMPILER_LOCK_SCHEMA_V3 | COMPILER_LOCK_SCHEMA_V4 => {
            let lock: CompilerLockV3 = serde_json::from_value(header).map_err(lock_undecodable)?;
            verify_model(&lock.model, &lock.model_digest)?;
            let projection = serde_json::to_vec(&lock.projection).map_err(lock_unverifiable)?;
            if digest(&projection)? != lock.projection_digest {
                return Err(lock_projection_mismatch());
            }
            Ok(Decoded::Whole(AcceptedCompilerState {
                model: lock.model,
                projection: Some(lock.projection),
                compiler: Some(lock.compiler),
                migrations: lock.migrations,
                migration_bytes: lock.migration_bytes,
            }))
        }
        COMPILER_LOCK_SCHEMA_V5 => {
            let lock: CompilerLockV5 = serde_json::from_value(header).map_err(lock_undecodable)?;
            verify_model(&lock.model, &lock.model_digest)?;
            Ok(Decoded::WithBase(lock))
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::BTreeSet;

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

        let Decoded::Whole(accepted) = decode_compiler_lock(&bytes).unwrap() else {
            panic!("a v1 lock decodes whole: it has no base tree beside it");
        };
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
        assert!(
            error
                .to_string()
                .contains("does not match the generated tree"),
            "{error}"
        );
    }
}
