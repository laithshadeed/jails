//! Executable JDBC adapters for canonical operations.

mod command;
pub(crate) mod outbox;
pub(crate) mod proof;
mod query;
mod transition;

use crate::CompileError;
use crate::emit_java::{JAVA_ROOT, entity, render};
use jails_contracts::{FileKind, FileMode, ProjectPath, Provenance, RenderedFile, RenderedTree};
use jails_model::{
    AppModel, BuiltinType, Entity, Facet, Field, Operation, OperationKind, Package, StableId,
    TypeRef, Value,
};
use std::collections::BTreeSet;

pub(crate) fn lower_and_emit(
    model: &AppModel,
    output: &mut RenderedTree,
) -> Result<(), CompileError> {
    let Some(capability) = model
        .capabilities
        .values()
        .find(|capability| capability.kind == "db")
    else {
        return Ok(());
    };
    for operation in model.operations.values() {
        let lowered = match &operation.kind {
            OperationKind::Command(spec) => {
                let target = stored_entity(model, operation, &spec.on, "command")?;
                if let Some(inputs) =
                    proof::input_fields(model, target, &spec.fields, &spec.semantics.parameters)
                    && let Some(written) = proof::write(
                        model,
                        capability.id.as_str(),
                        operation,
                        target,
                        proof::WriteShape {
                            expected: None,
                            optional: !spec.semantics.resolutions.is_empty(),
                            // **A resolved key needs its parent row stored.**
                            // The insert selects the foreign key out of the
                            // parent, so a proof that stores none proves the
                            // empty case by accident -- and the lookup
                            // parameter has to carry the value that row was
                            // written with, which is what a mapping says.
                            joins: &resolution_joins(spec),
                            port_suffix: "Command",
                            port_package: jails_model::Package::ApplicationCommands,
                            keyed: None,
                            inputs: &inputs,
                        },
                    )?
                {
                    output
                        .insert(written.0, written.1)
                        .map_err(CompileError::new)?;
                }
                command::lower(model, capability.id.as_str(), operation, spec)?
            }
            OperationKind::Query(spec) => {
                let target = stored_entity(model, operation, &spec.on, "query")?;
                let filters = query_filters(model, operation, target, spec)?;
                let ordering = ordering(operation, target, spec)?;
                if let Some(proof) = proof::query(
                    model,
                    capability.id.as_str(),
                    operation,
                    target,
                    &filters,
                    &spec.semantics.joins,
                )? {
                    output.insert(proof.0, proof.1).map_err(CompileError::new)?;
                }
                query::lower(
                    model,
                    capability.id.as_str(),
                    operation,
                    target,
                    query::Shape {
                        filters: &filters,
                        ordering: &ordering,
                        joins: &spec.semantics.joins,
                        limit: spec.semantics.limit.unwrap_or(query::DEFAULT_LIMIT),
                    },
                )?
            }
            OperationKind::Transition(spec) => {
                let target = stored_entity(model, operation, &spec.on, "transition")?;
                // Minus the row selector, exactly as the port's `Input` is:
                // `execute` takes the key beside the record, so a proof that
                // passed it inside as well would not compile.
                let key = crate::emit_java::transition_key(target, spec)?;
                // And minus the version, for the same reason: it travels as
                // `If-Match`, so `execute` takes it beside the record too.
                let expected = crate::emit_java::precondition(target, spec);
                let expected = expected.as_ref();
                let carried = spec
                    .fields
                    .iter()
                    .filter(|field| *field != &key.id)
                    .filter(|field| expected.is_none_or(|version| *field != &version.field.id))
                    .cloned()
                    .collect::<Vec<_>>();
                if let Some(inputs) = proof::input_fields(model, target, &carried, &[])
                    && let Some(written) = proof::write(
                        model,
                        capability.id.as_str(),
                        operation,
                        target,
                        proof::WriteShape {
                            port_suffix: "Transition",
                            port_package: jails_model::Package::ApplicationTransitions,
                            keyed: Some(key),
                            inputs: &inputs,
                            expected,
                            optional: false,
                            joins: &[],
                        },
                    )?
                {
                    output
                        .insert(written.0, written.1)
                        .map_err(CompileError::new)?;
                }
                transition::lower(model, capability.id.as_str(), operation, spec)?
            }
            OperationKind::Event(_) => continue,
        };
        output
            .insert(lowered.0, lowered.1)
            .map_err(CompileError::new)?;
    }
    Ok(())
}

