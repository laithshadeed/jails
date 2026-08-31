//! `g search`: full-text search over a resource that already exists.
//!
//! One decision carries the whole kind, and `deps/postgres`' own documentation
//! makes it: the `tsvector` is a **generated column**, not a trigger.
//!
//! The trigger recipe is older and still widely copied, and it has a silent
//! failure. Somebody adds an UPDATE path that does not fire it — a bulk fixup,
//! a migration, a second service writing the same table — the row's text
//! changes, the vector does not, and the row stops matching a search that
//! should find it. Nothing errors, and nobody finds out until a customer says a record
//! has vanished. `generated always as (…) stored` cannot drift from its inputs,
//! because PostgreSQL maintains it.
//!
//! Two details that follow, both taken from `textsearch.sgml` rather than
//! remembered: every column is wrapped in `coalesce(x, '')`, because `||` with
//! a NULL operand yields NULL and one null column would blank the whole vector;
//! and the configuration is named in the expression rather than left to
//! `default_text_search_config`, so the stemming a row was indexed under does
//! not change when a session setting does.
//!
//! The adapter uses `websearch_to_tsquery`, which the same document describes
//! as the syntax "in which simple unformatted text is a valid query".
//! `to_tsquery` throws a syntax error on a bare two-word phrase, which is what
//! a search box produces — a search endpoint that 500s on an apostrophe is the
//! failure that choice avoids.
//!
//! All of it checked against a live PostgreSQL rather than reasoned about: the
//! migration applies, a search matches, `websearch_to_tsquery` does not error
//! on `it's "a" -- fine`, and — the property the trigger recipe loses — after
//! an `UPDATE` that changes the body, the row stops matching the old text
//! without anything having to remember to reindex it.

use super::*;

/// The text search configuration. English because it is the one every
/// PostgreSQL ships and the one `default_text_search_config` usually names;
/// it is written into the migration so that changing it is a migration.
const CONFIGURATION: &str = "english";

pub(crate) fn search_files(
    slice: &Slice,
    name: &str,
    fields: &[String],
) -> jails_support::Result<Vec<Artifact>> {
    if !slice.project().has_jdbc() {
        return Err(format!(
            "search {name} indexes a PostgreSQL table.\n       fix: run `jails add db` first."
        )
        .into());
    }
    let root: &Path = slice.project().root();
    let domain: &str = &slice.placed(Layer::Domain);
    let app: &str = &slice.placed(Layer::App);
    let adapters: &str = &slice.placed(Layer::Adapters);

    let Some(record) = slice.record(Layer::Domain, name) else {
        return Err(format!(
            "search {name} needs the record it searches.\n       \
             fix: `jails g scaffold {name} ...` first, or correct the name."
        )
        .into());
    };
    let columns = crate::sql::columns(&record, slice.project(), domain, "rows");
    let table = crate::sql::table_name(name);

    // Which components are searched. Named rather than inferred: a `tsvector`
    // over every text column indexes ids and status codes as if they were
    // prose, and the reader then cannot tell why a search for "active" returns
    // everything.
    let searched = searched_columns(fields, &columns, name)?;
    let expression = format!(
        "to_tsvector('{CONFIGURATION}', {})",
        searched
            .iter()
            .map(|column| format!("coalesce({column}, '')"))
            .collect::<Vec<_>>()
            .join(" || ' ' || ")
    );

    let column = "search_vector";
    let select = columns
        .iter()
        .map(|c| c.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let mapper = columns
        .iter()
        .map(|c| c.read.clone().unwrap_or_else(|| "null".to_string()))
        .collect::<Vec<_>>()
        .join(", ");

    let mut extra = crate::generate::import_of(adapters, domain, name);
    extra.push_str(&crate::generate::import_of(
        adapters,
        app,
        &format!("{name}Search"),
    ));
    for import in crate::sql::imports(&columns) {
        extra.push_str(&format!("import {import};\n"));
    }

    Ok(vec![
        Artifact {
            kind: "search port",
            path: crate::generate::main_dir(root, app).join(format!("{name}Search.java")),
            contents: crate::template::render(
                crate::template_here!("spring/search_port_java.java"),
                &[
                    ("app", app),
                    ("name", name),
                    (
                        "record_import",
                        &crate::generate::import_of(app, domain, name),
                    ),
                ],
            ),
        },
        Artifact {
            kind: "search adapter",
            path: crate::generate::main_dir(root, adapters).join(format!("Jdbc{name}Search.java")),
            contents: crate::template::render(
                crate::template_here!("spring/search_adapter_java.java"),
                &[
                    ("adapters", adapters),
                    ("name", name),
                    ("extra", &extra),
                    ("table", &table),
                    ("column", column),
                    ("columns", &select),
                    ("mapper", &mapper),
                    ("configuration", CONFIGURATION),
                ],
            ),
        },
        Artifact {
            kind: "search migration",
            path: crate::generate::migration_file(
                slice.project(),
                &format!("add_search_to_{}", crate::sql::snake_case(name)),
            )?,
            contents: crate::template::render(
                crate::template_here!("spring/search_migration.sql"),
                &[
                    ("table", &table),
                    ("column", column),
                    ("expression", &expression),
                ],
            ),
        },
    ])
}

/// The columns to index, from the component names the caller passed.
///
/// Validated against the record rather than trusted: a typo would otherwise
/// produce a migration that fails at `flyway migrate`, which is the furthest
/// possible point from where the mistake was made.
fn searched_columns(
    fields: &[String],
    columns: &[crate::sql::Column],
    name: &str,
) -> jails_support::Result<Vec<String>> {
    if fields.is_empty() {
        return Err(format!(
            "search {name} needs the components to index, e.g. \
             `jails g search {name} title body`.\n       \
             Indexing every text column would index ids and status codes as prose."
        )
        .into());
    }
    let mut out = Vec::new();
    for field in fields {
        // Accepts a component name or the column it maps to, because both are
        // things the reader has in front of them.
        let wanted = crate::sql::snake_case(field.split(':').next().unwrap_or(field));
        let Some(column) = columns.iter().find(|column| column.name == wanted) else {
            return Err(format!(
                "{name} has no component `{field}`.\n       \
                 fix: one of {}",
                columns
                    .iter()
                    .map(|column| column.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
            .into());
        };
        if !column.sql_type.contains("text") && !column.sql_type.contains("varchar") {
            return Err(format!(
                "`{field}` is {} -- full-text search indexes text.\n       \
                 fix: search a text component, or add one.",
                column.sql_type
            )
            .into());
        }
        out.push(column.name.clone());
    }
    Ok(out)
}
