//! **The statements a migration is made of.**
//!
//! Split out of `sql.rs` under `abstract.md` rung 11. The parent's secret is
//! *what one record component is, on both sides of the JDBC boundary* -- a
//! column name, a column type, and the two expressions that move a value
//! across. This one's is *what the schema says about a table*, which is a
//! different question asked by different callers: `create table`, the five
//! `alter table` verbs field evolution issues, and the constraints that turn
//! a fact the Java side enforces into one the database enforces too.
//!
//! Every rule here is a fact the application already believes and the schema
//! did not carry. `backend.md` §5: the schema is the last line of defence and
//! the cheapest one.

use super::{Column, generated_key, key_column};
use jails_support::Result;

/// `check (length(trim(body)) > 0)`, where the field spec said `!`.
///
/// plan.md P5.4, modern.md §4.7. The Java constructor rejects a blank value
/// and the database did not, so any path that is not the constructor -- a
/// `copy`, a backfill, another service -- could put a blank in a column the
/// application believes cannot hold one. `backend.md` §5: the schema is the
/// last line of defence.
///
/// Trimmed, not `<> ''`: `' '` is the case that bites, and it is exactly what
/// the Java side trims before rejecting.
///
/// **`trim`, not `btrim`.** They mean the same thing -- PostgreSQL's `trim(x)`
/// with no `leading`/`trailing` *is* `btrim(x)` -- and only one of them is
/// spelled the same everywhere jails emits. `BTRIM` reaches H2 in 2.0.202
/// (`git tag --contains` the commit that adds `TrimFunction.java` in
/// `deps/h2database`), and Boot 2.7 manages H2 1.4.200, whose function table
/// has `"TRIM"` and no `"BTRIM"`. On that project the application did not
/// start: `Function "BTRIM" not found`, thrown while `spring.sql.init` ran
/// `schema.sql`, so every bean that wanted a DataSource failed with it. Found
/// by running `g scaffold` on `minicom-15-01-2026` rather than by reading the
/// bytes, which is the only way this class of thing is found.
fn non_blank_check(column: &Column) -> String {
    if !column.non_blank || column.sql_type != "text" {
        return String::new();
    }
    format!(" check (length(trim({})) > 0)", column.name)
}

/// Whether this column's uniqueness has to ignore case.
///
/// **One shape, named on the whole trailing word.** As written, `A@b.com` and
/// `a@b.com` were two accounts -- a `@unique` that holds in the schema and
/// not in the world. Kept to `email` rather than generalised, because that is
/// the one identifier whose case-insensitivity is a fact about the format
/// rather than about this project's policy.
/// The unique index that makes `A@b.com` and `a@b.com` one account.
///
/// **Two dialects, two statements, and it is not a preference.** PostgreSQL
/// indexes the expression `lower(email)`; H2 2.x has no expression index at
/// all and answers `Syntax error ... expected "ASC, DESC, NULLS, ,, )"`, so
/// there the lowered value is a stored generated column and the index is on
/// that. Both give the same guarantee; only one of them parses on each engine.
///
/// Measured against a real H2 2.4.240 rather than assumed, which is the same
/// bar `Dialect::column_type`'s one rewrite was held to.
pub(crate) fn unique_index(table: &str, column: &Column) -> String {
    let name = &column.name;
    match column.dialect {
        jails_spec::spec::kind::Dialect::Postgres => format!(
            "\n-- Unique regardless of case: `A@b.com` and `a@b.com` are one account.\n\
             create unique index {table}_{name}_key\n  on {table} (lower({name}));\n"
        ),
        jails_spec::spec::kind::Dialect::H2 => format!(
            "\n-- Unique regardless of case: `A@b.com` and `a@b.com` are one account.\n\
             -- H2 has no expression index, so the lowered value is a column.\n\
             alter table {table}\n  add column {name}_lower {} generated always as \
             (lower({name}));\n\
             create unique index {table}_{name}_key\n  on {table} ({name}_lower);\n",
            column.sql_type
        ),
    }
}

pub(crate) fn case_insensitive(column: &Column) -> bool {
    column.sql_type == "text" && column.name.rsplit('_').next().unwrap_or(&column.name) == "email"
}

/// **Named**, and at the table level rather than on the column, because
/// adding a constant to the enum has to be able to replace it:
/// `alter table … drop constraint …` needs a name, and PostgreSQL's automatic
/// one is an implementation detail. plan.md P5.2 is the command that uses it.
pub(crate) fn closed_set_constraint(table: &str, column: &Column) -> Option<(String, String)> {
    if column.closed_set.is_empty() {
        return None;
    }
    let values = column
        .closed_set
        .iter()
        .map(|constant| format!("'{constant}'"))
        .collect::<Vec<_>>()
        .join(", ");
    Some((
        format!("{table}_{}_allowed", column.name),
        format!("check ({} in ({values}))", column.name),
    ))
}

