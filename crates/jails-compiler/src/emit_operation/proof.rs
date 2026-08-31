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

    // The rows the database will insist on, and the keys it assigned them.
    let Some(fixtures) = parent_fixtures(model, operation, target, joins, &mut imports)? else {
        return Ok(None);
    };
    let Fixtures {
        setup,
        autowired,
        substitutions,
    } = fixtures;

    let Some(stored) = record_arguments(model, target, &substitutions, &mut imports) else {
        return Ok(None);
    };
    let Some(input_arguments) = filter_arguments(model, filters, &substitutions, &mut imports)
    else {
        return Ok(None);
    };
    // **A scoped query reads its tenant from the execution context**, whose
    // claims are strings the caller proves -- and the test is the caller. It
    // has just stored the row, so it knows exactly which tenant that row
    // belongs to: the claim is the scoped component of the value it saved,
    // as a string. This was `@Disabled` on the grounds that jails cannot spell
    // the value for every scopeable type, which is true of the *type* and not
    // of this test, where the row is in hand.
    let scoped = scopes(target);
    let (context_argument, disabled) = if scoped.is_empty() {
        (String::new(), "")
    } else {
        imports.insert("java.util.Map".to_string());
        imports.insert(format!(
            "{}.ExecutionContext",
            model.project.package_for(Package::Application)
        ));
        let entries = scoped
            .iter()
            .map(|field| {
                format!(
                    "Map.entry(\"{}\", String.valueOf(stored.{}()))",
                    field
                        .semantics
                        .scope
                        .as_ref()
                        .expect("scopes() filtered on Some")
                        .claim,
                    field.names.java_member
                )
            })
            .collect::<Vec<_>>();
        (
            format!(
                "new ExecutionContext(Map.ofEntries({})), ",
                entries.join(", ")
            ),
            "",
        )
    };
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

/// The parent rows a target row cannot be stored without, and their keys.
///
/// **Every declared relation, not only the joins.** A relation is a foreign
/// key, so storing the child without its parent fails on the constraint rather
/// than on the statement under test -- and a join and a relation can name the
/// same parent, so the second is dropped.
///
/// One owner because every operation proof needs the same rows: a query reads
/// one, a command writes one, a transition changes one. Written per kind they
/// would disagree about which parents exist, and the disagreement only shows
/// up against a real database.
struct Fixtures {
    setup: String,
    autowired: String,
    /// The child's foreign-key components, by the parent key they read.
    substitutions: BTreeMap<FieldId, String>,
}

