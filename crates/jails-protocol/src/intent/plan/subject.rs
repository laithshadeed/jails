//! The semantic subject carried through planning and preparation.

use super::DesiredAppliedEntity;
use crate::Result;
use crate::change::MaintenanceAttribution;
use crate::database::QueryId;
use crate::entity::{EntityId, OneShotId, OneShotSpec};
use crate::identity::{JavaType, ProjectPath};
use crate::ownership::{DesiredEntity, DesiredState, ReconcileScope};
use crate::request::{
    CompleteStorageRenameRequestV1, DestroyResourceRequestV2, EvolveFieldRequestV1,
    RenameResourceRequestV1, RepairResourceRequestV1, ReviveResourceRequestV1, UndoFilesRequestV1,
};
use jails_support::codec::{Codec, Decoder, Encoder, MAX_PROTOCOL_RECORD, ordered};
use std::collections::{BTreeMap, BTreeSet};

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
    RenameResource(Box<RenameResourceRequestV1>),
    CompleteStorageRename(Box<CompleteStorageRenameRequestV1>),
    AdoptLayout,
    Format {
        scopes: BTreeSet<ProjectPath>,
    },
    EvolveField(Box<EvolveFieldRequestV1>),
    DestroyResourceV2(Box<DestroyResourceRequestV2>),
    ReviveResource(Box<ReviveResourceRequestV1>),
    RepairResource(Box<RepairResourceRequestV1>),
    GenerateQueries {
        queries: BTreeSet<QueryId>,
    },
    ContractProjection {
        target: ProjectPath,
        json_schema: bool,
    },
    UndoFiles(Box<UndoFilesPlanV1>),
}

/// The authenticated state needed to restore a receipt without re-deriving it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UndoFilesPlanV1 {
    pub request: UndoFilesRequestV1,
    pub state_before: Option<Vec<u8>>,
}

impl Codec for UndoFilesPlanV1 {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.request.encode(encoder)?;
        encoder.option(self.state_before.as_ref(), |encoder, state| {
            encoder.object(state, MAX_PROTOCOL_RECORD as u64)
        })
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(Self {
            request: UndoFilesRequestV1::decode(decoder)?,
            state_before: decoder.option(|decoder| decoder.object(MAX_PROTOCOL_RECORD as u64))?,
        })
    }
}

impl PlannedSubject {
    /// The maintenance attribution a change under this subject may claim, if
    /// the subject is a maintenance one at all.
    pub fn maintenance(&self) -> Option<MaintenanceAttribution> {
        Some(match self {
            Self::AppInit { .. } => MaintenanceAttribution::AppInit,
            Self::Rename { .. } => MaintenanceAttribution::Rename,
            Self::RenameResource(_) => MaintenanceAttribution::Rename,
            Self::CompleteStorageRename(_) => MaintenanceAttribution::Rename,
            Self::AdoptLayout => MaintenanceAttribution::AdoptLayout,
            Self::Format { .. } => MaintenanceAttribution::Format,
            Self::ContractProjection { .. } => MaintenanceAttribution::ContractProjection,
            Self::UndoFiles(_) => MaintenanceAttribution::Undo,
            Self::Reconcile(_)
            | Self::ApplyOneShot { .. }
            | Self::DestroyCases { .. }
            | Self::EvolveField(_)
            | Self::DestroyResourceV2(_)
            | Self::ReviveResource(_)
            | Self::RepairResource(_) => return None,
            Self::GenerateQueries { .. } => return None,
        })
    }

    fn tag(&self) -> u8 {
        match self {
            Self::Reconcile(_) => 0,
            Self::ApplyOneShot { .. } => 1,
            Self::DestroyCases { .. } => 2,
            Self::AppInit { .. } => 3,
            Self::Rename { .. } => 4,
            Self::AdoptLayout => 5,
            Self::Format { .. } => 7,
            Self::EvolveField(_) => 8,
            Self::DestroyResourceV2(_) => 9,
            Self::ReviveResource(_) => 10,
            Self::RepairResource(_) => 11,
            Self::GenerateQueries { .. } => 12,
            Self::ContractProjection { .. } => 13,
            Self::RenameResource(_) => 14,
            Self::CompleteStorageRename(_) => 15,
            Self::UndoFiles(_) => 16,
        }
    }
}

