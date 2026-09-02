//! Where semantic desire becomes an exact, reviewable transition.
//!
//! **The single boundary between the compiler and the filesystem**: a
//! `PlanDraft` says
//! what should be true, a `WorkspaceSnapshot` says what is, and the `PlanBundle`
//! that comes out of here is the exact difference — preconditions, typed
//! operations, content-addressed trees and blobs, and one digest that preview,
//! export, confirmation and apply all refer to. Apply never replans, so
//! anything the executor needs must be decided here.
//!
//! Three properties are load-bearing and each has a test rather than a
//! comment:
//!
//! - **Equal snapshot, patch and compiler version give an equal digest.**
//!   Every other guarantee is downstream of it — a reviewed digest and an
//!   applied digest would otherwise be related only by hope. The digest covers
//!   the compiler version deliberately, so a compiler change is a different
//!   plan rather than the same plan meaning something else.
//! - **The bundle verifies itself.** `verify_bundle` recomputes every digest
//!   before the executor is allowed to touch anything, so a truncated or
//!   edited plan file fails at the boundary instead of halfway through a tree.
//! - **The persisted shape is goldened**, both the compiler lock and the plan
//!   itself. Byte-compared, because a round-trip goes through whatever the
//!   serializer currently does and can never notice that it moved.
//!
//! Regeneration is a merge, not an overwrite: the accepted projection in the
//! lock is BASE, the live tree is OURS, this draft is THEIRS. A clean merge is
//! frozen into the plan and the lock advances to THEIRS, so hand edits stay
//! deltas; a conflict refuses before any write.

use jails_contracts::{
    CanonicalModelPatch, ContentDigest, DocumentIntent, FileImageRef, FileKind, FileMode,
    ModelFileUpdate, Plan, PlanBundle, PlanDraft, PlannedOperation, ProjectPath, TreeEntry,
    TreeManifest, WorkspaceSnapshot,
};
use jails_support::codec::{hex, sha256};
use serde::Serialize;
use std::collections::BTreeMap;

mod authoring_source;

use authoring_source::publish_authoring_source;

const COMPILER_LOCK_SCHEMA: &str = "jails.compiler-lock.v3";

#[derive(Serialize)]
struct CompilerLock<'a> {
    schema: &'static str,
    compiler: &'a str,
    model_digest: ContentDigest,
    model: &'a jails_model::AppModel,
    projection_digest: ContentDigest,
    projection: &'a jails_contracts::RenderedTree,
    /// Every migration published so far, sealed by content.
    ///
    /// Carried forward rather than recomputed: the point is to record what was
    /// published, so a later capture reading a different file on disk is the
    /// finding rather than the new truth.
    migrations: BTreeMap<ProjectPath, ContentDigest>,
    /// The bytes behind those digests, so an edited one can be put back.
    ///
    /// **A digest can only say a migration changed.** The compiler derives a
    /// migration from a model *diff*, and the diff that produced a published
    /// one is history -- so regenerating cannot recover it and nothing else
    /// holds it. Flyway refuses on the checksum until the file matches what
    /// ran, which makes this the one restore that has no alternative.
    migration_bytes: BTreeMap<ProjectPath, Vec<u8>>,
}

pub(crate) const PLAN_SCHEMA: &str = "jails.plan.v1";
pub(crate) const BUNDLE_SCHEMA: &str = "jails.plan-bundle.v1";

