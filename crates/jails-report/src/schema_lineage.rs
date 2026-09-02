//! Which columns a resource's own migrations actually created.
//!
//! `doctor`'s other checks answer "are these the bytes jails wrote"; this one
//! answers "is this project coherent". The two differ when the Java carries a
//! component the schema history does not: every file on disk is byte-identical
//! to what jails wrote, so nothing else has a reason to complain, and only a
//! query at runtime would find it.
//!
//! **This reads only bytes jails published, verified by digest.** Every
//! migration in the lineage is a lifecycle *seal*, and the seal carries the
//! sha256 of what jails wrote; a file whose digest no longer matches is
//! already a separate `FAIL` and stops this check rather than being parsed.
//! That bound is what makes a reader this small honest: it is not parsing SQL,
//! it is reading back the handful of statements `jails-generate::sql` emits.
//!
//! **Unknown widens.** A statement outside that handful, a migration the
//! lineage does not hold, or a table the lineage renames away leaves the
//! answer `None` -- reported as nothing at all, never as a failure. Guessing a
//! column list and reporting a project broken on the strength of it would be
//! worse than the silence it replaces.

use crate::diagnostic::{Check, Status};
use crate::model::Project;
use jails_protocol::entity::{EntityId, EntitySpec};
use jails_protocol::lifecycle::ResourceState;
use jails_state::compat::MachineState;
use std::collections::BTreeSet;

/// Compare each active resource's recorded components against the columns its
/// own migration lineage declares.
pub(crate) fn schema_lineage_checks(project: &Project) -> Vec<Check> {
    let MachineState::Current(store) = jails_state::compat::read(project.root()) else {
        return Vec::new();
    };
    let mut checks = Vec::new();
    for lifecycle in &store.lifecycles {
        let EntityId::Intent(id) = &lifecycle.entity else {
            continue;
        };
        if !matches!(lifecycle.state, ResourceState::Active) {
            continue;
        }
        let Some(table) = lifecycle.table.as_ref() else {
            continue;
        };
        let EntitySpec::Intent(spec) = &lifecycle.last_spec else {
            continue;
        };
        let Some(declared) = lineage_columns(project, lifecycle, table.table.as_str()) else {
            continue;
        };
        let Some(expected) = expected_columns(project, id, spec) else {
            continue;
        };

        let title = format!("schema {}", id.name);
        let missing: Vec<&str> = expected
            .iter()
            .filter(|column| !declared.contains(*column))
            .map(String::as_str)
            .collect();
        let extra: Vec<&str> = declared
            .iter()
            .filter(|column| !expected.contains(*column))
            .map(String::as_str)
            .collect();
        if !missing.is_empty() {
            checks.push(
                Check::new(
                    Status::Fail,
                    title.clone(),
                    format!(
                        "`{}` has component(s) `{}` that table `{}` has no column for -- every \
                         query against them fails at runtime",
                        id.name,
                        missing.join("`, `"),
                        table.table.as_str()
                    ),
                )
                .fix(format!(
                    "jails resource field add {} <name>:<type> -- or restore the component list \
                     the migrations describe",
                    id.name
                )),
            );
        }
        if !extra.is_empty() {
            checks.push(
                Check::new(
                    Status::Warn,
                    title,
                    format!(
                        "table `{}` has column(s) `{}` that `{}` no longer declares",
                        table.table.as_str(),
                        extra.join("`, `"),
                        id.name
                    ),
                )
                .fix(format!(
                    "jails resource field drop {} <name> --confirm-column <column>, or declare \
                     the component again",
                    id.name
                )),
            );
        }
    }
    checks
}

/// The columns this resource's projection needs, as SQL names.
fn expected_columns(
    project: &Project,
    id: &jails_protocol::entity::IntentId,
    spec: &jails_protocol::declaration::IntentSpec,
) -> Option<BTreeSet<String>> {
    let fields = spec
        .fields()
        .iter()
        .map(|field| field.projected())
        .collect::<jails_support::Result<Vec<_>>>()
        .ok()?;
    let columns = jails_generate::sql::columns(
        &fields,
        project,
        &project.package_named(jails_spec::spec::layout::DOMAIN, None),
        &jails_generate::generate::lower_first(id.name.as_str()),
    );
    // A component jails cannot map to one column -- a project record reached
    // through an association -- has no column to look for, so it is not
    // evidence of anything either way.
    Some(
        columns
            .iter()
            .filter(|column| column.mapped())
            .map(|column| column.name.clone())
            .collect(),
    )
}

