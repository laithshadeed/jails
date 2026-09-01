//! Canonical resource evolution frontends.

use crate::Invocation;
use crate::cli::{GenerateArgs, ResourceFieldCommand};
use crate::model_generate::{
    PreparedMutation, field_declaration, finish_generation_with_reader_paths, parse_field,
};
use jails_model::{Facet, FieldAddPolicy, FieldId, ModelPatch};
use jails_support::{Failure, Result};
use serde_json::json;
use std::path::PathBuf;

pub(crate) fn run(command: ResourceFieldCommand, invocation: Invocation) -> Result<()> {
    // Every arm below evolves a declared field, so the project needs a model
    // before any of them has anything to evolve.
    crate::model_command::ensure_owned(invocation.clone())?;
    match command {
        ResourceFieldCommand::Add {
            entity,
            field_spec,
            default_literal,
            backfill_file,
            package,
        } => add_field(
            AddFieldRequest {
                entity,
                field_spec,
                default_literal,
                backfill_file,
                package,
            },
            invocation,
        ),
        ResourceFieldCommand::Rename {
            entity,
            field,
            new_name,
            column,
            package,
        } => crate::model_field_evolution::rename(
            crate::model_field_evolution::RenameRequest {
                entity,
                field,
                new_name,
                column,
                package,
            },
            invocation,
        ),
        ResourceFieldCommand::Type {
            entity,
            field,
            to,
            strategy,
            package,
        } => crate::model_field_evolution::change_type(
            crate::model_field_evolution::TypeRequest {
                entity,
                field,
                to,
                strategy,
                package,
            },
            invocation,
        ),
        ResourceFieldCommand::Nullability {
            entity,
            field,
            nullable,
            required,
            backfill_file,
            package,
        } => crate::model_field_evolution::set_nullability(
            crate::model_field_evolution::NullabilityRequest {
                entity,
                field,
                nullable,
                required,
                backfill_file,
                package,
            },
            invocation,
        ),
        ResourceFieldCommand::Drop {
            entity,
            field,
            confirm_column,
            package,
        } => crate::model_field_evolution::drop_field(
            crate::model_field_evolution::DropRequest {
                entity,
                field,
                confirm_column,
                package,
            },
            invocation,
        ),
    }
}

pub(crate) fn java_to_label(value: &str) -> String {
    let mut output = String::new();
    for (index, character) in value.chars().enumerate() {
        if character.is_ascii_uppercase() {
            if index > 0 {
                output.push('_');
            }
            output.push(character.to_ascii_lowercase());
        } else if character == '-' {
            output.push('_');
        } else {
            output.push(character);
        }
    }
    output
}

pub(crate) struct AddFieldRequest {
    pub(crate) entity: String,
    pub(crate) field_spec: String,
    pub(crate) default_literal: Option<String>,
    pub(crate) backfill_file: Option<String>,
    pub(crate) package: Option<String>,
}

pub(crate) fn add_generated_field(args: GenerateArgs, invocation: Invocation) -> Result<()> {
    if args.fields.len() != 1 {
        return Err(Failure::Told(
            "canonical `g field` needs exactly one field spec\n       fix: run `jails g field Entity name:type`"
                .to_string(),
        ));
    }
    add_field(
        AddFieldRequest {
            entity: args.name,
            field_spec: args.fields[0].clone(),
            default_literal: args.default_literal,
            backfill_file: args.backfill_file,
            package: args.package,
        },
        invocation,
    )
}

