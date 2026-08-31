//! What jails writes to *prove* a linked operation's JDBC adapter.
//!
//! **The adapter is SQL, and SQL is not proved by compiling.** A canonical
//! project's generated `select` reached no test at all: the unit tests build
//! records and drive controllers with a faked port, so every column name,
//! every bind parameter and every join in `emit_operation` was asserted by
//! nothing. That is how a `--via` query came to emit no join and silently drop
//! its filter -- the code compiled, the controller test passed, and the query
//! answered over every row.
//!
//! So each adapter gets one `@SpringBootTest` integration test that stores a
//! row through the entity's own repository and reads it back through the
//! operation. It runs against the real PostgreSQL `add db` wires, which is the
//! only place `cast(:x as text) is null`, a foreign key, and a quoted join
//! alias mean anything.
//!
//! Split from `query.rs` and its siblings by secret, the same cut
//! `spring/query/proof.rs` records for the engine this replaces: those modules
//! decide the SQL, and this one decides what a test of it looks like. The fact
//! a proof turns on -- which values the filter must match -- is one the adapter
//! already resolved, and resolving it twice is how the two drifted.

use super::{QueryFilter, scopes};
use crate::CompileError;
use crate::emit_companion_test::JAVA_TEST_ROOT;
use crate::emit_java::{domain_import, import_declared_type, render};
use jails_contracts::{FileKind, FileMode, ProjectPath, Provenance, RenderedFile};
use jails_model::{AppModel, Entity, FieldId, Join, Operation, Package, StableId};
use std::collections::{BTreeMap, BTreeSet};

