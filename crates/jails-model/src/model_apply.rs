//! Semantic application of atomic model patches.

use super::{
    AppModel, Capability, Dependency, Ejection, Entity, Field, Setting, refuse_retired_entity,
};
use crate::id::{
    CapabilityId, ComponentId, DependencyId, EntityId, FieldId, OperationId, SettingId, StableId,
    UnitId,
};
use crate::patch::{FieldPlacement, ModelPatch, StorageRetirementPolicy};
use crate::{Component, Operation};

impl AppModel {
    /// Apply a semantic patch without involving syntax or the filesystem.
    pub fn apply(&mut self, patch: ModelPatch) -> Result<(), String> {
        self.apply_one(patch)?;
        // Every patch that lands is a patch that could have moved a
        // projection, and `derived` is in the plan digest -- so it is
        // recomputed here rather than at each call site.
        self.refresh_derived();
        Ok(())
    }

    /// One patch, dispatched to the rule that owns its subject.
    ///
    /// **The arms are one line each on purpose**: with every rule for every
    /// variant inlined, "what does retiring an entity check" can only be
    /// answered by reading past everything it is not. The rules live below,
    /// grouped by what they are about, and the
    /// arms that are still inline are the ones with no rule to name: a field
    /// assignment, or a delegation that already has a home in
    /// [`crate::facet`], [`crate::index`] or [`crate::unit`].
    fn apply_one(&mut self, patch: ModelPatch) -> Result<(), String> {
        match patch {
            ModelPatch::Batch(patches) => apply_batch(self, patches)?,
            ModelPatch::SetDialect(dialect) => self.project.dialect = dialect,

            ModelPatch::AddCapability(capability) => add_capability(self, capability)?,
            ModelPatch::RemoveCapability(id) => remove_capability(self, id)?,
            ModelPatch::AddDependency(dependency) => add_dependency(self, dependency)?,
            ModelPatch::RemoveDependency(id) => remove_dependency(self, id)?,
            ModelPatch::SetSetting(setting) => set_setting(self, setting)?,
            ModelPatch::RemoveSetting(id) => remove_setting(self, id)?,
            ModelPatch::AddEjection(ejection) => add_ejection(self, ejection)?,

            ModelPatch::AddEntity(entity) => add_entity(self, entity)?,
            ModelPatch::RemoveEntity(id) => remove_entity(self, id)?,
            ModelPatch::RetireEntity { entity, policy } => retire_entity(self, entity, policy)?,
            ModelPatch::ReviveEntity {
                entity,
                confirmed_table,
            } => revive_entity(self, entity, confirmed_table)?,
            ModelPatch::AddEnumConstants { entity, constants } => {
                add_enum_constants(self, entity, constants)?
            }
            ModelPatch::RenameEntityProjection {
                entity,
                label,
                java,
                table,
                route,
            } => rename_entity_projection(self, entity, label, java, table, route)?,
            ModelPatch::AddFacet { entity, facet } => crate::facet::add(self, entity, facet)?,
            ModelPatch::RemoveFacet { entity, facet } => crate::facet::remove(self, entity, facet)?,

            ModelPatch::AddField {
                entity,
                field,
                policy: _,
                placement,
            } => add_field(self, entity, field, placement)?,
            ModelPatch::ReplaceField {
                entity,
                field,
                replacement,
                policy: _,
            } => replace_field(self, entity, field, replacement)?,
            ModelPatch::RemoveField {
                entity,
                field,
                confirmed_column: _,
            } => remove_field(self, entity, field)?,
            ModelPatch::AddIndex { entity, index } => crate::index::add(self, entity, index)?,
            ModelPatch::RemoveIndex {
                entity,
                index,
                confirmed_name,
            } => crate::index::remove(self, entity, index, confirmed_name)?,
            ModelPatch::AddRelation(relation) => add_relation(self, relation)?,
            ModelPatch::RemoveRelation {
                relation,
                confirmed_name,
            } => remove_relation(self, relation, confirmed_name)?,
            ModelPatch::AddProjection(projection) => add_projection(self, projection)?,

            ModelPatch::AddUnit(unit) => {
                crate::unit::insert(&mut self.units, unit)?;
            }
            ModelPatch::ReplaceUnit(unit) => crate::unit::replace(&mut self.units, unit)?,
            ModelPatch::RemoveUnit(id) => remove_unit(self, id)?,
            ModelPatch::AddComponent(component) => add_component(self, component)?,
            ModelPatch::ReplaceComponent(component) => replace_component(self, component)?,
            ModelPatch::RemoveComponent(id) => remove_component(self, id)?,

            ModelPatch::AddOperation(operation) => add_operation(self, operation)?,
            ModelPatch::RemoveOperation(id) => remove_operation(self, id)?,
        }
        Ok(())
    }
}

