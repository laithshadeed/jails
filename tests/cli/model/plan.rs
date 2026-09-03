//! The exact plan: `model plan` writes a bundle, `model apply` consumes that
//! bundle without replanning, and a before-image that moved between the two
//! refuses every write.
//!
use super::*;

#[test]
fn reviewed_model_format_refuses_a_concurrent_source_edit() {
    let root = jdl_project(
        "jdl-v1-format-stale",
        "jdl 1\r\napp Notes {\r\n pkg com.example.notes\r\n java 26\r\n platform spring\r\n build maven\r\n storage postgres\r\n}\r\n",
    );
    write_spring_fixture(&root);
    let model_path = root.join(".jails/model.jdl");
    let bundle = root.join("format-plan.json");
    let planned = jails_cmd(&root, None)
        .args(["model", "fmt", "--plan-out"])
        .arg(&bundle)
        .output()
        .unwrap();
    assert!(
        planned.status.success(),
        "{}",
        String::from_utf8_lossy(&planned.stderr)
    );

    let mut changed = fs::read_to_string(&model_path).unwrap();
    changed.push_str("// concurrent reader edit\r\n");
    fs::write(&model_path, &changed).unwrap();
    let before = fs::read(&model_path).unwrap();
    let applied = jails_cmd(&root, None)
        .args(["model", "apply", "--bundle"])
        .arg(&bundle)
        .output()
        .unwrap();
    assert!(!applied.status.success());
    assert!(
        String::from_utf8_lossy(&applied.stderr)
            .contains("no longer matches what this plan was reviewed against"),
        "{}",
        String::from_utf8_lossy(&applied.stderr)
    );
    assert_eq!(fs::read(&model_path).unwrap(), before);
}

#[test]
fn jdl_v1_cases_source_is_an_exact_plan_input() {
    let root = jdl_project(
        "jdl-v1-cases-stale-source",
        r#"jdl 1
app Notes {
  pkg com.example.notes
  java 26
  platform plain
  build maven
  storage none
}
"#,
    );
    let source_path = root.join("acceptance.md");
    fs::write(&source_path, "# Acceptance\n\n- first behavior\n").unwrap();
    let bundle = root.join("cases-plan.json");
    let planned = jails_cmd(&root, None)
        .args(["g", "cases", "acceptance.md", "--plan-out"])
        .arg(&bundle)
        .output()
        .unwrap();
    assert!(
        planned.status.success(),
        "{}",
        String::from_utf8_lossy(&planned.stderr)
    );
    assert!(bundle.is_file());
    assert!(
        !fs::read_to_string(root.join(".jails/model.jdl"))
            .unwrap()
            .contains("component cases"),
        "planning must not write the desired state"
    );

    fs::write(
        &source_path,
        "# Acceptance\n\n- first behavior\n- changed after review\n",
    )
    .unwrap();
    let before = snapshot_tree(&root);
    let applied = jails_cmd(&root, None)
        .args(["model", "apply", "--bundle"])
        .arg(&bundle)
        .output()
        .unwrap();
    assert!(!applied.status.success());
    assert!(
        String::from_utf8_lossy(&applied.stderr)
            .contains("no longer matches what this plan was reviewed against"),
        "{}",
        String::from_utf8_lossy(&applied.stderr)
    );
    let without_executor_lock = |mut tree: Vec<(PathBuf, Vec<u8>)>| {
        tree.retain(|(path, _)| !path.ends_with(".jails/apply.lock"));
        tree
    };
    assert_eq!(
        without_executor_lock(snapshot_tree(&root)),
        without_executor_lock(before),
        "a stale cases input wrote part of the component mutation"
    );
}

