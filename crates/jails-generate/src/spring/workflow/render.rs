//! What `usecase` writes, as opposed to what a use case *is*.
//!
//! The same cut `query` and `transition` took, and for the same reason: this
//! module decides the Java -- the command record, the port, the storing
//! implementation, the controller and their proofs -- while `workflow.rs`
//! decides what the operation means, which components it fills in and which
//! outcomes it can have. Both halves read the same facts and neither may work
//! one out a second time; `bugs.md` B48 is what happens when they do.

use super::*;

pub(crate) fn usecase_command_java(
    slice: &Slice,
    name: &str,
    fields: &[crate::generate::Field],
    endpoint: Endpoint<'_>,
) -> String {
    let pkg: &str = &slice.placed(Layer::Service);
    let domain: &str = &slice.owned(Layer::Domain);
    let command = format!("{name}Command");
    let mut source = crate::generate::bound_record_java(
        pkg,
        &command,
        fields,
        &endpoint.bindings(slice.project(), fields),
    );
    let mut imports = fields
        .iter()
        .filter(|field| field.owned && domain != pkg)
        .map(|field| format!("import {domain}.{};", field.java_type))
        .collect::<Vec<_>>();
    imports.sort();
    imports.dedup();
    if !imports.is_empty() {
        let package = format!("package {pkg};\n");
        source = source.replacen(&package, &format!("{package}\n{}\n", imports.join("\n")), 1);
        source = jails_java::tidy::normalize_imports(&source);
    }
    source.replace(
        &format!(" * An immutable {command} value."),
        &format!(" * Validated input for the {name} use case."),
    )
}

pub(super) fn usecase_port_java(slice: &Slice, name: &str, target: &str, optional: bool) -> String {
    let pkg: &str = &slice.placed(Layer::Service);
    let domain: &str = &slice.owned(Layer::Domain);
    let target_import = crate::generate::import_of(pkg, domain, target);
    let (returns, optional_import, returns_doc) = if optional {
        (
            format!("Optional<{target}>"),
            "import java.util.Optional;\n\n",
            "    /** Empty when no parent matched the component the caller sent. */\n",
        )
    } else {
        (target.to_string(), "", "")
    };
    crate::template::render(
        crate::template_here!("spring/usecase_port_java.java"),
        &[
            ("pkg", pkg),
            ("target_import", &*target_import),
            ("optional_import", optional_import),
            ("returns", &*returns),
            ("returns_doc", returns_doc),
            ("name", name),
        ],
    )
}

pub(super) fn usecase_impl_java(
    slice: &Slice,
    name: &str,
    target: &str,
    defaults: &Defaults,
    transactional: bool,
) -> String {
    let pkg: &str = &slice.placed(Layer::Service);
    let domain: &str = &slice.owned(Layer::Domain);
    let app: &str = &slice.owned(Layer::App);
    let expressions: &[String] = &defaults.expressions;
    let default_imports: &[String] = &defaults.imports;
    let target_import = crate::generate::import_of(pkg, domain, target);
    let repository_import = crate::generate::import_of(pkg, app, &format!("{target}Repository"));
    let imports = default_imports
        .iter()
        .map(|import| format!("import {import};\n"))
        .collect::<String>();
    let args = expressions
        .iter()
        .map(|expression| format!("                {expression}"))
        .collect::<Vec<_>>()
        .join(",\n");
    let var = crate::generate::lower_first(target);
    let (transaction_import, annotation) = if transactional {
        (
            "import org.springframework.transaction.annotation.Transactional;\n",
            "    @Transactional\n",
        )
    } else {
        ("", "")
    };
    crate::template::render(
        crate::template_here!("spring/usecase_impl_java.java"),
        &[
            ("pkg", pkg),
            ("target_import", &*target_import),
            ("repository_import", &*repository_import),
            ("imports", &*imports),
            ("transaction_import", transaction_import),
            ("name", name),
            ("target", target),
            ("annotation", annotation),
            ("var", &*var),
            ("args", &*args),
            ("preamble", defaults.preamble),
        ],
    )
}

