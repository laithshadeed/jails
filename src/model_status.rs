//! `jails resource status` for a canonical project.
//!
//! Four authorities, each read where it lives:
//!
//! | authority | read from |
//! |---|---|
//! | declaration | the entity in `.jails/model.jdl` |
//! | generated | the accepted projection in the lock |
//! | migration-history | `WorkspaceSnapshot::migration_history` |
//! | live | a datasource |
//!
//! **A file belongs to an entity because the compiler said so**, not because
//! its name starts with the entity's. `Provenance::semantic_ids` carries the
//! stable IDs a rendered file was lowered from, so `JdbcOrderRepository.java`
//! is attributed to `Order` by the same record that would attribute a file
//! whose name mentions no entity at all. Matching on the name would miss the
//! prefixed adapters and claim a `PaymentOrder` file for `Order`.
//!
//! **Drift is measured against the accepted projection, never a fresh
//! render.** That is `managed_drift`'s rule and it holds here for the same
//! reason: a merge deliberately preserves reader edits, so re-rendering and
//! diffing would report every preserved edit as drift on every run forever.

use crate::{Invocation, Output};
use jails_model::{AppModel, StableId};
use jails_support::{Failure, Result};
use serde_json::json;

const SCHEMA: &str = "jails.resource-status.v1";

/// What one authority says about the resource.
///
/// `Unknown` is not a failure: it is what an authority that was not consulted
/// reports, and it widens rather than narrowing. `jails resource status` with
/// no `--datasource` cannot know what the database holds and says so.
#[derive(Clone, Copy, Eq, PartialEq)]
enum Authority {
    Present,
    Absent,
    Pending,
    Drifted,
    Unknown,
}

impl Authority {
    fn label(self) -> &'static str {
        match self {
            Self::Present => "present",
            Self::Absent => "absent",
            Self::Pending => "pending",
            Self::Drifted => "drifted",
            Self::Unknown => "unknown",
        }
    }
}

/// Whether the authorities agree.
#[derive(Clone, Copy, Eq, PartialEq)]
enum Consistency {
    Consistent,
    Pending,
    Drifted,
    Retired,
    /// Everything jails owns agrees and the database has not caught up.
    ///
    /// **The one state a project can be in while every file is correct.** The
    /// model declares the entity, the managed tree matches the lock, and the
    /// migration that creates the table is on disk and unapplied -- so every
    /// authority jails can read on its own says "consistent" and the
    /// application still fails on its first query. It is only reachable with
    /// `--datasource`, which is the point of asking.
    RuntimeSchemaBehind,
    Unknown,
}

impl Consistency {
    fn label(self) -> &'static str {
        match self {
            Self::Consistent => "consistent",
            Self::Pending => "pending",
            Self::Drifted => "drifted",
            Self::Retired => "retired",
            Self::RuntimeSchemaBehind => "runtime-schema-behind",
            Self::Unknown => "unknown",
        }
    }
}

struct Finding {
    code: &'static str,
    message: String,
}

struct Report {
    resource: Option<String>,
    state: Consistency,
    declaration: Authority,
    generated: Authority,
    migration_history: Authority,
    /// What the database itself holds, or `Unknown` when it was not asked.
    live: Authority,
    table: Option<String>,
    generated_files: Vec<String>,
    migrations: Vec<String>,
    findings: Vec<Finding>,
    next: Vec<String>,
}

pub(crate) fn run(selector: &str, live: Option<Live>, invocation: Invocation) -> Result<()> {
    let manifest = crate::model_command::resolve_manifest(None)?;
    let (source, model) = crate::model_command::load_model(&manifest, invocation.output)?;
    let root = crate::model_command::root()?;
    let snapshot = jails_workspace::capture(&root, &manifest, source.as_bytes(), model)
        .map_err(|error| Failure::Told(format!("could not capture workspace: {error}")))?;
    let report = inspect(&snapshot, selector, live.as_ref());
    match invocation.output {
        Output::Human => print!("{}", render_human(&report)),
        _ => crate::model_command::print_json(&render_json(&report))?,
    }
    Ok(())
}

