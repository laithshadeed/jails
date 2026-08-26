//! Discovery and deterministic offline compilation for managed SQL queries.

use crate::application_manifest::{self, ManifestFormat};
use crate::model::Project;
use crate::query_compiler::{compile_catalog, compile_query_with_inputs, parse_query_file};
use jails_protocol::application::ApplicationSpecV1;
use jails_protocol::application::{AuditPolicy, DeclaredEntityLifecycle};
use jails_protocol::database::{
    CatalogSnapshot, QualifiedSqlName, QueryContractV1, QuerySource, SchemaObject, SchemaObjectId,
    SchemaObjectKind, SchemaProvenance, SchemaSnapshot, SqlDialect, SqlTypeName,
};
use jails_protocol::declaration::{FieldType, Optionality, ScalarFieldType};
use jails_protocol::identity::{Package, ProjectPath, SqlName};
use jails_support::Result;
use jails_support::codec::{Codec, Encoder, domain_hash};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CheckedQuery {
    pub source: QuerySource,
    pub contract: QueryContractV1,
    pub slice_package: Package,
    pub inputs: BTreeSet<ProjectPath>,
}

pub fn check_offline(
    project: &Project,
    manifest_path: Option<&Path>,
    selector: Option<&str>,
) -> Result<Vec<CheckedQuery>> {
    let (manifest, manifest_source) = read_manifest(project, manifest_path)?;
    let migrations = migrations(project)?;
    let mut common_inputs = migrations
        .iter()
        .map(|(path, _)| path.clone())
        .collect::<BTreeSet<_>>();
    common_inputs.insert(project_relative(project, &manifest_source)?);
    let catalog = compile_catalog(manifest.dialect, &migrations)?;
    let migration_digest = ordered_migration_digest(&migrations)?;
    let mut checked = Vec::new();
    for (slice_name, slice) in &manifest.slices {
        let package = slice.package.as_ref().unwrap_or(&manifest.base_package);
        for (query_name, query) in &slice.queries {
            if selector.is_some_and(|wanted| {
                wanted != query_name.as_str() && wanted != query.source.as_str()
            }) {
                continue;
            }
            let absolute = project.root().join(query.source.as_str());
            let contents = fs::read_to_string(&absolute).map_err(|error| {
                format!("failed to read managed query `{}`: {error}", query.source)
            })?;
            let source = parse_query_file(
                slice_name.as_str(),
                query.source.as_str(),
                &contents,
                manifest.dialect,
            )?;
            if source.id.name != *query_name {
                return Err(format!(
                    "manifest query `{}` points to a directive named `{}`.\n       fix: make the manifest key and `-- jails:name` agree.",
                    query_name.as_str(),
                    source.id.name.as_str()
                )
                .into());
            }
            let contract = compile_query_with_inputs(
                &source,
                &catalog,
                migration_digest,
                &manifest.type_mappings,
            )?;
            let mut inputs = common_inputs.clone();
            inputs.insert(query.source.clone());
            checked.push(CheckedQuery {
                source,
                contract,
                slice_package: package.clone(),
                inputs,
            });
        }
    }
    if checked.is_empty() {
        return Err(match selector {
            Some(selector) => format!(
                "no managed query matches `{selector}`.\n       fix: use a manifest query name or project-relative source path."
            )
            .into(),
            None => "the application manifest declares no managed queries.\n       fix: add a `[slices.<Slice>.queries.<Name>]` source entry."
                .into(),
        });
    }
    checked.sort_by(|left, right| left.source.id.cmp(&right.source.id));
    Ok(checked)
}

/// Compile migration authority without conflating it with declared or live
/// state. The ordered file list remains provenance, while the catalog digest
/// covers the normalized facts and every opaque statement.
pub fn migration_schema(project: &Project, manifest_path: Option<&Path>) -> Result<SchemaSnapshot> {
    let (manifest, _) = read_manifest(project, manifest_path)?;
    let migrations = migrations(project)?;
    let files = migrations.iter().map(|(path, _)| path.clone()).collect();
    Ok(SchemaSnapshot {
        catalog: compile_catalog(manifest.dialect, &migrations)?,
        provenance: SchemaProvenance::Migrations { files },
        ignored_schemas: BTreeSet::new(),
        ignores_extension_owned_objects: false,
    })
}

