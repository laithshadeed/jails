//! Migration-derived catalog facts and explicit opaque blockers.

use super::{ByteSpan, SchemaObjectId, SqlDialect, SqlTypeName};
use crate::Result;
use crate::identity::{ObjectId, ProjectPath, SqlName};
use jails_support::codec::{Codec, Decoder, Encoder, domain_hash};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SchemaObject {
    Schema,
    Table,
    Column {
        sql_type: SqlTypeName,
        nullable: bool,
        ordinal: u32,
        default_expression: Option<String>,
        generated: Option<String>,
        identity: Option<String>,
        comment: Option<String>,
    },
    PrimaryKey {
        columns: Vec<SqlName>,
    },
    ForeignKey {
        definition: String,
        referenced_table: SchemaObjectId,
    },
    Unique {
        definition: String,
    },
    Index {
        definition: String,
    },
    Check {
        definition: String,
    },
    Enum {
        labels: Vec<String>,
    },
    Domain {
        definition: String,
    },
    View {
        definition: String,
    },
    Routine {
        definition: String,
    },
    Policy {
        definition: String,
    },
    Opaque {
        definition: String,
    },
}

impl Codec for SchemaObject {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        match self {
            Self::Schema => {
                encoder.tag(0);
                Ok(())
            }
            Self::Table => {
                encoder.tag(1);
                Ok(())
            }
            Self::Column {
                sql_type,
                nullable,
                ordinal,
                default_expression,
                generated,
                identity,
                comment,
            } => {
                encoder.tag(2);
                sql_type.encode(encoder)?;
                encoder.bool(*nullable);
                encoder.u32(*ordinal);
                encoder.option(default_expression.as_ref(), |encoder, value| {
                    encoder.string(value)
                })?;
                encoder.option(generated.as_ref(), |encoder, value| encoder.string(value))?;
                encoder.option(identity.as_ref(), |encoder, value| encoder.string(value))?;
                encoder.option(comment.as_ref(), |encoder, value| encoder.string(value))
            }
            Self::PrimaryKey { columns } => {
                encoder.tag(3);
                encoder.seq(columns.len(), columns)
            }
            Self::ForeignKey {
                definition,
                referenced_table,
            } => {
                encoder.tag(4);
                encoder.string(definition)?;
                referenced_table.encode(encoder)
            }
            Self::Unique { definition } => encode_definition(encoder, 5, definition),
            Self::Index { definition } => encode_definition(encoder, 6, definition),
            Self::Check { definition } => encode_definition(encoder, 7, definition),
            Self::Enum { labels } => {
                encoder.tag(8);
                encoder.count(labels.len())?;
                for label in labels {
                    encoder.string(label)?;
                }
                Ok(())
            }
            Self::Domain { definition } => encode_definition(encoder, 9, definition),
            Self::View { definition } => encode_definition(encoder, 10, definition),
            Self::Routine { definition } => encode_definition(encoder, 11, definition),
            Self::Policy { definition } => encode_definition(encoder, 12, definition),
            Self::Opaque { definition } => encode_definition(encoder, 13, definition),
        }
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        match decoder.tag()? {
            0 => Ok(Self::Schema),
            1 => Ok(Self::Table),
            2 => Ok(Self::Column {
                sql_type: SqlTypeName::decode(decoder)?,
                nullable: decoder.bool()?,
                ordinal: decoder.u32()?,
                default_expression: decoder.option(|decoder| decoder.string())?,
                generated: decoder.option(|decoder| decoder.string())?,
                identity: decoder.option(|decoder| decoder.string())?,
                comment: decoder.option(|decoder| decoder.string())?,
            }),
            3 => Ok(Self::PrimaryKey {
                columns: decoder.seq()?,
            }),
            4 => Ok(Self::ForeignKey {
                definition: decoder.string()?,
                referenced_table: SchemaObjectId::decode(decoder)?,
            }),
            5 => Ok(Self::Unique {
                definition: decoder.string()?,
            }),
            6 => Ok(Self::Index {
                definition: decoder.string()?,
            }),
            7 => Ok(Self::Check {
                definition: decoder.string()?,
            }),
            8 => {
                let count = decoder.count()? as usize;
                let mut labels = Vec::with_capacity(count.min(1024));
                for _ in 0..count {
                    labels.push(decoder.string()?);
                }
                Ok(Self::Enum { labels })
            }
            9 => Ok(Self::Domain {
                definition: decoder.string()?,
            }),
            10 => Ok(Self::View {
                definition: decoder.string()?,
            }),
            11 => Ok(Self::Routine {
                definition: decoder.string()?,
            }),
            12 => Ok(Self::Policy {
                definition: decoder.string()?,
            }),
            13 => Ok(Self::Opaque {
                definition: decoder.string()?,
            }),
            other => Err(format!(
                "unknown schema object tag {other}.\n       fix: upgrade jails or delete the recomputable SQL cache and check again."
            )
            .into()),
        }
    }
}

