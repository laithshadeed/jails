use crate::EnumConstant;
use crate::Operation;
use crate::SourceUnit;
use crate::id::{
    CapabilityId, DependencyId, EjectionId, EntityId, FieldId, IndexId, OperationId, ProjectId,
    SettingId, StableId, UnitId,
};
use crate::patch::{ModelPatch, StorageRetirementPolicy};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

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
            + self.entities.len()
            + self
                .entities
                .values()
                .map(|entity| entity.fields.len() + entity.indexes.len())
                .sum::<usize>()
            + self.operations.len()
    }

    /// Apply a semantic patch without involving syntax or the filesystem.
    pub fn apply(&mut self, patch: ModelPatch) -> Result<(), String> {
        match patch {
            ModelPatch::AddEntity(entity) => {
                let id = entity.id.clone();
                if self.entities.contains_key(&id) {
                    return Err(format!("entity id `{id}` already exists"));
                }
                self.entities.insert(id, entity);
            }
            ModelPatch::AddFacet { entity, facet } => crate::facet::add(self, entity, facet)?,
            ModelPatch::RemoveFacet { entity, facet } => crate::facet::remove(self, entity, facet)?,
            ModelPatch::AddUnit(unit) => {
                crate::unit::insert(&mut self.units, unit)?;
            }
            ModelPatch::ReplaceUnit(unit) => crate::unit::replace(&mut self.units, unit)?,
            ModelPatch::RemoveUnit(id) => {
                refuse_ejected_target(self, id.as_str())?;
                if self.units.remove(&id).is_none() {
                    return Err(format!("source unit id `{id}` does not exist"));
                }
            }
            ModelPatch::AddCapability(capability) => {
                let id = capability.id.clone();
                if self.capabilities.contains_key(&id) {
                    return Err(format!("capability id `{id}` already exists"));
                }
                if self
                    .capabilities
                    .values()
                    .any(|existing| existing.kind == capability.kind)
                {
                    return Err(format!(
                        "capability kind `{}` already exists",
                        capability.kind
                    ));
                }
                self.capabilities.insert(id, capability);
            }
            ModelPatch::RemoveCapability(id) => {
                refuse_ejected_target(self, id.as_str())?;
                if self.capabilities.remove(&id).is_none() {
                    return Err(format!("capability id `{id}` does not exist"));
                }
            }
            ModelPatch::AddDependency(dependency) => {
                let id = dependency.id.clone();
                if self.dependencies.contains_key(&id) {
                    return Err(format!("dependency id `{id}` already exists"));
                }
                if self.dependencies.values().any(|existing| {
                    existing.group == dependency.group && existing.artifact == dependency.artifact
                }) {
                    return Err(format!(
                        "dependency coordinate `{}:{}` already exists",
                        dependency.group, dependency.artifact
                    ));
                }
                self.dependencies.insert(id, dependency);
            }
            ModelPatch::RemoveDependency(id) => {
                if self.dependencies.remove(&id).is_none() {
                    return Err(format!("dependency id `{id}` does not exist"));
                }
            }
            ModelPatch::SetSetting(setting) => {
                if self.settings.iter().any(|(id, existing)| {
                    id != &setting.id
                        && existing.target == setting.target
                        && existing.key == setting.key
                }) {
                    return Err(format!(
                        "setting key `{}` already exists for `{}`",
                        setting.key,
                        setting.target.label()
                    ));
                }
                self.settings.insert(setting.id.clone(), setting);
            }
            ModelPatch::RemoveSetting(id) => {
                if self.settings.remove(&id).is_none() {
                    return Err(format!("setting id `{id}` does not exist"));
                }
            }
            ModelPatch::AddEjection(ejection) => {
                if self.ejections.contains_key(&ejection.id) {
                    return Err(format!("ejection id `{}` already exists", ejection.id));
                }
                if self
                    .ejections
                    .values()
                    .any(|existing| existing.target == ejection.target)
                {
                    return Err(format!(
                        "semantic target `{}` is already ejected",
                        ejection.target
                    ));
                }
                if !is_ejectable_target(self, &ejection.target) {
                    return Err(format!(
                        "semantic target `{}` does not exist\n       fix: eject an entity, operation, or capability stable id",
                        ejection.target
                    ));
                }
                self.ejections.insert(ejection.id.clone(), ejection);
            }
            ModelPatch::Batch(patches) => {
                let mut next = self.clone();
                for patch in patches {
                    next.apply(patch)?;
                }
                *self = next;
            }
            ModelPatch::RemoveEntity(id) => {
                refuse_ejected_target(self, id.as_str())?;
                let references = self
                    .operations
                    .values()
                    .filter(|operation| crate::operation::references_entity(&operation.kind, &id))
                    .map(|operation| operation.label.as_str())
                    .collect::<Vec<_>>();
                if !references.is_empty() {
                    return Err(format!(
                        "entity id `{id}` is still referenced by operations: {}\n       fix: remove those operations before removing the entity",
                        references.join(", ")
                    ));
                }
                if self.entities.remove(&id).is_none() {
                    return Err(format!("entity id `{id}` does not exist"));
                }
            }
            ModelPatch::RetireEntity { entity, policy } => {
                refuse_ejected_target(self, entity.as_str())?;
                let references = self
                    .operations
                    .values()
                    .filter(|operation| {
                        crate::operation::references_entity(&operation.kind, &entity)
                    })
                    .map(|operation| operation.label.as_str())
                    .collect::<Vec<_>>();
                if !references.is_empty() {
                    return Err(format!(
                        "entity id `{entity}` is still referenced by operations: {}\n       fix: remove those operations before retiring the entity",
                        references.join(", ")
                    ));
                }
                let target = self
                    .entities
                    .get(&entity)
                    .ok_or_else(|| format!("entity id `{entity}` does not exist"))?;
                if !target.active {
                    return Err(format!(
                        "entity id `{entity}` is already retired\n       fix: revive it or choose an active entity"
                    ));
                }
                match policy {
                    StorageRetirementPolicy::Preserve => {
                        self.entities
                            .get_mut(&entity)
                            .expect("retirement target was checked")
                            .active = false;
                    }
                    StorageRetirementPolicy::Drop { confirmed_table } => {
                        if confirmed_table != target.names.sql_table {
                            return Err(format!(
                                "confirmed table `{confirmed_table}` is not `{}` for `{}`\n       fix: pass `--confirm-table {}` exactly, or use `--storage preserve`",
                                target.names.sql_table, target.label, target.names.sql_table
                            ));
                        }
                        self.entities.remove(&entity);
                    }
                }
            }
            ModelPatch::ReviveEntity {
                entity,
                confirmed_table,
            } => {
                let target = self
                    .entities
                    .get_mut(&entity)
                    .ok_or_else(|| format!("entity id `{entity}` does not exist"))?;
                if target.active {
                    return Err(format!(
                        "entity id `{entity}` is already active\n       fix: evolve the active entity directly"
                    ));
                }
                if confirmed_table != target.names.sql_table {
                    return Err(format!(
                        "confirmed table `{confirmed_table}` is not the preserved table `{}`\n       fix: pass `--table {}` exactly",
                        target.names.sql_table, target.names.sql_table
                    ));
                }
                target.active = true;
            }
            ModelPatch::AddField {
                entity,
                field,
                policy: _,
            } => {
                let target = self
                    .entities
                    .get_mut(&entity)
                    .ok_or_else(|| format!("entity id `{entity}` does not exist"))?;
                refuse_retired_entity(target)?;
                let id = field.id.clone();
                if target.fields.contains_key(&id) {
                    return Err(format!("field id `{id}` already exists on `{entity}`"));
                }
                target.fields.insert(id, field);
            }
            ModelPatch::AddIndex { entity, index } => crate::index::add(self, entity, index)?,
            ModelPatch::RemoveIndex {
                entity,
                index,
                confirmed_name,
            } => crate::index::remove(self, entity, index, confirmed_name)?,
            ModelPatch::ReplaceField {
                entity,
                field,
                replacement,
                policy: _,
            } => {
                let target = self
                    .entities
                    .get_mut(&entity)
                    .ok_or_else(|| format!("entity id `{entity}` does not exist"))?;
                refuse_retired_entity(target)?;
                let existing = target
                    .fields
                    .get(&field)
                    .ok_or_else(|| format!("field id `{field}` does not exist on `{entity}`"))?;
                if replacement.id != field || replacement.label != existing.label {
                    return Err(format!(
                        "field replacement must preserve stable id `{field}` and label `{}`",
                        existing.label
                    ));
                }
                target.fields.insert(field, replacement);
            }
            ModelPatch::RemoveField {
                entity,
                field,
                confirmed_column: _,
            } => {
                let references = self
                    .operations
                    .values()
                    .filter(|operation| {
                        crate::operation::fields(&operation.kind)
                            .into_iter()
                            .any(|candidate| candidate == &field)
                    })
                    .map(|operation| operation.label.as_str())
                    .collect::<Vec<_>>();
                if !references.is_empty() {
                    return Err(format!(
                        "field id `{field}` is still referenced by operations: {}\n       fix: remove or evolve those operations before removing the field",
                        references.join(", ")
                    ));
                }
                let target = self
                    .entities
                    .get_mut(&entity)
                    .ok_or_else(|| format!("entity id `{entity}` does not exist"))?;
                refuse_retired_entity(target)?;
                if target.fields.remove(&field).is_none() {
                    return Err(format!("field id `{field}` does not exist on `{entity}`"));
                }
            }
            ModelPatch::AddOperation(operation) => {
                let id = operation.id.clone();
                if self.operations.contains_key(&id) {
                    return Err(format!("operation id `{id}` already exists"));
                }
                self.operations.insert(id, operation);
            }
            ModelPatch::RemoveOperation(id) => {
                refuse_ejected_target(self, id.as_str())?;
                let references = self
                    .operations
                    .values()
                    .filter(|operation| {
                        crate::operation::emits(&operation.kind)
                            .into_iter()
                            .any(|emitted| emitted == &id)
                    })
                    .map(|operation| operation.label.as_str())
                    .collect::<Vec<_>>();
                if !references.is_empty() {
                    return Err(format!(
                        "operation id `{id}` is still yielded by transitions: {}\n       fix: remove those transitions before removing the operation",
                        references.join(", ")
                    ));
                }
                if self.operations.remove(&id).is_none() {
                    return Err(format!("operation id `{id}` does not exist"));
                }
            }
            ModelPatch::RenameEntityProjection {
                entity,
                java,
                table,
            } => {
                let target = self
                    .entities
                    .get_mut(&entity)
                    .ok_or_else(|| format!("entity id `{entity}` does not exist"))?;
                if let Some(java) = java {
                    target.names.java_type = java;
                }
                if let Some(table) = table {
                    target.names.sql_table = table;
                }
            }
        }
        Ok(())
    }
}