#[test]
fn model_plan_is_deterministic_and_writes_a_self_verifying_bundle() {
    let root = model_project("model-plan", MODEL);
    let first = root.join("first-plan.json");
    let second = root.join("second-plan.json");
    for path in [&first, &second] {
        let output = jails_cmd(&root, None)
            .args(["model", "plan", "--bundle"])
            .arg(path)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    assert_eq!(fs::read(&first).unwrap(), fs::read(&second).unwrap());
    let bundle: jails_contracts::PlanBundle =
        serde_json::from_slice(&fs::read(&first).unwrap()).unwrap();
    jails_workspace::verify_bundle(&bundle).unwrap();
    assert_eq!(bundle.plan.operations.len(), 3);
    // Fifteen: the record and its companion test, the repository port, the
    // in-memory adapter that implements it -- without which the service has a
    // port no bean satisfies -- that adapter's own test and the repository
    // contract they share, the service the controller delegates to, the HTTP
    // port, the controller and its test, and the project's `ArchitectureTest`
    // with the `archunit.properties` that points its freeze store at a
    // baseline the reader has to create deliberately. Then the three the
    // operation brings: its `Command` ABI, the `TimeOrderedUuid` its `uuid`
    // key is minted with, and the `.http` request file its route is callable
    // from.
    assert_eq!(bundle.plan.summary.managed_files, 15);
    assert_eq!(bundle.plan.id, bundle.plan.digest.as_str());
}

#[test]
fn model_apply_consumes_the_reviewed_plan_without_recompiling() {
    let root = model_project("model-apply", MODEL);
    let plan = root.join("plan.json");
    let planned = jails_cmd(&root, None)
        .args(["model", "plan", "--bundle"])
        .arg(&plan)
        .output()
        .unwrap();
    assert!(planned.status.success());

    let applied = jails_cmd(&root, None)
        .args(["model", "apply", "--bundle"])
        .arg(&plan)
        .output()
        .unwrap();
    assert!(
        applied.status.success(),
        "{}",
        String::from_utf8_lossy(&applied.stderr)
    );
    let record = root.join("src/main/java/com/example/notes/domain/Note.java");
    let source = fs::read_to_string(record).unwrap();
    assert!(source.contains("public record Note"), "{source}");
    assert!(source.contains("UUID id"), "{source}");
    let command = fs::read_to_string(
        root.join("src/main/java/com/example/notes/application/commands/CreateNoteCommand.java"),
    )
    .unwrap();
    assert!(
        command.contains("String ROUTE = \"POST /notes\""),
        "{command}"
    );
    assert!(command.contains("Note execute(Input input)"), "{command}");
    assert!(command.contains("public record Input"), "{command}");

    let retried = jails_cmd(&root, None)
        .args(["--output", "json", "model", "apply", "--bundle"])
        .arg(&plan)
        .output()
        .unwrap();
    assert!(retried.status.success());
    let execution: serde_json::Value = serde_json::from_slice(&retried.stdout).unwrap();
    assert!(
        execution["files"]
            .as_array()
            .is_some_and(|files| files.is_empty()),
        "a re-apply changes nothing, so the report lists nothing: {execution}"
    );

    let frozen = jails_cmd(&root, None)
        .args(["model", "check", "--frozen"])
        .output()
        .unwrap();
    assert!(
        frozen.status.success(),
        "{}",
        String::from_utf8_lossy(&frozen.stderr)
    );
}

#[test]
fn model_apply_rejects_a_stale_plan_before_writing() {
    let root = model_project("model-stale", MODEL);
    let plan = root.join("plan.json");
    let planned = jails_cmd(&root, None)
        .args(["model", "plan", "--bundle"])
        .arg(&plan)
        .output()
        .unwrap();
    assert!(planned.status.success());
    fs::write(
        root.join(".jails/model.jdl"),
        format!("{MODEL}\n# changed\n"),
    )
    .unwrap();

    let applied = jails_cmd(&root, None)
        .args(["model", "apply", "--bundle"])
        .arg(&plan)
        .output()
        .unwrap();
    assert!(!applied.status.success());
    let stderr = String::from_utf8(applied.stderr).unwrap();
    assert!(
        stderr.contains("no longer matches what this plan was reviewed against"),
        "{stderr}"
    );
    assert!(!root.join(".jails/compiler.lock.json").exists());
}

/// The digest a reader reviews is the digest that gets applied: preview, plan
/// export, confirmation and apply reference the same digest, which is the
/// whole reason apply may execute a bundle rather than recompute one.
///
/// Four surfaces, one number. `--pretend` is what a human reads, `--plan-out`
/// is what leaves the machine, and the execution report is what happened; a
/// disagreement between any two would mean the review was of something else.
#[test]
fn preview_export_and_apply_all_name_one_plan_digest() {
    let root = temp_dir("one-plan-digest");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(
        root.join(".jails/model.jdl"),
        "jdl 1\napp Demo @id(project_demo) {\n  pkg com.example.demo\n  java 26\n  \
         platform spring\n  build maven\n  storage postgres\n}\n",
    )
    .unwrap();
    let synced = jails_cmd(&root, None).arg("sync").output().unwrap();
    assert!(
        synced.status.success(),
        "{}",
        String::from_utf8_lossy(&synced.stderr)
    );

    let mutation = ["g", "record", "Note", "id:uuid@pk", "title:string"];

    // 1. Preview, as a human reads it.
    let previewed = jails_cmd(&root, None)
        .args(mutation)
        .arg("--pretend")
        .output()
        .unwrap();
    assert!(
        previewed.status.success(),
        "{}",
        String::from_utf8_lossy(&previewed.stderr)
    );
    let preview = String::from_utf8_lossy(&previewed.stdout);
    let previewed_digest = preview
        .split_whitespace()
        .nth(1)
        .expect("`plan <digest>: …`")
        .trim_end_matches(':')
        .to_string();
    // `sha256:` plus 64 hex.
    assert_eq!(previewed_digest.len(), 71, "{preview}");

    // 2. Export, as it leaves the machine. `--plan-out` reports too, so this
    //    also covers the human line staying in step with the file.
    let bundle_path = root.join("plan.json");
    let exported = jails_cmd(&root, None)
        .args(mutation)
        .arg("--plan-out")
        .arg(&bundle_path)
        .output()
        .unwrap();
    assert!(
        exported.status.success(),
        "{}",
        String::from_utf8_lossy(&exported.stderr)
    );
    let bundle: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&bundle_path).unwrap()).unwrap();
    assert_eq!(bundle["plan"]["digest"].as_str().unwrap(), previewed_digest);

    // 3. Apply. Nothing has changed in between, so replanning -- if that is
    //    what it were doing -- would have to land on the same number, and the
    //    contract is that it does not replan at all.
    let applied = jails_cmd(&root, None)
        .args(mutation)
        .args(["--output", "json"])
        .output()
        .unwrap();
    assert!(
        applied.status.success(),
        "{}",
        String::from_utf8_lossy(&applied.stderr)
    );
    let execution: serde_json::Value =
        serde_json::from_slice(&applied.stdout).expect("an execution report");
    assert_eq!(
        execution["plan_digest"].as_str().unwrap(),
        previewed_digest,
        "the applied plan is not the reviewed one"
    );
}

