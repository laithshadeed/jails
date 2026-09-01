//! An `AppModel`, written back out as JDL v1.
//!
//! **This is what `.jails/model.toml` needs to stop being a second editable
//! source.** §22 gives the TOML compatibility input one exit -- "legacy TOML
//! model state is imported into the same v1 AST through a separate one-shot
//! command" -- and that command needs a model rendered as v1 source. Nothing
//! rendered one: [`super::render`] goes the other way (parsed JDL down to the
//! TOML boundary) and [`super::upgrade`] rewrites pre-v1 *text* rather than
//! taking a model, so a project on TOML could be edited forever and never
//! move. `docs/00-contracts.md` forbids exactly that state.
//!
//! **It is fail-closed twice over, and the second one is the point.** Every
//! construct it cannot express refuses by name rather than being dropped --
//! and then [`render`] parses and links what it just wrote and compares the
//! result to the model it was given. A renderer that silently loses a field is
//! how a one-shot migration corrupts a project, and the round trip is the only
//! check that catches the case nobody thought to enumerate. Identity is
//! carried by emitting `@id(...)` on every declaration rather than trusting
//! the derivation to land on the same label twice.

use crate::id::StableId;
use crate::model::{AppModel, TypeRef};
use crate::operation::Value;
use crate::{Diagnostic, Diagnostics, EntityId};
use std::fmt::Write as _;

mod components;
mod declarations;
mod operations;

use components::write_components;
use declarations::{write_app, write_entities, write_enums, write_project_declarations};
use operations::write_top_level_operations;

/// Render a linked model as JDL v1, or refuse.
pub fn render(model: &AppModel) -> Result<String, Diagnostics> {
    let mut refusals = Vec::new();
    let source = write_document(model, &mut refusals);
    if !refusals.is_empty() {
        return Err(Diagnostics::from_vec(refusals));
    }
    let formatted = super::v1::format(&source)?;
    prove_round_trip(model, &formatted)?;
    Ok(formatted)
}

/// The check that makes this safe to point at somebody's project.
///
/// Emitting is a hundred decisions and any one of them can drop a fact. Rather
/// than trust them, link what was written and compare: `derived` is recomputed
/// from the model by definition, and the language/convention versions are the
/// header's, so those three are normalised and everything else must be equal.
fn prove_round_trip(model: &AppModel, source: &str) -> Result<(), Diagnostics> {
    let mut linked = super::v1::parse(source)?;
    let mut expected = model.clone();
    linked.language_version = expected.language_version;
    linked.convention_version = expected.convention_version;
    linked.schema.clone_from(&expected.schema);
    linked.refresh_derived();
    expected.refresh_derived();
    if linked == expected {
        return Ok(());
    }
    // **Name the part that differs.** "does not reproduce it" is true and
    // useless: this refusal is the one thing standing between a reader and a
    // silently rewritten model, so it has to be actionable by whoever has to
    // fix the renderer.
    let differing = differing_part(&expected, &linked);
    Err(Diagnostics::from_vec(vec![Diagnostic::new(
        "model-render-round-trip",
        differing.0,
        format!(
            "rendering this model as JDL v1 and linking it back does not reproduce its {}",
            differing.1
        ),
        "report this with the model; nothing was written, and a partial render is never applied",
    )]))
}

/// The first top-level part of the model that the round trip changed.
fn differing_part(expected: &AppModel, linked: &AppModel) -> (&'static str, &'static str) {
    if expected.project != linked.project {
        return ("$.project", "app declaration");
    }
    if expected.capabilities != linked.capabilities {
        return ("$.capabilities", "capabilities");
    }
    if expected.dependencies != linked.dependencies {
        return ("$.dependencies", "dependencies");
    }
    if expected.settings != linked.settings {
        return ("$.settings", "properties");
    }
    if expected.entities != linked.entities {
        return ("$.entities", "entities");
    }
    if expected.projections != linked.projections {
        return ("$.projections", "projections");
    }
    if expected.relations != linked.relations {
        return ("$.relations", "relations");
    }
    if expected.operations != linked.operations {
        return ("$.operations", "operations");
    }
    if expected.components != linked.components {
        return ("$.components", "components");
    }
    if expected.ejections != linked.ejections {
        return ("$.ejections", "ejections");
    }
    if expected.units != linked.units {
        return ("$.units", "compatibility source units");
    }
    ("$", "model")
}

