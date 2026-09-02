//! Executable proof that a declared relation is a database invariant.
//!
//! **A foreign key is the one thing in a generated project that no unit test
//! can observe.** The migration says `add constraint`, the golden suite
//! compares its bytes, and nothing runs it -- so a relation whose columns are
//! paired the wrong way round, or whose constraint never reaches the schema at
//! all, looks exactly like one that did.
//!
//! Two questions, because they fail differently. The catalogue says whether
//! the constraint is there and which ordered pairs it holds, which is what
//! catches a mapping written backwards. A rejected insert says the database
//! actually enforces it, which is what catches a constraint that exists and
//! was declared `not valid`.

use crate::CompileError;
use crate::emit_java::JavaUnit;
use jails_contracts::{FileKind, FileMode, Provenance, RenderedFile, RenderedTree};
use jails_model::{AppModel, Package, Relation, StableId as _, TypeRef};
use std::collections::BTreeSet;

const JAVA_TEST_ROOT: &str = ".jails/generated/test/java";

pub(crate) fn emit(
    model: &AppModel,
    output: &mut RenderedTree,
    _: &jails_contracts::WorkspaceSnapshot,
) -> Result<(), CompileError> {
    if !model
        .capabilities
        .values()
        .any(|capability| capability.kind == "db")
    {
        return Ok(());
    }
    for relation in model.relations.values() {
        let Some((path, file)) = proof(model, relation)? else {
            continue;
        };
        output.insert(path, file).map_err(CompileError::new)?;
    }
    Ok(())
}

fn proof(
    model: &AppModel,
    relation: &Relation,
) -> Result<Option<(jails_contracts::ProjectPath, RenderedFile)>, CompileError> {
    let (Some(child), Some(parent)) = (
        model.entities.get(&relation.child),
        model.entities.get(&relation.parent),
    ) else {
        return Ok(None);
    };
    if !child.active || !parent.active {
        return Ok(None);
    }
    let mut local = Vec::new();
    let mut remote = Vec::new();
    for mapping in &relation.mappings {
        let (Some(here), Some(there)) = (
            child.fields.iter().find(|field| field.id == mapping.local),
            parent
                .fields
                .iter()
                .find(|field| field.id == mapping.remote),
        ) else {
            return Ok(None);
        };
        local.push(here);
        remote.push(there);
    }
    // Every column the insert has to fill, with the foreign key pointed at a
    // parent that is not there. A component jails cannot spell means no proof
    // rather than a guess that fails to bind.
    let mut columns = Vec::new();
    let mut values = Vec::new();
    for field in &child.fields {
        // **An identity column takes no value at all.** PostgreSQL answers
        // `cannot insert a non-DEFAULT value into column "id"` for a
        // `generated always as identity` column, so naming it in the insert
        // fails before the foreign key is ever checked -- and the proof then
        // reports a failure about the wrong thing.
        if is_identity(field) {
            continue;
        }
        let Some(value) = sql_literal(model, field) else {
            return Ok(None);
        };
        columns.push(field.names.sql_column.as_str());
        values.push(match local.iter().any(|key| key.id == field.id) {
            true => orphan_literal(model, field)?,
            false => value,
        });
    }
    if columns.is_empty() {
        return Ok(None);
    }

    let package = model.project.package_for(Package::AdaptersJdbc);
    let type_name = format!("{}AssociationIT", upper_camel(&relation.label));
    let mapping = local
        .iter()
        .zip(&remote)
        .map(|(here, there)| format!("{}={}", here.names.sql_column, there.names.sql_column))
        .collect::<Vec<_>>()
        .join(",");
    let imports = BTreeSet::from([
        "org.junit.jupiter.api.Test".to_string(),
        "org.springframework.beans.factory.annotation.Autowired".to_string(),
        "org.springframework.boot.test.context.SpringBootTest".to_string(),
        "org.springframework.dao.DataAccessException".to_string(),
        "org.springframework.jdbc.core.simple.JdbcClient".to_string(),
        "org.springframework.transaction.annotation.Transactional".to_string(),
        "static org.assertj.core.api.Assertions.assertThat".to_string(),
        "static org.assertj.core.api.Assertions.assertThatThrownBy".to_string(),
    ]);
    let body = format!(
        "@SpringBootTest\n@Transactional\nclass {type_name} {{\n\n\
         \x20   @Autowired\n    private JdbcClient db;\n\n\
         \x20   /**\n\
         \x20    * The catalogue, because a mapping written backwards is a\n\
         \x20    * constraint that exists and pairs the wrong columns.\n\
         \x20    */\n\
         \x20   @Test\n\
         \x20   void schemaCarriesTheExactOrderedRelationship() {{\n\
         \x20       String mapping = db.sql(\"\"\"\n\
         \x20                       select string_agg(child_column.attname || '=' || parent_column.attname,\n\
         \x20                                        ',' order by pair.ordinality)\n\
         \x20                       from pg_constraint relation\n\
         \x20                       cross join lateral unnest(relation.conkey, relation.confkey)\n\
         \x20                           with ordinality as pair(child_number, parent_number, ordinality)\n\
         \x20                       join pg_attribute child_column\n\
         \x20                         on child_column.attrelid = relation.conrelid\n\
         \x20                        and child_column.attnum = pair.child_number\n\
         \x20                       join pg_attribute parent_column\n\
         \x20                         on parent_column.attrelid = relation.confrelid\n\
         \x20                        and parent_column.attnum = pair.parent_number\n\
         \x20                       where relation.contype = 'f' and relation.conname = :constraint\n\
         \x20                       \"\"\")\n\
         \x20               .param(\"constraint\", \"{}\")\n\
         \x20               .query(String.class)\n\
         \x20               .single();\n\n\
         \x20       assertThat(mapping).isEqualTo(\"{mapping}\");\n\
         \x20   }}\n\n\
         \x20   /**\n\
         \x20    * And the database enforces it, rather than merely recording it.\n\
         \x20    *\n\
         \x20    * <p>The rejection is not asked to name <em>which</em> key it broke:\n\
         \x20    * a child table with several relations trips whichever constraint\n\
         \x20    * PostgreSQL checks first, and that order is the migration\'s\n\
         \x20    * rather than this proof\'s. Which columns this one pairs is\n\
         \x20    * settled above, where nothing else can confound it.\n\
         \x20    */\n\
         \x20   @Test\n\
         \x20   void aRowNamingNoParentIsRejected() {{\n\
         \x20       assertThatThrownBy(() -> db.sql(\n\
         \x20                       \"insert into {} ({}) values ({})\")\n\
         \x20               .update())\n\
         \x20               .isInstanceOf(DataAccessException.class)\n\
         \x20               .rootCause()\n\
         \x20               .hasMessageContaining(\"violates foreign key constraint\")\n\
         \x20               .hasMessageContaining(\"{}\");\n\
         \x20   }}\n\
         }}",
        relation.sql_name,
        child.names.sql_table,
        columns.join(", "),
        values.join(", "),
        child.names.sql_table,
    );
    let artifact_id = format!("art_{}_association_it", relation.id.as_str());
    let mut unit = JavaUnit::new(&package, &imports, &body);
    crate::emit_capability::imported_test_container(model, &mut unit);
    let rendered = unit.render(&artifact_id);
    let path = jails_contracts::ProjectPath::parse(format!(
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
                semantic_ids: BTreeSet::from([relation.id.as_str().to_string()]),
                compiler_pass: "relation-proof".to_string(),
            },
        },
    )))
}