/// One filter of a query, resolved to the column it reads and the table that
/// column is in.
///
/// **The flat `Query.filters` cannot express a joined filter**, which is the
/// same shape [`jails_model::Transition`]'s own documentation records for
/// `sets`: a compatibility projection that emitters read in preference to the
/// linked semantics beside it. A `--via` query puts `user.email` in
/// `semantics.parameters` and the target's field list cannot hold it, so
/// reading the flat list drops the filter without a word -- an endpoint that
/// answers, and answers over every row.
pub(super) struct QueryFilter<'a> {
    /// The entity whose table the column is on -- the query's own target, or
    /// one it joins. Carried rather than re-derived because the answer is
    /// already made here and asking again is how the joined case gets lost.
    pub owner: &'a Entity,
    /// The field the column belongs to.
    pub field: &'a Field,
    /// The `Input` record component this filter binds from.
    pub member: String,
    /// The bind parameter and predicate name. Unique across joined entities,
    /// where two tables can each have a `name`.
    pub label: String,
    /// The join alias qualifying this column, or `None` when the column is
    /// the target's own. `None` renders unqualified unless the query joins,
    /// so a query without one keeps the SQL it always had.
    pub alias: Option<String>,
    /// Whether the filter is always applied.
    pub required: bool,
}

/// Resolve a query's filters against the target and every joined entity.
///
/// The linked parameters when there are any -- they are the only shape that
/// can name a column of a joined table -- and the flat field list otherwise,
/// which is what `.jails/model.toml` and an unjoined query still produce.
/// Each `resolve` declaration as the parent row a proof has to store first.
fn resolution_joins(command: &jails_model::Command) -> Vec<jails_model::Join> {
    command
        .semantics
        .resolutions
        .iter()
        .map(|resolution| jails_model::Join {
            entity: resolution.remote_entity.clone(),
            alias: resolution.remote_entity.as_str().to_string(),
            mappings: vec![jails_model::FieldMapping {
                local: resolution.target.clone(),
                remote: resolution.remote_value.clone(),
            }],
        })
        .collect()
}

/// One query, and the columns its `where` clause names.
pub(crate) struct FilteredQuery<'a> {
    pub operation: &'a Operation,
    /// Each filtered column, with the entity whose table it is on.
    pub columns: Vec<(&'a Entity, &'a Field)>,
}

/// Every query in the model, with the columns it is filtered by.
///
/// **Read through [`query_filters`], which is the same resolution the JDBC
/// `where` clause is built from.** A second walk of `Query.filters` would be
/// a second answer, and it would be wrong in exactly the case that resolution
/// exists for: a `--via` query's joined column is in `semantics.parameters`
/// and not in the flat list at all.
///
/// A query that does not resolve is skipped rather than reported. Anything
/// that would fail here has already refused the whole compile in
/// [`lower_and_emit`]; a diagnostic is not the place to say it a second time,
/// in worse words.
pub(crate) fn filtered_queries(model: &AppModel) -> Vec<FilteredQuery<'_>> {
    model
        .operations
        .values()
        .filter_map(|operation| {
            let OperationKind::Query(spec) = &operation.kind else {
                return None;
            };
            let target = entity(model, &spec.on).ok()?;
            let filters = query_filters(model, operation, target, spec).ok()?;
            Some(FilteredQuery {
                operation,
                columns: filters
                    .into_iter()
                    .map(|filter| (filter.owner, filter.field))
                    .collect(),
            })
        })
        .collect()
}

