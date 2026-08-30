//! Canonical application and slice intent shared by CLI and manifests.

use crate::Result;
use crate::database::{QueryName, SliceName, SqlDialect, SqlTypeName};
use crate::declaration::{FieldSpec, IndexSpec};
use crate::entity::EntityId;
use crate::identity::{JavaType, Name, ObjectId, Package, ProjectPath};
use crate::lifecycle::TableBinding;
use jails_support::codec::{Codec, Decoder, Encoder, domain_hash};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct JavaRelease(u16);

impl JavaRelease {
    pub fn new(value: u16) -> Result<Self> {
        if value < 21 {
            return Err(format!(
                "Java release {value} is unsupported.\n       fix: select Java 21 or newer."
            )
            .into());
        }
        Ok(Self(value))
    }

    pub fn get(self) -> u16 {
        self.0
    }
}

impl Codec for JavaRelease {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.u32(u32::from(self.0));
        Ok(())
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        let value = decoder.u32()?;
        Self::new(u16::try_from(value).map_err(|_| "Java release overflows u16")?)
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct RoutePath(String);

impl RoutePath {
    pub fn parse(value: &str) -> Result<Self> {
        if !value.starts_with('/')
            || value.contains(char::is_whitespace)
            || value.contains('?')
            || value.contains('#')
            || (value.len() > 1 && value.ends_with('/'))
            || value.contains("//")
        {
            return Err(format!(
                "`{value}` is not a canonical route prefix.\n       fix: use a path such as `/billing` with no query, fragment, whitespace, duplicate slash, or trailing slash."
            )
            .into());
        }
        Ok(Self(value.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Codec for RoutePath {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.string(&self.0)
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Self::parse(&decoder.string()?)
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub enum AuditPolicy {
    None,
    Created,
    CreatedAndUpdated,
}

impl Codec for AuditPolicy {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.tag(match self {
            Self::None => 0,
            Self::Created => 1,
            Self::CreatedAndUpdated => 2,
        });
        Ok(())
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        match decoder.tag()? {
            0 => Ok(Self::None),
            1 => Ok(Self::Created),
            2 => Ok(Self::CreatedAndUpdated),
            other => Err(format!(
                "unknown audit policy tag {other}.\n       fix: upgrade jails or restore the record from a known-good receipt."
            )
            .into()),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, jails_codec_derive::Codec)]
pub enum DeclaredEntityLifecycle {
    #[codec(tag = 0)]
    Active,
    #[codec(tag = 1)]
    RetiredPreservingStorage,
    #[codec(tag = 2)]
    RetiredDropPlanned { migration: ProjectPath },
}

#[derive(Clone, Debug, Eq, PartialEq, jails_codec_derive::Codec)]
pub struct EntitySpecV1 {
    pub id: EntityId,
    pub lifecycle: DeclaredEntityLifecycle,
    pub table: TableBinding,
    pub fields: Vec<FieldSpec>,
    pub indexes: Vec<IndexSpec>,
    pub audit: AuditPolicy,
}

#[derive(Clone, Debug, Eq, PartialEq, jails_codec_derive::Codec)]
pub struct QuerySpecV1 {
    pub source: ProjectPath,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, jails_codec_derive::Codec)]
pub struct SliceSpecV1 {
    pub package: Option<Package>,
    pub route_prefix: Option<RoutePath>,
    pub entities: BTreeMap<Name, EntitySpecV1>,
    pub queries: BTreeMap<QueryName, QuerySpecV1>,
    pub events: BTreeMap<Name, ProjectPath>,
    pub policies: BTreeMap<Name, ProjectPath>,
}

pub type SliceSpec = SliceSpecV1;

#[derive(Clone, Debug, Eq, PartialEq, jails_codec_derive::Codec)]
pub struct ApplicationSpecV1 {
    pub name: Name,
    pub base_package: Package,
    pub java_release: JavaRelease,
    pub dialect: SqlDialect,
    pub type_mappings: BTreeMap<SqlTypeName, JavaType>,
    pub slices: BTreeMap<SliceName, SliceSpecV1>,
}

impl ApplicationSpecV1 {
    pub fn semantic_digest(&self) -> Result<ObjectId> {
        let mut encoder = Encoder::new();
        self.encode(&mut encoder)?;
        Ok(ObjectId::from_bytes(domain_hash(
            "JAILS-APPLICATION-SPEC-1",
            &encoder.finish()?,
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::IntentId;
    use crate::identity::SqlName;
    use jails_spec::spec::kind::ArtifactKind;

    fn fixture(fields: Vec<FieldSpec>) -> ApplicationSpecV1 {
        let package = Package::parse("com.acme.billing").unwrap();
        let id = EntityId::Intent(IntentId::new(
            ArtifactKind::Record,
            Name::parse("Order").unwrap(),
            package.clone(),
        ));
        ApplicationSpecV1 {
            name: Name::parse("ExampleApp").unwrap(),
            base_package: Package::parse("com.acme").unwrap(),
            java_release: JavaRelease::new(26).unwrap(),
            dialect: SqlDialect::PostgreSql,
            type_mappings: BTreeMap::new(),
            slices: BTreeMap::from([(
                SliceName::parse("Billing").unwrap(),
                SliceSpecV1 {
                    package: Some(package),
                    route_prefix: Some(RoutePath::parse("/billing").unwrap()),
                    entities: BTreeMap::from([(
                        Name::parse("Order").unwrap(),
                        EntitySpecV1 {
                            id,
                            lifecycle: DeclaredEntityLifecycle::Active,
                            table: TableBinding {
                                table: SqlName::parse("orders").unwrap(),
                            },
                            fields,
                            indexes: Vec::new(),
                            audit: AuditPolicy::CreatedAndUpdated,
                        },
                    )]),
                    ..SliceSpecV1::default()
                },
            )]),
        }
    }

    #[test]
    fn cli_and_manifest_field_spellings_share_one_typed_digest() {
        let package = Package::parse("com.acme.billing").unwrap();
        let cli = ["id:uuid@pk", "total:decimal@positive"]
            .into_iter()
            .map(|field| FieldSpec::parse(field, &package).unwrap())
            .collect();
        let manifest = ["id:uuid@pk", "total:decimal@positive"]
            .into_iter()
            .map(|field| FieldSpec::parse(field, &package).unwrap())
            .collect();
        assert_eq!(
            fixture(cli).semantic_digest().unwrap(),
            fixture(manifest).semantic_digest().unwrap()
        );
    }

    #[test]
    fn application_spec_round_trips_canonical_bytes() {
        let value = fixture(Vec::new());
        let mut encoder = Encoder::new();
        value.encode(&mut encoder).unwrap();
        let bytes = encoder.finish().unwrap();
        let mut decoder = Decoder::new(&bytes).unwrap();
        assert_eq!(value, ApplicationSpecV1::decode(&mut decoder).unwrap());
        decoder.finish().unwrap();
    }
}
