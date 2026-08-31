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

pub(super) fn lower_fake_repository(
    model: &AppModel,
    capability_id: &str,
    entity: &Entity,
) -> Result<Unit, CompileError> {
    let primary_key = primary_key(entity)?;
    let package = model.project.package_for(Package::AdaptersMemory);
    let type_name = format!("InMemory{}Repository", entity.names.java_type);
    let repository = format!(
        "{}.{}Repository",
        model.project.package_for(Package::Repository),
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
    let body = format!(
        "public final class {type_name} implements {record}Repository {{\n\n    private final Map<{key_type}, {record}> rows = new LinkedHashMap<>();\n\n    @Override\n    public Optional<{record}> findById({key_type} id) {{\n        return Optional.ofNullable(rows.get(id));\n    }}\n\n    @Override\n    public List<{record}> findAll() {{\n        return List.copyOf(rows.values());\n    }}\n\n    @Override\n    public {record} save({record} value) {{\n        rows.put(value.{key}(), value);\n        return value;\n    }}\n\n    @Override\n    public boolean deleteById({key_type} id) {{\n        return rows.remove(id) != null;\n    }}\n}}"
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
    let package = model.project.package_for(Package::AdaptersJdbc);
    let type_name = format!("Jdbc{}Repository", entity.names.java_type);
    let repository = format!(
        "{}.{}Repository",
        model.project.package_for(Package::Repository),
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
    // **A `generated always as identity` column is the database's to write.**
    // PostgreSQL refuses an explicit value for one outright, so `save` failed
    // with `BadSqlGrammar` on every entity with a `long @pk` -- which is the
    // ordinary shape. It compiled, and no integration test ran to say
    // otherwise. `emit_operation/command.rs` has omitted defaulted fields from
    // its insert all along; this is the copy that did not.
    //
    // The `select` and `returning` lists keep the column, because reading it
    // back is the whole point of letting the database assign it.
    let written = entity
        .fields
        .iter()
        .filter(|field| !is_identity(field))
        .collect::<Vec<_>>();
    let write_list = written
        .iter()
        .map(|field| field.names.sql_column.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let values = written
        .iter()
        .map(|field| format!(":{}", field.names.sql_column))
        .collect::<Vec<_>>()
        .join(", ");
    let updates = written
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
    let params = written
        .iter()
        .map(|field| {
            let member = &field.names.java_member;
            let value = if field.required {
                super::jdbc_param(field, &format!("value.{member}()"))
            } else {
                // Unwrapped before conversion: a conversion applied to the
                // `Optional` would not compile, and one applied after
                // `orElse(null)` would call a method on null.
                let converted = super::jdbc_param(field, "value");
                if converted == "value" {
                    format!("value.{member}().orElse(null)")
                } else {
                    format!("value.{member}().map(value -> {converted}).orElse(null)")
                }
            };
            format!(
                "\n                .param(\"{}\", {value})",
                field.names.sql_column
            )
        })
        .collect::<String>();
    // With an identity key there is no caller-supplied value to conflict on,
    // so the upsert arm would never fire and `on conflict` naming a column the
    // insert does not mention is a statement PostgreSQL will not plan.
    let conflict = if is_identity(primary_key) {
        String::new()
    } else {
        format!(" on conflict ({key_column}) do update set {updates}")
    };
    let body = format!(
        "@Repository\npublic final class {type_name} implements {record}Repository {{\n\n    private final JdbcClient jdbc;\n\n    public {type_name}(JdbcClient jdbc) {{\n        this.jdbc = jdbc;\n    }}\n\n    @Override\n    public Optional<{record}> findById({key_type} id) {{\n        return jdbc.sql(\"select {column_list} from {table} where {key_column} = :id\")\n                .param(\"id\", id)\n                .query({record}.class)\n                .optional();\n    }}\n\n    @Override\n    public List<{record}> findAll() {{\n        return jdbc.sql(\"select {column_list} from {table} order by {key_column}\")\n                .query({record}.class)\n                .list();\n    }}\n\n    @Override\n    public {record} save({record} value) {{\n        return jdbc.sql(\"insert into {table} ({write_list}) values ({values}){conflict} returning {column_list}\"){params}\n                .query({record}.class)\n                .single();\n    }}\n\n    @Override\n    public boolean deleteById({key_type} id) {{\n        return jdbc.sql(\"delete from {table} where {key_column} = :id\")\n                .param(\"id\", id)\n                .update() > 0;\n    }}\n}}",
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

/// The repository adapter's integration test, against a real database.
///
/// **The JDBC adapter is the one artifact whose correctness a unit test cannot
/// reach.** Its whole content is SQL that PostgreSQL either accepts or does
/// not: a column list that drifted from the row mapper, an `on conflict` naming
/// a column with no unique index, a type the driver will not bind. All of it
/// compiles, and the canonical backend emitted nine adapters and one
/// integration test -- the one hand-written for `presence`.
///
/// A round trip is the whole assertion, and it is enough to catch every one of
/// those: `save` exercises the insert, its parameters and the returning
/// clause, and `findById` exercises the select and the row mapper. Asserting
/// the read equals the write is what makes a drifted column list fail here
/// rather than in production.
///
/// `@Transactional` so the row is rolled back and the ITs do not depend on the
/// order they run in. `None` when the entity has a component jails cannot
/// sample -- there is no row to write, and a guess would not compile.
pub(super) fn lower_db_repository_test(
    model: &AppModel,
    capability_id: &str,
    entity: &Entity,
    spring_boot: Option<&str>,
) -> Result<Option<Unit>, CompileError> {
    let primary_key = primary_key(entity)?;
    let package = model.project.package_for(Package::AdaptersJdbc);
    let type_name = format!("Jdbc{}RepositoryIT", entity.names.java_type);
    let record = &entity.names.java_type;
    let mut imports = BTreeSet::from([
        format!(
            "{}.{record}Repository",
            model.project.package_for(Package::Repository)
        ),
        domain_import(model, entity),
        format!(
            "{}.TestcontainersConfig",
            model.project.package_for(Package::Base)
        ),
        "org.junit.jupiter.api.Test".to_string(),
        "org.springframework.beans.factory.annotation.Autowired".to_string(),
        "org.springframework.boot.test.context.SpringBootTest".to_string(),
        "org.springframework.context.annotation.Import".to_string(),
        "org.springframework.transaction.annotation.Transactional".to_string(),
        "static org.assertj.core.api.Assertions.assertThat".to_string(),
    ]);
    // **A foreign key needs its row to exist.** A sampled `1` for
    // `Message.userId` names a `User` that was never inserted, and PostgreSQL
    // rejects the whole statement -- so the parent is saved through its own
    // repository first and its assigned key is what the child carries.
    //
    // One level, and a parent that is itself a child gets no test rather than
    // a wrong one: the chain would need ordering and cycle detection, and a
    // test that cannot be built correctly must not be guessed at.
    let mut overrides = std::collections::BTreeMap::new();
    let mut fixtures = String::new();
    let mut autowired = String::new();
    for relation in model
        .relations
        .values()
        .filter(|relation| relation.child == entity.id)
    {
        let Some(parent) = model.entities.get(&relation.parent) else {
            return Ok(None);
        };
        if model
            .relations
            .values()
            .any(|other| other.child == parent.id)
        {
            return Ok(None);
        }
        let Some(parent_row) =
            crate::emit_companion_test::constructor_call(model, parent, &mut imports)
        else {
            return Ok(None);
        };
        let parent_type = &parent.names.java_type;
        let variable = format!("saved{parent_type}");
        imports.insert(format!(
            "{}.{parent_type}Repository",
            model.project.package_for(Package::Repository)
        ));
        imports.insert(domain_import(model, parent));
        autowired.push_str(&format!(
            "\n    @Autowired\n    private {parent_type}Repository {}Repository;\n",
            lower_first(parent_type)
        ));
        fixtures.push_str(&format!(
            "        {parent_type} {variable} = {}Repository.save({parent_row});\n",
            lower_first(parent_type)
        ));
        for mapping in &relation.mappings {
            let Some(remote) = parent.field(&mapping.remote) else {
                return Ok(None);
            };
            overrides.insert(
                mapping.local.clone(),
                format!("{variable}.{}()", remote.names.java_member),
            );
        }
    }
    let Some(row) =
        crate::emit_companion_test::constructor_call_with(model, entity, &mut imports, &overrides)
    else {
        return Ok(None);
    };
    let _ = spring_boot;
    let key = &primary_key.names.java_member;
    let body = format!(
        "@Import(TestcontainersConfig.class)\n@SpringBootTest\n@Transactional\nclass {type_name} {{\n\n    @Autowired\n    private {record}Repository repository;\n{autowired}\n    @Test\n    void storesAndReadsBackTheSameRow() {{\n{fixtures}        {record} stored = repository.save({row});\n\n        // The stored row rather than the argument: with a database-assigned\n        // key or a compiler-managed audit column the two differ, and the\n        // round trip is what this test is for.\n        assertThat(repository.findById(stored.{key}())).contains(stored);\n    }}\n\n    // Reader-owned tests belong below this stable boundary.\n}}"
    );
    let artifact_id = format!("art_{capability_id}_{}_repository_test", entity.id.as_str());
    let rendered = render(&package, &imports, &body, &artifact_id);
    let package_path = package.replace('.', "/");
    let path = ProjectPath::parse(format!("{JAVA_TEST_ROOT}/{package_path}/{type_name}.java"))
        .map_err(CompileError::new)?;
    Ok(Some(Unit {
        path,
        file: RenderedFile {
            kind: FileKind::JavaTest,
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
                compiler_pass: "capability-db-test".to_string(),
            },
        },
    }))
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
pub(super) fn lower_search_adapter(
    model: &AppModel,
    capability_id: &str,
    entity: &Entity,
) -> Result<Unit, CompileError> {
    let package = model.project.package_for(Package::AdaptersJdbc);
    let record = &entity.names.java_type;
    let type_name = format!("Jdbc{record}Search");
    let port = format!(
        "{}.{record}Search",
        model.project.package_for(Package::PortsSearch)
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

/// Whether the database assigns this column and refuses anything else.
///
/// The same `identity()` default `emit_sql` turns into `generated always as
/// identity`, read here so the adapter and the DDL cannot disagree about who
/// writes the column.
fn is_identity(field: &jails_model::Field) -> bool {
    matches!(
        field.semantics.default.as_ref().map(|default| &default.value),
        Some(jails_model::Value::Function { name, arguments })
            if name == "identity" && arguments.is_empty()
    )
}