/// Whether any migration statement is destructive or deployment-sensitive.
///
/// The manifest is consulted for one thing -- the dialect -- and demanding it
/// made this command unusable on the shape `jails new` produces, which has no
/// manifest at all and is the shape every reproduction in `bugs.md` uses. The
/// question *is* answerable without one: the migrations are on disk and the
/// dialect is a fact about the driver the project declares, which is the same
/// authority `Project::sql_dialect` uses everywhere else. A manifest, when
/// there is one, still wins -- it is the declaration, and the driver is the
/// inference.
pub fn migration_lint(
    project: &Project,
    manifest_path: Option<&Path>,
) -> Result<Vec<crate::query_compiler::MigrationFinding>> {
    let dialect = match read_manifest(project, manifest_path) {
        Ok((manifest, _)) => manifest.dialect,
        // An explicitly named manifest that cannot be read is an error: the
        // caller asked for that file. An absent default one is not.
        Err(error) if manifest_path.is_some() => return Err(error),
        // The driver the project declares. `sqlite-jdbc` is read first
        // because `add sqlite` is the one capability whose statements this
        // lint judges differently; H2 and PostgreSQL share the vocabulary for
        // everything jails emits, and the lint has no H2 of its own.
        Err(_) if project.has_dependency("org.xerial", "sqlite-jdbc") => {
            jails_protocol::database::SqlDialect::Sqlite
        }
        Err(_) => jails_protocol::database::SqlDialect::PostgreSql,
    };
    crate::query_compiler::lint_migration_sources(dialect, &migrations(project)?)
}

pub fn declared_schema(project: &Project, manifest_path: Option<&Path>) -> Result<SchemaSnapshot> {
    let (manifest, _) = read_manifest(project, manifest_path)?;
    if manifest.dialect != SqlDialect::PostgreSql {
        return Err(
            "declared schema projection currently requires PostgreSQL.\n       fix: select migration or live authority for this dialect."
                .into(),
        );
    }
    let namespace = SqlName::parse("public")?;
    let mut objects = BTreeMap::from([(
        SchemaObjectId {
            dialect: manifest.dialect,
            namespace: namespace.clone(),
            kind: SchemaObjectKind::Schema,
            name: namespace.clone(),
            parent: None,
        },
        SchemaObject::Schema,
    )]);
    for slice in manifest.slices.values() {
        for entity in slice.entities.values() {
            if matches!(
                entity.lifecycle,
                DeclaredEntityLifecycle::RetiredDropPlanned { .. }
            ) {
                continue;
            }
            let table = entity.table.table.clone();
            objects.insert(
                schema_id(
                    manifest.dialect,
                    &namespace,
                    SchemaObjectKind::Table,
                    &table,
                    None,
                ),
                SchemaObject::Table,
            );
            let parent = QualifiedSqlName {
                namespace: Some(namespace.clone()),
                name: table.clone(),
            };
            let mut primary = Vec::new();
            let mut ordinal = 0u32;
            for field in &entity.fields {
                ordinal += 1;
                let column = SqlName::parse(&snake_case(field.name.as_str()))?;
                objects.insert(
                    schema_id(
                        manifest.dialect,
                        &namespace,
                        SchemaObjectKind::Column,
                        &column,
                        Some(parent.clone()),
                    ),
                    SchemaObject::Column {
                        sql_type: declared_sql_type(&field.field_type)?,
                        nullable: field.optionality == Optionality::Nullable,
                        ordinal,
                        default_expression: None,
                        generated: None,
                        identity: None,
                        comment: None,
                    },
                );
                if field.constraints.primary_key {
                    primary.push(column.clone());
                }
                let suffix = if field.constraints.unique {
                    Some((SchemaObjectKind::Unique, "key"))
                } else if field.constraints.indexed {
                    Some((SchemaObjectKind::Index, "idx"))
                } else {
                    None
                };
                if let Some((kind, suffix)) = suffix {
                    let name = SqlName::parse(&format!(
                        "{}_{}_{}",
                        table.as_str(),
                        column.as_str(),
                        suffix
                    ))?;
                    let definition = format!("({})", column.as_str());
                    let object = if kind == SchemaObjectKind::Unique {
                        SchemaObject::Unique { definition }
                    } else {
                        SchemaObject::Index { definition }
                    };
                    objects.insert(
                        schema_id(
                            manifest.dialect,
                            &namespace,
                            kind,
                            &name,
                            Some(parent.clone()),
                        ),
                        object,
                    );
                }
            }
            for name in audit_columns(entity.audit) {
                ordinal += 1;
                let column = SqlName::parse(name)?;
                objects.insert(
                    schema_id(
                        manifest.dialect,
                        &namespace,
                        SchemaObjectKind::Column,
                        &column,
                        Some(parent.clone()),
                    ),
                    SchemaObject::Column {
                        sql_type: SqlTypeName::parse("timestamptz")?,
                        nullable: false,
                        ordinal,
                        default_expression: None,
                        generated: None,
                        identity: None,
                        comment: None,
                    },
                );
            }
            if !primary.is_empty() {
                let name = SqlName::parse(&format!("{}_pkey", table.as_str()))?;
                objects.insert(
                    schema_id(
                        manifest.dialect,
                        &namespace,
                        SchemaObjectKind::PrimaryKey,
                        &name,
                        Some(parent),
                    ),
                    SchemaObject::PrimaryKey { columns: primary },
                );
            }
        }
    }
    Ok(SchemaSnapshot {
        catalog: CatalogSnapshot::new(manifest.dialect, objects, Vec::new())?,
        provenance: SchemaProvenance::Declared,
        ignored_schemas: BTreeSet::new(),
        ignores_extension_owned_objects: false,
    })
}