fn parent_fixtures(
    model: &AppModel,
    operation: &Operation,
    target: &Entity,
    joins: &[Join],
    imports: &mut BTreeSet<String>,
) -> Result<Option<Fixtures>, CompileError> {
    // The row the query has to find. Every filter is bound to the *same*
    // sample the stored row carries, so the two cannot drift into a test that
    // stores one value and asks for another.
    let mut substitutions = BTreeMap::new();
    let mut setup = String::new();
    let mut autowired = String::new();
    // **Every ancestor is stored once, deepest first, and shared.** Two things
    // needed that. A parent has parents of its own -- storing a `Contact`
    // without its `Workspace` fails on the constraint before the child is
    // reached. And `Conversation` references `Contact` by
    // `(workspace_id, contact_id)` and `Inbox` by `(workspace_id, inbox_id)`,
    // so a workspace sampled separately per branch left the child carrying one
    // parent's workspace and the other parent's id. Keyed by entity, the
    // second reference to a workspace is the row the first one stored.
    let mut stored: BTreeMap<jails_model::EntityId, String> = BTreeMap::new();
    let direct = ancestry(model, target, joins);
    for entity_id in store_order(model, &direct) {
        if stored.contains_key(&entity_id) {
            continue;
        }
        let parent = model.entities.get(&entity_id).ok_or_else(|| {
            CompileError::new(format!(
                "linked query `{}` references missing entity `{entity_id}`",
                operation.label
            ))
        })?;
        let mut inherited = BTreeMap::new();
        for ancestor in ancestry(model, parent, &[]) {
            let (Some(row), Some(grandparent)) = (
                stored.get(&ancestor.entity),
                model.entities.get(&ancestor.entity),
            ) else {
                continue;
            };
            for (local, remote) in &ancestor.mappings {
                let Some(remote) = grandparent.field(remote) else {
                    return Ok(None);
                };
                inherited.insert(
                    local.clone(),
                    format!("{row}.{}()", remote.names.java_member),
                );
            }
        }
        let Some(arguments) = record_arguments(model, parent, &inherited, imports) else {
            return Ok(None);
        };
        // Named after the entity rather than the relation: one row per entity
        // is the whole point, and two relations naming it would otherwise ask
        // for two variables holding the same thing.
        let variable = format!("{}Row", lower_first(&parent.names.java_type));
        let repository = lower_first(&parent.names.java_type);
        imports.insert(domain_import(model, parent));
        imports.insert(format!(
            "{}.{}Repository",
            model.project.package_for(Package::Repository),
            parent.names.java_type
        ));
        autowired.push_str(&format!(
            "    @Autowired\n    private {}Repository {repository}Repository;\n\n",
            parent.names.java_type
        ));
        setup.push_str(&format!(
            "        {} {variable} = {repository}Repository.save(new {}({arguments}));\n",
            parent.names.java_type, parent.names.java_type
        ));
        stored.insert(entity_id, variable);
    }
    // **The assigned key, not a sample.** The child's foreign key has to be
    // the value the database gave the parent, or the join finds nothing and
    // the test proves the opposite of what it says.
    for parent in &direct {
        let (Some(variable), Some(entity)) = (
            stored.get(&parent.entity),
            model.entities.get(&parent.entity),
        ) else {
            return Ok(None);
        };
        for (local, remote) in &parent.mappings {
            let Some(remote) = entity.field(remote) else {
                return Ok(None);
            };
            substitutions.insert(
                local.clone(),
                format!("{variable}.{}()", remote.names.java_member),
            );
        }
    }

    Ok(Some(Fixtures {
        setup,
        autowired,
        substitutions,
    }))
}

/// The ancestor closure in the order rows have to be written.
///
/// Post-order depth-first, so a grandparent is stored before the parent that
/// references it. The visited set is what makes a diamond -- two parents
/// sharing one grandparent -- one row rather than two, and what stops a
/// relation cycle from recursing forever.
fn store_order(model: &AppModel, direct: &[ParentRow]) -> Vec<jails_model::EntityId> {
    fn visit(
        model: &AppModel,
        entity_id: &jails_model::EntityId,
        seen: &mut BTreeSet<jails_model::EntityId>,
        order: &mut Vec<jails_model::EntityId>,
    ) {
        if !seen.insert(entity_id.clone()) {
            return;
        }
        if let Some(entity) = model.entities.get(entity_id) {
            for parent in ancestry(model, entity, &[]) {
                visit(model, &parent.entity, seen, order);
            }
        }
        order.push(entity_id.clone());
    }
    let mut seen = BTreeSet::new();
    let mut order = Vec::new();
    for parent in direct {
        visit(model, &parent.entity, &mut seen, &mut order);
    }
    order
}

/// Every parent row the database will insist on, joins first.
///
/// Not only the ones a query joins: a declared relation is a foreign key, so
/// storing the child without its parent fails on the constraint rather than on
/// the query. A join and a relation can name the same parent, and the join's
/// alias is the one the test reads, so the relation is dropped.
fn ancestry(model: &AppModel, child: &Entity, joins: &[Join]) -> Vec<ParentRow> {
    let mut parents: Vec<ParentRow> = joins
        .iter()
        .map(|join| ParentRow {
            entity: join.entity.clone(),
            mappings: join
                .mappings
                .iter()
                .map(|mapping| (mapping.local.clone(), mapping.remote.clone()))
                .collect(),
        })
        .collect();
    for relation in model.relations.values() {
        if relation.child != child.id
            || parents
                .iter()
                .any(|parent| parent.entity == relation.parent)
        {
            continue;
        }
        parents.push(ParentRow {
            entity: relation.parent.clone(),
            mappings: relation
                .mappings
                .iter()
                .map(|mapping| (mapping.local.clone(), mapping.remote.clone()))
                .collect(),
        });
    }
    parents
}

