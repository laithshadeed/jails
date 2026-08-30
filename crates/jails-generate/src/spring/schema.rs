//! Invariants the Java type system cannot state: `association` and
//! `idempotency`.
//!
//! A record says a component is a `UUID`. It cannot say that this UUID is a row
//! in another table, that a pair is unique together, or that an operation
//! carrying this key must not run twice. Those are constraints, and a
//! constraint that lives only in application code is one the database will
//! happily let you violate.
//!
//! Both kinds here answer that by putting the invariant in PostgreSQL and
//! generating the Java that respects it -- a composite key for `association`,
//! and for `idempotency` an `insert ... on conflict do nothing returning` that
//! makes claiming a key a single atomic decision rather than a check followed
//! by a write.

use super::*;

// ---------------------------------------------------------------------------
// `generate association` -- explicit, composite relational invariants.
// ---------------------------------------------------------------------------

pub(crate) fn association_files(
    slice: &Slice,
    name: &str,
    child: &str,
    parent: &str,
    mappings: &[String],
) -> jails_support::Result<Vec<Artifact>> {
    let root: &Path = slice.project().root();
    let domain: &str = &slice.owned(Layer::Domain);
    let adapters: &str = &slice.placed(Layer::Adapters);
    if mappings.is_empty() {
        return Err(format!(
            "association {name} needs at least one childField=parentField mapping"
        )
        .into());
    }
    if !slice.project().has_jdbc() {
        return Err(format!(
            "association {name} needs PostgreSQL/JDBC.\n       fix: run `jails add db` first."
        )
        .into());
    }
    // plan.md §9.4: one rule for where fields come from, and one refusal that
    // carries the fix. These two used to word it themselves, without one.
    let child_fields = Target::read(slice, "association", name, child)?.fields;
    let parent_fields = Target::read(slice, "association", name, parent)?.fields;

    let mut pairs = Vec::new();
    for mapping in mappings {
        let (local_name, parent_name) = mapping.split_once('=').ok_or_else(|| {
            format!("association {name}: mapping `{mapping}` must be childField=parentField")
        })?;
        let local_name = local_name.trim();
        let parent_name = parent_name.trim();
        if local_name.is_empty() || parent_name.is_empty() || parent_name.contains('=') {
            return Err(format!(
                "association {name}: mapping `{mapping}` must contain exactly two field names"
            )
            .into());
        }
        let local = child_fields
            .iter()
            .find(|field| field.name == local_name)
            .ok_or_else(|| format!("association {name}: {child} has no field `{local_name}`"))?;
        let remote = parent_fields
            .iter()
            .find(|field| field.name == parent_name)
            .ok_or_else(|| format!("association {name}: {parent} has no field `{parent_name}`"))?;
        if usecase_normalized_type(&local.java_type) != usecase_normalized_type(&remote.java_type) {
            return Err(format!(
                "association {name}: {child}.{local_name} is {}, but {parent}.{parent_name} is {}",
                local.java_type, remote.java_type
            )
            .into());
        }
        if pairs.iter().any(
            |(existing, _): &(&crate::generate::Field, &crate::generate::Field)| {
                existing.name == local.name
            },
        ) {
            return Err(format!(
                "association {name}: child field `{local_name}` is mapped more than once"
            )
            .into());
        }
        pairs.push((local, remote));
    }

    let child_table = crate::sql::table_name(child);
    let parent_table = crate::sql::table_name(parent);
    let local_columns = pairs
        .iter()
        .map(|(field, _)| crate::sql::snake_case(&field.name))
        .collect::<Vec<_>>();
    let parent_columns = pairs
        .iter()
        .map(|(_, field)| crate::sql::snake_case(&field.name))
        .collect::<Vec<_>>();
    let constraint = format!("{}_{}_fk", child_table, crate::sql::snake_case(name));
    let unique_index = format!(
        "{}_{}_association_key",
        parent_table,
        parent_columns.join("_")
    );
    let migration_dir = "src/main/resources/db/migration";
    let needs_unique_index = !migrations_declare_unique_key(
        slice.project(),
        migration_dir,
        &parent_table,
        &parent_columns,
    );
    for identifier in
        std::iter::once(&constraint).chain(needs_unique_index.then_some(&unique_index))
    {
        if identifier.len() > 63 {
            return Err(format!(
                "association {name} produces PostgreSQL identifier `{identifier}` longer than 63 bytes; use a shorter association name"
            ).into());
        }
    }

    let unique_index_ddl = if needs_unique_index {
        format!(
            "create unique index if not exists {unique_index}\n  on {parent_table} ({});\n\n",
            parent_columns.join(", ")
        )
    } else {
        String::new()
    };
    // **H2 has no `DEFERRABLE`.** It answers `Syntax error ... deferrable
    // [*]initially deferred`, so on that dialect the constraint is checked at
    // the statement and the paragraph explaining the trade would be a false
    // claim about the SQL beside it. Measured against a real H2 2.4.240.
    let deferral = match slice.project().sql_dialect() {
        jails_spec::spec::kind::Dialect::Postgres => (
            "-- `deferrable initially deferred`: the check happens at commit rather\n\
             -- than at the statement, so a transaction may insert a child before its\n\
             -- parent and a batch may load in any order. What it costs is where the\n\
             -- error surfaces -- at `commit`, naming this constraint rather than the\n\
             -- statement that broke it. Say `not deferrable` here if you would rather\n\
             -- pay for insert order and get the statement back.\n\
             --\n",
            "\n  deferrable initially deferred",
        ),
        jails_spec::spec::kind::Dialect::H2 => (
            "-- Checked at the statement: H2 has no `deferrable`, so a child row has\n\
             -- to be inserted after its parent. On PostgreSQL jails defers this to\n\
             -- commit, which is the one behavioural difference between the two.\n\
             --\n",
            "",
        ),
    };
    let migration = format!(
        "{unique_index_ddl}\
         -- Two deliberate choices, stated because neither is the obvious one.\n\
         --\n\
         {}\
         -- `on delete no action`: deleting a parent row is a decision about the\n\
         -- child rows, and jails cannot see enough of this domain to make it.\n\
         -- `cascade` deletes them and `restrict` refuses the delete outright --\n\
         -- note `restrict` is never deferred, so choosing it also gives up the\n\
         -- line above.\n\
         alter table {child_table}\n  add constraint {constraint}\n  foreign key ({}) references {parent_table} ({})\n  on update no action on delete no action{};\n",
        deferral.0,
        local_columns.join(", "),
        parent_columns.join(", "),
        deferral.1
    );

    let child_columns = crate::sql::columns(&child_fields, slice.project(), domain, "value");
    // The probe row omits a `generated always as identity` key, for the same
    // reason the adapter's insert does: PostgreSQL refuses a non-DEFAULT
    // value outright, so naming the column turns this test's own setup into
    // the failure it is meant to detect. plan.md P4.2.
    let generated = crate::sql::generated_key(&child_columns).map(|column| column.name.clone());
    let probed: Vec<&crate::sql::Column> = child_columns
        .iter()
        .filter(|column| Some(&column.name) != generated.as_ref())
        .collect();
    let insert_columns = probed
        .iter()
        .map(|column| column.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let insert_values = probed
        .iter()
        .map(|column| association_sql_literal(column))
        .collect::<Vec<_>>()
        .join(", ");
    let expected_mapping = local_columns
        .iter()
        .zip(&parent_columns)
        .map(|(local, remote)| format!("{local}={remote}"))
        .collect::<Vec<_>>()
        .join(",");
    let test = crate::template::render(
        crate::template_here!("spring/association_it_java.java"),
        &[
            ("pkg", adapters),
            ("name", name),
            ("child_table", child_table.as_str()),
            ("parent_table", parent_table.as_str()),
            ("constraint", constraint.as_str()),
            ("local_columns", local_columns.join(", ").as_str()),
            ("parent_columns", parent_columns.join(", ").as_str()),
            ("expected_mapping", expected_mapping.as_str()),
            ("insert_columns", insert_columns.as_str()),
            ("insert_values", insert_values.as_str()),
        ],
    );

    Ok(vec![
        Artifact {
            kind: "association migration",
            path: crate::generate::migration_file(
                slice.project(),
                &format!("add_{}_association", crate::sql::snake_case(name)),
            )?,
            contents: migration,
        },
        Artifact {
            kind: "association integration test",
            path: crate::generate::test_dir(root, adapters)
                .join(format!("{name}AssociationIT.java")),
            contents: test,
        },
    ])
}

/// Reuse a key already proven by earlier Flyway migrations. This recognizes
/// only SQL shapes Jails itself emits; unfamiliar/user-authored SQL falls back
/// to creating a named unique index rather than making a risky inference.
fn migrations_declare_unique_key(
    project: &Project,
    dir: &str,
    table: &str,
    columns: &[String],
) -> bool {
    let columns = columns.join(", ");
    let primary_key = format!("constraint {table}_pk\n    primary key ({columns})");
    let unique_index_target = format!("on {table} ({columns})");
    let create_table = format!("create table {table} (");

    project.projected_names_in(dir).into_iter().any(|name| {
        if !name.ends_with(".sql") {
            return false;
        }
        let Some(sql) = project.projected_text(&format!("{dir}/{name}")) else {
            return false;
        };
        if sql.contains(&primary_key)
            || sql.split(';').any(|statement| {
                statement.contains("create unique index")
                    && statement.contains(&unique_index_target)
            })
        {
            return true;
        }
        if !columns.contains(',')
            && let Some(body) = sql
                .split_once(&create_table)
                .and_then(|(_, rest)| rest.split_once("\n);").map(|(body, _)| body))
        {
            return body.lines().any(|line| {
                line.split_whitespace().next() == Some(columns.as_str())
                    && line
                        .split_whitespace()
                        .any(|token| token.trim_end_matches(',') == "unique")
            });
        }
        false
    })
}

fn association_sql_literal(column: &crate::sql::Column) -> String {
    // A column with a closed set only takes one of its constants now, so the
    // probe uses one rather than a string that reads well. plan.md P5.1 --
    // and the probe failing on it is the check doing exactly its job.
    if let Some(constant) = column.closed_set.first() {
        return format!("'{constant}'");
    }
    match column.sql_type.as_str() {
        "uuid" => "'90000000-0000-0000-0000-000000000009'::uuid",
        "integer" | "bigint" | "double precision" | "numeric" => "1",
        "boolean" => "false",
        "date" => "date '2026-01-01'",
        "timestamp" => "timestamp '2026-01-01 00:00:00'",
        "timestamptz" => "timestamptz '2026-01-01 00:00:00+00'",
        "bytea" => "'\\x00'::bytea",
        "jsonb" => "'{}'::jsonb",
        _ => "'association-probe'",
    }
    .to_string()
}

// ---------------------------------------------------------------------------
// `generate idempotency` -- at-most-once with a retained result.
// ---------------------------------------------------------------------------

/// A receipt store, the guard that uses it, and the table behind both.
///
/// The gap this closes is narrow and easy to mistake for solved: a `@unique`
/// column on the key already gives one row per key. What it does not give is
/// the **retained result**, so a retry finds the row, fails the insert, and is
/// answered 409 Conflict -- telling a caller that never saw the first response
/// that the work happened, while still withholding what happened. That is the
/// state it was retrying to escape.
///
/// Deliberately domain-blind. Idempotency is an HTTP/API concern, not a
/// payments one: the scope is a string the caller chooses, the request is bytes
/// the caller canonicalises, and the stored result is opaque. Nothing here
/// knows what is being made at most once.
pub(crate) fn idempotency_files(slice: &Slice, name: &str) -> jails_support::Result<Vec<Artifact>> {
    if !slice.project().has_jdbc() {
        return Err(format!(
            "idempotency {name} needs PostgreSQL/JDBC to keep receipts across restarts.\n       \
             fix: run `jails add db` first."
        )
        .into());
    }
    let root: &Path = slice.project().root();
    let domain: &str = &slice.placed(Layer::Domain);
    let app: &str = &slice.placed(Layer::App);
    let adapters: &str = &slice.placed(Layer::Adapters);
    let service: &str = &slice.placed(Layer::Service);

    let table = format!("{}_receipts", crate::sql::snake_case(name));
    let record = format!("{name}Receipt");
    let port = format!("{name}Receipts");

    Ok(vec![
        Artifact {
            kind: "idempotency receipt",
            path: crate::generate::main_dir(root, domain).join(format!("{record}.java")),
            contents: crate::template::render(
                crate::template_here!("spring/idempotency_record_java.java"),
                &[("domain", domain), ("name", name)],
            ),
        },
        Artifact {
            kind: "idempotency receipt store port",
            path: crate::generate::main_dir(root, app).join(format!("{port}.java")),
            contents: crate::template::render(
                crate::template_here!("spring/idempotency_port_java.java"),
                &[
                    ("app", app),
                    ("name", name),
                    (
                        "record_import",
                        &crate::generate::import_of(app, domain, &record),
                    ),
                ],
            ),
        },
        Artifact {
            kind: "idempotency PostgreSQL store",
            path: crate::generate::main_dir(root, adapters).join(format!("Jdbc{port}.java")),
            contents: crate::template::render(
                crate::template_here!("spring/idempotency_store_java.java"),
                &[
                    ("adapters", adapters),
                    ("name", name),
                    ("table", &table),
                    (
                        "record_import",
                        &crate::generate::import_of(adapters, domain, &record),
                    ),
                    (
                        "port_import",
                        &crate::generate::import_of(adapters, app, &port),
                    ),
                ],
            ),
        },
        Artifact {
            kind: "idempotency guard",
            path: crate::generate::main_dir(root, service).join(format!("{name}Guard.java")),
            contents: crate::template::render(
                crate::template_here!("spring/idempotency_guard_java.java"),
                &[
                    ("service", service),
                    ("name", name),
                    (
                        "record_import",
                        &crate::generate::import_of(service, domain, &record),
                    ),
                    (
                        "port_import",
                        &crate::generate::import_of(service, app, &port),
                    ),
                ],
            ),
        },
        Artifact {
            kind: "idempotency guard test",
            path: crate::generate::test_dir(root, service).join(format!("{name}GuardTest.java")),
            contents: crate::template::render(
                crate::template_here!("spring/idempotency_test_java.java"),
                &[
                    ("service", service),
                    ("name", name),
                    (
                        "record_import",
                        &crate::generate::import_of(service, domain, &record),
                    ),
                    (
                        "port_import",
                        &crate::generate::import_of(service, app, &port),
                    ),
                ],
            ),
        },
        Artifact {
            kind: "idempotency migration",
            path: crate::generate::migration_file(slice.project(), &format!("create_{table}"))?,
            contents: idempotency_migration(&table),
        },
    ])
}

/// The table, and the two constraints that carry the semantics.
fn idempotency_migration(table: &str) -> String {
    crate::template::render(
        crate::template_here!("spring/idempotency_migration.sql"),
        &[("table", table)],
    )
}

/// Attach a production HTTP destination to an existing typed transactional
/// outbox. The outbox owns persistence/retry; this generator owns only the
/// destination adapter, so the same mechanism remains useful for arbitrary
/// provider APIs rather than one inbox vendor.
pub(crate) fn http_sink_files(
    slice: &Slice,
    name: &str,
    usecase: &str,
    event: &str,
) -> jails_support::Result<Vec<Artifact>> {
    let root: &Path = slice.project().root();
    let project = slice.project();
    let jobs: &str = &slice.placed(Layer::Jobs);
    let messaging: &str = &slice.owned(Layer::Messaging);
    let adapters: &str = &slice.owned(Layer::Adapters);
    let sink_port = crate::generate::main_dir(root, jobs).join(format!("{usecase}OutboxSink.java"));
    // Through the projection, so a usecase this same manifest declares two
    // rows above counts. `Path::is_file` answers about disk, and in one
    // transition nothing has been written yet.
    if !project.projected_main_sources().contains_key(&sink_port) {
        return Err(format!(
            "http-sink {name} cannot find {usecase}OutboxSink.java.\n       fix: generate usecase {usecase} with `--yields {event}` first."
        ).into());
    }
    let event_class = format!("{event}Event");
    let fields = slice
        .project()
        .record_in(messaging, &event_class)
        .ok_or_else(|| format!("http-sink {name} cannot read {event_class}.java"))?;
    let id = fields
        .iter()
        .find(|field| field.name == "id")
        .ok_or_else(|| format!("http-sink {name} needs {event_class}.id for idempotency"))?;
    if id.optionality == crate::generate::Optionality::Nullable {
        return Err(
            format!("http-sink {name} needs a required {event_class}.id for idempotency").into(),
        );
    }
    let json = crate::generate::main_dir(root, adapters).join("Json.java");
    if !project.projected_main_sources().contains_key(&json) {
        return Err(format!(
            "http-sink {name} needs the generic JSON capability.\n       fix: run `jails add json` first."
        ).into());
    }

    let usecase_property = crate::sql::snake_case(usecase).replace('_', "-");
    let sink_property = crate::sql::snake_case(name).replace('_', "-");
    let property = format!("outbox.{usecase_property}.http.{sink_property}");
    let url_value = format!("${{{}.url}}", property);
    let bearer_token_value = format!("${{{}.bearer-token:}}", property);
    let connect_timeout_value = format!("${{{}.connect-timeout-ms:2000}}", property);
    let request_timeout_value = format!("${{{}.request-timeout-ms:5000}}", property);
    let main = crate::template::render(
        crate::template_here!("spring/http_outbox_sink_java.java"),
        &[
            ("pkg", jobs),
            ("messaging", messaging),
            ("adapters", adapters),
            ("name", name),
            ("usecase", usecase),
            ("event", event),
            ("property", property.as_str()),
            ("url_value", url_value.as_str()),
            ("bearer_token_value", bearer_token_value.as_str()),
            ("connect_timeout_value", connect_timeout_value.as_str()),
            ("request_timeout_value", request_timeout_value.as_str()),
        ],
    );

    let samples = fields
        .iter()
        .map(|field| crate::generate::sample_value(field, project, messaging))
        .collect::<Vec<_>>();
    let disabled = samples.iter().any(Option::is_none);
    // A disabled test must still compile. Preserve the constructor arity and
    // use null only for project-owned value types whose construction Jails
    // cannot honestly infer yet; the annotation makes that missing oracle
    // visible instead of silently pretending the contract ran.
    let args = samples
        .into_iter()
        .map(|sample| sample.unwrap_or_else(|| "null".to_string()))
        .collect::<Vec<_>>()
        .join(",\n                ");
    let imports = java_literal_imports(&fields, messaging)
        .into_iter()
        .map(|import| format!("import {import};\n"))
        .collect::<String>();
    let disabled_import = if disabled {
        "import org.junit.jupiter.api.Disabled;\n"
    } else {
        ""
    };
    let annotation = if disabled {
        "@Disabled(\"todo: supply an HTTP sink event sample Jails cannot fabricate\")\n"
    } else {
        ""
    };
    let test = crate::template::render(
        crate::template_here!("spring/http_outbox_sink_test_java.java"),
        &[
            ("pkg", jobs),
            ("messaging", messaging),
            ("name", name),
            ("event", event),
            ("imports", imports.as_str()),
            ("disabled_import", disabled_import),
            ("annotation", annotation),
            ("args", args.as_str()),
        ],
    );
    Ok(vec![
        Artifact {
            kind: "HTTP outbox sink",
            path: crate::generate::main_dir(root, jobs).join(format!("{name}HttpOutboxSink.java")),
            contents: main,
        },
        Artifact {
            kind: "HTTP outbox sink test",
            path: crate::generate::test_dir(root, jobs)
                .join(format!("{name}HttpOutboxSinkTest.java")),
            contents: test,
        },
    ])
}

/// A scratch project a renderer test can plan against.
///
/// Every renderer now takes a [`Slice`], and a `Slice` is a resolved
/// [`Project`] plus the `--package` override. So a unit test needs a real
/// project directory rather than a handful of package strings -- which is the
/// point: the strings could disagree with each other, and a `Project` cannot.
/// The default layer names put `com.example.demo.service` and friends exactly
/// where these tests used to spell them by hand.
#[cfg(test)]
pub(crate) fn scratch_project(tag: &str, pom: &str) -> (std::path::PathBuf, Project) {
    let root = jails_support::scratch::ScratchDir::in_temp(&format!("jails-spring-{tag}"))
        .unwrap()
        .keep();
    std::fs::create_dir_all(root.join("src/main/java/com/example/demo")).unwrap();
    std::fs::write(root.join("pom.xml"), pom).unwrap();
    std::fs::write(
        root.join("src/main/java/com/example/demo/App.java"),
        "package com.example.demo;\npublic final class App {}\n",
    )
    .unwrap();
    let project = Project::load(&root).unwrap();
    (root, project)
}

/// The same, with the JDBC starter the persistence-shaped recipes require.
#[cfg(test)]
pub(crate) fn scratch_jdbc_project(tag: &str) -> (std::path::PathBuf, Project) {
    scratch_project(
        tag,
        "<project><dependencies><dependency>\
         <groupId>org.springframework.boot</groupId>\
         <artifactId>spring-boot-starter-jdbc</artifactId>\
         </dependency></dependencies></project>",
    )
}

#[cfg(test)]
/// A project whose build declares Kafka, for the generators that require it.
///
/// `g event` writes four files importing `org.springframework.kafka` and now
/// refuses without the starter, so a fixture that does not declare it is a
/// project the command correctly will not serve -- bugs.md B58.
pub(crate) fn scratch_kafka_project(tag: &str) -> (std::path::PathBuf, Project) {
    scratch_project(
        tag,
        "<project><dependencies><dependency>\
         <groupId>org.springframework.boot</groupId>\
         <artifactId>spring-boot-starter-kafka</artifactId>\
         </dependency></dependencies></project>",
    )
}

#[cfg(test)]
mod association_and_http_sink_tests {
    use super::*;

    #[test]
    fn association_refuses_an_empty_mapping_before_writing_invalid_sql() {
        let (_root, project) = scratch_jdbc_project("association-empty-mapping");
        let error = association_files(
            &Slice::new(&project, None),
            "ChildParent",
            "Child",
            "Parent",
            &[],
        )
        .unwrap_err();

        assert!(error.contains("at least one childField=parentField mapping"));
    }

    #[test]
    fn association_reuses_primary_and_prior_composite_unique_keys() {
        let (root, live) = scratch_jdbc_project("association-existing-keys");
        let migration = "create table parents (\n  id uuid not null,\n  workspace_id uuid not null,\n  constraint parents_pk\n    primary key (id)\n);\n\ncreate unique index parents_workspace_id_id_association_key\n  on parents (workspace_id, id);\n";
        let path = "src/main/resources/db/migration/V001__parents.sql";
        let projected = Project::projected(
            &live,
            std::collections::BTreeMap::from([(
                jails_protocol::identity::ProjectPath::parse(path).unwrap(),
                migration.as_bytes().to_vec(),
            )]),
        )
        .unwrap();

        assert!(migrations_declare_unique_key(
            &projected,
            "src/main/resources/db/migration",
            "parents",
            &["id".to_string()]
        ));
        assert!(migrations_declare_unique_key(
            &projected,
            "src/main/resources/db/migration",
            "parents",
            &["workspace_id".to_string(), "id".to_string()]
        ));
        assert!(!migrations_declare_unique_key(
            &projected,
            "src/main/resources/db/migration",
            "parents",
            &["workspace_id".to_string()]
        ));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn disabled_http_sink_contract_keeps_constructor_arity_for_unknown_values() {
        let root = jails_support::scratch::ScratchDir::in_temp("jails-http-sink-unknown-sample")
            .unwrap()
            .keep();
        for package in ["jobs", "messaging", "adapters"] {
            std::fs::create_dir_all(root.join(format!("src/main/java/com/example/demo/{package}")))
                .unwrap();
        }
        std::fs::write(
            root.join("src/main/java/com/example/demo/jobs/SendOutboxSink.java"),
            "package com.example.demo.jobs; interface SendOutboxSink {}",
        )
        .unwrap();
        std::fs::write(
            root.join("src/main/java/com/example/demo/messaging/SentEvent.java"),
            "package com.example.demo.messaging; import java.util.UUID; public record SentEvent(UUID id, CustomValue value) {}",
        )
        .unwrap();
        std::fs::write(
            root.join("src/main/java/com/example/demo/adapters/Json.java"),
            "package com.example.demo.adapters; public final class Json {}",
        )
        .unwrap();
        std::fs::write(root.join("pom.xml"), "<project></project>").unwrap();
        std::fs::write(
            root.join("src/main/java/com/example/demo/App.java"),
            "package com.example.demo;\npublic final class App {}\n",
        )
        .unwrap();

        let project = Project::load(&root).unwrap();
        let files =
            http_sink_files(&Slice::new(&project, None), "Provider", "Send", "Sent").unwrap();
        let test = &files
            .iter()
            .find(|artifact| artifact.kind == "HTTP outbox sink test")
            .unwrap()
            .contents;

        assert!(test.contains("@Disabled("), "{test}");
        assert!(
            test.contains(
                "UUID.fromString(\"00000000-0000-0000-0000-000000000001\"),\n                null"
            ),
            "{test}"
        );

        std::fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(test)]
mod durable_job_tests {
    use super::*;

    fn fixture(tag: &str) -> std::path::PathBuf {
        let root = jails_support::scratch::ScratchDir::in_temp(&format!("jails-durable-job-{tag}"))
            .unwrap()
            .keep();
        for package in ["domain", "service"] {
            std::fs::create_dir_all(root.join(format!("src/main/java/com/example/demo/{package}")))
                .unwrap();
        }
        std::fs::create_dir_all(root.join("src/main/resources/db/migration")).unwrap();
        std::fs::write(
            root.join("pom.xml"),
            "<project><dependencies><dependency><groupId>org.springframework.boot</groupId><artifactId>spring-boot-starter-jdbc</artifactId></dependency></dependencies></project>",
        )
        .unwrap();
        // `Project::load` resolves the base package from the shallowest source
        // file, so a fixture needs one even when the test never reads it.
        std::fs::write(
            root.join("src/main/java/com/example/demo/App.java"),
            "package com.example.demo;\npublic final class App {}\n",
        )
        .unwrap();
        root
    }

    fn write_record(root: &Path, package: &str, name: &str, specs: &[&str]) {
        let fields = crate::generate::parse_fields(
            &specs
                .iter()
                .map(|spec| (*spec).to_string())
                .collect::<Vec<_>>(),
        )
        .unwrap();
        std::fs::write(
            root.join(format!(
                "src/main/java/com/example/demo/{package}/{name}.java"
            )),
            crate::generate::record_java(&format!("com.example.demo.{package}"), name, &fields),
        )
        .unwrap();
    }

    #[test]
    fn durable_job_has_leasing_bounded_retry_idempotency_and_recovery() {
        let root = fixture("contract");
        write_record(&root, "domain", "WorkItem", &["id:uuid", "sourceUrl:uri"]);
        write_record(
            &root,
            "service",
            "EnqueueWorkCommand",
            &["id:uuid", "sourceUrl:uri"],
        );
        let fields =
            crate::generate::parse_fields(&["id:uuid".to_string(), "sourceUrl:uri".to_string()])
                .unwrap();

        let project = Project::load(&root).unwrap();
        let files = durable_job_files(
            &Slice::new(&project, None),
            "WorkDispatcher",
            "EnqueueWork",
            "WorkItem",
            &fields,
        )
        .unwrap();
        let store = &files
            .iter()
            .find(|artifact| artifact.kind == "durable PostgreSQL store")
            .unwrap()
            .contents;
        let worker = &files
            .iter()
            .find(|artifact| artifact.kind == "durable worker")
            .unwrap()
            .contents;
        let migration = &files
            .iter()
            .find(|artifact| artifact.kind == "durable job migration")
            .unwrap()
            .contents;

        assert!(store.contains("for update skip locked"), "{store}");
        assert!(store.contains("lease_until <= now()"), "{store}");
        assert!(store.contains("attempts >= max_attempts"), "{store}");
        assert!(store.contains("on conflict (id) do nothing"), "{store}");
        assert!(store.contains("jobs.id as id"), "{store}");
        assert!(worker.contains("results.findById"), "{worker}");
        assert!(worker.contains("store.succeed(work.id())"), "{worker}");
        assert!(migration.contains("state in ('PENDING', 'RUNNING', 'SUCCEEDED', 'FAILED')"));
    }

    #[test]
    fn durable_job_requires_a_stable_id_shared_with_the_command() {
        let root = fixture("identity");
        write_record(&root, "domain", "WorkItem", &["id:uuid", "sourceUrl:uri"]);
        write_record(&root, "service", "EnqueueWorkCommand", &["sourceUrl:uri"]);
        let fields = crate::generate::parse_fields(&["sourceUrl:uri".to_string()]).unwrap();

        let project = Project::load(&root).unwrap();
        let error = durable_job_files(
            &Slice::new(&project, None),
            "WorkDispatcher",
            "EnqueueWork",
            "WorkItem",
            &fields,
        )
        .unwrap_err();

        assert!(
            error.contains("EnqueueWorkCommand has no required `id:uuid`"),
            "{error}"
        );
        assert!(error.contains("regenerate the use case"), "{error}");
    }
}
