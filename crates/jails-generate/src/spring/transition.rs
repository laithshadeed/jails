//! `g transition`: moving a resource from one state to the next.
//!
//! The sibling of `usecase` (`workflow.rs`, which documents the shape all three
//! share) and `query`. What makes this one its own kind is the state enum: the
//! generated adapter refuses a move the enum does not allow, so the illegal
//! transition fails in the database rather than in a code review.

use super::workflow::{scope_test_parts, usecase_command_java};

mod proof;
use super::*;
use proof::{jdbc_transition_it_java, transition_controller_test_java};

/// The three lists that together describe one optimistic update: the target's
/// columns, the command's columns, and which fields actually change.
///
/// Derived in one pass from one field spec, which is the whole reason they
/// cannot disagree -- so they travel as one value.
struct Update<'a> {
    target_columns: Vec<crate::sql::Column>,
    command_columns: Vec<crate::sql::Column>,
    fields: Vec<&'a crate::generate::Field>,
}

use crate::model::{Artifact, Layer, Slice};

pub(crate) fn transition_files(
    slice: &Slice,
    name: &str,
    target: &str,
    fields: &[crate::generate::Field],
    endpoint: Endpoint<'_>,
    selector: &str,
) -> jails_support::Result<Vec<Artifact>> {
    let root: &Path = slice.project().root();
    let service: &str = &slice.placed(Layer::Service);
    let web: &str = &slice.placed(Layer::Web);
    let domain: &str = &slice.owned(Layer::Domain);
    let adapters: &str = &slice.owned(Layer::Adapters);
    require_scope_authorizer(slice, "transition", name, fields)?;
    let target_fields = slice.record(Layer::Domain, target).ok_or_else(|| {
        format!("transition {name} targets {target}, but no record components could be read from {target}.java")
    })?;
    if fields.iter().any(|field| {
        field.optionality == crate::generate::Optionality::Nullable || field.collection
    }) {
        return Err(format!(
            "transition {name} accepts required scalar fields only so match and update semantics stay exact"
        ).into());
    }
    for field in fields {
        let Some(target_field) = target_fields
            .iter()
            .find(|candidate| candidate.name == field.name)
        else {
            return Err(format!(
                "transition {name} declares `{}`, but {target} has no component with that name",
                field.name
            )
            .into());
        };
        if usecase_normalized_type(&field.java_type)
            != usecase_normalized_type(&target_field.java_type)
        {
            return Err(format!(
                "transition {name} declares `{}` as {}, but {target} stores it as {}",
                field.name, field.java_type, target_field.java_type
            )
            .into());
        }
    }
    // A `--path` variable has to be *bound*, and the only value a transition
    // can bind from a URL is the one that identifies the row. So exactly one
    // variable is allowed and it must name the selector: with
    // `--path /admin_api/conversations/{userId}/status --select userId` the
    // key comes from the URL and the rest of the command from the body.
    //
    // Anything else is refused rather than mounted and ignored. The first
    // version of `--path` here mounted whatever it was given, and three
    // generated tests failed with `Not enough variable values available to
    // expand` -- a route that looks right and silently drops half of itself.
    let variables: Vec<&str> = endpoint
        .route
        .map(|route| {
            route
                .split('{')
                .skip(1)
                .filter_map(|rest| rest.split('}').next())
                .collect()
        })
        .unwrap_or_default();
    if variables.len() > 1 {
        return Err(format!(
            "transition {name} can bind one path variable, the one that identifies the row, \
             and this path has {}: {}.\n       fix: keep `{{{selector}}}` and move the rest \
             into the request body.",
            variables.len(),
            variables
                .iter()
                .map(|variable| format!("`{{{variable}}}`"))
                .collect::<Vec<_>>()
                .join(", ")
        )
        .into());
    }
    if let Some(first) = variables.first()
        && *first != selector
    {
        return Err(format!(
            "transition {name} cannot take `{{{first}}}` from the URL: it selects its row by \
             `{selector}`, and that is the only value a URL can identify a row with.\n                    fix: spell the variable `{{{selector}}}`, or name `{first}` the selector with \
             `--select {first}`."
        )
        .into());
    }
    let id = fields
        .iter()
        .find(|field| field.name == selector)
        .ok_or_else(|| {
            format!(
                "transition {name} needs the component that identifies the row, `{selector}`.\n                        fix: declare `{selector}:<type>` among its fields, or name another with \
                 `--select <field>`."
            )
        })?;
    let key = Key {
        component: selector,
        java_type: crate::generate::builtin_by_java_name(&id.java_type)
            .map(|(boxed, _)| boxed)
            .unwrap_or("String"),
        from_path: !variables.is_empty(),
    };
    // `missing.md` M11: both halves of this were good refusals and neither
    // said what to type, so a ported schema met the column requirement, then
    // met `g field`'s data-plan requirement, and only then had the two
    // commands. The sequence is knowable from here, so it is printed from
    // here.
    let version = fields
        .iter()
        .find(|field| field.name == "version")
        .ok_or_else(|| {
            format!(
                "transition {name} needs a required numeric `version` field: the update is a \
                 compare-and-set, so the row has to carry the version the caller matched \
                 against.\n       fix: run `jails g field {target} version:long \
                 --default-literal 0`, then add `version:long` to this command."
            )
        })?;
    if !matches!(usecase_normalized_type(&version.java_type), "long" | "int") {
        return Err(format!(
            "transition {name} needs `version:long` or `version:int`, not version:{}",
            version.java_type
        )
        .into());
    }
    let update_fields = fields
        .iter()
        .filter(|field| {
            field.name != id.name && field.name != version.name && !field.constraints.scoped
        })
        .collect::<Vec<_>>();
    if update_fields.is_empty() {
        return Err(format!(
            "transition {name} needs at least one field to update in addition to id, @scope fields, and version"
        ).into());
    }
    let target_columns = crate::sql::columns(&target_fields, slice.project(), domain, "rows");
    let command_columns = crate::sql::columns(fields, slice.project(), domain, "command");
    if target_columns
        .iter()
        .chain(command_columns.iter())
        .any(|column| !column.mapped())
    {
        return Err(format!("transition {name} contains a field Jails cannot map to JDBC").into());
    }
    let main_service = crate::generate::main_dir(root, service);
    let main_adapters = crate::generate::main_dir(root, adapters);
    let test_adapters = crate::generate::test_dir(root, adapters);
    let main_web = crate::generate::main_dir(root, web);
    let test_web = crate::generate::test_dir(root, web);
    let update = Update {
        target_columns,
        command_columns,
        fields: update_fields,
    };
    let resource = Target {
        name: target.to_string(),
        fields: target_fields,
    };
    Ok(vec![
        Artifact {
            kind: "transition command",
            path: main_service.join(format!("{name}Command.java")),
            // Without `version`: the expected version is a precondition,
            // not data the command carries. plan.md P4.5.
            contents: usecase_command_java(slice, name, &command_fields(fields, key), endpoint),
        },
        Artifact {
            kind: "transition port",
            path: main_service.join(format!("{name}UseCase.java")),
            contents: transition_port_java(slice, name, target, fields, key),
        },
        Artifact {
            kind: "optimistic JDBC transition",
            path: main_adapters.join(format!("Jdbc{name}Transition.java")),
            contents: jdbc_transition_java(slice, name, target, fields, &update, key),
        },
        Artifact {
            kind: "optimistic transition integration test",
            path: test_adapters.join(format!("Jdbc{name}TransitionIT.java")),
            contents: jdbc_transition_it_java(slice, name, &resource, fields, key),
        },
        Artifact {
            kind: "transition controller",
            path: main_web.join(format!("{name}Controller.java")),
            contents: transition_controller_java(slice, name, target, fields, endpoint, key),
        },
        Artifact {
            kind: "transition controller test",
            path: test_web.join(format!("{name}ControllerTest.java")),
            contents: transition_controller_test_java(
                slice, name, &resource, fields, endpoint, key,
            ),
        },
    ])
}

