//! Declared relations, as the constraints they are.
//!
//! Split out of `emit_sql.rs` by secret when the relation pass took that file
//! past the largest-module target. Everything here answers one question --
//! what foreign key does this relation mean, and where in the migration does
//! it go -- and the answer to the second half is *last*, which is the part a
//! reader is most likely to get wrong.

use super::{AppModel, BTreeSet, CompileError};
use jails_model::StableId as _;

/// Append one `alter table ... add constraint` per newly declared relation.
///
/// **After every `create table`, always.** A foreign key naming a table that
/// has not been created yet is a migration that fails on its first run, and
/// the entity pass walks a `BTreeMap` by stable id -- nothing about that order
/// is a dependency order, so an inline table constraint would work or not
/// depending on how the ids happened to sort.
pub(super) fn derive_into(
    removals: &std::collections::BTreeMap<String, String>,
    next: &AppModel,
    previous: &AppModel,
    statements: &mut Vec<String>,
    semantic_ids: &mut BTreeSet<String>,
    descriptions: &mut Vec<String>,
) -> Result<(), CompileError> {
    for relation in next.relations.values() {
        if previous.relations.contains_key(&relation.id) {
            continue;
        }
        statements.push(add_foreign_key(next, relation)?);
        semantic_ids.insert(relation.id.as_str().to_string());
        descriptions.push(format!("add_{}", relation.sql_name));
    }
    for old in previous.relations.values() {
        if next.relations.contains_key(&old.id) {
            continue;
        }
        // The same policy indexes have, for the same reason: dropping a
        // constraint is a forward migration somebody has to mean, and
        // inferring one from a deleted declaration is how a production
        // invariant disappears in a routine edit. `RemoveRelation` is how the
        // reader says they meant it, and it names the accepted constraint --
        // so the drop below is never inferred, only confirmed.
        let Some(confirmed) = removals.get(old.id.as_str()) else {
            return Err(CompileError::new(format!(
                "accepted foreign key `{}` was removed without a retirement policy\n       fix: run `jails destroy association {}`, or drop the constraint in a migration you write",
                old.sql_name, old.label
            )));
        };
        let child = next
            .entities
            .get(&old.child)
            .or_else(|| previous.entities.get(&old.child))
            .ok_or_else(|| {
                CompileError::new(format!(
                    "relation `{}` names a missing child entity\n       fix: repair the linked model before compiling",
                    old.label
                ))
            })?;
        statements.push(format!(
            "alter table {}\n  drop constraint {confirmed};",
            child.names.sql_table
        ));
        semantic_ids.insert(old.id.as_str().to_string());
        descriptions.push(format!("drop_{confirmed}"));
        continue;
    }
    Ok(())
}

/// One relation as the constraint it is.
///
/// `on delete` and `on update` come from the declaration rather than being
/// fixed at `no action` the way the legacy generator writes them: the model
/// carries the reader's answer, and emitting `no action` over a declared
/// `cascade` would be a schema that disagrees with the model it was compiled
/// from.
fn add_foreign_key(
    model: &AppModel,
    relation: &jails_model::Relation,
) -> Result<String, CompileError> {
    let child = model.entities.get(&relation.child).ok_or_else(|| {
        CompileError::new(format!(
            "relation `{}` names a missing child entity\n       fix: repair the linked model before compiling",
            relation.label
        ))
    })?;
    let parent = model.entities.get(&relation.parent).ok_or_else(|| {
        CompileError::new(format!(
            "relation `{}` names a missing parent entity\n       fix: repair the linked model before compiling",
            relation.label
        ))
    })?;
    let mut local = Vec::new();
    let mut remote = Vec::new();
    for mapping in &relation.mappings {
        local.push(column_of(child, &mapping.local, &relation.label)?);
        remote.push(column_of(parent, &mapping.remote, &relation.label)?);
    }
    Ok(format!(
        "alter table {}\n  add constraint {}\n  foreign key ({}) references {} ({})\n  on delete {} on update {};",
        child.names.sql_table,
        relation.sql_name,
        local.join(", "),
        parent.names.sql_table,
        remote.join(", "),
        referential_action(relation.on_delete),
        referential_action(relation.on_update),
    ))
}

