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

/// The lock this project has, rewritten in the shape the release before v5
/// wrote: every managed file's bytes inline, and no `.jails/base` beside it.
///
/// **The upgrade path is a fixture, not a story.** A lock is a file in
/// somebody's repository, so the only honest test of "an older one still
/// reads" is to hand the binary one -- built from the base tree it replaced,
/// which is where those bytes now live. `array` writes them the way every
/// release before v4 did, four characters per byte, so the oldest spelling is
/// exercised too.
fn downgrade_lock_to_v4(root: &Path, array: bool) {
    let lock_path = root.join(".jails/compiler.lock.json");
    let mut lock: serde_json::Value =
        serde_json::from_slice(&fs::read(&lock_path).unwrap()).unwrap();
    let base = lock["base"].as_object().expect("a v5 lock names its base");
    let mut files = serde_json::Map::new();
    for (path, entry) in base["files"].as_object().unwrap() {
        let bytes = fs::read(root.join(".jails/base").join(path)).unwrap();
        let mut file = serde_json::Map::new();
        file.insert("kind".to_string(), entry["kind"].clone());
        file.insert("mode".to_string(), entry["mode"].clone());
        file.insert("provenance".to_string(), entry["provenance"].clone());
        if array {
            file.insert(
                "bytes".to_string(),
                serde_json::Value::Array(
                    bytes
                        .iter()
                        .map(|byte| serde_json::Value::Number((*byte).into()))
                        .collect(),
                ),
            );
        } else {
            file.insert(
                "text".to_string(),
                serde_json::Value::String(String::from_utf8(bytes).unwrap()),
            );
        }
        files.insert(path.clone(), serde_json::Value::Object(file));
    }
    let mut projection = serde_json::Map::new();
    projection.insert("files".to_string(), serde_json::Value::Object(files));
    if let Some(facets) = base.get("reader_facets") {
        projection.insert("reader_facets".to_string(), facets.clone());
    }
    let object = lock.as_object_mut().unwrap();
    object.remove("base");
    object.insert(
        "projection".to_string(),
        serde_json::Value::Object(projection),
    );
    object.insert(
        "schema".to_string(),
        serde_json::Value::String(
            if array {
                "jails.compiler-lock.v3"
            } else {
                "jails.compiler-lock.v4"
            }
            .to_string(),
        ),
    );
    fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();
    fs::remove_dir_all(root.join(".jails/base")).unwrap();
}

/// Seal the merge base as it now stands on disk: every entry's digest, and
/// the digest of the tree they make.
///
/// **The fixture for "the accepted projection was written by something
/// else".** A test that wants an older emitter's bytes as the merge base
/// writes them into `.jails/base` and calls this; the lock then says exactly
/// what is there, which is what capture checks.
fn reseal_base(root: &Path) {
    let lock_path = root.join(".jails/compiler.lock.json");
    let mut lock: serde_json::Value =
        serde_json::from_slice(&fs::read(&lock_path).unwrap()).unwrap();
    let mut projection = jails_contracts::RenderedTree::new();
    let entries = lock["base"]["files"].as_object().unwrap().clone();
    for (path, entry) in &entries {
        let bytes = fs::read(root.join(".jails/base").join(path)).unwrap();
        let digest = format!(
            "sha256:{}",
            jails_support::hex(&jails_support::sha256(&bytes))
        );
        lock["base"]["files"][path]["digest"] = serde_json::Value::String(digest);
        projection.files.insert(
            jails_contracts::ProjectPath::parse(path.clone()).unwrap(),
            jails_contracts::RenderedFile {
                kind: serde_json::from_value(entry["kind"].clone()).unwrap(),
                mode: serde_json::from_value(entry["mode"].clone()).unwrap(),
                bytes,
                provenance: serde_json::from_value(entry["provenance"].clone()).unwrap(),
            },
        );
    }
    if let Some(facets) = lock["base"].get("reader_facets") {
        projection.reader_facets = serde_json::from_value(facets.clone()).unwrap();
    }
    let encoded = serde_json::to_vec(&projection).unwrap();
    lock["projection_digest"] = serde_json::Value::String(format!(
        "sha256:{}",
        jails_support::hex(&jails_support::sha256(&encoded))
    ));
    fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();
}
