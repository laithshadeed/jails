//! The transactional outbox half of `g usecase --yields`.
//!
//! One row written in the same transaction as the work, and a separate reader
//! that delivers it. That is the only shape surviving the two failures
//! everything else has: publishing *inside* the transaction sends an event for
//! work that then rolls back, and publishing *after* it commits loses the event
//! whenever the process dies in between.
//!
//! Split out of `durable.rs` under `abstract.md` rung 11 -- the two share a
//! store shape but not a secret. A durable job is *work jails runs*; an outbox
//! row is *a fact jails has to tell somebody else*.

use super::*;

/// Attach a generated use case to a typed event through a transactional
/// PostgreSQL outbox. `usecase --yields Event` is deliberately composition,
/// not a second domain-specific workflow language: the event's components
/// must come from the command/result or one safe timestamp default.
pub(crate) fn outbox_files(
    slice: &Slice,
    usecase: &str,
    target: &str,
    event: &str,
    command_fields: &[crate::generate::Field],
) -> jails_support::Result<Vec<Artifact>> {
    let root: &Path = slice.project().root();
    let service: &str = &slice.placed(Layer::Service);
    let adapters: &str = &slice.owned(Layer::Adapters);
    let messaging: &str = &slice.owned(Layer::Messaging);
    let jobs: &str = &slice.owned(Layer::Jobs);
    let json = crate::generate::main_dir(root, adapters).join("Json.java");
    if !json.exists() {
        return Err(format!(
            "usecase {usecase} --yields {event} needs the generic JSON capability for durable payloads.\n       fix: run `jails add json` first."
        ));
    }
    let event_class = format!("{event}Event");
    let event_fields = crate::generate::fields_from_record(root, messaging, &event_class)
        .ok_or_else(|| {
            format!(
                "usecase {usecase} yields {event}, but {event_class}.java does not exist or is not a record. Generate the typed event first."
            )
        })?;
    let event_id = event_fields
        .iter()
        .find(|field| field.name == "id")
        .ok_or_else(|| format!("outbox event {event_class} needs a stable id"))?;
    if usecase_normalized_type(&event_id.java_type) != "UUID"
        || event_id.optionality == crate::generate::Optionality::Nullable
    {
        return Err(format!(
            "transactional outbox v1 requires {event_class}.id to be a required UUID"
        ));
    }
    let target_fields = Target::read(slice, "usecase", usecase, target)?.fields;
    let mut expressions = Vec::with_capacity(event_fields.len());
    let mut needs_instant = false;
    for event_field in &event_fields {
        if let Some(field) = target_fields
            .iter()
            .find(|candidate| candidate.name == event_field.name)
        {
            ensure_outbox_type(usecase, event_field, field, target)?;
            expressions.push(format!("result.{}()", field.name));
        } else if let Some(field) = command_fields
            .iter()
            .find(|candidate| candidate.name == event_field.name)
        {
            ensure_outbox_type(usecase, event_field, field, "command")?;
            expressions.push(format!("command.{}()", field.name));
        } else if event_field.java_type == "Instant"
            && event_field.optionality != crate::generate::Optionality::Nullable
            && event_field.name.ends_with("At")
        {
            needs_instant = true;
            expressions.push("Instant.now()".to_string());
        } else if event_field.name == format!("{}Id", crate::generate::lower_first(target))
            && let Some(id) = target_fields.iter().find(|f| f.name == "id")
        {
            // `<Target>Id` is the identity of the row this use case just
            // created, referred to by the resource's own name rather than by
            // the component's. It is the same convention every other generic
            // relation already uses -- `association` maps `childField=id`,
            // `durable-job` carries the resource id, and a scaffold declares
            // a parent as `<parent>Id` -- so an event that names it is not
            // asking for an inference jails does not already make.
            //
            // Without this an event has to spell the field `id`, which it
            // cannot: `id` is the event's *own* identity, and the outbox
            // requires it to be a distinct required UUID. That is what App C
            // (`examples/payments-gateway`) refused on.
            ensure_outbox_type(usecase, event_field, id, target)?;
            expressions.push("result.id()".to_string());
        } else {
            return Err(format!(
                "usecase {usecase} cannot derive event field `{}` for {event_class}.\n       \
                 fix: use a component from the command/result, `{}Id` for the created \
                 {target}'s own id, or a required Instant named `...At`.",
                event_field.name,
                crate::generate::lower_first(target)
            ));
        }
    }
    let table = format!("{}_outbox", crate::sql::snake_case(usecase));
    let property = crate::sql::snake_case(usecase).replace('_', "-");
    let migration_dir = root.join("src/main/resources/db/migration");
    let version = crate::generate::next_migration_version(&migration_dir)?;
    let main_service = crate::generate::main_dir(root, service);
    let main_jobs = crate::generate::main_dir(root, jobs);
    let test_jobs = crate::generate::test_dir(root, jobs);
    let emission = Emission {
        expressions,
        needs_instant,
    };
    Ok(vec![
        Artifact {
            kind: "scheduling",
            path: main_jobs.join("SchedulingConfig.java"),
            contents: scheduling_config_java(jobs),
        },
        Artifact {
            kind: "transactional outbox use case",
            path: main_service.join(format!("Outbox{usecase}UseCase.java")),
            contents: outbox_usecase_java(slice, usecase, target, event, &emission),
        },
        Artifact {
            kind: "transactional outbox store",
            path: main_jobs.join(format!("Jdbc{usecase}Outbox.java")),
            contents: outbox_store_java(slice, usecase, event, &table, &property),
        },
        Artifact {
            kind: "transactional outbox sink port",
            path: main_jobs.join(format!("{usecase}OutboxSink.java")),
            contents: outbox_sink_java(jobs, messaging, usecase, event),
        },
        Artifact {
            kind: "Kafka outbox sink",
            path: main_jobs.join(format!("{usecase}KafkaOutboxSink.java")),
            contents: outbox_kafka_sink_java(jobs, messaging, usecase, event),
        },
        Artifact {
            kind: "transactional outbox worker",
            path: main_jobs.join(format!("{usecase}OutboxWorker.java")),
            contents: outbox_worker_java(jobs, usecase, &property),
        },
        Artifact {
            kind: "transactional outbox integration test",
            path: test_jobs.join(format!("{usecase}OutboxIT.java")),
            contents: outbox_it_java(slice, usecase, target, &property, command_fields),
        },
        Artifact {
            kind: "transactional outbox migration",
            path: migration_dir.join(format!("V{version:03}__create_{table}.sql")),
            contents: outbox_migration(&table),
        },
    ])
}

