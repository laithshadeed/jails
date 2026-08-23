//! Work that happens away from a request: `job` and `durable-job`.
//!
//! The two are a deliberate pair, and the difference is the whole point.
//! `job` is a `@Scheduled` method -- fine until the process dies mid-item, at
//! which point the work is simply gone. `durable-job` puts the queue in a
//! table, so a lease expires instead and the item is picked up again.
//!
//! One trap worth carrying at the top: `spring.task.scheduling.pool.size`
//! defaults to **1**. A single scheduled method that blocks stalls every other
//! scheduled method in the application, which is not obvious from any one
//! generated class.

use super::*;

// ---------------------------------------------------------------------------
// `generate job` -- scheduled work.
// ---------------------------------------------------------------------------

pub fn job_files(slice: &Slice, name: &str) -> Vec<Artifact> {
    let root: &Path = slice.project().root();
    let pkg: &str = &slice.placed(Layer::Jobs);
    let main = crate::generate::main_dir(root, pkg);
    let test = crate::generate::test_dir(root, pkg);
    vec![
        Artifact {
            kind: "job",
            path: main.join(format!("{name}Job.java")),
            contents: job_java(pkg, name),
        },
        Artifact {
            kind: "scheduling",
            path: main.join("SchedulingConfig.java"),
            contents: scheduling_config_java(pkg),
        },
        Artifact {
            kind: "job test",
            path: test.join(format!("{name}JobTest.java")),
            contents: job_test_java(pkg, name),
        },
    ]
}

fn job_java(pkg: &str, name: &str) -> String {
    let property = crate::sql::snake_case(name).replace('_', "-");
    crate::template::render(
        crate::template_here!("spring/job_java.java"),
        &[("pkg", pkg), ("property", &*property), ("name", name)],
    )
}

pub fn scheduling_config_java(pkg: &str) -> String {
    crate::template::render(
        crate::template_here!("spring/scheduling_config_java.java"),
        &[("pkg", pkg)],
    )
}

fn job_test_java(pkg: &str, name: &str) -> String {
    crate::template::render(
        crate::template_here!("spring/job_test_java.java"),
        &[("pkg", pkg), ("name", name)],
    )
}

// ---------------------------------------------------------------------------
// `generate durable-job` -- leased PostgreSQL work composed with a use case.
// ---------------------------------------------------------------------------

/// Test-only defaults shared by every Spring integration-test context.
///
/// Boot's default config-data locations include both `classpath:/` and
/// `classpath:/config/`. Keeping the durable-job overrides in the latter
/// means they supplement the application's main `application.properties`
/// instead of shadowing it, while every bare `@SpringBootTest` receives the
/// same Environment and therefore keeps the same context-cache key.
const DURABLE_JOB_TEST_PROPERTIES: &str = "src/test/resources/config/application.properties";

/// Install one durable job's scheduler/retry limits into the app-wide test
/// property source without touching another job's block or reader-owned
/// properties around it.
pub fn install_durable_job_test_properties(
    project: &crate::model::Project,
    name: &str,
    dry_run: bool,
) -> jails_support::Result<bool> {
    let root = project.root();
    let property = crate::sql::snake_case(name).replace('_', "-");
    let initial_delay = format!("jobs.{property}.initial-delay=PT1H");
    let max_attempts = format!("jobs.{property}.max-attempts=2");
    let expected = format!("{initial_delay}\n{max_attempts}\n");
    let marker = format!("durable-job-{property}");
    let marked = crate::codemod::Marked::new(&marker);
    let path = root.join(DURABLE_JOB_TEST_PROPERTIES);
    let existing = if path.exists() {
        std::fs::read_to_string(&path)
            .map_err(|error| format!("failed to read {}: {error}", path.display()))?
    } else {
        String::new()
    };

    if marked.present_in(&existing) {
        let body = marked.body_in(&existing).ok_or_else(|| {
            format!(
                "{} has an unterminated # jails:{marker} block",
                path.display()
            )
        })?;
        if body != expected {
            return Err(format!(
                "{} has an edited # jails:{marker} block and no longer keeps the generated integration test isolated.\n       fix: restore `{initial_delay}` and `{max_attempts}` inside that block.",
                path.display()
            ));
        }
        return Ok(false);
    }

    if dry_run {
        println!("would set {initial_delay} in {DURABLE_JOB_TEST_PROPERTIES}");
        println!("would set {max_attempts} in {DURABLE_JOB_TEST_PROPERTIES}");
        return Ok(true);
    }

    let block = marked.render(&expected);
    let next = if existing.is_empty() {
        block
    } else if existing.ends_with('\n') {
        format!("{existing}{block}")
    } else {
        format!("{existing}\n{block}")
    };
    crate::apply::put(&path, next)?;
    println!("set {initial_delay}");
    println!("set {max_attempts}");
    Ok(true)
}

