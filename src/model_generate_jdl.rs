//! Lossless JDL edits for the familiar `jails generate` surface.

mod component;
mod edit;
pub(crate) mod facet;
pub(crate) mod index;
mod operation;
mod relation;
mod unit;
pub(crate) use component::{component_kind, component_stem};
use edit::insert_entity_member;
pub(crate) use edit::{
    insert_field, is_v1_source, jdl_edit_failure, remove_capability, remove_dependency,
    remove_entity, remove_operation, remove_setting, remove_unit, rename_entity, set_entity_active,
};
use operation::operation_declaration;

use crate::cli::GenerateArgs;
use crate::generate::ArtifactKind;
use crate::model_generate::{ParsedField, PreparedMutation, finish_generation, parse_field};
use crate::model_resource::java_to_label;
use crate::{Invocation, model_generate};
use jails_model::{EntityId, ModelPatch, OperationId, StableId};
use jails_support::{Failure, Result};
use serde_json::json;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const MODEL_PATH: &str = crate::model_command::JDL_PATH;

/// Every generator kind, and there is deliberately no `_` arm.
///
/// The match became exhaustive when the last four kinds got an answer, and
/// keeping it that way is worth more than the arm it replaces: a kind added
/// without deciding what a canonical project does with it is now a compile
/// error rather than a silent fall-through to the compatibility refusal.
pub(crate) fn run(args: GenerateArgs, invocation: Invocation) -> Result<()> {
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
    let model_path = PathBuf::from(MODEL_PATH);
    let current_source = read_model(&invocation)?;
    let v1 = is_v1_source(&current_source);
    let current_model = parse(&current_source)?;
    // **An event may name no entity**, and then the block it becomes is a
    // top-level declaration rather than an entity member. Everything below
    // that reads the target -- the managed field list, the borrowed `--via`
    // component, where the block is spliced -- has nothing to read, so the
    // empty label is the honest value rather than a lookup that would fail.
    let (entity_label, entity_java_name) = match args.strategy_on.as_deref() {
        None => (String::new(), String::new()),
        Some(on) => {
            let requested_entity = java_to_label(on);
            let entity = current_model
                .entities
                .values()
                .find(|entity| entity.label == requested_entity || entity.names.java_type == on)
                .ok_or_else(|| {
                    Failure::Told(format!(
                        "`{on}` does not name a canonical entity.\n       fix: choose an entity declared in `{MODEL_PATH}`"
                    ))
                })?;
            (entity.label.clone(), entity.names.java_type.clone())
        }
    };
    let standalone = entity_java_name.is_empty();
    if standalone && !v1 {
        return Err(Failure::Told(format!(
            "an event with no entity needs `jdl 1`.\n       fix: upgrade `{MODEL_PATH}` to JDL v1, or pass `--on <Entity>`"
        )));
    }
    // An event's payload can carry a component the row does not: see
    // `event_component_declarations`. Every other operation's field list is a
    // projection of the target, so it goes through the checked resolver.
    let fields = if args.kind == ArtifactKind::Event {
        model_generate::event_component_declarations(&current_model, &entity_label, &args.fields)?
    } else {
        model_generate::operation_field_labels_via(
            &current_model,
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
    let declaration = operation_declaration(&args, &current_model, &entity_label, &fields, v1)?;
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
    if let Some(existing) = current_model.operations.get(&operation_id) {
        let without = remove_operation(&current_source, &args.name, operation_id.as_str())?;
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
                "canonical operation `{}` is already declared with a different shape.\n       fix: remove it explicitly before changing its semantic contract",
                args.name
            )));
        }
        return finish_generation(PreparedMutation {
            name: args.name,
            invocation,
            model_path,
            current_source: current_source.clone(),
            current_model,
            next_source: current_source,
            patch: ModelPatch::Batch(Vec::new()),
            patch_bytes: br#"{"kind":"batch","patches":[]}"#.to_vec(),
            authored_migration: None,
        });
    }
    let next_source = splice(&current_source)?;
    let next_model = parse(&next_source)?;
    let operation = next_model
        .operations
        .get(&operation_id)
        .cloned()
        .ok_or_else(|| Failure::Told(format!("new operation `{operation_id}` did not link")))?;
    let patch_bytes = serde_json::to_vec(&json!({
        "kind": "add-operation",
        "operation": operation,
    }))
    .map_err(|error| Failure::Told(format!("could not encode model patch: {error}")))?;
    finish_generation(PreparedMutation {
        name: args.name,
        invocation,
        model_path,
        current_source,
        current_model,
        next_source,
        patch: ModelPatch::AddOperation(operation),
        patch_bytes,
        authored_migration: None,
    })
}

