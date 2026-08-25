//! The one command that spans layers, and the evolution step that follows it.
//!
//! `scaffold` is different in kind from every other generator: it writes into
//! five packages at once, so it is the only place that has to say out loud
//! which package each half of a vertical slice lives in -- and pay the imports
//! that crossing those boundaries costs. `g field` is its second half, the
//! evolution step, and it lives here because it re-derives exactly the same
//! artifacts from a record already on disk.
//!
//! The scaffold has to produce an application that *starts*, not just one that
//! compiles, and `CLAUDE.md` records the two things that constrains: exactly
//! one repository adapter carries `@Repository`, and `Field::java_type` always
//! holds the inner type with `Optionality` carrying the rest.

use crate::model::{Artifact, Layer, Project};
use jails_support::Result;
use std::path::Path;

use super::*;

/// The one command that spans layers, and so the only place that has to say
/// out loud which package each half of a vertical slice lives in -- and add
/// the imports that crossing those boundaries now costs.
pub(crate) fn scaffold_artifacts(
    slice: &crate::model::Slice,
    name: &str,
    fields: &[String],
    indexes: &[String],
) -> Result<Vec<Artifact>> {
    let domain = slice.placed(Layer::Domain);
    let (parsed, reusing_record) =
        fields_from_spec_or_record(slice.project(), &domain, name, fields)?;
    // The unmapped-component refusal deliberately lives in
    // `scaffold_artifacts_from_fields`, which reads the referenced record's
    // stored `@pk` and names the two commands that do the job. A generic
    // refusal here would shadow that one and teach nothing.
    scaffold_artifacts_from_fields(slice, name, &parsed, indexes, !reusing_record)
}