pub(crate) fn add_field(request: AddFieldRequest, invocation: Invocation) -> Result<()> {
    let jdl = crate::model_command::owns_jdl();
    let model_path = PathBuf::from(if jdl {
        crate::model_command::JDL_PATH
    } else {
        crate::model_command::TOML_PATH
    });
    if request.package.is_some() {
        return Err(Failure::Told(
            "canonical entities have one stable identity and do not accept a legacy package selector.\n       fix: remove `--package` and name the entity declared in the application model"
                .to_string(),
        ));
    }
    let current_source = crate::model_command::read_source(&model_path)?;
    let current_model = parse_model(&current_source, jdl)?;
    let has_database = current_model
        .capabilities
        .values()
        .any(|capability| capability.kind == "db");
    let requested_label = java_to_label(&request.entity);
    let entity = current_model
        .entities
        .values()
        .find(|entity| {
            entity.label == requested_label || entity.names.java_type == request.entity
        })
        .ok_or_else(|| {
            Failure::Told(format!(
                "canonical entity `{}` does not exist.\n       fix: name an entity declared under `[entities]`",
                request.entity
            ))
        })?;
    // **Whether a field add touches SQL is a question about this entity, not
    // about the project.** A source-only record has no table whatever else the
    // project stores, so adding a field to one is a Java projection change with
    // no migration and nothing to backfill -- which is exactly what the policy
    // below already does for a project with no database at all, and what its
    // "a source-only record has no rows to backfill" arm already says.
    //
    // `has_database` was standing in for that, and it is only equivalent while
    // every entity in a stored project is itself stored. A refusal forced that
    // assumption to hold, at the cost of making the same operation on the same
    // kind of entity depend on an unrelated project property: `g record` then
    // `g field` works in a project with no database and was refused in one that
    // has a database elsewhere. Asking the entity removes the special case
    // rather than moving it.
    let stored = has_database && entity.facets.contains(&Facet::Repository);
    let entity_id = entity.id.clone();
    let entity_label = entity.label.clone();
    let entity_java_name = entity.names.java_type.clone();
    let parsed = parse_field(&request.field_spec)?;
    if parsed.primary_key {
        return Err(Failure::Told(
            "a field-add patch cannot replace the accepted primary key.\n       fix: add an ordinary field, or model identity change as an explicit evolution program"
                .to_string(),
        ));
    }
    // **Before the data plan, not after it.** A component that is already
    // declared cannot be added whatever backfill accompanies it, and asking
    // for one first told the reader to supply a `--default-literal` for a
    // field that was already there -- an instruction that cannot succeed.
    if entity
        .fields
        .iter()
        .any(|field| field.names.java_member == parsed.java_name || field.label == parsed.label)
    {
        return Err(Failure::Told(format!(
            "`{entity_java_name}` already has a `{}` component.\n       fix: change it with `jails resource field type|nullability|rename`, or drop it first",
            parsed.java_name
        )));
    }
    let (policy, reader_paths) = match (
        &request.default_literal,
        request.backfill_file.as_deref(),
        parsed.required,
        stored,
    ) {
        (Some(value), None, true, true) => {
            (FieldAddPolicy::BackfillLiteral(value.clone()), Vec::new())
        }
        (None, Some(path), true, true) => {
            let (path, bytes) = crate::model_field_evolution::read_reader_sql(path)?;
            (FieldAddPolicy::ReaderOwnedSql(bytes), vec![path])
        }
        (None, None, false, _) | (None, None, true, false) => {
            (FieldAddPolicy::Nullable, Vec::new())
        }
        (None, None, true, true) => {
            return Err(Failure::Told(format!(
                "required field `{}` needs a backfill for existing rows.\n       fix: pass `--default-literal <typed-value>`, `--backfill-file <project-path>`, or declare it with `?`",
                parsed.java_name
            )));
        }
        (Some(_), None, false, _) | (None, Some(_), false, _) => {
            return Err(Failure::Told(format!(
                "nullable field `{}` does not need a mandatory backfill.\n       fix: remove the backfill option or make the field required",
                parsed.java_name
            )));
        }
        (Some(_), None, true, false) | (None, Some(_), true, false) => return Err(Failure::Told(
            "a source-only record has no rows to backfill.\n       fix: remove `--default-literal`"
                .to_string(),
        )),
        (Some(_), Some(_), _, _) => return Err(Failure::Told(
            "choose only one backfill source.\n       fix: pass either `--default-literal` or `--backfill-file`"
                .to_string(),
        )),
    };
    // Where re-parsing the source we are about to write will put this field.
    //
    // **Both JDL dialects state field order now.** v1 always did -- its parser
    // records the order its CST walked. The pre-v1 draft did not, because it
    // reaches the linker by rendering intermediate TOML and a TOML table is
    // unordered; `audit.md` A2.2b fixed that by having the renderer carry
    // `field_order`, so an appended declaration in either dialect stays
    // appended.
    //
    // `.jails/model.toml` is what is left, and it keeps `ByLabel` deliberately:
    // it is the temporary compatibility input, its writer states no order, and
    // giving one to a format on the cutover's deletion list would be adding
    // surface to something being removed.
    let placement = if jdl {
        jails_model::FieldPlacement::Last
    } else {
        jails_model::FieldPlacement::ByLabel
    };
    let next_source = if jdl {
        let line = crate::model_generate_jdl::render_v1_field_line(&entity_label, &parsed);
        crate::model_generate_jdl::insert_field(&current_source, &entity_java_name, &line)?
    } else {
        let mut next = current_source.clone();
        if !next.ends_with('\n') {
            next.push('\n');
        }
        next.push('\n');
        next.push_str(&field_declaration(&entity_label, &parsed)?);
        next
    };
    let next_model = parse_model(&next_source, jdl)?;
    let field_id =
        FieldId::parse(format!("fld_{entity_label}_{}", parsed.label)).map_err(Failure::Told)?;
    let field = next_model
        .entities
        .get(&entity_id)
        .and_then(|entity| entity.field(&field_id))
        .cloned()
        .ok_or_else(|| Failure::Told(format!("new field `{field_id}` did not link")))?;
    let patch_bytes = serde_json::to_vec(&json!({
        "kind": "add-field",
        "entity": entity_id,
        "field": field,
        "policy": match &policy {
            FieldAddPolicy::Nullable => json!({"kind": "nullable"}),
            FieldAddPolicy::BackfillLiteral(value) => {
                json!({"kind": "backfill-literal", "value": value})
            }
            FieldAddPolicy::ReaderOwnedSql(bytes) => {
                json!({"kind": "reader-owned-sql", "bytes": bytes})
            }
        },
    }))
    .map_err(|error| Failure::Told(format!("could not encode model patch: {error}")))?;
    finish_generation_with_reader_paths(
        PreparedMutation {
            name: format!("{}.{}", request.entity, parsed.java_name),
            invocation,
            model_path,
            current_source,
            current_model,
            next_source,
            patch: ModelPatch::AddField {
                entity: entity_id,
                field,
                policy,
                placement,
            },
            patch_bytes,
            authored_migration: None,
        },
        &reader_paths,
    )
}

fn parse_model(source: &str, jdl: bool) -> Result<jails_model::AppModel> {
    let parsed = if jdl {
        jails_model::parse_jdl(source)
    } else {
        jails_model::parse_toml(source)
    };
    parsed.map_err(|diagnostics| Failure::Told(diagnostics.to_string().trim_end().to_string()))
}