/// The `order by` clause a list of this table's rows gets.
///
/// **Not the key.** `order by id` over a random UUID is a stable *random*
/// order presented to a reader as their data, which modern.md §4.4 calls the
/// defect a reader is most likely to notice as a user and least likely to
/// find in the code. `backend.md` §5 is the same point from the schema side.
///
/// So: the newest first, by whichever timestamp the table actually has --
/// `createdAt` where the scaffold was written with `--timestamps`, otherwise
/// the first required timestamp component declared. The key is appended as
/// the tiebreak rather than used as the sort, because two rows written in the
/// same instant otherwise come back in whatever order the plan happens to
/// produce, and a list that reorders between two identical requests is worse
/// than one ordered by something arbitrary but fixed.
///
/// A table with no timestamp at all falls back to the key, and the caller
/// says so in a comment: there is nothing else to order by, and SQL promises
/// no order without a clause.
pub(crate) fn ordering(columns: &[Column]) -> String {
    let key = key_column(columns).map(|column| column.name.as_str());
    let timestamp = columns
        .iter()
        .filter(|column| column.not_null && is_timestamp(&column.sql_type))
        .min_by_key(|column| match column.component.as_str() {
            "createdAt" => 0,
            _ => 1,
        })
        .map(|column| column.name.as_str());
    match (timestamp, key) {
        (Some(timestamp), Some(key)) => format!("{timestamp} desc, {key}"),
        (Some(timestamp), None) => format!("{timestamp} desc"),
        (None, Some(key)) => key.to_string(),
        // No key and no timestamp. With columns, the first is the only thing
        // left to name; with none, jails was given no field spec at all and
        // `id` is the same convention the rest of the adapter falls back on.
        (None, None) => columns
            .first()
            .map(|column| column.name.clone())
            .unwrap_or_else(|| "id".to_string()),
    }
}

/// Whether this column carries a point in time, in either dialect's spelling.
fn is_timestamp(sql_type: &str) -> bool {
    matches!(
        sql_type,
        "timestamptz" | "timestamp" | "timestamp with time zone" | "date"
    )
}

/// The table a type maps to: snake_case plus conservative regular-English
/// pluralisation.
///
/// **One owner, because three things derive from it**: the table name, the
/// route path (`web::resource_path`) and the fixture filename. A second
/// pluraliser somewhere else does not stay in step -- `g handler Category`
/// served `/categorys` while its table was `categories`, from two functions
/// forty lines apart.
///
/// Suffixes whose spelling rule is deterministic are applied; irregular
/// forms are **not guessed**, with two exceptions kept deliberately short:
/// a handful of English irregulars common enough in a schema that `persons`
/// and `childs` would look like a bug, and a handful of uncountables where
/// appending `s` is simply wrong. `jails.toml` gets no override for either:
/// derivability is what lets `destroy` find what `generate` wrote.
pub fn table_name(type_name: &str) -> String {
    let entity = jails_protocol::identity::Name::parse(type_name)
        .expect("generated type names are validated before SQL projection");
    jails_protocol::identity::SqlName::conventional_table(&entity)
        .as_str()
        .to_string()
}

/// `transactionId` -> `transaction_id`. Runs of capitals stay together
/// (`customerURL` -> `customer_url`) so an acronym does not explode into
/// one underscore per letter.
pub fn snake_case(name: &str) -> String {
    let component = jails_protocol::identity::Name::parse(name)
        .expect("generated names are validated before SQL projection");
    jails_protocol::identity::SqlName::conventional_column(&component)
        .as_str()
        .to_string()
}

