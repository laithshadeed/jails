use crate::Result;
use crate::application::RoutePath;
use crate::declaration::{FieldSpec, FieldType};
use crate::entity::EntityId;
use crate::identity::{JavaType, Name, ObjectId, OperationId, ProjectPath};
use crate::lifecycle::RenameCampaignId;
use jails_support::codec::{self, Codec, Decoder, Encoder};

pub use crate::identity::SqlName;

/// The explicit storage decision attached to retirement of a table-backed
/// resource. `force` is deliberately separate and cannot choose either arm.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageRetirement {
    Preserve { expected_table: SqlName },
    Drop { confirmed_table: SqlName },
}

impl Codec for StorageRetirement {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        match self {
            Self::Preserve { expected_table } => {
                encoder.tag(0);
                expected_table.encode(encoder)
            }
            Self::Drop { confirmed_table } => {
                encoder.tag(1);
                confirmed_table.encode(encoder)
            }
        }
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(match decoder.tag()? {
            0 => Self::Preserve {
                expected_table: SqlName::decode(decoder)?,
            },
            1 => Self::Drop {
                confirmed_table: SqlName::decode(decoder)?,
            },
            other => Err(format!(
                "unknown storage retirement tag {other}.\n       \
                 fix: use the jails version that wrote this state, or restore its `.jails` data \
                 from a compatible backup."
            ))?,
        })
    }
}

/// The resolved source identity of an entity at a lifecycle boundary.
pub type EntityPath = JavaType;
/// Stable field identity in the currently declared entity model.
pub type FieldId = Name;
/// Validated name assigned by a rename action.
pub type FieldName = Name;

/// The explicit storage transition selected for a logical resource rename.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RenameStrategy {
    PreserveTable,
    SingleCutover,
    Rolling,
}

impl Codec for RenameStrategy {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.tag(match self {
            Self::PreserveTable => 0,
            Self::SingleCutover => 1,
            Self::Rolling => 2,
        });
        Ok(())
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(match decoder.tag()? {
            0 => Self::PreserveTable,
            1 => Self::SingleCutover,
            2 => Self::Rolling,
            other => Err(format!(
                "unknown resource rename strategy tag {other}.\n       fix: upgrade jails or restore compatible `.jails` state"
            ))?,
        })
    }
}

/// Whether externally visible names participate in a logical rename.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExternalRenamePolicy {
    Preserve,
    Rename,
}

impl Codec for ExternalRenamePolicy {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.tag(match self {
            Self::Preserve => 0,
            Self::Rename => 1,
        });
        Ok(())
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(match decoder.tag()? {
            0 => Self::Preserve,
            1 => Self::Rename,
            other => Err(format!(
                "unknown external rename policy tag {other}.\n       fix: upgrade jails or restore compatible `.jails` state"
            ))?,
        })
    }
}

/// A resource rename after its selector has resolved to one durable entity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenameResourceRequestV1 {
    pub entity: EntityId,
    pub expected_path: EntityPath,
    pub new_name: Name,
    pub strategy: RenameStrategy,
    pub target_table: Option<SqlName>,
    pub api: ExternalRenamePolicy,
    pub target_route: Option<RoutePath>,
}

impl RenameResourceRequestV1 {
    pub fn validate(&self) -> Result<()> {
        if self.target_route.is_some() && self.api != ExternalRenamePolicy::Rename {
            return Err("a target route requires `api=rename`.\n       fix: pass `--api rename --route <path>`, or omit `--route` to preserve external names".into());
        }
        Ok(())
    }

    pub fn campaign_id(&self) -> Result<RenameCampaignId> {
        let mut encoder = Encoder::new();
        self.encode(&mut encoder)?;
        Ok(RenameCampaignId::from_object(ObjectId::from_bytes(
            codec::domain_hash("JAILS-RENAME-CAMPAIGN-1", &encoder.finish()?),
        )))
    }
}

impl Codec for RenameResourceRequestV1 {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.validate()?;
        self.entity.encode(encoder)?;
        self.expected_path.encode(encoder)?;
        self.new_name.encode(encoder)?;
        self.strategy.encode(encoder)?;
        encoder.option(self.target_table.as_ref(), |encoder, table| {
            table.encode(encoder)
        })?;
        self.api.encode(encoder)?;
        encoder.option(self.target_route.as_ref(), |encoder, route| {
            route.encode(encoder)
        })
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        let request = Self {
            entity: EntityId::decode(decoder)?,
            expected_path: JavaType::decode(decoder)?,
            new_name: Name::decode(decoder)?,
            strategy: RenameStrategy::decode(decoder)?,
            target_table: decoder.option(SqlName::decode)?,
            api: ExternalRenamePolicy::decode(decoder)?,
            target_route: decoder.option(RoutePath::decode)?,
        };
        request.validate()?;
        Ok(request)
    }
}

