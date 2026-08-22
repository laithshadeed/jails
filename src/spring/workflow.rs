//! The three operation kinds: `usecase` (with its transactional outbox half),
//! `transition` and `query`.
//!
//! One subject, and one shape underneath it. Each takes a `--on <Resource>`
//! that already exists, reads that record off disk through `Target::read`,
//! checks the fields it was given against it, and emits a command, a port, an
//! adapter, a route and tests. They are here together because that shape is
//! the thing worth reading once -- and because `spring.rs` holding all three
//! plus every capability is the logical cohesion `abstract.md` §3.2 names as
//! the worst module in the repository.
//!
//! `event` deliberately stayed behind: it is a messaging concern these three
//! merely *reference* through `--yields`.

use super::*;

// ---------------------------------------------------------------------------
// `generate usecase` -- an executable create operation over a scaffold.
// ---------------------------------------------------------------------------

pub(crate) fn require_scope_authorizer(
    slice: &Slice,
    kind: &str,
    name: &str,
    fields: &[crate::generate::Field],
) -> crate::Result<()> {
    if !fields.iter().any(|field| field.constraints.scoped) {
        return Ok(());
    }
    let guard = crate::generate::main_dir(slice.project().root(), slice.base())
        .join("ScopeAuthorizer.java");
    if !guard.exists() {
        return Err(format!(
            "{kind} {name} uses @scope, but the project has no ScopeAuthorizer.\n       fix: run `jails add security` before generating scoped HTTP operations."
        ));
    }
    Ok(())
}

fn scope_test_parts(
    security: &str,
    web: &str,
    fields: &[crate::generate::Field],
) -> (String, String) {
    if !fields.iter().any(|field| field.constraints.scoped) {
        return (String::new(), String::new());
    }
    (
        format!(
            "{}import org.springframework.mock.env.MockEnvironment;\n",
            crate::generate::import_of(web, security, "ScopeAuthorizer")
        ),
        r#"
        @Bean
        ScopeAuthorizer scopeAuthorizer() {
            return new ScopeAuthorizer(new MockEnvironment());
        }
"#
        .to_string(),
    )
}

/// Turn a small operation declaration into a complete vertical behavior.
///
/// `fields` are the values a caller supplies; `target` is an existing
/// scaffolded record named by `--on`. Every target component must either be
/// supplied or have one conservative conventional value Jails can prove how
/// to construct (identity, timestamp, empty optional/collection, zero counter,
/// false flag, or the first declared `status` enum constant). Anything else
/// is rejected at generation time rather than becoming a TODO in production
/// code.
/// The already-generated resource an operation names with `--on`.
///
/// abstract.md §4.4 calls this the typed reference an `Option<String>` was
/// standing in for. Reading it is also the one refusal every field-taking
/// operation shares, so it lives here once rather than in each generator with
/// its own wording -- plan.md §9.4's rule, in the only form that cannot drift.
pub(crate) struct Target {
    /// The resource's class name.
    pub(crate) name: String,
    /// Its record components, read off disk.
    pub(crate) fields: Vec<crate::generate::Field>,
}

impl Target {
    /// Read the resource, or refuse naming the command that creates it.
    ///
    /// plan.md §9.4 asks for one rule for where fields come from, stated once:
    /// the record on disk, else an error naming the record *and the fix*.
    /// `usecase`, `query` and `transition` each used to raise their own
    /// wording, and only some of them carried a `fix:` line.
    pub(crate) fn read(slice: &Slice, kind: &str, name: &str, target: &str) -> crate::Result<Self> {
        let fields = slice.record(Layer::Domain, target).ok_or_else(|| {
            format!(
                "{kind} {name} targets {target}, but no record components could be read from {target}.java.\n       fix: generate the {target} scaffold first, or correct `--on {target}`."
            )
        })?;
        Ok(Self {
            name: target.to_string(),
            fields,
        })
    }

    /// The stable non-optional `id` that lets an operation return a resource
    /// location and verify persistence, or a refusal saying why it needs one.
    pub(crate) fn id(&self, kind: &str, name: &str) -> crate::Result<&crate::generate::Field> {
        let target = &self.name;
        self.fields
            .iter()
            .find(|field| {
                field.name == "id" && field.optionality != crate::generate::Optionality::Nullable
            })
            .ok_or_else(|| {
                format!(
                    "{kind} {name} needs {target} to have a stable non-optional `id` component so it can return a resource location and verify persistence"
                )
            })
    }
}

/// What an operation supplies for the target components its command does not
/// carry: one expression per component, and the imports they cost.
///
/// The two are computed together and consumed together; splitting them into
/// two positional parameters is the Data Clump that made `usecase_impl_java`
/// an eight-parameter function.
struct Defaults {
    expressions: Vec<String>,
    imports: Vec<String>,
}

