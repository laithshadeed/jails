//! `g transition`: moving a resource from one state to the next.
//!
//! The sibling of `usecase` (`workflow.rs`, which documents the shape all three
//! share) and `query`. What makes this one its own kind is the state enum: the
//! generated adapter refuses a move the enum does not allow, so the illegal
//! transition fails in the database rather than in a code review.

use super::workflow::{json_sample, scope_test_parts, usecase_command_java};
use super::*;

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
    let id = fields
        .iter()
        .find(|field| field.name == "id")
        .ok_or_else(|| format!("transition {name} needs the target's required `id` field"))?;
    let version = fields
        .iter()
        .find(|field| field.name == "version")
        .ok_or_else(|| format!("transition {name} needs a required numeric `version` field"))?;
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
            contents: usecase_command_java(slice, name, &command_fields(fields)),
        },
        Artifact {
            kind: "transition port",
            path: main_service.join(format!("{name}UseCase.java")),
            contents: transition_port_java(slice, name, target, fields),
        },
        Artifact {
            kind: "optimistic JDBC transition",
            path: main_adapters.join(format!("Jdbc{name}Transition.java")),
            contents: jdbc_transition_java(slice, name, target, fields, &update),
        },
        Artifact {
            kind: "optimistic transition integration test",
            path: test_adapters.join(format!("Jdbc{name}TransitionIT.java")),
            contents: jdbc_transition_it_java(slice, name, &resource, fields),
        },
        Artifact {
            kind: "transition controller",
            path: main_web.join(format!("{name}Controller.java")),
            contents: transition_controller_java(slice, name, target, fields),
        },
        Artifact {
            kind: "transition controller test",
            path: test_web.join(format!("{name}ControllerTest.java")),
            contents: transition_controller_test_java(slice, name, &resource, fields),
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
fn command_fields(fields: &[crate::generate::Field]) -> Vec<crate::generate::Field> {
    fields
        .iter()
        .filter(|field| field.name != "version")
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
) -> String {
    let pkg: &str = &slice.placed(Layer::Service);
    let domain: &str = &slice.owned(Layer::Domain);
    let target_import = crate::generate::import_of(pkg, domain, target);
    let (version_type, _) = version_type(fields);
    let id = fields
        .iter()
        .find(|field| field.name == "id")
        .expect("a transition is refused without an id field");
    let key_type = crate::generate::builtin_by_java_name(&id.java_type)
        .map(|(boxed, _)| boxed)
        .unwrap_or("String");
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
            ("key_type", key_type),
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
        .filter(|field| field.name == "id" || field.constraints.scoped)
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
    let bindings_for = |selected: &[&crate::generate::Field], indent: &str| {
        selected
            .iter()
            .map(|field| {
                let column = command_columns
                    .iter()
                    .find(|column| column.name == crate::sql::snake_case(&field.name))
                    .expect("validated transition column");
                format!(
                    "{indent}.param(\"{}\", {})",
                    column.name,
                    column.write.as_deref().expect("mapped transition column")
                )
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
            ("id_component", "id"),
            ("map_args", &*map_args),
        ],
    )
}

fn transition_controller_java(
    slice: &Slice,
    name: &str,
    target: &str,
    fields: &[crate::generate::Field],
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
    let path = format!(
        "/actions/{}",
        crate::sql::snake_case(name).replace('_', "-")
    );
    let (failure_imports, arms) = outcome_arms(slice, web, name, target);
    let (version_type, parse) = version_type(fields);
    crate::template::render(
        crate::template_here!("spring/transition_controller_java.java"),
        &[
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
            ("scope_field", &*scope_field),
            ("scope_constructor", &*scope_constructor),
            ("scope_assignment", &*scope_assignment),
            ("target", target),
            ("scope_parameter", &*scope_parameter),
            ("scope_checks", &*scope_checks),
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

/// The component a target's database-assigned key lives in, if it has one.
///
/// `None` covers both "no generated key" and "no key jails can see", which
/// are the same answer to the only question the caller asks: may a generated
/// test write this component down as a literal?
fn generated_key_component(
    fields: &[crate::generate::Field],
    project: &crate::model::Project,
    domain: &str,
) -> Option<String> {
    let columns = crate::sql::columns(fields, project, domain, "value");
    crate::sql::generated_key(&columns).map(|column| column.component.clone())
}

fn jdbc_transition_it_java(
    slice: &Slice,
    name: &str,
    resource: &Target,
    fields: &[crate::generate::Field],
) -> String {
    let project = slice.project();
    let pkg: &str = &slice.owned(Layer::Adapters);
    let service: &str = &slice.placed(Layer::Service);
    let domain: &str = &slice.owned(Layer::Domain);
    let app: &str = &slice.owned(Layer::App);
    let target_fields: &[crate::generate::Field] = &resource.fields;
    let target: &str = &resource.name;
    // The scaffolded target's port is typed on its own key, so this test
    // hands it that value rather than a rendering of it. plan.md P3.3.
    let key = crate::generate::key_type_of(target_fields, project, domain);
    let key_argument = crate::generate::key_argument("stored.id()", &key);
    let command_key_argument = crate::generate::key_argument("command.id()", &key);
    let command_samples = fields
        .iter()
        .map(|field| crate::generate::sample_value(field, project, domain))
        .collect::<Option<Vec<_>>>();
    let target_samples = target_fields
        .iter()
        .map(|field| crate::generate::sample_value(field, project, domain))
        .collect::<Option<Vec<_>>>();
    let disabled = command_samples.is_none() || target_samples.is_none();
    let mut command_values = command_samples.unwrap_or_default();
    // The version is no longer a component of the command, so its sample
    // becomes the `expectedVersion` argument instead. plan.md P4.5.
    let expected_version = fields
        .iter()
        .position(|field| field.name == "version")
        .map(|index| command_values.remove(index))
        .unwrap_or_else(|| "1L".to_string());
    let fields: Vec<crate::generate::Field> = command_fields(fields);
    let fields: &[crate::generate::Field] = &fields;
    // A database-assigned key is not a literal this test can predict: the
    // sequence does not roll back with the transaction, so the second run of
    // the suite selects a row that is not there. The saved row knows its own
    // key, so the command is built from that. plan.md P4.2.
    if let Some(component) = generated_key_component(target_fields, project, domain)
        && let Some(index) = fields.iter().position(|field| field.name == component)
    {
        command_values[index] = format!("stored.{component}()");
    }
    let command_args = command_values.join(",\n                ");
    let target_args = target_samples
        .unwrap_or_default()
        .join(",\n                ");
    let wrong_scope_test = fields
        .iter()
        .enumerate()
        .find_map(|(index, field)| {
            field
                .constraints
                .scoped
                .then(|| durable_alternate_sample(field).map(|value| (index, value)))
                .flatten()
        })
        .map_or_else(String::new, |(changed, alternate)| {
            let args = command_values
                .iter()
                .enumerate()
                .map(|(index, value)| {
                    if index == changed {
                        alternate.clone()
                    } else {
                        value.clone()
                    }
                })
                .collect::<Vec<_>>()
                .join(",\n                ");
            format!(
                r#"
    @Test
    void aDifferentPersistedScopeIsNotFoundAndCannotMutateTheRow() {{
        var stored = repository.save(new {target}(
                {target_args}));
        var wrongScope = new {name}Command(
                {args});

        assertThat(useCase.execute(wrongScope, {expected_version}))
                .isInstanceOf({name}UseCase.Result.NotFound.class);
        assertThat(repository.findById({key_argument})).contains(stored);
    }}
"#
            )
        });
    let target_import = crate::generate::import_of(pkg, domain, target);
    let command_import = crate::generate::import_of(pkg, service, &format!("{name}Command"));
    let port_import = crate::generate::import_of(pkg, service, &format!("{name}UseCase"));
    let repository_import = crate::generate::import_of(pkg, app, &format!("{target}Repository"));
    let imports = java_literal_imports(target_fields, domain)
        .into_iter()
        .chain(java_literal_imports(fields, domain))
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .map(|import| format!("import {import};\n"))
        .collect::<String>();
    let disabled_import = if disabled {
        "import org.junit.jupiter.api.Disabled;\n"
    } else {
        ""
    };
    let annotation = if disabled {
        "@Disabled(\"todo: supply transition samples Jails cannot fabricate\")\n"
    } else {
        ""
    };
    crate::template::render(
        crate::template_here!("spring/jdbc_transition_it_java.java"),
        &[
            ("pkg", pkg),
            ("target_import", &*target_import),
            ("command_import", &*command_import),
            ("port_import", &*port_import),
            ("repository_import", &*repository_import),
            ("imports", &*imports),
            ("disabled_import", disabled_import),
            ("annotation", annotation),
            ("name", name),
            ("target", target),
            ("target_args", &*target_args),
            ("command_args", &*command_args),
            ("expected_version", &*expected_version),
            ("key_argument", &*command_key_argument),
            ("wrong_scope_test", &*wrong_scope_test),
        ],
    )
}

fn transition_controller_test_java(
    slice: &Slice,
    name: &str,
    resource: &Target,
    fields: &[crate::generate::Field],
) -> String {
    let project = slice.project();
    let security: &str = slice.base();
    let service: &str = &slice.placed(Layer::Service);
    let web: &str = &slice.placed(Layer::Web);
    let domain: &str = &slice.owned(Layer::Domain);
    let target_fields: &[crate::generate::Field] = &resource.fields;
    let target: &str = &resource.name;
    // The body carries the command, and the command no longer carries the
    // version -- it is the `If-Match` header. plan.md P4.5.
    let command = command_fields(fields);
    let json = command
        .iter()
        .map(|field| {
            json_sample(slice, field).map(|sample| format!("  \"{}\": {sample}", field.name))
        })
        .collect::<Option<Vec<_>>>();
    let target_samples = target_fields
        .iter()
        .map(|field| crate::generate::sample_value(field, project, domain))
        .collect::<Option<Vec<_>>>();
    let disabled = json.is_none() || target_samples.is_none();
    let json = json.unwrap_or_default().join(",\n");
    let target_args = target_samples
        .unwrap_or_default()
        .join(",\n                    ");
    let command_import = crate::generate::import_of(web, service, &format!("{name}Command"));
    let usecase_import = crate::generate::import_of(web, service, &format!("{name}UseCase"));
    let target_import = crate::generate::import_of(web, domain, target);
    let imports = java_literal_imports(target_fields, domain)
        .into_iter()
        .map(|import| format!("import {import};\n"))
        .collect::<String>();
    let disabled_import = if disabled {
        "import org.junit.jupiter.api.Disabled;\n"
    } else {
        ""
    };
    let annotation = if disabled {
        "    @Disabled(\"todo: supply transition samples\")\n"
    } else {
        ""
    };
    let (scope_import, scope_argument) = scope_test_parts(security, web, fields);
    // The fake returns the target built from these samples, so the `ETag` it
    // answers with is that sample's version. Written without the `L` suffix:
    // this is an HTTP header, not Java.
    let sample_version = target_fields
        .iter()
        .find(|field| field.name == "version")
        .and_then(|field| crate::generate::sample_value(field, project, domain))
        .map(|sample| sample.trim_end_matches(['L', 'l']).to_string())
        .unwrap_or_else(|| "1".to_string());
    crate::template::render(
        // No classic form, for the same reason `g query` has none.
        crate::template_here!("spring/transition_controller_test_java.java"),
        &[
            ("web", web),
            ("command_import", &*command_import),
            ("usecase_import", &*usecase_import),
            ("target_import", &*target_import),
            ("scope_import", &*scope_import),
            ("imports", &*imports),
            ("disabled_import", disabled_import),
            ("name", name),
            ("annotation", annotation),
            ("json", &*json),
            ("target", target),
            ("target_args", &*target_args),
            ("sample_version", &*sample_version),
            ("scope_argument", &*scope_argument),
        ],
    )
}
