//! Durable resource identity, retirement state, and append-only migration seals.

use crate::Result;
use crate::entity::{EntityId, EntitySpec};
use crate::identity::{JavaType, ObjectId, OperationId, ProjectPath, SqlName};
use jails_support::codec::{Codec, Decoder, Encoder, ordered};
use std::collections::BTreeSet;

pub type ReceiptId = OperationId;

/// Stable identifier for one durable rolling storage-rename campaign.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Hash, jails_codec_derive::Codec)]
pub struct RenameCampaignId(ObjectId);

impl RenameCampaignId {
    pub fn from_object(object: ObjectId) -> Self {
        Self(object)
    }

    pub fn parse_hex(text: &str) -> Result<Self> {
        ObjectId::parse_hex(text).map(Self)
    }

    pub fn to_hex(self) -> String {
        self.0.to_hex()
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct MigrationVersion(u32);

impl MigrationVersion {
    pub fn new(value: u32) -> Result<Self> {
        if value == 0 {
            return Err("migration version is zero.\n       fix: versions start at one".into());
        }
        Ok(Self(value))
    }

    pub fn get(self) -> u32 {
        self.0
    }
}

impl Codec for MigrationVersion {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.u32(self.0);
        Ok(())
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Self::new(decoder.u32()?)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, jails_codec_derive::Codec)]
pub struct TableBinding {
    pub table: SqlName,
}

#[derive(Clone, Debug, Eq, PartialEq, jails_codec_derive::Codec)]
pub enum ResourceState {
    #[codec(tag = 0)]
    Active,
    #[codec(tag = 1)]
    RetiredPreservingStorage { retired_by: ReceiptId },
    #[codec(tag = 2)]
    RetiredDropPlanned {
        migration: ProjectPath,
        retired_by: ReceiptId,
    },
    #[codec(tag = 3)]
    RenamePending {
        campaign: RenameCampaignId,
        from_logical: JavaType,
        to_logical: JavaType,
        current_table: SqlName,
        target_table: SqlName,
        code_stage_receipt: ReceiptId,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, jails_codec_derive::Codec)]
pub struct MigrationSealV1 {
    pub version: MigrationVersion,
    pub path: ProjectPath,
    pub content_digest: ObjectId,
    pub contributors: BTreeSet<EntityId>,
    pub receipt: ReceiptId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResourceLifecycleV1 {
    pub entity: EntityId,
    pub expected_path: JavaType,
    /// Last declared model, retained so preserve/revive is reversible.
    pub last_spec: EntitySpec,
    pub state: ResourceState,
    pub table: Option<TableBinding>,
    pub migrations: Vec<MigrationSealV1>,
}

impl Codec for ResourceLifecycleV1 {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.entity.encode(encoder)?;
        self.expected_path.encode(encoder)?;
        self.last_spec.encode(encoder)?;
        self.state.encode(encoder)?;
        encoder.option(self.table.as_ref(), |encoder, table| table.encode(encoder))?;
        encoder.count(self.migrations.len())?;
        let mut previous = None;
        for seal in &self.migrations {
            ordered(previous, &seal.version)?;
            previous = Some(&seal.version);
            seal.encode(encoder)?;
        }
        Ok(())
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        let entity = EntityId::decode(decoder)?;
        let expected_path = JavaType::decode(decoder)?;
        let last_spec = EntitySpec::decode(decoder)?;
        let state = ResourceState::decode(decoder)?;
        let table = decoder.option(TableBinding::decode)?;
        let count = decoder.count()?;
        let mut migrations = Vec::new();
        for _ in 0..count {
            let seal = MigrationSealV1::decode(decoder)?;
            ordered(
                migrations
                    .last()
                    .map(|last: &MigrationSealV1| &last.version),
                &seal.version,
            )?;
            migrations.push(seal);
        }
        Ok(Self {
            entity,
            expected_path,
            last_spec,
            state,
            table,
            migrations,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::declaration::{FieldSpec, IntentArguments, IntentSpec};
    use crate::entity::{IntentId, Recipe};
    use crate::identity::{Name, Package};
    use jails_support::codec;

    fn lifecycle() -> ResourceLifecycleV1 {
        let id = IntentId::new(
            Recipe::Scaffold,
            Name::parse("WorkItem").unwrap(),
            Package::parse("com.example.domain").unwrap(),
        );
        let entity = EntityId::Intent(id);
        ResourceLifecycleV1 {
            entity: entity.clone(),
            expected_path: JavaType::parse("com.example.domain.WorkItem").unwrap(),
            last_spec: EntitySpec::Intent(IntentSpec {
                arguments: IntentArguments::Fields(vec![
                    FieldSpec::parse("title:string", &Package::base()).unwrap(),
                ]),
                ..IntentSpec::default()
            }),
            state: ResourceState::RetiredPreservingStorage {
                retired_by: OperationId::from_bytes([7; codec::DIGEST_BYTES]),
            },
            table: Some(TableBinding {
                table: SqlName::parse("work_items").unwrap(),
            }),
            migrations: vec![MigrationSealV1 {
                version: MigrationVersion::new(1).unwrap(),
                path: ProjectPath::parse(
                    "src/main/resources/db/migration/V001__create_work_items.sql",
                )
                .unwrap(),
                content_digest: ObjectId::from_bytes([3; codec::DIGEST_BYTES]),
                contributors: BTreeSet::from([entity]),
                receipt: OperationId::from_bytes([5; codec::DIGEST_BYTES]),
            }],
        }
    }

    #[test]
    fn resource_lifecycle_round_trips_with_identity_model_and_lineage() {
        let value = lifecycle();
        let mut encoder = Encoder::new();
        value.encode(&mut encoder).unwrap();
        let bytes = encoder.finish().unwrap();
        let mut decoder = Decoder::new(&bytes).unwrap();
        assert_eq!(ResourceLifecycleV1::decode(&mut decoder).unwrap(), value);
        decoder.finish().unwrap();
    }

    #[test]
    fn migration_seals_are_strictly_ordered() {
        let mut value = lifecycle();
        value.migrations.push(value.migrations[0].clone());
        let mut encoder = Encoder::new();
        let error = value.encode(&mut encoder).unwrap_err();
        assert!(error.to_string().contains("duplicate key"), "{error}");
    }

    #[test]
    fn a_rolling_campaign_round_trips_inside_the_resource_lifecycle() {
        let mut value = lifecycle();
        value.state = ResourceState::RenamePending {
            campaign: RenameCampaignId::from_object(ObjectId::from_bytes([9; codec::DIGEST_BYTES])),
            from_logical: JavaType::parse("com.example.domain.Task").unwrap(),
            to_logical: JavaType::parse("com.example.domain.WorkItem").unwrap(),
            current_table: SqlName::parse("tasks").unwrap(),
            target_table: SqlName::parse("work_items").unwrap(),
            code_stage_receipt: OperationId::from_bytes([8; codec::DIGEST_BYTES]),
        };
        let mut encoder = Encoder::new();
        value.encode(&mut encoder).unwrap();
        let bytes = encoder.finish().unwrap();
        let mut decoder = Decoder::new(&bytes).unwrap();
        assert_eq!(ResourceLifecycleV1::decode(&mut decoder).unwrap(), value);
        decoder.finish().unwrap();
    }
}