/// A CLI name, as the Java type it names.
///
/// `jails g enum currency GBP EUR` writes `Currency.java` on the legacy path
/// and every generator that later says `currency:Currency` resolves against
/// it -- which is the whole of
/// `generators_compose_through_user_owned_field_types`. The canonical model
/// requires a real Java type name and refused the lower-camel spelling
/// outright, so the same command produced a project on one engine and a
/// diagnostic on the other.
///
/// Capitalising here rather than loosening the model: `java_name` is a
/// projection the model is right to hold to, and this is the CLI sugar
/// resolving what the reader typed, which is where the legacy path does it
/// too.
pub(crate) fn java_type_name(name: &str) -> String {
    let mut characters = name.chars();
    match characters.next() {
        Some(first) => first.to_uppercase().collect::<String>() + characters.as_str(),
        None => String::new(),
    }
}

fn run_entity(mut args: GenerateArgs, invocation: Invocation) -> Result<()> {
    args.name = java_type_name(&args.name);
    model_generate::validate_entity_args(&args)?;
    let model_path = PathBuf::from(MODEL_PATH);
    let current_source = read_model(&invocation)?;
    let v1 = is_v1_source(&current_source);
    let current_model = parse(&current_source)?;
    let entity_label = java_to_label(&args.name);
    let entity_id = EntityId::parse(format!("ent_{entity_label}"))
        .map_err(|error| Failure::Told(format!("could not assign entity identity: {error}")))?;
    let mut fields = args.fields.clone();
    if args.timestamps {
        fields.extend(if v1 {
            [
                "createdAt:instant@default(now())".to_string(),
                "updatedAt:instant@default(now())@updated".to_string(),
            ]
        } else {
            [
                "createdAt:instant".to_string(),
                "updatedAt:instant".to_string(),
            ]
        });
    }
    let declaration = match args.kind {
        ArtifactKind::Enum => enum_declaration(&args.name, &entity_label, &fields, v1)?,
        ArtifactKind::Record | ArtifactKind::Value | ArtifactKind::Scaffold => {
            entity_declaration_at(
                &current_model,
                &EntityDeclaration {
                    java_name: &args.name,
                    entity_label: &entity_label,
                    scaffold: args.kind == ArtifactKind::Scaffold,
                    fields: &fields,
                    v1,
                    path: args.path.as_deref(),
                    uniques: &args.uniques,
                },
            )?
        }
        _ => unreachable!("run only accepts entity kinds"),
    };
    if let Some(existing) = current_model.entity(&entity_id) {
        let requested = declaration_entity(&current_model, &declaration, &entity_id, v1)?;
        if !same_entity_contribution(existing, &requested) {
            // **A strict superset is an addition, not a disagreement.** A
            // declarative manifest states the shape it wants and is replayed
            // whenever it changes, so a row that gained a field has to mean
            // "add that field" -- refusing made `app apply` the one command
            // that could not converge on the file it exists to read. Typing
            // the same scaffold with one more field means the same thing.
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
                    "canonical entity `{}` gained {} and lost {}, and jails cannot say which change it is: a rename, a drop and an add, or a type change.\n       fix: `jails resource field rename|type|nullability|drop {}` states which one, and each keeps the rows differently",
                    args.name,
                    quoted_list(&added),
                    quoted_list(&dropped),
                    args.name
                )));
            }
            if added.is_empty() || !unchanged {
                return Err(Failure::Told(format!(
                    "canonical entity `{}` is already declared with a different shape.\n       fix: evolve it with `jails g field`, `jails resource field`, or `jails rename resource`",
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
                        "canonical entity `{}` gained field `{label}` from a declaration this command cannot restate.\n       fix: add it with `jails resource field add {} {label}:<type>`",
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
            model_path,
            current_source: current_source.clone(),
            current_model,
            next_source: current_source,
            patch: ModelPatch::Batch(Vec::new()),
            patch_bytes: br#"{"kind":"batch","patches":[]}"#.to_vec(),
            authored_migration: None,
        });
    }
    let next_source = append_declaration(current_source.clone(), &declaration)?;
    let next_model = parse(&next_source)?;
    let entity = next_model
        .entity(&entity_id)
        .cloned()
        .ok_or_else(|| Failure::Told(format!("new entity `{entity_id}` did not link")))?;
    // **The entity's projections ride with it.** `AddEntity` carries
    // `entity.facets`, which records *that* an entity is served over HTTP and
    // not *where*; the arguments live on the `Projection`. Sending the entity
    // alone made the patch a lossy description of the source that produced it,
    // so the first compile disagreed with every later `sync`.
    let projections: Vec<_> = next_model
        .projections
        .values()
        .filter(|projection| projection.entity == entity_id)
        .cloned()
        .collect();
    let patch_bytes = serde_json::to_vec(&json!({
        "kind": "add-entity",
        "entity": entity,
        "projections": projections,
    }))
    .map_err(|error| Failure::Told(format!("could not encode model patch: {error}")))?;
    let patch = if projections.is_empty() {
        ModelPatch::AddEntity(entity)
    } else {
        ModelPatch::Batch(
            std::iter::once(ModelPatch::AddEntity(entity))
                .chain(projections.into_iter().map(ModelPatch::AddProjection))
                .collect(),
        )
    };
    let indexes = std::mem::take(&mut args.indexes);
    let name = args.name.clone();
    finish_generation(PreparedMutation {
        name: args.name,
        invocation: invocation.clone(),
        model_path,
        current_source,
        current_model,
        next_source,
        patch,
        patch_bytes,
        authored_migration: None,
    })?;
    // **`--index` is a second patch, not a flag on the first.** An index is a
    // stable entity child with its own identity and its own forward
    // migration, which is `resource index add`'s whole contract -- so the
    // frontend that owns it is the one that applies it, rather than the entity
    // renderer growing a copy. Refusing instead was what stopped a proof
    // application's `g scaffold --index "user_id, time_stamp desc"` from
    // reaching the canonical path at all.
    //
    // After the entity, necessarily: the columns are resolved against model
    // field identity, and the fields do not exist until the patch above lands.
    for columns in indexes {
        crate::model_index::add(name.clone(), columns, None, invocation.clone())?;
    }
    Ok(())
}