/// The report is the change, and git is the oracle.
///
/// **`write` used to mean *in the plan*.** The managed tree's after-image
/// holds every path the model renders, touched or not, so `resource field
/// add Note tags:string` reported ten files written above a list of
/// twenty-two lines, nineteen of them over files `git status` showed
/// unchanged. The executor already skipped a file whose bytes were already on
/// disk, so the count under the list was right and the list was wrong.
///
/// So the oracle is git: after each mutation the report's file lines and
/// `git status --porcelain -uall` name the same paths, and therefore the same
/// number of them. `-uall` because a directory git has never seen is one
/// `status` collapses to a single `??` line, which would compare a report of
/// files against a listing of directories.
/// The lock records a generated file's bytes as text, and still reads a lock
/// written the old way.
///
/// **A byte as a JSON integer costs four characters**, and the lock is one
/// exact copy of every managed file, so a 25 kB tree was recorded as a 446 kB
/// lock. The digest rule did not change with the encoding -- it is still a
/// digest of the form `serde` derives -- which is what lets a lock from the
/// previous release verify and be rewritten in the new shape.
#[test]
fn the_lock_stores_managed_bytes_as_text_and_still_reads_the_old_array_form() {
    let root = model_project("model-lock-encoding", EMPTY_MODEL);
    let generated = jails_cmd(&root, None)
        .args(["g", "record", "Note", "title:string!"])
        .output()
        .unwrap();
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );

    let lock_path = root.join(".jails/compiler.lock.json");
    let lock: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&lock_path).unwrap()).unwrap();
    assert_eq!(lock["schema"], "jails.compiler-lock.v4");
    let files = lock["projection"]["files"].as_object().unwrap();
    let note = files
        .iter()
        .find(|(path, _)| path.ends_with("domain/Note.java"))
        .map(|(_, file)| file)
        .expect("the record is in the accepted projection");
    assert!(
        note["text"]
            .as_str()
            .is_some_and(|text| text.contains("record Note")),
        "the projection carries the file as text: {note}"
    );
    assert!(note.get("bytes").is_none(), "one spelling: {note}");

    // Now the old shape, byte for byte what the previous release wrote: the
    // same lock with `text` expanded back to an array and the schema it had.
    let mut old = lock.clone();
    for file in old["projection"]["files"]
        .as_object_mut()
        .unwrap()
        .values_mut()
    {
        let object = file.as_object_mut().unwrap();
        if let Some(text) = object
            .remove("text")
            .and_then(|text| text.as_str().map(str::to_string))
        {
            object.insert(
                "bytes".to_string(),
                serde_json::Value::Array(
                    text.bytes()
                        .map(|byte| serde_json::Value::Number(byte.into()))
                        .collect(),
                ),
            );
        }
    }
    old["schema"] = serde_json::Value::String("jails.compiler-lock.v3".to_string());
    fs::write(&lock_path, serde_json::to_vec_pretty(&old).unwrap()).unwrap();

    // A mutation over that project reads the old lock, merges against it, and
    // writes the new shape back.
    let evolved = jails_cmd(&root, None)
        .args(["entity", "field", "add", "Note", "body:string?"])
        .output()
        .unwrap();
    assert!(
        evolved.status.success(),
        "a lock in the previous release's shape must still be readable:\n{}{}",
        String::from_utf8_lossy(&evolved.stdout),
        String::from_utf8_lossy(&evolved.stderr)
    );
    let rewritten: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&lock_path).unwrap()).unwrap();
    assert_eq!(rewritten["schema"], "jails.compiler-lock.v4");
    assert!(
        rewritten["projection"]["files"]
            .as_object()
            .unwrap()
            .values()
            .all(|file| file.get("text").is_some() || file.get("bytes").is_some()),
        "every file carries one of the two spellings"
    );
}