/// The integration test beside a query's JDBC adapter.
pub(super) fn query(
    model: &AppModel,
    capability_id: &str,
    operation: &Operation,
    target: &Entity,
    filters: &[QueryFilter<'_>],
    joins: &[Join],
) -> Result<Option<(ProjectPath, RenderedFile)>, CompileError> {
    let package = model.project.package_for(Package::AdaptersJdbc);
    let type_name = format!("Jdbc{}QueryIT", operation.names.java_type);
    let port_type = format!("{}Query", operation.names.java_type);
    let mut imports = BTreeSet::from([
        "org.junit.jupiter.api.Test".to_string(),
        "org.springframework.beans.factory.annotation.Autowired".to_string(),
        "org.springframework.boot.test.context.SpringBootTest".to_string(),
        "org.springframework.transaction.annotation.Transactional".to_string(),
        "static org.assertj.core.api.Assertions.assertThat".to_string(),
        domain_import(model, target),
        format!(
            "{}.{port_type}",
            model.project.package_for(Package::ApplicationQueries)
        ),
        format!(
            "{}.{}Repository",
            model.project.package_for(Package::Repository),
            target.names.java_type
        ),
    ]);

    // The row the query has to find. Every filter is bound to the *same*
    // sample the stored row carries, so the two cannot drift into a test that
    // stores one value and asks for another.
    let mut substitutions = BTreeMap::new();
    // **Every parent row the database will insist on**, not only the ones the
    // query joins: a declared relation is a foreign key, so storing the child
    // without its parent fails on the constraint rather than on the query. A
    // join and a relation can name the same parent, so the second is dropped.
    let mut parents: Vec<ParentRow> = joins
        .iter()
        .map(|join| ParentRow {
            entity: join.entity.clone(),
            alias: join.alias.clone(),
            mappings: join
                .mappings
                .iter()
                .map(|mapping| (mapping.local.clone(), mapping.remote.clone()))
                .collect(),
        })
        .collect();
    for relation in model.relations.values() {
        if relation.child != target.id
            || parents
                .iter()
                .any(|parent| parent.entity == relation.parent)
        {
            continue;
        }
        parents.push(ParentRow {
            entity: relation.parent.clone(),
            alias: relation.label.clone(),
            mappings: relation
                .mappings
                .iter()
                .map(|mapping| (mapping.local.clone(), mapping.remote.clone()))
                .collect(),
        });
    }
    let mut setup = String::new();
    let mut autowired = String::new();
    for ParentRow {
        entity: entity_id,
        alias,
        mappings,
    } in &parents
    {
        let parent = model.entities.get(entity_id).ok_or_else(|| {
            CompileError::new(format!(
                "linked query `{}` references missing entity `{}`",
                operation.label, entity_id
            ))
        })?;
        let Some(parent_arguments) =
            record_arguments(model, parent, &BTreeMap::new(), &mut imports)
        else {
            return Ok(None);
        };
        let variable = format!("{}Row", lower_first(alias));
        imports.insert(domain_import(model, parent));
        imports.insert(format!(
            "{}.{}Repository",
            model.project.package_for(Package::Repository),
            parent.names.java_type
        ));
        autowired.push_str(&format!(
            "    @Autowired\n    private {}Repository {}Repository;\n\n",
            parent.names.java_type,
            lower_first(&parent.names.java_type)
        ));
        setup.push_str(&format!(
            "        {} {variable} = {}Repository.save(new {}({parent_arguments}));\n",
            parent.names.java_type,
            lower_first(&parent.names.java_type),
            parent.names.java_type
        ));
        // **The assigned key, not a sample.** The child's foreign key has to
        // be the value the database gave the parent, or the join finds nothing
        // and the test proves the opposite of what it says.
        for (local, remote) in mappings {
            let Some(remote) = parent.field(remote) else {
                return Ok(None);
            };
            substitutions.insert(
                local.clone(),
                format!("{variable}.{}()", remote.names.java_member),
            );
        }
    }

    let Some(stored) = record_arguments(model, target, &substitutions, &mut imports) else {
        return Ok(None);
    };
    let Some(input_arguments) = filter_arguments(model, filters, &substitutions, &mut imports)
    else {
        return Ok(None);
    };
    // **A scoped query reads its tenant from the execution context**, whose
    // claims are strings the caller proves -- so the value the filter needs is
    // the string form of whatever the stored row carries, and jails has no way
    // to spell that for every scopeable type. Emitted whole and `@Disabled`
    // rather than omitted: the class still compiles, so it keeps working as
    // the shape to fill in, and nothing is dropped in silence.
    let scoped = !scopes(target).is_empty();
    let (context_argument, disabled, class_disabled) = if scoped {
        imports.insert("org.junit.jupiter.api.Disabled".to_string());
        imports.insert("java.util.Map".to_string());
        imports.insert(format!(
            "{}.ExecutionContext",
            model.project.package_for(Package::Application)
        ));
        (
            "new ExecutionContext(Map.of()), ",
            "    @Disabled(\"todo: supply the scope claims this query proves against, then delete this @Disabled\")\n",
            "",
        )
    } else {
        ("", "", "")
    };
    let _ = class_disabled;
    let body = format!(
        "@SpringBootTest\n@Transactional\nclass {type_name} {{\n\n    @Autowired\n    private {}Repository repository;\n\n{autowired}    @Autowired\n    private {port_type} query;\n\n    @Test\n{disabled}    void answersFromTheRealDatabase() {{\n{setup}        // The *stored* row, not the argument: with a database-assigned key\n        // the two differ by exactly the column the query reads back.\n        {} stored = repository.save(new {}({stored}));\n\n        var found = query.execute({context_argument}new {port_type}.Input({input_arguments}));\n\n        assertThat(found).contains(stored);\n    }}\n\n    // Reader-owned cases belong below this stable boundary.\n}}",
        target.names.java_type, target.names.java_type, target.names.java_type
    );
    let artifact_id = format!("art_{}_query_it", operation.id.as_str());
    // Spliced into the *rendered file*, not the class body: the splice places
    // the annotation above the type and its import beside the others, so it
    // needs the imports to already be there.
    let rendered = crate::emit_capability::imported_test_container(
        model,
        &package,
        render(&package, &imports, &body, &artifact_id),
    );
    let path = ProjectPath::parse(format!(
        "{JAVA_TEST_ROOT}/{}/{type_name}.java",
        package.replace('.', "/")
    ))
    .map_err(CompileError::new)?;
    Ok(Some((
        path,
        RenderedFile {
            kind: FileKind::JavaTest,
            mode: FileMode::Regular,
            bytes: rendered.into_bytes(),
            provenance: Provenance {
                artifact_id,
                ejection_id: None,
                ejectable: true,
                semantic_ids: BTreeSet::from([
                    capability_id.to_string(),
                    operation.id.as_str().to_string(),
                    target.id.as_str().to_string(),
                ]),
                compiler_pass: "capability-db-query".to_string(),
            },
        },
    )))
}

/// A row that has to exist before the target row can be stored.
///
/// Both a join and a declared relation produce one, and both are the same
/// thing to the database: a foreign key whose parent must be there first.
struct ParentRow {
    entity: jails_model::EntityId,
    /// What the local variable holding the saved row is named after.
    alias: String,
    /// Child field to parent field, so the child can carry the key the
    /// database assigned rather than a sample.
    mappings: Vec<(FieldId, FieldId)>,
}

/// Every component of an entity, sampled. `None` when one cannot be.
fn record_arguments(
    model: &AppModel,
    entity: &Entity,
    substitutions: &BTreeMap<jails_model::FieldId, String>,
    imports: &mut BTreeSet<String>,
) -> Option<String> {
    entity
        .fields
        .iter()
        .map(|field| {
            if let Some(value) = substitutions.get(&field.id) {
                return Some(value.clone());
            }
            // The sample may name a type this file has not imported: an enum
            // component lives in `domain` and the proof lives in
            // `adapters.jdbc`.
            import_declared_type(model, &field.ty, imports);
            crate::emit_companion_test::sample(model, field, imports)
        })
        .collect::<Option<Vec<_>>>()
        .map(|arguments| arguments.join(", "))
}

/// The query's own arguments, matching the row that was just stored.
///
/// A filter on the target reads the same sample the stored row carries. A
/// filter on a joined entity reads the parent's, except for the mapped key,
/// which has to be the one the database assigned.
fn filter_arguments(
    model: &AppModel,
    filters: &[QueryFilter<'_>],
    substitutions: &BTreeMap<FieldId, String>,
    imports: &mut BTreeSet<String>,
) -> Option<String> {
    filters
        .iter()
        .map(|filter| {
            // A filter on a column the parent row supplied has to ask for the
            // value that was actually stored -- the key the database assigned,
            // not the sample the record would otherwise have carried.
            if let Some(value) = substitutions.get(&filter.field.id) {
                return Some(value.clone());
            }
            if filter.required {
                crate::emit_companion_test::sample(model, filter.field, imports)
            } else {
                imports.insert("java.util.Optional".to_string());
                Some("Optional.empty()".to_string())
            }
        })
        .collect::<Option<Vec<_>>>()
        .map(|arguments| arguments.join(", "))
}

fn lower_first(value: &str) -> String {
    let mut characters = value.chars();
    characters.next().map_or_else(String::new, |first| {
        first.to_ascii_lowercase().to_string() + characters.as_str()
    })
}