fn query_filters<'a>(
    model: &'a AppModel,
    operation: &Operation,
    target: &'a Entity,
    spec: &'a jails_model::Query,
) -> Result<Vec<QueryFilter<'a>>, CompileError> {
    if spec.semantics.parameters.is_empty() {
        return Ok(resolve_fields(operation, target, &spec.filters, "filters")?
            .into_iter()
            .map(|field| QueryFilter {
                owner: target,
                field,
                member: field.names.java_member.clone(),
                label: field.label.clone(),
                alias: None,
                required: field.required,
            })
            .collect());
    }
    spec.semantics
        .parameters
        .iter()
        .map(|parameter| {
            let jails_model::ParameterSource::Field(visible) = &parameter.source else {
                return Err(CompileError::new(format!(
                    "canonical query `{}` declares parameter `{}` with no source column\n       fix: filter on a declared field of the query's entity or one it joins",
                    operation.label, parameter.name
                )));
            };
            let owner = model.entities.get(&visible.entity).ok_or_else(|| {
                CompileError::new(format!(
                    "linked query `{}` references missing entity `{}`",
                    operation.label, visible.entity
                ))
            })?;
            let field = owner.field(&visible.field).ok_or_else(|| {
                CompileError::new(format!(
                    "linked query `{}` references missing filter field `{}`",
                    operation.label, visible.field
                ))
            })?;
            // The target's own columns keep the table as qualifier; a joined
            // entity's take the alias the model states, quoted because a
            // natural alias is often a reserved word -- `join User as user`
            // renders `user`, and unquoted that is PostgreSQL's own function.
            let alias = if owner.id == target.id {
                None
            } else {
                let join = spec
                    .semantics
                    .joins
                    .iter()
                    .find(|join| join.entity == owner.id)
                    .ok_or_else(|| {
                        CompileError::new(format!(
                            "canonical query `{}` filters on `{}`, which is not the query's entity and is not joined\n       fix: add a `join {} as <alias> on <local> -> <alias>.<remote>` to the query",
                            operation.label, parameter.name, owner.names.java_type
                        ))
                    })?;
                Some(format!("\"{}\"", join.alias))
            };
            Ok(QueryFilter {
                owner,
                field,
                member: crate::emit_java::parameter_member(parameter),
                // Two joined tables can both carry `name`, so the bind label
                // is the parameter's rather than the column's.
                label: parameter.name.clone(),
                alias,
                required: parameter.required && !parameter.optional_filter,
            })
        })
        .collect()
}

fn stored_entity<'a>(
    model: &'a AppModel,
    operation: &Operation,
    target: &jails_model::EntityId,
    kind: &str,
) -> Result<&'a Entity, CompileError> {
    let target = entity(model, target)?;
    if !target.active || !target.facets.contains(&Facet::Repository) {
        return Err(CompileError::new(format!(
            "canonical {kind} `{}` targets an entity without active storage\n       fix: target an active scaffold or remove the `db` capability",
            operation.label
        )));
    }
    Ok(target)
}

/// The `publishEvent` calls for one operation's `emit` list.
///
/// Shared because commands and transitions publish identically and the rule
/// -- every emitted event, an event of another entity refuses -- must not get
/// two implementations that can disagree. `emit` is repeatable (JDL v1 §12.2
/// and §12.4), so every declared event is published, not only the first.
///
/// The arguments are read off `result`, the row the statement returned, so an
/// event payload always reports what the database actually stored rather than
/// what the caller asked for.
pub(super) fn publications(
    model: &AppModel,
    operation: &Operation,
    target: &Entity,
    emits: &[jails_model::OperationId],
    imports: &mut BTreeSet<String>,
) -> Result<Vec<String>, CompileError> {
    let staged = outbox::delivery(operation) == jails_model::Delivery::Outbox;
    if staged {
        // Checked before a byte is rendered: exactly one event, staged by a
        // minted `id`. The refusals name the declaration to change.
        outbox::relayed(model, operation)?;
        imports.insert(format!(
            "{}.Jdbc{}Outbox",
            model.project.package_for(Package::Jobs),
            operation.names.java_type
        ));
        imports.insert("org.springframework.transaction.annotation.Transactional".to_string());
    }
    let mut publications = Vec::new();
    for event_id in emits {
        let yielded = model.operations.get(event_id).ok_or_else(|| {
            CompileError::new(format!(
                "linked operation `{}` references missing event `{event_id}`",
                operation.label
            ))
        })?;
        let OperationKind::Event(event) = &yielded.kind else {
            return Err(CompileError::new(format!(
                "linked operation `{}` emits non-event operation `{}`",
                operation.label, yielded.label
            )));
        };
        if event.on.as_ref().is_some_and(|entity| entity != &target.id) {
            return Err(CompileError::new(format!(
                "canonical operation `{}` emits event `{}` from another entity\n       fix: emit an event projected from `{}`",
                operation.label, yielded.label, target.label
            )));
        }
        let event_type = crate::emit_java::with_suffix(&yielded.names.java_type, "Event");
        imports.insert(format!(
            "{}.{event_type}",
            model.project.package_for(Package::DomainEvents)
        ));
        if !staged {
            imports.insert("org.springframework.context.ApplicationEventPublisher".to_string());
        }
        // **Read off `semantics.parameters`, not `fields`.** The flat list can
        // only name fields of the target entity; the linked parameters can
        // also carry a `Typed` component -- an event's own identity, a
        // timestamp -- and an emitter reading the flat form renders an empty
        // payload for an event declared with one. The linker folds `fields`
        // into the parameters, so this is the whole payload either way.
        let arguments = event
            .semantics
            .parameters
            .iter()
            .map(|parameter| match &parameter.source {
                jails_model::ParameterSource::Field(visible) => target
                    .field(&visible.field)
                    .map(|field| format!("result.{}()", field.names.java_member))
                    .ok_or_else(|| {
                        CompileError::new(format!(
                            "linked event `{}` references missing field `{}`",
                            yielded.label, visible.field
                        ))
                    }),
                // A component the target row does not carry needs a value
                // from somewhere, and a *direct* publication has none: the
                // command's own inputs are gone by the time the row comes
                // back, and inventing one would silently make an event's
                // identity the row's. Staging happens inside the transaction
                // that made the row, which is where the two values a payload
                // legitimately mints -- its own id and the moment it happened
                // -- can be produced honestly.
                jails_model::ParameterSource::Typed(ty) if staged => match ty {
                    TypeRef::Builtin(BuiltinType::Uuid) => {
                        imports.insert(format!(
                            "{}.TimeOrderedUuid",
                            model.project.package_for(Package::Domain)
                        ));
                        Ok("TimeOrderedUuid.next()".to_string())
                    }
                    TypeRef::Builtin(BuiltinType::Instant) => {
                        imports.insert("java.time.Instant".to_string());
                        Ok("Instant.now()".to_string())
                    }
                    _ => Err(CompileError::new(format!(
                        "outbox event `{}` declares `{}`, which nothing in the staging transaction can supply\n       fix: project it from a field of `{}`, or declare it `uuid` (minted) or `instant` (now)",
                        yielded.label, parameter.name, target.label
                    ))),
                },
                jails_model::ParameterSource::Typed(_) => Err(CompileError::new(format!(
                    "canonical event `{}` declares `{}`, which the target row does not carry\n       fix: project it from a field of `{}`, or deliver this command through an outbox, which can mint one",
                    yielded.label, parameter.name, target.label
                ))),
            })
            .collect::<Result<Vec<_>, _>>()?
            .join(", ");
        publications.push(if staged {
            format!("\n        outbox.stage(new {event_type}({arguments}));")
        } else {
            format!("\n        events.publishEvent(new {event_type}({arguments}));")
        });
    }
    Ok(publications)
}

