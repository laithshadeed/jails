//! Canonical resource evolution frontends.

use crate::Invocation;
use crate::cli::{GenerateArgs, ResourceFieldCommand};
use crate::dispatch;
use crate::model_generate::{
    PreparedMutation, field_declaration, finish_generation_with_reader_paths, parse_field,
};
use jails_model::{Facet, FieldAddPolicy, FieldId, ModelPatch};
use jails_support::{Failure, Result};
use serde_json::json;
use std::path::PathBuf;

pub(crate) fn owns() -> bool {
    crate::model_command::owns()
}

pub(crate) fn run(command: ResourceFieldCommand, invocation: Invocation) -> Result<()> {
    match command {
        ResourceFieldCommand::Add {
            entity,
            field_spec,
            default_literal,
            backfill_file,
            package,
        } => {
            if owns() {
                return add_field(
                    AddFieldRequest {
                        entity,
                        field_spec,
                        default_literal,
                        backfill_file,
                        package,
                    },
                    invocation,
                );
            }
            dispatch::mutate(invocation, false, |run| {
                jails_engine::route::add_field_with_data(
                    run,
                    &entity,
                    &field_spec,
                    package.as_deref(),
                    default_literal.as_deref(),
                    backfill_file.as_deref(),
                )
            })
        }
        ResourceFieldCommand::Rename {
            entity,
            field,
            new_name,
            column,
            package,
        } if owns() => crate::model_field_evolution::rename(
            crate::model_field_evolution::RenameRequest {
                entity,
                field,
                new_name,
                column,
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
        } => dispatch::mutate(invocation, false, |run| {
            jails_engine::route::rename_field(
                run,
                &entity,
                &field,
                &new_name,
                column.into(),
                package.as_deref(),
            )
        }),
        ResourceFieldCommand::Type {
            entity,
            field,
            to,
            strategy,
            package,
        } if owns() => crate::model_field_evolution::change_type(
            crate::model_field_evolution::TypeRequest {
                entity,
                field,
                to,
                strategy,
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
        } => dispatch::mutate(invocation, false, |run| {
            jails_engine::route::change_field_type(
                run,
                &entity,
                &field,
                &to,
                strategy.into(),
                package.as_deref(),
            )
        }),
        ResourceFieldCommand::Nullability {
            entity,
            field,
            nullable,
            required,
            backfill_file,
            package,
        } if owns() => crate::model_field_evolution::set_nullability(
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
        ResourceFieldCommand::Nullability {
            entity,
            field,
            nullable,
            required: _,
            backfill_file,
            package,
        } => dispatch::mutate(invocation, false, |run| {
            jails_engine::route::set_field_nullability_with_data(
                run,
                &entity,
                &field,
                nullable,
                backfill_file.as_deref(),
                package.as_deref(),
            )
        }),
        ResourceFieldCommand::Drop {
            entity,
            field,
            confirm_column,
            package,
        } if owns() => crate::model_field_evolution::drop_field(
            crate::model_field_evolution::DropRequest {
                entity,
                field,
                confirm_column,
                package,
            },
            invocation,
        ),
        ResourceFieldCommand::Drop {
            entity,
            field,
            confirm_column,
            package,
        } => dispatch::mutate(invocation, false, |run| {
            jails_engine::route::drop_field(
                run,
                &entity,
                &field,
                &confirm_column,
                package.as_deref(),
            )
        }),
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
    let current_source = std::fs::read_to_string(&model_path).map_err(|error| {
        Failure::Told(format!(
            "could not read canonical model `{}`: {error}",
            model_path.display()
        ))
    })?;
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
    if has_database && !entity.facets.contains(&Facet::Repository) {
        return Err(Failure::Told(format!(
            "canonical entity `{}` has no stored repository facet.\n       fix: evolve a stored entity, or edit a source-only record declaration directly",
            entity.label
        )));
    }
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
    let (policy, reader_paths) = match (
        &request.default_literal,
        request.backfill_file.as_deref(),
        parsed.required,
        has_database,
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
    // Only JDL v1 states field order: its parser records the order its CST
    // walked, so an appended declaration stays appended. A `.jails/model.toml`
    // table states none, and the pre-v1 JDL draft reaches the linker by
    // rendering that same TOML, so both re-parse sorted by label whatever
    // order they were written in.
    let placement = if jdl && crate::model_generate_jdl::is_v1_source(&current_source) {
        jails_model::FieldPlacement::Last
    } else {
        jails_model::FieldPlacement::ByLabel
    };
    let next_source = if jdl {
        let line = if crate::model_generate_jdl::is_v1_source(&current_source) {
            crate::model_generate_jdl::render_v1_field_line(&entity_label, &parsed)
        } else {
            crate::model_generate_jdl::render_field_line(&entity_label, &parsed)?
        };
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