/// What materialization does about a managed file the reader deleted.
///
/// **The desired tree is the same under both**, which is what makes this a
/// materialization policy rather than a compiler input: repair does not render
/// anything the model does not already imply, it waives one guard about how the
/// live tree got into its current state. Keeping it off `PlanDraft` keeps the
/// compiler's purity contract exactly where it was.
///
/// The guard is [`Restore::Refuse`] everywhere but `jails resource repair`,
/// because a managed file that vanished is usually the reader saying something
/// -- a half-finished `git checkout`, a deletion meant as "stop generating
/// this" -- and silently writing it back answers a question nobody asked.
/// `jails resource repair` is the one command that *can* answer it: a fix
/// line saying to restore the file by hand is advice a reader who deleted it
/// cannot take.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Restore {
    /// Refuse, naming the file. Every ordinary plan.
    Refuse,
    /// Write the desired bytes back. `jails resource repair` only.
    Deleted,
    /// Delete a managed file the reader edited, because they said to.
    ///
    /// **The other half of the same guard.** A file the compiler no longer
    /// renders and the reader has edited is a refusal for the same reason a
    /// deleted one is: jails does not throw away bytes it did not write.
    /// `remove --force` and `destroy --force` are the reader saying the edits
    /// can go; without this the refusal's fix line ("move the custom code to
    /// reader source") would be the only route out of a capability they had
    /// touched.
    ///
    /// It never widens beyond deletion: a *conflicting* merge still refuses,
    /// because that is two sets of edits to reconcile rather than one set to
    /// discard.
    EditedAndRemoved,
}

pub fn materialize(
    snapshot: &WorkspaceSnapshot,
    input: CanonicalModelPatch,
    draft: PlanDraft,
    compiler_version: &str,
    restore: Restore,
) -> Result<PlanBundle, String> {
    materialize_with_model(snapshot, input, draft, None, compiler_version, restore)
}

pub fn materialize_with_model(
    snapshot: &WorkspaceSnapshot,
    input: CanonicalModelPatch,
    draft: PlanDraft,
    model_update: Option<ModelFileUpdate>,
    compiler_version: &str,
    restore: Restore,
) -> Result<PlanBundle, String> {
    let mut blobs = BTreeMap::new();
    let before = captured_tree(snapshot, &draft.generated.root, &mut blobs)?;
    let after = crate::reconcile::tree(snapshot, &draft, &mut blobs, restore)?;
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
    // **Only a migration jails authored whole is sealed.** A derived one
    // names the declaration it came from; `g migration` writes a comment for
    // the reader to fill in and names nothing, and sealing that would report a
    // fault the moment the reader did what the file asks.
    let mut sealed_migrations = snapshot.accepted_migrations.clone();
    let mut sealed_bytes = snapshot.accepted_migration_bytes.clone();
    materialize_migrations(
        snapshot,
        &draft,
        &mut base,
        &mut blobs,
        &mut operations,
        &mut sealed_migrations,
        &mut sealed_bytes,
    )?;
    // **Restoring a sealed migration is `resource repair`'s job and no other
    // plan's.** An ordinary command that found one edited would be rewriting a
    // file the reader touched as a side effect of something else; repair is
    // the command that exists to say "put back what was published".
    if restore == Restore::Deleted {
        restore_sealed_migrations(
            snapshot,
            &sealed_bytes,
            &mut base,
            &mut blobs,
            &mut operations,
        )?;
    }
    crate::reader_facet::materialize(
        snapshot,
        &draft.baseline.reader_facets,
        &draft.generated.reader_facets,
        &mut blobs,
        &mut operations,
        restore,
    )?;
    materialize_document_intents(
        snapshot,
        &draft.reader_document_intents,
        &mut blobs,
        &mut operations,
    )?;
    publish_authoring_source(snapshot, model_update, &mut blobs, &mut operations)?;
    // The compiler lock is the acceptance/merge-base commit marker. Publish
    // it only after every reproducible artifact, append-only migration,
    // reader patch, and authoring-source update. If the process stops before
    // this operation, the old accepted projection remains BASE and the same
    // command can converge the partially published workspace without a WAL.
    materialize_compiler_lock(
        snapshot,
        &draft,
        compiler_version,
        sealed_migrations,
        sealed_bytes,
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
    crate::verify::verify_bundle(&bundle)?;
    Ok(bundle)
}

fn materialize_migrations(
    snapshot: &WorkspaceSnapshot,
    draft: &PlanDraft,
    base: &mut jails_contracts::SnapshotPreconditions,
    blobs: &mut BTreeMap<ContentDigest, Vec<u8>>,
    operations: &mut Vec<PlannedOperation>,
    sealed: &mut BTreeMap<ProjectPath, ContentDigest>,
    sealed_bytes: &mut BTreeMap<ProjectPath, Vec<u8>>,
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
        if !migration.semantic_ids.is_empty() {
            sealed.insert(path.clone(), after.blob.clone());
            sealed_bytes.insert(path.clone(), migration.bytes.clone());
        }
        operations.push(PlannedOperation::AppendMigration { path, after });
    }

    Ok(())
}