/// The ordering this query renders, with its direction.
///
/// The direction is why this is not `resolve_fields`: a flat `Vec<FieldId>`
/// has nowhere to hold one, so `order by [createdAt desc]` would compile to
/// `order by created_at` and a newest-first query would return oldest-first
/// with nothing to say so.
///
/// An ordering qualified by a join alias refuses: the `select` this emitter
/// builds names one table, so ordering by another one's column would produce
/// SQL that does not run.
fn ordering<'a>(
    operation: &Operation,
    target: &'a Entity,
    spec: &jails_model::Query,
) -> Result<Vec<(&'a Field, jails_model::SortDirection)>, CompileError> {
    spec.semantics
        .order
        .iter()
        .map(|ordering| {
            if ordering.field.qualifier.is_some() || ordering.field.entity != target.id {
                return Err(CompileError::new(format!(
                    "canonical query `{}` orders by a joined column
       fix: order by a field of `{}`, or eject this adapter and write the statement by hand",
                    operation.label, target.label
                )));
            }
            target
                .field(&ordering.field.field)
                .map(|field| (field, ordering.direction))
                .ok_or_else(|| {
                    CompileError::new(format!(
                        "linked operation `{}` references missing ordering field `{}`",
                        operation.label, ordering.field.field
                    ))
                })
        })
        .collect()
}