/// Every mutation prints the JDL it wrote, above the files that JDL implies.
///
/// The CLI is sugar over one editable source, and this is where a reader
/// learns the language from the tool: the next edit can be made by hand in
/// the file. `--pretend` prints the same lines without writing them.
#[test]
fn a_mutation_prints_the_declaration_it_wrote_above_the_files_it_implied() {
    let root = model_project("model-jdl-hunk", EMPTY_MODEL);

    let record = jails_cmd(&root, None)
        .args(["g", "record", "Money", "amount:long"])
        .output()
        .unwrap();
    assert!(
        record.status.success(),
        "{}",
        String::from_utf8_lossy(&record.stderr)
    );
    let stdout = String::from_utf8_lossy(&record.stdout).to_string();
    let hunk: Vec<&str> = stdout
        .lines()
        .skip_while(|line| !line.starts_with("applied"))
        .skip(1)
        .take_while(|line| !line.trim_start().starts_with("create "))
        .collect();
    assert_eq!(
        hunk,
        vec!["  entity Money {", "    amount: long", "  }"],
        "the declaration is printed above the file list:\n{stdout}"
    );

    // One line for a setting, and the same lines under `--pretend`.
    let setting = jails_cmd(&root, None)
        .args(["--pretend", "set", "server.port=9090"])
        .output()
        .unwrap();
    assert!(
        setting.status.success(),
        "{}",
        String::from_utf8_lossy(&setting.stderr)
    );
    let previewed = String::from_utf8_lossy(&setting.stdout).to_string();
    assert!(
        previewed.contains("  prop server.port = \"9090\""),
        "{previewed}"
    );
    assert!(previewed.contains("nothing was written."), "{previewed}");
    assert!(
        !fs::read_to_string(root.join(".jails/model.jdl"))
            .unwrap()
            .contains("server.port"),
        "--pretend wrote the declaration it printed"
    );
}

