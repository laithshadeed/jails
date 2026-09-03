//! Lossless JDL edits for the familiar `jails generate` surface.

mod component;
mod edit;
pub(crate) mod facet;
pub(crate) mod index;
mod operation;
mod relation;
pub(crate) mod render;
mod unit;
pub(crate) use component::{component_kind, component_stem};
use edit::insert_entity_member;
pub(crate) use edit::{
    insert_field, jdl_edit_failure, remove_capability, remove_dependency, remove_entity,
    remove_operation, remove_setting, remove_unit, rename_entity, set_entity_active,
};
use operation::operation_declaration;
pub(crate) use render::entity_declaration;
pub(crate) use render::{EntityDeclaration, normalize_package};
use render::{enum_declaration, field_label_of, quoted_list};
pub(crate) use render::{java_type_name, relation_member_name, render_v1_field_line};

use crate::ArtifactKind;
use crate::cli::GenerateArgs;
use crate::model_generate::{ParsedField, PreparedMutation, finish_generation, parse_field};
use crate::{Invocation, model_generate};
use jails_model::field_syntax::java_to_label;
use jails_model::{EntityId, Evolution, OperationId};
use jails_support::{Failure, Result};
use std::collections::BTreeSet;

const MODEL_PATH: &str = crate::model_command::JDL_PATH;

/// Every generator kind, and there is deliberately no `_` arm: a kind added
/// without deciding what a canonical project does with it is a compile error
/// rather than a silent fall-through.
pub(crate) fn run(args: GenerateArgs, invocation: Invocation) -> Result<()> {
    // **One identifier check, before the model.** Almost every kind turns its
    // name into a Java type, and the model can only describe the projections
    // a bad name broke -- `2Fast` came back as four linked diagnostics about
    // a label, a type, a table and a route the reader never typed. Two kinds
    // do not name a type: `migration` names a file, and `cases` names the
    // document it reads its examples out of, so both take a path.
    if !matches!(args.kind, ArtifactKind::Migration | ArtifactKind::Cases) {
        model_generate::refuse_non_java_identifier(&args.name)?;
    }
    match args.kind {
        ArtifactKind::Field => crate::model_resource::add_generated_field(args, invocation),
        ArtifactKind::Record
        | ArtifactKind::Value
        | ArtifactKind::Enum
        | ArtifactKind::Scaffold => run_entity(args, invocation),
        ArtifactKind::Factory => facet::run(args, invocation, facet::Kind::Factory),
        ArtifactKind::Dto => facet::run(args, invocation, facet::Kind::Dto),
        ArtifactKind::Repo => facet::run(args, invocation, facet::Kind::Repository),
        ArtifactKind::Seed => facet::run(args, invocation, facet::Kind::Seed),
        ArtifactKind::Search => facet::run(args, invocation, facet::Kind::Search),
        ArtifactKind::Usecase
        | ArtifactKind::Query
        | ArtifactKind::Transition
        | ArtifactKind::Event => run_operation(args, invocation),
        ArtifactKind::Class
        | ArtifactKind::Interface
        | ArtifactKind::Service
        | ArtifactKind::Handler
        | ArtifactKind::Command
        | ArtifactKind::Cli
        | ArtifactKind::Cases
        | ArtifactKind::Client
        | ArtifactKind::Fetcher
        | ArtifactKind::Job
        | ArtifactKind::HttpWorkflow
        | ArtifactKind::HttpSink
        | ArtifactKind::Idempotency
        | ArtifactKind::Auth
        | ArtifactKind::Webhook
        | ArtifactKind::DurableJob
        | ArtifactKind::Socket
        | ArtifactKind::Presence
        | ArtifactKind::Test
        | ArtifactKind::IntegrationTest
        | ArtifactKind::Sealed
        | ArtifactKind::Strategy
        | ArtifactKind::Controller => unit::run(args, invocation),
        ArtifactKind::Association => relation::run(args, invocation),
        ArtifactKind::Migration => crate::model_migration::run(args, invocation),
    }
}