fn resolve_fields<'a>(
    operation: &Operation,
    target: &'a Entity,
    ids: &[jails_model::FieldId],
    role: &str,
) -> Result<Vec<&'a Field>, CompileError> {
    ids.iter()
        .map(|id| {
            target.field(id).ok_or_else(|| {
                CompileError::new(format!(
                    "linked operation `{}` references missing {role} field `{id}`",
                    operation.label
                ))
            })
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn operation_file(
    model: &AppModel,
    capability_id: &str,
    operation: &Operation,
    target: &Entity,
    type_name: &str,
    artifact_suffix: &str,
    compiler_pass: &str,
    imports: BTreeSet<String>,
    body: String,
) -> Result<(ProjectPath, RenderedFile), CompileError> {
    let package = model.project.package_for(Package::AdaptersJdbc);
    let artifact_id = format!(
        "art_{capability_id}_{}_{artifact_suffix}",
        operation.id.as_str()
    );
    let rendered = render(&package, &imports, &body, &artifact_id);
    let path = ProjectPath::parse(format!(
        "{JAVA_ROOT}/{}/{}.java",
        package.replace('.', "/"),
        type_name
    ))
    .map_err(CompileError::new)?;
    Ok((
        path,
        RenderedFile {
            kind: FileKind::JavaMain,
            mode: FileMode::Regular,
            bytes: rendered.into_bytes(),
            provenance: Provenance {
                artifact_id,
                ejection_id: None,
                ejectable: true,
                semantic_ids: BTreeSet::from([
                    capability_id.to_string(),
                    operation.id.as_str().to_string(),
                    target.id.as_str().to_string(),
                ]),
                compiler_pass: compiler_pass.to_string(),
            },
        },
    ))
}

fn java_string(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn scopes(entity: &Entity) -> Vec<&Field> {
    entity
        .fields
        .iter()
        .filter(|field| field.semantics.scope.is_some())
        .collect()
}

fn context_parameter(model: &AppModel, entity: &Entity, imports: &mut BTreeSet<String>) -> String {
    if scopes(entity).is_empty() {
        String::new()
    } else {
        imports.insert(format!(
            "{}.ExecutionContext",
            model.project.package_for(Package::Application)
        ));
        "ExecutionContext context, ".to_string()
    }
}

fn context_value(field: &Field, imports: &mut BTreeSet<String>) -> String {
    let scope = field
        .semantics
        .scope
        .as_ref()
        .expect("context values are requested only for scope fields");
    let claim = java_string(&scope.claim);
    match field.ty {
        TypeRef::Builtin(BuiltinType::String) => format!("context.claim({claim})"),
        TypeRef::Builtin(BuiltinType::Uuid) => {
            imports.insert("java.util.UUID".to_string());
            format!("UUID.fromString(context.claim({claim}))")
        }
        TypeRef::Builtin(BuiltinType::Integer) => {
            format!("Integer.parseInt(context.claim({claim}))")
        }
        TypeRef::Builtin(BuiltinType::Long) => {
            format!("Long.parseLong(context.claim({claim}))")
        }
        _ => unreachable!("the linker accepts only string, uuid, int, and long scope fields"),
    }
}

fn assignment_sql_value(
    operation: &Operation,
    field: &Field,
    value: &Value,
) -> Result<String, CompileError> {
    match value {
        Value::String(value) | Value::EnumConstant(value) => {
            Ok(format!("'{}'", value.replace('\'', "''")))
        }
        Value::Integer(value) | Value::Decimal(value) if safe_numeric_literal(value) => {
            Ok(value.clone())
        }
        Value::Boolean(value) => Ok(value.to_string()),
        Value::Function { name, arguments } if name == "now" && arguments.is_empty() => {
            Ok("current_timestamp".to_string())
        }
        _ => Err(CompileError::new(format!(
            "canonical operation `{}` cannot lower the constant assigned to `{}`\n       fix: use a string, enum, numeric, boolean, or `now()` constant, or eject the implementation boundary",
            operation.label, field.label
        ))),
    }
}

fn safe_numeric_literal(value: &str) -> bool {
    let bytes = value.as_bytes();
    let mut index = usize::from(matches!(bytes.first(), Some(b'+' | b'-')));
    let integer_start = index;
    while bytes.get(index).is_some_and(u8::is_ascii_digit) {
        index += 1;
    }
    let mut has_digit = index > integer_start;
    if bytes.get(index) == Some(&b'.') {
        index += 1;
        let fraction_start = index;
        while bytes.get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
        }
        has_digit |= index > fraction_start;
    }
    if !has_digit {
        return false;
    }
    if matches!(bytes.get(index), Some(b'e' | b'E')) {
        index += 1;
        if matches!(bytes.get(index), Some(b'+' | b'-')) {
            index += 1;
        }
        let exponent_start = index;
        while bytes.get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
        }
        if index == exponent_start {
            return false;
        }
    }
    index == bytes.len()
}

#[cfg(test)]
mod tests {
    use super::safe_numeric_literal;

    #[test]
    fn assignment_numbers_are_single_sql_literals_not_token_sequences() {
        for valid in ["0", "-12", "+3.5", ".25", "6e-4"] {
            assert!(safe_numeric_literal(valid), "rejected `{valid}`");
        }
        for invalid in ["", "+", ".", "1-2", "1e", "0;drop table task"] {
            assert!(!safe_numeric_literal(invalid), "accepted `{invalid}`");
        }
    }
}
