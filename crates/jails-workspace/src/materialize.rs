use jails_contracts::{
    CanonicalModelPatch, ContentDigest, DocumentIntent, FileImageRef, FileKind, FileMode,
    ModelFileUpdate, Plan, PlanBundle, PlanDraft, PlannedOperation, ProjectPath, TreeEntry,
    TreeManifest, WorkspaceSnapshot,
};
use jails_support::codec::{hex, sha256};
use serde::Serialize;
use std::collections::BTreeMap;

const COMPILER_LOCK_SCHEMA: &str = "jails.compiler-lock.v2";

#[derive(Serialize)]
struct CompilerLock<'a> {
    schema: &'static str,
    compiler: &'a str,
    model_digest: ContentDigest,
    model: &'a jails_model::AppModel,
    projection_digest: ContentDigest,
    projection: &'a jails_contracts::RenderedTree,
}

const PLAN_SCHEMA: &str = "jails.plan.v1";
const BUNDLE_SCHEMA: &str = "jails.plan-bundle.v1";

pub fn materialize(
    snapshot: &WorkspaceSnapshot,
    input: CanonicalModelPatch,
    draft: PlanDraft,
    compiler_version: &str,
) -> Result<PlanBundle, String> {
    materialize_with_model(snapshot, input, draft, None, compiler_version)
}

pub fn materialize_with_model(
    snapshot: &WorkspaceSnapshot,
    input: CanonicalModelPatch,
    draft: PlanDraft,
    model_update: Option<ModelFileUpdate>,
    compiler_version: &str,
) -> Result<PlanBundle, String> {
    let mut blobs = BTreeMap::new();
    let before = captured_tree(snapshot, &draft.generated.root, &mut blobs)?;
    let after = crate::reconcile::tree(snapshot, &draft, &mut blobs)?;
    let managed_tree_changed = before != after;
    let before_id = if before.entries.is_empty() {
        None
    } else {
        Some(tree_id(&before)?)
    };
    let after_id = tree_id(&after)?;
    let mut trees = BTreeMap::new();
    if let Some(before_id) = &before_id {
        trees.insert(before_id.clone(), before);
    }
    trees.insert(after_id.clone(), after);

    let mut base = snapshot.preconditions.clone();
    for path in draft.generated.files.keys() {
        base.files
            .entry(path.clone())
            .or_insert(jails_contracts::FilePrecondition::Missing);
    }
    let mut operations = if managed_tree_changed {
        vec![PlannedOperation::PublishMergedTree {
            root: draft.generated.root.clone(),
            before: before_id,
            after: after_id,
        }]
    } else {
        Vec::new()
    };
    materialize_migrations(snapshot, &draft, &mut base, &mut blobs, &mut operations)?;
    crate::reader_facet::materialize(
        snapshot,
        &draft.baseline.reader_facets,
        &draft.generated.reader_facets,
        &mut blobs,
        &mut operations,
    )?;
    materialize_document_intents(
        snapshot,
        &draft.reader_document_intents,
        &mut blobs,
        &mut operations,
    )?;
    if let Some(update) = model_update {
        let before = snapshot.files.get(&update.path).map(|file| {
            let blob = digest(&file.bytes)?;
            blobs.insert(blob.clone(), file.bytes.clone());
            Ok::<_, String>(FileImageRef {
                blob,
                len: file.bytes.len() as u64,
                mode: if file.executable {
                    FileMode::Executable
                } else {
                    FileMode::Regular
                },
            })
        });
        let before = before.transpose()?;
        let after_blob = digest(&update.bytes)?;
        blobs.insert(after_blob.clone(), update.bytes.clone());
        operations.push(PlannedOperation::ReplaceModelFile {
            path: update.path,
            before,
            after: FileImageRef {
                blob: after_blob,
                len: update.bytes.len() as u64,
                mode: FileMode::Regular,
            },
        });
    }
    // The compiler lock is the acceptance/merge-base commit marker. Publish
    // it only after every reproducible artifact, append-only migration,
    // reader patch, and authoring-source update. If the process stops before
    // this operation, the old accepted projection remains BASE and the same
    // command can converge the partially published workspace without a WAL.
    materialize_compiler_lock(
        snapshot,
        &draft,
        compiler_version,
        &mut blobs,
        &mut operations,
    )?;

    let digest = plan_digest(
        compiler_version,
        &base,
        &input,
        &draft.summary,
        &operations,
        &draft.follow_up_effects,
    )?;
    let plan = Plan {
        schema: PLAN_SCHEMA.to_string(),
        id: digest.as_str().to_string(),
        compiler: compiler_version.to_string(),
        base,
        input,
        summary: draft.summary,
        operations,
        follow_up_effects: draft.follow_up_effects,
        digest,
    };
    let bundle = PlanBundle {
        schema: BUNDLE_SCHEMA.to_string(),
        plan,
        trees,
        blobs,
    };
    verify_bundle(&bundle)?;
    Ok(bundle)
}

