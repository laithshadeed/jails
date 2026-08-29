//! Cross-node rules for scope and compiler-managed entity fields.

use super::super::Linker;
use crate::model::{Capability, Entity};
use crate::operation::{Operation, OperationKind, ParameterSource, Precondition};
use crate::{CapabilityId, EntityId, Field, FieldId, OperationId};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn validate(
    operations: &BTreeMap<OperationId, Operation>,
    entities: &BTreeMap<EntityId, Entity>,
    capabilities: &BTreeMap<CapabilityId, Capability>,
    linker: &mut Linker,
) {
    let has_security = capabilities
        .values()
        .any(|capability| capability.kind == "security");
    for operation in operations.values() {
        let Some(entity_id) = crate::operation::entity(&operation.kind) else {
            continue;
        };
        let Some(entity) = entities.get(entity_id) else {
            continue;
        };
        let path = format!("$.operations.{}", operation.label);
        let fields = entity
            .fields
            .values()
            .map(|field| (&field.id, field))
            .collect::<BTreeMap<_, _>>();
        let scoped = entity
            .fields
            .values()
            .any(|field| field.semantics.scope.is_some());
        if scoped && routed(&operation.kind) && !has_security {
            linker.problem(
                "model-scope-security",
                format!("{path}.semantics.route"),
                format!(
                    "routed operation `{}` targets scoped entity `{}` without security",
                    operation.label, entity.label
                ),
                "declare `cap security` or make the operation internal and unrouted",
            );
        }

        validate_inputs(&operation.kind, &fields, &path, linker);
        validate_targets(&operation.kind, &fields, &path, linker);
        if let OperationKind::Transition(transition) = &operation.kind {
            validate_precondition(transition, entity, &path, linker);
        }
    }
}

fn routed(kind: &OperationKind) -> bool {
    match kind {
        OperationKind::Command(command) => {
            command.route.is_some() || command.semantics.route.is_some()
        }
        OperationKind::Query(query) => query.route.is_some() || query.semantics.route.is_some(),
        OperationKind::Transition(transition) => {
            transition.route.is_some() || transition.semantics.route.is_some()
        }
        OperationKind::Event(_) => false,
    }
}

fn validate_inputs(
    kind: &OperationKind,
    fields: &BTreeMap<&FieldId, &Field>,
    path: &str,
    linker: &mut Linker,
) {
    let mut inputs = Vec::new();
    match kind {
        OperationKind::Command(command) => {
            inputs.extend(command.fields.iter());
            inputs.extend(parameter_fields(&command.semantics.parameters));
        }
        OperationKind::Query(query) => {
            inputs.extend(query.filters.iter());
            inputs.extend(parameter_fields(&query.semantics.parameters));
        }
        OperationKind::Transition(transition) => {
            inputs.extend(transition.fields.iter());
            inputs.extend(parameter_fields(&transition.semantics.parameters));
            inputs.extend(transition.semantics.select.iter());
        }
        OperationKind::Event(_) => return,
    }
    let allow_transition_version = matches!(
        kind,
        OperationKind::Transition(transition)
            if matches!(
                transition.semantics.precondition,
                Some(Precondition::Required | Precondition::Optional)
            )
    );
    let mut diagnosed = BTreeSet::new();
    for id in inputs {
        let Some(field) = fields.get(id) else {
            continue;
        };
        let reason = if field.semantics.scope.is_some() {
            Some("scope fields come from execution context")
        } else if field.semantics.updated {
            Some("updated fields are compiler-managed")
        } else if field.semantics.version && !allow_transition_version {
            Some("version is request-visible only through an if-match precondition")
        } else {
            None
        };
        if let Some(reason) = reason
            && diagnosed.insert(field.id.clone())
        {
            linker.problem(
                "model-managed-field-input",
                format!("{path}.semantics.parameters"),
                format!(
                    "field `{}` cannot be a request input: {reason}",
                    field.label
                ),
                "remove the field parameter and let the compiler supply it",
            );
        }
    }
}

