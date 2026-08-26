//! Discovery and deterministic offline compilation for managed SQL queries.

use crate::application_manifest::{self, ManifestFormat};
use crate::model::Project;
use crate::query_compiler::{compile_catalog, compile_query, parse_query_file};
use jails_protocol::application::ApplicationSpecV1;
use jails_protocol::database::{QueryContractV1, QuerySource};
use jails_protocol::identity::{Package, ProjectPath};
use jails_support::Result;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CheckedQuery {
    pub source: QuerySource,
    pub contract: QueryContractV1,
    pub slice_package: Package,
}

pub fn check_offline(
    project: &Project,
    manifest_path: Option<&Path>,
    selector: Option<&str>,
) -> Result<Vec<CheckedQuery>> {
    let (manifest, _) = read_manifest(project, manifest_path)?;
    let migrations = migrations(project)?;
    let catalog = compile_catalog(manifest.dialect, &migrations)?;
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
            let contract = compile_query(&source, &catalog)?;
            checked.push(CheckedQuery {
                source,
                contract,
                slice_package: package.clone(),
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