fn materialize_compiler_lock(
    snapshot: &WorkspaceSnapshot,
    draft: &PlanDraft,
    compiler_version: &str,
    migrations: BTreeMap<ProjectPath, ContentDigest>,
    migration_bytes: BTreeMap<ProjectPath, Vec<u8>>,
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
        migrations,
        migration_bytes,
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
            DocumentIntent::ReconcileSpringTestImport {
                class,
                package,
                wanted,
            } => {
                // Every captured `@SpringBootTest` that disagrees with the
                // model, in path order so the plan is deterministic.
                // `spring_boot_test_targets` reads the snapshot rather than
                // the disk -- the whole point of the capture is that this set
                // was observed once.
                for path in crate::documents::spring_boot_test_targets(snapshot, class, *wanted) {
                    update_document(snapshot, &mut desired, path, |text| {
                        Ok(match wanted {
                            true => {
                                crate::documents::ensure_spring_test_import(text, class, package)
                            }
                            false => {
                                crate::documents::remove_spring_test_import(text, class, package)
                            }
                        })
                    })?;
                }
            }
            DocumentIntent::EnsureCommandRegistration { class, package } => {
                // Silence is the right answer when there is no dispatcher, or
                // more than one: the generated command's Javadoc carries the
                // line to paste, and splicing into a file jails cannot
                // uniquely identify is worse than saying nothing.
                if let Some(path) = crate::documents::command_dispatcher(snapshot) {
                    update_document(snapshot, &mut desired, path, |text| {
                        Ok(crate::documents::ensure_command_registration(
                            text, class, package,
                        ))
                    })?;
                }
            }
            DocumentIntent::SetMavenMainClass { class } => {
                update_document(
                    snapshot,
                    &mut desired,
                    ProjectPath::parse("pom.xml")?,
                    |text| Ok(crate::documents::set_maven_main_class(text, class)),
                )?;
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
        plan_reader_document(snapshot, operations, blobs, path, after_bytes)?;
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

/// One reader document's transition, as the operation that expresses it.
///
/// **Three outcomes, and the third is the one that has to be written down.**
/// A document that already agrees is not an operation at all; one that differs
/// is a patch; one that reconciles to *nothing* is a removal, because `remove`
/// has to reach a file's existence and not only its contents. A project that
/// never had `src/main/resources/application.properties` is not left holding
/// an empty one after the capability that created it goes, and a `compose.yaml`
/// whose last service was retired is not left holding a bare `services:`.
///
/// Anything the reader put in the file -- a bare comment included -- survives
/// reconciliation and keeps it, so emptiness is the proof that nothing but
/// jails' own content was ever there.
///
/// Shared by the document adapters and the reader facets because it is one
/// rule, and two copies of a rule go out of step.
pub(crate) fn plan_reader_document(
    snapshot: &WorkspaceSnapshot,
    operations: &mut Vec<PlannedOperation>,
    blobs: &mut BTreeMap<ContentDigest, Vec<u8>>,
    path: ProjectPath,
    after_bytes: Vec<u8>,
) -> Result<(), String> {
    let before_file = snapshot.files.get(&path);
    if before_file.is_some_and(|file| file.bytes == after_bytes)
        || before_file.is_none() && after_bytes.is_empty()
    {
        return Ok(());
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
    if let Some(before) = before.clone()
        && after_bytes.iter().all(u8::is_ascii_whitespace)
    {
        operations.push(PlannedOperation::RemoveReaderFile { path, before });
        return Ok(());
    }
    let after = file_image(&after_bytes, mode, blobs)?;
    operations.push(PlannedOperation::PatchReaderFile {
        path,
        before,
        after,
    });
    Ok(())
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

pub(crate) fn tree_id(tree: &TreeManifest) -> Result<ContentDigest, String> {
    canonical_digest("tree", tree)
}

pub(crate) fn plan_digest(
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

/// The content address of some bytes, in the one spelling the plan uses.
///
/// Public because a reader outside this crate has to be able to ask whether a
/// file still matches a digest the lock recorded, and re-deriving "sha256:" +
/// hex somewhere else is how two answers to one question start.
pub fn digest(bytes: &[u8]) -> Result<ContentDigest, String> {
    ContentDigest::parse(format!("sha256:{}", hex(&sha256(bytes))))
}

/// Put back a published migration the reader edited.
///
/// **Only `resource repair` asks for this**, and it is the one restore the
/// compiler cannot derive: a migration comes from a model *diff*, and the diff
/// that produced a published one is history. Flyway refuses on the checksum
/// until the file matches what ran, so a project whose migration was edited
/// after being applied has no other way back.
///
/// A migration the lock records no bytes for -- written by an older jails --
/// is left alone rather than guessed at.
fn restore_sealed_migrations(
    snapshot: &WorkspaceSnapshot,
    sealed: &BTreeMap<ProjectPath, Vec<u8>>,
    base: &mut jails_contracts::SnapshotPreconditions,
    blobs: &mut BTreeMap<ContentDigest, Vec<u8>>,
    operations: &mut Vec<PlannedOperation>,
) -> Result<(), String> {
    for (path, bytes) in sealed {
        let live = snapshot.files.get(path);
        if live.is_some_and(|file| file.bytes == *bytes) {
            continue;
        }
        let before = live
            .map(|file| file_image(&file.bytes, FileMode::Regular, blobs))
            .transpose()?;
        base.files.entry(path.clone()).or_insert(match &before {
            Some(image) => jails_contracts::FilePrecondition::Present {
                digest: image.blob.clone(),
                executable: image.mode == FileMode::Executable,
            },
            None => jails_contracts::FilePrecondition::Missing,
        });
        let after = file_image(bytes, FileMode::Regular, blobs)?;
        operations.push(PlannedOperation::PatchReaderFile {
            path: path.clone(),
            before,
            after,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use jails_compiler::Compiler;

    /// One entity and nothing else: the smallest model with a managed tree.
    const MODEL: &str = "jdl 1\n\napp Notes @id(project_notes) {\n  pkg com.example.notes\n  \
         java 26\n  platform spring\n  build maven\n  storage none\n}\n\n\
         entity Note @id(ent_note) {\n  id: uuid @id(fld_note_id) @pk\n}\n";

    /// A model that reaches **every persisted struct**, so the goldens below
    /// cover the format rather than the part of it a fixture happens to use.
    ///
    /// The compiler lock is `#[derive(Serialize)]` over `AppModel`, so adding
    /// a field to any model struct silently changes the persisted format; the
    /// lock then fails closed on the next run, which is right and is also the
    /// whole problem, because nothing says the shape moved. The goldens are
    /// byte-compared rather than round-tripped, because a round-trip passes
    /// through whatever the current serializer does and can never notice that
    /// it changed. `UPDATE_GOLDEN=1` refreshes them, and the diff is the
    /// notice.
    ///
    /// Written in JDL v1 rather than TOML because that is the authoring
    /// boundary, and because the TOML fixture above cannot express half of
    /// these. A struct missing from here is a struct whose shape can change
    /// without the golden noticing: `MODEL` has no source units, so adding a
    /// field to `SourceUnit` changes nothing there and the golden reports
    /// green.
    const EVERY_SHAPE: &str = "jdl 1\napp Notes @id(project_notes) {\n           pkg com.example.notes\n  java 26\n  platform spring\n  build maven\n           storage postgres\n}\n\ncap json\n\ndep org.jsoup:jsoup @version(\"1.18.3\")\n\n         prop server.port = \"8080\"\n\nentity Note @id(ent_note) {\n  use repo\n           use factory\n  id: uuid @id(fld_note_id) @pk\n           title: string @id(fld_note_title) @notBlank\n           status: string @id(fld_note_status)\n\n  index [status] @id(idx_note_status)\n\n           command Create(title, status) @id(op_note_create) {\n    emit Created\n  }\n\n           query Open(status) @id(op_note_open) {\n    limit 20\n  }\n\n           transition Rename(title) @id(op_note_rename) {\n    update [title]\n  }\n\n           event Created(id, title) @id(op_note_created)\n}\n\n         component sealed Outcome @id(cmp_outcome) {\n  variant Accepted\n  variant Rejected\n}\n\n         component service Notifier @id(cmp_notifier) {\n}\n";

    /// The smallest build file the golden's plan can edit.
    const GOLDEN_POM: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0">
  <modelVersion>4.0.0</modelVersion>
  <parent>
    <groupId>org.springframework.boot</groupId>
    <artifactId>spring-boot-starter-parent</artifactId>
    <version>4.1.0</version>
    <relativePath/>
  </parent>
  <groupId>com.example</groupId>
  <artifactId>notes</artifactId>
  <version>0.0.1-SNAPSHOT</version>
  <dependencies>
  </dependencies>
</project>
"#;

    /// **The exact plan's own encoding, which is what a reviewer confirms.**
    ///
    /// `jails.plan.v1` inside `jails.plan-bundle.v1` -- the two schemas
    /// `--plan-out` writes -- is the format with the widest
    /// contract in the system. `preview`, `--plan-out`, the confirmation
    /// prompt and `execute` all refer to one bundle, and "apply never replans"
    /// means the bytes reviewed are the bytes applied. A field appearing or
    /// changing shape there moves what a reader is agreeing to.
    ///
    /// Byte-compared for the same reason the lock is: a round-trip cannot
    /// notice that the serializer moved. `UPDATE_GOLDEN=1` refreshes it and
    /// the diff is the notice.
    ///
    /// **The blobs are elided and their digests are not**, which costs the
    /// golden nothing: a `ContentDigest` key *is* an assertion about the bytes
    /// it names, and a stronger one than a JSON array of them, so eliding the
    /// value keeps the diff readable while leaving any change to the content
    /// visible as a changed key. The same fixture as the lock golden is used
    /// deliberately -- one model reaching every persisted struct, so a new
    /// operation variant or a new precondition field cannot land unseen.
    #[test]
    fn the_exact_plan_encoding_matches_its_golden() {
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/protocol-golden/plan-bundle-v1.json");
        let bundle = golden_bundle();
        let mut parsed = serde_json::to_value(&bundle).unwrap();
        if let Some(blobs) = parsed
            .get_mut("blobs")
            .and_then(serde_json::Value::as_object_mut)
        {
            for bytes in blobs.values_mut() {
                let length = bytes.as_array().map_or(0, Vec::len);
                *bytes = serde_json::json!(format!("<{length} bytes elided>"));
            }
        }
        let encoded = serde_json::to_string_pretty(&parsed).unwrap() + "\n";

        if std::env::var_os("UPDATE_GOLDEN").is_some() {
            std::fs::write(&fixture, &encoded).unwrap();
            return;
        }
        let expected = std::fs::read_to_string(&fixture)
            .expect("tests/protocol-golden/plan-bundle-v1.json is checked in");
        assert_eq!(
            encoded, expected,
            "the exact plan encoding changed.\n       \
             This is the document a reviewer confirms and the executor \
             applies, so a change here changes what confirmation means.\n       \
             If it is intended, refresh with `UPDATE_GOLDEN=1 cargo test -p \
             jails-workspace` and read the diff."
        );
    }

    /// **The digest is part of the format, and is asserted separately.**
    ///
    /// The golden above would still pass if `plan_digest` stopped covering a
    /// field: the plan's `id` and `digest` are serialized like anything else,
    /// so a digest computed over less would simply be goldened as a different
    /// string. Recomputing it from the parts here is what makes the golden's
    /// digest an *answer* rather than a record of whatever the code produced.
    #[test]
    fn the_goldened_plan_digest_is_the_digest_of_its_own_parts() {
        let bundle = golden_bundle();
        let recomputed = plan_digest(
            &bundle.plan.compiler,
            &bundle.plan.base,
            &bundle.plan.input,
            &bundle.plan.summary,
            &bundle.plan.operations,
            &bundle.plan.follow_up_effects,
        )
        .unwrap();
        assert_eq!(recomputed, bundle.plan.digest);
        assert_eq!(bundle.plan.id, bundle.plan.digest.as_str());
    }

    /// The one bundle every golden here is taken from.
    fn golden_bundle() -> PlanBundle {
        let model = jails_model::parse_jdl(EVERY_SHAPE).unwrap();
        let mut snapshot = WorkspaceSnapshot::detached(model);
        // Pinned rather than observed: a Spring service unit needs a captured
        // Boot project, and a golden must not depend on what is on the
        // machine running it.
        snapshot.project.build_system = jails_contracts::BuildSystem::Maven;
        snapshot.project.spring_boot = Some("4.1.0".to_string());
        // A `dep` and a `prop` in the model mean the plan edits the reader's
        // build and properties files, and an exact plan will not touch a file
        // it has no before-image for. Captured here rather than dropped from
        // the fixture: `Dependency` and `Setting` are two more persisted
        // structs, and a golden that skipped them would be the same gap these
        // tests exist to close.
        snapshot.files.insert(
            ProjectPath::parse("pom.xml").unwrap(),
            jails_contracts::CapturedFile {
                bytes: GOLDEN_POM.as_bytes().to_vec(),
                executable: false,
            },
        );
        let draft = Compiler::compile(&snapshot, None).unwrap();
        materialize(
            &snapshot,
            CanonicalModelPatch::reconcile(),
            draft,
            // Pinned rather than `COMPILER_VERSION`: the version is *meant* to
            // move, and a golden that churned on every bump would be refreshed
            // without being read, which is how a golden stops being one.
            "jails.compiler.golden",
            Restore::Refuse,
        )
        .unwrap()
    }

    #[test]
    fn the_compiler_lock_encoding_matches_its_golden() {
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/protocol-golden/compiler-lock-v2.json");
        let bundle = golden_bundle();
        let lock = bundle
            .plan
            .operations
            .iter()
            .find_map(|operation| match operation {
                PlannedOperation::ReplaceStateFile { path, after, .. }
                    if path.as_str() == crate::capture::COMPILER_LOCK =>
                {
                    bundle.blobs.get(&after.blob)
                }
                _ => None,
            })
            .expect("the plan writes a compiler lock");
        // **The file *contents* are elided, and only they.** A lock carries
        // the whole accepted projection, so a verbatim golden is 380 KB of
        // generated Java as JSON byte arrays -- and a diff nobody can read is
        // a golden nobody reads, which is the failure this is meant to
        // prevent rather than cause. Every struct shape survives the
        // elision: `RenderedFile`, `Provenance`, `FileKind` and `FileMode`
        // are all still here with their fields. What the bytes themselves say
        // belongs in a tree golden rather than in the middle of a format one.
        let mut parsed: serde_json::Value = serde_json::from_slice(lock).unwrap();
        if let Some(files) = parsed
            .get_mut("projection")
            .and_then(|projection| projection.get_mut("files"))
            .and_then(serde_json::Value::as_object_mut)
        {
            for file in files.values_mut() {
                if let Some(bytes) = file.get_mut("bytes") {
                    let length = bytes.as_array().map_or(0, Vec::len);
                    *bytes = serde_json::json!(format!("<{length} bytes elided>"));
                }
            }
        }
        // A sealed migration's bytes are elided for the same reason, and the
        // digest beside them in `migrations` is what this golden is about.
        if let Some(sealed) = parsed
            .get_mut("migration_bytes")
            .and_then(serde_json::Value::as_object_mut)
        {
            for bytes in sealed.values_mut() {
                let length = bytes.as_array().map_or(0, Vec::len);
                *bytes = serde_json::json!(format!("<{length} bytes elided>"));
            }
        }
        let encoded = serde_json::to_string_pretty(&parsed).unwrap() + "\n";

        if std::env::var_os("UPDATE_GOLDEN").is_some() {
            std::fs::write(&fixture, &encoded).unwrap();
            return;
        }
        let expected = std::fs::read_to_string(&fixture)
            .expect("tests/protocol-golden/compiler-lock-v2.json is checked in");
        assert_eq!(
            encoded, expected,
            "the compiler lock encoding changed.\n       \
             If that is intended -- a field added to a model struct is enough \
             -- refresh with UPDATE_GOLDEN=1 and read the diff: every existing \
             project's accepted state stops decoding at the same moment."
        );
    }

    /// A lock written by an older jails still decodes.
    ///
    /// Old fixtures must decode, and the v1 envelope is the oldest there is:
    /// no `compiler`, no `projection`. `capture` still has the arm,
    /// and this is what proves the arm works rather than merely existing --
    /// a schema branch nothing exercises is a branch that has already rotted.
    #[test]
    fn a_v1_compiler_lock_still_decodes() {
        let model = jails_model::parse_jdl(EVERY_SHAPE).unwrap();
        let bytes = model.canonical_json().unwrap();
        let v1 = serde_json::json!({
            "schema": "jails.compiler-lock.v1",
            "model_digest": digest(&bytes).unwrap(),
            "model": serde_json::from_slice::<serde_json::Value>(&bytes).unwrap(),
        });
        crate::capture::decode_compiler_lock_for_test(&serde_json::to_vec(&v1).unwrap())
            .expect("a v1 lock decodes");

        // ... and a lock whose model does not match its digest is refused
        // rather than trusted, which is the property the digest is for.
        let mut tampered = v1.clone();
        tampered["model_digest"] = serde_json::json!(
            "sha256:0000000000000000000000000000000000000000000000000000000000000000"
        );
        let error =
            crate::capture::decode_compiler_lock_for_test(&serde_json::to_vec(&tampered).unwrap())
                .expect_err("a lock that disagrees with its own digest must refuse");
        assert!(error.contains("compiler.lock"), "{error}");
    }
    #[test]
    fn a_partially_published_tree_converges_before_lock_acceptance() {
        let model = jails_model::parse_jdl(MODEL).unwrap();
        let snapshot = WorkspaceSnapshot::detached(model);
        let draft = Compiler::compile(&snapshot, None).unwrap();
        let bundle = materialize(
            &snapshot,
            CanonicalModelPatch::reconcile(),
            draft,
            jails_compiler::COMPILER_VERSION,
            Restore::Refuse,
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

    #[test]
    fn an_absent_empty_managed_tree_is_already_converged() {
        let source = MODEL.split("\nentity Note").next().unwrap();
        let model = jails_model::parse_jdl(source).unwrap();
        let snapshot = WorkspaceSnapshot::detached(model);
        let draft = Compiler::compile(&snapshot, None).unwrap();
        let bundle = materialize(
            &snapshot,
            CanonicalModelPatch::reconcile(),
            draft,
            jails_compiler::COMPILER_VERSION,
            Restore::Refuse,
        )
        .unwrap();
        assert!(matches!(
            bundle.plan.operations.as_slice(),
            [PlannedOperation::ReplaceStateFile { .. }]
        ));
        crate::verify::verify_bundle(&bundle).unwrap();
    }

    /// A different patch is a different plan, for the same reason.
    ///
    /// The input is part of the reviewed identity: two plans that write the
    /// same bytes for different reasons are not the same plan, and a digest
    /// that could not tell them apart would let a confirmation be replayed
    /// against a request nobody made.
    #[test]
    fn a_different_patch_input_is_a_different_plan() {
        let model = jails_model::parse_jdl(MODEL).unwrap();
        let snapshot = WorkspaceSnapshot::detached(model);
        let digest = |input: CanonicalModelPatch| {
            let draft = Compiler::compile(&snapshot, None).unwrap();
            materialize(
                &snapshot,
                input,
                draft,
                jails_compiler::COMPILER_VERSION,
                Restore::Refuse,
            )
            .unwrap()
            .plan
            .digest
        };
        assert_ne!(
            digest(CanonicalModelPatch::reconcile()),
            digest(CanonicalModelPatch {
                schema: "jails.model-patch.v1".to_string(),
                bytes: br#"{"kind":"batch","patches":[]}"#.to_vec(),
            })
        );
    }

    /// ... and a different compiler version is a different plan.
    ///
    /// The other half of the same rule, and the one that makes the first
    /// half safe to rely on: a digest that ignored the compiler version would
    /// let a bundle reviewed under one renderer be applied by another, which
    /// reopens the question "did preview and apply run the same
    /// computation?".
    #[test]
    fn a_different_compiler_version_is_a_different_plan() {
        let model = jails_model::parse_jdl(MODEL).unwrap();
        let snapshot = WorkspaceSnapshot::detached(model);
        let digest = |version: &str| {
            let draft = Compiler::compile(&snapshot, None).unwrap();
            materialize(
                &snapshot,
                CanonicalModelPatch::reconcile(),
                draft,
                version,
                Restore::Refuse,
            )
            .unwrap()
            .plan
            .digest
        };
        assert_ne!(
            digest(jails_compiler::COMPILER_VERSION),
            digest("jails.compiler.test-only")
        );
    }

    /// **The property `apply never replans` rests on.**
    ///
    /// Identical `WorkspaceSnapshot + CanonicalModelPatch + CompilerVersion`
    /// yields an identical plan digest. Every other guarantee in the protocol
    /// is downstream of it --
    /// preview shows a digest, the reader confirms that digest, and the
    /// executor applies a bundle rather than recomputing one. If the same
    /// three inputs could produce two digests, the thing reviewed and the
    /// thing applied would be related only by hope.
    ///
    /// **The compile is run twice, not the materialization**, because the
    /// interesting half is the compiler: a `HashMap` iterated into a `Vec`, a
    /// timestamp, a path read from the environment, or an unsorted set would
    /// all be invisible in one run and fatal here. This is the whole model,
    /// not a fragment, so it exercises the maps every emitter builds.
    #[test]
    fn the_same_snapshot_patch_and_compiler_produce_the_same_plan_digest() {
        let model = jails_model::parse_jdl(MODEL).unwrap();
        let snapshot = WorkspaceSnapshot::detached(model);
        let digest = || {
            let draft = Compiler::compile(&snapshot, None).unwrap();
            materialize(
                &snapshot,
                CanonicalModelPatch::reconcile(),
                draft,
                jails_compiler::COMPILER_VERSION,
                Restore::Refuse,
            )
            .unwrap()
            .plan
            .digest
        };
        assert_eq!(digest(), digest());
    }

    #[test]
    fn materialization_is_exact_and_self_verifying() {
        let model = jails_model::parse_jdl(MODEL).unwrap();
        let snapshot = WorkspaceSnapshot::detached(model);
        let draft = Compiler::compile(&snapshot, None).unwrap();
        let bundle = materialize(
            &snapshot,
            CanonicalModelPatch::reconcile(),
            draft,
            jails_compiler::COMPILER_VERSION,
            Restore::Refuse,
        )
        .unwrap();
        assert_eq!(bundle.plan.operations.len(), 2);
        assert!(matches!(
            bundle.plan.operations.last(),
            Some(PlannedOperation::ReplaceStateFile { path, .. })
                if path.as_str() == crate::capture::COMPILER_LOCK
        ));
        crate::verify::verify_bundle(&bundle).unwrap();

        let mut damaged = bundle;
        let bytes = damaged.blobs.values_mut().next().unwrap();
        bytes.push(b'x');
        assert!(crate::verify::verify_bundle(&damaged).is_err());
    }
}
