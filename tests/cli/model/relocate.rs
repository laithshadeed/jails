//! `model relocate`: the one-time move of a project generated before managed
//! output lived under `src/`, through the one executor like every write.
use super::*;

/// Turn a current project into the shape an older release left: managed
/// files under `.jails/generated/<set>/<kind>`, a lock naming them there, and
/// the marked source-root block in the pom.
fn age(root: &Path) -> Vec<(String, String)> {
    let lock_path = root.join(".jails/compiler.lock.json");
    let mut lock: serde_json::Value =
        serde_json::from_slice(&fs::read(&lock_path).unwrap()).unwrap();
    let projection: jails_contracts::RenderedTree =
        serde_json::from_value(lock["projection"].clone()).unwrap();
    let mut moved = Vec::new();
    let mut aged = jails_contracts::RenderedTree::new();
    for (path, file) in projection.files {
        let old = path
            .as_str()
            .replacen("src/test/http/", ".jails/generated/requests/", 1)
            .replacen("src/", ".jails/generated/", 1);
        let from = root.join(path.as_str());
        let to = root.join(&old);
        fs::create_dir_all(to.parent().unwrap()).unwrap();
        fs::rename(&from, &to).unwrap();
        moved.push((old.clone(), path.as_str().to_string()));
        aged.files
            .insert(jails_contracts::ProjectPath::parse(old).unwrap(), file);
    }
    aged.reader_facets = projection.reader_facets;
    let encoded = serde_json::to_vec(&aged).unwrap();
    lock["projection_digest"] = serde_json::json!(format!(
        "sha256:{}",
        jails_support::hex(&jails_support::sha256(&encoded))
    ));
    lock["projection"] = serde_json::to_value(&aged).unwrap();
    fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();

    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    let block = "    <build>\n        <plugins>\n            <!-- jails:generated-source-roots -->\n            <plugin>\n                <groupId>org.codehaus.mojo</groupId>\n                <artifactId>build-helper-maven-plugin</artifactId>\n            </plugin>\n            <!-- /jails:generated-source-roots -->\n        </plugins>\n    </build>\n</project>\n";
    let aged_pom = pom.replace("</project>\n", block);
    assert_ne!(aged_pom, pom);
    fs::write(root.join("pom.xml"), aged_pom).unwrap();
    moved
}

#[test]
fn model_relocate_moves_every_managed_file_under_src_and_rewrites_the_lock() {
    let root = model_project("model-relocate", MODEL);
    apply_canonical_model(&root, "initial-plan");
    let moved = age(&root);
    assert!(!moved.is_empty());
    // A hand edit travels with the file: the captured bytes move, not a
    // fresh render.
    let (old_record, new_record) = moved
        .iter()
        .find(|(_, new)| new.ends_with("/domain/Note.java"))
        .cloned()
        .unwrap();
    let mut edited = fs::read_to_string(root.join(&old_record)).unwrap();
    edited.push_str("// reader edit before the move\n");
    fs::write(root.join(&old_record), &edited).unwrap();

    let before = snapshot_tree(&root);
    let preview = jails_cmd(&root, None)
        .args(["model", "relocate", "--pretend"])
        .output()
        .unwrap();
    assert!(
        preview.status.success(),
        "{}",
        String::from_utf8_lossy(&preview.stderr)
    );
    let shown = String::from_utf8_lossy(&preview.stdout);
    assert!(shown.contains("nothing was written."), "{shown}");
    assert_eq!(snapshot_tree(&root), before, "--pretend wrote files");

    let applied = jails_cmd(&root, None)
        .args(["model", "relocate"])
        .output()
        .unwrap();
    assert!(
        applied.status.success(),
        "{}",
        String::from_utf8_lossy(&applied.stderr)
    );
    for (old, new) in &moved {
        assert!(!root.join(old).exists(), "{old} is still there");
        assert!(root.join(new).is_file(), "{new} did not arrive");
    }
    assert_eq!(fs::read_to_string(root.join(&new_record)).unwrap(), edited);
    assert!(
        !root.join(".jails/generated").exists(),
        "the old root was left behind"
    );
    let lock = fs::read_to_string(root.join(".jails/compiler.lock.json")).unwrap();
    assert!(
        !lock.contains(".jails/generated"),
        "the lock still names the old root"
    );
    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    assert!(!pom.contains("jails:generated-source-roots"), "{pom}");
    assert!(!pom.contains("build-helper-maven-plugin"), "{pom}");

    // The project is frozen on the first ask: the move is the whole change.
    let frozen = jails_cmd(&root, None)
        .args(["model", "check", "--frozen"])
        .output()
        .unwrap();
    assert!(
        frozen.status.success(),
        "{}",
        String::from_utf8_lossy(&frozen.stderr)
    );
    let again = jails_cmd(&root, None)
        .args(["model", "relocate"])
        .output()
        .unwrap();
    assert!(again.status.success());
    assert!(
        String::from_utf8_lossy(&again.stdout).contains("nothing to relocate"),
        "{}",
        String::from_utf8_lossy(&again.stdout)
    );
}

