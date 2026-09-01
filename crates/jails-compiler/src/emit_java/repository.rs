//! How a repository adapter is written, and which one gets the bean.
//!
//! Split out of `emit_java.rs` under `plan.md` P13.3: that module was the
//! largest in the workspace and the ratchet had been red since it was written.
//! This is the cut that made sense rather than the one that made the number --
//! everything here answers one question, *what does storing this entity look
//! like in Java*, and nothing else in the emitter asks it.
//!
//! **Two adapters, and exactly one of them is a bean.** The in-memory one
//! exists so a generated project starts before anybody has run `add db`; the
//! JDBC one takes over when the starter is there. Annotating both would make
//! two beans qualify for one injection point, which is the ambiguity
//! `jails beans` reports and a scaffold that compiles and cannot run.

use super::*;
use jails_model::Package;

/// The in-memory adapter, and whether it is the project's repository bean.
///
/// **`bean` is true exactly when nothing else implements the port.** With `db`
/// declared the JDBC adapter carries `@Repository` and this one is a plain
/// class a test constructs; without it, this *is* the implementation, and
/// leaving it unannotated gave a scaffolded Spring project a service
/// constructor-injecting a port no bean satisfied. Annotating both would make
/// two beans qualify for one injection point, which is the ambiguity `jails
/// beans` reports.
pub(super) fn lower_fake_repository(
    model: &AppModel,
    capability_id: &str,
    entity: &Entity,
    bean: bool,
) -> Result<Unit, CompileError> {
    let primary_key = primary_key(entity)?;
    let package = crate::emit_java::entity_package(model, entity, Package::AdaptersMemory);
    let type_name = format!("InMemory{}Repository", entity.names.java_type);
    let repository = format!(
        "{}.{}Repository",
        crate::emit_java::entity_package(model, entity, Package::Repository),
        entity.names.java_type
    );
    let mut imports = BTreeSet::from([
        repository,
        domain_import(model, entity),
        "java.util.LinkedHashMap".to_string(),
        "java.util.List".to_string(),
        "java.util.Map".to_string(),
        "java.util.Optional".to_string(),
    ]);
    let key_type = java_type(primary_key, &mut imports);
    let record = &entity.names.java_type;
    let key = &primary_key.names.java_member;
    // **`@Component`, not `@Repository`.** The extra meaning `@Repository`
    // carries is persistence-exception translation, which registers a
    // post-processor that CGLIB-proxies every such bean -- and this class is
    // `final`, so the context dies with "Cannot subclass final class". There
    // is nothing here to translate either: a `LinkedHashMap` throws no
    // `SQLException`. The JDBC adapter keeps `@Repository`, where both halves
    // of that annotation are true.
    let annotation = if bean {
        imports.insert("org.springframework.stereotype.Component".to_string());
        "@Component\n"
    } else {
        ""
    };
    let body = format!(
        "{annotation}public final class {type_name} implements {record}Repository {{\n\n    private final Map<{key_type}, {record}> rows = new LinkedHashMap<>();\n\n    @Override\n    public Optional<{record}> findById({key_type} id) {{\n        return Optional.ofNullable(rows.get(id));\n    }}\n\n    @Override\n    public List<{record}> findAll() {{\n        return List.copyOf(rows.values());\n    }}\n\n    @Override\n    public {record} save({record} value) {{\n        rows.put(value.{key}(), value);\n        return value;\n    }}\n\n    @Override\n    public boolean deleteById({key_type} id) {{\n        return rows.remove(id) != null;\n    }}\n}}"
    );
    // **Keyed on the entity, not on whichever capability asked for it.** This
    // adapter is emitted whenever the port has no other implementation and
    // again when `fake` is declared, and the file is the same either way -- so
    // an id carrying the owner made `add fake` on a project that already had
    // the adapter look like a *different* artifact wanting a path the first one
    // held, which the executor correctly reports as reader-owned. The
    // capability still shows in `semantic_ids`.
    let artifact_id = format!("art_{}_repository_memory", entity.id.as_str());
    let rendered = render(&package, &imports, &body, &artifact_id);
    let package_path = package.replace('.', "/");
    let path = ProjectPath::parse(format!("{JAVA_ROOT}/{package_path}/{type_name}.java"))
        .map_err(CompileError::new)?;
    Ok(Unit {
        path,
        file: RenderedFile {
            kind: FileKind::JavaMain,
            mode: FileMode::Regular,
            bytes: rendered.into_bytes(),
            provenance: Provenance {
                artifact_id,
                ejection_id: None,
                ejectable: true,
                semantic_ids: BTreeSet::from([
                    capability_id.to_string(),
                    entity.id.as_str().to_string(),
                ]),
                compiler_pass: "capability-fake".to_string(),
            },
        },
    })
}