pub(crate) fn scaffold_artifacts_from_fields(
    slice: &crate::model::Slice,
    name: &str,
    parsed: &[Field],
    indexes: &[String],
    include_record: bool,
) -> Result<Vec<Artifact>> {
    let root: &Path = slice.project().root();
    let place = |layer| slice.placed(layer);
    let domain = place(Layer::Domain);
    let repository = place(Layer::App);
    let adapters = place(Layer::Adapters);
    let service = place(Layer::Service);
    let web = place(Layer::Web);

    crate::spring::require_scope_authorizer(slice, "scaffold", name, parsed)?;

    let domain_in = |user: &str| import_of(user, &domain, name);
    let columns = crate::sql::columns(parsed, slice.project(), &domain, &lower_first(name));
    for (field, column) in parsed.iter().zip(&columns) {
        if column.mapped() {
            continue;
        }
        if field.collection {
            return Err(format!(
                "scaffold {name} cannot persist `{}:{}`: a collection is not one column.\n       \
                 fix: generate a record for the element and model the relationship explicitly \
                 with `jails g association`.",
                field.name, field.java_type
            ));
        }
        if main_dir(root, &domain)
            .join(format!("{}.java", field.java_type))
            .is_file()
        {
            if let Some(key) = stored_primary_key(root, &field.java_type, slice.override_package())?
            {
                return Err(format!(
                    "{name}.{} has project record type {}, which cannot be persisted as one `{}` column.\n       \
                     fix: replace it with `{}:{}` and run `jails g association {name}{} {}={} --on {name} --yields {}`.",
                    field.name,
                    field.java_type,
                    column.sql_type,
                    field.name,
                    declared_type(&key),
                    capitalize(&field.name),
                    field.name,
                    key.name,
                    field.java_type
                ));
            }
            return Err(format!(
                "scaffold {name} cannot persist `{}:{}`: {} is a record in this project, but \
                 it has no single stored @pk to reference.\n       \
                 fix: give {} exactly one @pk component, or model the foreign key as a \
                 built-in scalar field and relate them with `jails g association`.",
                field.name, field.java_type, field.java_type, field.java_type
            ));
        }
        return Err(format!(
            "scaffold {name} cannot persist `{}:{}`: jails has no SQL/JDBC mapping for that \
             type.\n       \
             fix: use a built-in field type or an enum (stored by name), or generate {} as a \
             record and relate them with `jails g association`.",
            field.name, field.java_type, field.java_type
        ));
    }

    // The migration is emitted only when the project has somewhere to put
    // one -- `jails add db` creates db/migration, and a .sql file in a
    // project with no Flyway is dead weight nobody asked for. When it is
    // emitted it comes from the same column list as the adapter, which is
    // the point: a hand-written pair drifts (an `amount` column against an
    // `amount_minor` select), and one list cannot disagree with itself.
    let mut artifacts = Vec::new();

    // One sample, two readers: the collection a reader sends by hand and the
    // generated controller test that sends it on every build.
    let sample = sampled_request(slice.project(), &domain, parsed);

    artifacts.push(Artifact {
        kind: "HTTP request collection",
        path: root
            .join("requests")
            .join(format!("{}.http", crate::sql::snake_case(name))),
        contents: scaffold_requests(name, parsed, &sample.0),
    });

    // A fixture file, on the same rule as the migration: only when the
    // project already has somewhere to put one. `new`/`new-cli` seed
    // src/test/resources/fixtures, and `add testkit` generates the
    // `Fixtures` loader that reads it -- so the file is live, not decoration.
    let fixtures_dir = root.join("src/test/resources/fixtures");
    // The projection, not disk: `new`/`new-cli` seed the fixtures directory,
    // but in an aggregate apply an earlier row may be the thing creating it.
    if slice.project().has_directory("src/test/resources/fixtures") && !columns.is_empty() {
        let table = crate::sql::table_name(name);
        let constant = |type_name: &str| first_enum_constant(slice.project(), &domain, type_name);
        artifacts.push(Artifact {
            kind: "fixture",
            path: fixtures_dir.join(format!("{table}.json")),
            contents: crate::sql::fixture_json(&columns, &constant),
        });
    }
    // Same: `add db` creates `db/migration`, and in one transition it has not
    // been written when this plans. Reading disk here silently dropped every
    // scaffold's migration from the first `app apply` of a manifest that
    // installs the database and generates a resource together.
    if slice
        .project()
        .has_directory("src/main/resources/db/migration")
        && !columns.is_empty()
    {
        let table = crate::sql::table_name(name);
        // Checked before it is written: a typo here fails at `flyway migrate`
        // with "column does not exist", on whichever machine runs it first.
        for spec in indexes {
            crate::sql::validate_index(spec, &columns)?;
        }
        artifacts.push(Artifact {
            kind: "migration",
            path: crate::generate::migration_file(slice.project(), &format!("create_{table}"))?,
            contents: crate::sql::create_table(name, &columns, indexes),
        });
    }

    if include_record {
        artifacts.extend([
            Artifact {
                kind: "record",
                path: main_dir(root, &domain).join(format!("{name}.java")),
                contents: record_java(&domain, name, parsed),
            },
            Artifact {
                kind: "record test",
                path: test_dir(root, &domain).join(format!("{name}Test.java")),
                contents: record_test(slice.project(), &domain, name, parsed),
            },
        ]);
    }

    artifacts.extend(vec![
        Artifact {
            kind: "repository port",
            path: main_dir(root, &repository).join(format!("{name}Repository.java")),
            contents: repository_port(&repository, name, &domain_in(&repository)),
        },
        Artifact {
            kind: "JDBC adapter",
            path: main_dir(root, &adapters).join(format!("Jdbc{name}Repository.java")),
            contents: jdbc_repository_for(
                slice.project(),
                &adapters,
                name,
                &format!(
                    "{}{}",
                    domain_in(&adapters),
                    import_of(&adapters, &repository, &format!("{name}Repository"))
                ),
                // The record was just written from these same fields, so the
                // adapter and the type it maps cannot disagree.
                &columns,
                &domain,
            ),
        },
        Artifact {
            kind: "JDBC adapter integration test",
            path: test_dir(root, &adapters).join(format!("Jdbc{name}RepositoryIT.java")),
            contents: jdbc_repository_test_for(
                slice.project(),
                &adapters,
                &domain,
                &repository,
                name,
                parsed,
                &columns,
            ),
        },
        Artifact {
            kind: "in-memory adapter",
            path: main_dir(root, &adapters).join(format!("InMemory{name}Repository.java")),
            contents: crate::spring::in_memory_repository_java(
                &adapters,
                name,
                &format!(
                    "{}{}",
                    domain_in(&adapters),
                    import_of(&adapters, &repository, &format!("{name}Repository"))
                ),
                parsed
                    .iter()
                    .find(|f| f.name == "id")
                    .map(|f| f.name.as_str()),
                repository_wiring(slice.project()) != RepositoryWiring::JdbcClientBean,
            ),
        },
        Artifact {
            kind: "request",
            path: main_dir(root, &web).join(format!("{name}Request.java")),
            contents: crate::spring::request_java_for(
                &web,
                name,
                parsed,
                &domain_in(&web),
                &domain,
            ),
        },
        Artifact {
            kind: "response",
            path: main_dir(root, &web).join(format!("{name}Response.java")),
            contents: crate::spring::response_java_for(
                &web,
                name,
                parsed,
                &domain_in(&web),
                &domain,
            ),
        },
        Artifact {
            kind: "service",
            path: main_dir(root, &service).join(format!("{name}Service.java")),
            contents: crate::spring::resource_service_java(
                &service,
                name,
                &format!(
                    "{}{}",
                    domain_in(&service),
                    import_of(&service, &repository, &format!("{name}Repository"))
                ),
            ),
        },
        Artifact {
            kind: "service test",
            path: test_dir(root, &service).join(format!("{name}ServiceTest.java")),
            contents: crate::spring::resource_service_test_java(
                &service,
                name,
                &import_of(&service, &repository, &format!("{name}Repository")),
            ),
        },
        Artifact {
            kind: "controller",
            path: main_dir(root, &web).join(format!("{name}Controller.java")),
            contents: crate::spring::resource_controller_java(
                slice,
                name,
                &format!(
                    "{}{}",
                    domain_in(&web),
                    import_of(&web, &service, &format!("{name}Service"))
                ),
                parsed.iter().any(|f| f.name == "id"),
                parsed,
            ),
        },
        Artifact {
            kind: "controller test",
            path: test_dir(root, &web).join(format!("{name}ControllerTest.java")),
            contents: crate::spring::resource_controller_test_java(
                slice,
                name,
                &import_of(&web, &service, &format!("{name}Service")),
                parsed,
                (&sample.0, &sample.1[..]),
            ),
        },
    ]);

    Ok(artifacts)
}

