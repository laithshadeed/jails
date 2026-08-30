//! `use seed`: development data an application loads for itself.
//!
//! Three artifacts and each is answering a different question. The JSON file
//! is the data. The runner is *how* it gets in, and it goes through the
//! repository port rather than SQL, so a row the record rejects fails at
//! start-up instead of sitting in the table waiting to be read. The test is
//! the only thing that reads the file at all until somebody starts under the
//! seed profile -- without it a renamed component surfaces as a start-up that
//! dies, in whatever environment happened to be seeding.
//!
//! **Two guards on the runner, both load-bearing.** `@Profile("seed")` means
//! it never runs anywhere nobody asked for it, and the empty-table check means
//! it never runs twice: an edited seed row cannot be told from a change
//! somebody made in the database, so re-applying one would silently revert
//! their work.
//!
//! **The `json` capability is a prerequisite and its class is not assumed to
//! be called `Json`.** `cap json name=Api` writes `ApiJson`, and a runner
//! naming a class the project does not have is a compile error in a file the
//! reader did not write.

use crate::CompileError;
use jails_contracts::{FileKind, FileMode, ProjectPath, Provenance, RenderedFile};
use jails_model::{AppModel, Capability, Entity, Package, StableId, TypeRef};
use std::collections::BTreeSet;

const JAVA_MAIN_ROOT: &str = ".jails/generated/main/java";
const JAVA_TEST_ROOT: &str = ".jails/generated/test/java";
const RESOURCE_ROOT: &str = ".jails/generated/main/resources";

const SEEDER: &str = include_str!("../../../templates/spring/seeder_java.java");
const TEST: &str = include_str!("../../../templates/spring/seeder_test_java.java");

pub(crate) fn lower(
    model: &AppModel,
    entity: &Entity,
) -> Result<Vec<(ProjectPath, RenderedFile)>, CompileError> {
    let reader = json_reader(model, entity)?;
    let name = &entity.names.java_type;
    let adapters = model.project.package_for(Package::Adapters);
    let domain = model.project.package_for(Package::Domain);
    let repository = model.project.package_for(Package::Repository);
    let resource = format!("db/seeds/{}.json", entity.names.sql_table);
    let row = row(entity);
    let imports = format!(
        "{}{}{}",
        import(&adapters, &domain, name),
        import(&adapters, &repository, &format!("{name}Repository")),
        // The reader is a class of this same package, so this is empty today
        // -- and stays here because `cap json` may be placed elsewhere.
        import(&adapters, &adapters, &reader),
    );
    let seeder = SEEDER
        .replace("{{pkg}}", &adapters)
        .replace("{{imports}}", &imports)
        .replace("{{resource}}", &resource)
        .replace("{{json}}", &reader)
        .replace("{{name}}", name);
    let disabled = row.is_none();
    let test = TEST
        .replace("{{pkg}}", &adapters)
        .replace("{{imports}}", &import(&adapters, &domain, name))
        .replace("{{resource}}", &resource)
        .replace(
            "{{disabled_import}}",
            if disabled {
                "import org.junit.jupiter.api.Disabled;\n"
            } else {
                ""
            },
        )
        .replace(
            "{{disabled}}",
            &if disabled {
                format!(
                    "    @Disabled(\"todo: jails could not write a sample of every {name} component; fill in src/main/resources/{resource} by hand, then delete this @Disabled\")\n"
                )
            } else {
                String::new()
            },
        )
        .replace("{{name}}", name);

    Ok(vec![
        // The data itself, and an empty array when jails could not sample
        // every component: a file with a row it cannot honestly fill would
        // fail to bind, which is worse than an empty one the test reports.
        rendered(
            entity,
            "seed_data",
            RESOURCE_ROOT,
            &resource,
            FileKind::Resource,
            match &row {
                Some(components) => format!("[\n  {{\n{components}\n  }}\n]\n"),
                None => "[]\n".to_string(),
            },
        )?,
        rendered(
            entity,
            "seeder",
            JAVA_MAIN_ROOT,
            &format!("{}/{name}Seeder.java", adapters.replace('.', "/")),
            FileKind::JavaMain,
            seeder,
        )?,
        rendered(
            entity,
            "seeder_test",
            JAVA_TEST_ROOT,
            &format!("{}/{name}SeederTest.java", adapters.replace('.', "/")),
            FileKind::JavaTest,
            test,
        )?,
    ])
}