pub(super) fn lower_db_repository(
    model: &AppModel,
    capability_id: &str,
    entity: &Entity,
) -> Result<Unit, CompileError> {
    let primary_key = primary_key(entity)?;
    let package = crate::emit_java::entity_package(model, entity, Package::AdaptersJdbc);
    let type_name = format!("Jdbc{}Repository", entity.names.java_type);
    let repository = format!(
        "{}.{}Repository",
        crate::emit_java::entity_package(model, entity, Package::Repository),
        entity.names.java_type
    );
    let mut imports = BTreeSet::from([
        repository,
        domain_import(model, entity),
        "java.util.List".to_string(),
        "java.util.Optional".to_string(),
        "org.springframework.jdbc.core.simple.JdbcClient".to_string(),
        "org.springframework.stereotype.Repository".to_string(),
    ]);
    let record = &entity.names.java_type;
    let key_type = java_type(primary_key, &mut imports);
    let table = &entity.names.sql_table;
    let key_column = &primary_key.names.sql_column;
    let columns = entity
        .fields
        .iter()
        .map(|field| field.names.sql_column.as_str())
        .collect::<Vec<_>>();
    let column_list = columns.join(", ");
    // **A `generated always as identity` key is not writable, so `save` must
    // not name it.** PostgreSQL answers an insert that does with *"cannot
    // insert a non-DEFAULT value into column \"id\""*, which made `save`
    // impossible on every canonical entity whose key jails assigns -- the
    // default shape for `id:long@pk`. Nothing caught it because the generated
    // integration tests had never been run: Failsafe was not configured.
    //
    // Insert-only in that case, rather than an upsert on a key the caller
    // cannot have. That is what the row coming back is *for*: the stored row
    // and the argument differ by exactly the key the database assigned.
    let generated_key = crate::emit_sql::database_assigned(primary_key)?;
    let written = entity
        .fields
        .iter()
        .filter(|field| !(generated_key && field.id == primary_key.id))
        .map(|field| field.names.sql_column.as_str())
        .collect::<Vec<_>>();
    let written_list = written.join(", ");
    let values = written
        .iter()
        .map(|column| format!(":{column}"))
        .collect::<Vec<_>>()
        .join(", ");

    let updates = entity
        .fields
        .iter()
        .filter(|field| field.id != primary_key.id)
        .map(|field| {
            format!(
                "{} = excluded.{}",
                field.names.sql_column, field.names.sql_column
            )
        })
        .collect::<Vec<_>>();
    let updates = if updates.is_empty() {
        format!("{key_column} = excluded.{key_column}")
    } else {
        updates.join(", ")
    };
    let conflict = if generated_key {
        String::new()
    } else {
        format!(" on conflict ({key_column}) do update set {updates}")
    };
    // A loop rather than a `map`, because the write expression may need an
    // import of its own and a closure cannot borrow the set the surrounding
    // function is still filling.
    let mut params = String::new();
    for field in &entity.fields {
        if generated_key && field.id == primary_key.id {
            continue;
        }
        let member = &field.names.java_member;
        // Through the one write expression, because the PostgreSQL driver
        // cannot infer a type for every Java value the record can hold -- and
        // through the *optional* one when the component may be absent, because
        // a conversion applied after `orElse(null)` calls a method on null.
        let accessor = format!("value.{member}()");
        let value = match field.required {
            true => crate::emit_sql::bound_value(model, field, &accessor, &mut imports),
            false => crate::emit_sql::optional_bound_value(model, field, &accessor, &mut imports),
        };
        params.push_str(&format!(
            "\n                .param(\"{}\", {value})",
            field.names.sql_column
        ));
    }
    let body = format!(
        "@Repository\npublic final class {type_name} implements {record}Repository {{\n\n    private final JdbcClient jdbc;\n\n    public {type_name}(JdbcClient jdbc) {{\n        this.jdbc = jdbc;\n    }}\n\n    @Override\n    public Optional<{record}> findById({key_type} id) {{\n        return jdbc.sql(\"select {column_list} from {table} where {key_column} = :id\")\n                .param(\"id\", id)\n                .query({record}.class)\n                .optional();\n    }}\n\n    @Override\n    public List<{record}> findAll() {{\n        return jdbc.sql(\"select {column_list} from {table} order by {key_column}\")\n                .query({record}.class)\n                .list();\n    }}\n\n    @Override\n    public {record} save({record} value) {{\n        return jdbc.sql(\"insert into {table} ({written_list}) values ({values}){conflict} returning {column_list}\"){params}\n                .query({record}.class)\n                .single();\n    }}\n\n    @Override\n    public boolean deleteById({key_type} id) {{\n        return jdbc.sql(\"delete from {table} where {key_column} = :id\")\n                .param(\"id\", id)\n                .update() > 0;\n    }}\n}}",
        key_type = key_type,
    );
    let artifact_id = format!("art_{capability_id}_{}_repository", entity.id.as_str());
    let rendered = render(&package, &imports, &body, &artifact_id);
    let package_path = package.replace('.', "/");
    let path = ProjectPath::parse(format!("{JAVA_ROOT}/{package_path}/{type_name}.java"))
        .map_err(CompileError::new)?;
    Ok(Unit {
        path,
        file: RenderedFile {
            kind: FileKind::JavaMain,
            mode: FileMode::Regular,
            bytes: rendered.into_bytes(),
            provenance: Provenance {
                artifact_id,
                ejection_id: None,
                ejectable: true,
                semantic_ids: BTreeSet::from([
                    capability_id.to_string(),
                    entity.id.as_str().to_string(),
                ]),
                compiler_pass: "capability-db".to_string(),
            },
        },
    })
}