/// The `create table` for a scaffolded type, as a Flyway migration body.
///
/// The primary key is whichever columns are marked `@pk`, in declaration
/// order, so a composite key is just several of them. Failing that it is a
/// column named `id`, and failing *that* it is nothing -- jails will not
/// invent a surrogate key, because a record whose components are its natural
/// key (the common case for the value types jails generates) does not want
/// one.
///
/// `@unique`, `@positive`/`@nonnegative` and `@index` come from the same
/// field spec, which is the point: a generated schema that cannot express
/// the constraints the table actually has gets hand-edited the moment it is
/// written, and then the field spec and the schema disagree forever.
///
/// `extra_indexes` carries what a per-column marker cannot: a composite or
/// ordered index (`customer_id, created_at desc`). Passed through as written
/// after its column names are checked against the table, because index
/// ordering is a real schema decision with no shorthand worth inventing.
pub(crate) fn create_table(
    type_name: &str,
    columns: &[Column],
    extra_indexes: &[String],
) -> String {
    let table = table_name(type_name);
    let width = columns
        .iter()
        .map(|c| c.name.len())
        .max()
        .unwrap_or(0)
        .max(4);
    let type_width = columns
        .iter()
        .map(|c| c.sql_type.len())
        .max()
        .unwrap_or(0)
        .max(4);

    let generated = generated_key(columns).map(|column| column.name.as_str());
    let mut body = String::new();
    for column in columns {
        let null = if column.not_null { " not null" } else { "" };
        // `always`, not `by default`: the insert this schema is generated
        // beside omits the column entirely, so an explicit value reaching it
        // is a caller working around the policy rather than exercising it.
        let identity = if generated == Some(column.name.as_str()) {
            " generated always as identity"
        } else {
            ""
        };
        let check = match column.constraints.check {
            Some(check) => format!(" check ({})", check.predicate(&column.name)),
            None => non_blank_check(column),
        };
        // A case-insensitive unique gets its own index below rather than an
        // inline `unique`, because `unique (lower(x))` is not a column
        // constraint anywhere.
        let unique = if column.constraints.unique && !case_insensitive(column) {
            " unique"
        } else {
            ""
        };
        // Trimmed before the comma: a nullable column would otherwise carry
        // the padding that only exists to line `not null` up.
        let declaration = format!(
            "{:type_width$}{identity}{null}{unique}{check}",
            column.sql_type
        );
        body.push_str(&format!(
            "  {:width$}  {},\n",
            column.name,
            declaration.trim_end()
        ));
    }

    let marked: Vec<&Column> = columns
        .iter()
        .filter(|c| c.constraints.primary_key)
        .collect();
    let key_columns: Vec<&str> = if marked.is_empty() {
        columns
            .iter()
            .filter(|c| c.name == "id")
            .map(|c| c.name.as_str())
            .collect()
    } else {
        marked.iter().map(|c| c.name.as_str()).collect()
    };
    // The closed set the reader already declared, said in the schema. Named
    // and table-level so `g enum` adding a constant can replace it.
    let mut constraint = columns
        .iter()
        .filter_map(|column| closed_set_constraint(&table, column))
        .map(|(name, predicate)| format!("\n  constraint {name}\n    {predicate},\n"))
        .collect::<String>();
    if !key_columns.is_empty() {
        constraint.push_str(&format!(
            "\n  constraint {table}_pk\n    primary key ({})\n",
            key_columns.join(", ")
        ));
    }
    // The last one carries no comma, whichever it is.
    let constraint = match constraint.strip_suffix(",\n") {
        Some(trimmed) => format!("{trimmed}\n"),
        None => constraint,
    };
    // Trailing comma removed only when no constraint follows it.
    let body = if constraint.is_empty() {
        body.trim_end().trim_end_matches(',').to_string() + "\n"
    } else {
        body
    };

    let unmapped: Vec<&str> = columns
        .iter()
        .filter(|c| !c.mapped())
        .map(|c| c.name.as_str())
        .collect();
    let note = if unmapped.is_empty() {
        String::new()
    } else {
        format!(
            "-- jails could not derive a column type for: {}.\n\
             -- Those are guesses; correct them before this runs anywhere real.\n",
            unmapped.join(", ")
        )
    };

    // Every statement is terminated. An unterminated `create table` followed
    // by a `create index` is one unparseable statement, and Flyway reports it
    // as a syntax error somewhere in the middle of the file.
    let mut out = format!(
        "-- Forward-only migration, generated from the field spec.\n\
         {note}create table {table} (\n{body}{constraint});\n"
    );

    for column in columns
        .iter()
        .filter(|c| c.constraints.unique && case_insensitive(c))
    {
        out.push_str(&unique_index(&table, column));
    }

    for column in columns.iter().filter(|c| c.constraints.indexed) {
        out.push_str(&format!(
            "\ncreate index {table}_{}_idx\n  on {table} ({});\n",
            column.name, column.name
        ));
    }
    for (n, spec) in extra_indexes.iter().enumerate() {
        out.push_str(&declared_index(&table, n + 1, spec));
    }
    out
}