/// How this transition talks about the boundary it enforces.
///
/// Empty where there is no `@scope` field, because there is then no scope in
/// the SQL either. The old wording -- "resource not found in the authorized
/// scope", "scoped matches cannot mutate another tenant's row" -- was printed
/// over `where id = :id` in every project that declared no scope at all.
/// modern.md 5.3, plan.md P4.5.
/// The row this transition selects, and where its value comes from.
///
/// One value because the three are decided together in `transition_files` and
/// read together by every renderer below. It is also what keeps the port to
/// *one* shape: `execute(key, command, expectedVersion)` whether the key
/// arrived in the URL or in the body, so the adapter and the controller cannot
/// disagree about which of the two it was. Two shapes would be the `bugs.md`
/// B48 drift with a bigger surface.
#[derive(Clone, Copy)]
struct Key<'a> {
    /// The component that identifies the row -- `id` unless `--select` named
    /// another.
    component: &'a str,
    /// The Java type the port takes it as: the boxed builtin, because
    /// `Result.NotFound` is a record component and cannot hold a primitive
    /// generically.
    java_type: &'static str,
    /// True when a `--path` variable carries it, so the command record does
    /// not.
    from_path: bool,
}

impl Key<'_> {
    /// How the controller names the value it hands the port.
    fn expression(&self) -> String {
        if self.from_path {
            self.component.to_string()
        } else {
            format!("command.{}()", self.component)
        }
    }
}