pub(crate) fn usecase_files(
    slice: &Slice,
    name: &str,
    target: &str,
    fields: &[crate::generate::Field],
) -> crate::Result<Vec<Artifact>> {
    require_scope_authorizer(slice, "usecase", name, fields)?;
    let resolved = Target::read(slice, "usecase", name, target)?;
    let target_fields = &resolved.fields;
    let id = resolved.id("usecase", name)?;

    for field in fields {
        let Some(target_field) = target_fields
            .iter()
            .find(|candidate| candidate.name == field.name)
        else {
            return Err(format!(
                "usecase {name} accepts `{}`, but {target} has no component with that name",
                field.name
            ));
        };
        if usecase_normalized_type(&field.java_type)
            != usecase_normalized_type(&target_field.java_type)
            || (field.optionality == crate::generate::Optionality::Nullable)
                != (target_field.optionality == crate::generate::Optionality::Nullable)
        {
            return Err(format!(
                "usecase {name} declares `{}` as {}, but {target} declares it as {}{}",
                field.name,
                usecase_field_type(field),
                target_field.java_type,
                if target_field.optionality == crate::generate::Optionality::Nullable {
                    "?"
                } else {
                    ""
                }
            ));
        }
    }

    let mut expressions = Vec::with_capacity(target_fields.len());
    let mut default_imports = Vec::new();
    for field in target_fields {
        if fields.iter().any(|input| input.name == field.name) {
            expressions.push(format!("command.{}()", field.name));
            continue;
        }
        let Some((expression, imports)) = usecase_default(slice, field) else {
            return Err(format!(
                "usecase {name} cannot safely infer `{}` ({}) for {target}.\n       fix: add `{}:<type>` to the usecase fields; Jails only infers ids, timestamps, status defaults, counters, flags, and empty optional/collection values.",
                field.name, field.java_type, field.name
            ));
        };
        expressions.push(expression);
        default_imports.extend(imports);
    }
    default_imports.sort();
    default_imports.dedup();
    let defaults = Defaults {
        expressions,
        imports: default_imports,
    };

    let transactional = slice.project().has_jdbc();
    let service: &str = &slice.placed(Layer::Service);
    let main_service = slice.project().main_in(service);
    let test_service = slice.project().test_in(service);
    let main_web = slice.main(Layer::Web);
    let test_web = slice.test(Layer::Web);
    Ok(vec![
        Artifact {
            kind: "usecase command",
            path: main_service.join(format!("{name}Command.java")),
            contents: usecase_command_java(slice, name, fields),
        },
        Artifact {
            kind: "usecase port",
            path: main_service.join(format!("{name}UseCase.java")),
            contents: usecase_port_java(slice, name, target),
        },
        Artifact {
            kind: "usecase implementation",
            path: main_service.join(format!("Default{name}UseCase.java")),
            contents: usecase_impl_java(slice, name, target, &defaults, transactional),
        },
        Artifact {
            kind: "usecase test",
            path: test_service.join(format!("{name}UseCaseTest.java")),
            contents: usecase_test_java(slice, name, &resolved, fields, id),
        },
        Artifact {
            kind: "usecase controller",
            path: main_web.join(format!("{name}Controller.java")),
            contents: usecase_controller_java(slice, target, name, fields),
        },
        Artifact {
            kind: "usecase controller test",
            path: test_web.join(format!("{name}ControllerTest.java")),
            contents: usecase_controller_test_java(slice, name, &resolved, fields),
        },
    ])
}

fn usecase_default(slice: &Slice, field: &crate::generate::Field) -> Option<(String, Vec<String>)> {
    let root: &Path = slice.project().root();
    let domain: &str = &slice.owned(Layer::Domain);
    use crate::generate::Optionality;
    if field.optionality == Optionality::Nullable {
        return Some((
            "Optional.empty()".to_string(),
            vec!["java.util.Optional".to_string()],
        ));
    }
    if field.collection {
        let (expression, import) = if field.java_type.starts_with("Map") {
            ("Map.of()", "java.util.Map")
        } else {
            ("List.of()", "java.util.List")
        };
        return Some((expression.to_string(), vec![import.to_string()]));
    }
    match field.java_type.as_str() {
        "UUID" if field.name == "id" => Some((
            "UUID.randomUUID()".to_string(),
            vec!["java.util.UUID".to_string()],
        )),
        "String" if field.name == "id" => Some((
            "UUID.randomUUID().toString()".to_string(),
            vec!["java.util.UUID".to_string()],
        )),
        "Instant" => Some((
            "Instant.now()".to_string(),
            vec!["java.time.Instant".to_string()],
        )),
        "int" | "Integer" => Some(("0".to_string(), Vec::new())),
        "long" | "Long" => Some(("0L".to_string(), Vec::new())),
        "double" | "Double" => Some(("0.0d".to_string(), Vec::new())),
        "float" | "Float" => Some(("0.0f".to_string(), Vec::new())),
        "short" | "Short" => Some(("(short) 0".to_string(), Vec::new())),
        "byte" | "Byte" => Some(("(byte) 0".to_string(), Vec::new())),
        "boolean" | "Boolean" => Some(("false".to_string(), Vec::new())),
        owned if field.owned && field.name == "status" => {
            crate::generate::first_enum_constant(root, domain, owned).map(|_| {
                (
                    format!("{owned}.values()[0]"),
                    vec![format!("{domain}.{owned}")],
                )
            })
        }
        _ => None,
    }
}