/// One declared composite or ordered index, named by position.
///
/// **By position rather than by content**: an index over `created_at desc`
/// cannot put the ordering in an identifier, and a name derived by stripping
/// it would collide with the plain one. The position is the index's place in
/// the entity's recorded list, so the name a `create table` gives it and the
/// name a later `resource index add` gives the next one are the same series --
/// which is what stops the two disagreeing about which index is which.
pub fn declared_index(table: &str, position: usize, spec: &str) -> String {
    format!(
        "\ncreate index {table}_idx{position}\n  on {table} ({});\n",
        spec.trim()
    )
}

/// A forward-only migration for one newly introduced component.
///
/// Required columns carry a deterministic backfill default so this migration
/// is valid on a populated table, then drop that default: application code,
/// not the database, remains responsible for every future value.
pub fn add_column(type_name: &str, column: &Column) -> Result<String> {
    if !column.mapped() {
        return Err(format!(
            "field `{}` has project type `{}` and cannot be mapped to one column.\n       \
             fix: generate an association for a project record, or use a built-in/enum field type.",
            column.name, column.java_type
        )
        .into());
    }
    if column.constraints.primary_key {
        return Err(format!(
            "field `{}` cannot be added as a primary key to an existing table.\n       \
             fix: add a nullable/unique field, backfill it deliberately, then write a migration for the key change.",
            column.name
        ).into());
    }

    let table = table_name(type_name);
    let check = column
        .constraints
        .check
        .map(|check| format!(" check ({})", check.predicate(&column.name)))
        .unwrap_or_default();
    let unique = if column.constraints.unique && !case_insensitive(column) {
        " unique"
    } else {
        ""
    };
    let default = if column.not_null {
        Some(match column.sql_type.as_str() {
            "uuid" => "gen_random_uuid()",
            "integer" | "bigint" | "numeric" | "double precision"
                if column.constraints.check == Some(crate::generate::NumericCheck::Positive) =>
            {
                "1"
            }
            "integer" | "bigint" | "numeric" | "double precision" => "0",
            "boolean" => "false",
            "date" => "current_date",
            "timestamp" | "timestamptz" => "current_timestamp",
            "bytea" => r"'\x'::bytea",
            "text" if column.constraints.unique => {
                return Err(format!(
                    "required unique text field `{}` has no safe automatic backfill.\n       \
                     fix: add it as nullable first, backfill distinct values, then add not-null in a deliberate migration.",
                    column.name
                ).into());
            }
            "text" => "''",
            other => {
                return Err(format!(
                    "field `{}` maps to `{other}`, for which jails has no safe backfill default.\n       \
                     fix: make the field nullable, or write the data migration explicitly.",
                    column.name
                ).into());
            }
        })
    } else {
        None
    };

    let mut out = format!(
        "-- Forward-only migration generated for a new record component.\n\
         alter table {table}\n\
           add column {} {}",
        column.name, column.sql_type
    );
    if let Some(default) = default {
        out.push_str(&format!(" default {default} not null"));
    }
    out.push_str(unique);
    out.push_str(&check);
    out.push_str(";\n");
    if default.is_some() {
        out.push_str(&format!(
            "\n-- The default only backfilled rows that pre-date this field.\n\
             alter table {table}\n\
               alter column {} drop default;\n",
            column.name
        ));
    }
    // A column added later gets the same closed set a column declared at
    // create time does. Without this the schema's guarantee depends on when
    // the field was declared, which is not a fact about the domain.
    // plan.md P5.1.
    if column.constraints.unique && case_insensitive(column) {
        out.push_str(&unique_index(&table, column));
    }
    if let Some((name, predicate)) = closed_set_constraint(&table, column) {
        out.push_str(&format!(
            "\nalter table {table}\n  add constraint {name}\n  {predicate};\n"
        ));
    }
    if column.constraints.indexed {
        out.push_str(&format!(
            "\ncreate index {table}_{}_idx\n  on {table} ({});\n",
            column.name, column.name
        ));
    }
    Ok(out)
}

/// Add a required column through the safe populated-table sequence.
///
/// `backfill` is either a typed `update` produced by the planner or exact SQL
/// from a declared reader-owned input. The column is nullable until that data
/// step has completed, and only then becomes required.
pub fn add_required_column_with_backfill(
    type_name: &str,
    column: &Column,
    backfill: &str,
) -> Result<String> {
    let mut nullable = column.clone();
    nullable.not_null = false;
    let mut out = add_column(type_name, &nullable)?;
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str("\n-- Data plan supplied for rows that pre-date this field.\n");
    out.push_str(backfill.trim_end());
    out.push_str("\n\n");
    out.push_str(&set_column_nullable(type_name, &column.name, false));
    Ok(out)
}

