//! Stable PostgreSQL-first identities and query contracts.
//!
//! SQL text enters through [`QuerySource::new`]. Everything after that point
//! receives typed values and canonical bytes; no generator reparses directive
//! strings or silently rewrites reader-owned SQL.

use crate::Result;
use crate::identity::{JavaType, Name, ObjectId, ProjectPath, SqlName};
use jails_support::codec::{Codec, Decoder, Encoder, domain_hash};

mod catalog;
pub use catalog::*;
mod datasource;
pub use datasource::*;
mod flyway;
pub use flyway::*;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub enum SqlDialect {
    PostgreSql,
    MySql,
    Sqlite,
}

impl SqlDialect {
    pub fn label(self) -> &'static str {
        match self {
            Self::PostgreSql => "postgresql",
            Self::MySql => "mysql",
            Self::Sqlite => "sqlite",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "postgres" | "postgresql" => Ok(Self::PostgreSql),
            "mysql" => Ok(Self::MySql),
            "sqlite" => Ok(Self::Sqlite),
            other => Err(format!(
                "unsupported SQL dialect `{other}`.\n       fix: use `postgresql`, `mysql`, or `sqlite`."
            )
            .into()),
        }
    }
}

impl Codec for SqlDialect {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.string(self.label())
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Self::parse(&decoder.string()?)
    }
}

macro_rules! upper_name {
    ($type:ident, $kind:literal) => {
        #[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
        pub struct $type(Name);

        impl $type {
            pub fn parse(value: &str) -> Result<Self> {
                let name = Name::parse(value)?;
                if !value.starts_with(char::is_uppercase) {
                    return Err(format!(
                        "{} `{value}` must begin with an uppercase letter.\n       fix: rename it to begin with an uppercase letter.",
                        $kind
                    )
                    .into());
                }
                Ok(Self(name))
            }

            pub fn as_str(&self) -> &str {
                self.0.as_str()
            }
        }

        impl Codec for $type {
            fn encode(&self, encoder: &mut Encoder) -> Result<()> {
                self.0.encode(encoder)
            }

            fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
                Self::parse(Name::decode(decoder)?.as_str())
            }
        }
    };
}

upper_name!(SliceName, "slice name");
upper_name!(QueryName, "query name");

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct QueryId {
    pub slice: SliceName,
    pub name: QueryName,
}

impl QueryId {
    pub fn new(slice: SliceName, name: QueryName) -> Self {
        Self { slice, name }
    }
}