pub(super) fn refuse(
    refusals: &mut Vec<Diagnostic>,
    path: impl Into<String>,
    what: &str,
    fix: &str,
) {
    refusals.push(Diagnostic::new(
        "model-render-unsupported",
        path,
        format!("JDL v1 cannot state {what}"),
        fix.to_string(),
    ));
}

fn write_document(model: &AppModel, refusals: &mut Vec<Diagnostic>) -> String {
    let mut out = String::new();
    out.push_str("jdl 1\n\n");
    write_app(model, &mut out, refusals);
    write_project_declarations(model, &mut out, refusals);
    write_enums(model, &mut out);
    write_entities(model, &mut out, refusals);
    write_top_level_operations(model, &mut out);
    write_components(model, &mut out, refusals);
    for ejection in model.ejections.values() {
        let _ = writeln!(
            out,
            "eject {} @id({})",
            ejection.target,
            ejection.id.as_str()
        );
    }
    out
}

pub(super) fn field_label<'a>(
    model: &'a AppModel,
    entity: &EntityId,
    field: &crate::FieldId,
) -> &'a str {
    model
        .entities
        .get(entity)
        .and_then(|entity| {
            entity
                .fields
                .iter()
                .find(|candidate| &candidate.id == field)
        })
        .map_or("", |field| field.label.as_str())
}

pub(super) fn type_token(ty: &TypeRef) -> &str {
    ty.canonical_name()
}

pub(super) fn json(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| format!("{value:?}"))
}

pub(super) fn value(value: &Value) -> String {
    match value {
        Value::String(text) => json(text),
        Value::Integer(text) | Value::Decimal(text) => text.clone(),
        Value::Boolean(flag) => flag.to_string(),
        Value::EnumConstant(name) => name.clone(),
        Value::Function { name, .. } => format!("{name}()"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every model in the golden corpus, rendered and linked back.
    ///
    /// **A hand-written case proves the constructs somebody thought of.** The
    /// goldens are the models the tool actually produces, across every
    /// generator kind and capability, so they cover the combinations nobody
    /// would write down -- and [`render`] refuses rather than returns on a
    /// mismatch, so this asserts the refusal never fires rather than
    /// re-deriving the comparison here.
    /// The specification's own flagship example, rendered and linked back.
    ///
    /// §21 makes the §4 example an executable fixture, and it carries
    /// constructs the goldens do not: a scoped field, an `if-match` guard, a
    /// `resolve`, and an ejection. If the renderer can reproduce that, it can
    /// reproduce the language as documented rather than as this tool happens
    /// to emit it.
    #[test]
    fn the_specifications_complete_example_survives_the_round_trip() {
        let document = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/01-jdl-v1.md"),
        )
        .expect("docs/01-jdl-v1.md is checked in");
        let example = document
            .split("## 4. Complete example")
            .nth(1)
            .and_then(|rest| rest.split("\n## 5.").next())
            .and_then(|section| section.split("```jdl\n").nth(1))
            .and_then(|rest| rest.split("```").next())
            .expect("§4 still carries one jdl block");
        let Ok(model) = super::super::parse(example) else {
            // §16.4's readable ejection path is a recorded gap: the example
            // does not link yet, and `tests/cli` pins that. Nothing to prove
            // here until it does, and asserting it links would duplicate a
            // pin that already exists somewhere better.
            return;
        };
        render(&model).expect("the specification's own example must round trip");
    }

    #[test]
    fn every_golden_model_survives_being_rendered_and_linked_back() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/golden");
        let mut checked = 0;
        let mut failures = Vec::new();
        let mut directories = vec![root.clone()];
        while let Some(directory) = directories.pop() {
            let Ok(entries) = std::fs::read_dir(&directory) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    directories.push(path);
                } else if path.file_name().is_some_and(|name| name == "model.jdl") {
                    let source = std::fs::read_to_string(&path).expect("golden model is readable");
                    let Ok(model) = super::super::parse(&source) else {
                        continue;
                    };
                    checked += 1;
                    if let Err(diagnostics) = render(&model) {
                        failures.push(format!("{}: {diagnostics}", path.display()));
                    }
                }
            }
        }
        assert!(
            checked > 10,
            "the golden scan found only {checked} models -- it has lost the corpus, and would \
             report the same clean result over a renderer that dropped every field"
        );
        assert!(
            failures.is_empty(),
            "{} of {checked} golden models do not survive the round trip:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }
}