pub(super) fn usecase_test_java(
    slice: &Slice,
    name: &str,
    target: &Target,
    fields: &[crate::generate::Field],
    id: &crate::generate::Field,
) -> String {
    let project = slice.project();
    let pkg: &str = &slice.placed(Layer::Service);
    let domain: &str = &slice.owned(Layer::Domain);
    let adapters: &str = &slice.owned(Layer::Adapters);
    let target_fields: &[crate::generate::Field] = &target.fields;
    let target: &str = &target.name;
    let samples = fields
        .iter()
        .map(|field| crate::generate::sample_value(field, project, domain))
        .collect::<Vec<_>>();
    let missing = fields
        .iter()
        .zip(&samples)
        .filter(|(_, sample)| sample.is_none())
        .map(|(field, _)| field.name.as_str())
        .collect::<Vec<_>>();
    let args = fields
        .iter()
        .zip(samples)
        .map(|(field, sample)| {
            sample.unwrap_or_else(|| format!("null /* TODO: a {} */", field.java_type))
        })
        .collect::<Vec<_>>()
        .join(",\n                ");
    let copied = fields
        .iter()
        .map(|field| {
            format!(
                "        assertThat(created.{}()).isEqualTo(command.{}());",
                field.name, field.name
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let imports = java_literal_imports(fields, domain)
        .into_iter()
        .map(|import| format!("import {import};\n"))
        .collect::<String>();
    let target_import = crate::generate::import_of(pkg, domain, target);
    let adapter_import =
        crate::generate::import_of(pkg, adapters, &format!("InMemory{target}Repository"));
    let disabled_import = if missing.is_empty() {
        ""
    } else {
        "import org.junit.jupiter.api.Disabled;\n"
    };
    let disabled = if missing.is_empty() {
        String::new()
    } else {
        format!(
            "@Disabled(\"todo: supply a sample for {} -- Jails cannot fabricate it\")\n",
            missing.join(", ")
        )
    };
    // What "the key was assigned" looks like depends on who assigned it. A
    // primitive `long` is never null, so `isNotNull` on one asserts nothing;
    // an identity column starts at one, so a positive value is the honest
    // claim. plan.md P4.2 / missing.md M3.
    let id_assertion = match id.java_type.as_str() {
        "String" => "        assertThat(created.id()).isNotBlank();",
        "int" | "Integer" | "long" | "Long" => "        assertThat(created.id()).isPositive();",
        _ => "        assertThat(created.id()).isNotNull();",
    };
    // The scaffolded target's port is typed on its own key. plan.md P3.3.
    let key_argument = crate::generate::key_argument(
        "created.id()",
        &crate::generate::key_type_of(target_fields, project, domain),
    );
    // **The case missing.md M3 says would have caught it.** Every generated
    // test inserted exactly one row, so a create that hands the same key to
    // the store every time looked identical to one that assigns a fresh one:
    // two creates, one row, the first silently gone.
    //
    // Only where the *use case* assigns the key. A command that carries it is
    // `Assignment::ClientSupplied`, and there two identical commands are one
    // row on purpose -- asserting two would turn a correct idempotent create
    // into a red build.
    let two_creates_test = if fields.iter().any(|field| field.name == id.name) {
        String::new()
    } else {
        format!(
            "\n    /**\n\
         \x20    * missing.md M3: two creates are two rows. When the key was\n\
         \x20    * constructed rather than assigned, this was two creates and\n\
         \x20    * *one* row, with no exception and no log line.\n\
         \x20    */\n\
         \x20   @Test\n\
         \x20   void twoCreatesAreTwoRows() {{\n\
         \x20       {name}Command command = new {name}Command(\n\
         \x20               {args});\n\
         \n\
         \x20       {target} first = useCase.execute(command);\n\
         \x20       {target} second = useCase.execute(command);\n\
         \n\
         \x20       assertThat(second.id()).isNotEqualTo(first.id());\n\
         \x20       assertThat(repository.findAll()).hasSize(2);\n\
         \x20   }}\n"
        )
    };
    crate::template::render(
        crate::template_here!("spring/usecase_test_java.java"),
        &[
            ("pkg", pkg),
            ("target_import", &*target_import),
            ("adapter_import", &*adapter_import),
            ("imports", &*imports),
            ("disabled_import", disabled_import),
            ("disabled", &*disabled),
            ("name", name),
            ("target", target),
            ("args", &*args),
            ("id_assertion", id_assertion),
            ("key_argument", &*key_argument),
            ("two_creates_test", &*two_creates_test),
            ("copied", &*copied),
        ],
    )
}

pub(super) fn usecase_controller_java(
    slice: &Slice,
    target: &str,
    name: &str,
    fields: &[crate::generate::Field],
    answering: (Endpoint<'_>, bool),
) -> String {
    let (endpoint, optional) = answering;
    let route = endpoint.route;
    let security: &str = slice.base();
    let service: &str = &slice.placed(Layer::Service);
    let web: &str = &slice.placed(Layer::Web);
    let command_import = crate::generate::import_of(web, service, &format!("{name}Command"));
    let usecase_import = crate::generate::import_of(web, service, &format!("{name}UseCase"));
    // The contract when the caller names one, the derived shape when they do
    // not. `missing.md` M8.
    let path = route.map(str::to_string).unwrap_or_else(|| {
        format!(
            "/actions/{}",
            crate::sql::snake_case(name).replace('_', "-")
        )
    });
    let resource_path = format!("/{}", crate::sql::table_name(target).replace('_', "-"));
    let (
        scope_import,
        scope_field,
        scope_constructor,
        scope_assignment,
        scope_parameter,
        scope_checks,
    ) = scope_controller_parts(security, web, fields, "command");
    crate::template::render(
        crate::template_here!("spring/usecase_controller_java.java"),
        &[
            (
                "validation",
                crate::spring::validation_package(slice.project()),
            ),
            ("web", web),
            ("command_import", &*command_import),
            ("usecase_import", &*usecase_import),
            ("scope_import", &*scope_import),
            ("name", name),
            ("path", &*path),
            ("resource_path", &*resource_path),
            ("scope_field", &*scope_field),
            ("scope_constructor", &*scope_constructor),
            ("scope_assignment", &*scope_assignment),
            ("target", target),
            ("scope_parameter", &*scope_parameter),
            ("scope_checks", &*scope_checks),
            ("binding", endpoint.binding()),
            ("binding_import", endpoint.binding_import()),
            ("outcome", &*outcome(target, optional)),
        ],
    )
}

/// What the controller does with what the port returned.
///
/// 404 rather than 201 when the key could not be resolved: the caller named a
/// parent that is not there, which is a fact about their request rather than a
/// failure of this one.
fn outcome(target: &str, optional: bool) -> String {
    if optional {
        format!(
            "        return useCase.execute(command)\n\
             \x20               .map(created -> ResponseEntity.created(\n\
             \x20                               URI.create(RESOURCE_PATH + \"/\" + created.id()))\n\
             \x20                       .body({target}Response.from(created)))\n\
             \x20               .orElseGet(() -> ResponseEntity.notFound().build());"
        )
    } else {
        format!(
            "        var created = useCase.execute(command);\n\
             \x20       return ResponseEntity.created(URI.create(RESOURCE_PATH + \"/\" + created.id()))\n\
             \x20               .body({target}Response.from(created));"
        )
    }
}

pub(crate) fn json_sample(slice: &Slice, field: &crate::generate::Field) -> Option<String> {
    let project = slice.project();
    let domain: &str = &slice.owned(Layer::Domain);
    if field.optionality == crate::generate::Optionality::Nullable {
        return Some("null".to_string());
    }
    if field.collection {
        return Some(if field.java_type.starts_with("Map") {
            "{}".to_string()
        } else {
            "[]".to_string()
        });
    }
    let quoted = match field.java_type.as_str() {
        "String" => Some("sample".to_string()),
        "UUID" => Some("00000000-0000-0000-0000-000000000001".to_string()),
        "Instant" => Some("2024-01-01T00:00:00Z".to_string()),
        "LocalDate" => Some("2024-01-01".to_string()),
        "LocalDateTime" => Some("2024-01-01T00:00:00".to_string()),
        "Duration" => Some("PT1M".to_string()),
        "URI" => Some("https://example.test/items/1".to_string()),
        "Path" => Some("/tmp/example".to_string()),
        "ZoneId" => Some("UTC".to_string()),
        // Two the field vocabulary accepts and this table had no arm for, so a
        // use-case over a `currency` or `bytes` component produced a request
        // body with the field silently missing. `pending.md` §1.3.
        "Currency" => Some("GBP".to_string()),
        "byte[]" => Some("amFpbHM=".to_string()),
        // The *wire* value, not the constant: an enum declared
        // `OPEN=open` serialises and deserialises as `open`, so a request
        // carrying `OPEN` is rejected by the converter jails generated
        // alongside it. `first_enum_wire_value` falls back to the constant
        // when the enum has no `@JsonValue`, which is the common case and
        // why this read the wrong one for so long.
        owned if field.owned => crate::generate::first_enum_wire_value(project, domain, owned),
        _ => None,
    };
    if let Some(value) = quoted {
        return Some(format!("\"{value}\""));
    }
    match field.java_type.as_str() {
        "int" | "Integer" => Some("7".to_string()),
        "long" | "Long" => Some("7".to_string()),
        "double" | "Double" | "float" | "Float" | "BigDecimal" => Some("12.5".to_string()),
        "boolean" | "Boolean" => Some("true".to_string()),
        _ => None,
    }
}

pub(super) fn usecase_controller_test_java(
    slice: &Slice,
    name: &str,
    target: &Target,
    fields: &[crate::generate::Field],
    answering: (Endpoint<'_>, bool),
) -> String {
    let (endpoint, optional) = answering;
    let project = slice.project();
    let security: &str = slice.base();
    let service: &str = &slice.placed(Layer::Service);
    let web: &str = &slice.placed(Layer::Web);
    let domain: &str = &slice.owned(Layer::Domain);
    let target_fields: &[crate::generate::Field] = &target.fields;
    let target: &str = &target.name;
    let samples = fields
        .iter()
        .map(|field| json_sample(slice, field).map(|sample| (field.name.clone(), sample)))
        .collect::<Option<Vec<_>>>();
    let target_samples = target_fields
        .iter()
        .map(|field| crate::generate::sample_value(field, project, domain))
        .collect::<Option<Vec<_>>>();
    let disabled_reason = if samples.is_none() {
        Some("Jails cannot serialize one of the command field samples")
    } else if target_samples.is_none() {
        Some("Jails cannot construct the target resource sample")
    } else {
        None
    };
    // How this test sends the command has to be how the controller reads it.
    // It was not: a `--consumes form` use case got a proof that posted JSON at
    // an `@ModelAttribute` parameter, so every component arrived null and the
    // request was answered 400. `Endpoint` owns the pair now.
    let request = endpoint.request(project, &samples.unwrap_or_default(), "                ");
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
    let (disabled_import, disabled) = match disabled_reason {
        Some(reason) => (
            "import org.junit.jupiter.api.Disabled;\n",
            format!("    @Disabled(\"todo: {reason}\")\n"),
        ),
        None => ("", String::new()),
    };
    // The fake stands in for the port, so it has to have the port's shape.
    let fake_result = if optional {
        format!("Optional.of(new {target}(\n                    {target_args}))")
    } else {
        format!("new {target}(\n                    {target_args})")
    };
    let imports = if optional {
        format!("{imports}import java.util.Optional;\n")
    } else {
        imports
    };
    let (scope_import, scope_argument) = scope_test_parts(security, web, fields);
    crate::template::render(
        crate::spring::mockmvc_template(
            project,
            crate::template_here!("spring/usecase_controller_test_java.java"),
            crate::template_here!("spring/usecase_controller_test_classic_java.java"),
        ),
        &[
            ("web", web),
            ("command_import", &*command_import),
            ("usecase_import", &*usecase_import),
            ("target_import", &*target_import),
            ("scope_import", &*scope_import),
            ("imports", &*imports),
            ("disabled_import", disabled_import),
            ("name", name),
            ("disabled", &*disabled),
            ("request", &*request),
            ("media_type_import", endpoint.media_type_import()),
            ("fake_result", &*fake_result),
            ("scope_argument", &*scope_argument),
        ],
    )
}