/// Replay the lineage and return the column set it leaves, or `None` when
/// anything in it is outside what jails writes.
fn lineage_columns(
    project: &Project,
    lifecycle: &jails_protocol::lifecycle::ResourceLifecycleV1,
    table: &str,
) -> Option<BTreeSet<String>> {
    let mut columns = BTreeSet::new();
    let mut created = false;
    for seal in &lifecycle.migrations {
        let bytes = std::fs::read(project.root().join(seal.path.as_str())).ok()?;
        if jails_support::identity::ObjectId::from_bytes(jails_support::codec::sha256(&bytes))
            != seal.content_digest
        {
            // Already reported as a broken seal. Reading it would be reading
            // something jails did not write.
            return None;
        }
        let text = String::from_utf8(bytes).ok()?;
        apply(&text, table, &mut columns, &mut created)?;
    }
    created.then_some(columns)
}

/// Fold migration texts, in order, into the column set for one table.
///
/// The model-level half of this check needs the same reader, and two readers
/// of the statements jails emits would drift. `None` means the lineage is not
/// readable -- a statement outside the handful the compiler writes, or no
/// `create table` at all -- and unknown widens rather than accusing.
pub fn columns_from(migrations: &[&str], table: &str) -> Option<BTreeSet<String>> {
    let mut columns = BTreeSet::new();
    let mut created = false;
    for text in migrations {
        apply(text, table, &mut columns, &mut created)?;
    }
    created.then_some(columns)
}

/// Fold one migration's statements into the column set.
///
/// Returns `None` for anything this reader does not recognise *about this
/// table*, which is what keeps a hand-written or hand-edited lineage from
/// producing a confident wrong answer.
fn apply(
    text: &str,
    table: &str,
    columns: &mut BTreeSet<String>,
    created: &mut bool,
) -> Option<()> {
    for statement in statements(text) {
        let words: Vec<&str> = statement.split_whitespace().collect();
        match words.as_slice() {
            // Statements about some other table are not this resource's
            // business, and neither are the ones that touch no column.
            ["create", "unique", ..] | ["create", "index", ..] => {}
            ["drop", "table", name, ..] if unquote(name) != table => {}
            ["create", "table", name, ..] if unquote(name) != table => {}
            ["alter", "table", name, ..] if unquote(name) != table => {}
            ["create", "table", _, ..] => {
                if *created {
                    return None;
                }
                *created = true;
                columns.extend(create_table_columns(&statement)?);
            }
            ["drop", "table", _] => {
                *created = false;
                columns.clear();
            }
            ["alter", "table", _, "add", "column", column, ..] => {
                columns.insert(unquote(column).to_string());
            }
            ["alter", "table", _, "drop", "column", column, ..] => {
                columns.remove(unquote(column));
            }
            ["alter", "table", _, "rename", "column", from, "to", to] => {
                columns.remove(unquote(from));
                columns.insert(unquote(to).to_string());
            }
            // Type, nullability and constraint changes move no column in or
            // out, so the set is unchanged and the lineage stays readable.
            ["alter", "table", _, "alter", "column", ..]
            | ["alter", "table", _, "add", "constraint", ..]
            | ["alter", "table", _, "drop", "constraint", ..] => {}
            ["update", ..] | ["insert", ..] | ["delete", ..] => {}
            _ => return None,
        }
    }
    Some(())
}