fn run_operation(args: GenerateArgs, invocation: Invocation) -> Result<()> {
    let profile = model_generate::operation_profile(args.kind).ok_or_else(|| {
        Failure::Told(
            "this JDL generator is not an operation declaration\n       fix: use `usecase`, `query`, `transition`, or `event`"
                .to_string(),
        )
    })?;
    model_generate::reject_unsupported_operation_options(&args, profile)?;
    let current = crate::model_command::Current::load(&invocation)?;
    // **An event may name no entity**, and then the block it becomes is a
    // top-level declaration rather than an entity member. Everything below
    // that reads the target -- the managed field list, the borrowed `--via`
    // component, where the block is spliced -- has nothing to read, so the
    // empty label is the honest value rather than a lookup that would fail.
    let (entity_label, entity_java_name) = match args.strategy_on.as_deref() {
        None => (String::new(), String::new()),
        Some(on) => {
            let requested_entity = java_to_label(on);
            let entity = current.model
                .entities
                .values()
                .find(|entity| entity.label == requested_entity || entity.names.java_type == on)
                .ok_or_else(|| {
                    Failure::Told(format!(
                        "`{on}` does not name a entity.\n       fix: choose an entity declared in `{MODEL_PATH}`"
                    ))
                })?;
            (entity.label.clone(), entity.names.java_type.clone())
        }
    };
    let standalone = entity_java_name.is_empty();
    // An event's payload can carry a component the row does not: see
    // `event_component_declarations`. Every other operation's field list is a
    // projection of the target, so it goes through the checked resolver.
    let fields = if args.kind == ArtifactKind::Event {
        model_generate::event_component_declarations(&current.model, &entity_label, &args.fields)?
    } else {
        model_generate::operation_field_labels_via(
            &current.model,
            &entity_label,
            args.via
                .as_deref()
                .map(java_type_name)
                .as_deref()
                .map(java_to_label)
                .as_deref(),
            args.kind == ArtifactKind::Query,
            &args.fields,
        )?
    };
    let operation_label = java_to_label(&args.name);
    let operation_id = OperationId::parse(format!("op_{operation_label}"))
        .map_err(|error| Failure::Told(format!("could not assign operation identity: {error}")))?;
    let declaration = operation_declaration(&args, &current.model, &entity_label, &fields)?;
    // The same block one level out. An entity member is rendered nested; a
    // top-level declaration is the identical text without that indent, so it
    // is one transform rather than a second renderer.
    let declaration = match standalone {
        true => declaration
            .lines()
            .map(|line| line.strip_prefix("  ").unwrap_or(line))
            .collect::<Vec<_>>()
            .join("\n"),
        false => declaration,
    };
    let splice = |source: &str| -> Result<String> {
        match standalone {
            true => {
                let mut next = source.to_string();
                if !next.ends_with('\n') {
                    next.push('\n');
                }
                next.push('\n');
                next.push_str(declaration.trim_end());
                next.push('\n');
                Ok(next)
            }
            false => insert_entity_member(source, &entity_java_name, &declaration),
        }
    };
    if let Some(existing) = current.model.operations.get(&operation_id) {
        let without = remove_operation(&current.source, &args.name)?;
        let requested_source = splice(&without)?;
        let requested_model = parse(&requested_source)?;
        let requested = requested_model
            .operations
            .get(&operation_id)
            .ok_or_else(|| {
                Failure::Told(format!("requested operation `{operation_id}` did not link"))
            })?;
        if existing != requested {
            return Err(Failure::Told(format!(
                "operation `{}` is already declared with a different shape.\n       fix: remove it explicitly before changing what it declares",
                args.name
            )));
        }
        return finish_generation(PreparedMutation {
            name: args.name,
            invocation,
            next_source: current.source.clone(),
            current,
            evolution: Evolution::none(),
            authored_migration: None,
            reader_paths: Vec::new(),
        });
    }
    let next_source = splice(&current.source)?;
    finish_generation(PreparedMutation {
        name: args.name,
        invocation,
        current,
        next_source,
        evolution: Evolution::none(),
        authored_migration: None,
        reader_paths: Vec::new(),
    })
}

