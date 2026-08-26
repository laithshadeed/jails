//! Strict TOML/JSON decoding for the canonical application model.
//!
//! Syntax-only values live in this module. They are converted immediately
//! through protocol constructors so generators never parse manifest strings.

use jails_protocol::application::{
    ApplicationSpecV1, AuditPolicy, DeclaredEntityLifecycle, EntitySpecV1, JavaRelease,
    QuerySpecV1, RoutePath, SliceSpecV1,
};
use jails_protocol::database::{QueryName, SliceName, SqlDialect};
use jails_protocol::declaration::{FieldSpec, IndexSpec};
use jails_protocol::entity::EntityId;
use jails_protocol::identity::{Name, ObjectId, Package, ProjectPath, SqlName};
use jails_protocol::lifecycle::TableBinding;
use jails_support::Result;
use jails_support::codec::domain_hash;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ManifestFormat {
    Toml,
    Json,
}

pub fn decode(text: &str, format: ManifestFormat) -> Result<ApplicationSpecV1> {
    let raw: Document = match format {
        ManifestFormat::Toml => toml::from_str(text)
            .map_err(|error| format!("invalid application manifest TOML: {error}"))?,
        ManifestFormat::Json => serde_json::from_str(text)
            .map_err(|error| format!("invalid application manifest JSON: {error}"))?,
    };
    raw.resolve()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    schema: String,
    application: Application,
    #[serde(default)]
    slices: BTreeMap<String, Slice>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Application {
    name: String,
    base_package: String,
    java_release: u16,
    dialect: String,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Slice {
    package: Option<String>,
    route_prefix: Option<String>,
    #[serde(default)]
    entities: BTreeMap<String, Entity>,
    #[serde(default)]
    queries: BTreeMap<String, Query>,
    #[serde(default)]
    events: BTreeMap<String, PathDeclaration>,
    #[serde(default)]
    policies: BTreeMap<String, PathDeclaration>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Entity {
    id: Option<String>,
    table: Option<String>,
    lifecycle: Option<String>,
    audit: Option<String>,
    fields: Vec<String>,
    #[serde(default)]
    indexes: Vec<Index>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Index {
    #[allow(dead_code)]
    name: Option<String>,
    fields: Vec<String>,
    #[allow(dead_code)]
    unique: Option<bool>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Query {
    source: String,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum PathDeclaration {
    Path(String),
    Object { source: String },
}

impl PathDeclaration {
    fn source(&self) -> &str {
        match self {
            Self::Path(path) => path,
            Self::Object { source } => source,
        }
    }
}

impl Document {
    fn resolve(self) -> Result<ApplicationSpecV1> {
        if self.schema != "jails.app.v1" {
            return Err(format!(
                "unsupported application manifest schema `{}`; this jails supports `jails.app.v1`.\n       fix: upgrade jails for a newer schema or migrate this manifest to `jails.app.v1`.",
                self.schema
            ).into());
        }
        let name = Name::parse(&self.application.name)?;
        let base_package = Package::parse(&self.application.base_package)?;
        let java_release = JavaRelease::new(self.application.java_release)?;
        let dialect = SqlDialect::parse(&self.application.dialect)?;
        let application_identity = format!("{}|{}", name.as_str(), base_package.as_str());
        let mut slices = BTreeMap::new();
        let mut routes = BTreeSet::new();
        for (slice_name, slice) in self.slices {
            let slice_name = SliceName::parse(&slice_name)?;
            let package = slice
                .package
                .as_deref()
                .map(Package::parse)
                .transpose()?
                .unwrap_or(
                    base_package.join(&Package::parse(&slice_name.as_str().to_ascii_lowercase())?),
                );
            let route_prefix = slice
                .route_prefix
                .as_deref()
                .map(RoutePath::parse)
                .transpose()?;
            if let Some(route) = &route_prefix
                && !routes.insert(route.as_str().to_string())
            {
                return Err(format!("route prefix `{}` is declared by more than one slice.\n       fix: give every slice a distinct route prefix.", route.as_str()).into());
            }
            let resolved = slice.resolve(&application_identity, &slice_name, &package)?;
            slices.insert(
                slice_name,
                SliceSpecV1 {
                    package: Some(package),
                    route_prefix,
                    ..resolved
                },
            );
        }
        Ok(ApplicationSpecV1 {
            name,
            base_package,
            java_release,
            dialect,
            slices,
        })
    }
}

impl Slice {
    fn resolve(
        self,
        application: &str,
        slice: &SliceName,
        package: &Package,
    ) -> Result<SliceSpecV1> {
        let mut entities = BTreeMap::new();
        for (entity_name, entity) in self.entities {
            let name = Name::parse(&entity_name)?;
            entities.insert(
                name.clone(),
                entity.resolve(application, slice, &name, package)?,
            );
        }
        let queries = self
            .queries
            .into_iter()
            .map(|(name, query)| {
                Ok((
                    QueryName::parse(&name)?,
                    QuerySpecV1 {
                        source: ProjectPath::parse(&query.source)?,
                    },
                ))
            })
            .collect::<Result<BTreeMap<_, _>>>()?;
        let events = paths(self.events)?;
        let policies = paths(self.policies)?;
        Ok(SliceSpecV1 {
            package: None,
            route_prefix: None,
            entities,
            queries,
            events,
            policies,
        })
    }
}

impl Entity {
    fn resolve(
        self,
        application: &str,
        slice: &SliceName,
        name: &Name,
        package: &Package,
    ) -> Result<EntitySpecV1> {
        let fields = self
            .fields
            .iter()
            .map(|field| FieldSpec::parse(field, package))
            .collect::<Result<Vec<_>>>()?;
        let mut field_names = BTreeSet::new();
        for field in &fields {
            if !field_names.insert(field.name.clone()) {
                return Err(format!("entity `{name}` declares field `{}` twice.\n       fix: keep one declaration for each field.", field.name).into());
            }
        }
        let indexes = self
            .indexes
            .iter()
            .map(|index| IndexSpec::parse(&index.fields.join(", "), &fields))
            .collect::<Result<Vec<_>>>()?;
        let conventional_table = snake(name.as_str());
        let table = SqlName::parse(self.table.as_deref().unwrap_or(&conventional_table))?;
        let id = match self.id.as_deref() {
            Some(value) => EntityId::Application(parse_id(value)?),
            None => EntityId::Application(ObjectId::from_bytes(domain_hash(
                "JAILS-ENTITY-ID-1",
                format!("{application}|{}|{}", slice.as_str(), name.as_str()).as_bytes(),
            ))),
        };
        Ok(EntitySpecV1 {
            id,
            lifecycle: lifecycle(self.lifecycle.as_deref())?,
            table: TableBinding { table },
            fields,
            indexes,
            audit: audit(self.audit.as_deref())?,
        })
    }
}

fn parse_id(value: &str) -> Result<ObjectId> {
    let hex = value
        .strip_prefix("sha256:")
        .ok_or("application entity id must start with `sha256:`")?;
    ObjectId::parse_hex(hex)
}

fn lifecycle(value: Option<&str>) -> Result<DeclaredEntityLifecycle> {
    match value.unwrap_or("active") {
        "active" => Ok(DeclaredEntityLifecycle::Active),
        "retired-preserving-storage" => Ok(DeclaredEntityLifecycle::RetiredPreservingStorage),
        other => Err(format!("unsupported entity lifecycle `{other}`.\n       fix: use `active` or `retired-preserving-storage`; a planned drop also requires its migration path.").into()),
    }
}

fn audit(value: Option<&str>) -> Result<AuditPolicy> {
    match value.unwrap_or("none") {
        "none" => Ok(AuditPolicy::None),
        "created" => Ok(AuditPolicy::Created),
        "created-and-updated" => Ok(AuditPolicy::CreatedAndUpdated),
        other => Err(format!("unknown audit policy `{other}`.\n       fix: use `none`, `created`, or `created-and-updated`.").into()),
    }
}

fn paths(values: BTreeMap<String, PathDeclaration>) -> Result<BTreeMap<Name, ProjectPath>> {
    values
        .into_iter()
        .map(|(name, value)| Ok((Name::parse(&name)?, ProjectPath::parse(value.source())?)))
        .collect()
}

fn snake(name: &str) -> String {
    let mut output = String::new();
    for (index, character) in name.chars().enumerate() {
        if index > 0 && character.is_ascii_uppercase() {
            output.push('_');
        }
        output.push(character.to_ascii_lowercase());
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOML: &str = r#"
schema = "jails.app.v1"
[application]
name = "Orders"
base_package = "com.acme"
java_release = 26
dialect = "postgresql"
[slices.Billing]
package = "com.acme.billing"
route_prefix = "/billing"
[slices.Billing.entities.Order]
table = "orders"
audit = "created-and-updated"
fields = ["id:uuid@pk", "total:decimal@positive"]
[[slices.Billing.entities.Order.indexes]]
name = "orders_total_idx"
fields = ["total"]
unique = false
[slices.Billing.queries.FindPayableOrders]
source = "src/main/resources/db/queries/FindPayableOrders.sql"
"#;

    #[test]
    fn toml_and_json_construct_identical_typed_values() {
        let json = r#"{
          "schema":"jails.app.v1",
          "application":{"name":"Orders","base_package":"com.acme","java_release":26,"dialect":"postgresql"},
          "slices":{"Billing":{"package":"com.acme.billing","route_prefix":"/billing",
            "entities":{"Order":{"table":"orders","audit":"created-and-updated","fields":["id:uuid@pk","total:decimal@positive"],"indexes":[{"name":"orders_total_idx","fields":["total"],"unique":false}]}},
            "queries":{"FindPayableOrders":{"source":"src/main/resources/db/queries/FindPayableOrders.sql"}}}}
        }"#;
        let toml = decode(TOML, ManifestFormat::Toml).unwrap();
        let json = decode(json, ManifestFormat::Json).unwrap();
        assert_eq!(toml, json);
        assert_eq!(
            toml.semantic_digest().unwrap(),
            json.semantic_digest().unwrap()
        );
    }

    #[test]
    fn unknown_fields_fail_closed() {
        let error = decode(
            &TOML.replace("java_release = 26", "java_release = 26\njava_relase = 26"),
            ManifestFormat::Toml,
        )
        .unwrap_err();
        assert!(error.contains("java_relase"), "{error}");
    }

    #[test]
    fn derived_entity_identity_is_stable_for_identical_input() {
        let first = decode(TOML, ManifestFormat::Toml).unwrap();
        let second = decode(TOML, ManifestFormat::Toml).unwrap();
        assert_eq!(first, second);
    }
}