/// Rename one physical column in a forward-only migration.
pub fn rename_column(type_name: &str, from: &str, to: &str) -> String {
    format!(
        "-- Forward-only column rename generated by jails.\n\
         alter table {}\n\
           rename column {from} to {to};\n",
        table_name(type_name)
    )
}

/// Change one physical column to a type already proven safe by the planner.
pub fn change_column_type(type_name: &str, column: &str, sql_type: &str) -> String {
    format!(
        "-- Forward-only safe type widening generated by jails.\n\
         alter table {}\n\
           alter column {column} type {sql_type};\n",
        table_name(type_name)
    )
}

/// Change whether one physical column accepts null values.
pub fn set_column_nullable(type_name: &str, column: &str, nullable: bool) -> String {
    let action = if nullable {
        "drop not null"
    } else {
        "set not null"
    };
    format!(
        "-- Forward-only nullability change generated by jails.\n\
         alter table {}\n\
           alter column {column} {action};\n",
        table_name(type_name)
    )
}

/// Drop exactly one confirmed physical column.
pub fn drop_column(type_name: &str, column: &str) -> String {
    format!(
        "-- Forward-only column removal generated by jails.\n\
         alter table {}\n\
           drop column {column};\n",
        table_name(type_name)
    )
}

/// Check an `--index` spec against the table before it is written into a
/// migration.
///
/// A typo here fails at `flyway migrate` with "column does not exist", which
/// is a slow way to find out and happens on whichever machine runs it first.
pub(crate) fn validate_index(spec: &str, columns: &[Column]) -> Result<()> {
    let known: Vec<&str> = columns.iter().map(|c| c.name.as_str()).collect();
    for part in spec.split(',') {
        // `created_at desc` -- the column is the first word, the rest is
        // ordering that Postgres parses and jails does not.
        let column = part.split_whitespace().next().unwrap_or("");
        if column.is_empty() {
            return Err(format!("--index '{spec}': empty column name").into());
        }
        if !known.contains(&column) {
            return Err(format!(
                "--index '{spec}': no column '{column}' in this table. Columns: {}",
                known.join(", ")
            )
            .into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::parse_fields as parse;
    use std::path::PathBuf;

    fn cols(specs: &[&str]) -> Vec<Column> {
        let fields = parse(&specs.iter().map(|s| s.to_string()).collect::<Vec<_>>()).unwrap();
        let project = crate::model::Project::inspect(&PathBuf::from("/nonexistent")).unwrap();
        super::super::columns(&fields, &project, "com.example", "value")
    }

    /// plan.md P5.4, modern.md §4.7. The Java constructor rejected a blank
    /// value and the database did not, so any path that is not the
    /// constructor could put one in.
    #[test]
    fn a_non_blank_component_is_non_blank_in_the_schema_too() {
        let ddl = create_table(
            "Note",
            &cols(&["id:uuid@pk", "body:string!", "note:string"]),
            &[],
        );
        assert!(ddl.contains("check (length(trim(body)) > 0)"), "{ddl}");
        // `note:string` is required and may be empty: `!` is what asks for
        // the stronger claim, and inventing it would reject data the record
        // accepts.
        assert!(!ddl.contains("trim(note)"), "{ddl}");
        // And never `btrim`, which is PostgreSQL's spelling and reaches H2
        // only in 2.0.202. Boot 2.7 manages H2 1.4.200, where it does not
        // resolve -- so the application failed to start while `spring.sql.init`
        // ran the schema, and every bean wanting a DataSource failed with it.
        assert!(!ddl.contains("btrim"), "{ddl}");
    }

    /// As written, `A@b.com` and `a@b.com` were two accounts -- a `@unique`
    /// that holds in the schema and not in the world.
    #[test]
    fn a_unique_email_is_unique_regardless_of_case() {
        let ddl = create_table("User", &cols(&["id:uuid@pk", "email:string!@unique"]), &[]);
        assert!(ddl.contains("create unique index users_email_key"), "{ddl}");
        assert!(ddl.contains("on users (lower(email))"), "{ddl}");
        // Not both: an inline `unique` beside it would be a second index
        // over the same column, case-sensitively.
        assert!(!ddl.contains("text not null unique"), "{ddl}");
    }

    /// Only `email`. A case-insensitive `@unique` on anything else is this
    /// project's policy, not a fact about the format.
    #[test]
    fn another_unique_text_column_stays_case_sensitive() {
        let ddl = create_table("Coupon", &cols(&["id:uuid@pk", "code:string!@unique"]), &[]);
        assert!(ddl.contains("unique"), "{ddl}");
        assert!(!ddl.contains("lower(code)"), "{ddl}");
    }
}