// Project-level declarations -- the things a project has rather than the
// things it models. Each is a set with one identity rule and one collision
// rule, and the collision is the interesting half: a capability collides on
// *kind*, a dependency on its coordinate, a setting on `(target, key)`.

fn add_capability(model: &mut AppModel, capability: Capability) -> Result<(), String> {
    let id = capability.id.clone();
    if model.capabilities.contains_key(&id) {
        return Err(format!("capability id `{id}` already exists"));
    }
    if model
        .capabilities
        .values()
        .any(|existing| existing.kind == capability.kind)
    {
        return Err(format!(
            "capability kind `{}` already exists",
            capability.kind
        ));
    }
    model.capabilities.insert(id, capability);
    Ok(())
}

fn remove_capability(model: &mut AppModel, id: CapabilityId) -> Result<(), String> {
    refuse_ejected_target(model, id.as_str())?;
    if model.capabilities.remove(&id).is_none() {
        return Err(format!("capability id `{id}` does not exist"));
    }
    Ok(())
}

fn add_dependency(model: &mut AppModel, dependency: Dependency) -> Result<(), String> {
    let id = dependency.id.clone();
    if model.dependencies.contains_key(&id) {
        return Err(format!("dependency id `{id}` already exists"));
    }
    if model.dependencies.values().any(|existing| {
        existing.group == dependency.group && existing.artifact == dependency.artifact
    }) {
        return Err(format!(
            "dependency coordinate `{}:{}` already exists",
            dependency.group, dependency.artifact
        ));
    }
    model.dependencies.insert(id, dependency);
    Ok(())
}

fn remove_dependency(model: &mut AppModel, id: DependencyId) -> Result<(), String> {
    if model.dependencies.remove(&id).is_none() {
        return Err(format!("dependency id `{id}` does not exist"));
    }
    Ok(())
}

fn set_setting(model: &mut AppModel, setting: Setting) -> Result<(), String> {
    if model.settings.iter().any(|(id, existing)| {
        id != &setting.id && existing.target == setting.target && existing.key == setting.key
    }) {
        return Err(format!(
            "setting key `{}` already exists for `{}`",
            setting.key,
            setting.target.label()
        ));
    }
    model.settings.insert(setting.id.clone(), setting);
    Ok(())
}

fn remove_setting(model: &mut AppModel, id: SettingId) -> Result<(), String> {
    if model.settings.remove(&id).is_none() {
        return Err(format!("setting id `{id}` does not exist"));
    }
    Ok(())
}

fn add_ejection(model: &mut AppModel, ejection: Ejection) -> Result<(), String> {
    if model.ejections.contains_key(&ejection.id) {
        return Err(format!("ejection id `{}` already exists", ejection.id));
    }
    if model
        .ejections
        .values()
        .any(|existing| existing.target == ejection.target)
    {
        return Err(format!(
            "semantic target `{}` is already ejected",
            ejection.target
        ));
    }
    if !is_ejectable_target(model, &ejection.target) {
        return Err(format!(
            "semantic target `{}` does not exist\n       fix: eject an entity, operation, or capability stable id",
            ejection.target
        ));
    }
    model.ejections.insert(ejection.id.clone(), ejection);
    Ok(())
}