fn scope_clause(fields: &[crate::generate::Field]) -> &'static str {
    if fields.iter().any(|field| field.constraints.scoped) {
        " within the caller's authorized scope"
    } else {
        ""
    }
}

/// The version component, which `transition_files` has already proved exists
/// and proved numeric.
fn version_field(fields: &[crate::generate::Field]) -> &crate::generate::Field {
    fields
        .iter()
        .find(|field| field.name == "version")
        .expect("a transition is refused without a numeric version field")
}

/// The command a caller sends: every declared field except the version, which
/// travels as `If-Match`. plan.md P4.5.
///
/// And except the selector, when a `--path` variable carries it: a component
/// bound from two places at once is a component that can disagree with itself,
/// and the URL is the half a router already matched on.
fn command_fields(fields: &[crate::generate::Field], key: Key<'_>) -> Vec<crate::generate::Field> {
    fields
        .iter()
        .filter(|field| field.name != "version")
        .filter(|field| !(key.from_path && field.name == key.component))
        .cloned()
        .collect()
}

/// The Java type the expected version is passed as, and the parser for it.
fn version_type(fields: &[crate::generate::Field]) -> (&'static str, &'static str) {
    match usecase_normalized_type(&version_field(fields).java_type) {
        "int" => ("int", "Integer.parseInt"),
        _ => ("long", "Long.parseLong"),
    }
}

fn transition_port_java(
    slice: &Slice,
    name: &str,
    target: &str,
    fields: &[crate::generate::Field],
    key: Key<'_>,
) -> String {
    let pkg: &str = &slice.placed(Layer::Service);
    let domain: &str = &slice.owned(Layer::Domain);
    let target_import = crate::generate::import_of(pkg, domain, target);
    let (version_type, _) = version_type(fields);
    let id = fields
        .iter()
        .find(|field| field.name == key.component)
        .expect("a transition is refused without its selector field");
    let key_import = crate::generate::builtin_by_java_name(&id.java_type)
        .and_then(|(_, import)| import)
        .map(|import| format!("import {import};\n"))
        .unwrap_or_default();
    crate::template::render(
        crate::template_here!("spring/transition_port_java.java"),
        &[
            ("pkg", pkg),
            ("target_import", &format!("{target_import}{key_import}")),
            ("scope_clause", scope_clause(fields)),
            ("version_type", version_type),
            ("key_type", key.java_type),
            ("id_component", key.component),
            ("name", name),
            ("target", target),
        ],
    )
}