/// One value of this column, as SQL.
///
/// **Not the Java sample.** These go into a statement rather than a record, so
/// a `uuid` is a quoted literal and a `boolean` is bare -- and a project type
/// jails cannot spell means the whole proof is skipped rather than emitted
/// with a value that does not bind.
fn sql_literal(model: &AppModel, field: &jails_model::Field) -> Option<String> {
    match &field.ty {
        TypeRef::Builtin(builtin) => Some(builtin.semantics().sql_sample.to_string()),
        TypeRef::External(name) => {
            let declared = model
                .entities
                .values()
                .find(|entity| &entity.names.java_type == name)
                .filter(|entity| entity.facets.contains(&jails_model::Facet::Enum))?;
            let constant = declared.enum_constants.first()?;
            Some(format!("'{}'", constant.java_name))
        }
    }
}

/// The key of a parent that is not there.
fn orphan_literal(model: &AppModel, field: &jails_model::Field) -> Result<String, CompileError> {
    match &field.ty {
        TypeRef::Builtin(builtin) => Ok(builtin.semantics().sql_alternate.to_string()),
        TypeRef::External(_) => sql_literal(model, field).ok_or_else(|| {
            CompileError::new(format!(
                "relation key `{}` has no SQL literal jails can spell",
                field.label
            ))
        }),
    }
}

/// `child_owner` as `ChildOwner`, for the class name.
fn upper_camel(label: &str) -> String {
    label
        .split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut characters = part.chars();
            match characters.next() {
                Some(first) => first.to_ascii_uppercase().to_string() + characters.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

/// Whether the database assigns this column rather than the caller.
///
/// Read from the model's own default registry rather than from the rendered
/// DDL: `emit_sql` lowers `identity()` to `generated always as identity`, and
/// a second reader of that string would drift from the one that wrote it.
fn is_identity(field: &jails_model::Field) -> bool {
    matches!(
        field.semantics.default.as_ref().map(|default| &default.value),
        Some(jails_model::Value::Function { name, arguments })
            if name == "identity" && arguments.is_empty()
    )
}
