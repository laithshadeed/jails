//! Semantic application of atomic model patches.

use super::{AppModel, refuse_retired_entity};
use crate::id::StableId;
use crate::patch::{FieldPlacement, ModelPatch, StorageRetirementPolicy};

impl AppModel {
    /// Apply a semantic patch without involving syntax or the filesystem.
    pub fn apply(&mut self, patch: ModelPatch) -> Result<(), String> {
        self.apply_one(patch)?;
        // Every patch that lands is a patch that could have moved a
        // projection, and `derived` is in the plan digest -- so it is
        // recomputed here rather than at the handful of call sites that
        // currently remember to.
        self.refresh_derived();
        Ok(())
    }

    fn apply_one(&mut self, patch: ModelPatch) -> Result<(), String> {
        match patch {
            ModelPatch::ReplaceModel(model) => *self = *model,
            ModelPatch::AddEntity(entity) => {
                let id = entity.id.clone();
                if self.entities.contains_key(&id) {
                    return Err(format!("entity id `{id}` already exists"));
                }
                self.entities.insert(id, entity);
            }
            ModelPatch::AddRelation(relation) => {
                let id = relation.id.clone();
                if self.relations.contains_key(&id) {
                    return Err(format!("relation id `{id}` already exists"));
                }
                for (side, entity) in [("child", &relation.child), ("parent", &relation.parent)] {
                    if !self.entities.contains_key(entity) {
                        return Err(format!(
                            "relation `{id}` names a missing {side} entity `{entity}`"
                        ));
                    }
                }
                self.relations.insert(id, relation);
            }
            ModelPatch::AddFacet { entity, facet } => crate::facet::add(self, entity, facet)?,
            ModelPatch::RemoveFacet { entity, facet } => crate::facet::remove(self, entity, facet)?,
            ModelPatch::AddUnit(unit) => {
                crate::unit::insert(&mut self.units, unit)?;
            }
            ModelPatch::ReplaceUnit(unit) => crate::unit::replace(&mut self.units, unit)?,
            ModelPatch::RemoveUnit(id) => {
                refuse_ejected_target(self, id.as_str())?;
                if self
                    .components
                    .keys()
                    .any(|component| component.as_str() == id.as_str())
                {
                    return Err(format!(
                        "source unit id `{id}` is derived from a typed component\n       fix: remove or evolve the component declaration instead"
                    ));
                }
                if self.units.remove(&id).is_none() {
                    return Err(format!("source unit id `{id}` does not exist"));
                }
            }
            ModelPatch::AddComponent(component) => {
                let id = component.id.clone();
                if self.components.insert(id.clone(), component).is_some() {
                    return Err(format!("component id `{id}` already exists"));
                }
            }
            ModelPatch::ReplaceComponent(component) => {
                let id = component.id.clone();
                if !self.components.contains_key(&id) {
                    return Err(format!("component id `{id}` does not exist"));
                }
                self.components.insert(id, component);
            }
            ModelPatch::RemoveComponent(id) => {
                let referenced = self.components.values().any(|component| {
                    component.on.as_ref().is_some_and(|reference| {
                        matches!(reference, crate::ComponentReference::Component(target) if target == &id)
                    }) || component.yields.as_ref().is_some_and(|reference| {
                        matches!(reference, crate::ComponentReference::Component(target) if target == &id)
                    })
                });
                if referenced {
                    return Err(format!(
                        "component id `{id}` is still referenced by another component"
                    ));
                }
                if self.components.remove(&id).is_none() {
                    return Err(format!("component id `{id}` does not exist"));
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
                    .chain(
                        self.components
                            .values()
                            .filter(|component| crate::component::references_entity(component, &id))
                            .map(|component| component.label.as_str()),
                    )
                    .chain(
                        self.relations
                            .values()
                            .filter(|relation| relation.child == id || relation.parent == id)
                            .map(|relation| relation.label.as_str()),
                    )
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
                    .chain(
                        self.components
                            .values()
                            .filter(|component| {
                                crate::component::references_entity(component, &entity)
                            })
                            .map(|component| component.label.as_str()),
                    )
                    .chain(
                        self.relations
                            .values()
                            .filter(|relation| {
                                relation.child == entity || relation.parent == entity
                            })
                            .map(|relation| relation.label.as_str()),
                    )
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
                placement,
            } => {
                let target = self
                    .entities
                    .get_mut(&entity)
                    .ok_or_else(|| format!("entity id `{entity}` does not exist"))?;
                refuse_retired_entity(target)?;
                let id = field.id.clone();
                if target.has_field(&id) {
                    return Err(format!("field id `{id}` already exists on `{entity}`"));
                }
                // Placed where the frontend that wrote the source put it,
                // because a record's component order is ABI and the patched
                // model has to equal what re-parsing those bytes yields.
                match placement {
                    FieldPlacement::Last => target.fields.push(field),
                    FieldPlacement::ByLabel => {
                        let position = target
                            .fields
                            .partition_point(|existing| existing.label < field.label);
                        target.fields.insert(position, field);
                    }
                }
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
                if !target.has_field(&field) {
                    return Err(format!("field id `{field}` does not exist on `{entity}`"));
                }
                if replacement.id != field {
                    return Err(format!(
                        "field replacement must preserve stable id `{field}`"
                    ));
                }
                // Replaced in place. Preserving the stable id is exactly the
                // promise that the component keeps its position, so a rename
                // or a type change is not an ABI reordering.
                let slot = target
                    .fields
                    .iter_mut()
                    .find(|existing| existing.id == field)
                    .expect("the field was just confirmed to exist");
                *slot = replacement;
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
                    .chain(
                        self.relations
                            .values()
                            .filter(|relation| {
                                relation.mappings.iter().any(|mapping| {
                                    mapping.local == field || mapping.remote == field
                                })
                            })
                            .map(|relation| relation.label.as_str()),
                    )
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
                let before = target.fields.len();
                target.fields.retain(|existing| existing.id != field);
                if target.fields.len() == before {
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
                    .chain(
                        self.components
                            .values()
                            .filter(|component| {
                                crate::component::references_operation(component, &id)
                            })
                            .map(|component| component.label.as_str()),
                    )
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
                label,
                java,
                table,
            } => {
                let target = self
                    .entities
                    .get_mut(&entity)
                    .ok_or_else(|| format!("entity id `{entity}` does not exist"))?;
                if let Some(label) = label {
                    target.label = label;
                }
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
        || model.components.keys().any(|id| id.as_str() == target)
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
