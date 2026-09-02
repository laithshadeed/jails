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
pub(crate) use render::{EntityDeclaration, normalize_package};
use render::{entity_declaration_at, enum_declaration, field_label_of, quoted_list};
pub(crate) use render::{java_type_name, relation_member_name, render_v1_field_line};

use crate::ArtifactKind;
use crate::cli::GenerateArgs;
use crate::model_generate::{ParsedField, PreparedMutation, finish_generation, parse_field};
use crate::model_resource::java_to_label;
use crate::{Invocation, model_generate};
use jails_model::{EntityId, ModelPatch, OperationId};
use jails_support::{Failure, Result};
use serde_json::json;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const MODEL_PATH: &str = crate::model_command::JDL_PATH;

/// Every generator kind, and there is deliberately no `_` arm: a kind added
/// without deciding what a canonical project does with it is a compile error
/// rather than a silent fall-through.
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
    let declaration = operation_declaration(&args, &current_model, &entity_label, &fields)?;
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
        let without = remove_operation(&current_source, &args.name)?;
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

fn run_entity(mut args: GenerateArgs, invocation: Invocation) -> Result<()> {
    args.name = java_type_name(&args.name);
    model_generate::validate_entity_args(&args)?;
    let model_path = PathBuf::from(MODEL_PATH);
    let current_source = read_model(&invocation)?;
    let current_model = parse(&current_source)?;
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
        .map(|package| normalize_package(&current_model.project.base_package, package))
        .transpose()?;
    let declaration = match args.kind {
        ArtifactKind::Enum => enum_declaration(&args.name, &entity_label, &fields)?,
        ArtifactKind::Record | ArtifactKind::Value | ArtifactKind::Scaffold => {
            entity_declaration_at(
                &current_model,
                &EntityDeclaration {
                    java_name: &args.name,
                    entity_label: &entity_label,
                    scaffold: args.kind == ArtifactKind::Scaffold,
                    fields: &fields,
                    path: args.path.as_deref(),
                    uniques: &args.uniques,
                    package: package.as_deref(),
                },
            )?
        }
        _ => unreachable!("run only accepts entity kinds"),
    };
    if let Some(existing) = current_model.entity(&entity_id) {
        let requested = declaration_entity(&current_model, &declaration, &entity_id)?;
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
                    "canonical entity `{}` gained {} and lost {}, and jails cannot say which change it is: a rename, a drop and an add, or a type change.\n       fix: `jails resource field rename|type|nullability|drop {}` states which one, and each keeps the rows differently",
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
                true => facet::widen_enum(&current_source, existing, &requested.enum_constants)?,
                false => None,
            };
            if let Some((next_source, patches)) = widened {
                let patch = ModelPatch::Batch(patches);
                let patch_bytes = serde_json::to_vec(&serde_json::json!({
                    "kind": "widen-enum",
                    "entity": entity_id,
                }))
                .map_err(|error| Failure::Told(format!("could not encode model patch: {error}")))?;
                return finish_generation(PreparedMutation {
                    name: args.name.clone(),
                    invocation,
                    model_path,
                    current_source: current_source.clone(),
                    current_model,
                    next_source,
                    patch,
                    patch_bytes,
                    authored_migration: None,
                });
            }
            if unchanged
                && let Some((next_source, patches)) =
                    facet::add_facets(&current_source, existing, &requested.facets)?
            {
                let patch = ModelPatch::Batch(patches);
                let patch_bytes = serde_json::to_vec(&serde_json::json!({
                    "kind": "add-facets",
                    "entity": entity_id,
                }))
                .map_err(|error| Failure::Told(format!("could not encode model patch: {error}")))?;
                finish_generation(PreparedMutation {
                    name: args.name.clone(),
                    invocation: invocation.clone(),
                    model_path: model_path.clone(),
                    current_source: current_source.clone(),
                    current_model: current_model.clone(),
                    next_source,
                    patch,
                    patch_bytes,
                    authored_migration: None,
                })?;
                if added.is_empty() {
                    return Ok(());
                }
            } else if added.is_empty() || !unchanged {
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
    // alone would make the patch a lossy description of the source that
    // produced it, so the first compile would disagree with every later
    // `sync`.
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
    // renderer growing a copy.
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

pub(crate) fn read_model(invocation: &Invocation) -> Result<String> {
    crate::model_command::read_source_at(&invocation.root()?, Path::new(MODEL_PATH))
}

pub(crate) fn parse(source: &str) -> Result<jails_model::AppModel> {
    jails_model::parse_jdl(source)
        .map_err(|diagnostics| Failure::Told(diagnostics.to_string().trim_end().to_string()))
}

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