fn run_entity(mut args: GenerateArgs, invocation: Invocation) -> Result<()> {
    args.name = java_type_name(&args.name);
    model_generate::validate_entity_args(&args)?;
    let current = crate::model_command::Current::load(&invocation)?;
    let entity_label = java_to_label(&args.name);
    let entity_id = EntityId::parse(format!("ent_{entity_label}"))
        .map_err(|error| Failure::Told(format!("could not assign entity identity: {error}")))?;
    let mut fields = args.fields.clone();
    if args.timestamps {
        fields.extend([
            "createdAt:instant@default(now())".to_string(),
            "updatedAt:instant@default(now())@updated".to_string(),
        ]);
    }
    // **Normalized, so both spellings mean one place.** A reader types the
    // package they see in their editor -- `com.example.demo.billing` -- and
    // the model states it relative to the base, so an absolute one has the
    // base stripped rather than appended: appended, the entity lands in
    // `com/example/demo/com/example/demo/billing`.
    let package = args
        .package
        .as_deref()
        .map(|package| normalize_package(&current.model.project.base_package, package))
        .transpose()?;
    let declaration = match args.kind {
        ArtifactKind::Enum => enum_declaration(&args.name, &fields)?,
        ArtifactKind::Record | ArtifactKind::Value | ArtifactKind::Scaffold => entity_declaration(
            &current.model,
            &EntityDeclaration {
                java_name: &args.name,
                scaffold: args.kind == ArtifactKind::Scaffold,
                fields: &fields,
                path: args.path.as_deref(),
                uniques: &args.uniques,
                package: package.as_deref(),
            },
        )?,
        _ => unreachable!("run only accepts entity kinds"),
    };
    if let Some(existing) = current.model.entity(&entity_id) {
        let requested = declaration_entity(&current.model, &declaration, &entity_id)?;
        if !same_entity_contribution(existing, &requested) {
            // **A strict superset is an addition, not a disagreement.** A
            // declarative manifest states the shape it wants and is replayed
            // whenever it changes, so a row that gained a field has to mean
            // "add that field" -- refusing would make `app apply` the one
            // command that cannot converge on the file it exists to read.
            // Typing the same scaffold with one more field means the same
            // thing.
            //
            // Only additions. A field that exists with a *different* shape is
            // an evolution with a policy attached -- a type change, a
            // nullability change, a rename -- and each of those is its own
            // command for a reason.
            let added = requested
                .fields
                .iter()
                .filter(|field| existing.field(&field.id).is_none())
                .map(|field| field.label.clone())
                .collect::<Vec<_>>();
            let unchanged = requested
                .fields
                .iter()
                .filter(|field| existing.field(&field.id).is_some())
                .all(|field| existing.field(&field.id) == Some(field));
            // **A field that left is not an append.** Dropping one component
            // and adding another reads exactly like renaming it, and jails
            // cannot tell the two apart from the shapes alone -- so it says
            // so rather than picking, because the difference is whether the
            // column's rows survive.
            let dropped = existing
                .fields
                .iter()
                .filter(|field| requested.field(&field.id).is_none())
                .map(|field| field.label.clone())
                .collect::<Vec<_>>();
            if !dropped.is_empty() {
                return Err(Failure::Told(format!(
                    "entity `{}` gained {} and lost {}, and jails cannot say which change it is: a rename, a drop and an add, or a type change.\n       fix: `jails entity field rename|type|nullability|drop {}` states which one, and each keeps the rows differently",
                    args.name,
                    quoted_list(&added),
                    quoted_list(&dropped),
                    args.name
                )));
            }
            // **A projection the entity has not got is an addition too**, and
            // so is a constant appended to a closed set. `g record Post` then
            // `g scaffold Post` asks for the repository, service, DTO and HTTP
            // facets over the record that is already there, which is the order
            // a reader types them in; `g enum Status OPEN CLOSED PENDING` over
            // `OPEN CLOSED` asks for the third constant.
            let widened = match unchanged {
                true => facet::widen_enum(&current.source, existing, &requested.enum_constants)?,
                false => None,
            };
            if let Some(next_source) = widened {
                return finish_generation(PreparedMutation {
                    name: args.name.clone(),
                    invocation,
                    current,
                    next_source,
                    evolution: Evolution::none(),
                    authored_migration: None,
                    reader_paths: Vec::new(),
                });
            }
            if unchanged
                && let Some(next_source) =
                    facet::add_facets(&current.source, existing, &requested.facets)?
            {
                finish_generation(PreparedMutation {
                    name: args.name.clone(),
                    invocation: invocation.clone(),
                    current: current.clone(),
                    next_source,
                    evolution: Evolution::none(),
                    authored_migration: None,
                    reader_paths: Vec::new(),
                })?;
                if added.is_empty() {
                    return Ok(());
                }
            } else if added.is_empty() || !unchanged {
                return Err(Failure::Told(format!(
                    "entity `{}` is already declared with a different shape.\n       fix: evolve it with `jails entity field add` or `jails rename resource`",
                    args.name
                )));
            }
            for label in added {
                let Some(spec) = args
                    .fields
                    .iter()
                    .find(|spec| field_label_of(spec) == label)
                    .cloned()
                else {
                    return Err(Failure::Told(format!(
                        "entity `{}` gained field `{label}` from a declaration this command cannot restate.\n       fix: add it with `jails entity field add {} {label}:<type>`",
                        args.name, args.name
                    )));
                };
                crate::model_resource::add_field(
                    crate::model_resource::AddFieldRequest {
                        entity: args.name.clone(),
                        field_spec: spec,
                        default_literal: args.default_literal.clone(),
                        backfill_file: args.backfill_file.clone(),
                        package: None,
                    },
                    invocation.clone(),
                )?;
            }
            return Ok(());
        }
        return finish_generation(PreparedMutation {
            name: args.name,
            invocation,
            next_source: current.source.clone(),
            current,
            evolution: Evolution::none(),
            authored_migration: None,
            reader_paths: Vec::new(),
        });
    }
    // **`--index` at creation is part of the `create table`.** An index is a
    // stable entity child with its own identity, and `entity index add` is
    // still the command that adds one to a table that exists -- but a table
    // and an index asked for in the same breath are one plan and one
    // migration, not a `create table` followed by an `alter` against a table
    // one command old. The columns resolve against the declaration's own
    // linked entity, so a name that is not a component still refuses before
    // anything is written.
    let indexes = std::mem::take(&mut args.indexes);
    let declaration = match indexes.is_empty() {
        true => declaration,
        false => {
            refuse_indexes_without_storage(&current.model)?;
            let entity = declaration_entity(&current.model, &declaration, &entity_id)?;
            let mut members = Vec::new();
            for columns in &indexes {
                let canonical = crate::model_index::canonical_columns(&entity, columns)?;
                members.push(format!("  index [{}]", canonical.join(", ")));
            }
            declare_indexes(&declaration, &members)?
        }
    };
    let next_source = append_declaration(current.source.clone(), &declaration)?;
    finish_generation(PreparedMutation {
        name: args.name,
        invocation,
        current,
        next_source,
        evolution: Evolution::none(),
        authored_migration: None,
        reader_paths: Vec::new(),
    })
}