/// How one outbox row's payload is built: an expression per event component,
/// and whether that costs an `Instant` import.
///
/// Computed in one loop and consumed in one renderer, so they travel as one
/// value rather than as the last two of nine positional parameters.
struct Emission {
    expressions: Vec<String>,
    needs_instant: bool,
}

fn ensure_outbox_type(
    usecase: &str,
    event: &crate::generate::Field,
    source: &crate::generate::Field,
    owner: &str,
) -> jails_support::Result<()> {
    if usecase_normalized_type(&event.java_type) != usecase_normalized_type(&source.java_type)
        || (event.optionality == crate::generate::Optionality::Nullable)
            != (source.optionality == crate::generate::Optionality::Nullable)
    {
        return Err(format!(
            "usecase {usecase} cannot map event field `{}` ({}) from {owner} ({})",
            event.name, event.java_type, source.java_type
        ));
    }
    Ok(())
}

fn outbox_usecase_java(
    slice: &Slice,
    usecase: &str,
    target: &str,
    event: &str,
    emission: &Emission,
) -> String {
    let service: &str = &slice.placed(Layer::Service);
    let domain: &str = &slice.owned(Layer::Domain);
    let messaging: &str = &slice.owned(Layer::Messaging);
    let jobs: &str = &slice.owned(Layer::Jobs);
    let expressions: &[String] = &emission.expressions;
    let needs_instant: bool = emission.needs_instant;
    let target_import = crate::generate::import_of(service, domain, target);
    let event_import = crate::generate::import_of(service, messaging, &format!("{event}Event"));
    let store_import = crate::generate::import_of(service, jobs, &format!("Jdbc{usecase}Outbox"));
    let instant_import = if needs_instant {
        "import java.time.Instant;\n"
    } else {
        ""
    };
    let args = expressions
        .iter()
        .map(|expression| format!("                {expression}"))
        .collect::<Vec<_>>()
        .join(",\n");
    crate::template::render(
        crate::template_here!("spring/outbox_usecase_java.java"),
        &[
            ("service", service),
            ("target_import", &*target_import),
            ("event_import", &*event_import),
            ("store_import", &*store_import),
            ("instant_import", instant_import),
            ("usecase", usecase),
            ("target", target),
            ("event", event),
            ("args", &*args),
        ],
    )
}

fn outbox_store_java(
    slice: &Slice,
    usecase: &str,
    event: &str,
    table: &str,
    property: &str,
) -> String {
    let pkg: &str = &slice.owned(Layer::Jobs);
    let adapters: &str = &slice.owned(Layer::Adapters);
    let messaging: &str = &slice.owned(Layer::Messaging);
    let json_import = crate::generate::import_of(pkg, adapters, "Json");
    let event_import = crate::generate::import_of(pkg, messaging, &format!("{event}Event"));
    crate::template::render(
        crate::template_here!("spring/outbox_store_java.java"),
        &[
            ("pkg", pkg),
            ("json_import", &*json_import),
            ("event_import", &*event_import),
            ("usecase", usecase),
            ("property", property),
            ("event", event),
            ("table", table),
        ],
    )
}