fn usecase_command_java(slice: &Slice, name: &str, fields: &[crate::generate::Field]) -> String {
    let pkg: &str = &slice.placed(Layer::Service);
    let domain: &str = &slice.owned(Layer::Domain);
    let command = format!("{name}Command");
    let mut source = crate::generate::record_java(pkg, &command, fields);
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
        source = crate::generate::normalize_imports(&source);
    }
    source.replace(
        &format!(" * An immutable {command} value."),
        &format!(" * Validated input for the {name} use case."),
    )
}

fn usecase_port_java(slice: &Slice, name: &str, target: &str) -> String {
    let pkg: &str = &slice.placed(Layer::Service);
    let domain: &str = &slice.owned(Layer::Domain);
    let target_import = crate::generate::import_of(pkg, domain, target);
    crate::template::render(
        crate::template::template!("spring/usecase_port_java.java"),
        &[
            ("pkg", pkg),
            ("target_import", &*target_import),
            ("name", name),
            ("target", target),
        ],
    )
}

fn usecase_impl_java(
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
        crate::template::template!("spring/usecase_impl_java.java"),
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
        ],
    )
}

fn usecase_test_java(
    slice: &Slice,
    name: &str,
    target: &Target,
    fields: &[crate::generate::Field],
    id: &crate::generate::Field,
) -> String {
    let root: &Path = slice.project().root();
    let pkg: &str = &slice.placed(Layer::Service);
    let domain: &str = &slice.owned(Layer::Domain);
    let adapters: &str = &slice.owned(Layer::Adapters);
    let target_fields: &[crate::generate::Field] = &target.fields;
    let target: &str = &target.name;
    let samples = fields
        .iter()
        .map(|field| crate::generate::sample_value(field, root, domain))
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
    let id_assertion = if id.java_type == "String" {
        "        assertThat(created.id()).isNotBlank();"
    } else {
        "        assertThat(created.id()).isNotNull();"
    };
    let _ = target_fields;
    crate::template::render(
        crate::template::template!("spring/usecase_test_java.java"),
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
            ("copied", &*copied),
        ],
    )
}

fn usecase_controller_java(
    slice: &Slice,
    target: &str,
    name: &str,
    fields: &[crate::generate::Field],
) -> String {
    let security: &str = slice.base();
    let service: &str = &slice.placed(Layer::Service);
    let web: &str = &slice.placed(Layer::Web);
    let command_import = crate::generate::import_of(web, service, &format!("{name}Command"));
    let usecase_import = crate::generate::import_of(web, service, &format!("{name}UseCase"));
    let path = format!(
        "/actions/{}",
        crate::sql::snake_case(name).replace('_', "-")
    );
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
        crate::template::template!("spring/usecase_controller_java.java"),
        &[
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
        ],
    )
}