fn declaration_entity(
    model: &jails_model::AppModel,
    declaration: &str,
    entity_id: &EntityId,
    v1: bool,
) -> Result<jails_model::Entity> {
    let storage = match model.project.dialect.as_str() {
        "postgresql" => "postgres",
        "h2" => "h2",
        "sqlite" => "sqlite",
        _ => "none",
    };
    let source = if v1 {
        format!(
            "jdl 1\napp Comparison @id(project_comparison) {{\n  pkg {}\n  java {}\n  platform {}\n  build {}\n  storage {storage}\n}}\n\n{}",
            model.project.base_package,
            model.project.java_release,
            model.project.platform,
            model.project.build,
            declaration
        )
    } else {
        format!(
            "application Comparison @id(project_comparison)\npackage {}\njava {}\ndialect {}\n\n{}",
            model.project.base_package,
            model.project.java_release,
            model.project.dialect,
            declaration
        )
    };
    parse(&source)?
        .entity(entity_id)
        .cloned()
        .ok_or_else(|| Failure::Told(format!("requested entity `{entity_id}` did not link")))
}

fn quoted_list(labels: &[String]) -> String {
    labels
        .iter()
        .map(|label| format!("`{label}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The model label a `name:type` field spec declares.
///
/// The same fold the parser applies: `userId` and `user_id` are one field, so
/// matching a requested label against a typed spec has to agree with it.
fn field_label_of(spec: &str) -> String {
    let name = spec.split(':').next().unwrap_or_default();
    let mut label = String::new();
    for (index, character) in name.chars().enumerate() {
        if character.is_ascii_uppercase() {
            if index > 0 {
                label.push('_');
            }
            label.push(character.to_ascii_lowercase());
        } else if character == '-' {
            label.push('_');
        } else {
            label.push(character);
        }
    }
    label
}

fn same_entity_contribution(
    existing: &jails_model::Entity,
    requested: &jails_model::Entity,
) -> bool {
    existing.id == requested.id
        && existing.label == requested.label
        && existing.names == requested.names
        && existing.active == requested.active
        // **A subset, like the fields and indexes below.** This asked for
        // equality, so re-declaring an entity that had since gained a facet
        // refused -- "already declared with a different shape" -- even though
        // everything the request asks for is present. The minicom manifest
        // declares `User` as a scaffold and again as a seed, so replaying it
        // a second time hit exactly that, and the function's own name is
        // `contribution`: what this request contributes must be there, not
        // everything that is there must have come from this request.
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

pub(crate) fn read_model(invocation: &Invocation) -> Result<String> {
    crate::model_command::read_source_at(&invocation.root()?, Path::new(MODEL_PATH))
}

pub(crate) fn parse(source: &str) -> Result<jails_model::AppModel> {
    jails_model::parse_jdl(source)
        .map_err(|diagnostics| Failure::Told(diagnostics.to_string().trim_end().to_string()))
}

fn append_declaration(mut source: String, declaration: &str) -> Result<String> {
    if is_v1_source(&source) {
        return jails_model::append_jdl_declaration(&source, declaration).map_err(jdl_edit_failure);
    }
    if !source.ends_with('\n') {
        source.push('\n');
    }
    source.push('\n');
    source.push_str(declaration);
    Ok(source)
}

/// The lowerCamel member name a relation is declared under.
///
/// The single owner of the `item_owner` -> `itemOwner` direction, so
/// `g association` and `destroy association` cannot disagree about which
/// member they are naming -- which they did, and the destroy half reported the
/// declaration as missing from the entity it was sitting in.
pub(crate) fn relation_member_name(label: &str) -> String {
    let mut out = String::with_capacity(label.len());
    let mut capitalise = false;
    for character in label.chars() {
        if character == '_' {
            capitalise = true;
            continue;
        }
        if capitalise {
            out.extend(character.to_uppercase());
            capitalise = false;
        } else {
            out.push(character);
        }
    }
    out
}

/// The same declaration, with the collection route the reader pinned.
///
/// **`--path` is a projection argument, not a new mechanism.** `use
/// scaffold(path: "/admin_api/operators")` is already in the v1 grammar and
/// `emit_resource_http::resource_path` already prefers it over the table name;
/// what refused was the frontend, so the flag reached a project that could
/// represent it and was told the profile could not. The pre-v1 draft has no
/// projection arguments, so pinning there is refused rather than silently
/// dropped -- a route the reader asked for and did not get is the failure this
/// closes, and writing it into a dialect on the deletion list would only move
/// it.
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
/// The engine flattened it silently -- the record compiled, the DDL had no
/// column for it, and the adapter's insert named one that did not exist. The
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

/// One entity declaration's inputs, together because they are decided
/// together: what to call it, what it holds, and which dialect it is written
/// in.
pub(crate) struct EntityDeclaration<'a> {
    pub(crate) java_name: &'a str,
    pub(crate) entity_label: &'a str,
    pub(crate) scaffold: bool,
    pub(crate) fields: &'a [String],
    pub(crate) v1: bool,
    pub(crate) path: Option<&'a str>,
    pub(crate) uniques: &'a [String],
}

pub(crate) fn entity_declaration_at(
    model: &jails_model::AppModel,
    declaration: &EntityDeclaration<'_>,
) -> Result<String> {
    let EntityDeclaration {
        java_name,
        entity_label,
        scaffold,
        fields,
        v1,
        path,
        uniques,
    } = *declaration;
    let mut labels = BTreeSet::new();
    let mut parsed = Vec::new();
    for token in fields {
        let field = parse_field(token)?;
        // **One column, named twice.** `id` and `Id` converge on the same
        // Java component and the same SQL column, so this is not two fields
        // colliding -- it is one field spelled two ways, and the refusal says
        // so in both projections rather than echoing whichever spelling came
        // second.
        if !labels.insert(field.label.clone()) {
            return Err(Failure::Told(format!(
                "`{}` is declared twice: `{}` and the column `{}` are one field, whatever the spelling.\n       fix: keep one declaration",
                field.java_name,
                jails_model::lower_camel_case(&field.label),
                field
                    .mapped_column
                    .clone()
                    .unwrap_or_else(|| field.label.clone()),
            )));
        }
        parsed.push(field);
    }
    // **A component called `version` is the row version.** The engine this
    // replaces inferred it, every `--if-match` transition depends on it, and
    // an entity that declared `version:long` without the marker got a plain
    // column: the transition then refused with "entity `note` has 0 version
    // fields" about a field the reader had just declared and named. Inferred
    // in the frontend and written into the model as `@version`, so the
    // convention is visible in `.jails/model.jdl` rather than hidden in the
    // compiler -- and an entity that means something else by the word says so
    // by editing the declaration.
    //
    // v1 only, because `@version` is a v1 marker; the draft dialect cannot
    // express it and inferring one it cannot write would be a lie.
    if v1 {
        for field in &mut parsed {
            if field.label == "version" && matches!(field.type_name.as_str(), "long" | "int") {
                field.version = true;
            }
        }
    }
    if scaffold {
        // **`scaffold` is four Spring facets, so it needs Spring.** The
        // linker reaches the same conclusion one projection at a time --
        // `projection `dto` on `note` requires platform spring` -- which
        // names symbols the reader never typed and says nothing about which
        // of the two ways out they want.
        if model.project.platform != "spring" {
            return Err(Failure::Told(format!(
                "`scaffold` is a Spring Boot capability -- a DTO, a controller and a service are Spring types -- and this project declares `platform {}`.\n       fix: `jails g record {java_name}` for the record and its repository, or declare Spring in `{MODEL_PATH}`",
                model.project.platform
            )));
        }
        refuse_unstorable_identity(&parsed, java_name)?;
        refuse_unstorable_components(model, &parsed, java_name)?;
    }
    let mut output = format!("entity {java_name} @id(ent_{entity_label}) {{\n");
    if scaffold {
        if v1 {
            match path {
                Some(path) => output.push_str(&format!(
                    "  use scaffold(path: {})\n\n",
                    serde_json::to_string(path).expect("a route path encodes as a JSON string")
                )),
                None => output.push_str("  use scaffold\n\n"),
            }
        } else {
            if path.is_some() {
                return Err(Failure::Told(
                    "pinning a resource route needs a `jdl 1` model.\n       fix: run `jails model upgrade` and repeat the command"
                        .to_string(),
                ));
            }
            output = output.replacen(" {", " @scaffold {", 1);
        }
    }
    for field in &parsed {
        let line = if v1 {
            render_v1_field_line(entity_label, field)
        } else {
            render_field_line(entity_label, field)?
        };
        output.push_str(&line);
        output.push('\n');
    }
    // **A composite unique is a constraint on the table, not a marker on one
    // component**, so it is its own member. PostgreSQL requires the columns a
    // foreign key names to carry a unique constraint of their own, which is
    // why a tenant-scoped reference needs `(workspaceId, id)` stated even
    // where `id` alone is already the key.
    for columns in uniques {
        let components = columns
            .split(',')
            .map(str::trim)
            .filter(|component| !component.is_empty())
            .map(|component| labels.get(&java_to_label(component)).cloned().ok_or_else(|| {
                Failure::Told(format!(
                    "`{component}` is not a component of `{java_name}`.\n       fix: name components this entity declares"
                ))
            }))
            .collect::<Result<Vec<_>>>()?;
        if components.is_empty() {
            return Err(Failure::Told(
                "a composite unique key needs at least one component.\n       fix: give `--unique` a comma-separated component list"
                    .to_string(),
            ));
        }
        if !v1 {
            return Err(Failure::Told(
                "a composite unique key needs a `jdl 1` model.\n       fix: run `jails model upgrade` and repeat the command"
                    .to_string(),
            ));
        }
        output.push_str(&format!("  unique [{}]\n", components.join(", ")));
    }
    output.push_str("}\n");
    Ok(output)
}

pub(crate) fn enum_declaration(
    java_name: &str,
    label: &str,
    values: &[String],
    v1: bool,
) -> Result<String> {
    let values = values
        .iter()
        .map(|value| {
            jails_protocol::declaration::ConstantSpec::parse(value)
                .map(|constant| constant.canonical())
        })
        .collect::<Result<Vec<_>>>()?;
    let mut output = format!("enum {java_name} @id(ent_{label}) {{\n");
    for value in values {
        output.push_str("  ");
        if v1 {
            if let Some((constant, wire)) = value.split_once('=') {
                output.push_str(constant);
                output.push_str(" = ");
                output.push_str(&serde_json::to_string(wire).map_err(|error| {
                    Failure::Told(format!("could not quote enum wire value: {error}"))
                })?);
            } else {
                output.push_str(&value);
            }
        } else {
            output.push_str(&value);
        }
        output.push('\n');
    }
    output.push_str("}\n");
    Ok(output)
}

pub(crate) fn render_field_line(entity_label: &str, field: &ParsedField) -> Result<String> {
    field.require_v1_for_rich_semantics()?;
    let suffix = if !field.required {
        "?"
    } else if field.non_blank {
        "!"
    } else {
        ""
    };
    let range = if field.min_length.is_some() || field.max_length.is_some() {
        format!(
            "({}..{})",
            field
                .min_length
                .map(|value| value.to_string())
                .unwrap_or_default(),
            field
                .max_length
                .map(|value| value.to_string())
                .unwrap_or_default(),
        )
    } else {
        String::new()
    };
    let mut output = format!(
        "  {}: {}{}{} @id(fld_{}_{})",
        field.java_name, field.type_name, suffix, range, entity_label, field.label
    );
    if field.primary_key {
        output.push_str(" @pk");
    }
    if field.unique {
        output.push_str(" @unique");
    }
    if field.indexed {
        output.push_str(" @index");
    }
    if let Some(column) = &field.mapped_column {
        output.push_str(&format!(" @column({column})"));
    }
    Ok(output)
}

pub(crate) fn render_v1_field_line(entity_label: &str, field: &ParsedField) -> String {
    let optional = if field.required { "" } else { "?" };
    let mut output = format!(
        "  {}: {}{} @id(fld_{}_{})",
        field.java_name, field.type_name, optional, entity_label, field.label
    );
    if let Some(default) = &field.default {
        output.push_str(&format!(" @default({default})"));
    }
    if field.primary_key {
        output.push_str(" @pk");
    }
    if field.version {
        output.push_str(" @version");
    }
    if field.min_length.is_some() || field.max_length.is_some() {
        output.push_str(&format!(
            " @length({}..{})",
            field
                .min_length
                .map(|value| value.to_string())
                .unwrap_or_default(),
            field
                .max_length
                .map(|value| value.to_string())
                .unwrap_or_default(),
        ));
    }
    if field.nonnegative {
        output.push_str(" @nonnegative");
    }
    if field.non_blank {
        output.push_str(" @notBlank");
    }
    if field.positive {
        output.push_str(" @positive");
    }
    if field.indexed {
        output.push_str(" @index");
    }
    if field.scoped {
        output.push_str(" @scope");
    }
    if field.unique {
        output.push_str(" @unique");
    }
    if field.updated {
        output.push_str(" @updated");
    }
    if let Some(column) = &field.mapped_column {
        let column = serde_json::to_string(column).expect("string serialization cannot fail");
        output.push_str(&format!(" @map({column})"));
    }
    output
}