fn column_of(
    entity: &jails_model::Entity,
    field: &jails_model::FieldId,
    relation: &str,
) -> Result<String, CompileError> {
    entity
        .fields
        .iter()
        .find(|candidate| candidate.id == *field)
        .map(|candidate| candidate.names.sql_column.clone())
        .ok_or_else(|| {
            CompileError::new(format!(
                "relation `{relation}` maps missing field `{field}`\n       fix: repair the linked model before compiling"
            ))
        })
}

fn referential_action(action: jails_model::ReferentialAction) -> &'static str {
    match action {
        jails_model::ReferentialAction::Restrict => "restrict",
        jails_model::ReferentialAction::Cascade => "cascade",
        jails_model::ReferentialAction::SetNull => "set null",
    }
}

#[cfg(test)]
mod relation_tests {
    use jails_contracts::{BuildSystem, WorkspaceSnapshot};

    const MODEL: &str = "jdl 1\n\
app Demo {\n pkg com.example.demo\n java 26\n platform spring\n build maven\n storage postgres\n}\n\
entity Author {\n id: uuid @pk\n name: string\n}\n\
entity Book {\n id: uuid @pk\n authorId: uuid\n title: string\n \
relation author to Author {\n  map authorId -> id\n  on delete cascade\n }\n}\n\
use repo for Author\nuse repo for Book\n";

    fn first_migration(source: &str) -> String {
        let model = jails_model::parse_jdl(source).expect("the fixture parses");
        let mut snapshot = WorkspaceSnapshot::detached(model);
        snapshot.project.build_system = BuildSystem::Maven;
        snapshot.project.spring_boot = Some("4.1.0".to_string());
        let draft = crate::Compiler::compile(&snapshot, None).expect("the fixture compiles");
        String::from_utf8(
            draft
                .migrations
                .first()
                .expect("a first migration")
                .bytes
                .clone(),
        )
        .expect("migrations are utf-8")
    }

    /// A declared relation becomes a constraint, rather than nothing at all.
    ///
    /// Linking `AppModel.relations` without emitting one leaves `sync`
    /// reporting success over a `book.author_id` that references no table. A
    /// declaration that reports success and produces no artifact is the same
    /// silence `bugs.md` B59 describes, arriving from the other side.
    #[test]
    fn a_declared_relation_becomes_a_foreign_key() {
        let sql = first_migration(MODEL);
        assert!(
            sql.contains("foreign key (author_id) references authors (id)"),
            "{sql}"
        );
        assert!(sql.contains("add constraint fk_books_author"), "{sql}");
    }

    /// The declared action is honoured, not replaced by a fixed one.
    ///
    /// The legacy generator writes `on delete no action` always, because the
    /// CLI has nowhere to say otherwise. The model does, and emitting the
    /// fixed answer over a declared one would be a schema that disagrees with
    /// what it was compiled from.
    #[test]
    fn the_declared_referential_action_reaches_the_constraint() {
        assert!(
            first_migration(MODEL).contains("on delete cascade"),
            "{}",
            first_migration(MODEL)
        );
    }

    /// Every constraint comes after every `create table`, so the order the
    /// entity pass happens to walk in cannot make a migration that fails on
    /// its first run.
    ///
    /// `Book` sorts before `Author` in nothing here by luck -- the pass walks
    /// a `BTreeMap` by stable id, and nothing about that order is a dependency
    /// order.
    #[test]
    fn constraints_come_after_every_create_table() {
        let sql = first_migration(MODEL);
        let last_create = sql.rfind("create table").expect("two creates");
        let constraint = sql.find("add constraint").expect("one constraint");
        assert!(
            constraint > last_create,
            "a foreign key precedes a create table, which fails on first run:\n{sql}"
        );
    }
}
