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
use crate::emit_java::JavaUnit;
use jails_contracts::{FileKind, FileMode, ProjectPath, Provenance, RenderedFile};
use jails_model::{AppModel, Capability, Entity, Package, StableId, TypeRef, boundary};
use std::collections::BTreeSet;

const JAVA_MAIN_ROOT: &str = ".jails/generated/main/java";
const JAVA_TEST_ROOT: &str = ".jails/generated/test/java";
const RESOURCE_ROOT: &str = ".jails/generated/main/resources";

const SEEDER: crate::Template = crate::template!("spring/seeder_java.java");
const TEST: crate::Template = crate::template!("spring/seeder_test_java.java");

pub(crate) fn lower(
    model: &AppModel,
    entity: &Entity,
    templates: &jails_contracts::TemplateOverrides,
) -> Result<Vec<(ProjectPath, RenderedFile)>, CompileError> {
    let reader = json_reader(model, entity)?;
    let name = &entity.names.java_type;
    let adapters = crate::emit_java::entity_package(model, entity, Package::Adapters);
    let domain = crate::emit_java::entity_package(model, entity, Package::Domain);
    let repository = crate::emit_java::entity_package(model, entity, Package::Repository);
    let resource = format!("db/seeds/{}.json", entity.names.sql_table);
    let rows = [row(model, entity, true), row(model, entity, false)];
    let mut seeder = JavaUnit::from_source(
        &SEEDER
            .resolve(templates)?
            .replace("{{pkg}}", &adapters)
            .replace("{{resource}}", &resource)
            .replace("{{json}}", &reader)
            .replace("{{name}}", name),
    );
    seeder.import_from(&domain, name);
    seeder.import_from(&repository, &format!("{name}Repository"));
    // The reader is a class of this same package, so this adds nothing today
    // -- and stays because `cap json` may be placed elsewhere.
    seeder.import_from(&adapters, &reader);
    let disabled = rows[0].is_none() || rows[1].is_none();
    let mut test = JavaUnit::from_source(
        &TEST
            .resolve(templates)?
            .replace("{{pkg}}", &adapters)
            .replace("{{resource}}", &resource)
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
            .replace("{{name}}", name),
    );
    test.import_from(&domain, name);
    if disabled {
        test.import("org.junit.jupiter.api.Disabled");
    }

    Ok(vec![
        // The data itself, and an empty array when jails could not sample
        // every component: a file with a row it cannot honestly fill would
        // fail to bind, which is worse than an empty one the test reports.
        rendered(
            entity,
            &boundary::SEED_DATA,
            RESOURCE_ROOT,
            &resource,
            FileKind::Resource,
            Rendered::Resource(match (&rows[0], &rows[1]) {
                (Some(first), Some(second)) => {
                    format!("[\n  {{\n{first}\n  }},\n  {{\n{second}\n  }}\n]\n")
                }
                _ => "[]\n".to_string(),
            }),
        )?,
        rendered(
            entity,
            &boundary::SEEDER,
            JAVA_MAIN_ROOT,
            &format!("{}/{name}Seeder.java", adapters.replace('.', "/")),
            FileKind::JavaMain,
            Rendered::Java(seeder),
        )?,
        rendered(
            entity,
            &boundary::SEEDER_TEST,
            JAVA_TEST_ROOT,
            &format!("{}/{name}SeederTest.java", adapters.replace('.', "/")),
            FileKind::JavaTest,
            Rendered::Java(test),
        )?,
    ])
}

/// One row of this entity, as JSON, or `None` when a component has no sample.
///
/// A project-owned type is one jails cannot invent a value of. Writing a guess
/// would ship a file that fails to bind at start-up, so the row is left out
/// and the generated test is `@Disabled` naming what to fill in -- the same
/// trade `sample_value` makes for a generated record test.
fn row(model: &AppModel, entity: &Entity, first: bool) -> Option<String> {
    if entity.fields.is_empty() {
        return None;
    }
    entity
        .fields
        .iter()
        .map(|field| {
            // **The second row leaves every optional component out.** One row
            // proves the file binds; the pair proves the loader reads more
            // than one and that an absent optional is absent rather than the
            // four-character string `null`.
            if !first && !field.required {
                return Some(format!("    \"{}\": null", field.names.java_member));
            }
            let value = match &field.ty {
                // **The second row's values differ, not only its absences.**
                // Two identical rows are a duplicate key the moment the entity
                // has one, and a seed file that fails to bind is worse than no
                // seed file at all.
                TypeRef::Builtin(builtin) => if first {
                    builtin.semantics().json
                } else {
                    builtin.semantics().json_alternate
                }
                .to_string(),
                // **An enum is the one project type jails can sample**, and by
                // its wire value: the record's converter reads that, and the
                // Java constant is what it refuses. Leaving it out would make
                // every seeded entity with an enum column ship `[]` and a
                // `@Disabled` test.
                TypeRef::External(name) => enum_sample(model, name)?,
            };
            Some(format!("    \"{}\": {value}", field.names.java_member))
        })
        .collect::<Option<Vec<_>>>()
        .map(|components| components.join(",\n"))
}

/// The first constant of a declared enum, as the JSON a caller would send.
fn enum_sample(model: &AppModel, java_type: &str) -> Option<String> {
    let declared = model
        .entities
        .values()
        .find(|entity| entity.names.java_type == java_type)
        .filter(|entity| entity.facets.contains(&jails_model::Facet::Enum))?;
    let constant = declared.enum_constants.first()?;
    Some(format!(
        "\"{}\"",
        constant.wire_name.as_deref().unwrap_or(&constant.java_name)
    ))
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
    boundary: &boundary::Boundary,
    root: &str,
    relative: &str,
    kind: FileKind,
    body: Rendered,
) -> Result<(ProjectPath, RenderedFile), CompileError> {
    let artifact = boundary.owned_by(entity.id.as_str());
    let path = ProjectPath::parse(format!("{root}/{relative}")).map_err(CompileError::new)?;
    // A JSON file has nowhere to carry a comment, so only the Java gets the
    // provenance banner every other managed source has.
    let bytes = match body {
        Rendered::Resource(text) => text,
        Rendered::Java(unit) => unit.render(&artifact),
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

/// A seed artifact is either the data or a Java unit; only the second has
/// anywhere to carry a provenance header.
enum Rendered {
    Resource(String),
    Java(JavaUnit),
}