/// The integration test beside a command's or transition's JDBC adapter.
///
/// **The write half of an operation reached no test at all.** A query's proof
/// stores a row and reads it back; a command's `insert ... returning` and a
/// transition's `update ... returning` were asserted by nothing, so a column
/// list that drifted from the row mapper, a bind the driver will not take, and
/// an `on conflict` naming a column with no unique index all compiled and
/// shipped.
///
/// What it asserts is that the statement runs and answers. It deliberately
/// does not assert *which* row: a command's returned key is the database's,
/// and a transition's audit columns are `current_timestamp`, so pinning
/// equality would fail on the shape of the schema rather than on the
/// correctness of the statement.
///
/// A transition needs a row to change, so it stores one first through the
/// entity's own repository; a command writes its own.
pub(super) fn write(
    model: &AppModel,
    capability_id: &str,
    operation: &Operation,
    target: &Entity,
    shape: WriteShape<'_>,
) -> Result<Option<(ProjectPath, RenderedFile)>, CompileError> {
    let WriteShape {
        port_suffix,
        port_package,
        keyed,
        inputs,
    } = shape;
    let package = model.project.package_for(Package::AdaptersJdbc);
    let port_type = format!("{}{port_suffix}", operation.names.java_type);
    let type_name = format!("Jdbc{port_type}IT");
    let mut imports = BTreeSet::from([
        "org.junit.jupiter.api.Test".to_string(),
        "org.springframework.beans.factory.annotation.Autowired".to_string(),
        "org.springframework.boot.test.context.SpringBootTest".to_string(),
        "org.springframework.transaction.annotation.Transactional".to_string(),
        "static org.assertj.core.api.Assertions.assertThat".to_string(),
        domain_import(model, target),
        format!("{}.{port_type}", model.project.package_for(port_package)),
    ]);

    let Some(fixtures) = parent_fixtures(model, operation, target, &[], &mut imports)? else {
        return Ok(None);
    };
    let Fixtures {
        mut setup,
        mut autowired,
        substitutions,
    } = fixtures;

    let record = &target.names.java_type;
    // **The claims this operation proves against, spelled by the test.** The
    // context is a map of claim to string, and the caller is what proves it --
    // here that caller is the test, which either stored the row (so it knows
    // the tenant) or is about to create one (so it chooses the tenant). This
    // was skipped entirely on the grounds that jails cannot spell the value
    // for every scopeable type, which is true of the *type* and not of a test
    // that has the value in hand.
    let scoped = scopes(target);
    let context_argument;
    if !scoped.is_empty() {
        imports.insert("java.util.Map".to_string());
        imports.insert(format!(
            "{}.ExecutionContext",
            model.project.package_for(Package::Application)
        ));
    }
    // A transition changes a row that has to be there; a command makes its own.
    let invocation = if let Some(key) = keyed {
        let Some(stored) = record_arguments(model, target, &substitutions, &mut imports) else {
            return Ok(None);
        };
        imports.insert(format!(
            "{}.{record}Repository",
            model.project.package_for(Package::Repository)
        ));
        autowired.push_str(&format!(
            "    @Autowired\n    private {record}Repository repository;\n\n"
        ));
        setup.push_str(&format!(
            "        {record} stored = repository.save(new {record}({stored}));\n"
        ));
        context_argument = claims(&scoped, |field| {
            format!("String.valueOf(stored.{}())", field.names.java_member)
        });
        format!(
            "operation.execute({context_argument}stored.{}(), new {port_type}.Input({{arguments}}))",
            key.names.java_member
        )
    } else {
        let mut chosen = Vec::new();
        for field in &scoped {
            let Some(value) = crate::emit_companion_test::sample(model, field, &mut imports) else {
                return Ok(None);
            };
            chosen.push((field.names.java_member.clone(), value));
        }
        context_argument = claims(&scoped, |field| {
            let value = chosen
                .iter()
                .find(|(member, _)| member == &field.names.java_member)
                .map(|(_, value)| value.clone())
                .expect("every scoped field was sampled above");
            format!("String.valueOf({value})")
        });
        format!("operation.execute({context_argument}new {port_type}.Input({{arguments}}))")
    };
    let Some(arguments) = input_arguments(model, target, inputs, &substitutions, &mut imports)
    else {
        return Ok(None);
    };
    let invocation = invocation.replace("{arguments}", &arguments);

    let body = format!(
        "@SpringBootTest\n@Transactional\nclass {type_name} {{\n\n{autowired}    @Autowired\n    private {port_type} operation;\n\n    @Test\n    void writesThroughTheRealDatabase() {{\n{setup}        {record} answered = {invocation};\n\n        // `returning` answers with the row the statement wrote, so a null\n        // here means it matched none -- which is the failure worth catching.\n        assertThat(answered).isNotNull();\n    }}\n\n    // Reader-owned cases belong below this stable boundary.\n}}"
    );
    let artifact_id = format!("art_{}_write_it", operation.id.as_str());
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
                compiler_pass: "capability-db-write".to_string(),
            },
        },
    )))
}

