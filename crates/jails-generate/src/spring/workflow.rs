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

mod ensure;
use ensure::{conflict_column, ensuring_usecase_it_java, ensuring_usecase_java};

// ---------------------------------------------------------------------------
// `generate usecase` -- an executable create operation over a scaffold.
// ---------------------------------------------------------------------------

pub(crate) fn require_scope_authorizer(
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
    // The *projection*, not disk. In an aggregate `app apply` the `add
    // security` row that writes `ScopeAuthorizer` and the `g scaffold` row
    // that needs it are one transition, so the file this asks about has not
    // been written when this plans -- and a disk read refuses a manifest
    // whose steps are perfectly well ordered.
    if !slice
        .project()
        .projected_main_sources()
        .contains_key(&guard)
    {
        return Err(format!(
            "{kind} {name} uses @scope, but the project has no ScopeAuthorizer.\n       fix: run `jails add security` before generating scoped HTTP operations."
        ).into());
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
pub(crate) struct Target {
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
        Ok(self.fields
            .iter()
            .find(|field| {
                field.name == "id" && field.optionality != crate::generate::Optionality::Nullable
            })
            .ok_or_else(|| {
                format!(
                    "{kind} {name} needs {target} to have a stable non-optional `id` component so it can return a resource location and verify persistence"
                )
            })?)
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
    /// The lines that go above the constructor call, or empty. Today that is
    /// the single hoisted clock read every timestamp default shares.
    preamble: &'static str,
}

/// How this write differs from a plain insert of what the request carried.
///
/// Three things decided by the caller and read together here: whether a unique
/// constraint turns the create into a get-or-create, which components the
/// endpoint pins rather than the caller supplying, and where and how the route
/// answers. One value because `usecase_files` is where every one of them lands
/// and passing them positionally is the Long Parameter List `Recipe` itself
/// exists to have removed.
#[derive(Clone, Copy)]
pub(crate) struct Written<'a> {
    pub(crate) on_conflict: Option<&'a str>,
    pub(crate) pins: &'a [String],
    pub(crate) endpoint: Endpoint<'a>,
}

pub(crate) fn usecase_files(
    slice: &Slice,
    name: &str,
    target: &str,
    fields: &[crate::generate::Field],
    written: Written<'_>,
) -> jails_support::Result<Vec<Artifact>> {
    let Written {
        on_conflict,
        pins,
        endpoint,
    } = written;
    require_scope_authorizer(slice, "usecase", name, fields)?;
    let resolved = Target::read(slice, "usecase", name, target)?;
    let target_fields = &resolved.fields;
    let id = resolved.id("usecase", name)?;
    let pins = crate::spring::pin::resolve(
        slice,
        crate::spring::Pinning {
            recipe: "usecase",
            name,
            target,
        },
        (target_fields, fields),
        &crate::spring::pin::parse(pins)?,
    )?;

    for field in fields {
        let Some(target_field) = target_fields
            .iter()
            .find(|candidate| candidate.name == field.name)
        else {
            return Err(format!(
                "usecase {name} accepts `{}`, but {target} has no component with that name",
                field.name
            )
            .into());
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
            )
            .into());
        }
    }

    // **The database assigns this key, so the command may not.** Without the
    // refusal the component is accepted, rendered into the record, and then
    // dropped by an insert that omits the identity column -- a create that
    // reads as honouring the caller's id and silently does not.
    // plan.md P4.2.
    let target_columns = crate::sql::columns(
        target_fields,
        slice.project(),
        &slice.owned(Layer::Domain),
        "value",
    );
    if let Some(generated) = crate::sql::generated_key(&target_columns)
        && let Some(named) = fields
            .iter()
            .find(|field| field.name == generated.component)
    {
        return Err(format!(
            "usecase {name} accepts `{}`, and {target}'s key is assigned by the database.\n       \
             fix: drop `{}` from the usecase fields; the created {target} carries the key the \
             insert returned.",
            named.name, named.name
        )
        .into());
    }

    let mut expressions: Vec<String> = Vec::with_capacity(target_fields.len());
    let mut default_imports = Vec::new();
    for field in target_fields {
        if fields.iter().any(|input| input.name == field.name) {
            expressions.push(format!("command.{}()", field.name));
            continue;
        }
        // Before the inferred default, and it has to be: a pinned component
        // is one jails *could* have inferred -- `senderType` is an enum whose
        // first constant is a perfectly good default -- and the whole point of
        // pinning it is that this endpoint writes a particular one.
        if let Some(pin) = pins.iter().find(|pin| pin.component == field.name) {
            expressions.push(pin.expression.clone());
            default_imports.extend(pin.imports.iter().cloned());
            continue;
        }
        let Some((expression, imports)) = usecase_default(slice, field) else {
            return Err(format!(
                "usecase {name} cannot safely infer `{}` ({}) for {target}.\n       fix: add `{}:<type>` to the usecase fields; Jails only infers ids, timestamps, status defaults, counters, flags, and empty optional/collection values.",
                field.name, field.java_type, field.name
            ).into());
        };
        expressions.push(expression);
        default_imports.extend(imports);
    }
    default_imports.sort();
    default_imports.dedup();
    // One clock read for every timestamp this create fills in, and the same
    // explanation the scaffold's `toDomain` already gives -- `modern.md`
    // §13.9 found the two disagreeing about the same record in the same
    // package. Two `Instant.now()` calls in one constructor differ by
    // microseconds, which is enough for a freshly created row to look already
    // edited. plan.md P6.5.
    let preamble = if expressions.iter().filter(|e| *e == "Instant.now()").count() > 1 {
        for expression in &mut expressions {
            if expression == "Instant.now()" {
                "now".clone_into(expression);
            }
        }
        crate::spring::dto::AUDIT_PREAMBLE
    } else {
        ""
    };
    let defaults = Defaults {
        expressions,
        imports: default_imports,
        preamble,
    };

    // Get-or-create is a different statement, not a flag on the same one:
    // `repository.save` cannot express `on conflict do nothing returning`, and
    // select-then-insert leaves the window where two callers both see nothing
    // and both proceed. `g explain idempotency` already describes the exact
    // statement; `missing.md` M6 is that it had no verb.
    let conflict = on_conflict
        .map(|component| {
            conflict_column(
                name,
                target,
                target_fields,
                &target_columns,
                fields,
                component,
            )
        })
        .transpose()?;
    let transactional = slice.project().has_jdbc();
    let service: &str = &slice.placed(Layer::Service);
    let main_service = slice.project().main_in(service);
    let test_service = slice.project().test_in(service);
    let main_web = slice.main(Layer::Web);
    let test_web = slice.test(Layer::Web);
    let mut artifacts = vec![
        Artifact {
            kind: "usecase command",
            path: main_service.join(format!("{name}Command.java")),
            contents: usecase_command_java(slice, name, fields, endpoint),
        },
        Artifact {
            kind: "usecase port",
            path: main_service.join(format!("{name}UseCase.java")),
            contents: usecase_port_java(slice, name, target),
        },
    ];
    if let Some(conflict) = &conflict {
        let adapters = slice.owned(Layer::Adapters);
        artifacts.push(Artifact {
            kind: "get-or-create use case",
            path: crate::generate::main_dir(slice.project().root(), &adapters)
                .join(format!("Ensuring{name}UseCase.java")),
            contents: ensuring_usecase_java(
                slice,
                name,
                target,
                &defaults,
                &target_columns,
                conflict,
            ),
        });
        artifacts.push(Artifact {
            kind: "get-or-create integration test",
            path: crate::generate::test_dir(slice.project().root(), &adapters)
                .join(format!("Ensuring{name}UseCaseIT.java")),
            contents: ensuring_usecase_it_java(slice, name, &resolved, fields, conflict),
        });
    } else {
        artifacts.push(Artifact {
            kind: "usecase implementation",
            path: main_service.join(format!("Storing{name}UseCase.java")),
            contents: usecase_impl_java(slice, name, target, &defaults, transactional),
        });
        artifacts.push(Artifact {
            kind: "usecase test",
            path: test_service.join(format!("{name}UseCaseTest.java")),
            contents: usecase_test_java(slice, name, &resolved, fields, id),
        });
    }
    artifacts.extend([
        Artifact {
            kind: "usecase controller",
            path: main_web.join(format!("{name}Controller.java")),
            contents: usecase_controller_java(slice, target, name, fields, endpoint),
        },
        Artifact {
            kind: "usecase controller test",
            path: test_web.join(format!("{name}ControllerTest.java")),
            contents: usecase_controller_test_java(slice, name, &resolved, fields),
        },
    ]);
    Ok(artifacts)
}