/// Remove one durable job's contribution, retaining every other block and
/// every reader-owned property in the shared source.
pub fn uninstall_durable_job_test_properties(
    project: &crate::model::Project,
    name: &str,
    dry_run: bool,
) -> jails_support::Result<bool> {
    let property = crate::sql::snake_case(name).replace('_', "-");
    let marker = format!("durable-job-{property}");
    let path = project.root().join(DURABLE_JOB_TEST_PROPERTIES);
    if !path.exists() {
        return Ok(false);
    }
    let existing = std::fs::read_to_string(&path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    let Some(next) = crate::codemod::Marked::new(&marker).strip_from(&existing) else {
        return Ok(false);
    };

    if dry_run {
        println!("would unset jobs.{property}.* in {DURABLE_JOB_TEST_PROPERTIES}");
        return Ok(true);
    }
    if next.is_empty() {
        jails_support::apply::remove(&path)
            .map_err(|error| format!("failed to remove {}: {error}", path.display()))?;
    } else {
        crate::apply::put(&path, next)?;
    }
    println!("unset jobs.{property}.* in {DURABLE_JOB_TEST_PROPERTIES}");
    Ok(true)
}

/// Generate at-least-once durable execution without teaching Jails a domain.
///
/// The work fields must exactly match an existing generated command and must
/// include its stable UUID `id`. `--yields` names the resource created by the
/// use case. That lets a reclaimed execution observe an already-committed
/// resource and mark the work successful after a crash between the business
/// commit and the queue acknowledgement.
pub fn durable_job_files(
    slice: &Slice,
    name: &str,
    usecase: &str,
    target: &str,
    fields: &[crate::generate::Field],
) -> jails_support::Result<Vec<Artifact>> {
    let root: &Path = slice.project().root();
    let jobs: &str = &slice.placed(Layer::Jobs);
    let web: &str = &slice.owned(Layer::Web);
    require_scope_authorizer(slice, "durable-job", name, fields)?;
    if !slice.project().has_jdbc() {
        return Err(format!(
            "durable-job {name} needs PostgreSQL/JDBC for durable leasing.\n       fix: run `jails add db` before generating it."
        ));
    }
    let id = fields
        .iter()
        .find(|field| field.name == "id")
        .ok_or_else(|| format!("durable-job {name} needs a stable `id:uuid` field"))?;
    if usecase_normalized_type(&id.java_type) != "UUID"
        || id.optionality == crate::generate::Optionality::Nullable
    {
        return Err(format!(
            "durable-job {name} needs required `id:uuid`; it received id:{}",
            id.java_type
        ));
    }
    if let Some(field) = fields.iter().find(|field| {
        field.optionality == crate::generate::Optionality::Nullable || field.collection
    }) {
        return Err(format!(
            "durable-job {name} field `{}` is optional or a collection. Durable payload v1 accepts required scalar JDBC fields so storage and equality are exact.",
            field.name
        ));
    }
    let service: &str = &slice.owned(Layer::Service);
    let domain: &str = &slice.owned(Layer::Domain);
    let command_name = format!("{usecase}Command");
    let command_fields = crate::generate::fields_from_record(root, service, &command_name)
        .ok_or_else(|| {
            format!(
                "durable-job {name} cannot read {command_name}.java. Generate usecase {usecase} first."
            )
        })?;
    if fields.len() != command_fields.len()
        || fields.iter().zip(&command_fields).any(|(work, command)| {
            work.name != command.name
                || usecase_normalized_type(&work.java_type)
                    != usecase_normalized_type(&command.java_type)
                || (work.optionality == crate::generate::Optionality::Nullable)
                    != (command.optionality == crate::generate::Optionality::Nullable)
        })
    {
        let wanted = command_fields
            .iter()
            .map(|field| format!("{}:{}", field.name, usecase_field_type(field)))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!(
            "durable-job {name} fields must exactly match {command_name} in declaration order.\n       expected: {wanted}"
        ));
    }
    let target_fields = Target::read(slice, "durable-job", name, target)?.fields;
    let target_id = target_fields
        .iter()
        .find(|field| field.name == "id")
        .ok_or_else(|| format!("durable-job {name} target {target} has no stable id"))?;
    if usecase_normalized_type(&target_id.java_type) != "UUID" {
        return Err(format!(
            "durable-job {name} v1 needs {target}.id to be UUID so work and effect share one stable identity"
        ));
    }

    let columns = crate::sql::columns(fields, slice.project(), domain, "work");
    let unmapped = columns
        .iter()
        .filter(|column| !column.mapped())
        .map(|column| column.name.as_str())
        .collect::<Vec<_>>();
    if !unmapped.is_empty() {
        return Err(format!(
            "durable-job {name} cannot map payload column(s): {}",
            unmapped.join(", ")
        ));
    }

    let migration_dir = root.join("src/main/resources/db/migration");
    let version = crate::generate::next_migration_version(&migration_dir)?;
    let table = format!("{}_jobs", crate::sql::snake_case(name));
    let main_jobs = crate::generate::main_dir(root, jobs);
    let test_jobs = crate::generate::test_dir(root, jobs);
    let main_web = crate::generate::main_dir(root, web);
    Ok(vec![
        Artifact {
            kind: "scheduling",
            path: main_jobs.join("SchedulingConfig.java"),
            contents: scheduling_config_java(jobs),
        },
        Artifact {
            kind: "durable work payload",
            path: main_jobs.join(format!("{name}Work.java")),
            contents: durable_work_java(slice, name, fields),
        },
        Artifact {
            kind: "durable work queue port",
            path: main_jobs.join(format!("{name}Queue.java")),
            contents: durable_queue_java(jobs, name),
        },
        Artifact {
            kind: "durable PostgreSQL store",
            path: main_jobs.join(format!("Jdbc{name}Store.java")),
            contents: durable_store_java(slice, name, &table, &columns),
        },
        Artifact {
            kind: "durable worker",
            path: main_jobs.join(format!("{name}Worker.java")),
            contents: durable_worker_java(slice, name, usecase, target, fields),
        },
        Artifact {
            kind: "durable job controller",
            path: main_web.join(format!("{name}JobController.java")),
            contents: durable_job_controller_java(slice, name, fields),
        },
        Artifact {
            kind: "durable job integration test",
            path: test_jobs.join(format!("{name}JobIT.java")),
            contents: durable_job_it_java(slice, name, target, &table, fields),
        },
        Artifact {
            kind: "durable job migration",
            path: migration_dir.join(format!("V{version:03}__create_{table}.sql")),
            contents: durable_job_migration(&table, &columns),
        },
    ])
}