/// The `ExecutionContext` argument, built from a value the test can name.
///
/// One helper because both halves of the write proof need it and they name the
/// value differently: a transition reads it off the row it stored, a command
/// chooses it and the insert writes it.
fn claims(scoped: &[&jails_model::Field], value: impl Fn(&jails_model::Field) -> String) -> String {
    let entries = scoped
        .iter()
        .map(|field| {
            format!(
                "Map.entry(\"{}\", {})",
                field
                    .semantics
                    .scope
                    .as_ref()
                    .expect("scopes() filtered on Some")
                    .claim,
                value(field)
            )
        })
        .collect::<Vec<_>>();
    match entries.is_empty() {
        true => String::new(),
        false => format!(
            "new ExecutionContext(Map.ofEntries({})), ",
            entries.join(", ")
        ),
    }
}

/// The fields this operation's `Input` declares, or `None` when the test
/// cannot be built from them.
///
/// The flat field list when there is one; otherwise the linked parameters,
/// resolved back to the target's own fields. A parameter that is not a field
/// of the target -- a joined filter, a typed component of the operation's own
/// -- has no sample this proof can reach, so it emits nothing rather than a
/// constructor call that will not compile.
pub(super) fn input_fields<'a>(
    target: &'a Entity,
    flat: &[FieldId],
    parameters: &[jails_model::OperationParameter],
) -> Option<Vec<&'a jails_model::Field>> {
    if parameters.is_empty() {
        return flat.iter().map(|id| target.field(id)).collect();
    }
    parameters
        .iter()
        .map(|parameter| match &parameter.source {
            jails_model::ParameterSource::Field(visible) if visible.entity == target.id => {
                target.field(&visible.field)
            }
            _ => None,
        })
        .collect()
}

/// What a write proof needs to know about the port it drives.
pub(super) struct WriteShape<'a> {
    pub(super) port_suffix: &'a str,
    pub(super) port_package: Package,
    /// Whether `execute` takes the row's key before its input.
    /// The component the operation addresses its row by, when it changes an
    /// existing one. `None` for a command, which writes its own.
    pub(super) keyed: Option<&'a jails_model::Field>,
    /// The fields the `Input` record declares, in order.
    pub(super) inputs: &'a [&'a jails_model::Field],
}

/// The `Input` arguments, matching the row the fixtures stored.
///
/// A component reading a foreign key takes the parent's assigned value rather
/// than a sample, for the same reason the row does: a sampled key names a row
/// that is not there.
fn input_arguments(
    model: &AppModel,
    target: &Entity,
    inputs: &[&jails_model::Field],
    substitutions: &BTreeMap<FieldId, String>,
    imports: &mut BTreeSet<String>,
) -> Option<String> {
    let _ = target;
    inputs
        .iter()
        .map(|field| {
            if let Some(value) = substitutions.get(&field.id) {
                return Some(value.clone());
            }
            import_declared_type(model, &field.ty, imports);
            crate::emit_companion_test::sample(model, field, imports)
        })
        .collect::<Option<Vec<_>>>()
        .map(|arguments| arguments.join(", "))
}

/// A row that has to exist before the target row can be stored.
///
/// Both a join and a declared relation produce one, and both are the same
/// thing to the database: a foreign key whose parent must be there first.
struct ParentRow {
    entity: jails_model::EntityId,
    /// What the local variable holding the saved row is named after.
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