fn jdbc_transition_java(
    slice: &Slice,
    name: &str,
    target: &str,
    fields: &[crate::generate::Field],
    update: &Update,
    key: Key<'_>,
) -> String {
    let pkg: &str = &slice.owned(Layer::Adapters);
    let service: &str = &slice.placed(Layer::Service);
    let domain: &str = &slice.owned(Layer::Domain);
    let target_columns: &[crate::sql::Column] = &update.target_columns;
    let command_columns: &[crate::sql::Column] = &update.command_columns;
    let update_fields: &[&crate::generate::Field] = &update.fields;
    let target_import = crate::generate::import_of(pkg, domain, target);
    let command_import = crate::generate::import_of(pkg, service, &format!("{name}Command"));
    let port_import = crate::generate::import_of(pkg, service, &format!("{name}UseCase"));
    let mut imports = crate::sql::imports(target_columns)
        .into_iter()
        .chain(crate::sql::imports(command_columns))
        .map(|import| format!("import {import};\n"))
        .collect::<String>();
    if target_columns.iter().any(|column| {
        column
            .read
            .as_deref()
            .is_some_and(|read| read.contains("Optional."))
    }) {
        imports.push_str("import java.util.Optional;\n");
    }
    for column in target_columns.iter().chain(command_columns.iter()) {
        if crate::generate::builtin_by_java_name(&column.java_type).is_none() {
            imports.push_str(&crate::generate::import_of(pkg, domain, &column.java_type));
        }
    }
    let maintains_updated_at = target_columns
        .iter()
        .any(|column| column.name == "updated_at" && column.java_type == "Instant")
        && !update_fields.iter().any(|field| field.name == "updatedAt");
    let assignments = update_fields
        .iter()
        .map(|field| {
            let column = crate::sql::snake_case(&field.name);
            format!("{column} = :{column}")
        })
        .chain(maintains_updated_at.then_some("updated_at = current_timestamp".to_string()))
        .chain(std::iter::once("version = version + 1".to_string()))
        .collect::<Vec<_>>()
        .join(",\n                            ");
    let match_fields = fields
        .iter()
        .filter(|field| field.name == key.component || field.constraints.scoped)
        .collect::<Vec<_>>();
    let optimistic_predicates = match_fields
        .iter()
        .map(|field| {
            let column = crate::sql::snake_case(&field.name);
            format!("{column} = :{column}")
        })
        .chain(std::iter::once("version = :version".to_string()))
        .collect::<Vec<_>>()
        .join("\n                          and ");
    let existence_predicates = match_fields
        .iter()
        .map(|field| {
            let column = crate::sql::snake_case(&field.name);
            format!("{column} = :{column}")
        })
        .collect::<Vec<_>>()
        .join("\n                                  and ");
    // The key is bound from the port's own parameter, not off the command --
    // the command does not always carry it. `sql::columns` renders a write
    // expression against a receiver (`command.userId()`, or
    // `Timestamp.from(command.at())` where the receiver sits in the middle),
    // so the substitution is the receiver prefix rather than the whole
    // expression. Naming the parameter after the component is what makes that
    // one replacement enough.
    let receiver = format!("command.{}()", key.component);
    let bindings_for = |selected: &[&crate::generate::Field], indent: &str| {
        selected
            .iter()
            .map(|field| {
                let column = command_columns
                    .iter()
                    .find(|column| column.name == crate::sql::snake_case(&field.name))
                    .expect("validated transition column");
                let write = column.write.as_deref().expect("mapped transition column");
                let write = if field.name == key.component {
                    write.replacen(&receiver, key.component, 1)
                } else {
                    write.to_string()
                };
                format!("{indent}.param(\"{}\", {write})", column.name)
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    // Every field except the version, which is bound from the separate
    // `expectedVersion` parameter -- it is no longer a component of the
    // command to read it off. plan.md P4.5.
    let bound = fields
        .iter()
        .filter(|field| field.name != "version")
        .collect::<Vec<_>>();
    let update_bindings = bindings_for(&bound, "                ");
    let existence_bindings = bindings_for(&match_fields, "                ");
    let select = target_columns
        .iter()
        .map(|column| column.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let map_args = target_columns
        .iter()
        .map(|column| format!("                {}", column.read.as_deref().unwrap()))
        .collect::<Vec<_>>()
        .join(",\n");
    let table = crate::sql::table_name(target);
    crate::template::render(
        crate::template_here!("spring/jdbc_transition_java.java"),
        &[
            ("pkg", pkg),
            ("target_import", &*target_import),
            ("command_import", &*command_import),
            ("port_import", &*port_import),
            ("imports", &*imports),
            ("name", name),
            ("target", target),
            ("table", &*table),
            ("assignments", &*assignments),
            ("optimistic_predicates", &*optimistic_predicates),
            ("select", &*select),
            ("update_bindings", &*update_bindings),
            ("existence_predicates", &*existence_predicates),
            ("existence_bindings", &*existence_bindings),
            ("scope_clause", scope_clause(fields)),
            ("version_type", version_type(fields).0),
            ("id_component", key.component),
            ("key_type", key.java_type),
            ("map_args", &*map_args),
        ],
    )
}

fn transition_controller_java(
    slice: &Slice,
    name: &str,
    target: &str,
    fields: &[crate::generate::Field],
    endpoint: Endpoint<'_>,
    key: Key<'_>,
) -> String {
    let security: &str = slice.base();
    let service: &str = &slice.placed(Layer::Service);
    let web: &str = &slice.placed(Layer::Web);
    let command_import = crate::generate::import_of(web, service, &format!("{name}Command"));
    let usecase_import = crate::generate::import_of(web, service, &format!("{name}UseCase"));
    let (
        scope_import,
        scope_field,
        scope_constructor,
        scope_assignment,
        scope_parameter,
        scope_checks,
    ) = scope_controller_parts(security, web, fields, "command");
    // The contract when the caller names one, the derived shape when they do
    // not -- the same rule `usecase` and `query` follow. A transition already
    // took `--consumes`, so it could say *how* it binds and not *where* it
    // answers; a frontend calling `PATCH /admin_api/conversations/{id}/status`
    // is a fixed contract, and deriving `/actions/set-status` for it is the
    // `missing.md` M8 shape one recipe at a time.
    let path = endpoint.route.map(str::to_string).unwrap_or_else(|| {
        format!(
            "/actions/{}",
            crate::sql::snake_case(name).replace('_', "-")
        )
    });
    // PUT by default, because that is what every transition emitted before
    // `--method` reached this recipe and a compare-and-swap update is
    // idempotent. PATCH is the other legitimate spelling for "set one field on
    // this row", and a frontend that calls one will not accept the other.
    let method = endpoint.method;
    let (failure_imports, arms) = outcome_arms(slice, web, name, target);
    let (version_type, parse) = version_type(fields);
    // Mounted *and* bound, or neither. `bugs.md` B48 is the half-built
    // version: a variable in the `@RequestMapping` that no parameter reads,
    // which fails at the URI before the verb or the body can matter.
    let (key_parameter, path_variable_import) = if key.from_path {
        (
            format!(
                "            @PathVariable {} {},\n",
                key.java_type, key.component
            ),
            "import org.springframework.web.bind.annotation.PathVariable;\n",
        )
    } else {
        (String::new(), "")
    };
    crate::template::render(
        crate::template_here!("spring/transition_controller_java.java"),
        &[
            ("key_parameter", &*key_parameter),
            ("path_variable_import", path_variable_import),
            ("key_expression", &key.expression()),
            (
                "validation",
                crate::spring::validation_package(slice.project()),
            ),
            ("failure_imports", &*failure_imports),
            ("arms", &*arms),
            ("web", web),
            ("command_import", &*command_import),
            ("usecase_import", &*usecase_import),
            ("target_import", ""),
            ("scope_import", &*scope_import),
            ("version_type", version_type),
            ("parse", parse),
            ("name", name),
            ("path", &*path),
            ("mapping", method.mapping()),
            ("scope_field", &*scope_field),
            ("scope_constructor", &*scope_constructor),
            ("scope_assignment", &*scope_assignment),
            ("target", target),
            ("scope_parameter", &*scope_parameter),
            ("scope_checks", &*scope_checks),
            ("binding", endpoint.binding()),
            ("binding_import", endpoint.binding_import()),
        ],
    )
}

/// How this controller turns each sealed outcome into a response.
///
/// **The applied arm always returns**, with the new version as an `ETag`, so
/// a caller's next `If-Match` is a value this endpoint issued.
///
/// The other two depend on the project's error model, and that is deliberate.
/// `add api` installs a sealed `ApiException`, an exhaustive handler and RFC
/// 9457 responses -- and **nothing threw it, in 0 of 7 real projects**, while
/// the one operation with real failure modes hand-rolled its own statuses. So
/// where the class exists the transition raises into it, and the caller reads
/// the current version from a follow-up `GET`. Without it there is no class to
/// raise, and the response carries the stored row and its `ETag` directly --
/// the same rule `repository_wiring` follows for `JdbcClient`.
///
/// Both messages carry values. `backend.md` §1: *"Exception messages carry the
/// values."* The string these replace was `"resource not found in the
/// authorized scope"` -- the same text for every 404 the service would ever
/// serve, naming neither the resource nor the id, over SQL with no scope in
/// it at all.
fn outcome_arms(slice: &Slice, web: &str, name: &str, target: &str) -> (String, String) {
    let applied = format!(
        "            case {name}UseCase.Result.Applied(var resource) ->\n                    \
         ResponseEntity.ok()\n                            \
         .eTag(String.valueOf(resource.version()))\n                            \
         .body({target}Response.from(resource));"
    );
    // `owned`, not `placed`: this is where `add api` put its class, which is a
    // different question from where this transition's own classes go.
    let api: &str = &slice.owned(Layer::Api);
    if slice.project().declares_type("ApiException") {
        return (
            crate::generate::import_of(web, api, "ApiException"),
            format!(
                "{applied}\n            \
                 case {name}UseCase.Result.StaleVersion(var current) ->\n                    \
                 throw new ApiException.Conflict(\n                            \
                 \"expected version \" + expected + \", stored version is \"\n                                    \
                 + current.version());\n            \
                 case {name}UseCase.Result.NotFound(var id) ->\n                    \
                 throw new ApiException.NotFound(\"no such {target}: \" + id);"
            ),
        );
    }
    (
        String::new(),
        format!(
            "{applied}\n            \
             case {name}UseCase.Result.StaleVersion(var current) ->\n                    \
             ResponseEntity.status(HttpStatus.PRECONDITION_FAILED)\n                            \
             .eTag(String.valueOf(current.version()))\n                            \
             .body({target}Response.from(current));\n            \
             case {name}UseCase.Result.NotFound(var id) -> ResponseEntity.notFound().build();"
        ),
    )
}