fn json_sample(slice: &Slice, field: &crate::generate::Field) -> Option<String> {
    let root: &Path = slice.project().root();
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
        owned if field.owned => crate::generate::first_enum_constant(root, domain, owned),
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

fn usecase_controller_test_java(
    slice: &Slice,
    name: &str,
    target: &Target,
    fields: &[crate::generate::Field],
) -> String {
    let root: &Path = slice.project().root();
    let security: &str = slice.base();
    let service: &str = &slice.placed(Layer::Service);
    let web: &str = &slice.placed(Layer::Web);
    let domain: &str = &slice.owned(Layer::Domain);
    let webmvc_test_import: &str = slice.project().webmvc_test_import();
    let target_fields: &[crate::generate::Field] = &target.fields;
    let target: &str = &target.name;
    let json = fields
        .iter()
        .map(|field| {
            json_sample(slice, field).map(|sample| format!("  \"{}\": {sample}", field.name))
        })
        .collect::<Option<Vec<_>>>();
    let target_samples = target_fields
        .iter()
        .map(|field| crate::generate::sample_value(field, root, domain))
        .collect::<Option<Vec<_>>>();
    let disabled_reason = if json.is_none() {
        Some("Jails cannot serialize one of the command field samples")
    } else if target_samples.is_none() {
        Some("Jails cannot construct the target resource sample")
    } else {
        None
    };
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
    let (disabled_import, disabled) = match disabled_reason {
        Some(reason) => (
            "import org.junit.jupiter.api.Disabled;\n",
            format!("    @Disabled(\"todo: {reason}\")\n"),
        ),
        None => ("", String::new()),
    };
    let (scope_import, scope_bean) = scope_test_parts(security, web, fields);
    crate::template::render(
        crate::template::template!("spring/usecase_controller_test_java.java"),
        &[
            ("web", web),
            ("command_import", &*command_import),
            ("usecase_import", &*usecase_import),
            ("target_import", &*target_import),
            ("scope_import", &*scope_import),
            ("imports", &*imports),
            ("disabled_import", disabled_import),
            ("webmvc_test_import", webmvc_test_import),
            ("name", name),
            ("disabled", &*disabled),
            ("json", &*json),
            ("target", target),
            ("target_args", &*target_args),
            ("scope_bean", &*scope_bean),
        ],
    )
}

#[cfg(test)]
mod usecase_tests {
    use super::*;

    fn scratch(tag: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "jails-usecase-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("src/main/java/com/example/demo/domain")).unwrap();
        // `Project::load` is the one window onto disk these recipes get, so a
        // fixture has to be a real project: a pom to read the flavour and the
        // JDBC starter from, and one source file to resolve the base package.
        std::fs::write(
            root.join("pom.xml"),
            "<project><dependencies><dependency>\
             <groupId>org.springframework.boot</groupId>\
             <artifactId>spring-boot-starter-jdbc</artifactId>\
             </dependency></dependencies></project>",
        )
        .unwrap();
        std::fs::write(
            root.join("src/main/java/com/example/demo/App.java"),
            "package com.example.demo;\npublic final class App {}\n",
        )
        .unwrap();
        root
    }

    fn write_record(root: &Path, name: &str, specs: &[&str]) {
        let fields = crate::generate::parse_fields(
            &specs
                .iter()
                .map(|spec| (*spec).to_string())
                .collect::<Vec<_>>(),
        )
        .unwrap();
        std::fs::write(
            root.join(format!("src/main/java/com/example/demo/domain/{name}.java")),
            crate::generate::record_java("com.example.demo.domain", name, &fields),
        )
        .unwrap();
    }

    #[test]
    fn usecase_derives_only_conservative_defaults_and_persists_the_result() {
        let root = scratch("defaults");
        std::fs::write(
            root.join("pom.xml"),
            r#"<project><dependencies><dependency><groupId>org.springframework.boot</groupId><artifactId>spring-boot-starter-jdbc</artifactId></dependency></dependencies></project>"#,
        )
        .unwrap();
        std::fs::write(
            root.join("src/main/java/com/example/demo/domain/WorkStatus.java"),
            "package com.example.demo.domain;\npublic enum WorkStatus { QUEUED, RUNNING }\n",
        )
        .unwrap();
        write_record(
            &root,
            "WorkItem",
            &[
                "id:uuid",
                "seedUrl:uri",
                "status:WorkStatus",
                "unitsProcessed:long",
                "startedAt:instant?",
            ],
        );
        let fields = crate::generate::parse_fields(&["seedUrl:uri".to_string()]).unwrap();

        let project = Project::load(&root).unwrap();
        let files = usecase_files(
            &Slice::new(&project, None),
            "CreateWorkItem",
            "WorkItem",
            &fields,
        )
        .unwrap();
        let implementation = &files
            .iter()
            .find(|artifact| artifact.kind == "usecase implementation")
            .unwrap()
            .contents;

        assert!(
            implementation.contains("UUID.randomUUID()"),
            "{implementation}"
        );
        assert!(
            implementation.contains("command.seedUrl()"),
            "{implementation}"
        );
        assert!(
            implementation.contains("WorkStatus.values()[0]"),
            "{implementation}"
        );
        assert!(implementation.contains("0L"), "{implementation}");
        assert!(
            implementation.contains("Optional.empty()"),
            "{implementation}"
        );
        assert!(
            implementation.contains("repository.save(workItem)"),
            "{implementation}"
        );
        assert!(
            implementation.contains("@Transactional"),
            "{implementation}"
        );
        assert!(!implementation.contains("final class"), "{implementation}");
        assert!(!implementation.contains("TODO"), "{implementation}");
    }

    #[test]
    fn usecase_refuses_to_invent_a_foreign_identity() {
        let root = scratch("foreign-id");
        write_record(&root, "Membership", &["id:uuid", "workspaceId:uuid"]);

        let project = Project::load(&root).unwrap();
        let error = usecase_files(
            &Slice::new(&project, None),
            "CreateMembership",
            "Membership",
            &[],
        )
        .unwrap_err();

        assert!(
            error.contains("cannot safely infer `workspaceId`"),
            "{error}"
        );
    }

    #[test]
    fn usecase_rejects_input_that_the_target_cannot_store() {
        let root = scratch("unknown-input");
        write_record(&root, "Tenant", &["id:uuid", "name:string"]);
        let fields = crate::generate::parse_fields(&["slug:string".to_string()]).unwrap();

        let project = Project::load(&root).unwrap();
        let error = usecase_files(
            &Slice::new(&project, None),
            "CreateWorkspace",
            "Tenant",
            &fields,
        )
        .unwrap_err();

        assert!(error.contains("Tenant has no component"), "{error}");
    }
}

// ---------------------------------------------------------------------------
// `generate transition` -- scope-safe optimistic updates in PostgreSQL.
// ---------------------------------------------------------------------------

pub(crate) fn transition_files(
    slice: &Slice,
    name: &str,
    target: &str,
    fields: &[crate::generate::Field],
) -> crate::Result<Vec<Artifact>> {
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
        ));
    }
    for field in fields {
        let Some(target_field) = target_fields
            .iter()
            .find(|candidate| candidate.name == field.name)
        else {
            return Err(format!(
                "transition {name} declares `{}`, but {target} has no component with that name",
                field.name
            ));
        };
        if usecase_normalized_type(&field.java_type)
            != usecase_normalized_type(&target_field.java_type)
        {
            return Err(format!(
                "transition {name} declares `{}` as {}, but {target} stores it as {}",
                field.name, field.java_type, target_field.java_type
            ));
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
        ));
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
        ));
    }
    let target_columns = crate::sql::columns(&target_fields, slice.project(), domain, "rows");
    let command_columns = crate::sql::columns(fields, slice.project(), domain, "command");
    if target_columns
        .iter()
        .chain(command_columns.iter())
        .any(|column| !column.mapped())
    {
        return Err(format!(
            "transition {name} contains a field Jails cannot map to JDBC"
        ));
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

fn transition_port_java(slice: &Slice, name: &str, target: &str) -> String {
    let pkg: &str = &slice.placed(Layer::Service);
    let domain: &str = &slice.owned(Layer::Domain);
    let target_import = crate::generate::import_of(pkg, domain, target);
    crate::template::render(
        crate::template::template!("spring/transition_port_java.java"),
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
        crate::template::template!("spring/jdbc_transition_java.java"),
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
    crate::template::render(
        crate::template::template!("spring/transition_controller_java.java"),
        &[
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

fn jdbc_transition_it_java(
    slice: &Slice,
    name: &str,
    resource: &Target,
    fields: &[crate::generate::Field],
) -> String {
    let root: &Path = slice.project().root();
    let pkg: &str = &slice.owned(Layer::Adapters);
    let service: &str = &slice.placed(Layer::Service);
    let domain: &str = &slice.owned(Layer::Domain);
    let app: &str = &slice.owned(Layer::App);
    let target_fields: &[crate::generate::Field] = &resource.fields;
    let target: &str = &resource.name;
    let command_samples = fields
        .iter()
        .map(|field| crate::generate::sample_value(field, root, domain))
        .collect::<Option<Vec<_>>>();
    let target_samples = target_fields
        .iter()
        .map(|field| crate::generate::sample_value(field, root, domain))
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
        crate::template::template!("spring/jdbc_transition_it_java.java"),
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
    let root: &Path = slice.project().root();
    let security: &str = slice.base();
    let service: &str = &slice.placed(Layer::Service);
    let web: &str = &slice.placed(Layer::Web);
    let domain: &str = &slice.owned(Layer::Domain);
    let webmvc_test_import: &str = slice.project().webmvc_test_import();
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
        .map(|field| crate::generate::sample_value(field, root, domain))
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
    let (scope_import, scope_bean) = scope_test_parts(security, web, fields);
    crate::template::render(
        crate::template::template!("spring/transition_controller_test_java.java"),
        &[
            ("web", web),
            ("command_import", &*command_import),
            ("usecase_import", &*usecase_import),
            ("target_import", &*target_import),
            ("scope_import", &*scope_import),
            ("imports", &*imports),
            ("disabled_import", disabled_import),
            ("webmvc_test_import", webmvc_test_import),
            ("name", name),
            ("annotation", annotation),
            ("json", &*json),
            ("target", target),
            ("target_args", &*target_args),
            ("scope_bean", &*scope_bean),
        ],
    )
}

// ---------------------------------------------------------------------------
// `generate query` -- typed equality filters executed by PostgreSQL.
// ---------------------------------------------------------------------------

pub(crate) fn query_files(
    slice: &Slice,
    name: &str,
    target: &str,
    fields: &[crate::generate::Field],
) -> crate::Result<Vec<Artifact>> {
    let root: &Path = slice.project().root();
    let service: &str = &slice.placed(Layer::Service);
    let web: &str = &slice.placed(Layer::Web);
    let domain: &str = &slice.owned(Layer::Domain);
    let adapters: &str = &slice.owned(Layer::Adapters);
    require_scope_authorizer(slice, "query", name, fields)?;
    if fields.is_empty() {
        return Err(format!(
            "query {name} needs at least one typed filter; use the scaffold's list endpoint for an unfiltered read"
        ));
    }
    if let Some(field) = fields.iter().find(|field| {
        field.optionality == crate::generate::Optionality::Nullable || field.collection
    }) {
        return Err(format!(
            "query {name} filter `{}` is optional or a collection. This first query contract only accepts required scalar equality filters so null/list semantics are never guessed.",
            field.name
        ));
    }
    let target_fields = Target::read(slice, "query", name, target)?.fields;
    for field in fields {
        let Some(target_field) = target_fields
            .iter()
            .find(|candidate| candidate.name == field.name)
        else {
            return Err(format!(
                "query {name} filters `{}`, but {target} has no component with that name",
                field.name
            ));
        };
        if usecase_normalized_type(&field.java_type)
            != usecase_normalized_type(&target_field.java_type)
        {
            return Err(format!(
                "query {name} declares `{}` as {}, but {target} stores it as {}",
                field.name, field.java_type, target_field.java_type
            ));
        }
    }
    let target_columns = crate::sql::columns(&target_fields, slice.project(), domain, "row");
    let filter_columns = crate::sql::columns(fields, slice.project(), domain, "query");
    let unmapped = target_columns
        .iter()
        .chain(filter_columns.iter())
        .filter(|column| !column.mapped())
        .map(|column| column.name.as_str())
        .collect::<Vec<_>>();
    if !unmapped.is_empty() {
        return Err(format!(
            "query {name} cannot map database column(s): {}. Model collections/owned values separately or add an explicit mapping before generating the query.",
            unmapped.join(", ")
        ));
    }
    let main_service = crate::generate::main_dir(root, service);
    let main_adapters = crate::generate::main_dir(root, adapters);
    let test_adapters = crate::generate::test_dir(root, adapters);
    let main_web = crate::generate::main_dir(root, web);
    let test_web = crate::generate::test_dir(root, web);
    let resource = Target {
        name: target.to_string(),
        fields: target_fields,
    };
    let projection = Projection {
        target_columns,
        filter_columns,
    };
    Ok(vec![
        Artifact {
            kind: "query input",
            path: main_service.join(format!("{name}Query.java")),
            contents: query_record_java(slice, name, fields),
        },
        Artifact {
            kind: "query port",
            path: main_service.join(format!("{name}QueryPort.java")),
            contents: query_port_java(slice, name, target),
        },
        Artifact {
            kind: "JDBC query adapter",
            path: main_adapters.join(format!("Jdbc{name}Query.java")),
            contents: jdbc_query_java(slice, name, target, &projection),
        },
        Artifact {
            kind: "JDBC query integration test",
            path: test_adapters.join(format!("Jdbc{name}QueryIT.java")),
            contents: jdbc_query_it_java(slice, name, &resource, fields),
        },
        Artifact {
            kind: "query controller",
            path: main_web.join(format!("{name}QueryController.java")),
            contents: query_controller_java(slice, name, target, fields),
        },
        Artifact {
            kind: "query controller test",
            path: test_web.join(format!("{name}QueryControllerTest.java")),
            contents: query_controller_test_java(slice, name, &resource, fields),
        },
    ])
}

/// The two column lists one query reads through: what it selects, and what it
/// filters on. Both are derived from the same field spec in one place, which
/// is what stops a select and a where clause naming different columns.
struct Projection {
    target_columns: Vec<crate::sql::Column>,
    filter_columns: Vec<crate::sql::Column>,
}

fn query_record_java(slice: &Slice, name: &str, fields: &[crate::generate::Field]) -> String {
    let pkg: &str = &slice.placed(Layer::Service);
    let domain: &str = &slice.owned(Layer::Domain);
    let class = format!("{name}Query");
    let mut source = crate::generate::record_java(pkg, &class, fields);
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
        source = crate::generate::normalize_imports(&source);
    }
    source.replace(
        &format!(" * An immutable {class} value."),
        &format!(" * Typed filters for the {name} query."),
    )
}

fn query_port_java(slice: &Slice, name: &str, target: &str) -> String {
    let pkg: &str = &slice.placed(Layer::Service);
    let domain: &str = &slice.owned(Layer::Domain);
    let target_import = crate::generate::import_of(pkg, domain, target);
    crate::template::render(
        crate::template::template!("spring/query_port_java.java"),
        &[
            ("pkg", pkg),
            ("target_import", &*target_import),
            ("name", name),
            ("target", target),
        ],
    )
}

fn jdbc_query_java(slice: &Slice, name: &str, target: &str, projection: &Projection) -> String {
    let pkg: &str = &slice.owned(Layer::Adapters);
    let service: &str = &slice.placed(Layer::Service);
    let domain: &str = &slice.owned(Layer::Domain);
    let target_columns: &[crate::sql::Column] = &projection.target_columns;
    let filter_columns: &[crate::sql::Column] = &projection.filter_columns;
    let target_import = crate::generate::import_of(pkg, domain, target);
    let query_import = crate::generate::import_of(pkg, service, &format!("{name}Query"));
    let port_import = crate::generate::import_of(pkg, service, &format!("{name}QueryPort"));
    let mut imports = crate::sql::imports(target_columns)
        .into_iter()
        .chain(crate::sql::imports(filter_columns))
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
    for column in target_columns.iter().chain(filter_columns.iter()) {
        if crate::generate::builtin_by_java_name(&column.java_type).is_none() {
            imports.push_str(&crate::generate::import_of(pkg, domain, &column.java_type));
        }
    }
    let select = target_columns
        .iter()
        .map(|column| format!("            {},", column.name))
        .collect::<Vec<_>>()
        .join("\n")
        .trim_end_matches(',')
        .to_string();
    let predicates = filter_columns
        .iter()
        .map(|column| format!("{} = :{}", column.name, column.name))
        .collect::<Vec<_>>()
        .join("\n                          and ");
    let bindings = filter_columns
        .iter()
        .map(|column| {
            format!(
                "                .param(\"{}\", {})",
                column.name,
                column.write.as_deref().expect("mapped query column")
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let map_args = target_columns
        .iter()
        .map(|column| {
            format!(
                "                {}",
                column.read.as_deref().expect("mapped target column")
            )
        })
        .collect::<Vec<_>>()
        .join(",\n");
    let table = crate::sql::table_name(target);
    let order = target_columns
        .iter()
        .find(|column| column.name == "id")
        .map(|column| column.name.as_str())
        .unwrap_or(&target_columns[0].name);
    crate::template::render(
        crate::template::template!("spring/jdbc_query_java.java"),
        &[
            ("pkg", pkg),
            ("target_import", &*target_import),
            ("query_import", &*query_import),
            ("port_import", &*port_import),
            ("imports", &*imports),
            ("name", name),
            ("select", &*select),
            ("target", target),
            ("table", &*table),
            ("predicates", &*predicates),
            ("order", order),
            ("bindings", &*bindings),
            ("map_args", &*map_args),
        ],
    )
}

fn jdbc_query_it_java(
    slice: &Slice,
    name: &str,
    resource: &Target,
    fields: &[crate::generate::Field],
) -> String {
    let root: &Path = slice.project().root();
    let pkg: &str = &slice.owned(Layer::Adapters);
    let service: &str = &slice.placed(Layer::Service);
    let domain: &str = &slice.owned(Layer::Domain);
    let app: &str = &slice.owned(Layer::App);
    let target_fields: &[crate::generate::Field] = &resource.fields;
    let target: &str = &resource.name;
    let query_samples = fields
        .iter()
        .map(|field| crate::generate::sample_value(field, root, domain))
        .collect::<Option<Vec<_>>>();
    let target_samples = target_fields
        .iter()
        .map(|field| crate::generate::sample_value(field, root, domain))
        .collect::<Option<Vec<_>>>();
    let disabled = query_samples.is_none() || target_samples.is_none();
    let query_args = query_samples
        .unwrap_or_default()
        .join(",\n                ");
    let target_args = target_samples
        .unwrap_or_default()
        .join(",\n                ");
    let target_import = crate::generate::import_of(pkg, domain, target);
    let query_import = crate::generate::import_of(pkg, service, &format!("{name}Query"));
    let port_import = crate::generate::import_of(pkg, service, &format!("{name}QueryPort"));
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
        "@Disabled(\"todo: supply query/target samples Jails cannot fabricate\")\n"
    } else {
        ""
    };
    crate::template::render(
        crate::template::template!("spring/jdbc_query_it_java.java"),
        &[
            ("pkg", pkg),
            ("target_import", &*target_import),
            ("query_import", &*query_import),
            ("port_import", &*port_import),
            ("repository_import", &*repository_import),
            ("imports", &*imports),
            ("disabled_import", disabled_import),
            ("annotation", annotation),
            ("name", name),
            ("target", target),
            ("target_args", &*target_args),
            ("query_args", &*query_args),
        ],
    )
}

fn query_controller_java(
    slice: &Slice,
    name: &str,
    target: &str,
    fields: &[crate::generate::Field],
) -> String {
    let security: &str = slice.base();
    let service: &str = &slice.placed(Layer::Service);
    let web: &str = &slice.placed(Layer::Web);
    let query_import = crate::generate::import_of(web, service, &format!("{name}Query"));
    let port_import = crate::generate::import_of(web, service, &format!("{name}QueryPort"));
    let path = format!(
        "/queries/{}",
        crate::sql::snake_case(name).replace('_', "-")
    );
    let (
        scope_import,
        scope_field,
        scope_constructor,
        scope_assignment,
        scope_parameter,
        scope_checks,
    ) = scope_controller_parts(security, web, fields, "query");
    crate::template::render(
        crate::template::template!("spring/query_controller_java.java"),
        &[
            ("web", web),
            ("query_import", &*query_import),
            ("port_import", &*port_import),
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

fn query_controller_test_java(
    slice: &Slice,
    name: &str,
    resource: &Target,
    fields: &[crate::generate::Field],
) -> String {
    let root: &Path = slice.project().root();
    let security: &str = slice.base();
    let service: &str = &slice.placed(Layer::Service);
    let web: &str = &slice.placed(Layer::Web);
    let domain: &str = &slice.owned(Layer::Domain);
    let webmvc_test_import: &str = slice.project().webmvc_test_import();
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
        .map(|field| crate::generate::sample_value(field, root, domain))
        .collect::<Option<Vec<_>>>();
    let disabled = json.is_none() || target_samples.is_none();
    let json = json.unwrap_or_default().join(",\n");
    let target_args = target_samples
        .unwrap_or_default()
        .join(",\n                    ");
    let port_import = crate::generate::import_of(web, service, &format!("{name}QueryPort"));
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
        "    @Disabled(\"todo: supply query/target samples Jails cannot fabricate\")\n"
    } else {
        ""
    };
    let (scope_import, scope_bean) = scope_test_parts(security, web, fields);
    crate::template::render(
        crate::template::template!("spring/query_controller_test_java.java"),
        &[
            ("web", web),
            ("port_import", &*port_import),
            ("target_import", &*target_import),
            ("scope_import", &*scope_import),
            ("imports", &*imports),
            ("disabled_import", disabled_import),
            ("webmvc_test_import", webmvc_test_import),
            ("name", name),
            ("annotation", annotation),
            ("json", &*json),
            ("target", target),
            ("target_args", &*target_args),
            ("scope_bean", &*scope_bean),
        ],
    )
}

#[cfg(test)]
mod query_tests {
    use super::*;

    fn scratch(tag: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "jails-query-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("src/main/java/com/example/demo/domain")).unwrap();
        // `Project::load` is the one window onto disk these recipes get, so a
        // fixture has to be a real project: a pom to read the flavour and the
        // JDBC starter from, and one source file to resolve the base package.
        std::fs::write(
            root.join("pom.xml"),
            "<project><dependencies><dependency>\
             <groupId>org.springframework.boot</groupId>\
             <artifactId>spring-boot-starter-jdbc</artifactId>\
             </dependency></dependencies></project>",
        )
        .unwrap();
        std::fs::write(
            root.join("src/main/java/com/example/demo/App.java"),
            "package com.example.demo;\npublic final class App {}\n",
        )
        .unwrap();
        root
    }

    fn write_record(root: &Path, name: &str, specs: &[&str]) {
        let fields = crate::generate::parse_fields(
            &specs
                .iter()
                .map(|spec| (*spec).to_string())
                .collect::<Vec<_>>(),
        )
        .unwrap();
        std::fs::write(
            root.join(format!("src/main/java/com/example/demo/domain/{name}.java")),
            crate::generate::record_java("com.example.demo.domain", name, &fields),
        )
        .unwrap();
    }

    #[test]
    fn query_generates_visible_named_parameter_sql_and_real_database_test() {
        let root = scratch("sql");
        write_record(
            &root,
            "Message",
            &[
                "id:uuid",
                "conversationId:uuid",
                "body:string!",
                "createdAt:instant",
            ],
        );
        let fields = crate::generate::parse_fields(&["conversationId:uuid".to_string()]).unwrap();

        let project = Project::load(&root).unwrap();
        let files = query_files(
            &Slice::new(&project, None),
            "MessagesByConversation",
            "Message",
            &fields,
        )
        .unwrap();
        let adapter = &files
            .iter()
            .find(|artifact| artifact.kind == "JDBC query adapter")
            .unwrap()
            .contents;
        let integration_test = &files
            .iter()
            .find(|artifact| artifact.kind == "JDBC query integration test")
            .unwrap()
            .contents;

        assert!(
            adapter.contains("where conversation_id = :conversation_id"),
            "{adapter}"
        );
        assert!(adapter.contains(".param(\"conversation_id\""), "{adapter}");
        assert!(adapter.contains("order by id"), "{adapter}");
        assert!(adapter.contains("limit :max_results"), "{adapter}");
        assert!(adapter.contains("MAX_RESULTS = 100"), "{adapter}");
        assert!(
            integration_test.contains("repository.save(stored)"),
            "{integration_test}"
        );
        assert!(
            integration_test.contains("contains(stored)"),
            "{integration_test}"
        );
    }

    #[test]
    fn query_rejects_an_unfiltered_read_instead_of_guessing_pagination() {
        let root = scratch("empty");
        write_record(&root, "Contact", &["id:uuid", "workspaceId:uuid"]);

        let project = Project::load(&root).unwrap();
        let error =
            query_files(&Slice::new(&project, None), "Contacts", "Contact", &[]).unwrap_err();

        assert!(error.contains("at least one typed filter"), "{error}");
    }

    #[test]
    fn query_rejects_nullable_filters_instead_of_inventing_null_semantics() {
        let root = scratch("nullable");
        write_record(&root, "Contact", &["id:uuid", "email:string?"]);
        let fields = crate::generate::parse_fields(&["email:string?".to_string()]).unwrap();

        let project = Project::load(&root).unwrap();
        let error = query_files(
            &Slice::new(&project, None),
            "ContactsByEmail",
            "Contact",
            &fields,
        )
        .unwrap_err();

        assert!(
            error.contains("null/list semantics are never guessed"),
            "{error}"
        );
    }
}