#[test]
fn every_line_of_a_mutation_report_is_a_file_git_sees_change() {
    let root = model_project("report-is-the-change", EMPTY_MODEL);
    write_spring_fixture(&root);
    let repository = std::process::Command::new("git")
        .args(["init", "--quiet"])
        .current_dir(&root)
        .status();
    if !repository.is_ok_and(|status| status.success()) {
        skip("git not found on PATH");
        return;
    }

    let commit = |message: &str| {
        for arguments in [vec!["add", "-A"], vec!["commit", "--quiet", "-m", message]] {
            let done = std::process::Command::new("git")
                .args(&arguments)
                .env("GIT_AUTHOR_NAME", "jails")
                .env("GIT_AUTHOR_EMAIL", "jails@example.com")
                .env("GIT_COMMITTER_NAME", "jails")
                .env("GIT_COMMITTER_EMAIL", "jails@example.com")
                .current_dir(&root)
                .status()
                .unwrap();
            assert!(done.success(), "`git {}` failed", arguments.join(" "));
        }
    };
    commit("the fixture");

    // One of each shape a mutation takes: files created beside a reader file
    // patched, files created beside a tree left alone, files rewritten with
    // nothing created, a capability, and a deletion.
    for mutation in [
        vec!["g", "scaffold", "Note", "id:uuid@pk", "title:string!"],
        vec!["g", "scaffold", "Task", "id:uuid@pk", "name:string!"],
        vec!["entity", "field", "add", "Task", "priority:int"],
        vec!["add", "json"],
        vec!["g", "record", "Money", "amount:long"],
        vec!["destroy", "record", "Money", "--force"],
    ] {
        let output = jails_cmd(&root, None).args(&mutation).output().unwrap();
        assert!(
            output.status.success(),
            "`jails {}`: {}",
            mutation.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
        let report = String::from_utf8_lossy(&output.stdout).to_string();
        let reported: std::collections::BTreeSet<String> = report
            .lines()
            .filter_map(|line| line.strip_prefix("  "))
            .filter_map(|line| line.split_once(char::is_whitespace))
            .filter(|(verb, _)| ["create", "write", "patch", "delete", "append"].contains(verb))
            .map(|(_, path)| path.trim().to_string())
            .collect();

        let status = std::process::Command::new("git")
            .args(["status", "--porcelain", "-uall"])
            .current_dir(&root)
            .output()
            .unwrap();
        let changed: std::collections::BTreeSet<String> = String::from_utf8_lossy(&status.stdout)
            .lines()
            .map(|line| line[3..].trim().to_string())
            // The executor's own state, not project files: the advisory lock
            // and the ignore file naming it are written outside the plan --
            // after the transaction, so a refused plan writes neither -- and a
            // `jails new` project ignores both. This fixture is a bare Maven
            // tree with no `.gitignore` of its own, so git still reports them.
            .filter(|path| path != ".jails/apply.lock" && path != ".jails/.gitignore")
            .collect();

        assert_eq!(
            reported,
            changed,
            "`jails {}` reported a change git does not see, or missed one:\n{report}",
            mutation.join(" ")
        );
        commit(&mutation.join(" "));
    }
}