/// The body, and the types jails could not sample for it.
///
/// A required component with no sample is written `null`, which is a
/// placeholder a reader replaces -- and a request that cannot be made. The
/// generated create test is `@Disabled` naming those types rather than
/// shipping one that fails on every build, which is the same rule
/// `generate::sample_value` follows for the DTO round trip.
pub(crate) fn sampled_request(
    project: &Project,
    domain: &str,
    fields: &[Field],
) -> (String, Vec<String>) {
    let audited = crate::spring::has_audit_pair(fields);
    let mut unsampled = Vec::new();
    let body = fields
        .iter()
        // The audit columns the create path sets itself. The request record
        // does not declare them, so a body carrying them describes a request
        // that cannot be made.
        .filter(|field| !crate::spring::is_audit_component(field, audited))
        .map(|field| {
            let value = if field.optionality == Optionality::Nullable {
                "null".to_string()
            } else if field.collection {
                if field.java_type.starts_with("Map") {
                    "{}".to_string()
                } else {
                    "[]".to_string()
                }
            } else {
                json_sample(project, domain, field).unwrap_or_else(|| {
                    unsampled.push(field.java_type.clone());
                    "null".to_string()
                })
            };
            format!("  \"{}\": {value}", field.name)
        })
        .collect::<Vec<_>>()
        .join(",\n");
    unsampled.sort();
    unsampled.dedup();
    (body, unsampled)
}