fn schema_id(
    dialect: SqlDialect,
    namespace: &SqlName,
    kind: SchemaObjectKind,
    name: &SqlName,
    parent: Option<QualifiedSqlName>,
) -> SchemaObjectId {
    SchemaObjectId {
        dialect,
        namespace: namespace.clone(),
        kind,
        name: name.clone(),
        parent,
    }
}

fn audit_columns(policy: AuditPolicy) -> &'static [&'static str] {
    match policy {
        AuditPolicy::None => &[],
        AuditPolicy::Created => &["created_at"],
        AuditPolicy::CreatedAndUpdated => &["created_at", "updated_at"],
    }
}

fn declared_sql_type(field_type: &FieldType) -> Result<SqlTypeName> {
    let name = match field_type {
        FieldType::List(_) | FieldType::Map { .. } => "jsonb".to_string(),
        FieldType::Scalar(scalar) => match scalar {
            ScalarFieldType::Text
            | ScalarFieldType::Currency
            | ScalarFieldType::ZoneId
            | ScalarFieldType::Uri
            | ScalarFieldType::Path => "text".to_string(),
            ScalarFieldType::Integer => "int4".to_string(),
            ScalarFieldType::Long => "int8".to_string(),
            ScalarFieldType::Boolean => "bool".to_string(),
            ScalarFieldType::LocalDate => "date".to_string(),
            ScalarFieldType::LocalDateTime => "timestamp".to_string(),
            ScalarFieldType::Instant => "timestamptz".to_string(),
            ScalarFieldType::Uuid => "uuid".to_string(),
            ScalarFieldType::Decimal => "numeric".to_string(),
            ScalarFieldType::Bytes => "bytea".to_string(),
            ScalarFieldType::Duration => "interval".to_string(),
            ScalarFieldType::Double => "float8".to_string(),
            ScalarFieldType::Project(java_type) => {
                let qualified = java_type.qualified();
                let simple = qualified.rsplit('.').next().unwrap_or(&qualified);
                format!("public.{}", snake_case(simple))
            }
        },
    };
    SqlTypeName::parse(&name)
}

fn snake_case(value: &str) -> String {
    let mut out = String::new();
    for (index, character) in value.chars().enumerate() {
        if character.is_ascii_uppercase() {
            if index > 0 {
                out.push('_');
            }
            out.push(character.to_ascii_lowercase());
        } else {
            out.push(character);
        }
    }
    out
}

fn ordered_migration_digest(
    migrations: &[(ProjectPath, String)],
) -> Result<jails_protocol::identity::ObjectId> {
    let mut encoder = Encoder::new();
    encoder.count(migrations.len())?;
    for (path, contents) in migrations {
        path.encode(&mut encoder)?;
        encoder.string(contents)?;
    }
    Ok(jails_protocol::identity::ObjectId::from_bytes(domain_hash(
        "JAILS-SQL-MIGRATIONS-1",
        &encoder.finish()?,
    )))
}

fn project_relative(project: &Project, path: &Path) -> Result<ProjectPath> {
    let relative = path.strip_prefix(project.root()).map_err(|_| {
        format!(
            "application manifest {} is outside the project and cannot be guarded by a transaction.\n       fix: copy it beneath the project before generating SQL contracts.",
            path.display()
        )
    })?;
    ProjectPath::parse(&relative.to_string_lossy())
}