fn outbox_sink_java(pkg: &str, messaging: &str, usecase: &str, event: &str) -> String {
    let event_import = crate::generate::import_of(pkg, messaging, &format!("{event}Event"));
    crate::template::render(
        crate::template_here!("spring/outbox_sink_java.java"),
        &[
            ("pkg", pkg),
            ("event_import", &*event_import),
            ("usecase", usecase),
            ("event", event),
        ],
    )
}

fn outbox_kafka_sink_java(pkg: &str, messaging: &str, usecase: &str, event: &str) -> String {
    let event_import = crate::generate::import_of(pkg, messaging, &format!("{event}Event"));
    let publisher_import = crate::generate::import_of(pkg, messaging, &format!("{event}Publisher"));
    crate::template::render(
        crate::template_here!("spring/outbox_kafka_sink_java.java"),
        &[
            ("pkg", pkg),
            ("event_import", &*event_import),
            ("publisher_import", &*publisher_import),
            ("usecase", usecase),
            ("event", event),
        ],
    )
}

fn outbox_worker_java(pkg: &str, usecase: &str, property: &str) -> String {
    crate::template::render(
        crate::template_here!("spring/outbox_worker_java.java"),
        &[("pkg", pkg), ("usecase", usecase), ("property", property)],
    )
}

fn outbox_it_java(
    slice: &Slice,
    usecase: &str,
    target: &str,
    property: &str,
    fields: &[crate::generate::Field],
) -> String {
    let root: &Path = slice.project().root();
    let pkg: &str = &slice.owned(Layer::Jobs);
    let service: &str = &slice.placed(Layer::Service);
    let domain: &str = &slice.owned(Layer::Domain);
    let app: &str = &slice.owned(Layer::App);
    let base: String = slice.root_package();
    let samples = fields
        .iter()
        .map(|field| crate::generate::sample_value(field, root, domain))
        .collect::<Option<Vec<_>>>();
    let disabled = samples.is_none();
    let args = samples.unwrap_or_default().join(",\n                ");
    let command_import = crate::generate::import_of(pkg, service, &format!("{usecase}Command"));
    let usecase_import = crate::generate::import_of(pkg, service, &format!("{usecase}UseCase"));
    let target_import = crate::generate::import_of(pkg, domain, target);
    let repo_import = crate::generate::import_of(pkg, app, &format!("{target}Repository"));
    let kafka_testcontainers_import =
        crate::generate::import_of(pkg, &base, KAFKA_TESTCONTAINERS_CONFIG);
    let imports = java_literal_imports(fields, domain)
        .into_iter()
        .map(|import| format!("import {import};\n"))
        .collect::<String>();
    let disabled_import = if disabled {
        "import org.junit.jupiter.api.Disabled;\n"
    } else {
        ""
    };
    let annotation = if disabled {
        "@Disabled(\"todo: supply outbox command samples\")\n"
    } else {
        ""
    };
    crate::template::render(
        crate::template_here!("spring/outbox_it_java.java"),
        &[
            ("pkg", pkg),
            ("command_import", &*command_import),
            ("usecase_import", &*usecase_import),
            ("target_import", &*target_import),
            ("repo_import", &*repo_import),
            ("kafka_testcontainers_import", &*kafka_testcontainers_import),
            ("KAFKA_TESTCONTAINERS_CONFIG", KAFKA_TESTCONTAINERS_CONFIG),
            ("imports", &*imports),
            ("disabled_import", disabled_import),
            ("annotation", annotation),
            ("property", property),
            ("usecase", usecase),
            ("target", target),
            ("args", &*args),
            ("usecase_snake", &crate::sql::snake_case(usecase)),
        ],
    )
}

fn outbox_migration(table: &str) -> String {
    format!(
        "-- Transactional outbox: business writes and event staging share one commit.\n\
         create table {table} (\n\
           id uuid primary key,\n\
           payload jsonb not null,\n\
           state text not null check (state in ('PENDING', 'RUNNING', 'SUCCEEDED', 'FAILED')),\n\
           attempts integer not null check (attempts >= 0),\n\
           max_attempts integer not null check (max_attempts > 0),\n\
           next_attempt_at timestamptz not null,\n\
           lease_until timestamptz,\n\
           last_error text,\n\
           created_at timestamptz not null,\n\
           completed_at timestamptz\n\
         );\n\n\
         create index {table}_runnable_idx on {table} (state, next_attempt_at)\n\
           where state in ('PENDING', 'RUNNING');\n"
    )
}
