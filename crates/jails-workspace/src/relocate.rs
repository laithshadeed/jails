//! `jails model relocate`: the one-time move of a project generated before
//! managed output lived under `src/`.
//!
//! Older releases rendered into `.jails/generated/{main,test}/{java,resources}`
//! and `.jails/generated/requests`, and told the build about it with a marked
//! source-root block. A project like that still opens -- the lock names those
//! paths and capture reads whatever the lock names -- but every render from
//! here on wants the `src/` path, and the reconciler would refuse each moved
//! artifact as a rename onto a path that already exists. So the move is one
//! plan, built here and executed like every other write: the captured bytes
//! of every managed file, hand edits included, published at the reader path
//! and retired from the old one; the lock rewritten to name the new paths;
//! the source-root block taken out of the build file. It refuses if any
//! destination already exists, naming it, and writes nothing.
//!
//! **This is the only place the old root is spelled.** Nothing else in the
//! product knows the string, which is what makes the exit condition of the
//! move checkable with `grep`.

use crate::materialize::{digest, file_image, plan_digest, tree_id};
use jails_contracts::{
    FileMode, FilePrecondition, Plan, PlanBundle, PlanInput, PlannedOperation, ProjectPath,
    RenderedTree, SemanticPlan, SourceRoot, TreeEntry, TreeManifest, WorkspaceSnapshot,
};
use jails_model::Diagnostic;
use std::collections::BTreeMap;

/// Where the older releases rendered.
const OLD_ROOT: &str = ".jails/generated/";

/// The four old subtrees and the roots they move to; the order matters only
/// for the one entry whose prefix is a prefix of another's.
const MOVES: [(&str, SourceRoot); 5] = [
    ("main/java/", SourceRoot::MainJava),
    ("test/java/", SourceRoot::TestJava),
    ("main/resources/", SourceRoot::MainResources),
    ("test/resources/", SourceRoot::TestResources),
    ("requests/", SourceRoot::TestHttp),
];

/// Where a managed path the lock names moves to, or `None` when it already
/// lives under `src/`.
fn destination(path: &ProjectPath) -> Result<Option<ProjectPath>, Diagnostic> {
    let Some(rest) = path.as_str().strip_prefix(OLD_ROOT) else {
        return Ok(None);
    };
    for (prefix, root) in MOVES {
        if let Some(relative) = rest.strip_prefix(prefix) {
            return crate::capture::project_path(format!("{}/{relative}", root.path())).map(Some);
        }
    }
    Err(Diagnostic::new(
        "workspace-relocate-unmapped",
        path.to_string(),
        format!("managed file `{path}` is under the old generated root and matches no source set"),
        "move it by hand and edit `.jails/compiler.lock.json` to name the new path",
    ))
}

/// Every `(old, new)` pair this project's lock implies.
///
/// The frontend observes each destination before asking for the plan, so a
/// reader file already there is captured and refused by name rather than
/// discovered by the executor.
pub fn relocation_targets(
    snapshot: &WorkspaceSnapshot,
) -> Result<Vec<(ProjectPath, ProjectPath)>, Diagnostic> {
    let Some(projection) = snapshot.accepted_projection.as_ref() else {
        return Err(Diagnostic::new(
            "workspace-relocate-nothing-accepted",
            "$.projection",
            "this project has no generated tree yet",
            "nothing is managed yet, so there is nothing to relocate",
        ));
    };
    projection
        .files
        .keys()
        .filter_map(|path| {
            destination(path)
                .map(|moved| moved.map(|moved| (path.clone(), moved)))
                .transpose()
        })
        .collect()
}