fn materialize_migrations(
    snapshot: &WorkspaceSnapshot,
    draft: &PlanDraft,
    base: &mut jails_contracts::SnapshotPreconditions,
    blobs: &mut BTreeMap<ContentDigest, Vec<u8>>,
    operations: &mut Vec<PlannedOperation>,
) -> Result<(), String> {
    if draft.migrations.is_empty() {
        return Ok(());
    }
    let mut versions = snapshot
        .migration_history
        .records
        .iter()
        .map(|record| {
            record.version.parse::<u64>().map_err(|_| {
                format!(
                    "migration `{}` has non-integer version `{}`\n       fix: import the history before asking the canonical compiler to allocate a migration",
                    record.path, record.version
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    versions.sort_unstable();
    versions.dedup();
    if versions.len() != snapshot.migration_history.records.len() {
        return Err(
            "migration history contains duplicate versions\n       fix: repair the Flyway history before planning another migration"
                .to_string(),
        );
    }
    let first = versions.last().copied().unwrap_or(0) + 1;
    for (offset, migration) in draft.migrations.iter().enumerate() {
        let next = first + offset as u64;
        if migration.logical_name.is_empty()
            || !migration
                .logical_name
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        {
            return Err(format!(
                "compiler produced invalid migration name `{}`",
                migration.logical_name
            ));
        }
        let path = ProjectPath::parse(format!(
            "{}/V{next:03}__{}.sql",
            crate::capture::MIGRATION_ROOT,
            migration.logical_name
        ))?;
        if snapshot.files.contains_key(&path) {
            return Err(format!("allocated migration path `{path}` already exists"));
        }
        base.files
            .entry(path.clone())
            .or_insert(jails_contracts::FilePrecondition::Missing);
        let after = file_image(&migration.bytes, FileMode::Regular, blobs)?;
        operations.push(PlannedOperation::AppendMigration { path, after });
    }

    Ok(())
}

fn materialize_compiler_lock(
    snapshot: &WorkspaceSnapshot,
    draft: &PlanDraft,
    compiler_version: &str,
    blobs: &mut BTreeMap<ContentDigest, Vec<u8>>,
    operations: &mut Vec<PlannedOperation>,
) -> Result<(), String> {
    let model_bytes = draft
        .next_model
        .canonical_json()
        .map_err(|error| format!("could not encode accepted compiler model: {error}"))?;
    let model_digest = digest(&model_bytes)?;
    let projection_bytes = serde_json::to_vec(&draft.generated)
        .map_err(|error| format!("could not encode accepted compiler projection: {error}"))?;
    let projection_digest = digest(&projection_bytes)?;
    let lock_bytes = serde_json::to_vec_pretty(&CompilerLock {
        schema: COMPILER_LOCK_SCHEMA,
        compiler: compiler_version,
        model_digest,
        model: &draft.next_model,
        projection_digest,
        projection: &draft.generated,
    })
    .map_err(|error| format!("could not encode compiler lock: {error}"))?;
    let path = ProjectPath::parse(crate::capture::COMPILER_LOCK)?;
    let before = snapshot
        .files
        .get(&path)
        .map(|file| {
            file_image(
                &file.bytes,
                if file.executable {
                    FileMode::Executable
                } else {
                    FileMode::Regular
                },
                blobs,
            )
        })
        .transpose()?;
    let after = file_image(&lock_bytes, FileMode::Regular, blobs)?;
    if before.as_ref() == Some(&after) {
        return Ok(());
    }
    operations.push(PlannedOperation::ReplaceStateFile {
        path,
        before,
        after,
    });
    Ok(())
}

fn materialize_document_intents(
    snapshot: &WorkspaceSnapshot,
    intents: &[DocumentIntent],
    blobs: &mut BTreeMap<ContentDigest, Vec<u8>>,
    operations: &mut Vec<PlannedOperation>,
) -> Result<(), String> {
    let mut desired = BTreeMap::<ProjectPath, Vec<u8>>::new();
    let mut removals = BTreeMap::<ProjectPath, ProjectPath>::new();
    for intent in intents {
        match intent {
            DocumentIntent::EnsureMavenSourceRoots { roots } => {
                update_document(
                    snapshot,
                    &mut desired,
                    ProjectPath::parse("pom.xml")?,
                    |text| crate::documents::ensure_maven_source_roots(text, roots),
                )?;
            }
            DocumentIntent::EnsureGradleSourceRoot { path, source_set } => {
                let (build, kotlin) = gradle_build_file(snapshot)?;
                update_document(snapshot, &mut desired, build, |text| {
                    crate::documents::ensure_gradle_source_root(
                        text,
                        path.as_str(),
                        *source_set,
                        kotlin,
                    )
                })?;
            }
            DocumentIntent::EnsureSpringTestImport { class, package } => {
                // Every captured `@SpringBootTest`, in path order so the plan
                // is deterministic. `spring_boot_test_targets` reads the
                // snapshot rather than the disk -- the whole point of the
                // capture is that this set was observed once.
                for path in crate::documents::spring_boot_test_targets(snapshot, class) {
                    update_document(snapshot, &mut desired, path, |text| {
                        Ok(crate::documents::ensure_spring_test_import(
                            text, class, package,
                        ))
                    })?;
                }
            }
            DocumentIntent::ReconcileDependencies { dependencies } => {
                match snapshot.project.build_system {
                    jails_contracts::BuildSystem::Maven => update_document(
                        snapshot,
                        &mut desired,
                        ProjectPath::parse("pom.xml")?,
                        |text| crate::documents::reconcile_maven_dependencies(text, dependencies),
                    )?,
                    jails_contracts::BuildSystem::Gradle => {
                        let (build, kotlin) = gradle_build_file(snapshot)?;
                        update_document(snapshot, &mut desired, build, |text| {
                            crate::documents::reconcile_gradle_dependencies(
                                text,
                                dependencies,
                                kotlin,
                            )
                        })?;
                    }
                    jails_contracts::BuildSystem::Unknown => {
                        return Err(
                            "cannot reconcile dependencies without one captured Maven or Gradle build\n       fix: restore exactly one supported build file, then re-plan"
                                .to_string(),
                        );
                    }
                }
            }
            DocumentIntent::ReconcileBuildFeatures { features } => match snapshot
                .project
                .build_system
            {
                jails_contracts::BuildSystem::Maven => update_document(
                    snapshot,
                    &mut desired,
                    ProjectPath::parse("pom.xml")?,
                    |text| {
                        crate::documents::reconcile_maven_build_features(
                            text,
                            features,
                            snapshot.project.spring_boot.is_some(),
                        )
                    },
                )?,
                jails_contracts::BuildSystem::Gradle => {
                    let (build, kotlin) = gradle_build_file(snapshot)?;
                    update_document(snapshot, &mut desired, build, |text| {
                        crate::documents::reconcile_gradle_build_features(text, features, kotlin)
                    })?;
                }
                jails_contracts::BuildSystem::Unknown => {
                    if !features.is_empty() {
                        return Err(
                            "cannot reconcile build features without one captured Maven or Gradle build\n       fix: restore exactly one supported build file, then re-plan"
                                .to_string(),
                        );
                    }
                }
            },
            DocumentIntent::ReconcileProperties {
                path,
                previous,
                desired: properties,
            } => {
                update_optional_document(snapshot, &mut desired, path.clone(), |text| {
                    crate::documents::reconcile_properties(text, previous, properties)
                })?;
            }
            DocumentIntent::EjectFile { path, bytes, .. } => {
                if snapshot.files.contains_key(path) {
                    return Err(format!(
                        "reader source `{path}` already exists\n       fix: move or remove the destination before ejecting"
                    ));
                }
                if desired.insert(path.clone(), bytes.clone()).is_some() {
                    return Err(format!("two ejections target reader source `{path}`"));
                }
            }
            DocumentIntent::AdoptJava { source, path, .. } => {
                if desired.contains_key(source) {
                    return Err(format!(
                        "reader source `{source}` cannot be patched and adopted in one plan"
                    ));
                }
                if removals.insert(source.clone(), path.clone()).is_some() {
                    return Err(format!(
                        "reader source `{source}` is adopted more than once"
                    ));
                }
            }
        }
    }

    for (path, after_bytes) in desired {
        let before_file = snapshot.files.get(&path);
        if before_file.is_some_and(|file| file.bytes == after_bytes)
            || before_file.is_none() && after_bytes.is_empty()
        {
            continue;
        }
        let mode = before_file.map_or(FileMode::Regular, |file| {
            if file.executable {
                FileMode::Executable
            } else {
                FileMode::Regular
            }
        });
        let before = before_file
            .map(|file| file_image(&file.bytes, mode, blobs))
            .transpose()?;
        let after = file_image(&after_bytes, mode, blobs)?;
        operations.push(PlannedOperation::PatchReaderFile {
            path,
            before,
            after,
        });
    }
    for (source, target) in removals {
        let before_file = snapshot.files.get(&source).ok_or_else(|| {
            format!(
                "reader source `{source}` was not captured for adoption into `{target}`\n       fix: restore the source and re-plan the import"
            )
        })?;
        let before = file_image(
            &before_file.bytes,
            if before_file.executable {
                FileMode::Executable
            } else {
                FileMode::Regular
            },
            blobs,
        )?;
        operations.push(PlannedOperation::RemoveReaderFile {
            path: source,
            before,
        });
    }
    Ok(())
}

fn update_optional_document(
    snapshot: &WorkspaceSnapshot,
    desired: &mut BTreeMap<ProjectPath, Vec<u8>>,
    path: ProjectPath,
    update: impl FnOnce(&str) -> Result<String, String>,
) -> Result<(), String> {
    let current = desired
        .get(&path)
        .map(Vec::as_slice)
        .or_else(|| snapshot.files.get(&path).map(|file| file.bytes.as_slice()))
        .unwrap_or_default();
    let text = std::str::from_utf8(current).map_err(|_| {
        format!(
            "reader document `{path}` is not UTF-8\n       fix: convert it to UTF-8, then re-plan"
        )
    })?;
    desired.insert(path, update(text)?.into_bytes());
    Ok(())
}

fn update_document(
    snapshot: &WorkspaceSnapshot,
    desired: &mut BTreeMap<ProjectPath, Vec<u8>>,
    path: ProjectPath,
    update: impl FnOnce(&str) -> Result<String, String>,
) -> Result<(), String> {
    let captured = snapshot.files.get(&path).ok_or_else(|| {
        format!(
            "reader document `{path}` was not captured\n       fix: restore the build file and re-plan"
        )
    })?;
    let current = desired
        .get(&path)
        .map(Vec::as_slice)
        .unwrap_or(&captured.bytes);
    let text = std::str::from_utf8(current).map_err(|_| {
        format!(
            "reader document `{path}` is not UTF-8\n       fix: convert it to UTF-8, then re-plan"
        )
    })?;
    desired.insert(path, update(text)?.into_bytes());
    Ok(())
}

fn gradle_build_file(snapshot: &WorkspaceSnapshot) -> Result<(ProjectPath, bool), String> {
    let groovy = ProjectPath::parse("build.gradle")?;
    let kotlin = ProjectPath::parse("build.gradle.kts")?;
    match (
        snapshot.files.contains_key(&groovy),
        snapshot.files.contains_key(&kotlin),
    ) {
        (true, false) => Ok((groovy, false)),
        (false, true) => Ok((kotlin, true)),
        (true, true) => Err(
            "both build.gradle and build.gradle.kts exist\n       fix: keep one canonical Gradle build script, then re-plan"
                .to_string(),
        ),
        (false, false) => Err(
            "captured Gradle project has no build script\n       fix: restore build.gradle or build.gradle.kts, then re-plan"
                .to_string(),
        ),
    }
}

pub(crate) fn file_image(
    bytes: &[u8],
    mode: FileMode,
    blobs: &mut BTreeMap<ContentDigest, Vec<u8>>,
) -> Result<FileImageRef, String> {
    let blob = digest(bytes)?;
    blobs.insert(blob.clone(), bytes.to_vec());
    Ok(FileImageRef {
        blob,
        len: bytes.len() as u64,
        mode,
    })
}

pub fn verify_bundle(bundle: &PlanBundle) -> Result<(), String> {
    if bundle.schema != BUNDLE_SCHEMA || bundle.plan.schema != PLAN_SCHEMA {
        return Err("unsupported exact-plan schema".to_string());
    }
    for (id, bytes) in &bundle.blobs {
        if &digest(bytes)? != id {
            return Err(format!("blob `{id:?}` does not match its content"));
        }
    }
    for (id, tree) in &bundle.trees {
        for entry in tree.entries.values() {
            if !bundle.blobs.contains_key(&entry.blob) {
                return Err(format!("tree `{id:?}` references a missing blob"));
            }
        }
        if &tree_id(tree)? != id {
            return Err(format!("tree `{id:?}` does not match its manifest"));
        }
    }
    for operation in &bundle.plan.operations {
        match operation {
            PlannedOperation::PublishMergedTree { before, after, .. } => {
                if before
                    .as_ref()
                    .is_some_and(|tree| !bundle.trees.contains_key(tree))
                    || !bundle.trees.contains_key(after)
                {
                    return Err("managed-tree operation references a missing tree".to_string());
                }
            }
            PlannedOperation::ReplaceModelFile { before, after, .. } => {
                if let Some(before) = before {
                    verify_image(bundle, before)?;
                }
                verify_image(bundle, after)?;
            }
            PlannedOperation::PatchReaderFile { before, after, .. } => {
                if let Some(before) = before {
                    verify_image(bundle, before)?;
                }
                verify_image(bundle, after)?;
            }
            PlannedOperation::RemoveReaderFile { before, .. } => verify_image(bundle, before)?,
            PlannedOperation::ReplaceStateFile { before, after, .. } => {
                if let Some(before) = before {
                    verify_image(bundle, before)?;
                }
                verify_image(bundle, after)?;
            }
            PlannedOperation::AppendMigration { after, .. } => verify_image(bundle, after)?,
        }
    }
    let actual = plan_digest(
        &bundle.plan.compiler,
        &bundle.plan.base,
        &bundle.plan.input,
        &bundle.plan.summary,
        &bundle.plan.operations,
        &bundle.plan.follow_up_effects,
    )?;
    if actual != bundle.plan.digest || bundle.plan.id != actual.as_str() {
        return Err("plan digest does not match the exact plan".to_string());
    }
    Ok(())
}

fn verify_image(bundle: &PlanBundle, image: &FileImageRef) -> Result<(), String> {
    let bytes = bundle.blobs.get(&image.blob).ok_or_else(|| {
        format!(
            "file image references missing blob `{}`",
            image.blob.as_str()
        )
    })?;
    if bytes.len() as u64 != image.len {
        return Err(format!(
            "file image `{}` has the wrong length",
            image.blob.as_str()
        ));
    }
    Ok(())
}

fn captured_tree(
    snapshot: &WorkspaceSnapshot,
    root: &ProjectPath,
    blobs: &mut BTreeMap<ContentDigest, Vec<u8>>,
) -> Result<TreeManifest, String> {
    let mut tree = TreeManifest::default();
    for (path, file) in &snapshot.files {
        if !path.is_within(root) {
            continue;
        }
        let blob = digest(&file.bytes)?;
        blobs.insert(blob.clone(), file.bytes.clone());
        tree.entries.insert(
            path.clone(),
            TreeEntry {
                kind: file_kind(path),
                mode: if file.executable {
                    FileMode::Executable
                } else {
                    FileMode::Regular
                },
                blob,
            },
        );
    }
    Ok(tree)
}

pub(crate) fn file_kind(path: &ProjectPath) -> FileKind {
    if path.as_str().ends_with(".java") && path.as_str().contains("/test/") {
        FileKind::JavaTest
    } else if path.as_str().ends_with(".java") {
        FileKind::JavaMain
    } else if path.as_str().ends_with(".http") {
        FileKind::HttpCollection
    } else {
        FileKind::Resource
    }
}

fn tree_id(tree: &TreeManifest) -> Result<ContentDigest, String> {
    canonical_digest("tree", tree)
}

fn plan_digest(
    compiler: &str,
    base: &jails_contracts::SnapshotPreconditions,
    input: &CanonicalModelPatch,
    summary: &jails_contracts::SemanticPlan,
    operations: &[PlannedOperation],
    effects: &[jails_contracts::EffectIntent],
) -> Result<ContentDigest, String> {
    canonical_digest(
        "plan",
        &(compiler, base, input, summary, operations, effects),
    )
}

fn canonical_digest(label: &str, value: &impl Serialize) -> Result<ContentDigest, String> {
    let mut bytes = format!("JAILS-{}-1\0", label.to_ascii_uppercase()).into_bytes();
    bytes.extend(
        serde_json::to_vec(value)
            .map_err(|error| format!("could not encode {label} identity: {error}"))?,
    );
    digest(&bytes)
}

pub(crate) fn digest(bytes: &[u8]) -> Result<ContentDigest, String> {
    ContentDigest::parse(format!("sha256:{}", hex(&sha256(bytes))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use jails_compiler::Compiler;

    const MODEL: &str = r#"
schema = "jails.model.v1"

[project]
id = "project_notes"
name = "Notes"
base_package = "com.example.notes"
java_release = 26
dialect = "postgresql"

[entities.note]
id = "ent_note"
facets = ["record"]

[entities.note.fields.id]
id = "fld_note_id"
type = "uuid"
primary_key = true
"#;

    #[test]
    fn materialization_is_exact_and_self_verifying() {
        let model = jails_model::parse_toml(MODEL).unwrap();
        let snapshot = WorkspaceSnapshot::detached(model);
        let draft = Compiler::compile(&snapshot, None).unwrap();
        let bundle = materialize(
            &snapshot,
            CanonicalModelPatch::reconcile(),
            draft,
            jails_compiler::COMPILER_VERSION,
        )
        .unwrap();
        assert_eq!(bundle.plan.operations.len(), 2);
        assert!(matches!(
            bundle.plan.operations.last(),
            Some(PlannedOperation::ReplaceStateFile { path, .. })
                if path.as_str() == crate::capture::COMPILER_LOCK
        ));
        verify_bundle(&bundle).unwrap();

        let mut damaged = bundle;
        let bytes = damaged.blobs.values_mut().next().unwrap();
        bytes.push(b'x');
        assert!(verify_bundle(&damaged).is_err());
    }

    #[test]
    fn an_absent_empty_managed_tree_is_already_converged() {
        let source = MODEL.split("\n[entities.note]").next().unwrap();
        let model = jails_model::parse_toml(source).unwrap();
        let snapshot = WorkspaceSnapshot::detached(model);
        let draft = Compiler::compile(&snapshot, None).unwrap();
        let bundle = materialize(
            &snapshot,
            CanonicalModelPatch::reconcile(),
            draft,
            jails_compiler::COMPILER_VERSION,
        )
        .unwrap();
        assert!(matches!(
            bundle.plan.operations.as_slice(),
            [PlannedOperation::ReplaceStateFile { .. }]
        ));
        verify_bundle(&bundle).unwrap();
    }

    #[test]
    fn a_partially_published_tree_converges_before_lock_acceptance() {
        let model = jails_model::parse_toml(MODEL).unwrap();
        let snapshot = WorkspaceSnapshot::detached(model);
        let draft = Compiler::compile(&snapshot, None).unwrap();
        let bundle = materialize(
            &snapshot,
            CanonicalModelPatch::reconcile(),
            draft,
            jails_compiler::COMPILER_VERSION,
        )
        .unwrap();
        let tree = bundle
            .plan
            .operations
            .iter()
            .find_map(|operation| match operation {
                PlannedOperation::PublishMergedTree { after, .. } => bundle.trees.get(after),
                _ => None,
            })
            .unwrap();
        let (path, entry) = tree.entries.iter().next().unwrap();
        let root = tempfile::tempdir().unwrap();
        let partial = root.path().join(path.as_str());
        std::fs::create_dir_all(partial.parent().unwrap()).unwrap();
        std::fs::write(&partial, bundle.blobs.get(&entry.blob).unwrap()).unwrap();
        assert!(!root.path().join(crate::capture::COMPILER_LOCK).exists());

        crate::execute(root.path(), &bundle).unwrap();

        assert!(root.path().join(crate::capture::COMPILER_LOCK).is_file());
        for (path, entry) in &tree.entries {
            assert_eq!(
                std::fs::read(root.path().join(path.as_str())).unwrap(),
                *bundle.blobs.get(&entry.blob).unwrap()
            );
        }
    }
}
