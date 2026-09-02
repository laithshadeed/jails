//! Canonical resource evolution frontends.

use crate::Invocation;
use crate::cli::{GenerateArgs, ResourceFieldCommand};
use crate::model_generate::{PreparedMutation, finish_generation, parse_field};
use jails_model::field_syntax::java_to_label;
use jails_model::{Evolution, EvolutionStep, Facet, FieldAddPolicy, FieldId};
use jails_support::{Failure, Result};

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
    if request.package.is_some() {
        return Err(Failure::Told(
            "canonical entities have one stable identity and do not accept a legacy package selector.\n       fix: remove `--package` and name the entity declared in the application model"
                .to_string(),
        ));
    }
    let current = crate::model_command::Current::load(&invocation)?;
    let has_database = current
        .model
        .capabilities
        .values()
        .any(|capability| capability.kind == "db");
    let requested_label = java_to_label(&request.entity);
    let entity = current.model
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
    // no migration and nothing to backfill -- the same answer the policy below
    // gives a project with no database at all. `has_database` alone is only
    // equivalent while every entity in a stored project is itself stored, and
    // `g record` then `g field` must not depend on an unrelated project
    // property.
    let stored = has_database && entity.facets.contains(&Facet::Repository);
    entity.refuse_retired().map_err(Failure::Told)?;
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
    // for one first would tell the reader to supply a `--default-literal` for
    // a field that is already there -- an instruction that cannot succeed.
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
    let line = crate::model_generate_jdl::render_v1_field_line(&entity_label, &parsed);
    let next_source =
        crate::model_generate_jdl::insert_field(&current.source, &entity_java_name, &line)?;
    let field_id =
        FieldId::parse(format!("fld_{entity_label}_{}", parsed.label)).map_err(Failure::Told)?;
    finish_generation(PreparedMutation {
        name: format!("{}.{}", request.entity, parsed.java_name),
        invocation,
        current,
        next_source,
        evolution: Evolution::one(EvolutionStep::AddField {
            field: field_id,
            policy,
        }),
        authored_migration: None,
        reader_paths,
    })
}