/// The exact plan that moves every managed file under `src/`.
pub fn relocate(
    snapshot: &WorkspaceSnapshot,
    compiler_version: &str,
) -> Result<PlanBundle, Diagnostic> {
    let targets = relocation_targets(snapshot)?;
    if targets.is_empty() {
        return Err(Diagnostic::without_a_fix(
            "workspace-relocate-nothing-to-move",
            "$.projection",
            "nothing to relocate: every managed file already lives under `src/`",
        ));
    }
    let projection = snapshot
        .accepted_projection
        .as_ref()
        .expect("relocation targets came from the generated tree");
    let mut blobs = BTreeMap::new();
    let mut before = TreeManifest::default();
    let mut after = TreeManifest::default();
    let mut base = snapshot.preconditions.clone();
    let mut relocated = RenderedTree {
        files: BTreeMap::new(),
        reader_facets: projection.reader_facets.clone(),
    };
    let moved: BTreeMap<&ProjectPath, &ProjectPath> =
        targets.iter().map(|(old, new)| (old, new)).collect();
    for (path, file) in &projection.files {
        let Some(new) = moved.get(path) else {
            relocated.files.insert(path.clone(), file.clone());
            continue;
        };
        if snapshot.files.contains_key(new) {
            return Err(Diagnostic::new(
                "workspace-relocate-destination-exists",
                new.to_string(),
                format!("`{new}` already exists, and the managed file `{path}` moves there"),
                "move or remove your file first; nothing was written",
            ));
        }
        let Some(live) = snapshot.files.get(path) else {
            return Err(Diagnostic::new(
                "workspace-relocate-source-missing",
                path.to_string(),
                format!("managed file `{path}` is missing from the tree"),
                "restore it, then run `jails model relocate` again; nothing was written",
            ));
        };
        let mode = if live.executable {
            FileMode::Executable
        } else {
            FileMode::Regular
        };
        let kind = crate::materialize::file_kind(new);
        let blob = digest(&live.bytes)?;
        blobs.insert(blob.clone(), live.bytes.clone());
        before.entries.insert(
            path.clone(),
            TreeEntry {
                kind,
                mode,
                blob: blob.clone(),
            },
        );
        after
            .entries
            .insert((*new).clone(), TreeEntry { kind, mode, blob });
        base.files
            .entry((*new).clone())
            .or_insert(FilePrecondition::Missing);
        relocated.files.insert((*new).clone(), file.clone());
    }

    let mut operations = vec![PlannedOperation::PublishMergedTree {
        root: crate::capture::project_path(SourceRoot::PARENT)?,
        before: Some(tree_id(&before)?),
        after: tree_id(&after)?,
    }];
    for build_file in ["pom.xml", "build.gradle", "build.gradle.kts"] {
        let path = crate::capture::project_path(build_file)?;
        let Some(captured) = snapshot.files.get(&path) else {
            continue;
        };
        let text = std::str::from_utf8(&captured.bytes).map_err(|_| {
            Diagnostic::new(
                "workspace-relocate-document-not-utf8",
                path.to_string(),
                format!("reader document `{path}` is not UTF-8"),
                "convert it to UTF-8, then run again",
            )
        })?;
        let stripped = crate::documents::strip_generated_source_roots(text)?;
        crate::materialize::plan_reader_document(
            snapshot,
            &mut operations,
            &mut blobs,
            path,
            stripped.into_bytes(),
        )?;
    }
    let accepted_model = snapshot
        .accepted_model
        .as_ref()
        .expect("a generated tree is accepted beside its model");
    let lock_bytes = crate::materialize::encode_compiler_lock(
        snapshot
            .accepted_compiler
            .as_deref()
            .unwrap_or(compiler_version),
        accepted_model,
        &relocated,
        snapshot.accepted_migrations.clone(),
        snapshot.accepted_migration_bytes.clone(),
    )?;
    let lock_path = crate::capture::project_path(crate::capture::COMPILER_LOCK)?;
    let lock_before = snapshot
        .files
        .get(&lock_path)
        .map(|file| file_image(&file.bytes, FileMode::Regular, &mut blobs))
        .transpose()?;
    let lock_after = file_image(&lock_bytes, FileMode::Regular, &mut blobs)?;
    operations.push(PlannedOperation::ReplaceStateFile {
        path: lock_path,
        before: lock_before,
        after: lock_after,
    });

    let input = PlanInput::reconcile();
    let summary = SemanticPlan {
        model_nodes: accepted_model.node_count(),
        managed_files: relocated.files.len(),
        migrations: 0,
        reader_document_intents: operations.len() - 2,
        effects: 0,
    };
    let digest = plan_digest(compiler_version, &base, &input, &summary, &operations, &[])?;
    let plan = Plan {
        schema: crate::materialize::PLAN_SCHEMA.to_string(),
        id: digest.as_str().to_string(),
        compiler: compiler_version.to_string(),
        base,
        input,
        summary,
        operations,
        follow_up_effects: Vec::new(),
        digest,
    };
    let mut trees = BTreeMap::new();
    trees.insert(tree_id(&before)?, before);
    trees.insert(tree_id(&after)?, after);
    let bundle = PlanBundle {
        schema: crate::materialize::BUNDLE_SCHEMA.to_string(),
        plan,
        trees,
        blobs,
    };
    crate::verify::verify_bundle(&bundle)?;
    Ok(bundle)
}