impl Codec for QueryId {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.slice.encode(encoder)?;
        self.name.encode(encoder)
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(Self::new(
            SliceName::decode(decoder)?,
            QueryName::decode(decoder)?,
        ))
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct SqlTypeName(String);

impl SqlTypeName {
    pub fn parse(value: &str) -> Result<Self> {
        let valid = !value.is_empty()
            && !value.starts_with('.')
            && !value.ends_with('.')
            && value.split('.').all(|part| {
                let mut chars = part.chars();
                chars
                    .next()
                    .is_some_and(|ch| ch.is_ascii_lowercase() || ch == '_')
                    && chars.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_')
            });
        if !valid {
            return Err(format!(
                "`{value}` is not a lowercase SQL type name.\n       fix: declare a PostgreSQL type such as `text`, `int4`, or `public.order_status`."
            )
            .into());
        }
        Ok(Self(value.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Codec for SqlTypeName {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.string(&self.0)
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Self::parse(&decoder.string()?)
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct QualifiedSqlName {
    pub namespace: Option<SqlName>,
    pub name: SqlName,
}

impl Codec for QualifiedSqlName {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.maybe(self.namespace.as_ref())?;
        self.name.encode(encoder)
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(Self {
            namespace: decoder.option(SqlName::decode)?,
            name: SqlName::decode(decoder)?,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Hash, jails_codec_derive::Codec)]
pub enum SchemaObjectKind {
    #[codec(tag = 0)]
    Schema,
    #[codec(tag = 1)]
    Table,
    #[codec(tag = 2)]
    Column,
    #[codec(tag = 3)]
    PrimaryKey,
    #[codec(tag = 4)]
    ForeignKey,
    #[codec(tag = 5)]
    Unique,
    #[codec(tag = 6)]
    Index,
    #[codec(tag = 7)]
    Check,
    #[codec(tag = 8)]
    Enum,
    #[codec(tag = 9)]
    Domain,
    #[codec(tag = 10)]
    View,
    #[codec(tag = 11)]
    Routine,
    #[codec(tag = 12)]
    Policy,
    #[codec(tag = 13)]
    Opaque,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct SchemaObjectId {
    pub dialect: SqlDialect,
    pub namespace: SqlName,
    pub kind: SchemaObjectKind,
    pub name: SqlName,
    pub parent: Option<QualifiedSqlName>,
}

impl Codec for SchemaObjectId {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.dialect.encode(encoder)?;
        self.namespace.encode(encoder)?;
        self.kind.encode(encoder)?;
        self.name.encode(encoder)?;
        encoder.maybe(self.parent.as_ref())
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(Self {
            dialect: SqlDialect::decode(decoder)?,
            namespace: SqlName::decode(decoder)?,
            kind: SchemaObjectKind::decode(decoder)?,
            name: SqlName::decode(decoder)?,
            parent: decoder.option(QualifiedSqlName::decode)?,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct ByteSpan {
    pub start: u32,
    pub end: u32,
}

impl ByteSpan {
    pub fn new(start: usize, end: usize) -> Result<Self> {
        if start > end {
            return Err(
                "a byte span starts after it ends.\n       fix: report this jails analyzer bug; no project edit can repair an invalid internal span."
                    .into(),
            );
        }
        Ok(Self {
            start: u32::try_from(start).map_err(|_| "byte span start overflows u32")?,
            end: u32::try_from(end).map_err(|_| "byte span end overflows u32")?,
        })
    }
}

impl Codec for ByteSpan {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.u32(self.start);
        encoder.u32(self.end);
        Ok(())
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        let start = decoder.u32()?;
        let end = decoder.u32()?;
        Self::new(start as usize, end as usize)
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub enum Cardinality {
    One,
    Optional,
    Many,
    Exec,
    ExecRows,
}

impl Cardinality {
    pub fn label(self) -> &'static str {
        match self {
            Self::One => "one",
            Self::Optional => "optional",
            Self::Many => "many",
            Self::Exec => "exec",
            Self::ExecRows => "execrows",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "one" => Ok(Self::One),
            "optional" => Ok(Self::Optional),
            "many" => Ok(Self::Many),
            "exec" => Ok(Self::Exec),
            "execrows" => Ok(Self::ExecRows),
            other => Err(format!(
                "unknown query cardinality `{other}`.\n       fix: use one, optional, many, exec, or execrows."
            )
            .into()),
        }
    }
}

impl Codec for Cardinality {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.string(self.label())
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Self::parse(&decoder.string()?)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeclaredParameter {
    pub name: Name,
    pub sql_type: SqlTypeName,
    pub nullable: bool,
    pub span: ByteSpan,
}

jails_support::codec!(struct DeclaredParameter { name, sql_type, nullable, span });

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QuerySource {
    pub id: QueryId,
    pub path: ProjectPath,
    pub statement_span: ByteSpan,
    pub sql: String,
    pub cardinality: Cardinality,
    pub declared_parameters: Vec<DeclaredParameter>,
}

impl QuerySource {
    pub fn new(
        id: QueryId,
        path: ProjectPath,
        statement_span: ByteSpan,
        sql: &str,
        cardinality: Cardinality,
        declared_parameters: Vec<DeclaredParameter>,
    ) -> Result<Self> {
        if sql.contains('\0') {
            return Err(
                "query SQL contains a NUL byte.\n       fix: remove the NUL byte from the reader-owned SQL file."
                    .into(),
            );
        }
        let mut normalized = sql.replace("\r\n", "\n").replace('\r', "\n");
        while normalized.ends_with("\n\n") {
            normalized.pop();
        }
        if !normalized.ends_with('\n') {
            normalized.push('\n');
        }
        Ok(Self {
            id,
            path,
            statement_span,
            sql: normalized,
            cardinality,
            declared_parameters,
        })
    }

    pub fn query_digest(&self) -> ObjectId {
        ObjectId::from_bytes(domain_hash("JAILS-QUERY-SOURCE-1", self.sql.as_bytes()))
    }
}

impl Codec for QuerySource {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.id.encode(encoder)?;
        self.path.encode(encoder)?;
        self.statement_span.encode(encoder)?;
        encoder.string(&self.sql)?;
        self.cardinality.encode(encoder)?;
        encoder.seq(self.declared_parameters.len(), &self.declared_parameters)
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Self::new(
            QueryId::decode(decoder)?,
            ProjectPath::decode(decoder)?,
            ByteSpan::decode(decoder)?,
            &decoder.string()?,
            Cardinality::decode(decoder)?,
            decoder.seq()?,
        )
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Hash, jails_codec_derive::Codec)]
pub enum EvidenceSubject {
    #[codec(tag = 0)]
    Query(QueryId),
    #[codec(tag = 1)]
    SchemaObject(SchemaObjectId),
    #[codec(tag = 2)]
    Mapping { query: QueryId, name: Name },
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub enum EvidenceLevel {
    Parsed,
    VerifiedOffline,
    VerifiedLive,
    Executed,
}

impl Codec for EvidenceLevel {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.tag(match self {
            Self::Parsed => 0,
            Self::VerifiedOffline => 1,
            Self::VerifiedLive => 2,
            Self::Executed => 3,
        });
        Ok(())
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        match decoder.tag()? {
            0 => Ok(Self::Parsed),
            1 => Ok(Self::VerifiedOffline),
            2 => Ok(Self::VerifiedLive),
            3 => Ok(Self::Executed),
            other => Err(format!(
                "unknown evidence level tag {other}.\n       fix: upgrade jails or restore the contract from a known-good revision."
            )
            .into()),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvidenceRecord {
    pub subject: EvidenceSubject,
    pub level: EvidenceLevel,
    pub input_digest: ObjectId,
    pub catalog_digest: Option<ObjectId>,
    pub toolchain_digest: ObjectId,
    pub details_digest: ObjectId,
}

impl Codec for EvidenceRecord {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.subject.encode(encoder)?;
        self.level.encode(encoder)?;
        self.input_digest.encode(encoder)?;
        encoder.maybe(self.catalog_digest.as_ref())?;
        self.toolchain_digest.encode(encoder)?;
        self.details_digest.encode(encoder)
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(Self {
            subject: EvidenceSubject::decode(decoder)?,
            level: EvidenceLevel::decode(decoder)?,
            input_digest: ObjectId::decode(decoder)?,
            catalog_digest: decoder.option(ObjectId::decode)?,
            toolchain_digest: ObjectId::decode(decoder)?,
            details_digest: ObjectId::decode(decoder)?,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParameterContract {
    pub name: Name,
    pub sql_type: SqlTypeName,
    pub java_type: JavaType,
    pub nullable: bool,
    pub evidence: ObjectId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ColumnContract {
    pub name: SqlName,
    pub sql_type: SqlTypeName,
    pub java_name: Name,
    pub java_type: JavaType,
    pub nullable: bool,
    pub evidence: ObjectId,
}

macro_rules! contract_field_codec {
    ($type:ident, $name:ident, $($extra:ident),+) => {
        impl Codec for $type {
            fn encode(&self, encoder: &mut Encoder) -> Result<()> {
                self.$name.encode(encoder)?;
                $(self.$extra.encode(encoder)?;)+
                encoder.bool(self.nullable);
                self.evidence.encode(encoder)
            }

            fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
                Ok(Self {
                    $name: Codec::decode(decoder)?,
                    $($extra: Codec::decode(decoder)?,)+
                    nullable: decoder.bool()?,
                    evidence: ObjectId::decode(decoder)?,
                })
            }
        }
    };
}

contract_field_codec!(ParameterContract, name, sql_type, java_type);
contract_field_codec!(ColumnContract, name, sql_type, java_name, java_type);

#[derive(Clone, Debug, Eq, PartialEq, jails_codec_derive::Codec)]
pub struct QueryContractV1 {
    pub id: QueryId,
    pub dialect: SqlDialect,
    pub query_digest: ObjectId,
    pub catalog_digest: ObjectId,
    pub cardinality: Cardinality,
    pub parameters: Vec<ParameterContract>,
    pub columns: Vec<ColumnContract>,
    pub evidence: EvidenceRecord,
}

impl QueryContractV1 {
    pub fn contract_digest(&self) -> Result<ObjectId> {
        let mut encoder = Encoder::new();
        self.encode(&mut encoder)?;
        Ok(ObjectId::from_bytes(domain_hash(
            "JAILS-SQL-CONTRACT-1",
            &encoder.finish()?,
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn round_trip<T: Codec + Eq + std::fmt::Debug>(value: &T) {
        let mut encoder = Encoder::new();
        value.encode(&mut encoder).unwrap();
        let bytes = encoder.finish().unwrap();
        let mut decoder = Decoder::new(&bytes).unwrap();
        let decoded = T::decode(&mut decoder).unwrap();
        decoder.finish().unwrap();
        assert_eq!(*value, decoded);
    }

    #[test]
    fn query_source_normalizes_only_line_endings_and_terminal_newline() {
        let source = QuerySource::new(
            QueryId::new(
                SliceName::parse("Billing").unwrap(),
                QueryName::parse("FindPayableOrders").unwrap(),
            ),
            ProjectPath::parse("src/main/resources/db/queries/FindPayableOrders.sql").unwrap(),
            ByteSpan::new(0, 10).unwrap(),
            "SELECT  *\r\nFROM orders;\r\n\r\n",
            Cardinality::Many,
            Vec::new(),
        )
        .unwrap();
        assert_eq!(source.sql, "SELECT  *\nFROM orders;\n");
        round_trip(&source);
    }

    #[test]
    fn catalog_identity_is_order_independent_and_checked_on_decode() {
        let table = SchemaObjectId {
            dialect: SqlDialect::PostgreSql,
            namespace: SqlName::parse("public").unwrap(),
            kind: SchemaObjectKind::Table,
            name: SqlName::parse("orders").unwrap(),
            parent: None,
        };
        let catalog = CatalogSnapshot::new(
            SqlDialect::PostgreSql,
            BTreeMap::from([(table, SchemaObject::Table)]),
            Vec::new(),
        )
        .unwrap();
        round_trip(&catalog);
    }

    #[test]
    fn schema_object_identity_has_a_frozen_canonical_codec() {
        let table = SchemaObjectId {
            dialect: SqlDialect::PostgreSql,
            namespace: SqlName::parse("public").unwrap(),
            kind: SchemaObjectKind::Table,
            name: SqlName::parse("orders").unwrap(),
            parent: None,
        };
        let mut encoder = Encoder::new();
        table.encode(&mut encoder).unwrap();
        assert_eq!(
            jails_support::codec::hex_bytes(&encoder.finish().unwrap()),
            "0000000a706f737467726573716c000000067075626c696301000000066f726465727300"
        );
    }

    #[test]
    fn evidence_identity_contains_no_runtime_location_or_clock() {
        let id = QueryId::new(
            SliceName::parse("Billing").unwrap(),
            QueryName::parse("FindPayableOrders").unwrap(),
        );
        let zero = ObjectId::from_bytes([0; 32]);
        round_trip(&EvidenceRecord {
            subject: EvidenceSubject::Query(id),
            level: EvidenceLevel::VerifiedOffline,
            input_digest: zero,
            catalog_digest: Some(zero),
            toolchain_digest: zero,
            details_digest: zero,
        });
    }
}