/// What became of a resource the model no longer declares.
///
/// **A confirmed drop leaves the model and leaves the history.** Removal is
/// model subtraction, so nothing survives in the declaration to report -- and
/// answering `unknown` about a resource whose table this project created and
/// then dropped is the one question the migrations can answer exactly. The
/// table name is derived rather than remembered, which is the same
/// derivability that lets `destroy` find what `generate` wrote.
fn retired(snapshot: &jails_contracts::WorkspaceSnapshot, selector: &str) -> Report {
    let table = jails_model::plural_snake_case(&crate::model_resource::java_to_label(selector));
    let mut migrations = Vec::new();
    let mut created = false;
    let mut dropped = false;
    for record in &snapshot.migration_history.records {
        let Some(captured) = snapshot.files.get(&record.path) else {
            continue;
        };
        if !mentions_table(&captured.bytes, &table) {
            continue;
        }
        let text = String::from_utf8_lossy(&captured.bytes);
        created |= text.contains(&format!("create table {table}"));
        dropped |= text.contains(&format!("drop table {table}"));
        migrations.push(record.version.clone());
    }
    if !(created && dropped) {
        return Report {
            resource: None,
            state: Consistency::Unknown,
            declaration: Authority::Absent,
            generated: Authority::Unknown,
            migration_history: Authority::Unknown,
            live: Authority::Unknown,
            table: None,
            generated_files: Vec::new(),
            migrations: Vec::new(),
            findings: vec![Finding {
                code: "resource-not-declared",
                message: format!("`{selector}` names no entity in this project's model"),
            }],
            next: vec![format!("jails g record {selector} <field>:<type>")],
        };
    }
    Report {
        resource: Some(selector.to_string()),
        state: Consistency::Retired,
        declaration: Authority::Absent,
        generated: Authority::Absent,
        migration_history: Authority::Present,
        live: Authority::Unknown,
        table: Some(table.clone()),
        generated_files: Vec::new(),
        migrations,
        findings: vec![Finding {
            code: "resource-retired",
            message: format!("`{table}` was created and dropped by this project's migrations"),
        }],
        next: vec![format!("jails g scaffold {selector} <field>:<type>")],
    }
}

/// What the database itself holds, when a `--datasource` was named.
///
/// **Two facts, because a table can be there for the wrong reason.** The
/// catalogue says whether the table exists and Flyway's own history says which
/// migrations the database has run -- so a table created by hand and a table
/// created by the migration jails wrote are told apart, and a project whose
/// history is behind is named as such rather than reported `drifted`.
pub(crate) struct Live {
    pub tables: std::collections::BTreeSet<String>,
    pub applied: std::collections::BTreeSet<String>,
}