/// An index needs a schema to live in, and says so before anything is written.
///
/// The same condition `entity index add` refuses on, checked here because
/// this is the command that would otherwise have written the entity first and
/// refused afterwards -- half of what the reader asked for.
fn refuse_indexes_without_storage(model: &jails_model::AppModel) -> Result<()> {
    if model
        .capabilities
        .values()
        .any(|capability| capability.kind == "db")
    {
        return Ok(());
    }
    Err(Failure::Told(
        "an index needs a database to live in.
       fix: run `jails add db` first, or \
         generate without `--index`"
            .to_string(),
    ))
}

/// Put the index members inside the entity block, before its closing brace.
///
/// Text, because the declaration is text at this point and the whole value of
/// rendering it here is that one compile sees the entity and its indexes
/// together.
fn declare_indexes(declaration: &str, members: &[String]) -> Result<String> {
    let closing = declaration.rfind('}').ok_or_else(|| {
        Failure::Told(format!(
            "the rendered declaration has no closing brace: {declaration}\n       fix: report \
             this as a bug in the declaration renderer"
        ))
    })?;
    let mut out = declaration[..closing].trim_end().to_string();
    out.push('\n');
    out.push('\n');
    for member in members {
        out.push_str(member);
        out.push('\n');
    }
    out.push_str(&declaration[closing..]);
    Ok(out)
}