/// One field as JSON, or `None` when jails has no model of the type.
///
/// The wire spellings are Jackson's defaults for each type, checked against
/// `deps/jackson-databind`: `Currency` and `ZoneId` are their identifiers,
/// `byte[]` is base64, and `Duration` accepts ISO-8601 in either direction
/// even though it is written as decimal seconds.
pub(crate) fn json_sample(project: &Project, domain: &str, field: &Field) -> Option<String> {
    Some(match field.java_type.as_str() {
        "String" => format!("\"sample-{}\"", field.name),
        "Integer" | "int" | "Long" | "long" | "Double" | "double" | "BigDecimal" => "1".to_string(),
        "Boolean" | "boolean" => "true".to_string(),
        "UUID" => "\"00000000-0000-0000-0000-000000000001\"".to_string(),
        "LocalDate" => "\"2026-01-01\"".to_string(),
        "LocalDateTime" => "\"2026-01-01T00:00:00\"".to_string(),
        "Instant" => "\"2026-01-01T00:00:00Z\"".to_string(),
        "URI" => format!(
            "\"https://example.invalid/{}\"",
            crate::sql::snake_case(&field.name)
        ),
        "Currency" => "\"GBP\"".to_string(),
        "ZoneId" => "\"Europe/London\"".to_string(),
        "Duration" => "\"PT30S\"".to_string(),
        "byte[]" => "\"amFpbHM=\"".to_string(),
        // `pending.md` §1.3: this table and the field-type vocabulary are two
        // spellings of one set, and they had drifted. `path` was accepted by
        // `g scaffold` and had no sample here, so the generated `.http`
        // collection documented a request the record it was generated from
        // refuses. `every_builtin_type_has_a_json_sample` is what stops the
        // two separating again.
        "Path" => "\"/tmp/example\"".to_string(),
        other if field.owned => {
            format!("\"{}\"", first_enum_constant(project, domain, other)?)
        }
        _ => return None,
    })
}

/// The requests the generated controller actually answers, as a collection an
/// editor can send.
///
/// **Only the ones it answers.** A scoped scaffold's controller is create-only
/// -- every read has to be a `jails g query` with the tenant in its signature
/// -- so the `### List` block this used to end with unconditionally answered
/// 405 there. A reader sending it learns nothing about their project, only
/// about this file.
pub(crate) fn scaffold_requests(name: &str, fields: &[Field], body: &str) -> String {
    let route = resource_path(name);
    // Scoped resources are create-only; reads go through `jails g query`,
    // which writes its own collection.
    let list = if fields.iter().any(|field| field.constraints.scoped) {
        String::new()
    } else {
        format!("\n### List {name}\nGET {{{{baseUrl}}}}{route}\nAccept: application/json\n")
    };
    format!(
        "@baseUrl = http://localhost:8080\n\n\
         ### Create {name}\n\
         POST {{{{baseUrl}}}}{route}\n\
         Content-Type: application/json\n\n\
         {{\n{body}\n}}\n{list}"
    )
}

pub(crate) fn stored_primary_key(
    root: &Path,
    type_name: &str,
    package: Option<&str>,
) -> Result<Option<Field>> {
    let spec = match crate::generated_files::model_fields(root, type_name, package)? {
        Some(spec) => Some(spec),
        None if package.is_some() => crate::generated_files::model_fields(root, type_name, None)?,
        None => None,
    };
    let Some(spec) = spec else {
        return Ok(None);
    };
    let fields = parse_fields(&spec)?;
    let mut keys = fields
        .into_iter()
        .filter(|field| field.constraints.primary_key);
    let first = keys.next();
    Ok(if first.is_some() && keys.next().is_none() {
        first
    } else {
        None
    })
}
