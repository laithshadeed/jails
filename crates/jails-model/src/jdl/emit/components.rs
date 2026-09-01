//! Typed generic components, and the references they carry.

use super::operations::{constraint_attributes, write_bindings, write_route};
use super::{json, refuse, type_token};
use crate::Diagnostic;
use crate::id::StableId;
use crate::model::AppModel;
use std::fmt::Write as _;

pub(super) fn write_components(model: &AppModel, out: &mut String, refusals: &mut Vec<Diagnostic>) {
    for component in model.components.values() {
        let parameters = component
            .parameters
            .iter()
            .map(|parameter| {
                let mut rendered = format!(
                    "{}: {}{}",
                    parameter.name,
                    type_token(&parameter.ty),
                    if parameter.required { "" } else { "?" }
                );
                rendered.push_str(&constraint_attributes(&parameter.constraints));
                rendered
            })
            .collect::<Vec<_>>()
            .join(", ");
        let _ = writeln!(
            out,
            "component {} {}({parameters}) @id({}) {{",
            component.kind.label(),
            component.name,
            component.id.as_str()
        );
        if let Some(reference) = &component.on {
            let _ = writeln!(out, "  on {}", reference_name(model, reference, refusals));
        }
        if let Some(reference) = &component.yields {
            let _ = writeln!(
                out,
                "  yields {}",
                reference_name(model, reference, refusals)
            );
        }
        write_route(out, "  ", component.route.as_ref());
        write_bindings(out, "  ", &component.bindings);
        for variant in &component.variants {
            let _ = writeln!(
                out,
                "  variant {} @id({})",
                variant.name,
                variant.id.as_str()
            );
        }
        if let Some(source) = &component.source {
            let _ = writeln!(out, "  source {}", json(source));
        }
        out.push_str("}\n\n");
    }
}

fn reference_name(
    model: &AppModel,
    reference: &crate::ComponentReference,
    refusals: &mut Vec<Diagnostic>,
) -> String {
    match reference {
        crate::ComponentReference::Entity(id) => model
            .entities
            .get(id)
            .map(|entity| entity.names.java_type.clone())
            .unwrap_or_default(),
        crate::ComponentReference::Operation(id) => model
            .operations
            .get(id)
            .map(|operation| operation.names.java_type.clone())
            .unwrap_or_default(),
        crate::ComponentReference::Component(id) => model
            .components
            .get(id)
            .map(|component| component.name.clone())
            .unwrap_or_else(|| {
                refuse(
                    refusals,
                    "$.components",
                    "a component reference to a component that is not in this model",
                    "restore the referenced component before upgrading",
                );
                String::new()
            }),
    }
}
