//! The typed comparison, run beside the imperative one and checked against it.
//!
//! ## Why a shadow and not a switch
//!
//! plan.md R1.5 step 7 switches `app plan` and *"test-only shadow reports"* to
//! the typed comparison. The switch cannot complete inside R1: §R1.3 builds the
//! observed side from `ProjectSnapshot`, which is R2's, and the desired side
//! from captured project facts this phase does not gather yet. What R1 *can*
//! do is compute the typed decision from what is already available and check
//! that it agrees with the decision being acted on.
//!
//! That is worth more than it sounds. The two paths reach the same three
//! answers by completely different routes — one by string comparison against a
//! recorded row, the other by owner-set reconciliation over typed identities —
//! so an disagreement is a real defect in one of them, found before the typed
//! path is load-bearing rather than after.
//!
//! ## What it deliberately cannot see
//!
//! An entity whose recorded row this binary cannot represent — a name that
//! predates the protocol's validation, a field spec no current parser accepts
//! — is skipped rather than guessed at. A shadow that reported a disagreement
//! every time it met an old row would be noise, and noise is how a check stops
//! being read.

use jails_protocol::declaration::IntentSpec;
use jails_protocol::entity::{EntityId, EntitySpec, IntentId, OwnerId};
use jails_protocol::identity::{Name, Package};
use jails_protocol::ownership::{
    DesiredEntity, ObservedEntity, ReconcileScope, Reconciled, reconcile,
};
use std::collections::{BTreeMap, BTreeSet};

/// The three answers `app plan` prints.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Status {
    Applied,
    Update,
    Pending,
}

impl Status {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Applied => "applied",
            Self::Update => "update",
            Self::Pending => "pending",
        }
    }
}

/// Build the typed desired and observed sides, as far as this phase can.
///
/// Returns `None` for any entity it cannot represent, rather than a guess.
pub(super) fn typed_view(
    intents: &[super::ResolvedIntent],
    applied: &[jails_project::ledger::Applied],
) -> Option<TypedView> {
    let mut declared = BTreeMap::new();
    for intent in intents {
        let id = intent_id(intent)?;
        let spec = intent_spec(intent)?;
        declared.insert(
            EntityId::Intent(id.clone()),
            DesiredEntity {
                id: EntityId::Intent(id),
                spec: EntitySpec::Intent(spec),
                owners: BTreeSet::from([OwnerId::AppManifest]),
            },
        );
    }

    let mut observed = BTreeMap::new();
    for row in applied {
        // A row this binary cannot represent is skipped, not guessed at.
        let Ok(id) = row.typed_id() else { continue };
        let base = id.package.clone();
        let Ok(spec) = IntentSpec::parse(&row.fields, &row.indexes, row.timestamps, &base) else {
            continue;
        };
        // The recorded owner, from the same `has_spec` fact `app plan` reads.
        let owner = match row.spec {
            jails_project::ledger::SpecPresence::Present => OwnerId::AppManifest,
            jails_project::ledger::SpecPresence::Absent => OwnerId::DirectCli,
            // Unresolvable origin: it has an owner, but not one this can name.
            jails_project::ledger::SpecPresence::UnknownLegacy => continue,
        };
        observed.insert(
            EntityId::Intent(id.clone()),
            ObservedEntity {
                spec: EntitySpec::Intent(spec),
                owners: BTreeSet::from([owner]),
            },
        );
    }

    Some(TypedView { declared, observed })
}

pub(super) struct TypedView {
    declared: BTreeMap<EntityId, DesiredEntity>,
    observed: BTreeMap<EntityId, ObservedEntity>,
}

impl TypedView {
    /// Reconcile the manifest scope, so the comparison is over owner sets
    /// rather than over string equality with a recorded row.
    pub(super) fn reconcile(&self) -> crate::Result<Reconciled> {
        reconcile(
            &ReconcileScope::AppManifest,
            &self.declared,
            &self.observed,
            &[],
        )
    }

    /// What the typed comparison says about one intent, or `None` when it
    /// cannot represent it.
    pub(super) fn status(&self, intent: &super::ResolvedIntent) -> Option<Status> {
        let id = EntityId::Intent(intent_id(intent)?);
        let declared = self.declared.get(&id)?;
        Some(match self.observed.get(&id) {
            None => Status::Pending,
            Some(applied) if applied.spec == declared.spec => Status::Applied,
            Some(_) => Status::Update,
        })
    }
}

/// A one-line name for an entity, matching `ResolvedIntent::label`'s shape.
pub(super) fn describe(id: &EntityId) -> String {
    match id {
        EntityId::Intent(intent) => format!("{} {}", label_of(intent), intent.name),
        EntityId::Capability(capability) => capability.kind.label().to_string(),
        EntityId::ToolFeature(_) => "fast-test".to_string(),
    }
}

fn label_of(intent: &IntentId) -> String {
    use clap::ValueEnum;
    intent
        .recipe
        .to_possible_value()
        .expect("every ArtifactKind has a clap value")
        .get_name()
        .to_string()
}

fn intent_id(intent: &super::ResolvedIntent) -> Option<IntentId> {
    let package = match intent.package.as_deref() {
        Some(text) => Package::parse(text).ok()?,
        None => Package::base(),
    };
    Some(IntentId::new(
        intent.kind,
        Name::parse(&intent.name).ok()?,
        package,
    ))
}

fn intent_spec(intent: &super::ResolvedIntent) -> Option<IntentSpec> {
    let base = match intent.package.as_deref() {
        Some(text) => Package::parse(text).ok()?,
        None => Package::base(),
    };
    IntentSpec::parse(&intent.fields, &intent.indexes, intent.timestamps, &base).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_labels_are_the_ones_app_plan_prints() {
        assert_eq!(Status::Applied.label(), "applied");
        assert_eq!(Status::Update.label(), "update");
        assert_eq!(Status::Pending.label(), "pending");
    }
}
