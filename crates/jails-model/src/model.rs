#[path = "model_apply.rs"]
mod mutation;

use crate::EnumConstant;
use crate::Operation;
use crate::SourceUnit;
use crate::app::ProjectIntent;
use crate::constraint::EntityConstraint;
use crate::id::{
    CapabilityId, ComponentId, ConstraintId, DependencyId, EjectionId, EntityId, FieldId, IndexId,
    OperationId, ProjectionId, RelationId, SettingId, UnitId,
};
use crate::projection::Projection;
use crate::relation::Relation;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) use mutation::refuse_ejected_target;

/// The only desired-state value consumed by the application compiler.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AppModel {
    pub schema: String,
    pub project: ProjectIntent,
    pub capabilities: BTreeMap<CapabilityId, Capability>,
    pub dependencies: BTreeMap<DependencyId, Dependency>,
    pub settings: BTreeMap<SettingId, Setting>,
    pub ejections: BTreeMap<EjectionId, Ejection>,
    #[serde(default)]
    pub units: BTreeMap<UnitId, SourceUnit>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub components: BTreeMap<ComponentId, crate::Component>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub projections: BTreeMap<ProjectionId, Projection>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub relations: BTreeMap<RelationId, Relation>,
    pub entities: BTreeMap<EntityId, Entity>,
    pub operations: BTreeMap<OperationId, Operation>,
}

impl AppModel {
    pub fn entity(&self, id: &EntityId) -> Option<&Entity> {
        self.entities.get(id)
    }

    pub fn canonical_json(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(self)
    }

    pub fn node_count(&self) -> usize {
        1 + self.capabilities.len()
            + self.dependencies.len()
            + self.settings.len()
            + self.ejections.len()
            + self.units.len()
            + self.components.len()
            + self.projections.len()
            + self.relations.len()
            + self.entities.len()
            + self
                .entities
                .values()
                .map(|entity| entity.fields.len() + entity.indexes.len() + entity.constraints.len())
                .sum::<usize>()
            + self.operations.len()
    }
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Capability {
    pub id: CapabilityId,
    pub label: String,
    pub kind: String,
    /// Optional reader-selected base name for capability-owned Java types.
    pub name: Option<String>,
    /// Fully resolved Java package override. `None` selects the backend's
    /// conventional package below the application's base package.
    pub java_package: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Dependency {
    pub id: DependencyId,
    pub label: String,
    pub group: String,
    pub artifact: String,
    pub version: Option<String>,
    pub scope: DependencyScope,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DependencyScope {
    Compile,
    Runtime,
    Test,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Setting {
    pub id: SettingId,
    pub label: String,
    pub key: String,
    pub value: String,
    pub target: SettingTarget,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Ejection {
    pub id: EjectionId,
    pub label: String,
    pub target: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SettingTarget {
    Main,
    Test,
}

impl SettingTarget {
    pub fn label(self) -> &'static str {
        match self {
            Self::Main => "main",
            Self::Test => "test",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Entity {
    pub id: EntityId,
    pub label: String,
    pub names: EntityNames,
    #[serde(default = "crate::facet::active_entity")]
    pub active: bool,
    pub facets: BTreeSet<Facet>,
    pub enum_constants: Vec<EnumConstant>,
    pub fields: BTreeMap<FieldId, Field>,
    pub indexes: BTreeMap<IndexId, Index>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub constraints: BTreeMap<ConstraintId, EntityConstraint>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EntityNames {
    pub java_type: String,
    pub sql_table: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Field {
    pub id: FieldId,
    pub label: String,
    pub names: FieldNames,
    pub ty: TypeRef,
    pub required: bool,
    pub non_blank: bool,
    pub primary_key: bool,
    pub unique: bool,
    pub indexed: bool,
    #[serde(default)]
    pub length: Option<LengthRange>,
    #[serde(default, skip_serializing_if = "FieldSemantics::is_empty")]
    pub semantics: FieldSemantics,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct FieldSemantics {
    pub positive: bool,
    pub nonnegative: bool,
    pub scope: Option<FieldScope>,
    pub version: bool,
    pub default: Option<FieldDefault>,
    pub updated: bool,
}

impl FieldSemantics {
    fn is_empty(&self) -> bool {
        self == &Self::default()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FieldScope {
    pub claim: String,
    /// True when the claim name was pinned explicitly in source.
    pub pinned: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FieldDefault {
    pub value: crate::operation::Value,
    /// True when the compiler derived the value from another field rule.
    pub derived: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LengthRange {
    pub min: Option<u32>,
    pub max: Option<u32>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FieldNames {
    pub java_member: String,
    pub sql_column: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Index {
    pub id: IndexId,
    pub label: String,
    pub sql_name: String,
    pub columns: Vec<IndexColumn>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct IndexColumn {
    pub field: FieldId,
    pub direction: IndexDirection,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum IndexDirection {
    Asc,
    Desc,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Facet {
    Enum,
    Record,
    Factory,
    Dto,
    Repository,
    Service,
    Http,
    Events,
    Search,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum TypeRef {
    Builtin(BuiltinType),
    External(String),
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum BuiltinType {
    String,
    Integer,
    Long,
    Double,
    Decimal,
    Boolean,
    Uuid,
    Date,
    DateTime,
    Instant,
    Duration,
    Uri,
    Path,
    ZoneId,
    Currency,
    Bytes,
}

impl TypeRef {
    pub(crate) fn parse(value: &str) -> Result<Self, String> {
        if let Some(builtin) = BuiltinType::from_token(value) {
            return Ok(Self::Builtin(builtin));
        }
        if value.rsplit('.').next().is_some_and(valid_java_type)
            && value.split('.').all(valid_java_type)
        {
            return Ok(Self::External(value.to_string()));
        }
        Err(format!(
            "`{value}` is neither a builtin type nor a Java type"
        ))
    }

    pub fn canonical_name(&self) -> &str {
        match self {
            Self::Builtin(builtin) => builtin.semantics().token,
            Self::External(name) => name,
        }
    }
}

fn valid_java_type(value: &str) -> bool {
    let mut characters = value.chars();
    characters
        .next()
        .is_some_and(|character| character.is_ascii_alphabetic() || character == '_')
        && characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

pub(crate) fn refuse_retired_entity(entity: &Entity) -> Result<(), String> {
    if entity.active {
        return Ok(());
    }
    Err(format!(
        "entity id `{}` is retired\n       fix: revive the preserved entity before evolving it",
        entity.id
    ))
}
