//! An `AppModel`, written back out as JDL v1.
//!
//! **This is the one exit for the `.jails/model.toml` compatibility input.**
//! JDL v1 §22 has TOML model state "imported into the same v1 AST through a
//! separate one-shot command", and that command needs a model rendered as v1
//! source: [`super::render`] goes the other way (parsed JDL down to the TOML
//! boundary) and [`super::upgrade`] rewrites pre-v1 *text* rather than
//! taking a model.
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

/// The projection that carries a facet, where one does.
///
/// **One authority, two callers** -- the same shape as
/// [`storage_capability`]. The renderer refuses an entity whose facets its
/// projections do not carry; the upgrade off `.jails/model.toml` materialises
/// those projections, because the TOML front end sets facets directly and v1
/// derives them. `Record` is every entity's by default and `Enum` is not an
/// entity facet, so neither needs one; `Search` and `Events` return `None`
/// because a facet carries neither a field list nor an operation set.
///
/// **Typed rather than a label**, so the upgrade can hand the answer straight
/// to `AddProjection` instead of mapping a `use` spelling back through a
/// match that needs a refusal arm for a case that cannot happen.
pub fn projection_for_facet(facet: crate::Facet) -> Option<crate::ProjectionKind> {
    match facet {
        crate::Facet::Repository => Some(crate::ProjectionKind::Repository),
        crate::Facet::Service => Some(crate::ProjectionKind::Service),
        crate::Facet::Http => Some(crate::ProjectionKind::Http { path: None }),
        crate::Facet::Dto => Some(crate::ProjectionKind::Dto),
        crate::Facet::Factory => Some(crate::ProjectionKind::Factory),
        crate::Facet::Seed => Some(crate::ProjectionKind::Seed),
        crate::Facet::Record | crate::Facet::Enum | crate::Facet::Search | crate::Facet::Events => {
            None
        }
    }
}

/// The capability a primary storage axis materialises, if any.
///
/// **One authority, two callers.** JDL v1 reads `storage postgres` as a `db`
/// capability, so the renderer must not emit a redundant `cap db` -- and the
/// upgrade off `.jails/model.toml` must *add* one, because the TOML dialect is
/// not a capability and JDL v1 §22 records that difference as a note the reviewer
/// reads. Both need the same mapping, and a second copy of it is how a project
/// upgrades into a model with a JDBC adapter nobody mentioned.
pub fn storage_capability(dialect: &str) -> Option<&'static str> {
    declarations::derived_capability(dialect)
}

