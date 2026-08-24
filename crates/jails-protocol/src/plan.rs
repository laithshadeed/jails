//! The whole invocation as a value: the ordered changes, what they are for,
//! and what the store is to say afterwards.
//!
//! The subject and the attribution of every change are checked against each
//! other here. That is what keeps `destroy` honest: a file a `format` run
//! touched must not become a file some entity owns, because the entity's
//! removal would then delete it.

use crate::Result;
use crate::change::{
    ChangeAttribution, DesiredChange, MaintenanceAttribution, decode_all, encode_all,
};
use crate::entity::{EntityId, EntitySpec, OneShotId, OneShotSpec, OwnerId};
use crate::identity::{JavaType, ProjectPath};
use crate::ownership::{DesiredEntity, DesiredState, ReconcileScope};
use crate::resource::{DesiredResource, OneShotLifecycle, OneShotState};
use jails_support::codec::{Decoder, Encoder, ordered};
use std::collections::{BTreeMap, BTreeSet};

// ---------------------------------------------------------------------------
// The change set
// ---------------------------------------------------------------------------

/// What the store is to say afterwards.
///
/// `generation_before` is the guard: the ledger this plan was computed against.
/// Applying against a different generation would write a store that never
/// existed, which is exactly the lost update a plan-then-apply design invites.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LedgerIntent {
    pub generation_before: u64,
    pub entities_after: Vec<DesiredAppliedEntity>,
    pub one_shots_after: Vec<DesiredOneShotReceipt>,
    pub resources_after: Vec<DesiredResource>,
    /// Entities this transition takes out of the store.
    ///
    /// Removal has to be *said* rather than implied. A request speaks for one
    /// scope, so an entity missing from `entities_after` means "this request
    /// has nothing to say about it" -- silence, not deletion. Without this
    /// list, expressing a removal would mean sending the complete state of the
    /// whole store with one request's scope, and every request could then
    /// delete everything it had not heard of.
    ///
    /// Resources need no such list: a resource is owned, so a resource whose
    /// last owner is on this list has lost its last owner, and that is
    /// derivable rather than declarable. Two lists that could disagree about
    /// the same fact is exactly the drift this schema exists to remove.
    pub entities_removed: Vec<EntityId>,
}

impl LedgerIntent {
    /// The store's rows are semantic sets. A duplicate identity would make the
    /// written order decide which of two rows survives.
    pub fn validate(&self) -> Result<()> {
        let mut entities = BTreeSet::new();
        for entity in &self.entities_after {
            if !entity.spec.matches(&entity.id) {
                return Err(format!(
                    "applied entity {:?} pairs an identity and a spec of different kinds",
                    entity.id
                ));
            }
            if entity.owners.is_empty() {
                return Err(format!(
                    "applied entity {:?} has no owner; it should be absent instead",
                    entity.id
                ));
            }
            if !entities.insert(&entity.id) {
                return Err(format!("entity {:?} appears twice", entity.id));
            }
        }
        let mut one_shots = BTreeSet::new();
        for receipt in &self.one_shots_after {
            if !receipt.spec.matches(&receipt.id) {
                return Err(format!(
                    "one-shot receipt {:?} pairs an identity and a spec that disagree",
                    receipt.id
                ));
            }
            if !one_shots.insert(&receipt.id) {
                return Err(format!("one-shot {:?} appears twice", receipt.id));
            }
        }
        let mut resources = BTreeSet::new();
        for resource in &self.resources_after {
            resource.value.agrees_with(&resource.key)?;
            if !resources.insert(&resource.key) {
                return Err(format!("resource {:?} appears twice", resource.key));
            }
        }
        Ok(())
    }

    pub fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.validate()?;
        encoder.u64(self.generation_before);
        encode_all(encoder, &self.entities_after, DesiredAppliedEntity::encode)?;
        encode_all(
            encoder,
            &self.one_shots_after,
            DesiredOneShotReceipt::encode,
        )?;
        encode_all(encoder, &self.resources_after, DesiredResource::encode)?;
        encode_all(encoder, &self.entities_removed, |id, e| id.encode(e))?;
        Ok(())
    }

    pub fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        let generation_before = decoder.u64()?;
        let entities_after = decode_all(decoder, DesiredAppliedEntity::decode)?;
        let one_shots_after = decode_all(decoder, DesiredOneShotReceipt::decode)?;
        let resources_after = decode_all(decoder, DesiredResource::decode)?;
        let entities_removed = decode_all(decoder, EntityId::decode)?;
        let intent = Self {
            generation_before,
            entities_after,
            one_shots_after,
            resources_after,
            entities_removed,
        };
        intent.validate()?;
        Ok(intent)
    }
}

