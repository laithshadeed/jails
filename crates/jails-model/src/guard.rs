//! What a removal would break, in the words of what it would break.
//!
//! A frontend edits the source and the linker says whether the result still
//! links; a declaration removed while an operation edge still points at it
//! is a link error. That error names the dangling reference, which is the
//! symptom. These answer the question the reader has before editing -- *can
//! this go?* -- naming each dependent by its kind and the spelling the reader
//! typed, because the fix differs: an operation is removed, a component is
//! removed, an association is retired forward with its own command.
//!
//! Every function is a query over one model and writes nothing.

use crate::id::{ComponentId, EntityId, FieldId, OperationId, StableId, UnitId};
use crate::model::AppModel;

impl AppModel {
    /// Refuse to take an entity away while something still points at it.
    pub fn refuse_entity_removal(&self, entity: &EntityId, verb: &str) -> Result<(), String> {
        self.refuse_ejected_target(entity.as_str())?;
        let references = self.entity_dependents(entity);
        if references.is_empty() {
            return Ok(());
        }
        Err(refuse_dependents(entity.as_str(), &references, verb))
    }

    /// Everything that would be left pointing at an entity if it went away.
    ///
    /// **Each one is named by what it is**, because the fix differs. A
    /// projection is deliberately not one: `use repo` says something about
    /// `note` and has no meaning without it, so it goes with the entity.
    pub fn entity_dependents(&self, entity: &EntityId) -> Vec<String> {
        self.operations
            .values()
            .filter(|operation| crate::operation::references_entity(&operation.kind, entity))
            .map(|operation| {
                format!(
                    "operation {}",
                    crate::naming::upper_camel_case(&operation.label)
                )
            })
            .chain(
                self.components
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
                self.relations
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

    /// Refuse to drop a field an operation or a relation still names.
    pub fn refuse_field_removal(&self, field: &FieldId) -> Result<(), String> {
        let references = self
            .operations
            .values()
            .filter(|operation| {
                crate::operation::fields(&operation.kind)
                    .into_iter()
                    .any(|candidate| candidate == field)
            })
            .map(|operation| operation.label.as_str())
            .chain(
                self.relations
                    .values()
                    .filter(|relation| {
                        relation
                            .mappings
                            .iter()
                            .any(|mapping| mapping.local == *field || mapping.remote == *field)
                    })
                    .map(|relation| relation.label.as_str()),
            )
            .collect::<Vec<_>>();
        if references.is_empty() {
            return Ok(());
        }
        Err(format!(
            "field id `{field}` is still referenced by operations: {}\n       fix: remove or evolve those operations before removing the field",
            references.join(", ")
        ))
    }

    /// Refuse to remove an operation another transition yields or a
    /// component references.
    pub fn refuse_operation_removal(&self, operation: &OperationId) -> Result<(), String> {
        self.refuse_ejected_target(operation.as_str())?;
        let references = self
            .operations
            .values()
            .filter(|candidate| {
                crate::operation::emits(&candidate.kind)
                    .into_iter()
                    .any(|emitted| emitted == operation)
            })
            .map(|candidate| candidate.label.as_str())
            .chain(
                self.components
                    .values()
                    .filter(|component| {
                        crate::component::references_operation(component, operation)
                    })
                    .map(|component| component.label.as_str()),
            )
            .collect::<Vec<_>>();
        if references.is_empty() {
            return Ok(());
        }
        Err(format!(
            "operation id `{operation}` is still yielded by transitions: {}\n       fix: remove those transitions before removing the operation",
            references.join(", ")
        ))
    }

    /// Refuse to remove a component another component's `on` or `yields`
    /// names.
    pub fn refuse_component_removal(&self, component: &ComponentId) -> Result<(), String> {
        let referenced = self.components.values().any(|candidate| {
            candidate.on.as_ref().is_some_and(|reference| {
                matches!(reference, crate::ComponentReference::Component(target) if target == component)
            }) || candidate.yields.as_ref().is_some_and(|reference| {
                matches!(reference, crate::ComponentReference::Component(target) if target == component)
            })
        });
        if referenced {
            return Err(format!(
                "component id `{component}` is still referenced by another component\n       fix: remove or repoint the component that names it first"
            ));
        }
        Ok(())
    }

    /// Refuse to remove a source unit that is a typed component's projection.
    pub fn refuse_unit_removal(&self, unit: &UnitId) -> Result<(), String> {
        self.refuse_ejected_target(unit.as_str())?;
        if self
            .components
            .keys()
            .any(|component| component.as_str() == unit.as_str())
        {
            return Err(format!(
                "source unit id `{unit}` is derived from a typed component\n       fix: remove or evolve the component declaration instead"
            ));
        }
        Ok(())
    }

    /// Refuse to touch a semantic target an ejection declaration names.
    ///
    /// An adopted boundary is not in the way: the reader's file was theirs
    /// before the declaration existed, so removing the declaration deletes
    /// nothing, and the frontend takes the `eject ... @adopted` line out with
    /// the owner it names.
    pub fn refuse_ejected_target(&self, target: &str) -> Result<(), String> {
        if self
            .ejections
            .values()
            .filter(|ejection| !ejection.adopted)
            .any(|ejection| artifact_mentions(&ejection.target, target))
        {
            return Err(format!(
                "`{target}` is reader-owned\n       fix: remove or migrate its ejection declaration before removing the target"
            ));
        }
        Ok(())
    }

    /// Whether the reader wrote this artifact before the model knew it.
    pub fn is_adopted(&self, artifact_id: &str) -> bool {
        self.ejections
            .values()
            .any(|ejection| ejection.adopted && ejection.target == artifact_id)
    }

    /// The adopted boundaries of one owner: the `eject ... @adopted` lines
    /// that go when the owner goes.
    pub fn adopted_ejections_of(&self, target: &str) -> Vec<&crate::model::Ejection> {
        self.ejections
            .values()
            .filter(|ejection| ejection.adopted && artifact_mentions(&ejection.target, target))
            .collect()
    }

    /// Whether an ejection may name this target at all.
    pub fn is_ejectable_target(&self, target: &str) -> bool {
        target.starts_with("art_")
            || self.capabilities.keys().any(|id| id.as_str() == target)
            || self.units.keys().any(|id| id.as_str() == target)
            || self.components.keys().any(|id| id.as_str() == target)
            || self.entities.keys().any(|id| id.as_str() == target)
            || self.operations.keys().any(|id| id.as_str() == target)
    }
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

fn artifact_mentions(artifact: &str, semantic_id: &str) -> bool {
    artifact == semantic_id
        || artifact == format!("art_{semantic_id}")
        || artifact.starts_with(&format!("art_{semantic_id}_"))
        || artifact.contains(&format!("_{semantic_id}_"))
        || artifact.ends_with(&format!("_{semantic_id}"))
}
