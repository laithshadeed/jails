//! How a `jails g usecase|query|transition|event` invocation becomes one JDL
//! operation block.
//!
//! Split from [`super`] by secret: that module decides which declaration a
//! command is appending and where, and this one decides what an operation
//! block says. They move for different reasons -- a new entity flag touches
//! neither of these, and every operation flag touches only this.
//!
//! **The pattern to expect here is a flag the model already carries.** `--via`,
//! `--order-by`, `--on-conflict`, `--set`, `--select`, `--if-match`, `--bind`
//! and `--consumes` were each parsed by the JDL grammar, linked into
//! `OperationSemantics`, and read by the compiler long before this frontend
//! would emit them -- so the refusal told the reader to hand-edit
//! `.jails/model.jdl`, which is true and useless. Before adding an emitter for
//! the next one, check whether it is only the syntax editor that is missing.

use super::{MODEL_PATH, java_to_label, java_type_name};
use crate::cli::GenerateArgs;
use crate::generate::ArtifactKind;
use crate::model_generate;
use jails_support::{Failure, Result};

pub(super) fn operation_declaration(
    args: &GenerateArgs,
    model: &jails_model::AppModel,
    entity_label: &str,
    fields: &[String],
    v1: bool,
) -> Result<String> {
    let kind = match args.kind {
        ArtifactKind::Usecase => "command",
        ArtifactKind::Query => "query",
        ArtifactKind::Transition => "transition",
        ArtifactKind::Event => "event",
        _ => unreachable!("operation generation accepts only operation kinds"),
    };
    // **A pinned component is not a caller input.** `--set seen=true` says the
    // transition writes a constant, so `seen` leaves the parameter list as
    // well as the update list -- the compiler refuses a field supplied "from
    // both input and a constant assignment", which is the check that keeps the
    // two meanings apart.
    let pinned = args
        .set
        .iter()
        .filter_map(|assignment| assignment.split_once('='))
        .map(|(component, _)| java_to_label(component))
        .collect::<Vec<_>>();
    let parameters = fields
        .iter()
        .filter(|field| !pinned.contains(field))
        .cloned()
        .collect::<Vec<_>>();
    let mut output = format!(
        "  {kind} {}({}) @id(op_{}) {{\n",
        args.name,
        parameters.join(", "),
        java_to_label(&args.name)
    );
    if args.kind == ArtifactKind::Query {
        if let Some(order_by) = &args.order_by {
            let order_by = order_by
                .split(',')
                .map(str::trim)
                // **`asc`/`desc` pass through.** `operation_order_list`
                // has parsed a direction since the grammar existed --
                // `order by [ timeStamp desc ]` -- and this refused to emit
                // one, so a query whose whole point is "newest first" could
                // not reach a canonical project. The field still goes through
                // the checked resolver; only the direction rides beside it.
                .map(|item| {
                    let (field, direction) = match item.split_once(char::is_whitespace) {
                        Some((field, rest)) => (field, rest.trim()),
                        None => (item, ""),
                    };
                    if field.is_empty() {
                        return Err(Failure::Told(
                            "canonical query ordering needs a field name.\n       fix: give `--order-by` a comma-separated field list"
                                .to_string(),
                        ));
                    }
                    let direction = match direction {
                        "" | "asc" => "",
                        "desc" => " desc",
                        other => {
                            return Err(Failure::Told(format!(
                                "`{other}` is not an ordering direction.\n       fix: use `asc` or `desc`"
                            )));
                        }
                    };
                    let label =
                        model_generate::operation_field_label(model, entity_label, field)?;
                    Ok(format!("{label}{direction}"))
                })
                .collect::<Result<Vec<_>>>()?;
            if v1 {
                output.push_str(&format!("    order by [{}]\n", order_by.join(", ")));
            } else {
                output.push_str(&format!("    orderBy: {}\n", order_by.join(", ")));
            }
        }
        if let Some(limit) = args.limit {
            if v1 {
                output.push_str(&format!("    limit {limit}\n"));
            } else {
                output.push_str(&format!("    limit: {limit}\n"));
            }
        }
    }
    if args.kind == ArtifactKind::Usecase
        && let Some(yields) = &args.strategy_yields
    {
        // `--yields` on a use case is the legacy spelling of *staged*
        // delivery: it is what `g usecase --yields E` has always built an
        // outbox for. Writing `emit` alone would honour the flag with direct
        // publication, which is the weaker guarantee and the exact
        // substitution `deliver` exists to make impossible.
        let event = java_to_label(yields);
        if v1 {
            output.push_str(&format!("    emit {event}\n    deliver outbox\n"));
        } else {
            output.push_str(&format!("    emits: {event}\n    delivery: outbox\n"));
        }
    }
    // `--via` is a `join`: `g query --via User` reads `users` alongside
    // `messages`, on the `userId` the child already declares. The model has
    // carried `Query.semantics.joins` and the JDL has parsed
    // `join User as user on userId -> user.id` all along; only this frontend
    // refused to translate the flag.
    //
    // The column is derived from the two entities rather than recorded, which
    // is the legacy `join` module's rule: `<parent>Id` on the child, and the
    // parent's own primary key on the other side. A reference the model does
    // not declare is named rather than guessed at.
    if args.kind == ArtifactKind::Query
        && let Some(via) = &args.via
    {
        let parent = java_type_name(via);
        let parent_label = java_to_label(&parent);
        let parent_entity = model
            .entities
            .values()
            .find(|entity| entity.label == parent_label)
            .ok_or_else(|| {
                Failure::Told(format!(
                    "`{parent}` does not name a canonical entity.\n       fix: choose an entity declared in `{MODEL_PATH}`"
                ))
            })?;
        let key = parent_entity
            .fields
            .iter()
            .find(|field| field.primary_key)
            .ok_or_else(|| {
                Failure::Told(format!(
                    "`{parent}` has no primary key, so nothing can join to it.\n       fix: declare one component `@pk`"
                ))
            })?;
        let child = model
            .entities
            .values()
            .find(|entity| entity.label == entity_label)
            .and_then(|entity| {
                entity
                    .fields
                    .iter()
                    .find(|field| field.label == format!("{parent_label}_id"))
            })
            .ok_or_else(|| {
                Failure::Told(format!(
                    "`{}` declares no `{parent_label}_id` component, so it does not reference `{parent}`.\n       fix: add one, or drop `--via {parent}`",
                    args.name
                ))
            })?;
        let alias = &parent_label;
        if v1 {
            output.push_str(&format!(
                "    join {parent} as {alias} on {} -> {alias}.{}\n",
                child.label, key.label
            ));
        } else {
            output.push_str(&format!(
                "    via: {parent}\n    join_on: {} -> {}\n",
                child.label, key.label
            ));
        }
    }
    // `--on-conflict` is `conflict on [field]`: one
    // `insert ... on conflict (col) do nothing returning`, then a read of the
    // row that was already there. The model has carried `conflict_key` and the
    // JDL has parsed `conflict on [...]` all along; only this frontend refused
    // to translate the flag, so `g usecase --on-conflict` could not reach a
    // canonical project at all.
    if args.kind == ArtifactKind::Usecase
        && let Some(component) = &args.on_conflict
    {
        let label = model_generate::operation_field_label(model, entity_label, component)?;
        if v1 {
            output.push_str(&format!("    conflict on [{label}]\n"));
        } else {
            output.push_str(&format!("    conflict_on: {label}\n"));
        }
    }
    // **`set`, `select`, `if-match`, `bind` and `consumes` are grammar and
    // model already**; only this frontend refused them, which is the same
    // shape `--via` had. `TransitionSemantics` carries `select`, `assignments`
    // and `precondition`, every operation carries `bindings`, and the parser
    // has read `set x = 1`, `select [a]`, `if-match optional`,
    // `bind p from form "wire"` and `consumes form` all along -- so the
    // refusal told the reader to hand-edit `.jails/model.jdl`, which is true
    // and useless.
    if args.kind == ArtifactKind::Transition {
        // **The selector and every pinned component are subtracted from the
        // update.** A transition does not write the column it selects by, and
        // a component the command pins is a constant rather than something the
        // caller supplies -- the linker refuses either overlap by name
        // (`model-transition-field-role`), which is the check that makes the
        // three roles mean different things.
        let selector = args.select.as_ref().map(|field| java_to_label(field));
        // A scope, a version and an `@updated` stamp are the compiler's to
        // write -- the linker refuses an explicit target for any of them
        // (`model-managed-field-target`). `--on Message id version` is the
        // familiar spelling of a transition and always names the version, so
        // subtracting it here is what lets that spelling keep working.
        let managed = model
            .entities
            .values()
            .find(|candidate| candidate.label == entity_label)
            .map(|entity| {
                entity
                    .fields
                    .iter()
                    .filter(|field| {
                        field.semantics.scope.is_some()
                            || field.semantics.version
                            || field.semantics.updated
                    })
                    .map(|field| field.label.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let updated = fields
            .iter()
            .filter(|field| {
                selector.as_deref() != Some(field.as_str())
                    && !pinned.contains(field)
                    && !managed.contains(field)
            })
            .cloned()
            .collect::<Vec<_>>();
        if v1 {
            if let Some(selector) = &selector {
                output.push_str(&format!("    select [{selector}]\n"));
            }
            // Omitted rather than empty when every component is pinned,
            // managed or the selector: `update []` is not a field reference
            // list the grammar accepts, and a transition whose whole effect is
            // a pinned constant is an ordinary shape.
            if !updated.is_empty() {
                output.push_str(&format!("    update [{}]\n", updated.join(", ")));
            }
            if let Some(policy) = args.if_match {
                output.push_str(&format!("    if-match {}\n", policy.label()));
            }
        } else {
            output.push_str(&format!("    sets: {}\n", updated.join(", ")));
        }
        if let Some(yields) = &args.strategy_yields {
            if v1 {
                output.push_str(&format!("    emit {}\n", java_to_label(yields)));
            } else {
                output.push_str(&format!("    yields: {}\n", java_to_label(yields)));
            }
        }
    }
    if v1 {
        for assignment in &args.set {
            let (component, value) = assignment.split_once('=').ok_or_else(|| {
                Failure::Told(format!(
                    "`{assignment}` is not a pinned component\n       fix: write `<component>=<value>`, for example `seen=true`"
                ))
            })?;
            output.push_str(&format!(
                "    set {} = {}\n",
                java_to_label(component),
                literal(value)
            ));
        }
        for binding in &args.bind {
            let (component, wire) = binding.split_once('=').ok_or_else(|| {
                Failure::Told(format!(
                    "`{binding}` is not a parameter binding\n       fix: write `<component>=<parameter>`, for example `id=note_id`"
                ))
            })?;
            // From the form, because `--bind` is refused without
            // `--consumes form`: the whole reason a binding exists is that a
            // form field's name is the page's, not the model's.
            output.push_str(&format!(
                "    bind {} from form {}\n",
                java_to_label(component),
                serde_json::to_string(wire).map_err(|error| Failure::Told(format!(
                    "could not quote a bound parameter name: {error}"
                )))?
            ));
        }
    }
    // **`consumes` rides on the route**, which is the grammar's shape rather
    // than a statement of its own -- `route POST "/x" consumes form`. So a
    // format asked for without a path needs a route to carry it, and the one
    // it gets is the conventional route the linker would have derived anyway
    // (`derived_route`), written out explicitly.
    let path = args.path.clone().or_else(|| {
        args.consumes.map(|_| {
            let name = java_to_label(&args.name).replace('_', "-");
            match args.kind {
                ArtifactKind::Transition => format!("/actions/{name}/{{id}}"),
                ArtifactKind::Query => format!("/queries/{name}"),
                _ => format!("/actions/{name}"),
            }
        })
    });
    if let Some(path) = &path {
        let method = match args.kind {
            ArtifactKind::Usecase => "POST".to_string(),
            ArtifactKind::Query if fields.is_empty() => "GET".to_string(),
            ArtifactKind::Query => "POST".to_string(),
            ArtifactKind::Transition => args.method.map_or_else(
                || "PUT".to_string(),
                |method| method.label().to_ascii_uppercase(),
            ),
            _ => unreachable!("event paths are rejected during validation"),
        };
        if v1 {
            let path = serde_json::to_string(path)
                .map_err(|error| Failure::Told(format!("could not quote route path: {error}")))?;
            let consumes = args.consumes.map_or_else(String::new, |format| {
                format!(" consumes {}", format.label())
            });
            output.push_str(&format!("    route {method} {path}{consumes}\n"));
        } else {
            output.push_str(&format!("    route: {method} {path}\n"));
        }
    }
    output.push_str("  }");
    Ok(output)
}

/// A pinned component's value, as a JDL literal.
///
/// `true`, `false` and a number are bare; anything else is a quoted string.
/// The grammar's `take_literal` draws the same line, and quoting a boolean
/// would make `set seen = "true"` a text assignment to a boolean column.
fn literal(value: &str) -> String {
    if matches!(value, "true" | "false") || value.parse::<f64>().is_ok() {
        return value.to_string();
    }
    serde_json::to_string(value).unwrap_or_else(|_| format!("\"{value}\""))
}