fn validate_targets(
    kind: &OperationKind,
    fields: &BTreeMap<&FieldId, &Field>,
    path: &str,
    linker: &mut Linker,
) {
    let mut targets = Vec::new();
    match kind {
        OperationKind::Command(command) => {
            targets.extend(
                command
                    .semantics
                    .assignments
                    .iter()
                    .map(|assignment| &assignment.field),
            );
            targets.extend(
                command
                    .semantics
                    .resolutions
                    .iter()
                    .map(|resolution| &resolution.target),
            );
        }
        OperationKind::Transition(transition) => {
            // Only `update` names a field the transition writes. This also
            // extended `transition.sets`, which the JDL frontend filled with
            // every parameter whenever `update` was omitted -- so a row
            // selector or an `@version` guard was reported as a managed-field
            // write, and `jdl-sol.md` §4's `transition Complete(id, version)`
            // could not link.
            targets.extend(transition.semantics.update.iter());
            targets.extend(
                transition
                    .semantics
                    .assignments
                    .iter()
                    .map(|assignment| &assignment.field),
            );
        }
        OperationKind::Query(_) | OperationKind::Event(_) => return,
    }
    let mut diagnosed = BTreeSet::new();
    for id in targets {
        let Some(field) = fields.get(id) else {
            continue;
        };
        if (field.semantics.scope.is_some() || field.semantics.version || field.semantics.updated)
            && diagnosed.insert(field.id.clone())
        {
            linker.problem(
                "model-managed-field-target",
                format!("{path}.semantics"),
                format!(
                    "field `{}` is compiler-managed and cannot be set or updated",
                    field.label
                ),
                "remove the explicit target and let the compiler manage the field",
            );
        }
    }
}

fn validate_precondition(
    transition: &crate::Transition,
    entity: &Entity,
    path: &str,
    linker: &mut Linker,
) {
    let versions = entity
        .fields
        .values()
        .filter(|field| field.semantics.version)
        .collect::<Vec<_>>();
    match transition.semantics.precondition {
        Some(Precondition::Required | Precondition::Optional) => {
            if versions.len() != 1 {
                linker.problem(
                    "model-transition-version-count",
                    format!("{path}.semantics.precondition"),
                    format!(
                        "if-match requires exactly one version field, but entity `{}` has {}",
                        entity.label,
                        versions.len()
                    ),
                    "declare one required `long @version` field",
                );
                return;
            }
            let version = versions[0];
            let shorthand = transition
                .semantics
                .parameters
                .iter()
                .filter(|parameter| {
                    matches!(
                        &parameter.source,
                        ParameterSource::Field(field) if field.field == version.id
                    )
                })
                .count();
            if shorthand != 1 {
                linker.problem(
                    "model-transition-version-parameter",
                    format!("{path}.semantics.parameters"),
                    format!(
                        "if-match needs one shorthand parameter for version field `{}`, found {shorthand}",
                        version.label
                    ),
                    "include the version field once in the transition parameter list",
                );
            }
        }
        Some(Precondition::None) => {
            let exposes_version = transition.semantics.parameters.iter().any(|parameter| {
                matches!(
                    &parameter.source,
                    ParameterSource::Field(field)
                        if versions.iter().any(|version| version.id == field.field)
                )
            });
            if exposes_version {
                linker.problem(
                    "model-transition-version-parameter",
                    format!("{path}.semantics.parameters"),
                    "`if-match none` forbids a version parameter",
                    "remove the version parameter or choose required/optional",
                );
            }
        }
        None => {}
    }
}

fn parameter_fields(parameters: &[crate::OperationParameter]) -> impl Iterator<Item = &FieldId> {
    parameters
        .iter()
        .filter_map(|parameter| match &parameter.source {
            ParameterSource::Field(field) => Some(&field.field),
            ParameterSource::Typed(_) => None,
        })
}