/// The column names a `create table` declares, skipping table constraints.
fn create_table_columns(statement: &str) -> Option<Vec<String>> {
    let open = statement.find('(')?;
    let close = statement.rfind(')')?;
    let mut columns = Vec::new();
    let mut depth = 0usize;
    let mut current = String::new();
    for ch in statement.get(open + 1..close)?.chars() {
        match ch {
            '(' => {
                depth += 1;
                current.push(ch);
            }
            ')' => {
                depth = depth.checked_sub(1)?;
                current.push(ch);
            }
            ',' if depth == 0 => {
                push_column(&current, &mut columns);
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    push_column(&current, &mut columns);
    Some(columns)
}

fn push_column(item: &str, columns: &mut Vec<String>) {
    let Some(first) = item.split_whitespace().next() else {
        return;
    };
    // `constraint <name> primary key (...)`, `primary key (...)`, `unique
    // (...)` and `check (...)` are table constraints, not columns.
    if matches!(
        first,
        "constraint" | "primary" | "unique" | "check" | "foreign"
    ) {
        return;
    }
    columns.push(unquote(first).to_string());
}

/// Statement-splitting for the shapes jails writes: line comments stripped,
/// then split on `;`.
fn statements(text: &str) -> Vec<String> {
    let stripped: String = text
        .lines()
        .filter(|line| !line.trim_start().starts_with("--"))
        .collect::<Vec<_>>()
        .join("\n");
    stripped
        .split(';')
        .map(|statement| statement.trim().to_ascii_lowercase())
        .filter(|statement| !statement.is_empty())
        .collect()
}

fn unquote(name: &str) -> &str {
    name.trim_matches('"').trim_end_matches('(')
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Replay a lineage the way `lineage_columns` does, minus the digest
    /// check, so the fold can be exercised without a project on disk.
    fn replay(migrations: &[&str], table: &str) -> Option<Vec<String>> {
        let mut columns = BTreeSet::new();
        let mut created = false;
        for text in migrations {
            apply(text, table, &mut columns, &mut created)?;
        }
        created.then(|| columns.into_iter().collect())
    }

    #[test]
    fn the_lineage_is_the_create_plus_every_forward_change_to_it() {
        let columns = replay(
            &[
                "create table orders (\n  id uuid not null,\n  total numeric(19, 2) not null,\n  constraint orders_pk primary key (id)\n);\n",
                "alter table orders add column note text;\n",
                "alter table orders rename column note to memo;\n",
                "alter table orders alter column memo set not null;\n",
                "alter table orders add column scratch text;\n",
                "alter table orders drop column scratch;\n",
            ],
            "orders",
        )
        .unwrap();
        assert_eq!(columns, ["id", "memo", "total"]);
    }

    /// The property the whole reader rests on: it would rather say nothing
    /// than say something wrong. A hand-written statement it does not
    /// recognise ends the lineage, so no column list is compared at all.
    #[test]
    fn an_unrecognised_statement_widens_to_no_answer_rather_than_a_guess() {
        assert!(
            replay(
                &[
                    "create table orders (id uuid not null);\n",
                    "alter table orders add column total numeric(19, 2);\n",
                    "alter table orders rename to legacy_orders;\n",
                ],
                "orders",
            )
            .is_none()
        );
        // Two `create table`s for one table is a lineage this reader cannot
        // fold, rather than one that silently wins.
        assert!(
            replay(
                &[
                    "create table orders (id uuid not null);\n",
                    "create table orders (id uuid not null, total numeric);\n",
                ],
                "orders",
            )
            .is_none()
        );
        // Never created at all: nothing to compare against.
        assert!(
            replay(
                &["alter table orders add column total numeric;\n"],
                "orders"
            )
            .is_none()
        );
    }

    /// A shared migration directory holds every resource's history, so a
    /// statement about somebody else's table must move nothing here -- and
    /// must not be read as unrecognised either, or one `g scaffold` would
    /// silence the coherence check for every other resource in the project.
    #[test]
    fn statements_about_another_table_leave_this_lineage_alone() {
        let columns = replay(
            &[
                "create table orders (id uuid not null);\n",
                "create table customers (id uuid not null, email text not null);\n",
                "alter table customers add column name text;\n",
                "create index customers_email_idx on customers (email);\n",
                "alter table orders add column total numeric(19, 2);\n",
            ],
            "orders",
        )
        .unwrap();
        assert_eq!(columns, ["id", "total"]);
    }

    /// `destroy --storage drop` then a regenerate is a real lineage, and the
    /// answer is the *second* table's columns, not the union of both.
    #[test]
    fn a_dropped_and_recreated_table_reports_what_the_second_create_declared() {
        let columns = replay(
            &[
                "create table orders (id uuid not null, legacy text);\n",
                "drop table orders;\n",
                "create table orders (id uuid not null, total numeric(19, 2) not null);\n",
            ],
            "orders",
        )
        .unwrap();
        assert_eq!(columns, ["id", "total"]);
    }

    /// A table constraint is not a column, and a type whose own declaration
    /// carries a comma must not be split into two.
    #[test]
    fn table_constraints_are_not_columns_and_a_parenthesised_type_is_one() {
        let columns = create_table_columns(
            "create table orders (\n  id uuid not null,\n  total numeric(19, 2) not null,\n  \
             note text,\n  constraint orders_pk primary key (id),\n  unique (note)\n)",
        )
        .unwrap();
        assert_eq!(columns, ["id", "total", "note"]);
    }

    /// Comments carry the prose jails writes above a generated migration, and
    /// a `;` inside one would otherwise end a statement early.
    #[test]
    fn comment_lines_are_not_statements() {
        assert_eq!(
            statements(
                "-- add column note; forward-only\nalter table orders add column note text;\n"
            ),
            ["alter table orders add column note text"]
        );
    }
}