fn usecase_default(slice: &Slice, field: &crate::generate::Field) -> Option<(String, Vec<String>)> {
    let project = slice.project();
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
    // The key is minted through the project's own generator, not
    // `UUID.randomUUID()`: a version 4 key is random, and a random primary
    // key destroys b-tree locality on the table it names. plan.md P4.4.
    let identifier = format!(
        "{}.{}",
        crate::spring::identity::package(slice),
        crate::spring::identity::TIME_ORDERED_UUID
    );
    match field.java_type.as_str() {
        "UUID" | "String" if field.name == "id" => crate::spring::identity::mint(&field.java_type)
            .map(|expression| (expression.to_string(), vec![identifier])),
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
        // The constant by name. `Status.values()[0]` is a default that
        // silently changes meaning when somebody reorders the `g enum` --
        // every create written after that reorder starts storing a different
        // status, and nothing in the diff says so. plan.md P6.5.
        owned if field.owned && field.name == "status" => {
            crate::generate::first_enum_constant(project, domain, owned).map(|constant| {
                (
                    format!("{owned}.{constant}"),
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
    endpoint: Endpoint<'_>,
) -> String {
    let pkg: &str = &slice.placed(Layer::Service);
    let domain: &str = &slice.owned(Layer::Domain);
    let command = format!("{name}Command");
    let mut source = crate::generate::bound_record_java(
        pkg,
        &command,
        fields,
        endpoint.binding_naming(slice.project()),
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
            ("preamble", defaults.preamble),
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

fn usecase_controller_java(
    slice: &Slice,
    target: &str,
    name: &str,
    fields: &[crate::generate::Field],
    endpoint: Endpoint<'_>,
) -> String {
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
        ],
    )
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

fn usecase_controller_test_java(
    slice: &Slice,
    name: &str,
    target: &Target,
    fields: &[crate::generate::Field],
) -> String {
    let project = slice.project();
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
        .map(|field| crate::generate::sample_value(field, project, domain))
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
        let root = jails_support::scratch::ScratchDir::in_temp(&format!("jails-usecase-{tag}"))
            .unwrap()
            .keep();
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

    /// Both audit columns get the same `Instant`, and say so in the same
    /// words the scaffold's `toDomain` uses.
    ///
    /// `modern.md` §13.9: one generator hoisted the clock read and explained
    /// precisely why, and this one called `Instant.now()` once per column --
    /// the same record, the same package, minutes apart. Two `now()` calls in
    /// one constructor differ by microseconds, which is enough for a freshly
    /// created row to look already edited. plan.md P6.5.
    #[test]
    fn one_create_reads_the_clock_once_for_every_timestamp_it_fills_in() {
        let root = scratch("one-clock-read");
        std::fs::write(root.join("pom.xml"), "<project></project>").unwrap();
        write_record(
            &root,
            "Note",
            &[
                "id:uuid",
                "body:string!",
                "createdAt:instant",
                "updatedAt:instant",
            ],
        );
        let fields = crate::generate::parse_fields(&["body:string!".to_string()]).unwrap();

        let project = Project::load(&root).unwrap();
        let files = usecase_files(
            &Slice::new(&project, None),
            "WriteNote",
            "Note",
            &fields,
            Written {
                on_conflict: None,
                pins: &[],
                endpoint: Endpoint::json(),
            },
        )
        .unwrap();
        let implementation = &files
            .iter()
            .find(|artifact| artifact.kind == "usecase implementation")
            .unwrap()
            .contents;

        assert_eq!(
            implementation.matches("Instant.now()").count(),
            1,
            "{implementation}"
        );
        assert_eq!(implementation.matches("\n                now,").count(), 1);
        assert!(
            implementation.contains("\n                now);"),
            "{implementation}"
        );
        assert!(
            implementation.contains(crate::spring::dto::AUDIT_PREAMBLE),
            "{implementation}"
        );
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
            Written {
                on_conflict: None,
                pins: &[],
                endpoint: Endpoint::json(),
            },
        )
        .unwrap();
        let implementation = &files
            .iter()
            .find(|artifact| artifact.kind == "usecase implementation")
            .unwrap()
            .contents;

        // Version 7, through the project's own generator: a random key
        // destroys b-tree locality on the table it names. plan.md P4.4.
        assert!(
            implementation.contains("TimeOrderedUuid.next()"),
            "{implementation}"
        );
        assert!(
            implementation.contains("command.seedUrl()"),
            "{implementation}"
        );
        // The constant by name: `values()[0]` changes meaning when somebody
        // reorders the enum, and nothing in the diff says so. plan.md P6.5.
        assert!(
            implementation.contains("WorkStatus.QUEUED"),
            "{implementation}"
        );
        assert!(!implementation.contains("values()[0]"), "{implementation}");
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
            Written {
                on_conflict: None,
                pins: &[],
                endpoint: Endpoint::json(),
            },
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
            Written {
                on_conflict: None,
                pins: &[],
                endpoint: Endpoint::json(),
            },
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
        let root = jails_support::scratch::ScratchDir::in_temp(&format!("jails-query-{tag}"))
            .unwrap()
            .keep();
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
            None,
            Bounds {
                order_by: None,
                limit: None,
            },
            Endpoint::json(),
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
        // Newest first with the key as the tiebreak: `order by id` over a
        // random UUID is a stable random order. plan.md P4.4.
        assert!(
            adapter.contains("order by created_at desc, id"),
            "{adapter}"
        );
        assert!(adapter.contains("limit :max_results"), "{adapter}");
        assert!(adapter.contains("MAX_RESULTS = 100"), "{adapter}");
        assert!(
            integration_test.contains("stored = repository.save(new Message("),
            "the query test filters on the stored row, not the argument: {integration_test}"
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
        let error = query_files(
            &Slice::new(&project, None),
            "Contacts",
            "Contact",
            &[],
            None,
            Bounds {
                order_by: None,
                limit: None,
            },
            Endpoint::json(),
        )
        .unwrap_err();

        assert!(error.contains("at least one typed filter"), "{error}");
    }

    /// An optional filter means "do not filter on this", which is one answer
    /// and not a guess -- `missing.md` M16. The cast is what makes it work on
    /// PostgreSQL, which rejects a bare `$1 is null` because that position
    /// gives the parameter no type to infer from.
    #[test]
    fn an_optional_filter_is_generated_as_absent_meaning_unfiltered() {
        let root = scratch("nullable");
        write_record(&root, "Contact", &["id:uuid", "email:string?"]);
        let fields = crate::generate::parse_fields(&["email:string?".to_string()]).unwrap();

        let project = Project::load(&root).unwrap();
        let files = query_files(
            &Slice::new(&project, None),
            "ContactsByEmail",
            "Contact",
            &fields,
            None,
            Bounds {
                order_by: None,
                limit: None,
            },
            Endpoint::json(),
        )
        .unwrap();

        let adapter = &files
            .iter()
            .find(|artifact| artifact.kind == "JDBC query adapter")
            .unwrap()
            .contents;
        assert!(
            adapter.contains("(cast(:email as text) is null or email = :email)"),
            "{adapter}"
        );
        assert!(
            adapter.contains(".param(\"email\", criteria.email().orElse(null))"),
            "{adapter}"
        );
    }

    /// A collection still is a guess: `in (...)`, `= any(...)` and "every one
    /// of them" are three different queries.
    #[test]
    fn query_still_refuses_a_collection_filter() {
        let root = scratch("collection");
        write_record(&root, "Contact", &["id:uuid", "email:string"]);
        let fields = crate::generate::parse_fields(&["email:list<string>".to_string()]).unwrap();

        let project = Project::load(&root).unwrap();
        let error = query_files(
            &Slice::new(&project, None),
            "ContactsByEmail",
            "Contact",
            &fields,
            None,
            Bounds {
                order_by: None,
                limit: None,
            },
            Endpoint::json(),
        )
        .unwrap_err();

        assert!(error.contains("is a collection"), "{error}");
    }
}
