//! The one aggregate post-commit effect: bringing compose up to the document
//! this transition committed.
//!
//! ## Why an effect at all, rather than a step in the commit
//!
//! Starting a container is not a file operation and cannot be rolled back by
//! restoring a preimage. plan.md §R6.6 keeps it explicitly outside the project
//! transaction for that reason, and §R3.3 gives it the shape that makes it
//! survivable anyway: a *descriptor*, frozen at preparation, naming the exact
//! documents and the exact service sets it will act on. The commit records the
//! descriptor; the attempt happens afterwards and can be retried, because the
//! descriptor says precisely what a retry would do.
//!
//! ## Why the documents are objects and not paths
//!
//! `docker compose` is handed `--file <object>`, never the live
//! `compose.yaml`. Between the commit and the attempt somebody may edit the
//! file; running against what they wrote would stop or start services this
//! transition never described. The frozen images are also what makes the
//! stop set derivable at all: `stop_services` is what the *prior* managed map
//! held and the *committed* document no longer names, so both documents have
//! to be pinned.
//!
//! ## The rule this module is
//!
//! §R3.3, verbatim: emit at most one, and only when the invocation did not say
//! `--no-start` and either the managed service map or the committed compose
//! output changed. Owner-only changes, unrelated repair and an already
//! satisfied second apply emit nothing.

use crate::Result;
use crate::prepare::{FileOp, OperationTarget};
use jails_protocol::effect::PostCommitEffect;
use jails_protocol::identity::{ObjectId, ProjectPath, ServiceName};
use jails_protocol::resource::{ResourceKey, ResourceValue};
use jails_protocol::snapshot::{Captured, ProjectSnapshot};
use jails_support::codec::{Encoder, domain_hash};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

/// The canonical project path a compose document lives at, matching the one
/// the projection splices into. One spelling, so the effect cannot name a
/// file the transition did not write.
pub(super) fn compose_output() -> Result<ProjectPath> {
    ProjectPath::parse("compose.yaml")
}

/// Build the effect, or say why there is none.
///
/// `start_services` is false both for `--no-start` and for every request
/// variant that has no such field: §R3.3 makes those ineligible rather than
/// defaulting them on, so a maintenance action cannot change runtime services
/// by accident.
pub(super) fn compose_reconcile(
    start_services: bool,
    prior_rows: &[jails_protocol::resource::ResourceRecord],
    desired_rows: &[jails_protocol::resource::DesiredResource],
    base: &ProjectSnapshot,
    operations: &[FileOp],
    objects: &mut BTreeMap<ObjectId, Arc<[u8]>>,
) -> Result<Option<PostCommitEffect>> {
    if !start_services {
        return Ok(None);
    }
    let path = compose_output()?;

    let prior_managed_services: BTreeMap<ServiceName, ObjectId> = prior_rows
        .iter()
        .filter_map(|row| managed(&row.key, &row.value))
        .collect::<Result<_>>()?;
    let desired_services: BTreeMap<ServiceName, ObjectId> = desired_rows
        .iter()
        .filter_map(|row| managed(&row.key, &row.value))
        .collect::<Result<_>>()?;

    let (before_document, before_bytes) = match base.read(&path)? {
        Captured::Present(file) => (Some(file.sha256), Some(file.bytes.clone())),
        Captured::Absent => (None, None),
    };
    let (after_document, after_bytes) =
        committed(&path, operations, objects).unwrap_or((before_document, before_bytes.clone()));

    // §R3.3's existence rule. Both halves matter: a capability that changes
    // nothing about the services but rewrites the document still needs the
    // running containers brought to it, and a document that did not move can
    // still have lost a managed service to another owner's removal.
    if prior_managed_services == desired_services && before_document == after_document {
        return Ok(None);
    }

    // What the committed document no longer names. A managed service the
    // reader deliberately kept as an unmanaged block is therefore *not*
    // stopped -- it is still in the file, so it is still something they run.
    let surviving: BTreeSet<ServiceName> = match &after_bytes {
        Some(bytes) => names(bytes)?,
        None => BTreeSet::new(),
    };
    let stop_services: BTreeSet<ServiceName> = prior_managed_services
        .keys()
        .filter(|name| !surviving.contains(*name))
        .cloned()
        .collect();

    // The guards §R3.3 requires, and they are guards rather than reporting
    // metadata: an attempt hands `--file` one of these objects, so an absent
    // one is an attempt that cannot be made.
    if !stop_services.is_empty() && before_document.is_none() {
        return Err(format!(
            "compose services {} are no longer declared, and there is no compose document to \
             stop them with.\n       fix: restore `{path}`, or pass `--no-start` to leave the \
             running containers alone. jails will not guess a document it did not read.",
            list(&stop_services)
        ));
    }
    if !desired_services.is_empty() && after_document.is_none() {
        return Err(format!(
            "this change wants compose services running and commits no `{path}`.\n       fix: \
             pass `--no-start`; starting a service from a document that will not exist is not \
             something a retry could repeat."
        ));
    }
    if let Some(bytes) = &before_bytes {
        let declared = names(bytes)?;
        if let Some(missing) = stop_services.iter().find(|name| !declared.contains(*name)) {
            return Err(format!(
                "`{missing}` is recorded as a managed compose service and the document jails \
                 last read does not declare it, so there is no truthful file to stop it \
                 with.\n       fix: pass `--no-start`, or put the service block back before \
                 removing it."
            ));
        }
    }

    // Both documents have to survive as objects: an attempt hands `--file`
    // one of them, and the preimage of a replaced file is guarded rather than
    // interned, so nothing else would have kept it.
    for (id, bytes) in [
        (before_document, before_bytes),
        (after_document, after_bytes.clone()),
    ] {
        if let (Some(id), Some(bytes)) = (id, bytes) {
            objects.entry(id).or_insert(bytes);
        }
    }

    Ok(Some(PostCommitEffect::ComposeReconcile {
        compose_output: path,
        before_document,
        after_document,
        prior_managed_services,
        desired_services,
        stop_services,
    }))
}