/// Attested completion of the storage half of a rolling resource rename.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompleteStorageRenameRequestV1 {
    pub entity: EntityId,
    pub campaign: RenameCampaignId,
    pub expected_path: EntityPath,
    pub current_table: SqlName,
    pub target_table: SqlName,
    pub code_stage_receipt: OperationId,
    pub old_version_retired: bool,
}

impl CompleteStorageRenameRequestV1 {
    pub fn validate(&self) -> Result<()> {
        if !self.old_version_retired {
            return Err("rolling storage completion requires the old-version-retired attestation.\n       fix: retire the old application version, then pass `--old-version-retired`".into());
        }
        if self.current_table == self.target_table {
            return Err("rolling storage completion has identical current and target tables.\n       fix: inspect the active campaign and use its exact table identities".into());
        }
        Ok(())
    }
}

impl Codec for CompleteStorageRenameRequestV1 {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.validate()?;
        self.entity.encode(encoder)?;
        self.campaign.encode(encoder)?;
        self.expected_path.encode(encoder)?;
        self.current_table.encode(encoder)?;
        self.target_table.encode(encoder)?;
        self.code_stage_receipt.encode(encoder)?;
        encoder.bool(self.old_version_retired);
        Ok(())
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        let request = Self {
            entity: EntityId::decode(decoder)?,
            campaign: RenameCampaignId::decode(decoder)?,
            expected_path: JavaType::decode(decoder)?,
            current_table: SqlName::decode(decoder)?,
            target_table: SqlName::decode(decoder)?,
            code_stage_receipt: OperationId::decode(decoder)?,
            old_version_retired: decoder.bool()?,
        };
        request.validate()?;
        Ok(request)
    }
}

/// A typed constant written lexically at the CLI boundary.
///
/// The planner validates it against the affected [`FieldType`]; retaining the
/// exact lexical form here keeps request fingerprints distinct without
/// smuggling SQL into the protocol.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypedLiteral(String);