fn write_document(model: &AppModel, refusals: &mut Vec<Diagnostic>) -> String {
    let mut out = String::new();
    out.push_str("jdl 1\n\n");
    write_app(model, &mut out, refusals);
    write_project_declarations(model, &mut out, refusals);
    write_enums(model, &mut out);
    write_entities(model, &mut out, refusals);
    write_top_level_operations(model, &mut out, refusals);
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

    /// The constructs the golden corpus never exercises.
    ///
    /// **The golden corpus is only as strong as what its models contain**,
    /// and twelve constructs appear in none of them: `dep`,
    /// `prop`, `eject`, a composite `unique`, `@scope(claim:)`, `@updated`,
    /// `@length`, `@retired`, `@internal`, `partition by`, `use value` and an
    /// enum wire value. The corpus is what the *tool* emits; this is what the
    /// *language* allows, and the renderer has to survive both.
    #[test]
    fn the_constructs_no_golden_model_carries_survive_the_round_trip() {
        const COVERAGE: &str = r#"jdl 1

app Coverage @id(project_coverage) {
  pkg com.example.coverage
  java 26
  platform spring
  build maven
  storage postgres
}

cap security
cap kafka

dep org.example:widget @version("1.2.3") @scope(test)

prop server.port = "8080"
prop logging.level.root = "INFO" @target(test)

enum Status {
  OPEN
  IN_PROGRESS = "in_progress"
}

entity Archived @retired {
  id: uuid @pk
}

entity Order {
  use value
  use repo, service, dto

  id: uuid @pk
  tenantId: uuid @scope(claim: "org")
  reference: string @length(2..40) @notBlank
  status: Status @default(OPEN)
  version: long @version
  updatedAt: instant @default(now()) @updated

  unique [tenantId, reference]

  transition Close(id, status, version) @internal {
    select [id]
    update [status]
    if-match required
  }
}

event OrderClosed(id: uuid, at: instant) {
  partition by id
}

eject cmp_service_pricing
component service Pricing
"#;
        let model = super::super::parse(COVERAGE).expect("the coverage fixture links");
        render(&model).expect("every construct the language allows must round trip");
    }

    /// The specification's own flagship example, rendered and linked back.
    ///
    /// JDL v1 §21 makes the §4 example an executable fixture, and it carries
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
        // **The one line that does not link, removed rather than skipped.**
        // JDL v1 §16.4's readable ejection path is a recorded gap --
        // `tests/cli`'s
        // `the_specification_complete_example_links_except_its_one_recorded_gap`
        // pins both halves of it -- and a test that bails out when the
        // example fails to link asserts nothing and reports green.
        let example = example.replace("eject Task.repo.fake\n", "");
        let model =
            super::super::parse(&example).expect("§4 links once its one recorded gap is removed");
        render(&model).expect("the specification's own example must round trip");
    }

    /// **The compatibility spelling that is authority, not a projection.**
    /// `.jails/model.toml` may state an operation's inputs as a flat `fields`
    /// list with no parameters, and `emit_java::input` reads exactly that list
    /// when the rich one is empty -- so a renderer that emitted
    /// `command CreateNote()` would drop the request's whole shape and the
    /// round trip would catch it as an unhelpful "does not reproduce its
    /// operations". It refuses by name instead, and `jails model upgrade`
    /// materialises the parameters where the change can be said out loud.
    #[test]
    fn a_flat_input_list_with_no_parameters_refuses_by_name() {
        const TOML: &str = r#"
schema = "jails.model.v1"

[project]
id = "project_notes"
name = "Notes"
base_package = "com.example.notes"
java_release = 26
dialect = "none"

[entities.note]
id = "ent_note"
facets = ["record"]

[entities.note.fields.id]
id = "fld_note_id"
type = "uuid"
primary_key = true

[entities.note.fields.title]
id = "fld_note_title"
type = "string"

[operations.create_note]
kind = "command"
id = "op_create_note"
on = "note"
fields = ["title"]
"#;
        let model = crate::parse_toml(TOML).expect("the compatibility input links");
        let refused = render(&model).expect_err("a flat input list has no v1 spelling");
        let told = refused.to_string();
        assert!(
            told.contains("$.operations.create_note.fields"),
            "the refusal does not name the construct:\n{told}"
        );
        assert!(
            told.contains("no parameters to carry it"),
            "the refusal does not say why:\n{told}"
        );
        // The same model with parameters is renderable, so the refusal is
        // about the flat spelling rather than about the operation.
        let mut stated = model.clone();
        if let Some(operation) = stated.operations.values_mut().next()
            && let crate::OperationKind::Command(command) = &mut operation.kind
        {
            command.semantics.parameters = vec![crate::OperationParameter {
                name: "title".to_string(),
                source: crate::ParameterSource::Field(crate::VisibleField {
                    entity: command.on.clone(),
                    field: command.fields[0].clone(),
                    qualifier: None,
                }),
                required: true,
                optional_filter: false,
                constraints: crate::ParameterConstraints::default(),
            }];
        }
        let rendered = render(&stated).expect("parameters are statable");
        assert!(rendered.contains("command CreateNote(title)"), "{rendered}");
    }

    /// Every model in the golden corpus, rendered and linked back.
    ///
    /// **A hand-written case proves the constructs somebody thought of.** The
    /// goldens are the models the tool actually produces, across every
    /// generator kind and capability, so they cover the combinations nobody
    /// would write down -- and [`render`] refuses rather than returns on a
    /// mismatch, so this asserts the refusal never fires rather than
    /// re-deriving the comparison here.
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