/// The search port's JDBC implementation.
///
/// Two details decide whether this works at all, and both are easy to undo.
///
/// **`websearch_to_tsquery`, not `to_tsquery`.** The latter demands operator
/// syntax and throws a syntax error on anything a person would actually type,
/// a bare two-word phrase included. The former accepts what a search box
/// produces -- quotes, `OR`, `-` -- and never throws on malformed input. A
/// search endpoint that 500s on an apostrophe is what that avoids.
///
/// **The query is a bind parameter.** It is text PostgreSQL parses, not SQL it
/// executes, so there is no injection surface and no escaping to get right.
/// The integration test for the adapter above.
///
/// **The only tier that answers the question this adapter exists for.** A
/// `JdbcClient` statement is a string until something runs it: a column list
/// that drifted, a type PostgreSQL will not accept, a `returning` clause that
/// names a column the insert does not write -- every one of them compiles, and
/// every one of them fails on the first real call. The unit tiers cannot see
/// any of it, and the adapter shipped with no test at all.
///
/// `None` when the entity has a component jails cannot sample: a guessed value
/// would not compile, and a test that constructs nothing proves nothing. That
/// is the same rule the record's own companion follows.
pub(super) fn lower_db_repository_it(
    model: &AppModel,
    capability_id: &str,
    entity: &Entity,
) -> Result<Option<Unit>, CompileError> {
    let primary_key = primary_key(entity)?;
    let package = crate::emit_java::entity_package(model, entity, Package::AdaptersJdbc);
    let type_name = format!("Jdbc{}RepositoryIT", entity.names.java_type);
    let record = &entity.names.java_type;
    let mut imports = BTreeSet::from([
        domain_import(model, entity),
        format!(
            "{}.{record}Repository",
            crate::emit_java::entity_package(model, entity, Package::Repository)
        ),
        "org.junit.jupiter.api.Test".to_string(),
        "org.springframework.beans.factory.annotation.Autowired".to_string(),
        "org.springframework.boot.test.context.SpringBootTest".to_string(),
        "org.springframework.transaction.annotation.Transactional".to_string(),
        "static org.assertj.core.api.Assertions.assertThat".to_string(),
    ]);
    // **Its ancestors first.** A `Member` stored without its `Workspace` fails
    // on the foreign key before anything about the adapter is proved, so the
    // same fixture builder the operation proofs use writes the rows this one
    // references -- deepest first, shared, and bound to the same key the child
    // carries.
    let Some(fixtures) =
        crate::emit_operation::proof::ancestor_fixtures(model, entity, &[], record, &mut imports)?
    else {
        return Ok(None);
    };
    let Some(arguments) = crate::emit_operation::proof::record_arguments(
        model,
        entity,
        &fixtures.substitutions,
        &mut imports,
    ) else {
        return Ok(None);
    };
    let (setup, autowired) = (fixtures.setup, fixtures.autowired);
    let key = &primary_key.names.java_member;
    let body = format!(
        "@SpringBootTest\n@Transactional\nclass {type_name} {{\n\n    @Autowired\n    private {record}Repository repository;\n\n{autowired}    @Test\n    void savesReadsAndDeletesThroughTheRealDatabase() {{\n{setup}        // The *stored* row, not the argument: with a database-assigned key the\n        // two differ by exactly the column the insert did not write.\n        {record} stored = repository.save(new {record}({arguments}));\n\n        assertThat(repository.findById(stored.{key}())).contains(stored);\n        assertThat(repository.findAll()).contains(stored);\n        assertThat(repository.deleteById(stored.{key}())).isTrue();\n        assertThat(repository.findById(stored.{key}())).isEmpty();\n        // A second delete is not a failure: the row is already gone, which is\n        // what the caller asked for, and it is `false` rather than an error.\n        assertThat(repository.deleteById(stored.{key}())).isFalse();\n    }}\n\n    // Reader-owned cases belong below this stable boundary.\n}}"
    );
    let artifact_id = format!("art_{capability_id}_{}_repository_it", entity.id.as_str());
    let rendered = crate::emit_capability::imported_test_container(
        model,
        &package,
        render(&package, &imports, &body, &artifact_id),
    );
    let path = ProjectPath::parse(format!(
        "{}/{}/{type_name}.java",
        crate::emit_companion_test::JAVA_TEST_ROOT,
        package.replace('.', "/")
    ))
    .map_err(CompileError::new)?;
    Ok(Some(Unit {
        path,
        file: RenderedFile {
            kind: FileKind::JavaTest,
            mode: FileMode::Regular,
            bytes: rendered.into_bytes(),
            provenance: Provenance {
                artifact_id,
                ejection_id: Some(capability_id.to_string()),
                ejectable: true,
                semantic_ids: BTreeSet::from([
                    capability_id.to_string(),
                    entity.id.as_str().to_string(),
                ]),
                compiler_pass: "java-facets".to_string(),
            },
        },
    }))
}

