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

pub(super) fn lower_fake_repository(
    model: &AppModel,
    capability_id: &str,
    entity: &Entity,
) -> Result<Unit, CompileError> {
    let primary_key = primary_key(entity)?;
    let package = model.project.package_for("adapters.memory");
    let type_name = format!("InMemory{}Repository", entity.names.java_type);
    let repository = format!(
        "{}.{}Repository",
        model.project.package_for("repository"),
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
    let package = model.project.package_for("adapters.jdbc");
    let type_name = format!("Jdbc{}Repository", entity.names.java_type);
    let repository = format!(
        "{}.{}Repository",
        model.project.package_for("repository"),
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
    let values = columns
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
    let params = entity
        .fields
        .iter()
        .map(|field| {
            let member = &field.names.java_member;
            let value = if field.required {
                format!("value.{member}()")
            } else {
                format!("value.{member}().orElse(null)")
            };
            format!(
                "\n                .param(\"{}\", {value})",
                field.names.sql_column
            )
        })
        .collect::<String>();
    let body = format!(
        "@Repository\npublic final class {type_name} implements {record}Repository {{\n\n    private final JdbcClient jdbc;\n\n    public {type_name}(JdbcClient jdbc) {{\n        this.jdbc = jdbc;\n    }}\n\n    @Override\n    public Optional<{record}> findById({key_type} id) {{\n        return jdbc.sql(\"select {column_list} from {table} where {key_column} = :id\")\n                .param(\"id\", id)\n                .query({record}.class)\n                .optional();\n    }}\n\n    @Override\n    public List<{record}> findAll() {{\n        return jdbc.sql(\"select {column_list} from {table} order by {key_column}\")\n                .query({record}.class)\n                .list();\n    }}\n\n    @Override\n    public {record} save({record} value) {{\n        return jdbc.sql(\"insert into {table} ({column_list}) values ({values}) on conflict ({key_column}) do update set {updates} returning {column_list}\"){params}\n                .query({record}.class)\n                .single();\n    }}\n\n    @Override\n    public boolean deleteById({key_type} id) {{\n        return jdbc.sql(\"delete from {table} where {key_column} = :id\")\n                .param(\"id\", id)\n                .update() > 0;\n    }}\n}}",
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