fn durable_work_java(slice: &Slice, name: &str, fields: &[crate::generate::Field]) -> String {
    let pkg: &str = &slice.placed(Layer::Jobs);
    let domain: &str = &slice.owned(Layer::Domain);
    let class = format!("{name}Work");
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
        &format!(" * Stable, persistable input for the {name} durable job."),
    )
}

fn durable_queue_java(pkg: &str, name: &str) -> String {
    crate::template::render(
        crate::template_here!("spring/durable_queue_java.java"),
        &[("pkg", pkg), ("name", name)],
    )
}

fn durable_store_java(
    slice: &Slice,
    name: &str,
    table: &str,
    columns: &[crate::sql::Column],
) -> String {
    let pkg: &str = &slice.placed(Layer::Jobs);
    let domain: &str = &slice.owned(Layer::Domain);
    let property = crate::sql::snake_case(name).replace('_', "-");
    let mut imports = crate::sql::imports(columns)
        .into_iter()
        .map(|import| format!("import {import};\n"))
        .collect::<String>();
    for column in columns {
        if crate::generate::builtin_by_java_name(&column.java_type).is_none() {
            imports.push_str(&crate::generate::import_of(pkg, domain, &column.java_type));
        }
    }
    let names = columns
        .iter()
        .map(|column| column.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let placeholders = columns
        .iter()
        .map(|column| format!(":{}", column.name))
        .collect::<Vec<_>>()
        .join(", ");
    let bindings = columns
        .iter()
        .map(|column| {
            format!(
                "                .param(\"{}\", {})",
                column.name,
                column.write.as_deref().expect("mapped durable column")
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let select = columns
        .iter()
        .map(|column| column.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let returning = columns
        .iter()
        .map(|column| format!("jobs.{} as {}", column.name, column.name))
        .collect::<Vec<_>>()
        .join(", ");
    let map_args = columns
        .iter()
        .map(|column| format!("                    {}", column.read.as_deref().unwrap()))
        .collect::<Vec<_>>()
        .join(",\n");
    crate::template::render(
        crate::template_here!("spring/durable_store_java.java"),
        &[
            ("pkg", pkg),
            ("imports", &*imports),
            ("name", name),
            ("property", &*property),
            ("table", table),
            ("names", &*names),
            ("placeholders", &*placeholders),
            ("bindings", &*bindings),
            ("returning", &*returning),
            ("select", &*select),
            ("map_args", &*map_args),
        ],
    )
}

fn durable_worker_java(
    slice: &Slice,
    name: &str,
    usecase: &str,
    target: &str,
    fields: &[crate::generate::Field],
) -> String {
    let pkg: &str = &slice.placed(Layer::Jobs);
    let service: &str = &slice.owned(Layer::Service);
    let app: &str = &slice.owned(Layer::App);
    let command_import = crate::generate::import_of(pkg, service, &format!("{usecase}Command"));
    let usecase_import = crate::generate::import_of(pkg, service, &format!("{usecase}UseCase"));
    let repo_import = crate::generate::import_of(pkg, app, &format!("{target}Repository"));
    let args = fields
        .iter()
        .map(|field| format!("                    work.{}()", field.name))
        .collect::<Vec<_>>()
        .join(",\n");
    let property = crate::sql::snake_case(name).replace('_', "-");
    crate::template::render(
        crate::template_here!("spring/durable_worker_java.java"),
        &[
            ("pkg", pkg),
            ("command_import", &*command_import),
            ("usecase_import", &*usecase_import),
            ("repo_import", &*repo_import),
            ("name", name),
            ("usecase", usecase),
            ("target", target),
            ("property", &*property),
            ("args", &*args),
        ],
    )
}

fn durable_job_controller_java(
    slice: &Slice,
    name: &str,
    fields: &[crate::generate::Field],
) -> String {
    let security: &str = slice.base();
    let jobs: &str = &slice.placed(Layer::Jobs);
    let web: &str = &slice.owned(Layer::Web);
    let queue_import = crate::generate::import_of(web, jobs, &format!("{name}Queue"));
    let work_import = crate::generate::import_of(web, jobs, &format!("{name}Work"));
    let (
        scope_import,
        scope_field,
        scope_constructor,
        scope_assignment,
        scope_parameter,
        scope_checks,
    ) = scope_controller_parts(security, web, fields, "work");
    let path = format!("/jobs/{}", crate::sql::snake_case(name).replace('_', "-"));
    crate::template::render(
        crate::template_here!("spring/durable_job_controller_java.java"),
        &[
            ("web", web),
            ("queue_import", &*queue_import),
            ("work_import", &*work_import),
            ("scope_import", &*scope_import),
            ("name", name),
            ("path", &*path),
            ("scope_field", &*scope_field),
            ("scope_constructor", &*scope_constructor),
            ("scope_assignment", &*scope_assignment),
            ("scope_parameter", &*scope_parameter),
            ("scope_checks", &*scope_checks),
        ],
    )
}

fn durable_job_it_java(
    slice: &Slice,
    name: &str,
    target: &str,
    table: &str,
    fields: &[crate::generate::Field],
) -> String {
    let root: &Path = slice.project().root();
    let pkg: &str = &slice.placed(Layer::Jobs);
    let app: &str = &slice.owned(Layer::App);
    let domain: &str = &slice.owned(Layer::Domain);
    let samples = fields
        .iter()
        .map(|field| crate::generate::sample_value(field, root, domain))
        .collect::<Option<Vec<_>>>();
    let disabled = samples.is_none();
    let args = samples.unwrap_or_default().join(",\n                ");
    let alternate = fields.iter().enumerate().find_map(|(index, field)| {
        (field.name != "id")
            .then(|| durable_alternate_sample(field))
            .flatten()
            .map(|value| (index, value))
    });
    let conflict_test = alternate.map_or_else(String::new, |(changed, alternate)| {
        let alternate_args = fields
            .iter()
            .enumerate()
            .map(|(index, field)| {
                if index == changed {
                    alternate.clone()
                } else {
                    crate::generate::sample_value(field, root, domain).unwrap()
                }
            })
            .collect::<Vec<_>>()
            .join(",\n                ");
        format!(
            r#"
    @Test
    void reusingAnIdWithDifferentPayloadIsAConflict() {{
        var original = new {name}Work(
                {args});
        var conflicting = new {name}Work(
                {alternate_args});

        queue.enqueue(original);

        assertThatThrownBy(() -> queue.enqueue(conflicting))
                .isInstanceOf({name}Queue.IdempotencyConflictException.class);
    }}
"#
        )
    });
    let imports = java_literal_imports(fields, domain)
        .into_iter()
        .map(|import| format!("import {import};\n"))
        .collect::<String>();
    let repository_import = crate::generate::import_of(pkg, app, &format!("{target}Repository"));
    let disabled_import = if disabled {
        "import org.junit.jupiter.api.Disabled;\n"
    } else {
        ""
    };
    let annotation = if disabled {
        "@Disabled(\"todo: supply a durable-work sample Jails cannot fabricate\")\n"
    } else {
        ""
    };
    crate::template::render(
        crate::template_here!("spring/durable_job_it_java.java"),
        &[
            ("pkg", pkg),
            ("repository_import", &*repository_import),
            ("imports", &*imports),
            ("disabled_import", disabled_import),
            ("annotation", annotation),
            ("name", name),
            ("target", target),
            ("args", &*args),
            ("table", table),
            ("conflict_test", &*conflict_test),
        ],
    )
}

pub fn durable_alternate_sample(field: &crate::generate::Field) -> Option<String> {
    match usecase_normalized_type(&field.java_type) {
        "String" => Some("\"different-payload\"".to_string()),
        "UUID" => Some("UUID.fromString(\"00000000-0000-0000-0000-000000000002\")".to_string()),
        "URI" => Some("URI.create(\"https://different.example.test/\")".to_string()),
        "Integer" => Some("2".to_string()),
        "Long" => Some("2L".to_string()),
        "Double" => Some("2.5".to_string()),
        "Boolean" => Some("false".to_string()),
        _ => None,
    }
}

fn durable_job_migration(table: &str, columns: &[crate::sql::Column]) -> String {
    let payload = columns
        .iter()
        .map(|column| format!("  {} {} not null,", column.name, column.sql_type))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "-- Durable, leased, at-least-once work.\n\
         create table {table} (\n\
         {payload}\n\
           state text not null check (state in ('PENDING', 'RUNNING', 'SUCCEEDED', 'FAILED')),\n\
           attempts integer not null check (attempts >= 0),\n\
           max_attempts integer not null check (max_attempts > 0),\n\
           next_attempt_at timestamptz not null,\n\
           lease_until timestamptz,\n\
           last_error text,\n\
           created_at timestamptz not null,\n\
           completed_at timestamptz,\n\
           constraint {table}_pk primary key (id)\n\
         );\n\n\
         create index {table}_runnable_idx\n\
           on {table} (state, next_attempt_at)\n\
           where state in ('PENDING', 'RUNNING');\n"
    )
}

#[cfg(test)]
mod durable_job_test_properties_tests {
    use super::*;

    fn scratch(tag: &str) -> (std::path::PathBuf, crate::model::Project) {
        crate::spring::schema::scratch_project(
            &format!("durable-properties-{tag}"),
            "<project></project>",
        )
    }

    #[test]
    fn durable_jobs_merge_into_one_test_source_without_clobbering() {
        let (root, project) = scratch("merge");
        let path = root.join(DURABLE_JOB_TEST_PROPERTIES);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "reader.owned=kept").unwrap();

        assert!(install_durable_job_test_properties(&project, "EmailSender", false).unwrap());
        assert!(install_durable_job_test_properties(&project, "InvoiceWriter", false).unwrap());
        let once = std::fs::read_to_string(&path).unwrap();
        assert!(
            once.starts_with("reader.owned=kept\n# jails:durable-job-email-sender\n"),
            "{once}"
        );
        assert!(once.contains("jobs.email-sender.initial-delay=PT1H"));
        assert!(once.contains("jobs.email-sender.max-attempts=2"));
        assert!(once.contains("jobs.invoice-writer.initial-delay=PT1H"));
        assert!(once.contains("jobs.invoice-writer.max-attempts=2"));

        assert!(!install_durable_job_test_properties(&project, "EmailSender", false).unwrap());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), once);

        assert!(uninstall_durable_job_test_properties(&project, "EmailSender", false).unwrap());
        let without_email = std::fs::read_to_string(&path).unwrap();
        assert!(without_email.starts_with("reader.owned=kept\n"));
        assert!(!without_email.contains("jobs.email-sender."));
        assert!(without_email.contains("jobs.invoice-writer.initial-delay=PT1H"));
        assert!(without_email.contains("jobs.invoice-writer.max-attempts=2"));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn removing_the_only_job_removes_the_generated_source() {
        let (root, project) = scratch("remove-only");
        let path = root.join(DURABLE_JOB_TEST_PROPERTIES);

        install_durable_job_test_properties(&project, "EmailSender", false).unwrap();
        assert!(path.is_file());
        assert!(uninstall_durable_job_test_properties(&project, "EmailSender", false).unwrap());
        assert!(!path.exists());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn prefix_related_job_names_keep_independent_property_blocks() {
        let (root, project) = scratch("prefixes");
        let path = root.join(DURABLE_JOB_TEST_PROPERTIES);

        install_durable_job_test_properties(&project, "EmailSender", false).unwrap();
        install_durable_job_test_properties(&project, "Email", false).unwrap();
        uninstall_durable_job_test_properties(&project, "Email", false).unwrap();

        let remaining = std::fs::read_to_string(&path).unwrap();
        assert!(!remaining.contains("# jails:durable-job-email\n"));
        assert!(remaining.contains("# jails:durable-job-email-sender\n"));
        assert!(remaining.contains("jobs.email-sender.initial-delay=PT1H"));
        assert!(remaining.contains("jobs.email-sender.max-attempts=2"));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn an_edited_safety_block_is_rejected_instead_of_silently_clobbered() {
        let (root, project) = scratch("edited");
        let path = root.join(DURABLE_JOB_TEST_PROPERTIES);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            "# jails:durable-job-email-sender\n\
             jobs.email-sender.initial-delay=PT0S\n\
             jobs.email-sender.max-attempts=99\n\
             # /jails:durable-job-email-sender\n",
        )
        .unwrap();

        let error =
            install_durable_job_test_properties(&project, "EmailSender", false).unwrap_err();
        assert!(error.contains("no longer keeps the generated integration test isolated"));
        assert!(error.contains("initial-delay=PT1H"));
        assert!(error.contains("max-attempts=2"));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_later_duplicate_cannot_override_the_safety_values() {
        let (root, project) = scratch("duplicate-override");
        let path = root.join(DURABLE_JOB_TEST_PROPERTIES);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            "# jails:durable-job-email-sender\n\
             jobs.email-sender.initial-delay=PT1H\n\
             jobs.email-sender.max-attempts=2\n\
             jobs.email-sender.initial-delay=PT0S\n\
             # /jails:durable-job-email-sender\n",
        )
        .unwrap();

        let error =
            install_durable_job_test_properties(&project, "EmailSender", false).unwrap_err();
        assert!(error.contains("no longer keeps the generated integration test isolated"));

        std::fs::remove_dir_all(root).unwrap();
    }
}