/// One managed service row, as `(name, spec hash)`.
///
/// The value is hashed rather than stored so the descriptor stays a fixed size
/// and so two specs that differ anywhere differ here: §R3.3 fixes the domain
/// separator, and an attempt revalidates the store's current map against
/// exactly this one before running.
fn managed(key: &ResourceKey, value: &ResourceValue) -> Option<Result<(ServiceName, ObjectId)>> {
    let ResourceKey::ComposeService(name) = key else {
        return None;
    };
    let ResourceValue::ComposeService(spec) = value else {
        return Some(Err(format!(
            "`{name}` is keyed as a compose service and its value is not one"
        )));
    };
    let mut encoder = Encoder::new();
    Some(
        match spec.encode(&mut encoder).and_then(|()| encoder.finish()) {
            Ok(bytes) => Ok((
                name.clone(),
                ObjectId::from_bytes(domain_hash("JAILS-COMPOSE-SERVICE-SPEC-1", &bytes)),
            )),
            Err(why) => Err(why),
        },
    )
}

/// A document as this module needs it: what it is addressed by, and what it
/// says. Absent on both counts is a file that will not be there.
type Document = (Option<ObjectId>, Option<Arc<[u8]>>);

/// The image this transition commits at `path`, when it commits one.
///
/// `None` means no operation touches it, which is a different answer from a
/// deletion: the caller falls back to the preimage for the first and records
/// an absence for the second.
fn committed(
    path: &ProjectPath,
    operations: &[FileOp],
    objects: &BTreeMap<ObjectId, Arc<[u8]>>,
) -> Option<Document> {
    let target = OperationTarget::Project(path.clone());
    operations.iter().find_map(|operation| {
        if *operation.target() != target {
            return None;
        }
        match operation {
            FileOp::Create { after, .. } | FileOp::Replace { after, .. } => {
                Some((Some(after.id), objects.get(&after.id).cloned()))
            }
            FileOp::Delete { .. } => Some((None, None)),
        }
    })
}

/// The service names a document declares, through the one compose reader.
fn names(bytes: &[u8]) -> Result<BTreeSet<ServiceName>> {
    let text = std::str::from_utf8(bytes).map_err(|_| {
        "the compose document is not UTF-8, so its services cannot be read".to_string()
    })?;
    jails_project::compose::all_service_names(text)
        .into_iter()
        .map(ServiceName::parse)
        .collect()
}

fn list(names: &BTreeSet<ServiceName>) -> String {
    names
        .iter()
        .map(ServiceName::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}