/// An entity row as this plan wants it recorded.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DesiredAppliedEntity {
    pub id: EntityId,
    pub owners: BTreeSet<OwnerId>,
    pub spec: EntitySpec,
}

impl DesiredAppliedEntity {
    pub fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.id.encode(encoder)?;
        encoder.count(self.owners.len())?;
        let mut previous: Option<&OwnerId> = None;
        for owner in &self.owners {
            ordered(previous, owner)?;
            previous = Some(owner);
            encoder.tag(owner.tag());
        }
        self.spec.encode(encoder)
    }

    pub fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        let id = EntityId::decode(decoder)?;
        let count = decoder.count()?;
        let mut owners = BTreeSet::new();
        let mut previous: Option<OwnerId> = None;
        for _ in 0..count {
            let owner = OwnerId::from_tag(decoder.tag()?)?;
            ordered(previous.as_ref(), &owner)?;
            previous = Some(owner);
            owners.insert(owner);
        }
        Ok(Self {
            id,
            owners,
            spec: EntitySpec::decode(decoder)?,
        })
    }
}

/// A one-shot receipt as this plan wants it recorded.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DesiredOneShotReceipt {
    pub id: OneShotId,
    pub spec: OneShotSpec,
    pub state: OneShotState,
    pub lifecycle: OneShotLifecycle,
}

impl DesiredOneShotReceipt {
    pub fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.id.encode(encoder)?;
        self.spec.encode(encoder)?;
        self.state.encode(encoder);
        self.lifecycle.encode(encoder)
    }

    pub fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(Self {
            id: OneShotId::decode(decoder)?,
            spec: OneShotSpec::decode(decoder)?,
            state: OneShotState::decode(decoder)?,
            lifecycle: OneShotLifecycle::decode(decoder)?,
        })
    }
}

/// What the whole invocation is about.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PlannedSubject {
    Reconcile(DesiredState),
    ApplyOneShot {
        id: OneShotId,
        spec: OneShotSpec,
    },
    DestroyCases {
        id: OneShotId,
        force: bool,
    },
    AppInit {
        target: ProjectPath,
    },
    Rename {
        from: JavaType,
        to: JavaType,
        force: bool,
    },
    AdoptLayout,
    Format {
        scopes: BTreeSet<ProjectPath>,
    },
}

impl PlannedSubject {
    /// The maintenance attribution a change under this subject may claim, if
    /// the subject is a maintenance one at all.
    pub fn maintenance(&self) -> Option<MaintenanceAttribution> {
        Some(match self {
            Self::AppInit { .. } => MaintenanceAttribution::AppInit,
            Self::Rename { .. } => MaintenanceAttribution::Rename,
            Self::AdoptLayout => MaintenanceAttribution::AdoptLayout,
            Self::Format { .. } => MaintenanceAttribution::Format,
            Self::Reconcile(_) | Self::ApplyOneShot { .. } | Self::DestroyCases { .. } => {
                return None;
            }
        })
    }
}

/// The ordered changes, what they are for, and what the store is to say.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DesiredChangeSet {
    pub ordered: Vec<DesiredChange>,
    pub subject: PlannedSubject,
    pub ledger_intent: LedgerIntent,
}