impl Codec for PlannedSubject {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
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
            Self::RenameResource(request) => request.encode(encoder),
            Self::CompleteStorageRename(request) => request.encode(encoder),
            Self::UndoFiles(plan) => plan.encode(encoder),
            Self::AdoptLayout => Ok(()),
            Self::Format { scopes } => {
                encoder.set(scopes)?;
                Ok(())
            }
            Self::EvolveField(request) => request.encode(encoder),
            Self::DestroyResourceV2(request) => request.encode(encoder),
            Self::ReviveResource(request) => request.encode(encoder),
            Self::RepairResource(request) => request.encode(encoder),
            Self::GenerateQueries { queries } => encoder.set(queries),
            Self::ContractProjection {
                target,
                json_schema,
            } => {
                target.encode(encoder)?;
                encoder.bool(*json_schema);
                Ok(())
            }
        }
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
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
            7 => Self::Format {
                scopes: decoder.set()?,
            },
            8 => Self::EvolveField(Box::new(EvolveFieldRequestV1::decode(decoder)?)),
            9 => Self::DestroyResourceV2(Box::new(DestroyResourceRequestV2::decode(decoder)?)),
            10 => Self::ReviveResource(Box::new(ReviveResourceRequestV1::decode(decoder)?)),
            11 => Self::RepairResource(Box::new(RepairResourceRequestV1::decode(decoder)?)),
            12 => Self::GenerateQueries {
                queries: decoder.set()?,
            },
            13 => Self::ContractProjection {
                target: ProjectPath::decode(decoder)?,
                json_schema: decoder.bool()?,
            },
            14 => Self::RenameResource(Box::new(RenameResourceRequestV1::decode(decoder)?)),
            15 => Self::CompleteStorageRename(Box::new(CompleteStorageRenameRequestV1::decode(
                decoder,
            )?)),
            16 => Self::UndoFiles(Box::new(UndoFilesPlanV1::decode(decoder)?)),
            other => Err(format!(
                "unknown planned subject tag {other}.\n       fix: upgrade jails or restore \
                 compatible `.jails` state"
            ))?,
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
        other => Err(format!(
            "unknown reconcile scope tag {other}.\n       fix: upgrade jails or restore \
             compatible `.jails` state"
        ))?,
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
mod tests {
    use super::*;
    use crate::database::{QueryName, SliceName};
    use crate::declaration::FieldSpec;
    use crate::entity::{IntentId, Recipe};
    use crate::identity::{Name, Package, SqlName};
    use crate::request::{DataEvolution, FieldEvolution, RepairStrategy, StorageRetirement};

    fn entity() -> EntityId {
        EntityId::Intent(IntentId::new(
            Recipe::Scaffold,
            Name::parse("Note").unwrap(),
            Package::parse("com.example.domain").unwrap(),
        ))
    }

    fn round_trip(subject: PlannedSubject) {
        let mut encoder = Encoder::new();
        subject.encode(&mut encoder).unwrap();
        let bytes = encoder.finish().unwrap();
        let mut decoder = Decoder::new(&bytes).unwrap();
        assert_eq!(PlannedSubject::decode(&mut decoder).unwrap(), subject);
        decoder.finish().unwrap();
    }

    #[test]
    fn lifecycle_subjects_round_trip_under_distinct_tags() {
        let entity = entity();
        let path = JavaType::parse("com.example.domain.Note").unwrap();
        let table = SqlName::parse("notes").unwrap();
        round_trip(PlannedSubject::EvolveField(Box::new(
            EvolveFieldRequestV1 {
                entity: entity.clone(),
                expected_path: path.clone(),
                expected_table: Some(table.clone()),
                action: FieldEvolution::Add(
                    FieldSpec::parse("body:string", &Package::base()).unwrap(),
                ),
                data: DataEvolution::None,
            },
        )));
        round_trip(PlannedSubject::DestroyResourceV2(Box::new(
            DestroyResourceRequestV2 {
                entity: entity.clone(),
                expected_path: path.clone(),
                storage: StorageRetirement::Preserve {
                    expected_table: table.clone(),
                },
                migration_effect: None,
            },
        )));
        round_trip(PlannedSubject::ReviveResource(Box::new(
            ReviveResourceRequestV1 {
                entity: entity.clone(),
                expected_table: table,
            },
        )));
        round_trip(PlannedSubject::RepairResource(Box::new(
            RepairResourceRequestV1 {
                entity,
                expected_path: path,
                strategy: RepairStrategy::RollForward,
                datasource: None,
            },
        )));
        round_trip(PlannedSubject::GenerateQueries {
            queries: BTreeSet::from([QueryId::new(
                SliceName::parse("Billing").unwrap(),
                QueryName::parse("FindPayableOrders").unwrap(),
            )]),
        });
    }
}