fn is_ejectable_target(model: &AppModel, target: &str) -> bool {
    target.starts_with("art_")
        || model.capabilities.keys().any(|id| id.as_str() == target)
        || model.units.keys().any(|id| id.as_str() == target)
        || model.entities.keys().any(|id| id.as_str() == target)
        || model.operations.keys().any(|id| id.as_str() == target)
}

pub(crate) fn refuse_ejected_target(model: &AppModel, target: &str) -> Result<(), String> {
    if model
        .ejections
        .values()
        .any(|ejection| artifact_mentions(&ejection.target, target))
    {
        return Err(format!(
            "semantic target `{target}` is reader-owned\n       fix: remove or migrate its ejection declaration before removing the target"
        ));
    }
    Ok(())
}

fn artifact_mentions(artifact: &str, semantic_id: &str) -> bool {
    artifact == semantic_id
        || artifact == format!("art_{semantic_id}")
        || artifact.starts_with(&format!("art_{semantic_id}_"))
        || artifact.contains(&format!("_{semantic_id}_"))
        || artifact.ends_with(&format!("_{semantic_id}"))
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProjectIntent {
    pub id: ProjectId,
    pub name: String,
    pub base_package: String,
    pub java_release: u16,
    pub dialect: String,
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
        let builtin = match value {
            "string" => Some(BuiltinType::String),
            "int" => Some(BuiltinType::Integer),
            "long" => Some(BuiltinType::Long),
            "double" => Some(BuiltinType::Double),
            "decimal" => Some(BuiltinType::Decimal),
            "boolean" => Some(BuiltinType::Boolean),
            "uuid" => Some(BuiltinType::Uuid),
            "date" => Some(BuiltinType::Date),
            "datetime" => Some(BuiltinType::DateTime),
            "instant" => Some(BuiltinType::Instant),
            "duration" => Some(BuiltinType::Duration),
            "uri" => Some(BuiltinType::Uri),
            "path" => Some(BuiltinType::Path),
            "zone-id" => Some(BuiltinType::ZoneId),
            "currency" => Some(BuiltinType::Currency),
            "bytes" => Some(BuiltinType::Bytes),
            _ => None,
        };
        if let Some(builtin) = builtin {
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
            Self::Builtin(builtin) => match builtin {
                BuiltinType::String => "string",
                BuiltinType::Integer => "int",
                BuiltinType::Long => "long",
                BuiltinType::Double => "double",
                BuiltinType::Decimal => "decimal",
                BuiltinType::Boolean => "boolean",
                BuiltinType::Uuid => "uuid",
                BuiltinType::Date => "date",
                BuiltinType::DateTime => "datetime",
                BuiltinType::Instant => "instant",
                BuiltinType::Duration => "duration",
                BuiltinType::Uri => "uri",
                BuiltinType::Path => "path",
                BuiltinType::ZoneId => "zone-id",
                BuiltinType::Currency => "currency",
                BuiltinType::Bytes => "bytes",
            },
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
