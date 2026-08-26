//! Migration-derived catalog facts and explicit opaque blockers.

use super::{ByteSpan, SchemaObjectId, SqlDialect, SqlTypeName};
use crate::Result;
use crate::identity::{ObjectId, ProjectPath, SqlName};
use jails_support::codec::{Codec, Decoder, Encoder, domain_hash};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SchemaObject {
    Table,
    Column {
        sql_type: SqlTypeName,
        nullable: bool,
        ordinal: u32,
    },
    PrimaryKey {
        columns: Vec<SqlName>,
    },
}

impl Codec for SchemaObject {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        match self {
            Self::Table => {
                encoder.tag(0);
                Ok(())
            }
            Self::Column {
                sql_type,
                nullable,
                ordinal,
            } => {
                encoder.tag(1);
                sql_type.encode(encoder)?;
                encoder.bool(*nullable);
                encoder.u32(*ordinal);
                Ok(())
            }
            Self::PrimaryKey { columns } => {
                encoder.tag(2);
                encoder.seq(columns.len(), columns)
            }
        }
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        match decoder.tag()? {
            0 => Ok(Self::Table),
            1 => Ok(Self::Column {
                sql_type: SqlTypeName::decode(decoder)?,
                nullable: decoder.bool()?,
                ordinal: decoder.u32()?,
            }),
            2 => Ok(Self::PrimaryKey {
                columns: decoder.seq()?,
            }),
            other => Err(format!(
                "unknown schema object tag {other}.\n       fix: upgrade jails or delete the recomputable SQL cache and check again."
            )
            .into()),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpaqueMigrationStatement {
    pub path: ProjectPath,
    pub span: ByteSpan,
    pub digest: ObjectId,
    pub reason: String,
}

impl Codec for OpaqueMigrationStatement {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.path.encode(encoder)?;
        self.span.encode(encoder)?;
        self.digest.encode(encoder)?;
        encoder.string(&self.reason)
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(Self {
            path: ProjectPath::decode(decoder)?,
            span: ByteSpan::decode(decoder)?,
            digest: ObjectId::decode(decoder)?,
            reason: decoder.string()?,
        })
    }
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
