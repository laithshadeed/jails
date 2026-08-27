//! What jails writes to prove a transition, as opposed to what the transition
//! is.
//!
//! Split from `transition.rs` by secret, the same cut `query.rs` took: that
//! module decides the SQL, the port and the route, and this one decides what a
//! test of them has to look like. Both facts it turns on -- which component
//! selects the row, and whether a `--path` variable carries it -- are resolved
//! once in `Key` and read here rather than worked out a second time.

use super::*;

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

pub(super) fn jdbc_transition_it_java(
    slice: &Slice,
    name: &str,
    resource: &Target,
    fields: &[crate::generate::Field],
    key: Key<'_>,
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
    let target_key = crate::generate::key_type_of(target_fields, project, domain);
    // The saved row, not the command. They coincide only while the selector
    // is `id`: `--select userId` builds a command with no `id()` component at
    // all, and this test stopped compiling on the real project the flag was
    // added for. `findById` wants the primary key either way, and `stored` is
    // the row that was just written -- so it is the one thing here that always
    // knows it.
    let key_argument = crate::generate::key_argument("stored.id()", &target_key);
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
    // The key travels beside the command, so its sample goes with it -- the
    // record has no component to put it in. Indexed against the list with the
    // version already removed, which is what `command_values` now holds.
    if key.from_path
        && let Some(index) = fields
            .iter()
            .filter(|field| field.name != "version")
            .position(|field| field.name == key.component)
    {
        command_values.remove(index);
    }
    let fields: Vec<crate::generate::Field> = command_fields(fields, key);
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
            // The key travels beside the command now, so this test has to hand
            // the port one too. When the scope that is being falsified *is* the
            // selector, the falsified value is the key -- passing the stored
            // row's would contradict the command in the same call.
            let wrong_key = if fields[changed].name == key.component {
                alternate.clone()
            } else {
                format!("stored.{}()", key.component)
            };
            format!(
                r#"
    @Test
    void aDifferentPersistedScopeIsNotFoundAndCannotMutateTheRow() {{
        var stored = repository.save(new {target}(
                {target_args}));
        var wrongScope = new {name}Command(
                {args});

        assertThat(useCase.execute({wrong_key}, wrongScope, {expected_version}))
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
            ("key_argument", &*key_argument),
            // The saved row again, for the same reason: with the key in the
            // path the command has no component to read it from, and with the
            // key in the body the two are the same value anyway.
            ("key_expression", &format!("stored.{}()", key.component)),
            ("wrong_scope_test", &*wrong_scope_test),
        ],
    )
}

pub(super) fn transition_controller_test_java(
    slice: &Slice,
    name: &str,
    resource: &Target,
    fields: &[crate::generate::Field],
    endpoint: Endpoint<'_>,
    key: Key<'_>,
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
    let command = command_fields(fields, key);
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
    // `MockMvcTester.uri(template, vararg)` expands the placeholder. Missing
    // it is `bugs.md` B48's failure exactly -- `IllegalArgumentException: Not
    // enough variable values available to expand` -- so the two move together
    // or not at all.
    let path_arguments = if key.from_path {
        fields
            .iter()
            .find(|field| field.name == key.component)
            .and_then(|field| json_sample(slice, field))
            .map(|sample| format!(", \"{}\"", sample.trim_matches('"')))
            .unwrap_or_default()
    } else {
        String::new()
    };
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
            // `MockMvcTester` names the verb in lower case, and the mapping
            // annotation names it capitalised -- one value, two renderings, so
            // the test cannot exercise a verb the controller does not answer.
            ("verb", endpoint.method.label()),
            ("id_component", key.component),
            ("path_arguments", &*path_arguments),
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