/// Answer from the captured workspace. Pure once capture has happened, which
/// is what lets a table drive the tests rather than a live project.
fn inspect(
    snapshot: &jails_contracts::WorkspaceSnapshot,
    selector: &str,
    live: Option<&Live>,
) -> Report {
    let declared = &snapshot.model.model;
    let Some(entity) = find(declared, selector) else {
        return retired(snapshot, selector);
    };
    let id = entity.id.as_str().to_string();
    let stored = entity.facets.contains(&jails_model::Facet::Repository);
    let mut findings = Vec::new();
    let mut next = Vec::new();

    // The lock's accepted model is what the last executed plan agreed to. An
    // entity declared but not yet in it is a pending `sync`, which is an
    // ordinary state rather than a fault.
    let accepted_entity = snapshot
        .accepted_model
        .as_ref()
        .and_then(|accepted| accepted.entities.get(&entity.id));
    let accepted_matches = accepted_entity == Some(entity);

    let mut generated_files = Vec::new();
    let mut drifted = Vec::new();
    if let Some(projection) = snapshot.accepted_projection.as_ref() {
        for (path, file) in &projection.files {
            if !file.provenance.semantic_ids.contains(&id) {
                continue;
            }
            generated_files.push(path.as_str().to_string());
            let live = snapshot.files.get(path);
            match live {
                None => drifted.push(format!("{} is missing", path.as_str())),
                Some(captured) if captured.bytes != file.bytes => {
                    drifted.push(format!("{} differs from the accepted image", path.as_str()))
                }
                Some(_) => {}
            }
        }
    }
    generated_files.sort();

    let generated = match (
        generated_files.is_empty(),
        accepted_matches,
        drifted.is_empty(),
    ) {
        (true, true, _) => Authority::Absent,
        (true, false, _) => Authority::Pending,
        (false, _, false) => Authority::Drifted,
        (false, true, true) => Authority::Present,
        (false, false, true) => Authority::Pending,
    };

    // A migration is this entity's when it names its table. The reader is
    // bounded to the statements the compiler emits, the same bound
    // `schema_lineage` works under: anything else widens to unknown rather
    // than being parsed.
    let mut migrations = Vec::new();
    let mut unreadable = false;
    if stored {
        // **The lineage follows the renames.** A single cutover moves the
        // table, so every migration written before it names a table this
        // entity no longer has -- and reporting one row of history for a
        // resource that has three is worse than reporting none, because it
        // reads as a resource whose creation was never recorded. The rename
        // statement names both sides, so walking backwards over it recovers
        // the names this entity used to have without a second record of them.
        let mut names = vec![entity.names.sql_table.clone()];
        let mut walked = 0;
        while walked < names.len() {
            let current = names[walked].clone();
            walked += 1;
            for record in &snapshot.migration_history.records {
                let Some(captured) = snapshot.files.get(&record.path) else {
                    continue;
                };
                let text = String::from_utf8_lossy(&captured.bytes);
                for line in text.lines() {
                    let Some((head, tail)) = line.split_once(" rename to ") else {
                        continue;
                    };
                    if tail.trim().trim_end_matches(';') != current {
                        continue;
                    }
                    let Some(before) = head.split_whitespace().next_back() else {
                        continue;
                    };
                    if !names.iter().any(|name| name == before) {
                        names.push(before.to_string());
                    }
                }
            }
        }
        for record in &snapshot.migration_history.records {
            match snapshot.files.get(&record.path) {
                Some(captured) => {
                    if names
                        .iter()
                        .any(|table| mentions_table(&captured.bytes, table))
                    {
                        migrations.push(record.version.clone());
                    }
                }
                None => unreadable = true,
            }
        }
    }

    let migration_history = match (stored, migrations.is_empty(), unreadable) {
        (false, _, _) => Authority::Unknown,
        (true, _, true) => Authority::Unknown,
        (true, true, false) => Authority::Absent,
        (true, false, false) => Authority::Present,
    };

    for reason in &drifted {
        findings.push(Finding {
            code: "generated-drift",
            message: reason.clone(),
        });
    }
    if !accepted_matches {
        findings.push(Finding {
            code: "declaration-not-accepted",
            message: "the model declares this entity and the lock has not accepted it".to_string(),
        });
        next.push("jails sync".to_string());
    }
    if stored && migration_history == Authority::Absent {
        findings.push(Finding {
            code: "table-without-migration",
            message: format!("no recorded migration creates `{}`", entity.names.sql_table),
        });
        next.push("jails sync".to_string());
    }
    if !drifted.is_empty() {
        next.push("jails sync".to_string());
    }
    // **A preserved retirement has one way back, and it is exact.** The table
    // is still there, so reviving it means naming that table -- and a reader
    // looking at `state: retired` has no other way to learn which spelling the
    // command insists on.
    if !entity.active && stored {
        next.push(format!(
            "jails resource revive {} --table {}",
            entity.names.java_type, entity.names.sql_table
        ));
    }
    next.dedup();

    // **The database is asked last and can only widen the answer.** Everything
    // above is a question about files this project owns; the live authority is
    // a question about a machine somewhere else, and a project whose files are
    // all correct can still be running against a schema that has not caught up.
    let live_state = live.map(|live| match stored {
        false => Authority::Absent,
        true => match live.tables.contains(&entity.names.sql_table) {
            true => Authority::Present,
            false => Authority::Absent,
        },
    });
    if let (Some(live), Some(Authority::Absent)) = (live, live_state)
        && stored
    {
        let unapplied = migrations
            .iter()
            .filter(|version| !live.applied.contains(*version))
            .cloned()
            .collect::<Vec<_>>();
        findings.push(Finding {
            code: "live-table-missing",
            message: match unapplied.is_empty() {
                true => format!(
                    "the database has no `{}`, and every migration that would create it is already recorded as applied",
                    entity.names.sql_table
                ),
                false => format!(
                    "the database has no `{}`; migration(s) {} are on disk and not applied",
                    entity.names.sql_table,
                    unapplied.join(", ")
                ),
            },
        });
        next.push("jails migrate".to_string());
    }
    next.dedup();
    let live = live_state.unwrap_or(Authority::Unknown);

    let state = match (entity.active, accepted_matches, drifted.is_empty()) {
        (false, _, _) => Consistency::Retired,
        (true, false, _) => Consistency::Pending,
        (true, true, false) => Consistency::Drifted,
        (true, true, true) => match live {
            Authority::Absent if stored => Consistency::RuntimeSchemaBehind,
            _ => Consistency::Consistent,
        },
    };

    Report {
        resource: Some(entity.names.java_type.clone()),
        state,
        declaration: Authority::Present,
        generated,
        migration_history,
        live,
        table: stored.then(|| entity.names.sql_table.clone()),
        generated_files,
        migrations,
        findings,
        next,
    }
}

