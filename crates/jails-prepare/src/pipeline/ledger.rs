//! What the store says after this transition.
//!
//! A draft, never a rewrite: every table here starts from what was observed
//! and merges what this request states, because a scope may only speak for its
//! own claims. Rebuilding the store from one request's intent would quietly
//! delete everything that request did not mention.

use super::*;

/// Everything the ledger's `outputs` table needs that the operations alone do
/// not say.
#[derive(Default)]
pub(super) struct Recorded {
    pub(super) outputs: BTreeMap<ProjectPath, (StoredFileImage, LiveFileImage)>,
    pub(super) retired: BTreeSet<ProjectPath>,
    pub(super) stamps: BTreeMap<ProjectPath, DesiredProvenance>,
}

/// Whether a computed store says anything the observed one did not.
///
/// Only the rows are compared. `written_by`, `generation` and
/// `last_operation` change on every commit by construction, so including them
/// would make every store differ from itself.
pub(super) fn unchanged(store: &LedgerV2, observed: &Option<LedgerV2>) -> bool {
    let Some(observed) = observed else {
        return store.applied.is_empty()
            && store.one_shots.is_empty()
            && store.resources.is_empty()
            && store.outputs.is_empty()
            && store.pending_conflict.is_none();
    };
    store.applied == observed.applied
        && store.one_shots == observed.one_shots
        && store.resources == observed.resources
        && store.outputs == observed.outputs
        && store.pending_conflict == observed.pending_conflict
}

/// The store this transition will leave behind.
///
/// An *update* of what was observed, never a fresh construction: a row this
/// request says nothing about -- another capability's dependency, an entity it
/// has never heard of -- survives untouched. Rebuilding the store from one
/// request's intent would quietly delete everything the request did not
/// mention, which is the opposite of what a scope means.
///
/// `outputs` is deliberately not written here. §R1.4's `OutputRecord` carries
/// a `RendererStamp`, and this route's bytes arrive already rendered by a
/// recipe that never produced one. An output row with an invented stamp would
/// claim provenance that did not happen, and provenance is what R5.2's upgrade
/// path reads to decide whether a template moved. It lands with that stamp.
pub(super) fn record_store(
    observed: &ObservedStore,
    intent: &LedgerIntent,
    operation: jails_protocol::identity::OperationId,
    generation: u64,
) -> Result<LedgerV2> {
    observed.validate()?;
    let mut store = observed.ledger.clone().unwrap_or_else(|| LedgerV2 {
        written_by: String::new(),
        generation: 0,
        last_operation: None,
        applied: Vec::new(),
        one_shots: Vec::new(),
        resources: Vec::new(),
        outputs: Vec::new(),
        pending_conflict: None,
    });
    store.written_by = env!("CARGO_PKG_VERSION").to_string();
    store.generation = generation;
    store.last_operation = Some(operation);

    for entity in &intent.entities_after {
        let applied = jails_protocol::record::AppliedEntity {
            id: entity.id.clone(),
            owners: entity.owners.clone(),
            version: jails_protocol::record::AppliedVersion {
                spec: entity.spec.clone(),
                operation,
            },
        };
        match store.applied.iter_mut().find(|row| row.id == applied.id) {
            // An entity whose owners and spec are unchanged keeps the
            // operation that applied it. Stamping the current one would say a
            // transition happened to it when none did, and the store would
            // then differ from itself on every repeat -- which is exactly how
            // "already set up" stops being reachable.
            Some(row)
                if row.owners == applied.owners && row.version.spec == applied.version.spec => {}
            Some(row) => *row = applied,
            None => store.applied.push(applied),
        }
    }
    store
        .applied
        .retain(|row| !intent.entities_removed.contains(&row.id));
    store.applied.sort_by(|a, b| a.id.cmp(&b.id));

    for desired in &intent.resources_after {
        match store
            .resources
            .iter_mut()
            .find(|row| row.key == desired.key)
        {
            // Owners union rather than replace: two capabilities wanting one
            // dependency both own it, and a request that stated only its own
            // claim must not erase the other's.
            // Owners union rather than replace. An intent states the claims
            // *this request* makes, which is all a scope may speak for; the
            // other owners of a shared dependency are none of its business,
            // and replacing the set would drop them.
            Some(row) => {
                row.owners.extend(desired.owners.iter().cloned());
                row.value = desired.value.clone();
            }
            None => store
                .resources
                .push(jails_protocol::resource::ResourceRecord {
                    key: desired.key.clone(),
                    owners: desired.owners.clone(),
                    value: desired.value.clone(),
                }),
        }
    }
    // Derived, never declared: a resource is owned, so a resource whose last
    // owner just left has lost its last owner. A second list saying which
    // resources to delete could disagree with the first about the same fact.
    for id in &intent.entities_removed {
        let owner = jails_protocol::resource::ResourceOwner::Entity(id.clone());
        for row in &mut store.resources {
            row.owners.remove(&owner);
        }
    }
    store.resources.retain(|row| !row.owners.is_empty());
    store.resources.sort_by(|a, b| a.key.cmp(&b.key));

    for desired in &intent.one_shots_after {
        let receipt = jails_protocol::record::OneShotReceipt {
            id: desired.id.clone(),
            spec: desired.spec.clone(),
            state: desired.state,
            lifecycle: desired.lifecycle.clone(),
            operation,
        };
        match store.one_shots.iter_mut().find(|row| row.id == receipt.id) {
            // Same rule as an entity: a receipt whose content is unchanged
            // keeps the operation that wrote it.
            Some(row)
                if row.spec == receipt.spec
                    && row.state == receipt.state
                    && row.lifecycle == receipt.lifecycle => {}
            Some(row) => *row = receipt,
            None => store.one_shots.push(receipt),
        }
    }
    store.one_shots.sort_by(|a, b| a.id.cmp(&b.id));

    Ok(store)
}