impl TypedLiteral {
    pub fn parse(value: &str) -> Result<Self> {
        if value.contains(['\n', '\r', '\0']) {
            return Err("a typed literal must be one NUL-free line.\n       fix: pass one lexical value; use --backfill-file for SQL".into());
        }
        Ok(Self(value.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Codec for TypedLiteral {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.string(&self.0)
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Self::parse(&decoder.string()?)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColumnRenamePolicy {
    Preserve,
    SingleCutover,
    Rolling,
}

impl Codec for ColumnRenamePolicy {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.tag(match self {
            Self::Preserve => 0,
            Self::SingleCutover => 1,
            Self::Rolling => 2,
        });
        Ok(())
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(match decoder.tag()? {
            0 => Self::Preserve,
            1 => Self::SingleCutover,
            2 => Self::Rolling,
            other => Err(format!(
                "unknown column rename policy tag {other}.\n       fix: upgrade jails or restore compatible `.jails` state"
            ))?,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TypeChangeStrategy {
    Safe,
    ExpandContract,
}

impl Codec for TypeChangeStrategy {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.tag(match self {
            Self::Safe => 0,
            Self::ExpandContract => 1,
        });
        Ok(())
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(match decoder.tag()? {
            0 => Self::Safe,
            1 => Self::ExpandContract,
            other => Err(format!(
                "unknown type change strategy tag {other}.\n       fix: upgrade jails or restore compatible `.jails` state"
            ))?,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FieldEvolution {
    Add(FieldSpec),
    Rename {
        field: FieldId,
        new_name: FieldName,
        column: ColumnRenamePolicy,
    },
    ChangeType {
        field: FieldId,
        to: FieldType,
        strategy: TypeChangeStrategy,
    },
    SetNullability {
        field: FieldId,
        nullable: bool,
    },
    Drop {
        field: FieldId,
        confirmed_column: SqlName,
    },
}

impl Codec for FieldEvolution {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        match self {
            Self::Add(field) => {
                encoder.tag(0);
                field.encode(encoder)
            }
            Self::Rename {
                field,
                new_name,
                column,
            } => {
                encoder.tag(1);
                field.encode(encoder)?;
                new_name.encode(encoder)?;
                column.encode(encoder)
            }
            Self::ChangeType {
                field,
                to,
                strategy,
            } => {
                encoder.tag(2);
                field.encode(encoder)?;
                to.encode(encoder)?;
                strategy.encode(encoder)
            }
            Self::SetNullability { field, nullable } => {
                encoder.tag(3);
                field.encode(encoder)?;
                encoder.bool(*nullable);
                Ok(())
            }
            Self::Drop {
                field,
                confirmed_column,
            } => {
                encoder.tag(4);
                field.encode(encoder)?;
                confirmed_column.encode(encoder)
            }
        }
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(match decoder.tag()? {
            0 => Self::Add(FieldSpec::decode(decoder)?),
            1 => Self::Rename {
                field: Name::decode(decoder)?,
                new_name: Name::decode(decoder)?,
                column: ColumnRenamePolicy::decode(decoder)?,
            },
            2 => Self::ChangeType {
                field: Name::decode(decoder)?,
                to: FieldType::decode(decoder)?,
                strategy: TypeChangeStrategy::decode(decoder)?,
            },
            3 => Self::SetNullability {
                field: Name::decode(decoder)?,
                nullable: decoder.bool()?,
            },
            4 => Self::Drop {
                field: Name::decode(decoder)?,
                confirmed_column: SqlName::decode(decoder)?,
            },
            other => Err(format!(
                "unknown field evolution tag {other}.\n       fix: upgrade jails or restore compatible `.jails` state"
            ))?,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataEvolution {
    None,
    TypedLiteral(TypedLiteral),
    ReaderOwnedSql(ProjectPath),
}

impl Codec for DataEvolution {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        match self {
            Self::None => {
                encoder.tag(0);
                Ok(())
            }
            Self::TypedLiteral(value) => {
                encoder.tag(1);
                value.encode(encoder)
            }
            Self::ReaderOwnedSql(path) => {
                encoder.tag(2);
                path.encode(encoder)
            }
        }
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(match decoder.tag()? {
            0 => Self::None,
            1 => Self::TypedLiteral(TypedLiteral::decode(decoder)?),
            2 => Self::ReaderOwnedSql(ProjectPath::decode(decoder)?),
            other => Err(format!(
                "unknown data evolution tag {other}.\n       fix: upgrade jails or restore compatible `.jails` state"
            ))?,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvolveFieldRequestV1 {
    pub entity: EntityId,
    pub expected_path: EntityPath,
    pub expected_table: SqlName,
    pub action: FieldEvolution,
    pub data: DataEvolution,
}

impl Codec for EvolveFieldRequestV1 {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.entity.encode(encoder)?;
        self.expected_path.encode(encoder)?;
        self.expected_table.encode(encoder)?;
        self.action.encode(encoder)?;
        self.data.encode(encoder)
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(Self {
            entity: EntityId::decode(decoder)?,
            expected_path: JavaType::decode(decoder)?,
            expected_table: SqlName::decode(decoder)?,
            action: FieldEvolution::decode(decoder)?,
            data: DataEvolution::decode(decoder)?,
        })
    }
}

/// Named datasource reference. It is an identifier, never a URL or secret.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct DatasourceRef(String);

impl DatasourceRef {
    pub fn parse(value: &str) -> Result<Self> {
        if value.is_empty()
            || !value
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
        {
            return Err(format!(
                "`{value}` is not a datasource name.\n       fix: use letters, digits, `_`, or `-`"
            )
            .into());
        }
        Ok(Self(value.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Codec for DatasourceRef {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.string(&self.0)
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Self::parse(&decoder.string()?)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DestroyResourceRequestV2 {
    pub entity: EntityId,
    pub expected_path: EntityPath,
    pub storage: StorageRetirement,
    pub migration_effect: Option<DatasourceRef>,
}

impl Codec for DestroyResourceRequestV2 {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.entity.encode(encoder)?;
        self.expected_path.encode(encoder)?;
        self.storage.encode(encoder)?;
        encoder.option(self.migration_effect.as_ref(), |encoder, value| {
            value.encode(encoder)
        })
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(Self {
            entity: EntityId::decode(decoder)?,
            expected_path: JavaType::decode(decoder)?,
            storage: StorageRetirement::decode(decoder)?,
            migration_effect: decoder.option(DatasourceRef::decode)?,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReviveResourceRequestV1 {
    pub entity: EntityId,
    pub expected_table: SqlName,
}

impl Codec for ReviveResourceRequestV1 {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.entity.encode(encoder)?;
        self.expected_table.encode(encoder)
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(Self {
            entity: EntityId::decode(decoder)?,
            expected_table: SqlName::decode(decoder)?,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RepairStrategy {
    RollForward,
}

impl Codec for RepairStrategy {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.tag(0);
        Ok(())
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        match decoder.tag()? {
            0 => Ok(Self::RollForward),
            other => Err(format!("unknown resource repair strategy tag {other}.\n       fix: upgrade jails or restore compatible `.jails` state").into()),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RepairResourceRequestV1 {
    pub entity: EntityId,
    pub expected_path: EntityPath,
    pub strategy: RepairStrategy,
    pub datasource: Option<DatasourceRef>,
}

impl Codec for RepairResourceRequestV1 {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.entity.encode(encoder)?;
        self.expected_path.encode(encoder)?;
        self.strategy.encode(encoder)?;
        encoder.option(self.datasource.as_ref(), |encoder, value| {
            value.encode(encoder)
        })
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(Self {
            entity: EntityId::decode(decoder)?,
            expected_path: JavaType::decode(decoder)?,
            strategy: RepairStrategy::decode(decoder)?,
            datasource: decoder.option(DatasourceRef::decode)?,
        })
    }
}