/// Match by Java type or by model label, case-insensitively.
///
/// A reader types the name they see in their editor, which is the Java type;
/// the model's own label is lower camel. Accepting both is what keeps
/// `jails resource status order` and `jails resource status Order` the same
/// question.
fn find<'a>(model: &'a AppModel, selector: &str) -> Option<&'a jails_model::Entity> {
    model.entities.values().find(|entity| {
        entity.names.java_type.eq_ignore_ascii_case(selector)
            || entity.label.eq_ignore_ascii_case(selector)
    })
}

/// Does this migration name the table, as a statement rather than as a
/// substring? `orders` must not match `order_lines`.
fn mentions_table(bytes: &[u8], table: &str) -> bool {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return false;
    };
    let lowered = text.to_ascii_lowercase();
    let table = table.to_ascii_lowercase();
    lowered
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .any(|word| word == table)
}

fn render_human(report: &Report) -> String {
    let mut out = format!(
        "resource: {}\nstate: {}\n",
        report.resource.as_deref().unwrap_or("unknown"),
        report.state.label()
    );
    out.push_str(&format!(
        "declaration: {}\ngenerated: {}\nmigration-history: {}\nlive: {}\n",
        report.declaration.label(),
        report.generated.label(),
        report.migration_history.label(),
        report.live.label()
    ));
    if let Some(table) = &report.table {
        out.push_str(&format!("table: {table}\n"));
    }
    for path in &report.generated_files {
        out.push_str(&format!("file: {path}\n"));
    }
    for version in &report.migrations {
        out.push_str(&format!("migration: {version}\n"));
    }
    for finding in &report.findings {
        out.push_str(&format!("finding: {}: {}\n", finding.code, finding.message));
    }
    for command in &report.next {
        out.push_str(&format!("next: {command}\n"));
    }
    out
}

fn render_json(report: &Report) -> serde_json::Value {
    json!({
        "schema": SCHEMA,
        "resource": report.resource,
        "state": report.state.label(),
        "declaration": report.declaration.label(),
        "generated": report.generated.label(),
        "migrationHistory": report.migration_history.label(),
        "live": report.live.label(),
        "table": report.table,
        "files": report.generated_files,
        "migrations": report.migrations,
        "findings": report
            .findings
            .iter()
            .map(|finding| json!({"code": finding.code, "message": finding.message}))
            .collect::<Vec<_>>(),
        "next": report.next,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_table_name_is_matched_as_a_word_and_not_as_a_substring() {
        let sql = b"create table order_lines (id uuid primary key);";
        assert!(!mentions_table(sql, "orders"));
        assert!(mentions_table(sql, "order_lines"));
    }

    #[test]
    fn a_migration_names_its_table_whatever_the_surrounding_punctuation() {
        assert!(mentions_table(b"create table orders(id uuid);", "orders"));
        assert!(mentions_table(
            b"alter table \"orders\" add column x int;",
            "orders"
        ));
        assert!(mentions_table(b"CREATE TABLE ORDERS (id uuid);", "orders"));
    }

    #[test]
    fn bytes_that_are_not_text_report_no_table_rather_than_panicking() {
        assert!(!mentions_table(&[0xff, 0xfe, 0x00], "orders"));
    }
}
