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

use super::Column;
use jails_support::Result;

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
