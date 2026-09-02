//! Full-text search, as the generated column and index it is.
//!
//! Its own module beside `relation.rs` and for the same reason: one question,
//! answered in one place. What is *not* here is the port and its adapter --
//! those are Java, and `emit_java` owns them.
//!
//! The declaration carries the columns to index and that is load-bearing. A
//! `tsvector` over every text column indexes ids and status codes as if they
//! were prose, and the reader then cannot tell why a search for "active"
//! returns everything.

use super::{AppModel, BTreeSet, CompileError};
use jails_model::{ProjectionKind, StableId as _};

/// The text search configuration, named rather than left to
/// `default_text_search_config`, so the stemming a row was indexed under does
/// not change when a session or a server setting does.
pub(crate) const CONFIGURATION: &str = "english";

/// The generated column's name, one per searched table.
pub(crate) const COLUMN: &str = "search_vector";

/// Whether a search projection names this entity, and so whether the table
/// carries the generated column below.
///
/// Read off the model rather than off the emitted SQL: the column is one of
/// the table's, and anything listing what the table has has to include it.
pub(super) fn indexes(model: &AppModel, entity: &jails_model::Entity) -> bool {
    model.projections.values().any(|projection| {
        matches!(projection.kind, ProjectionKind::Search { .. }) && projection.entity == entity.id
    })
}

/// Append the column and index for every newly declared search projection.
pub(super) fn derive_into(
    next: &AppModel,
    previous: &AppModel,
    statements: &mut Vec<String>,
    semantic_ids: &mut BTreeSet<String>,
    descriptions: &mut Vec<String>,
) -> Result<(), CompileError> {
    for projection in next.projections.values() {
        let ProjectionKind::Search { fields } = &projection.kind else {
            continue;
        };
        if previous.projections.contains_key(&projection.id) {
            continue;
        }
        let entity = next.entities.get(&projection.entity).ok_or_else(|| {
            CompileError::new(
                "a search projection names a missing entity\n       fix: repair the linked model before compiling",
            )
        })?;
        let mut columns = Vec::new();
        for field in fields {
            let column = entity
                .fields
                .iter()
                .find(|candidate| candidate.id == *field)
                .ok_or_else(|| {
                    CompileError::new(format!(
                        "search on `{}` indexes missing field `{field}`\n       fix: repair the linked model before compiling",
                        entity.label
                    ))
                })?;
            columns.push(column.names.sql_column.clone());
        }
        if columns.is_empty() {
            return Err(CompileError::new(format!(
                "search on `{}` names no components to index\n       fix: list them, as `use search(fields: [title, body])` -- indexing every text column would index ids and status codes as prose",
                entity.label
            )));
        }
        let table = &entity.names.sql_table;
        // `coalesce(x, '')` around every column is not defensive noise: `||`
        // with a NULL operand yields NULL, so one null column would blank the
        // whole vector and the row would match nothing at all.
        let expression = format!(
            "to_tsvector('{CONFIGURATION}', {})",
            columns
                .iter()
                .map(|column| format!("coalesce({column}, '')"))
                .collect::<Vec<_>>()
                .join(" || ' ' || ")
        );
        // `generated always as (...) stored`, not a trigger. A trigger is the
        // older recipe with one silent failure: somebody adds an UPDATE path
        // that forgets it, the row's text changes, the vector does not, and
        // the row stops matching a search that should find it. Nothing errors.
        statements.push(format!(
            "alter table {table}\n    add column {COLUMN} tsvector\n    generated always as ({expression}) stored;"
        ));
        // GIN, not GiST: slower to build and faster to search, which is the
        // right trade for a column written once per row change and read on
        // every query.
        statements.push(format!(
            "create index {table}_{COLUMN}_idx on {table} using gin ({COLUMN});"
        ));
        semantic_ids.insert(projection.id.as_str().to_string());
        semantic_ids.insert(entity.id.as_str().to_string());
        descriptions.push(format!("add_search_to_{table}"));
    }
    for old in previous.projections.values() {
        if !matches!(old.kind, ProjectionKind::Search { .. })
            || next.projections.contains_key(&old.id)
        {
            continue;
        }
        // The policy indexes and relations already have: dropping a column is
        // a forward migration somebody has to mean.
        return Err(CompileError::new(
            "an accepted search column was removed without a retirement policy\n       fix: keep the search projection, or drop the column and index in a migration you write",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use jails_contracts::{BuildSystem, WorkspaceSnapshot};

    const MODEL: &str = "jdl 1\n\
app Demo {\n pkg com.example.demo\n java 26\n platform spring\n build maven\n storage postgres\n}\n\
entity Note {\n id: uuid @pk\n title: string\n body: string\n}\n\
use repo for Note\nuse search(fields: [title, body]) for Note\n";

    fn migration() -> String {
        let model = jails_model::parse_jdl(MODEL).expect("the fixture parses");
        let mut snapshot = WorkspaceSnapshot::detached(model);
        snapshot.project.build_system = BuildSystem::Maven;
        snapshot.project.spring_boot = Some("4.1.0".to_string());
        let draft = crate::Compiler::compile(
            &snapshot,
            &snapshot.model.model,
            &jails_model::Evolution::none(),
        )
        .expect("the fixture compiles");
        String::from_utf8(draft.migrations.first().expect("a migration").bytes.clone())
            .expect("migrations are utf-8")
    }

    /// A declared search projection becomes a column and an index.
    ///
    /// Emitting a port interface and nothing else leaves the reader a type to
    /// inject and no way to answer a query with it.
    #[test]
    fn a_search_projection_becomes_a_generated_column_and_a_gin_index() {
        let sql = migration();
        assert!(sql.contains("add column search_vector tsvector"), "{sql}");
        assert!(
            sql.contains("generated always as (to_tsvector('english',"),
            "{sql}"
        );
        assert!(
            sql.contains("using gin (search_vector)"),
            "GiST would be the wrong trade for a column read on every query: {sql}"
        );
    }

    /// Only the named components are indexed, and every one is coalesced.
    ///
    /// `||` with a NULL operand yields NULL, so one null column would blank
    /// the whole vector and the row would match nothing at all -- a failure
    /// that looks like "search is broken for some rows" and nothing else.
    #[test]
    fn every_indexed_column_is_coalesced_and_the_key_is_not_indexed() {
        let sql = migration();
        assert!(
            sql.contains("coalesce(title, '') || ' ' || coalesce(body, '')"),
            "{sql}"
        );
        assert!(
            !sql.contains("coalesce(id"),
            "indexing the key as prose is what naming the columns exists to prevent: {sql}"
        );
    }

    /// The column is added after the table exists.
    #[test]
    fn the_column_comes_after_its_create_table() {
        let sql = migration();
        let create = sql.find("create table").expect("a create");
        let column = sql.find("add column search_vector").expect("the column");
        assert!(column > create, "{sql}");
    }
}