fn declaration_entity(
    model: &jails_model::AppModel,
    declaration: &str,
    entity_id: &EntityId,
) -> Result<jails_model::Entity> {
    let storage = match model.project.dialect.as_str() {
        "postgresql" => "postgres",
        "h2" => "h2",
        "sqlite" => "sqlite",
        _ => "none",
    };
    let source = format!(
        "jdl 1\napp Comparison @id(project_comparison) {{\n  pkg {}\n  java {}\n  platform {}\n  build {}\n  storage {storage}\n}}\n\n{}",
        model.project.base_package,
        model.project.java_release,
        model.project.platform,
        model.project.build,
        declaration
    );
    parse(&source)?
        .entity(entity_id)
        .cloned()
        .ok_or_else(|| Failure::Told(format!("requested entity `{entity_id}` did not link")))
}

fn same_entity_contribution(
    existing: &jails_model::Entity,
    requested: &jails_model::Entity,
) -> bool {
    existing.id == requested.id
        && existing.label == requested.label
        && existing.names == requested.names
        && existing.active == requested.active
        // **A subset, like the fields and indexes below.** Equality would
        // refuse re-declaring an entity that has since gained a facet --
        // "already declared with a different shape" -- even though everything
        // the request asks for is present, and a manifest declaring `User` as
        // a scaffold and again as a seed replays into exactly that. The
        // function's own name is `contribution`: what this request
        // contributes must be there, not everything that is there must have
        // come from this request.
        && requested.facets.is_subset(&existing.facets)
        && existing.enum_constants == requested.enum_constants
        && requested
            .fields
            .iter()
            .all(|field| existing.field(&field.id) == Some(field))
        && requested
            .indexes
            .iter()
            .all(|(id, index)| existing.indexes.get(id) == Some(index))
}

use crate::model_command::parse;

fn append_declaration(source: String, declaration: &str) -> Result<String> {
    jails_model::append_jdl_declaration(&source, declaration).map_err(jdl_edit_failure)
}

/// A stored resource needs exactly one primary key, and it has to be declared.
///
/// **Refused here rather than by the linker.** The projection prerequisite
/// says `projection `repo` on `book` requires a primary key`, which is true
/// and is about a projection the reader never typed; this one is about the
/// command they did.
fn refuse_unstorable_identity(fields: &[ParsedField], java_name: &str) -> Result<()> {
    let keys: Vec<&str> = fields
        .iter()
        .filter(|field| field.primary_key)
        .map(|field| field.java_name.as_str())
        .collect();
    match keys.len() {
        1 => Ok(()),
        0 => Err(Failure::Told(format!(
            "`{java_name}` needs exactly one `@pk` field to be stored, addressed and updated by.\n       fix: mark one component, for example `id:uuid@pk`"
        ))),
        _ => Err(Failure::Told(format!(
            "`{java_name}` declares a composite primary key ({}), and a scaffold addresses one row by one value.\n       fix: keep one `@pk` and make the rest `@unique`, or declare the record and its repository by hand",
            keys.join(", ")
        ))),
    }
}

/// A component whose type is another record has no column to live in.
///
/// Flattening it silently would compile the record, give the DDL no column
/// for it, and have the adapter's insert name one that does not exist. The
/// two things a reader actually wants are both named here: the foreign key
/// column, and the declaration that makes it an invariant.
fn refuse_unstorable_components(
    model: &jails_model::AppModel,
    fields: &[ParsedField],
    java_name: &str,
) -> Result<()> {
    for field in fields {
        let Some(referenced) = model
            .entities
            .values()
            .find(|entity| entity.names.java_type == field.type_name)
        else {
            continue;
        };
        if referenced.facets.contains(&jails_model::Facet::Enum) {
            continue;
        }
        let key = referenced
            .fields
            .iter()
            .find(|candidate| candidate.primary_key)
            .map(|candidate| match &candidate.ty {
                jails_model::TypeRef::Builtin(builtin) => builtin.semantics().token.to_string(),
                jails_model::TypeRef::External(name) => name.clone(),
            })
            .unwrap_or_else(|| "uuid".to_string());
        return Err(Failure::Told(format!(
            "`{}` names the record `{}`, which cannot be persisted as a column of `{java_name}`.\n       fix: hold its key -- `{}:{key}` -- and declare the invariant with `jails g association {}{} {}=id --on {java_name} --yields {}`",
            field.java_name,
            field.type_name,
            field.java_name,
            java_name,
            field.type_name,
            field.label,
            field.type_name,
        )));
    }
    Ok(())
}