impl DesiredChangeSet {
    /// Refuses a maintenance change under a resource subject and vice versa.
    ///
    /// This is the check that keeps `destroy` honest: a file a `format` run
    /// touched must not become a file some entity owns, because the entity's
    /// removal would then delete it.
    pub fn validate(&self) -> Result<()> {
        let maintenance = self.subject.maintenance();
        for change in &self.ordered {
            change.validate()?;
            match (&change.attribution, maintenance) {
                (ChangeAttribution::Maintenance(kind), Some(expected)) if *kind == expected => {}
                (ChangeAttribution::Resource(_), None) => {}
                (ChangeAttribution::Maintenance(kind), _) => {
                    return Err(format!(
                        "a change attributed to {kind:?} appears under a subject that does not \
                         perform it"
                    ));
                }
                (ChangeAttribution::Resource(owner), Some(_)) => {
                    return Err(format!(
                        "a maintenance subject cannot charge a change to {owner:?}"
                    ));
                }
            }
        }
        self.ledger_intent.validate()
    }

    pub fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.validate()?;
        encode_all(encoder, &self.ordered, DesiredChange::encode)?;
        self.subject.encode(encoder)?;
        self.ledger_intent.encode(encoder)
    }

    pub fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        let set = Self {
            ordered: decode_all(decoder, DesiredChange::decode)?,
            subject: PlannedSubject::decode(decoder)?,
            ledger_intent: LedgerIntent::decode(decoder)?,
        };
        set.validate()?;
        Ok(set)
    }
}

impl PlannedSubject {
    fn tag(&self) -> u8 {
        match self {
            Self::Reconcile(_) => 0,
            Self::ApplyOneShot { .. } => 1,
            Self::DestroyCases { .. } => 2,
            Self::AppInit { .. } => 3,
            Self::Rename { .. } => 4,
            Self::AdoptLayout => 5,
            Self::Format { .. } => 7,
        }
    }

    pub fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.tag(self.tag());
        match self {
            Self::Reconcile(state) => encode_desired_state(encoder, state),
            Self::ApplyOneShot { id, spec } => {
                id.encode(encoder)?;
                spec.encode(encoder)
            }
            Self::DestroyCases { id, force } => {
                id.encode(encoder)?;
                encoder.bool(*force);
                Ok(())
            }
            Self::AppInit { target } => target.encode(encoder),
            Self::Rename { from, to, force } => {
                from.encode(encoder)?;
                to.encode(encoder)?;
                encoder.bool(*force);
                Ok(())
            }
            Self::AdoptLayout => Ok(()),
            Self::Format { scopes } => {
                encoder.count(scopes.len())?;
                let mut previous: Option<&ProjectPath> = None;
                for scope in scopes {
                    ordered(previous, scope)?;
                    previous = Some(scope);
                    scope.encode(encoder)?;
                }
                Ok(())
            }
        }
    }

    pub fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(match decoder.tag()? {
            0 => Self::Reconcile(decode_desired_state(decoder)?),
            1 => Self::ApplyOneShot {
                id: OneShotId::decode(decoder)?,
                spec: OneShotSpec::decode(decoder)?,
            },
            2 => Self::DestroyCases {
                id: OneShotId::decode(decoder)?,
                force: decoder.bool()?,
            },
            3 => Self::AppInit {
                target: ProjectPath::decode(decoder)?,
            },
            4 => Self::Rename {
                from: JavaType::decode(decoder)?,
                to: JavaType::decode(decoder)?,
                force: decoder.bool()?,
            },
            5 => Self::AdoptLayout,
            7 => {
                let count = decoder.count()?;
                let mut scopes = BTreeSet::new();
                let mut previous: Option<ProjectPath> = None;
                for _ in 0..count {
                    let scope = ProjectPath::decode(decoder)?;
                    ordered(previous.as_ref(), &scope)?;
                    previous = Some(scope.clone());
                    scopes.insert(scope);
                }
                Self::Format { scopes }
            }
            other => Err(format!("unknown planned subject tag {other}"))?,
        })
    }
}

fn encode_desired_state(encoder: &mut Encoder, state: &DesiredState) -> Result<()> {
    match &state.scope {
        ReconcileScope::AppManifest => encoder.tag(0),
        ReconcileScope::DirectConfig => encoder.tag(1),
        ReconcileScope::DirectEntity(id) => {
            encoder.tag(2);
            id.encode(encoder)?;
        }
    }
    encoder.count(state.entities.len())?;
    let mut previous: Option<&EntityId> = None;
    for (id, entity) in &state.entities {
        ordered(previous, id)?;
        previous = Some(id);
        DesiredAppliedEntity {
            id: entity.id.clone(),
            owners: entity.owners.clone(),
            spec: entity.spec.clone(),
        }
        .encode(encoder)?;
    }
    Ok(())
}