/// One row of this entity, as JSON, or `None` when a component has no sample.
///
/// A project-owned type is one jails cannot invent a value of. Writing a guess
/// would ship a file that fails to bind at start-up, so the row is left out
/// and the generated test is `@Disabled` naming what to fill in -- the same
/// trade `sample_value` makes for a generated record test.
fn row(entity: &Entity) -> Option<String> {
    if entity.fields.is_empty() {
        return None;
    }
    entity
        .fields
        .iter()
        .map(|field| match &field.ty {
            TypeRef::Builtin(builtin) => Some(format!(
                "    \"{}\": {}",
                field.names.java_member,
                builtin.semantics().json
            )),
            TypeRef::External(_) => None,
        })
        .collect::<Option<Vec<_>>>()
        .map(|components| components.join(",\n"))
}

/// The project's JSON reader, by name, or a refusal naming the fix.
///
/// Two of them is refused rather than picked between, on `source.rs`'s rule:
/// choosing silently points the generated code at the wrong one.
fn json_reader(model: &AppModel, entity: &Entity) -> Result<String, CompileError> {
    let readers = model
        .capabilities
        .values()
        .filter(|capability| capability.kind == "json")
        .collect::<Vec<_>>();
    match readers.as_slice() {
        [reader] => Ok(class(reader)),
        [] => Err(CompileError::new(format!(
            "seeded entity `{}` reads its rows from a JSON file\n       fix: declare `cap json` in the model",
            entity.label
        ))),
        many => Err(CompileError::new(format!(
            "seeded entity `{}` has {} JSON readers to choose between ({})\n       fix: leave one `cap json`, or write the seeder by hand",
            entity.label,
            many.len(),
            many.iter()
                .map(|reader| class(reader))
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

/// The class `cap json` writes: `Json`, or `<Name>Json` when it is named.
fn class(capability: &Capability) -> String {
    let prefix = capability.name.as_deref().map_or_else(String::new, |name| {
        let mut characters = name.chars();
        characters.next().map_or_else(String::new, |first| {
            first.to_ascii_uppercase().to_string() + characters.as_str()
        })
    });
    format!("{prefix}Json")
}

fn rendered(
    entity: &Entity,
    suffix: &str,
    root: &str,
    relative: &str,
    kind: FileKind,
    body: String,
) -> Result<(ProjectPath, RenderedFile), CompileError> {
    let artifact = format!("art_{}_{}", entity.id.as_str(), suffix);
    let path = ProjectPath::parse(format!("{root}/{relative}")).map_err(CompileError::new)?;
    // A JSON file has nowhere to carry a comment, so only the Java gets the
    // provenance banner every other managed source has.
    let bytes = if kind == FileKind::Resource {
        body
    } else {
        format!(
            "// Generated by jails from {artifact}. Clean hand edits survive regeneration.\n{body}"
        )
    };
    Ok((
        path,
        RenderedFile {
            kind,
            mode: FileMode::Regular,
            bytes: bytes.into_bytes(),
            provenance: Provenance {
                artifact_id: artifact,
                ejection_id: None,
                ejectable: true,
                semantic_ids: BTreeSet::from([entity.id.as_str().to_string()]),
                compiler_pass: "seed".to_string(),
            },
        },
    ))
}

/// One import line, or nothing when the two packages are the same.
fn import(user: &str, owner: &str, class: &str) -> String {
    if user == owner {
        String::new()
    } else {
        format!("import {owner}.{class};\n")
    }
}