/// Write the `outputs` table §R5.2 requires: one row per path jails wrote,
/// with the exact bytes it wrote as the base to measure the next change from.
///
/// The row is only written when the renderer said where the bytes came from.
/// §R5.2 makes `RendererStamp` non-optional on an `OutputRecord` for a reason
/// -- provenance is what tells a template upgrade from a declaration change --
/// so a path with no stamp records no row rather than a row with an invented
/// one. That is the same "safe direction" the base-less refusal took, narrowed
/// to the renderers that have not been taught to stamp yet.
pub(super) fn record_outputs(
    store: &mut jails_protocol::envelope::LedgerV2,
    recorded: &Recorded,
    operations: &[FileOp],
) -> Result<()> {
    for (path, (base, current)) in &recorded.outputs {
        let Some(provenance) = recorded.stamps.get(path) else {
            continue;
        };
        // Whoever the operations say contributed, which is the same set the
        // receipt records. Taking it from the change instead would let the
        // two disagree about who wrote a file.
        let contributors = operations
            .iter()
            .find(|operation| matches!(operation.target(), at if at == path))
            .map(|operation| operation.contributors().clone())
            .unwrap_or_default();
        let row = jails_protocol::record::OutputRecord {
            path: path.clone(),
            contributors,
            current: *current,
            base: *base,
            renderer: provenance.stamp.clone(),
        };
        match store.outputs.iter_mut().find(|held| held.path == row.path) {
            // A row whose images this transition did not move keeps *every*
            // field, the stamp included. That is the same rule `record_store`
            // applies to an entity, and for the same reason: a repeat run
            // would otherwise restamp a file it did not write, the store
            // would differ from itself, and "already set up" would stop being
            // reachable. The context genuinely differs between the two runs --
            // a capability is installed the second time -- so this is not a
            // rounding error, it is the answer.
            Some(held) if held.base == row.base && held.current == row.current => {}
            // A row whose contributors this transition did not restate keeps
            // them: a run that moves the bytes without an operation for the
            // path -- the reader's edit standing -- would otherwise lose who
            // owns it.
            Some(held) if row.contributors.is_empty() => {
                held.current = row.current;
                held.base = row.base;
                held.renderer = row.renderer;
            }
            Some(held) => *held = row,
            None => store.outputs.push(row),
        }
    }
    store
        .outputs
        .retain(|row| !recorded.retired.contains(&row.path));
    store.outputs.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(())
}