fn apply_batch(model: &mut AppModel, patches: Vec<ModelPatch>) -> Result<(), String> {
    let mut next = model.clone();
    for patch in patches {
        next.apply(patch)?;
    }
    *model = next;
    Ok(())
}

// Entity lifecycle. Retirement and revival are here rather than beside the
// field rules because they are about whether the entity exists at all, and
// both take exact evidence -- a confirmed table -- rather than a flag.

fn add_entity(model: &mut AppModel, entity: Entity) -> Result<(), String> {
    let id = entity.id.clone();
    if model.entities.contains_key(&id) {
        return Err(format!("entity id `{id}` already exists"));
    }
    model.entities.insert(id, entity);
    Ok(())
}

fn remove_entity(model: &mut AppModel, id: EntityId) -> Result<(), String> {
    refuse_ejected_target(model, id.as_str())?;
    let references = dependents(model, &id);
    if !references.is_empty() {
        return Err(refuse_dependents(id.as_str(), &references, "removing"));
    }
    if !forget_entity(model, &id) {
        return Err(format!("entity id `{id}` does not exist"));
    }
    Ok(())
}

/// Take an entity out of the model, and its projections with it.
///
/// **A projection is the entity's child, not a dependent of it.** `use repo`
/// says something about `note` and has no meaning without it, so `dependents`
/// deliberately does not count one -- and removing the entity alone leaves
/// the patched model carrying projections pointing at nothing. Nothing reads
/// them, so the emitted tree is right and the *accepted* model is not:
/// `model check --frozen` reports the project as diverged from its own
/// source, permanently, because re-linking the source yields no projections
/// and the lock still has them.
///
/// One function rather than two lines at each site, because there are two
/// removals -- `RemoveEntity` and `RetireEntity` with a confirmed drop -- and
/// both need the second line.
fn forget_entity(model: &mut AppModel, id: &EntityId) -> bool {
    let removed = model.entities.remove(id).is_some();
    model
        .projections
        .retain(|_, projection| &projection.entity != id);
    removed
}

fn retire_entity(
    model: &mut AppModel,
    entity: EntityId,
    policy: StorageRetirementPolicy,
) -> Result<(), String> {
    refuse_ejected_target(model, entity.as_str())?;
    let references = dependents(model, &entity);
    if !references.is_empty() {
        return Err(refuse_dependents(entity.as_str(), &references, "retiring"));
    }
    let target = model
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
            model
                .entities
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
            forget_entity(model, &entity);
        }
    }
    Ok(())
}

fn revive_entity(
    model: &mut AppModel,
    entity: EntityId,
    confirmed_table: String,
) -> Result<(), String> {
    let target = model
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
    Ok(())
}

fn add_enum_constants(
    model: &mut AppModel,
    entity: EntityId,
    constants: Vec<crate::EnumConstant>,
) -> Result<(), String> {
    let subject = model.entities.get_mut(&entity).ok_or_else(|| {
        format!("enum `{entity}` is not declared\n       fix: declare it before widening it")
    })?;
    for constant in constants {
        if subject
            .enum_constants
            .iter()
            .any(|existing| existing.java_name == constant.java_name)
        {
            return Err(format!(
                "enum `{entity}` already declares `{}`\n       fix: state the set once",
                constant.java_name
            ));
        }
        subject.enum_constants.push(constant);
    }
    Ok(())
}

fn rename_entity_projection(
    model: &mut AppModel,
    entity: EntityId,
    label: Option<String>,
    java: Option<String>,
    table: Option<String>,
    route: Option<String>,
) -> Result<(), String> {
    if let Some(route) = route {
        for projection in model.projections.values_mut() {
            if projection.entity == entity
                && let crate::projection::ProjectionKind::Http { path } = &mut projection.kind
            {
                *path = Some(route.clone());
            }
        }
    }
    let target = model
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
    Ok(())
}

