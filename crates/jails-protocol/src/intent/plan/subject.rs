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
#[derive(Clone, Debug, Eq, PartialEq, jails_codec_derive::Codec)]
#[codec(unknown_fix = "upgrade jails or restore compatible `.jails` state")]
pub enum PlannedSubject {
    #[codec(tag = 0)]
    Reconcile(DesiredState),
    #[codec(tag = 1)]
    ApplyOneShot { id: OneShotId, spec: OneShotSpec },
    #[codec(tag = 2)]
    DestroyCases { id: OneShotId, force: bool },
    #[codec(tag = 3)]
    AppInit { target: ProjectPath },
    #[codec(tag = 4)]
    Rename {
        from: JavaType,
        to: JavaType,
        force: bool,
    },
    #[codec(tag = 14)]
    RenameResource(Box<RenameResourceRequestV1>),
    #[codec(tag = 15)]
    CompleteStorageRename(Box<CompleteStorageRenameRequestV1>),
    #[codec(tag = 5)]
    AdoptLayout,
    #[codec(tag = 7)]
    Format { scopes: BTreeSet<ProjectPath> },
    #[codec(tag = 8)]
    EvolveField(Box<EvolveFieldRequestV1>),
    #[codec(tag = 9)]
    DestroyResourceV2(Box<DestroyResourceRequestV2>),
    #[codec(tag = 10)]
    ReviveResource(Box<ReviveResourceRequestV1>),
    #[codec(tag = 11)]
    RepairResource(Box<RepairResourceRequestV1>),
    #[codec(tag = 12)]
    GenerateQueries { queries: BTreeSet<QueryId> },
    #[codec(tag = 13)]
    ContractProjection {
        target: ProjectPath,
        json_schema: bool,
    },
    #[codec(tag = 16)]
    UndoFiles(Box<UndoFilesPlanV1>),
    #[codec(tag = 17)]
    Modernize { files: BTreeSet<ProjectPath> },
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
            Self::Modernize { .. } => MaintenanceAttribution::Modernize,
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
}

/// The reconcile scope and the entities it declares.
///
/// This lived beside its one caller as a pair of free functions, which is
/// why `PlannedSubject` had to be written by hand: a derive can only reach a
/// field's encoding through [`Codec`]. Stating it on the type is what lets
/// the enum above be derived, and the bytes are the ones those functions
/// wrote -- `decode` still goes through [`DesiredState::new`], so a value a
/// recovered journal carries is one the constructor accepted.
impl Codec for DesiredState {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        match &self.scope {
            ReconcileScope::AppManifest => encoder.tag(0),
            ReconcileScope::DirectConfig => encoder.tag(1),
            ReconcileScope::DirectEntity(id) => {
                encoder.tag(2);
                id.encode(encoder)?;
            }
        }
        encoder.count(self.entities.len())?;
        let mut previous: Option<&EntityId> = None;
        for (id, entity) in &self.entities {
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

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
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