pub(super) fn lower_search_adapter(
    model: &AppModel,
    capability_id: &str,
    entity: &Entity,
) -> Result<Unit, CompileError> {
    let package = crate::emit_java::entity_package(model, entity, Package::AdaptersJdbc);
    let record = &entity.names.java_type;
    let type_name = format!("Jdbc{record}Search");
    let port = format!(
        "{}.{record}Search",
        crate::emit_java::entity_package(model, entity, Package::PortsSearch)
    );
    let imports = BTreeSet::from([
        port,
        domain_import(model, entity),
        "java.util.List".to_string(),
        "org.springframework.jdbc.core.simple.JdbcClient".to_string(),
        "org.springframework.stereotype.Repository".to_string(),
    ]);
    let table = &entity.names.sql_table;
    let column_list = entity
        .fields
        .iter()
        .map(|field| field.names.sql_column.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let column = crate::emit_sql::SEARCH_COLUMN;
    let configuration = crate::emit_sql::SEARCH_CONFIGURATION;
    let body = format!(
        "@Repository\npublic final class {type_name} implements {record}Search {{\n\n    private static final String SQL =\n            \"\"\"\n            select {column_list}\n              from {table}\n             where {column} @@ websearch_to_tsquery('{configuration}', :query)\n             order by ts_rank({column}, websearch_to_tsquery('{configuration}', :query)) desc\n             limit :limit\n            \"\"\";\n\n    private final JdbcClient jdbc;\n\n    public {type_name}(JdbcClient jdbc) {{\n        this.jdbc = jdbc;\n    }}\n\n    @Override\n    public List<{record}> matching(String query, int limit) {{\n        return jdbc.sql(SQL)\n                .param(\"query\", query)\n                .param(\"limit\", limit)\n                .query({record}.class)\n                .list();\n    }}\n}}"
    );
    let artifact_id = format!("art_{capability_id}_{}_search", entity.id.as_str());
    let rendered = render(&package, &imports, &body, &artifact_id);
    let package_path = package.replace('.', "/");
    let path = ProjectPath::parse(format!("{JAVA_ROOT}/{package_path}/{type_name}.java"))
        .map_err(CompileError::new)?;
    Ok(Unit {
        path,
        file: RenderedFile {
            kind: FileKind::JavaMain,
            mode: FileMode::Regular,
            bytes: rendered.into_bytes(),
            provenance: Provenance {
                artifact_id,
                ejection_id: Some(capability_id.to_string()),
                ejectable: true,
                semantic_ids: BTreeSet::from([entity.id.as_str().to_string()]),
                compiler_pass: "java-search-adapter".to_string(),
            },
        },
    })
}