// Entity children. Every one of these refuses on a retired parent, and the
// destructive ones name what they are destroying: `remove_field` refuses
// while an operation or relation still points at the field.

fn add_field(
    model: &mut AppModel,
    entity: EntityId,
    field: Field,
    placement: FieldPlacement,
) -> Result<(), String> {
    let target = model
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
    }
    Ok(())
}

fn replace_field(
    model: &mut AppModel,
    entity: EntityId,
    field: FieldId,
    replacement: Field,
) -> Result<(), String> {
    let target = model
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
    Ok(())
}

fn remove_field(model: &mut AppModel, entity: EntityId, field: FieldId) -> Result<(), String> {
    let references = model
        .operations
        .values()
        .filter(|operation| {
            crate::operation::fields(&operation.kind)
                .into_iter()
                .any(|candidate| candidate == &field)
        })
        .map(|operation| operation.label.as_str())
        .chain(
            model
                .relations
                .values()
                .filter(|relation| {
                    relation
                        .mappings
                        .iter()
                        .any(|mapping| mapping.local == field || mapping.remote == field)
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
    let target = model
        .entities
        .get_mut(&entity)
        .ok_or_else(|| format!("entity id `{entity}` does not exist"))?;
    refuse_retired_entity(target)?;
    let before = target.fields.len();
    target.fields.retain(|existing| existing.id != field);
    if target.fields.len() == before {
        return Err(format!("field id `{field}` does not exist on `{entity}`"));
    }
    Ok(())
}

fn add_relation(model: &mut AppModel, relation: crate::Relation) -> Result<(), String> {
    let id = relation.id.clone();
    if model.relations.contains_key(&id) {
        return Err(format!("relation id `{id}` already exists"));
    }
    for (side, entity) in [("child", &relation.child), ("parent", &relation.parent)] {
        if !model.entities.contains_key(entity) {
            return Err(format!(
                "relation `{id}` names a missing {side} entity `{entity}`"
            ));
        }
    }
    model.relations.insert(id, relation);
    Ok(())
}

fn remove_relation(
    model: &mut AppModel,
    relation: crate::id::RelationId,
    confirmed_name: String,
) -> Result<(), String> {
    let accepted = model
        .relations
        .get(&relation)
        .ok_or_else(|| format!("relation id `{relation}` does not exist"))?;
    if accepted.sql_name != confirmed_name {
        return Err(format!(
            "relation `{relation}` is accepted as constraint `{}`, not `{confirmed_name}`\n       fix: name the accepted constraint",
            accepted.sql_name
        ));
    }
    model.relations.remove(&relation);
    Ok(())
}

fn add_projection(model: &mut AppModel, projection: crate::Projection) -> Result<(), String> {
    if !model.entities.contains_key(&projection.entity) {
        return Err(format!(
            "projection `{}` names a missing entity `{}`",
            projection.id, projection.entity
        ));
    }
    let id = projection.id.clone();
    if model.projections.contains_key(&id) {
        return Err(format!("projection id `{id}` already exists"));
    }
    model.projections.insert(id, projection);
    Ok(())
}

// Units and components. `add`/`replace` for units already live in
// [`crate::unit`]; what is here is removal, which has to check the ejection
// declarations and the component derivation first.

fn remove_unit(model: &mut AppModel, id: UnitId) -> Result<(), String> {
    refuse_ejected_target(model, id.as_str())?;
    if model
        .components
        .keys()
        .any(|component| component.as_str() == id.as_str())
    {
        return Err(format!(
            "source unit id `{id}` is derived from a typed component\n       fix: remove or evolve the component declaration instead"
        ));
    }
    if model.units.remove(&id).is_none() {
        return Err(format!("source unit id `{id}` does not exist"));
    }
    Ok(())
}

fn add_component(model: &mut AppModel, component: Component) -> Result<(), String> {
    let id = component.id.clone();
    if model.components.insert(id.clone(), component).is_some() {
        return Err(format!("component id `{id}` already exists"));
    }
    Ok(())
}

fn replace_component(model: &mut AppModel, component: Component) -> Result<(), String> {
    let id = component.id.clone();
    if !model.components.contains_key(&id) {
        return Err(format!("component id `{id}` does not exist"));
    }
    model.components.insert(id, component);
    Ok(())
}

fn remove_component(model: &mut AppModel, id: ComponentId) -> Result<(), String> {
    let referenced = model.components.values().any(|component| {
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
    if model.components.remove(&id).is_none() {
        return Err(format!("component id `{id}` does not exist"));
    }
    Ok(())
}

// Operations. Removal is the only one with a rule: an operation another
// transition emits, or a component references, cannot go.

fn add_operation(model: &mut AppModel, operation: Operation) -> Result<(), String> {
    let id = operation.id.clone();
    if model.operations.contains_key(&id) {
        return Err(format!("operation id `{id}` already exists"));
    }
    model.operations.insert(id, operation);
    Ok(())
}

fn remove_operation(model: &mut AppModel, id: OperationId) -> Result<(), String> {
    refuse_ejected_target(model, id.as_str())?;
    let references = model
        .operations
        .values()
        .filter(|operation| {
            crate::operation::emits(&operation.kind)
                .into_iter()
                .any(|emitted| emitted == &id)
        })
        .map(|operation| operation.label.as_str())
        .chain(
            model
                .components
                .values()
                .filter(|component| crate::component::references_operation(component, &id))
                .map(|component| component.label.as_str()),
        )
        .collect::<Vec<_>>();
    if !references.is_empty() {
        return Err(format!(
            "operation id `{id}` is still yielded by transitions: {}\n       fix: remove those transitions before removing the operation",
            references.join(", ")
        ));
    }
    if model.operations.remove(&id).is_none() {
        return Err(format!("operation id `{id}` does not exist"));
    }
    Ok(())
}

fn is_ejectable_target(model: &AppModel, target: &str) -> bool {
    target.starts_with("art_")
        || model.capabilities.keys().any(|id| id.as_str() == target)
        || model.units.keys().any(|id| id.as_str() == target)
        || model.components.keys().any(|id| id.as_str() == target)
        || model.entities.keys().any(|id| id.as_str() == target)
        || model.operations.keys().any(|id| id.as_str() == target)
}

/// Everything that would be left pointing at an entity if it went away.
///
/// **Each one is named by what it is**, because the fix differs: an operation
/// is removed, a component is removed, and an association is retired forward
/// with its own command. The message this feeds names the kind and the Java
/// spelling: a reader who typed `jails g association ChildParent` and is told
/// to remove an operation called `child_parent` is told neither the thing nor
/// the word they typed.
fn dependents(model: &AppModel, entity: &crate::EntityId) -> Vec<String> {
    model
        .operations
        .values()
        .filter(|operation| crate::operation::references_entity(&operation.kind, entity))
        .map(|operation| {
            format!(
                "operation {}",
                crate::naming::upper_camel_case(&operation.label)
            )
        })
        .chain(
            model
                .components
                .values()
                .filter(|component| crate::component::references_entity(component, entity))
                .map(|component| {
                    format!(
                        "component {}",
                        crate::naming::upper_camel_case(&component.label)
                    )
                }),
        )
        .chain(
            model
                .relations
                .values()
                .filter(|relation| relation.child == *entity || relation.parent == *entity)
                .map(|relation| {
                    format!(
                        "association {}",
                        crate::naming::upper_camel_case(&relation.label)
                    )
                }),
        )
        .collect()
}

/// The refusal, in the words of what the removal would break.
fn refuse_dependents(subject: &str, references: &[String], verb: &str) -> String {
    format!(
        "{verb} `{subject}` would leave {} pointing at nothing\n       fix: remove or retire {} first",
        references.join(", "),
        if references.len() == 1 {
            "it"
        } else {
            "each of them"
        }
    )
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