fn encode_definition(encoder: &mut Encoder, tag: u8, definition: &str) -> Result<()> {
    encoder.tag(tag);
    encoder.string(definition)
}

#[derive(Clone, Debug, Eq, PartialEq, jails_codec_derive::Codec)]
pub struct OpaqueMigrationStatement {
    pub path: ProjectPath,
    pub span: ByteSpan,
    pub digest: ObjectId,
    pub reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogSnapshot {
    pub dialect: SqlDialect,
    pub objects: BTreeMap<SchemaObjectId, SchemaObject>,
    pub opaque: Vec<OpaqueMigrationStatement>,
    pub digest: ObjectId,
}

impl CatalogSnapshot {
    pub fn calculate_digest(
        dialect: SqlDialect,
        objects: &BTreeMap<SchemaObjectId, SchemaObject>,
        opaque: &[OpaqueMigrationStatement],
    ) -> Result<ObjectId> {
        let mut encoder = Encoder::new();
        dialect.encode(&mut encoder)?;
        encoder.map(objects)?;
        encoder.seq(opaque.len(), opaque)?;
        Ok(ObjectId::from_bytes(domain_hash(
            "JAILS-SQL-CATALOG-1",
            &encoder.finish()?,
        )))
    }

    pub fn new(
        dialect: SqlDialect,
        objects: BTreeMap<SchemaObjectId, SchemaObject>,
        opaque: Vec<OpaqueMigrationStatement>,
    ) -> Result<Self> {
        let digest = Self::calculate_digest(dialect, &objects, &opaque)?;
        Ok(Self {
            dialect,
            objects,
            opaque,
            digest,
        })
    }
}

impl Codec for CatalogSnapshot {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.dialect.encode(encoder)?;
        encoder.map(&self.objects)?;
        encoder.seq(self.opaque.len(), &self.opaque)?;
        self.digest.encode(encoder)
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        let dialect = SqlDialect::decode(decoder)?;
        let objects = decoder.map()?;
        let opaque = decoder.seq()?;
        let digest = ObjectId::decode(decoder)?;
        let expected = Self::calculate_digest(dialect, &objects, &opaque)?;
        if digest != expected {
            return Err(
                "catalog digest does not match its canonical contents.\n       fix: delete the recomputable SQL cache and run `jails sql check --offline` again."
                    .into(),
            );
        }
        Ok(Self {
            dialect,
            objects,
            opaque,
            digest,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SchemaProvenance {
    Declared,
    Migrations {
        files: Vec<ProjectPath>,
    },
    Live {
        server_major: u16,
        database_fingerprint: ObjectId,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SchemaSnapshot {
    pub catalog: CatalogSnapshot,
    pub provenance: SchemaProvenance,
    /// The complete, explicit observation policy. These are never inferred as
    /// absent schema objects during reconciliation.
    pub ignored_schemas: BTreeSet<SqlName>,
    pub ignores_extension_owned_objects: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SchemaOp {
    Create {
        id: SchemaObjectId,
        object: SchemaObject,
    },
    Alter {
        id: SchemaObjectId,
        before: SchemaObject,
        after: SchemaObject,
    },
    Rename {
        before: SchemaObjectId,
        after: SchemaObjectId,
    },
    Drop {
        id: SchemaObjectId,
        object: SchemaObject,
    },
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum MigrationRisk {
    Additive,
    DataDependent,
    ConstraintLoss,
    Destructive,
    DeploymentIncompatible,
    Opaque,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlannedSchemaOp {
    pub operation: SchemaOp,
    pub dependencies: BTreeSet<SchemaObjectId>,
    pub risks: BTreeSet<MigrationRisk>,
}
