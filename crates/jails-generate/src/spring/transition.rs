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
            contents: usecase_command_java(slice, name, fields),
        },
        Artifact {
            kind: "transition port",
            path: main_service.join(format!("{name}UseCase.java")),
            contents: transition_port_java(slice, name, target),
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

fn transition_port_java(slice: &Slice, name: &str, target: &str) -> String {
    let pkg: &str = &slice.placed(Layer::Service);
    let domain: &str = &slice.owned(Layer::Domain);
    let target_import = crate::generate::import_of(pkg, domain, target);
    crate::template::render(
        crate::template_here!("spring/transition_port_java.java"),
        &[
            ("pkg", pkg),
            ("target_import", &*target_import),
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
    let all = fields.iter().collect::<Vec<_>>();
    let update_bindings = bindings_for(&all, "                ");
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
    let (failure_imports, failure_arms) = failure_mapping(slice, web, name);
    crate::template::render(
        crate::template_here!("spring/transition_controller_java.java"),
        &[
            (
                "validation",
                crate::spring::validation_package(slice.project()),
            ),
            ("failure_imports", &*failure_imports),
            ("failure_arms", &*failure_arms),
            ("web", web),
            ("command_import", &*command_import),
            ("usecase_import", &*usecase_import),
            ("scope_import", &*scope_import),
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

/// How this controller turns the two expected outcomes into a status.
///
/// `add api` installs a sealed `ApiException`, an exhaustive handler with no
/// `default` arm, RFC 9457 `ProblemDetail` responses, and forty lines of
/// Javadoc explaining why the switch is exhaustive -- and **nothing threw it,
/// in 0 of 7 real projects**. Meanwhile the one operation with real failure
/// modes hand-rolled its own status mapping with `ResponseStatusException`,
/// bypassing the whole thing. A reader finds `ApiException`, believes it is
/// the error model, and is wrong.
///
/// So the transition throws into it when it is there. The other branch is not
/// a fallback for tidiness: without `add api` the class does not exist and the
/// generated controller would not compile -- the same rule
/// `repository_wiring` follows for `JdbcClient`.
///
/// Read through the projection rather than off disk, so `jails add api` and
/// `jails g transition` in one manifest apply see each other.
fn failure_mapping(slice: &Slice, web: &str, name: &str) -> (String, String) {
    // `owned`, not `placed`: this is where `add api` put its class, which is a
    // different question from where this transition's own classes go.
    let api: &str = &slice.owned(Layer::Api);
    if !slice.project().declares_type("ApiException") {
        return (
            concat!(
                "import org.springframework.web.server.ResponseStatusException;\n\n",
                "import static org.springframework.http.HttpStatus.CONFLICT;\n",
                "import static org.springframework.http.HttpStatus.NOT_FOUND;\n",
            )
            .to_string(),
            format!(
                "        }} catch ({name}UseCase.NotFoundException missing) {{\n            \
                 throw new ResponseStatusException(NOT_FOUND, missing.getMessage(), missing);\n        \
                 }} catch ({name}UseCase.StaleVersionException stale) {{\n            \
                 throw new ResponseStatusException(CONFLICT, stale.getMessage(), stale);\n"
            ),
        );
    }
    (
        crate::generate::import_of(web, api, "ApiException"),
        format!(
            "        }} catch ({name}UseCase.NotFoundException missing) {{\n            \
             throw new ApiException.NotFound(missing.getMessage());\n        \
             }} catch ({name}UseCase.StaleVersionException stale) {{\n            \
             throw new ApiException.Conflict(stale.getMessage());\n"
        ),
    )
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
    let command_samples = fields
        .iter()
        .map(|field| crate::generate::sample_value(field, project, domain))
        .collect::<Option<Vec<_>>>();
    let target_samples = target_fields
        .iter()
        .map(|field| crate::generate::sample_value(field, project, domain))
        .collect::<Option<Vec<_>>>();
    let disabled = command_samples.is_none() || target_samples.is_none();
    let command_values = command_samples.unwrap_or_default();
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
        var stored = new {target}(
                {target_args});
        repository.save(stored);
        var wrongScope = new {name}Command(
                {args});

        assertThatThrownBy(() -> useCase.execute(wrongScope))
                .isInstanceOf({name}UseCase.NotFoundException.class);
        assertThat(repository.findById(String.valueOf(stored.id()))).contains(stored);
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
    let json = fields
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
            ("scope_argument", &*scope_argument),
        ],
    )
}
