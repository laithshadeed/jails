//! `g seed <Resource>`: development data, in a file, applied through the port.
//!
//! `missing.md` M10. jails wrote `src/test/resources/fixtures/*.json` for
//! generated tests and nothing at all for `dev`, and the absence has a
//! consequence: one ported project ends up calling `ensureSeedUsers()` from
//! inside a `GET` handler, because there was nowhere else to put it.
//!
//! Three decisions:
//!
//! - **Through the repository port, never JDBC.** A seeder that writes SQL
//!   bypasses the record's own constructor, so seed data becomes the one
//!   dataset in the project that nothing validates. `save` means a row the
//!   domain would reject fails at start-up rather than sitting in the table.
//! - **Behind a profile, not a property.** `@Profile("seed")` cannot be
//!   reached by accident and reads, at the class, as the answer to "does this
//!   run in production": no.
//! - **In `src/main/resources`, not `src/test`.** This is the `dev` dataset. A
//!   fixture is what a test asserts against, and they are not the same list --
//!   `jails new` already seeds `src/test/resources/fixtures`.

use super::workflow::json_sample;
use super::*;

pub(crate) fn seed_files(slice: &Slice, name: &str) -> Result<Vec<Artifact>> {
    let project = slice.project();
    if !project.has_jdbc() {
        return Err(format!(
            "seed {name} loads rows through {name}Repository, which needs a database.\n       \
             fix: run `jails add db` first."
        )
        .into());
    }
    let root: &Path = project.root();
    let domain: &str = &slice.owned(Layer::Domain);
    let app: &str = &slice.owned(Layer::App);
    let adapters: &str = &slice.placed(Layer::Adapters);
    let json = json_reader(slice, name)?;
    let fields = Target::read(slice, "seed", name, name)?.fields;
    let table = crate::sql::table_name(name);
    let resource = format!("db/seeds/{table}.json");
    let row = seed_row(slice, &fields);
    let disabled = match &row {
        Some(_) => String::new(),
        None => format!(
            "    @Disabled(\"todo: jails could not write a sample of every {name} component; \
             fill in {resource} by hand, then delete this @Disabled\")\n"
        ),
    };
    Ok(vec![
        Artifact {
            kind: "seed data",
            path: root
                .join("src/main/resources/db/seeds")
                .join(format!("{table}.json")),
            contents: match &row {
                Some(components) => format!("[\n  {{\n{components}\n  }}\n]\n"),
                None => "[]\n".to_string(),
            },
        },
        Artifact {
            kind: "seeder",
            path: crate::generate::main_dir(root, adapters).join(format!("{name}Seeder.java")),
            contents: crate::template::render(
                crate::template_here!("spring/seeder_java.java"),
                &[
                    ("pkg", adapters),
                    (
                        "imports",
                        &format!(
                            "{}{}{}",
                            crate::generate::import_of(adapters, domain, name),
                            crate::generate::import_of(adapters, app, &format!("{name}Repository")),
                            crate::generate::import_of(
                                adapters,
                                &slice.owned(Layer::Adapters),
                                &json
                            ),
                        ),
                    ),
                    ("name", name),
                    ("resource", &resource),
                    ("json", &json),
                ],
            ),
        },
        Artifact {
            kind: "seeder test",
            path: crate::generate::test_dir(root, adapters).join(format!("{name}SeederTest.java")),
            contents: crate::template::render(
                crate::template_here!("spring/seeder_test_java.java"),
                &[
                    ("pkg", adapters),
                    (
                        "imports",
                        &crate::generate::import_of(adapters, domain, name),
                    ),
                    ("name", name),
                    ("resource", &resource),
                    (
                        "disabled_import",
                        match &row {
                            Some(_) => "",
                            None => "import org.junit.jupiter.api.Disabled;\n",
                        },
                    ),
                    ("disabled", &disabled),
                ],
            ),
        },
    ])
}

/// The project's JSON reader, by name, or a refusal naming the fix.
///
/// `add json --name X` writes `XJson`, so the class cannot be assumed to be
/// called `Json` -- and a seeder that names one the project does not have is
/// a compile error in a file the reader did not write. Two of them is refused
/// rather than picked between, on `source.rs`'s rule: choosing silently sends
/// the generated code at the wrong one.
fn json_reader(slice: &Slice, name: &str) -> Result<String> {
    let adapters = slice.owned(Layer::Adapters);
    let directory = format!("src/main/java/{}", adapters.replace('.', "/"));
    let readers: Vec<String> = slice
        .project()
        .projected_names_in(&directory)
        .into_iter()
        .filter_map(|file| {
            file.strip_suffix("Json.java")
                .map(|base| format!("{base}Json"))
        })
        .collect();
    match readers.as_slice() {
        [reader] => Ok(reader.clone()),
        [] => Err(format!(
            "seed {name} reads a JSON file and the project has no JSON reader.\n       fix: run \
             `jails add json` first."
        )
        .into()),
        many => Err(format!(
            "seed {name} found {} JSON readers in {adapters} ({}), so it cannot tell which one \
             the seed file should be read with.\n       fix: pass `--package` naming the one to \
             generate beside.",
            many.len(),
            many.join(", ")
        )
        .into()),
    }
}

/// One row, keyed by record component -- which is what Jackson binds -- and
/// only when every component has a sample jails can prove how to write.
///
/// An empty array is not a lesser version of the same thing: the generated
/// test reads this file back, and over `[]` it would pass while proving
/// nothing. So a partial row is never emitted; the file is empty and the test
/// is `@Disabled` naming what to do, which is the same trade `sample_value`
/// makes everywhere else.
fn seed_row(slice: &Slice, fields: &[crate::generate::Field]) -> Option<String> {
    if fields.is_empty() {
        return None;
    }
    let components: Option<Vec<String>> = fields
        .iter()
        .map(|field| {
            json_sample(slice, field).map(|value| format!("    \"{}\": {value}", field.name))
        })
        .collect();
    Some(components?.join(",\n"))
}