fn decode_desired_state(decoder: &mut Decoder<'_>) -> Result<DesiredState> {
    let scope = match decoder.tag()? {
        0 => ReconcileScope::AppManifest,
        1 => ReconcileScope::DirectConfig,
        2 => ReconcileScope::DirectEntity(EntityId::decode(decoder)?),
        other => Err(format!("unknown reconcile scope tag {other}"))?,
    };
    let count = decoder.count()?;
    let mut entities = BTreeMap::new();
    let mut previous: Option<EntityId> = None;
    for _ in 0..count {
        let row = DesiredAppliedEntity::decode(decoder)?;
        ordered(previous.as_ref(), &row.id)?;
        previous = Some(row.id.clone());
        entities.insert(
            row.id.clone(),
            DesiredEntity {
                id: row.id,
                spec: row.spec,
                owners: row.owners,
            },
        );
    }
    DesiredState::new(scope, entities)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::change::tests::{dependency_change, intent, path};

    pub(crate) fn change_set(subject: PlannedSubject, change: DesiredChange) -> DesiredChangeSet {
        DesiredChangeSet {
            ordered: vec![change],
            subject,
            ledger_intent: LedgerIntent {
                generation_before: 4,
                entities_after: Vec::new(),
                one_shots_after: Vec::new(),
                resources_after: Vec::new(),
                entities_removed: Vec::new(),
            },
        }
    }

    /// A file `format` touched must not become a file an entity owns, because
    /// removing that entity would then delete it.
    #[test]
    fn a_maintenance_subject_cannot_charge_a_change_to_an_owner() {
        let set = change_set(
            PlannedSubject::Format {
                scopes: BTreeSet::from([path("src/main/java")]),
            },
            dependency_change(),
        );
        let error = set.validate().unwrap_err();
        assert!(error.contains("cannot charge a change"), "{error}");
    }

    #[test]
    fn a_resource_subject_cannot_carry_a_maintenance_change() {
        let state = DesiredState::new(
            crate::ownership::ReconcileScope::AppManifest,
            BTreeMap::new(),
        )
        .unwrap();
        let set = change_set(
            PlannedSubject::Reconcile(state),
            DesiredChange::maintenance(MaintenanceAttribution::Format),
        );
        let error = set.validate().unwrap_err();
        assert!(error.contains("does not perform it"), "{error}");
    }

    #[test]
    fn a_change_set_round_trips_through_its_subject_and_ledger_intent() {
        let mut entities = BTreeMap::new();
        let id = EntityId::Intent(intent("Note"));
        entities.insert(
            id.clone(),
            crate::ownership::DesiredEntity {
                id: id.clone(),
                spec: EntitySpec::Intent(crate::declaration::IntentSpec::default()),
                owners: BTreeSet::from([OwnerId::AppManifest]),
            },
        );
        let state =
            DesiredState::new(crate::ownership::ReconcileScope::AppManifest, entities).unwrap();
        let set = change_set(PlannedSubject::Reconcile(state), dependency_change());

        let mut encoder = Encoder::new();
        set.encode(&mut encoder).unwrap();
        let bytes = encoder.finish().unwrap();
        let mut decoder = Decoder::new(&bytes).unwrap();
        assert_eq!(DesiredChangeSet::decode(&mut decoder).unwrap(), set);
        decoder.finish().unwrap();
    }

    #[test]
    fn a_ledger_intent_refuses_one_entity_twice() {
        let id = EntityId::Intent(intent("Note"));
        let row = DesiredAppliedEntity {
            id: id.clone(),
            owners: BTreeSet::from([OwnerId::AppManifest]),
            spec: EntitySpec::Intent(crate::declaration::IntentSpec::default()),
        };
        let intent_rows = LedgerIntent {
            generation_before: 1,
            entities_after: vec![row.clone(), row],
            one_shots_after: Vec::new(),
            resources_after: Vec::new(),
            entities_removed: Vec::new(),
        };
        assert!(
            intent_rows
                .validate()
                .unwrap_err()
                .contains("appears twice")
        );
    }
}