pub fn read_manifest(
    project: &Project,
    requested: Option<&Path>,
) -> Result<(ApplicationSpecV1, PathBuf)> {
    let path = requested
        .map(|path| {
            if path.is_absolute() {
                path.to_path_buf()
            } else {
                project.root().join(path)
            }
        })
        .unwrap_or_else(|| project.root().join(".jails/app.toml"));
    let contents = fs::read_to_string(&path).map_err(|error| {
        format!(
            "failed to read application manifest {}: {error}",
            path.display()
        )
    })?;
    let format = match path.extension().and_then(|extension| extension.to_str()) {
        Some("toml") => ManifestFormat::Toml,
        Some("json") => ManifestFormat::Json,
        other => {
            return Err(format!(
                "application manifest {} has unsupported extension {:?}.\n       fix: use `.toml` or `.json`.",
                path.display(),
                other
            )
            .into());
        }
    };
    Ok((application_manifest::decode(&contents, format)?, path))
}

fn migrations(project: &Project) -> Result<Vec<(ProjectPath, String)>> {
    let relative = Path::new("src/main/resources/db/migration");
    let directory = project.root().join(relative);
    let mut entries = Vec::new();
    let read = fs::read_dir(&directory).map_err(|error| {
        format!(
            "failed to read migration directory {}: {error}.\n       fix: add the database capability or create the ordered Flyway directory.",
            directory.display()
        )
    })?;
    for entry in read {
        let entry = entry.map_err(|error| format!("failed to read migration entry: {error}"))?;
        let file_type = entry
            .file_type()
            .map_err(|error| format!("failed to inspect migration entry: {error}"))?;
        if !file_type.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.ends_with(".sql") {
            continue;
        }
        let version = migration_version(&name)?;
        let path = relative.join(&name);
        let project_path = ProjectPath::parse(&path.to_string_lossy())?;
        let contents = fs::read_to_string(entry.path())
            .map_err(|error| format!("failed to read migration `{project_path}`: {error}"))?;
        entries.push((version, project_path, contents));
    }
    entries.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
    for pair in entries.windows(2) {
        if pair[0].0 == pair[1].0 {
            return Err(format!(
                "migrations `{}` and `{}` both use version {}.\n       fix: allocate a distinct forward migration version.",
                pair[0].1, pair[1].1, pair[0].0
            )
            .into());
        }
    }
    Ok(entries
        .into_iter()
        .map(|(_, path, contents)| (path, contents))
        .collect())
}

fn migration_version(name: &str) -> Result<u64> {
    let version = name
        .strip_prefix('V')
        .and_then(|rest| rest.split_once("__"))
        .map(|(version, _)| version)
        .ok_or_else(|| {
            format!(
                "migration `{name}` is not a versioned Flyway migration.\n       fix: name it `V001__description.sql`."
            )
        })?;
    version.parse().map_err(|_| {
        format!(
            "migration `{name}` has a non-numeric version.\n       fix: use a positive integer after `V`."
        )
        .into()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn flagship_query_flows_from_manifest_and_ordered_migrations() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fs::write(root.join("pom.xml"), "<project><modelVersion>4.0.0</modelVersion><groupId>org.example</groupId><artifactId>sample</artifactId><version>1</version></project>").unwrap();
        fs::create_dir_all(root.join("src/main/java/org/example/sample")).unwrap();
        fs::write(
            root.join("src/main/java/org/example/sample/App.java"),
            "package org.example.sample;\npublic class App {}\n",
        )
        .unwrap();
        fs::create_dir_all(root.join(".jails")).unwrap();
        fs::create_dir_all(root.join("src/main/resources/db/migration")).unwrap();
        fs::create_dir_all(root.join("src/main/resources/db/queries")).unwrap();
        fs::write(
            root.join(".jails/app.toml"),
            r#"schema = "jails.app.v1"
[application]
name = "Example"
base_package = "org.example.sample"
java_release = 26
dialect = "postgresql"
[slices.Sample]
[slices.Sample.queries.FindEntries]
source = "src/main/resources/db/queries/FindEntries.sql"
"#,
        )
        .unwrap();
        fs::write(
            root.join("src/main/resources/db/migration/V001__entries.sql"),
            "CREATE TABLE entries (id uuid PRIMARY KEY, state text NOT NULL);",
        )
        .unwrap();
        fs::write(
            root.join("src/main/resources/db/queries/FindEntries.sql"),
            "-- jails:name FindEntries\n-- jails:cardinality many\n-- jails:param state text\nSELECT id, state FROM entries WHERE state = :state;\n",
        )
        .unwrap();

        let project = Project::load(root).unwrap();
        let checked = check_offline(&project, None, None).unwrap();
        assert_eq!(checked.len(), 1);
        assert_eq!(checked[0].contract.columns.len(), 2);
        assert_eq!(checked[0].contract.parameters.len(), 1);
    }
}
