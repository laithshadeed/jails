//! The `jails` model commands, end to end through the binary.
//!
//! One submodule per subject a reader looks for; the fixtures and the project
//! builders every submodule shares live here, and each submodule reaches them
//! with `use super::*;`.

use super::*;

mod adopt;
mod build;
mod capability;
mod database;
mod destroy;
mod eject;
mod generate;
mod merge;
mod operations;
mod plan;
mod relocate;
mod reports;
mod resource;
mod source;

/// The smallest `jdl 1` model a Spring fixture can carry, for the tests that
/// build everything else with `jails g`.
///
/// `storage none`: every scenario that wants storage adds it with `add db` or
/// `add h2`, and a seed that declared it would hand forty tests a JDBC
/// adapter and a migration none of them asked for.
const DEMO_JDL: &str = "jdl 1\n\napp Demo @id(project_demo) {\n  pkg com.example.demo\n  \
     java 26\n  platform spring\n  build maven\n  storage none\n}\n";

/// [`DEMO_JDL`] for the fixtures whose package is `com.example.notes`.
const NOTES_JDL: &str = "jdl 1\n\napp Notes @id(project_notes) {\n  pkg com.example.notes\n  \
     java 26\n  platform spring\n  build maven\n  storage none\n}\n";

/// A project seeded with one of the model fixtures below; the same project as
/// [`jdl_project`].
fn model_project(label: &str, source: &str) -> PathBuf {
    jdl_project(label, source)
}

/// [`NOTES_JDL`] with the resource these tests mutate.
///
/// `use repo`, `use service` and `use http` rather than `use scaffold`:
/// `scaffold` would add a DTO nothing here asked for.
const MODEL: &str = "jdl 1\n\napp Notes @id(project_notes) {\n  pkg com.example.notes\n  \
     java 26\n  platform spring\n  build maven\n  storage none\n}\n\n\
     entity Note @id(ent_note) {\n  use repo\n  use service\n  use http\n\n  \
     id: uuid @id(fld_note_id) @pk\n  title: string @id(fld_note_title) @notBlank\n\n  \
     command CreateNote(title) @id(op_create_note) {\n    route POST \"/notes\"\n  }\n}\n";

/// The same project with no resource in it, for the tests that declare their
/// own. Identical to [`NOTES_JDL`], and named for what it is used as.
const EMPTY_MODEL: &str = NOTES_JDL;

/// The same project with a Gradle build instead of Maven.
///
/// **The pom has to go rather than be joined by a second build file.** Capture
/// refuses a module with both by name, and the model's `build` axis has to
/// name what is on disk or the dependency adapter reconciles into a file the
/// project does not have.
fn gradle_model_project(label: &str, source: &str, build_file: &str, build: &str) -> PathBuf {
    let root = model_project(label, &source.replace("build maven", "build gradle"));
    fs::remove_file(root.join("pom.xml")).unwrap();
    fs::write(root.join(build_file), build).unwrap();
    root
}

/// A canonical project whose authoring source is written by hand.
///
/// It carries a real Spring build, because the models these tests write
/// declare `platform spring` -- the default -- and the compiler will not emit
/// a `@RestController` into a project whose build has no Spring Boot in it.
/// A bare directory holding only `.jails/model.jdl` is not a project any of
/// this could compile into, so proving JDL editing against one would prove it
/// against a shape nobody has.
fn jdl_project(label: &str, source: &str) -> PathBuf {
    let root = temp_dir(label);
    write_spring_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(root.join(".jails/model.jdl"), source).unwrap();
    root
}

fn eject_model_project(label: &str) -> PathBuf {
    model_project(label, &format!("{MODEL}\ncap fake @id(cap_fake)\n"))
}

fn apply_canonical_model(root: &Path, label: &str) {
    let bundle = root.join(format!("{label}.json"));
    let planned = jails_cmd(root, None)
        .args(["model", "plan", "--bundle"])
        .arg(&bundle)
        .output()
        .unwrap();
    assert!(
        planned.status.success(),
        "{}",
        String::from_utf8_lossy(&planned.stderr)
    );
    let applied = jails_cmd(root, None)
        .args(["model", "apply", "--bundle"])
        .arg(&bundle)
        .output()
        .unwrap();
    assert!(
        applied.status.success(),
        "{}",
        String::from_utf8_lossy(&applied.stderr)
    );
}

fn canonical_database_project(label: &str) -> PathBuf {
    let root = model_project(label, EMPTY_MODEL);
    write_spring_fixture(&root);
    for arguments in [
        vec!["g", "scaffold", "Note", "id:uuid@pk", "title:string!"],
        vec!["add", "db", "--no-start"],
    ] {
        let output = jails_cmd(&root, None).args(arguments).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    root
}

/// One model source with its member columns collapsed to a single space.
///
/// **What a declaration says, not which column it says it in.** The formatter
/// lines up the type column of a run of members, so `title: string` is
/// `title:     string` beside a `createdAt`, and an assertion about the
/// attributes on a field would otherwise have to restate a width that moves
/// whenever a sibling field is added. The two tests that are *about* the
/// columns assert them directly and do not come through here.
pub fn unaligned(source: &str) -> String {
    source
        .split_inclusive('\n')
        .map(|line| {
            let indent = line.len() - line.trim_start().len();
            let mut collapsed = line[..indent].to_string();
            let mut spaces = 0;
            for character in line[indent..].chars() {
                if character == ' ' {
                    spaces += 1;
                    continue;
                }
                if spaces > 0 {
                    collapsed.push(' ');
                    spaces = 0;
                }
                collapsed.push(character);
            }
            collapsed.push_str(&" ".repeat(spaces));
            collapsed
        })
        .collect()
}
