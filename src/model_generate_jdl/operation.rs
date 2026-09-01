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
    // **A scope, a version and an `@updated` stamp are never request inputs.**
    // The linker says so by name -- "version is request-visible only through
    // an if-match precondition" -- and the familiar spelling of a transition
    // always names the version in its field list, so it has to come out here
    // as well as out of the update list below.
    // **The two lists exclude different things, and the version is why.**
    // Without `--if-match` the linker refuses the version as a request input
    // at all; with it, the linker *requires* one shorthand parameter for it,
    // because stating the value you expect to be replacing is what a
    // compare-and-swap is. It is never a *target*, though -- the compiler
    // increments it -- so it leaves the update list either way. A scope and an
    // `@updated` stamp are neither input nor target.
    // **The precondition is the default, not a flag.** An entity with a
    // `@version` column has one for exactly one reason -- so a stale update is
    // a no-op rather than a blind overwrite -- and a transition that omitted
    // the guard silently gave that up. `--if-match optional` relaxes it to
    // "check it if the caller sent one"; there is no spelling that removes it,
    // because an entity that does not want one does not declare the column.
    //
    // A transition only. A command writes the row, so its version is the
    // compiler's initial value and there is nothing for a caller to state --
    // which is what the linker refuses a version parameter on anything else
    // for.
    let precondition = (args.kind == ArtifactKind::Transition
        && model
            .entities
            .values()
            .find(|candidate| candidate.label == entity_label)
            .is_some_and(|entity| entity.fields.iter().any(|field| field.semantics.version)))
    .then(|| {
        args.if_match
            .unwrap_or(jails_spec::spec::kind::Precondition::Required)
    });
    let managed = |as_input: bool| {
        model
            .entities
            .values()
            .find(|candidate| candidate.label == entity_label)
            .map(|entity| {
                entity
                    .fields
                    .iter()
                    .filter(|field| {
                        field.semantics.scope.is_some()
                            || (field.semantics.version && !(as_input && precondition.is_some()))
                            || field.semantics.updated
                    })
                    .map(|field| field.label.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    };
    let managed_inputs = managed(true);
    let managed_targets = managed(false);
    let borrowed = |field: &String| match (args.kind, args.via.as_ref(), field.split_once('.')) {
        (ArtifactKind::Usecase, Some(via), Some((_, member))) => {
            format!("{}.{member} as {member}", java_type_name(via))
        }
        _ => field.clone(),
    };
    let parameters = fields
        .iter()
        .filter(|field| !pinned.contains(field) && !managed_inputs.contains(field))
        .map(borrowed)
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
    // **`--via` on a command is a key resolution**, not a join: the caller
    // states a natural key of the parent -- an author's email -- and the
    // command resolves the foreign key from it before inserting. The grammar
    // has read `resolve authorId from Author.id where Author.email = email`
    // all along, and the parameter form is `Author.email as email` rather than
    // the query's `author.email`, because a command has no join to alias.
    if args.kind == ArtifactKind::Usecase
        && let Some(via) = &args.via
    {
        let parent_type = java_type_name(via);
        let parent = model
            .entities
            .values()
            .find(|entity| entity.names.java_type == parent_type)
            .ok_or_else(|| {
                Failure::Told(format!(
                    "`{parent_type}` does not name a canonical entity.\n       fix: choose an entity declared in `{MODEL_PATH}`"
                ))
            })?;
        let parent_key = parent
            .fields
            .iter()
            .find(|field| field.primary_key)
            .ok_or_else(|| {
                Failure::Told(format!(
                    "`{parent_type}` has no primary key, so nothing can resolve to it.\n       fix: declare one component `@pk`"
                ))
            })?;
        let child = model
            .entities
            .values()
            .find(|entity| entity.label == entity_label);
        let foreign_key = model
            .relations
            .values()
            .find(|relation| relation.parent == parent.id)
            .and_then(|relation| relation.mappings.first())
            .and_then(|mapping| child.and_then(|entity| entity.field(&mapping.local)))
            // **The conventional column when no association is declared.**
            // `--via Author` on an entity carrying `authorId` is the shape the
            // legacy recipe read, and refusing it would make the flag usable
            // only after `g association`, which is not what it is for. A
            // declared relation still wins, because it states the pairing
            // rather than assuming it.
            .or_else(|| {
                let conventional = format!("{}_id", parent.label);
                child.and_then(|entity| {
                    entity
                        .fields
                        .iter()
                        .find(|field| field.label == conventional)
                })
            })
            .ok_or_else(|| {
                Failure::Told(format!(
                    "nothing on `{entity_label}` points at `{parent_type}`.\n       fix: give it a `{}_id` column, or declare the association with `jails g association {parent_type}Link <childField>=<parentField> --on {entity_label} --yields {parent_type}`",
                    java_to_label(&parent_type)
                ))
            })?;
        // **Which component identifies the parent is the reader's to state,
        // and jails' to refuse when they have not.** A qualified
        // `Author.email` says it outright; an unqualified list says it by
        // naming exactly one component the parent has. Zero and two are both
        // failures with an answer, and the compiler's own refusal -- "cannot
        // construct required field `author_id`" -- names a column the reader
        // never typed.
        // **The caller states which component identifies the parent, and the
        // field list says so by resolving against it.** A field the child does
        // not declare is qualified with the parent's alias on the way in --
        // `author.email` -- so the lookup is whichever fields did that. Zero
        // and two are both failures with an answer, and the compiler's own
        // refusals name a column (`author_id`) or a uniqueness rule the reader
        // never typed.
        let lookups = fields
            .iter()
            .filter_map(|field| field.split_once('.'))
            .map(|(_, member)| member.split(':').next().unwrap_or(member).to_string())
            .collect::<Vec<_>>();
        let candidates = parent
            .fields
            .iter()
            .filter(|field| !field.primary_key)
            .map(|field| format!("`{}`", field.names.java_member))
            .collect::<Vec<_>>()
            .join(", ");
        match lookups.len() {
            1 => {}
            0 => {
                return Err(Failure::Told(format!(
                    "`--via {parent_type}` resolves the foreign key from a component the caller states, and none of its fields names a component of {parent_type}.\n       fix: carry one of {candidates}"
                )));
            }
            count => {
                return Err(Failure::Told(format!(
                    "`--via {parent_type}` resolves one foreign key, and this use case names {count} components of {parent_type}.\n       fix: keep one of {}",
                    lookups
                        .iter()
                        .map(|member| format!("`{member}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                )));
            }
        }
        for member in lookups {
            output.push_str(&format!(
                "    resolve {} from {parent_type}.{} where {parent_type}.{member} = {member}\n",
                foreign_key.label, parent_key.label
            ));
        }
    }
    if args.kind == ArtifactKind::Transition {
        // **The selector and every pinned component are subtracted from the
        // update.** A transition does not write the column it selects by, and
        // a component the command pins is a constant rather than something the
        // caller supplies -- the linker refuses either overlap by name
        // (`model-transition-field-role`), which is the check that makes the
        // three roles mean different things.
        // **With no `--select`, the row selector is the primary key.** That is
        // what a transition selects by, and the familiar spelling names it in
        // the field list -- `g transition MarkSeen id:long version:long` --
        // so without this it arrived as an update and the compiler refused it
        // as "attempts to rewrite primary key `id`". Legacy inferred the same
        // thing; this makes the inference explicit in the model rather than
        // leaving it to be re-derived.
        let selector = args.select.as_ref().map_or_else(
            || {
                model
                    .entities
                    .values()
                    .find(|candidate| candidate.label == entity_label)
                    .and_then(|entity| {
                        entity
                            .fields
                            .iter()
                            .find(|field| field.primary_key)
                            .map(|field| field.label.clone())
                    })
            },
            |field| Some(java_to_label(field)),
        );
        // **Checked against the entity before it reaches the model.** The
        // linker refuses an unknown selector too, and names it as
        // `$.operations.x.semantics.select` -- a JSON path for a flag the
        // reader typed, and no list of what would have worked. This says both.
        if let Some(selector) = selector.as_deref()
            && let Some(entity) = model
                .entities
                .values()
                .find(|candidate| candidate.label == entity_label)
            && !entity.fields.iter().any(|field| field.label == selector)
        {
            return Err(Failure::Told(format!(
                "`{selector}` does not name a component of `{}`.\n       fix: pass `--select` one of {}",
                entity.names.java_type,
                entity
                    .fields
                    .iter()
                    .map(|field| format!("`{}`", field.names.java_member))
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
        let updated = fields
            .iter()
            .filter(|field| {
                selector.as_deref() != Some(field.as_str())
                    && !pinned.contains(field)
                    && !managed_targets.contains(field)
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
            if let Some(policy) = precondition {
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
        let target = model
            .entities
            .values()
            .find(|candidate| candidate.label == entity_label);
        for assignment in &args.set {
            let (component, value) = assignment.split_once('=').ok_or_else(|| {
                Failure::Told(format!(
                    "`{assignment}` is not a pinned component\n       fix: write `<component>=<value>`, for example `seen=true`"
                ))
            })?;
            // **Refused before any type is consulted.** A pin is a constant
            // this project declares, and anything an expression could hide in
            // never reaches a type at all -- so the message is about the value
            // rather than about the component whose type it failed to be.
            if let Some(character) = value
                .chars()
                .find(|character| "()\\\"'`;{}<>".contains(*character) || character.is_control())
            {
                return Err(Failure::Told(format!(
                    "`{value}` contains `{character}`, so it is not a literal.\n       fix: pin a constant -- a number, `true`, `false`, a plain string, or an enum constant"
                )));
            }
            if let Some(target) = target {
                refuse_unpinnable(model, target, &args.fields, component, value)?;
            }
            output.push_str(&format!(
                "    set {} = {}\n",
                java_to_label(component),
                literal(value)
            ));
        }
        // **A binding is an instruction to Spring's data binder, and the data
        // binder only reads a form.** On a JSON body Jackson does the binding
        // and the annotation is not even looked at -- so a `--bind` there is a
        // wire name the reader asked for and silently did not get.
        if !args.bind.is_empty() && args.consumes != Some(jails_spec::spec::kind::WireFormat::Form)
        {
            return Err(Failure::Told(
                "this endpoint reads a JSON body, where the wire names come from Jackson and `--bind` is not read at all.\n       fix: pass `--consumes form`, or set `spring.jackson.property-naming-strategy` for the whole project".to_string(),
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
    // **A path variable that is not the row selector has nowhere to go.** A
    // transition takes exactly one value that identifies the row, and it is
    // whatever `--select` names; a `{id}` in the URL of a transition selecting
    // on `userId` would be mounted and then ignored, which is a route that
    // answers and does the wrong thing. Two variables is the same fault twice:
    // only one of them can be the selector.
    if let Some(path) = &path
        && args.kind == ArtifactKind::Transition
    {
        let variables: Vec<&str> = path
            .split('{')
            .skip(1)
            .filter_map(|rest| rest.split_once('}'))
            .map(|(name, _)| name)
            .collect();
        // The same derivation the declaration above uses: `--select`, or the
        // entity's primary key when the reader named none.
        let expected = args
            .select
            .as_ref()
            .map(|field| java_to_label(field))
            .or_else(|| {
                model
                    .entities
                    .values()
                    .find(|candidate| candidate.label == entity_label)
                    .and_then(|entity| {
                        entity
                            .fields
                            .iter()
                            .find(|field| field.primary_key)
                            .map(|field| field.label.clone())
                    })
            })
            .map(|label| jails_model::lower_camel_case(&label));
        if variables.len() > 1 {
            return Err(Failure::Told(format!(
                "a transition can bind one path variable -- the value that identifies the row -- and `{path}` has {}.\n       fix: keep `{{{}}}` and pass the rest in the body",
                variables.len(),
                expected.as_deref().unwrap_or(variables[0])
            )));
        }
        if let (Some(variable), Some(expected)) = (variables.first(), expected.as_deref())
            && *variable != expected
        {
            return Err(Failure::Told(format!(
                "this transition selects on `{expected}` and cannot take `{{{variable}}}` from the URL.\n       fix: pass `--select {variable}`, or name `{{{expected}}}` in the path"
            )));
        }
    }
    // **A query's path variable has to name one of its filters.** The
    // controller binds `@ModelAttribute`, and Spring's data binder fills the
    // criteria from the request parameters *and* the URI template variables
    // together -- so `/tickets/{userId}` binds `userId` and a mix with
    // `?subject=x` is ordinary. A variable naming nothing binds nothing: the
    // route is mounted, the value is dropped, and the query answers with the
    // filter unset. That is the failure this refuses.
    if let Some(path) = &path
        && args.kind == ArtifactKind::Query
    {
        // The specs reaching here carry model labels (`user_id:long`), and
        // the URL carries the Java member the criteria record declares -- so
        // the comparison and the suggestion are both in the reader's spelling.
        let filters: Vec<String> = fields
            .iter()
            .map(|field| {
                jails_model::lower_camel_case(
                    field
                        .split_once(':')
                        .map_or(field.as_str(), |(name, _)| name),
                )
            })
            .collect();
        for variable in path
            .split('{')
            .skip(1)
            .filter_map(|rest| rest.split_once('}'))
            .map(|(name, _)| name)
        {
            if !filters.iter().any(|filter| filter == variable) {
                return Err(Failure::Told(format!(
                    "this query's route names `{{{variable}}}`, which is not one of its filters.\n       fix: name one of {}, or drop it from `--path`",
                    filters
                        .iter()
                        .map(|filter| format!("`{{{filter}}}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                )));
            }
        }
    }
    if let Some(path) = &path {
        let method = match args.kind {
            ArtifactKind::Usecase => "POST".to_string(),
            // **A query answers GET, whatever its filters.** The controller
            // binds its criteria with `@ModelAttribute`, which reads request
            // parameters and URI template variables -- never a JSON body -- so
            // a POST here produced a route that could only be driven by a form
            // post nobody writes, and a generated proof that had to post one.
            // `--consumes json` is the one way to ask for a body, and it is
            // the only shape that needs a verb with one.
            ArtifactKind::Query
                if args.consumes == Some(jails_spec::spec::kind::WireFormat::Json)
                    && !fields.is_empty() =>
            {
                "POST".to_string()
            }
            ArtifactKind::Query => "GET".to_string(),
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
/// Everything a pin can be wrong about, refused before anything is written.
///
/// **The literal is resolved against the component's declared type**, which is
/// the whole reason `--set` is not a passthrough: `senderType=SHOUTING` is
/// text that renders and Java that does not compile, in a file the reader did
/// not write. The model already holds the type and the enum's constants, so
/// none of this is read off disk.
fn refuse_unpinnable(
    model: &jails_model::AppModel,
    target: &jails_model::Entity,
    declared: &[String],
    component: &str,
    value: &str,
) -> Result<()> {
    let label = java_to_label(component);
    let Some(field) = target.fields.iter().find(|field| field.label == label) else {
        return Err(Failure::Told(format!(
            "`{}` has no component with that name.\n       fix: pin one of {}",
            target.names.java_type,
            target
                .fields
                .iter()
                .map(|field| format!("`{}`", field.names.java_member))
                .collect::<Vec<_>>()
                .join(", ")
        )));
    };
    // A pin the caller can override is not a pin, so one of the two has to
    // go. Checked against what the reader typed rather than the filtered
    // parameter list, which has already had the pinned components taken out of
    // it -- so the collision would never be visible there.
    if declared
        .iter()
        .any(|field| java_to_label(field.split(':').next().unwrap_or(field)) == label)
    {
        // The primary key is the sharper case of the same mistake: it selects
        // the row rather than being written to it, so a pin of it is a value
        // that would have to be two things at once.
        if field.primary_key {
            return Err(Failure::Told(format!(
                "`{component}` identifies the row, so it cannot also be a value this endpoint writes.\n       fix: drop the `--set`, and change a component that is not the key"
            )));
        }
        return Err(Failure::Told(format!(
            "this endpoint both accepts `{component}` and pins it, and a pin the caller can override is not one.\n       fix: drop `{component}` from the field list, or drop the `--set`"
        )));
    }
    match &field.ty {
        // A pinned instant is a timestamp frozen at generation time, which is
        // never what anyone means. `@default(now())` is the declaration that
        // says "when the row was written".
        jails_model::TypeRef::Builtin(builtin) if temporal(*builtin) => {
            Err(Failure::Told(format!(
                "`{component}` is a moment in time, and a pin is a constant this project declares -- not a value with a lifetime of its own.\n       fix: declare it `@default(now())`, or carry it in the request"
            )))
        }
        jails_model::TypeRef::Builtin(jails_model::BuiltinType::Boolean)
            if !matches!(value, "true" | "false") =>
        {
            Err(Failure::Told(format!(
                "`{component}` is a boolean and `{value}` is not one.\n       fix: pin `true` or `false`"
            )))
        }
        jails_model::TypeRef::Builtin(builtin)
            if numeric(*builtin) && value.parse::<f64>().is_err() =>
        {
            Err(Failure::Told(format!(
                "`{component}` is a number and `{value}` is not one.\n       fix: write a number"
            )))
        }
        jails_model::TypeRef::External(name) => {
            let Some(declared) = model
                .entities
                .values()
                .find(|entity| &entity.names.java_type == name)
                .filter(|entity| entity.facets.contains(&jails_model::Facet::Enum))
            else {
                return Err(Failure::Told(format!(
                    "`{component}` is a `{name}`, and the only project type jails can resolve a constant of is an enum it declares.\n       fix: declare `{name}` with `jails g enum`, or carry `{component}` in the request"
                )));
            };
            if declared
                .enum_constants
                .iter()
                .any(|constant| constant.java_name == value)
            {
                return Ok(());
            }
            Err(Failure::Told(format!(
                "`{value}` is not a constant of {name}.\n       fix: pin one of {}",
                declared
                    .enum_constants
                    .iter()
                    .map(|constant| constant.java_name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )))
        }
        _ => Ok(()),
    }
}

/// Whether this builtin carries a moment rather than a value.
fn temporal(builtin: jails_model::BuiltinType) -> bool {
    use jails_model::BuiltinType::{Date, DateTime, Instant};
    matches!(builtin, Instant | Date | DateTime)
}

/// Whether a pin of this builtin has to parse as a number.
fn numeric(builtin: jails_model::BuiltinType) -> bool {
    use jails_model::BuiltinType::{Decimal, Double, Integer, Long};
    matches!(builtin, Integer | Long | Double | Decimal)
}

fn literal(value: &str) -> String {
    if matches!(value, "true" | "false") || value.parse::<f64>().is_ok() {
        return value.to_string();
    }
    serde_json::to_string(value).unwrap_or_else(|_| format!("\"{value}\""))
}