#[test]
fn model_relocate_refuses_an_occupied_destination_without_writing() {
    let root = model_project("model-relocate-collision", MODEL);
    apply_canonical_model(&root, "initial-plan");
    let moved = age(&root);
    let (_, destination) = moved
        .iter()
        .find(|(_, new)| new.ends_with("/domain/Note.java"))
        .cloned()
        .unwrap();
    fs::create_dir_all(root.join(&destination).parent().unwrap()).unwrap();
    fs::write(
        root.join(&destination),
        "package com.example.notes.domain;\n",
    )
    .unwrap();
    let before = snapshot_tree(&root);

    let refused = jails_cmd(&root, None)
        .args(["model", "relocate"])
        .output()
        .unwrap();
    assert!(!refused.status.success());
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(stderr.contains(&destination), "{stderr}");
    assert!(stderr.contains("already exists"), "{stderr}");
    assert_eq!(snapshot_tree(&root), before, "the refusal wrote bytes");
}

/// `--output json` names the pass that refused, and says the same sentence.
///
/// **Both halves in one test, because they are one property.** Adopting the
/// diagnostic contract added a code and was not allowed to reword anything, so
/// this drives one refusal twice -- once human, once machine -- and asserts
/// that the JSON `error.message` is exactly the human line with `jails: `
/// taken off, while `error.code` is the workspace code for *this* refusal
/// rather than the constant `invalid-request` every refusal used to carry.
#[test]
fn a_refused_relocation_reports_the_code_of_the_pass_and_the_same_sentence() {
    let root = model_project("model-relocate-json-code", MODEL);
    apply_canonical_model(&root, "initial-plan");
    let moved = age(&root);
    let (_, destination) = moved
        .iter()
        .find(|(_, new)| new.ends_with("/domain/Note.java"))
        .cloned()
        .unwrap();
    fs::create_dir_all(root.join(&destination).parent().unwrap()).unwrap();
    fs::write(
        root.join(&destination),
        "package com.example.notes.domain;\n",
    )
    .unwrap();

    let human = jails_cmd(&root, None)
        .args(["model", "relocate"])
        .output()
        .unwrap();
    assert!(!human.status.success());
    let stderr = String::from_utf8_lossy(&human.stderr);
    let told = stderr
        .trim_end()
        .strip_prefix("jails: ")
        .unwrap_or_else(|| panic!("{stderr}"))
        .to_string();

    let machine = jails_cmd(&root, None)
        .args(["model", "relocate", "--output", "json"])
        .output()
        .unwrap();
    assert!(!machine.status.success());
    let envelope: serde_json::Value = serde_json::from_slice(&machine.stdout)
        .unwrap_or_else(|error| panic!("{error}: {}", String::from_utf8_lossy(&machine.stdout)));
    assert_eq!(
        envelope["error"]["code"],
        serde_json::json!("workspace-relocate-destination-exists"),
        "{envelope}"
    );
    assert_eq!(
        envelope["error"]["message"].as_str().unwrap(),
        told,
        "the machine and human renderings of one refusal disagree"
    );
    assert!(
        told.contains("fix: move or remove your file first"),
        "{told}"
    );
}
