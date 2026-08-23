//! `usecase`, with its transactional outbox half -- and the shape its two
//! siblings share.
//!
//! Each of `usecase`, `transition` (`transition.rs`) and `query` (`query.rs`)
//! takes a `--on <Resource>` that already exists, reads that record off disk
//! through `Target::read`, checks the fields it was given against it, and emits
//! a command, a port, an adapter, a route and tests. That shape is the thing
//! worth reading once, and `usecase` is where it is written down.
//!
//! They were one file until `abstract.md` rung 11: three kinds is three
//! secrets, and 1,374 production lines made this the largest module in the
//! repository. What they genuinely share -- `require_scope_authorizer`,
//! `scope_test_parts`, `Target`, `Projection` -- stays reachable from here.
//!
//! `event` deliberately stayed behind: it is a messaging concern these three
//! merely *reference* through `--yields`.

use super::*;

// ---------------------------------------------------------------------------
// `generate usecase` -- an executable create operation over a scaffold.
// ---------------------------------------------------------------------------

pub fn require_scope_authorizer(
    slice: &Slice,
    kind: &str,
    name: &str,
    fields: &[crate::generate::Field],
) -> jails_support::Result<()> {
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

pub(super) fn scope_test_parts(
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
        ",\n            new ScopeAuthorizer(new MockEnvironment())".to_string(),
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
pub struct Target {
    /// The resource's class name.
    pub name: String,
    /// Its record components, read off disk.
    pub fields: Vec<crate::generate::Field>,
}

impl Target {
    /// Read the resource, or refuse naming the command that creates it.
    ///
    /// plan.md §9.4 asks for one rule for where fields come from, stated once:
    /// the record on disk, else an error naming the record *and the fix*.
    /// `usecase`, `query` and `transition` each used to raise their own
    /// wording, and only some of them carried a `fix:` line.
    pub fn read(
        slice: &Slice,
        kind: &str,
        name: &str,
        target: &str,
    ) -> jails_support::Result<Self> {
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
    pub fn id(&self, kind: &str, name: &str) -> jails_support::Result<&crate::generate::Field> {
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

pub fn usecase_files(
    slice: &Slice,
    name: &str,
    target: &str,
    fields: &[crate::generate::Field],
) -> jails_support::Result<Vec<Artifact>> {
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

pub(super) fn usecase_command_java(
    slice: &Slice,
    name: &str,
    fields: &[crate::generate::Field],
) -> String {
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
        crate::template_here!("spring/usecase_port_java.java"),
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
        crate::template_here!("spring/usecase_controller_java.java"),
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

pub(super) fn json_sample(slice: &Slice, field: &crate::generate::Field) -> Option<String> {
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
    let (scope_import, scope_argument) = scope_test_parts(security, web, fields);
    crate::template::render(
        crate::template_here!("spring/usecase_controller_test_java.java"),
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
            ("json", &*json),
            ("target", target),
            ("target_args", &*target_args),
            ("scope_argument", &*scope_argument),
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

// ---------------------------------------------------------------------------

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
