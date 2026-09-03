//! `.jails/model.jdl` as the one editable source: `model fmt`, `model check`,
//! `model init`, `model explain`, `sync`, and the refusals that keep a second
//! editable source out.
//!
use super::*;

/// JDL v1 §18.4: convention must not mean hidden behaviour.
///
/// A generated project is full of names nobody typed, and `model explain` is
/// how a reader learns which rule produced one. This drives the command end
/// to end and asserts the three things it has to get right.
///
/// The reader's layer rename has to be applied, which is why the project
/// carries a `jails.toml`. The records live in the model and a linked model
/// carries the default packages -- the layout arrives with the workspace --
/// so a version of this command that reported on the parsed model would print
/// `com.example.notes.domain` to a project that has no such package.
///
/// And the §9.7 divergence has to be visible. Six of the twenty-three emitted
/// packages sit under a head §9.7 does not close, so a layer rename does not
/// reach them. Their rule says `convention.facet.*` where a layer's says
/// `convention.layer.*`: the same tree, two rules, and the difference is
/// something a reader can be shown.
#[test]
fn model_explain_shows_which_rule_produced_each_derived_name() {
    let root = jdl_project(
        "model-explain-derived",
        "jdl 1\napp Notes @id(project_notes) {\n  pkg com.example.notes\n  java 26\n  \
         platform plain\n  build maven\n  storage none\n}\n\n\
         entity SupportPerson @id(ent_person) {\n  use repo\n  \
         id: uuid @id(fld_person_id) @pk\n  \
         familyName: string @id(fld_person_family)\n}\n",
    );
    write_plain_fixture(&root);
    fs::write(root.join("jails.toml"), "[layout]\ndomain = \"core\"\n").unwrap();

    let explained = jails_cmd(&root, None)
        .args(["model", "explain"])
        .output()
        .unwrap();
    assert!(
        explained.status.success(),
        "{}",
        String::from_utf8_lossy(&explained.stderr)
    );
    let told = String::from_utf8_lossy(&explained.stdout);
    for row in [
        // The rename reached the layer...
        "com.example.notes.core  [convention.layer.domain]",
        // ...and did not reach a head §9.7 does not close.
        "com.example.notes.repository  [convention.facet.repository]",
        // The pluralizer, on §9.7's own irregular case.
        "support_people  [convention.sql-table.pluralize]",
        "family_name  [convention.sql-column.snake-case]",
    ] {
        assert!(told.contains(row), "missing `{row}`:\n{told}");
    }

    // **A filter narrows to one boundary, and a field's column counts as
    // inside it.** The record for `family_name` names `ent_person` in its
    // inputs, so asking about the entity answers with the entity's own names
    // *and* what they were derived for -- which is the question somebody
    // typing an entity id is asking. What it must not do is answer with the
    // project: none of the twenty-three package rows belongs to this entity.
    let filtered = jails_cmd(&root, None)
        .args(["model", "explain", "ent_person"])
        .output()
        .unwrap();
    assert!(filtered.status.success());
    let filtered = String::from_utf8_lossy(&filtered.stdout);
    assert!(filtered.contains("support_people"), "{filtered}");
    assert!(filtered.contains("family_name"), "{filtered}");
    assert!(!filtered.contains("java-package"), "{filtered}");

    let missed = jails_cmd(&root, None)
        .args(["model", "explain", "no-such-boundary"])
        .output()
        .unwrap();
    assert!(missed.status.success());
    assert!(
        String::from_utf8_lossy(&missed.stdout).contains("no derived value matches"),
        "a miss is an answer, not a failure"
    );

    fs::remove_dir_all(root).ok();
}

#[test]
fn model_fmt_checks_then_atomically_formats_only_the_jdl_source() {
    let root = jdl_project(
        "jdl-v1-format",
        "jdl 1\r\n\r\napp Notes {\r\n\tpkg com.example.notes  \r\n java 26\r\n platform spring\r\n build maven\r\n storage postgres\r\n}\r\n\r\n// reader comment\r\nentity Task {\r\n\tid: uuid @pk  \r\n}\r\n",
    );
    write_spring_fixture(&root);
    let model_path = root.join(".jails/model.jdl");
    let before = fs::read(&model_path).unwrap();

    let checked = jails_cmd(&root, None)
        .args(["model", "fmt", "--check"])
        .output()
        .unwrap();
    assert!(!checked.status.success());
    assert!(
        String::from_utf8_lossy(&checked.stderr).contains("run `jails model fmt`"),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );
    assert_eq!(fs::read(&model_path).unwrap(), before, "check wrote source");

    let previewed = jails_cmd(&root, None)
        .args(["model", "fmt", "--pretend", "--diff"])
        .output()
        .unwrap();
    assert!(
        previewed.status.success(),
        "{}",
        String::from_utf8_lossy(&previewed.stderr)
    );
    assert_eq!(
        fs::read(&model_path).unwrap(),
        before,
        "preview wrote source"
    );

    let formatted = jails_cmd(&root, None)
        .args(["model", "fmt"])
        .output()
        .unwrap();
    assert!(
        formatted.status.success(),
        "{}",
        String::from_utf8_lossy(&formatted.stderr)
    );
    let after = fs::read_to_string(&model_path).unwrap();
    assert!(!after.contains('\r'), "{after:?}");
    assert!(!after.contains('\t'), "{after:?}");
    assert!(!after.lines().any(|line| line.ends_with(' ')), "{after:?}");
    assert!(after.contains("// reader comment\n"), "{after}");
    assert!(after.ends_with('\n'));

    let checked = jails_cmd(&root, None)
        .args(["model", "fmt", "--check"])
        .output()
        .unwrap();
    assert!(
        checked.status.success(),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );
}

#[test]
fn model_fmt_keeps_typed_field_semantics_and_refuses_invalid_rules_atomically() {
    let root = jdl_project(
        "jdl-v1-field-semantics",
        r#"jdl 1
app Notes {
 pkg com.example.notes
 java 26
 platform spring
 build maven
 storage postgres
}
entity Task {
 updatedAt: instant @updated @default(now())
 version: long @nonnegative @version
 tenantId: uuid @map("tenant_id") @scope(claim: "tenant") @id(fld_task_tenant)
 id: uuid @pk
}
"#,
    );
    write_spring_fixture(&root);
    let model_path = root.join(".jails/model.jdl");

    let formatted = jails_cmd(&root, None)
        .args(["model", "fmt"])
        .output()
        .unwrap();
    assert!(
        formatted.status.success(),
        "{}",
        String::from_utf8_lossy(&formatted.stderr)
    );
    let source = fs::read_to_string(&model_path).unwrap();
    assert!(source.contains("updatedAt: instant @default(now()) @updated"));
    assert!(source.contains("version: long @version @nonnegative"));
    assert!(source.contains(
        "tenantId: uuid @id(fld_task_tenant) @scope(claim: \"tenant\") @map(\"tenant_id\")"
    ));

    let model = jails_model::parse_jdl(&source).unwrap();
    let task = model
        .entities
        .values()
        .find(|entity| entity.label == "task")
        .unwrap();
    let version = task
        .fields
        .iter()
        .find(|field| field.label == "version")
        .unwrap();
    assert!(version.semantics.version);
    assert!(version.semantics.default.as_ref().unwrap().derived);
    let tenant = task
        .fields
        .iter()
        .find(|field| field.label == "tenant_id")
        .unwrap();
    let scope = tenant.semantics.scope.as_ref().unwrap();
    assert_eq!(scope.claim, "tenant");
    assert!(scope.pinned);

    let invalid = source.replace("tenantId: uuid ", "tenantId: uuid? ");
    fs::write(&model_path, &invalid).unwrap();
    let before = fs::read(&model_path).unwrap();
    let refused = jails_cmd(&root, None)
        .args(["model", "fmt"])
        .output()
        .unwrap();
    assert!(!refused.status.success());
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("model-scope-required"),
        "{}",
        String::from_utf8_lossy(&refused.stderr)
    );
    assert_eq!(fs::read(&model_path).unwrap(), before);
}

/// A project can run its own formatter.
///
/// Managed output is merge-managed, so the formatter's pass over it is an
/// ordinary reader edit the next generation keeps.
#[test]
fn a_canonical_project_runs_its_own_formatter() {
    if !real_mvn_available() {
        skip("mvn not found on PATH");
        return;
    }
    let root = jdl_project(
        "jdl-v1-canonical-fmt",
        r#"jdl 1
app Demo {
 pkg com.example.demo
 java 26
 platform plain
 build maven
 storage none
}
"#,
    );
    write_plain_fixture(&root);
    let installed = jails_cmd(&root, None)
        .args(["add", "format"])
        .output()
        .unwrap();
    assert!(
        installed.status.success(),
        "{}",
        String::from_utf8_lossy(&installed.stderr)
    );

    let formatted = jails_cmd(&root, None).arg("fmt").output().unwrap();
    let told = String::from_utf8_lossy(&formatted.stderr);
    assert!(
        !told.contains("does not route `fmt`"),
        "a canonical project still refuses its own formatter: {told}"
    );
    assert!(formatted.status.success(), "{told}");
}

/// `model init`: the on-ramp for a repository jails did not create. `new`
/// seeds a model, so a project jails creates has one from its first command;
/// somebody else's repository has none.
///
/// What it writes is the app block and nothing else. The reader's Java is not
/// adopted, moved or rewritten; what changes is that the next `jails g`
/// renders through the compiler, and the lock says which files are jails'.
#[test]
fn model_init_makes_a_foreign_project_canonical_without_touching_its_sources() {
    let root = temp_dir("model-init-foreign");
    write_plain_fixture(&root);
    let reader = root.join("src/main/java/com/example/demo/Existing.java");
    fs::create_dir_all(reader.parent().unwrap()).unwrap();
    let untouched = "package com.example.demo;\n\npublic class Existing {\n}\n";
    fs::write(&reader, untouched).unwrap();

    let created = jails_cmd(&root, None)
        .args(["model", "init"])
        .output()
        .unwrap();
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );

    // Every field is read off the project rather than asked for, because each
    // is a fact the project already states.
    let model = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(model.starts_with("jdl 1\n"), "{model}");
    assert!(model.contains("pkg com.example.demo"), "{model}");
    assert!(model.contains("build maven"), "{model}");
    // `storage none` is the one judgement, and it is the same one `new` makes:
    // jails has installed no database here, so the model claims none.
    assert!(model.contains("storage none"), "{model}");

    // From here the generator renders into the managed tree.
    let generated = jails_cmd(&root, None)
        .args(["g", "record", "Note", "title:string!"])
        .output()
        .unwrap();
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );
    assert!(
        root.join("src/main/java/com/example/demo/domain/Note.java")
            .is_file()
    );
    assert!(
        !root.join(".jails/ledger.toml").exists(),
        "a canonical project created a legacy ledger"
    );

    // And the reader's own file is exactly as they left it.
    assert_eq!(fs::read_to_string(&reader).unwrap(), untouched);
}

/// One editable source.
#[test]
fn model_init_refuses_a_project_that_already_has_a_model_or_a_ledger() {
    let modelled = jdl_project(
        "model-init-twice",
        r#"jdl 1
app Demo {
 pkg com.example.demo
 java 26
 platform plain
 build maven
 storage none
}
"#,
    );
    write_plain_fixture(&modelled);
    let refused = jails_cmd(&modelled, None)
        .args(["model", "init"])
        .output()
        .unwrap();
    assert!(!refused.status.success());
    let told = String::from_utf8_lossy(&refused.stderr);
    assert!(told.contains("already has an application model"), "{told}");

    // A `.jails/ledger.toml` is refused by name rather than treated as an
    // absence: nothing here can read what it holds, and seeding a model
    // beside declarations this binary cannot see would strand a project's
    // whole contents outside the model that owns it.
    let legacy = temp_dir("model-init-legacy-ledger");
    write_plain_fixture(&legacy);
    fs::create_dir_all(legacy.join(".jails")).unwrap();
    fs::write(
        legacy.join(".jails/ledger.toml"),
        "written by another jails\n",
    )
    .unwrap();
    let refused = jails_cmd(&legacy, None)
        .args(["model", "init"])
        .output()
        .unwrap();
    assert!(!refused.status.success());
    let told = String::from_utf8_lossy(&refused.stderr);
    assert!(told.contains("legacy ledger"), "{told}");
    assert!(told.contains(".jails/ledger.toml"), "{told}");
}

#[test]
fn familiar_mutations_write_valid_jdl_v1_through_one_cst_pipeline() {
    let root = jdl_project(
        "jdl-v1-familiar-mutations",
        r#"jdl 1
app Notes {
  pkg com.example.notes
  java 26
  platform spring
  build maven
  storage postgres
}
"#,
    );
    write_spring_fixture(&root);
    fs::write(
        root.join("acceptance.md"),
        "# Acceptance\n\n- creates a task\n- refuses an empty title\n",
    )
    .unwrap();
    let commands = [
        vec!["g", "scaffold", "Task", "id:uuid@pk", "title:string!"],
        vec!["g", "field", "Task", "done:boolean?"],
        vec!["g", "factory", "Task"],
        vec!["g", "dto", "Task"],
        vec!["g", "class", "Clock"],
        vec!["g", "interface", "TaskPort"],
        vec!["g", "service", "Billing"],
        vec!["g", "sealed", "Outcome", "Success", "Failed"],
        vec![
            "g", "strategy", "Policy", "Fast", "Safe", "--on", "Task", "--yields", "Task",
        ],
        // `--method post`, because `--on` declares a request body. The default
        // is GET, which carries none, and the component linker refuses that
        // pair by name.
        vec![
            "g",
            "controller",
            "TaskApi",
            "--method",
            "post",
            "--on",
            "Task",
            "--yields",
            "Task",
            "--path",
            "/task-api",
        ],
        vec!["g", "test", "Smoke"],
        vec!["g", "integration-test", "Database"],
        vec!["add", "fake"],
        vec![
            "add",
            "dependency",
            "org.jsoup:jsoup",
            "--version",
            "1.18.3",
            "--scope",
            "test",
        ],
        vec!["set", "server.port=8080"],
        vec!["g", "event", "TaskCreated", "id", "title", "--on", "Task"],
        // A *second* event, and the difference between the two is the point:
        // `id:uuid` is a component the row does not carry, minted when the
        // event is staged. `TaskCreated` projects the row's id and is
        // published directly -- an event cannot be both, because a staged
        // event keyed on the resource id deduplicates the wrong thing.
        vec![
            "g",
            "event",
            "TaskStaged",
            "id:uuid",
            "title",
            "--on",
            "Task",
        ],
        // The outbox store encodes the staged payload with `Json`.
        vec!["add", "json"],
        vec![
            "g",
            "usecase",
            "CreateTask",
            "title",
            "--on",
            "Task",
            "--path",
            "/tasks",
        ],
        vec![
            "g",
            "query",
            "OpenTasks",
            "title",
            "--on",
            "Task",
            "--limit",
            "50",
            "--path",
            "/tasks/search",
        ],
        vec![
            "g",
            "transition",
            "RenameTask",
            "title",
            "--on",
            "Task",
            "--yields",
            "TaskCreated",
            "--path",
            "/tasks/{id}",
            "--method",
            "patch",
        ],
        vec!["g", "handler", "Health", "--path", "/healthz"],
        vec!["g", "cli", "Admin"],
        vec!["g", "command", "Refresh", "--on", "AdminCli"],
        vec!["g", "cases", "acceptance.md"],
        vec![
            "g",
            "client",
            "Audit",
            "requestId:uuid",
            "--on",
            "Task",
            "--yields",
            "Task",
            "--path",
            "/v1/audit",
            "--method",
            "post",
        ],
        vec![
            "g",
            "usecase",
            "StageTask",
            "title",
            "--on",
            "Task",
            "--yields",
            "TaskStaged",
        ],
        vec!["g", "fetcher", "Remote"],
        vec!["g", "job", "Sweep"],
        vec!["g", "http-workflow", "Crawl", "--on", "RemoteFetcher"],
        vec![
            "g",
            "http-sink",
            "Delivery",
            "--on",
            "StageTask",
            "--yields",
            "TaskStaged",
        ],
        vec!["g", "idempotency", "Request"],
        // The encoder, the decoder and the filter chain that reads the token
        // are one story, so `g auth` refuses without the capability that
        // carries the other two.
        vec!["add", "security"],
        vec!["g", "auth", "Api"],
        vec![
            "g",
            "webhook",
            "Stripe",
            "signature:string!",
            "--path",
            "/hooks/stripe",
            "--method",
            "post",
            "--consumes",
            "form",
            "--bind",
            "signature=stripe_signature",
        ],
        // `--on` is the command it runs later and `--yields` the entity that
        // command creates -- the row whose presence tells a retry from a
        // repeat. The payload is the command's own `Input`, so there are no
        // fields to repeat here.
        vec![
            "g",
            "durable-job",
            "Dispatch",
            "--on",
            "CreateTask",
            "--yields",
            "Task",
        ],
        vec![
            "g", "socket", "Chat", "--path", "/ws/chat", "--method", "get",
        ],
        vec!["g", "presence", "Online"],
    ];
    // Empty, and the list stays: a kind added without a backend belongs here
    // rather than in the success path, because the loop above must not accept
    // a declaration that reports success and emits nothing.
    const UNSERVED: &[&str] = &[];
    for command in commands {
        let output = jails_cmd(&root, None).args(&command).output().unwrap();
        if UNSERVED.contains(&command[1]) {
            let before = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
            assert!(
                !output.status.success(),
                "`jails {}` has no compiler backend and must refuse",
                command.join(" ")
            );
            // The exact wording is pinned by
            // `every_component_kind_is_emitted_or_refused` in `jails-compiler`,
            // which reaches every unserved kind directly. Here the refusal can
            // also arrive second-hand -- `g command --on AdminCli` cannot
            // resolve a component `g cli Admin` was refused permission to
            // declare -- and what this test owns is the property that survives
            // either route: the command fails and the source is untouched.
            assert_eq!(
                before,
                fs::read_to_string(root.join(".jails/model.jdl")).unwrap(),
                "`jails {}` refused and still edited the source",
                command.join(" ")
            );
            continue;
        }
        assert!(
            output.status.success(),
            "`jails {}` failed:\n{}",
            command.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
        let source = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
        jails_model::parse_jdl_cst(&source).unwrap_or_else(|diagnostics| {
            panic!(
                "`jails {}` wrote invalid CST:\n{diagnostics}",
                command.join(" ")
            )
        });
        jails_model::parse_jdl(&source).unwrap_or_else(|diagnostics| {
            panic!(
                "`jails {}` wrote invalid semantics:\n{diagnostics}",
                command.join(" ")
            )
        });
    }

    let source = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    for declaration in [
        "use scaffold",
        "use factory",
        // No `use dto`: the scaffold profile carries `Facet::Dto`, so `g dto`
        // on a scaffolded entity declares nothing it does not already have.
        // Recording it again would give one facet two spellings in one
        // entity, and the second is the one nothing removes.
        "component class Clock @id(cmp_class_clock)",
        "component interface TaskPort @id(cmp_interface_task_port)",
        "component service Billing @id(cmp_service_billing)",
        "component sealed Outcome @id(cmp_sealed_outcome)",
        "variant Success @id(var_cmp_sealed_outcome_success)",
        "component strategy Policy @id(cmp_strategy_policy)",
        "component controller TaskApi @id(cmp_controller_task_api)",
        "component test Smoke @id(cmp_test_smoke)",
        "component integration-test Database @id(cmp_integration_test_database)",
        "done: boolean? @id(fld_task_done)",
        "cap fake @id(cap_fake)",
        "dep org.jsoup:jsoup @id(dep_",
        "@version(\"1.18.3\") @scope(test)",
        "prop server.port = \"8080\" @id(set_",
        "event TaskCreated(id, title) @id(op_task_created)",
        "event TaskStaged(id: uuid, title) @id(op_task_staged)",
        "deliver outbox",
        "route POST \"/tasks\"",
        "limit 50",
        "emit task_created",
        "route PATCH \"/tasks/{id}\"",
        "component cases Acceptance @id(cmp_cases_acceptance)",
        "source \"acceptance.md\"",
        // A kind with no compiler backend never reaches the source: a mutation
        // compiles before it writes, so refusing to emit is refusing to
        // record.
    ] {
        assert!(
            source.contains(declaration),
            "missing `{declaration}`:\n{source}"
        );
    }

    for command in [
        vec!["destroy", "factory", "Task", "--force"],
        vec!["destroy", "class", "Clock", "--force"],
        vec!["destroy", "sealed", "Outcome", "--force"],
        vec!["destroy", "cases", "acceptance.md", "--force"],
        vec!["remove", "fake", "--force"],
        vec!["remove", "dependency", "org.jsoup:jsoup"],
        vec!["unset", "server.port"],
    ] {
        let output = jails_cmd(&root, None).args(&command).output().unwrap();
        assert!(
            output.status.success(),
            "`jails {}` failed:\n{}",
            command.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let source = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    jails_model::parse_jdl(&source).unwrap();
    assert!(!source.contains("use factory"), "{source}");
    assert!(!source.contains("component class Clock"), "{source}");
    assert!(!source.contains("component sealed Outcome"), "{source}");
    assert!(!source.contains("component cases Acceptance"), "{source}");
    assert!(!source.contains("cap fake"), "{source}");
    assert!(!source.contains("dep org.jsoup:jsoup"), "{source}");
    assert!(!source.contains("prop server.port"), "{source}");
}

#[test]
fn model_check_links_the_real_model_through_the_binary() {
    let root = model_project("model-check", MODEL);
    let before = snapshot_tree(&root);
    let output = jails_cmd(&root, None)
        .args(["model", "check"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("model valid: .jails/model.jdl"), "{stdout}");
    assert!(
        stdout.contains("8 nodes, 1 entities, 1 operations"),
        "{stdout}"
    );
    assert_eq!(
        snapshot_tree(&root),
        before,
        "model check must be read-only"
    );
}

#[test]
fn model_check_json_exposes_the_resolved_stable_id_world() {
    let root = model_project("model-json", MODEL);
    let output = jails_cmd(&root, None)
        .args(["model", "check", "--output", "json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["schema"], "jails.model-check.v1");
    assert_eq!(report["valid"], true);
    assert_eq!(report["model"]["entities"]["ent_note"]["label"], "note");
    assert_eq!(
        report["model"]["operations"]["op_create_note"]["kind"]["Command"]["on"],
        "ent_note"
    );
}

#[test]
fn model_check_reports_all_semantic_failures_as_one_json_document() {
    // A field the entity does not declare, and an operation whose id is the
    // entity's: one unresolved reference and one collision, so the report has
    // to carry both rather than stopping at the first.
    let invalid = MODEL
        .replace("CreateNote(title)", "CreateNote(missing)")
        .replace("@id(op_create_note)", "@id(ent_note)");
    let root = model_project("model-invalid", &invalid);
    let output = jails_cmd(&root, None)
        .args(["--output", "json", "model", "check"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stderr.is_empty(), "failure was reported twice");
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["valid"], false);
    let diagnostics = report["diagnostics"].as_array().unwrap();
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic["code"] == "model-id-collision")
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic["code"] == "model-field-reference")
    );
}

#[test]
fn model_check_names_the_fix_when_the_default_model_is_missing() {
    let root = temp_dir("model-missing");
    let output = jails_cmd(&root, None)
        .args(["model", "check"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains(".jails/model.jdl"), "{stderr}");
    assert!(stderr.contains("--manifest <path>"), "{stderr}");
}

#[test]
fn jdl_is_compiled_directly_as_the_single_authoring_source() {
    let root = jdl_project(
        "model-jdl",
        r#"jdl 1

app Notes @id(project_notes) {
  pkg com.example.notes
  java 26
  platform spring
  build maven
  storage postgres
}

entity Task @id(ent_task) {
  title: string @id(fld_task_title) @notBlank
  done: boolean?
}

enum Status @id(ent_status) {
  OPEN
  CLOSED
}
"#,
    );
    let checked = jails_cmd(&root, None)
        .args(["model", "check"])
        .output()
        .unwrap();
    assert!(
        checked.status.success(),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );
    assert!(
        String::from_utf8_lossy(&checked.stdout).contains(".jails/model.jdl"),
        "{}",
        String::from_utf8_lossy(&checked.stdout)
    );

    let synced = jails_cmd(&root, None).args(["sync"]).output().unwrap();
    assert!(
        synced.status.success(),
        "{}",
        String::from_utf8_lossy(&synced.stderr)
    );
    let task =
        fs::read_to_string(root.join("src/main/java/com/example/notes/domain/Task.java")).unwrap();
    assert!(task.contains("String title"), "{task}");
    assert!(task.contains("Optional<Boolean> done"), "{task}");
    assert!(
        root.join("src/main/java/com/example/notes/domain/Status.java")
            .is_file()
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
fn canonical_sync_recompiles_model_state_without_the_legacy_store() {
    let root = model_project("model-sync", MODEL);
    apply_canonical_model(&root, "initial-sync");
    let record = root.join("src/main/java/com/example/notes/domain/Note.java");
    let contents = fs::read_to_string(&record).unwrap();
    let split = contents.rfind("\n}").unwrap();
    fs::write(
        &record,
        format!(
            "{}\n\n    public String handWritten() {{ return \"reader\"; }}{}",
            &contents[..split],
            &contents[split..]
        ),
    )
    .unwrap();
    let model_path = root.join(".jails/model.jdl");
    let mut source = fs::read_to_string(&model_path).unwrap();
    source.push_str("\ncap fake @id(cap_fake)\n");
    fs::write(&model_path, source).unwrap();

    let synced = jails_cmd(&root, None).arg("sync").output().unwrap();
    assert!(
        synced.status.success(),
        "{}",
        String::from_utf8_lossy(&synced.stderr)
    );
    assert!(fs::read_to_string(&record).unwrap().contains("handWritten"));
    assert!(
        root.join("src/main/java/com/example/notes/adapters/memory/InMemoryNoteRepository.java")
            .is_file()
    );
    for legacy in ["objects", "receipts", "journal", "state"] {
        assert!(
            !root.join(".jails").join(legacy).exists(),
            "canonical sync created legacy state `{legacy}`"
        );
    }
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
fn canonical_projects_fail_closed_before_legacy_mutation_routes() {
    let root = model_project("model-no-legacy-routes", MODEL);
    apply_canonical_model(&root, "initial-no-legacy-routes");
    for case in [
        // The textual rename is not routed anywhere -- it is a different
        // operation, and it works. What it refuses is a type the *model*
        // declares, because carrying only the Java would leave the next
        // compilation rendering the old name back.
        (
            "rename Note Memo",
            vec!["rename", "Note", "Memo"],
            "declared in this project's application model",
        ),
        // `fmt`, `app plan` and `app apply` all work on a modelled project:
        // `a_canonical_project_runs_its_own_formatter` and
        // `a_manifest_replays_into_the_model_and_converges` hold them. `init`
        // refuses: it *writes* the manifest, which beside a model is a second
        // editable source.
        ("app init", vec!["app", "init"], "one editable source"),
    ] {
        // `adopt` and `modernize` are not refused: both run *before* a project
        // has a model and neither claims anything a later command reconciles,
        // so they write directly.
        let (label, arguments, told) = case;
        let before = snapshot_tree(&root);
        let output = jails_cmd(&root, None).args(&arguments).output().unwrap();
        assert!(!output.status.success(), "{label} unexpectedly passed");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains(told), "{label}: {stderr}");
        assert_eq!(snapshot_tree(&root), before, "{label} wrote files");
    }
}

/// A command run from a subdirectory is about the same project.
///
/// The dispatch switch and the build-file walk are one walk: `jails g record`
/// typed in `src/main/java` renders through the compiler and writes no
/// `.jails/ledger.toml`.
#[test]
fn a_canonical_command_run_from_a_subdirectory_is_still_canonical() {
    let root = temp_dir("canonical-subdirectory");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(
        root.join(".jails/model.jdl"),
        "jdl 1\napp Demo @id(project_demo) {\n  pkg com.example.demo\n  java 26\n  \
         platform spring\n  build maven\n  storage none\n}\n",
    )
    .unwrap();
    let inside = common::generated(&root, "src/main/java/com/example/demo");

    let generated = jails_cmd(&inside, None)
        .args(["g", "record", "Sub", "name:String"])
        .output()
        .unwrap();
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );
    assert!(
        root.join("src/main/java/com/example/demo/domain/Sub.java")
            .exists(),
        "the record was not written to the managed tree"
    );
    assert!(
        !root.join(".jails/ledger.toml").exists(),
        "a canonical project was given a legacy ledger"
    );
    assert!(
        !inside.join("Sub.java").exists(),
        "the record was written into the reader's own tree"
    );

    // The read-only commands answer about the same project, and name the
    // model project-relative rather than by whatever absolute path this
    // directory happens to have.
    let checked = jails_cmd(&inside, None)
        .args(["model", "check"])
        .output()
        .unwrap();
    assert!(
        checked.status.success(),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );
    assert!(
        String::from_utf8(checked.stdout)
            .unwrap()
            .contains("model valid: .jails/model.jdl"),
        "`model check` did not report the project's own model"
    );
    let synced = jails_cmd(&inside, None).arg("sync").output().unwrap();
    assert!(
        synced.status.success(),
        "{}",
        String::from_utf8_lossy(&synced.stderr)
    );
}

/// `jails adopt` and `jails model init` are each other's obvious next step in
/// a foreign repository, so `adopt` must leave nothing `model init` refuses
/// on: a layout row is configuration, not a transition.
#[test]
fn a_project_that_only_recorded_its_layout_can_still_become_canonical() {
    let root = temp_dir("adopt-then-model-init");
    write_plain_fixture(&root);
    // A directory name jails' synonym table maps onto a layer, so `adopt` has
    // something to record.
    let renamed = common::generated(&root, "src/main/java/com/example/demo/persistence");
    fs::create_dir_all(&renamed).unwrap();
    fs::write(
        renamed.join("Repo.java"),
        "package com.example.demo.persistence;\npublic class Repo {}\n",
    )
    .unwrap();

    let adopted = jails_cmd(&root, None).arg("adopt").output().unwrap();
    assert!(
        adopted.status.success(),
        "{}",
        String::from_utf8_lossy(&adopted.stderr)
    );
    // `adopt` writes the layout row and nothing else -- it is configuration,
    // not a transition, so it leaves no machine state of jails' own for
    // `model init` to have an opinion about.
    assert!(!root.join(".jails/ledger.toml").exists(), "{adopted:?}");
    let layout = fs::read_to_string(root.join("jails.toml")).unwrap();
    assert!(
        layout.contains("adapters = \"persistence\""),
        "the layout row is what adopt is for:\n{layout}"
    );

    let initialised = jails_cmd(&root, None)
        .args(["model", "init"])
        .output()
        .unwrap();
    assert!(
        initialised.status.success(),
        "an adopted layout must not block the on-ramp:\n{}",
        String::from_utf8_lossy(&initialised.stderr)
    );

    // And the layout the reader adopted is the one the compiler projects with.
    let explained = jails_cmd(&root, None)
        .args(["model", "explain", "java-package"])
        .output()
        .unwrap();
    let explained = String::from_utf8_lossy(&explained.stdout).to_string();
    assert!(
        explained.contains("com.example.demo.persistence"),
        "the adopted layer name should reach the compiler's projection:\n{explained}"
    );
}

/// `.jails/app.toml` is an import format, not a second editable source.
///
/// A `[[generate]]` row is a `GenerateArgs` -- the same value `jails g`
/// parses -- so every row goes through the frontend that already knows how to
/// declare it, and the manifest's own syntax is the only thing its parser
/// knows that the CLI does not.
///
/// Row by row rather than one transition, which costs atomicity and buys
/// convergence: each frontend is idempotent, so an interrupted replay
/// converges by being run again.
#[test]
fn a_manifest_replays_into_the_model_and_converges() {
    let root = jdl_project(
        "jdl-v1-app-replay",
        r#"jdl 1
app Books {
 pkg com.example.books
 java 26
 platform plain
 build maven
 storage none
}
"#,
    );
    write_plain_fixture(&root);
    fs::write(
        root.join(".jails/app.toml"),
        r#"schema = 1
capabilities = ["json"]

[[generate]]
kind = "record"
name = "Loan"
fields = ["id:uuid", "days:int"]

[[generate]]
kind = "enum"
name = "Shelf"
fields = ["OPEN", "CLOSED"]
"#,
    )
    .unwrap();

    let applied = jails_cmd(&root, None)
        .args(["app", "apply"])
        .output()
        .unwrap();
    assert!(
        applied.status.success(),
        "{}",
        String::from_utf8_lossy(&applied.stderr)
    );
    let model = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    for declaration in ["cap json", "entity Loan", "enum Shelf"] {
        assert!(
            model.contains(declaration),
            "the manifest's rows should reach the model, missing `{declaration}`:\n{model}"
        );
    }
    assert!(
        root.join("src/main/java/com/example/books/domain/Loan.java")
            .is_file(),
        "and be compiled into the managed tree"
    );
    assert!(
        !root.join(".jails/ledger.toml").exists(),
        "a canonical replay must not create a legacy ledger"
    );

    // Convergent: every row is already declared, so nothing is written.
    let again = jails_cmd(&root, None)
        .args(["app", "apply"])
        .output()
        .unwrap();
    let again = String::from_utf8_lossy(&again.stdout).to_string();
    assert!(
        again
            .matches("nothing to do, the project already matches the model")
            .count()
            >= 3,
        "a second replay declares nothing new:\n{again}"
    );

    // `plan` is the same replay, pretending.
    let planned = jails_cmd(&root, None)
        .args(["app", "plan"])
        .output()
        .unwrap();
    assert!(
        planned.status.success(),
        "{}",
        String::from_utf8_lossy(&planned.stderr)
    );

    // `app init` refuses: it writes the manifest, and a manifest beside a
    // model is a second editable source.
    let refused = jails_cmd(&root, None)
        .args(["app", "init"])
        .output()
        .unwrap();
    assert!(
        !refused.status.success()
            && String::from_utf8_lossy(&refused.stderr).contains("one editable source"),
        "`app init` writes a second editable source and must still refuse"
    );
}

/// `resource index add` writes a declaration its own parser accepts: v1 reads
/// `index [ user_id, created_at desc ]` and allows only `@id` and `@map`, and
/// the renderer picks the form by dialect rather than by filename.
#[test]
fn an_index_on_a_v1_model_uses_the_grammar_that_model_is_written_in() {
    let root = jdl_project(
        "jdl-v1-index-grammar",
        r#"jdl 1
app Depot {
 pkg com.example.depot
 java 26
 platform spring
 build maven
 storage postgres
}
entity Crate {
 id: uuid @pk
 label: string
 slot: int
 use repo
}
"#,
    );
    write_spring_fixture(&root);
    let synced = jails_cmd(&root, None).arg("sync").output().unwrap();
    assert!(
        synced.status.success(),
        "{}",
        String::from_utf8_lossy(&synced.stderr)
    );

    let added = jails_cmd(&root, None)
        .args(["resource", "index", "add", "Crate", "label, slot desc"])
        .output()
        .unwrap();
    assert!(
        added.status.success(),
        "an index on a v1 model must use v1's own grammar:\n{}",
        String::from_utf8_lossy(&added.stderr)
    );
    let model = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(
        model.contains("index [label, slot desc]"),
        "the bracketed field list is the v1 form:\n{model}"
    );
    assert!(
        !model.contains("@as("),
        "and a v1 index takes its label from its columns, so `@as` is rejected:\n{model}"
    );

    // One forward migration, which is the half that makes an index a stable
    // entity child rather than a rendering.
    let migrations = fs::read_dir(root.join("src/main/resources/db/migration"))
        .unwrap()
        .flatten()
        .filter(|entry| entry.path().extension().is_some_and(|e| e == "sql"))
        .count();
    assert_eq!(migrations, 2, "create table, then add index");

    // And `g scaffold --index` reaches the same node rather than refusing:
    // the entity lands first, then the index, because the columns resolve
    // against model field identity.
    let scaffolded = jails_cmd(&root, None)
        .args([
            "g",
            "scaffold",
            "Bin",
            "id:uuid@pk",
            "code:string",
            "--index",
            "code",
        ])
        .output()
        .unwrap();
    assert!(
        scaffolded.status.success(),
        "`g scaffold --index` should apply the index rather than refuse:\n{}",
        String::from_utf8_lossy(&scaffolded.stderr)
    );
    let model = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(
        model.contains("index [code]"),
        "the scaffold's index should be in the model:\n{model}"
    );
}

/// `jails add db` on a JDL v1 project sets the storage axis, not a capability.
///
/// v1's closed capability registry has no `db`: `storage postgres` is what the
/// reader declares and the `db` capability is what the linker materializes
/// from it; appending `cap db` would write a model that does not parse.
#[test]
fn add_db_on_a_v1_model_sets_the_storage_axis() {
    let root = temp_dir("v1-add-db");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(
        root.join(".jails/model.jdl"),
        "jdl 1\n// a comment the edit must not disturb\napp Demo @id(project_demo) {\n  \
         pkg com.example.demo\n  java 26\n  platform spring\n  build maven\n  storage none\n}\n",
    )
    .unwrap();

    let added = jails_cmd(&root, None)
        .args(["add", "db", "--no-start"])
        .output()
        .unwrap();
    assert!(
        added.status.success(),
        "{}",
        String::from_utf8_lossy(&added.stderr)
    );

    let source = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(source.contains("storage postgres"), "{source}");
    assert!(!source.contains("storage none"), "{source}");
    assert!(!source.contains("cap db"), "{source}");
    // The edit is one line inside the `app` block; everything else survives.
    assert!(
        source.contains("// a comment the edit must not disturb"),
        "{source}"
    );
    assert!(source.contains("platform spring"), "{source}");

    // And it is the real capability: the JDBC adapter and the schema follow.
    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    assert!(pom.contains("spring-boot-starter-jdbc"), "{pom}");

    // `sqlite` is deliberately not routed to the axis: v1 carries `cap sqlite`
    // as well as `storage sqlite`, and the capability is the primary spelling.
    let sqlite = jails_cmd(&root, None)
        .args(["add", "sqlite"])
        .output()
        .unwrap();
    assert!(
        sqlite.status.success(),
        "{}",
        String::from_utf8_lossy(&sqlite.stderr)
    );
    let source = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(source.contains("cap sqlite"), "{source}");
    assert!(source.contains("storage postgres"), "{source}");
}

/// Adding a field leaves the model and the source it just wrote agreeing.
///
/// `AddField` is positional, and "already sorted by label" must not be read
/// as "the source states no order": a JDL v1 entity can be declared
/// alphabetically by chance,
/// where appending `delta` to `alpha, beta, gamma` would put it third in the
/// model and fourth in the file.
///
/// The entities below are chosen so the two answers differ: each is in label
/// order, and each added field sorts into the middle. `model check --frozen`
/// is the assertion, because it is the one that recompiles from the file and
/// compares bytes -- exactly the check a divergence here breaks.
#[test]
fn adding_a_field_leaves_the_model_and_its_source_agreeing() {
    let jdl = jdl_project(
        "model-field-placement-jdl",
        r#"jdl 1

app Ord {
  pkg com.example.ord
  java 26
  platform plain
  build maven
  storage none
}

entity Task {
  alpha: uuid   @pk
  beta:  string
  gamma: string
}
"#,
    );
    write_plain_fixture(&jdl);
    let synced = jails_cmd(&jdl, None).args(["sync"]).output().unwrap();
    assert!(
        synced.status.success(),
        "{}",
        String::from_utf8_lossy(&synced.stderr)
    );
    let added = jails_cmd(&jdl, None)
        .args(["g", "field", "Task", "delta:string?"])
        .output()
        .unwrap();
    assert!(
        added.status.success(),
        "{}",
        String::from_utf8_lossy(&added.stderr)
    );
    let frozen = jails_cmd(&jdl, None)
        .args(["model", "check", "--frozen"])
        .output()
        .unwrap();
    assert!(
        frozen.status.success(),
        "JDL v1 appends, so the record must too:\n{}",
        String::from_utf8_lossy(&frozen.stderr)
    );
    let record =
        fs::read_to_string(jdl.join("src/main/java/com/example/ord/domain/Task.java")).unwrap();
    let delta = record.find("delta").expect("the added component");
    let gamma = record.find("gamma").expect("the existing component");
    assert!(
        gamma < delta,
        "an appended declaration stays appended:\n{record}"
    );

    // The other side of the same rule: a TOML table states no order, so
    // re-parsing sorts by label and the patch has to place it there.
    let toml = model_project("model-field-placement-toml", MODEL);
    write_spring_fixture(&toml);
    apply_canonical_model(&toml, "initial-field-placement");
    let added = jails_cmd(&toml, None)
        .args(["g", "field", "Note", "body:string?"])
        .output()
        .unwrap();
    assert!(
        added.status.success(),
        "{}",
        String::from_utf8_lossy(&added.stderr)
    );
    let frozen = jails_cmd(&toml, None)
        .args(["model", "check", "--frozen"])
        .output()
        .unwrap();
    assert!(
        frozen.status.success(),
        "a TOML table re-parses by label, so the record must too:\n{}",
        String::from_utf8_lossy(&frozen.stderr)
    );
}

/// JDL v1 §21's first conformance family: the §4 complete example is a
/// fixture, not prose.
///
/// §21 says the complete example "is an executable conformance fixture" and
/// that documentation examples MUST be extracted in CI rather than copied into
/// disconnected test strings; otherwise the flagship example of the language
/// could stop linking and the only thing that would notice is a reader typing
/// it in.
///
/// The example is read out of `docs/01-jdl-v1.md` rather than pasted here, which is
/// the whole point -- a copy is a second document that drifts silently, and a
/// test asserting a copy links proves nothing about what a reader sees.
///
/// `eject Task.repo.fake` is the line to watch: §16.4 says the preferred
/// ejection reference is a readable boundary path resolved by a boundary
/// registry, and `jails_model::boundary` is that registry. The linker
/// resolves the path to `art_ent_task_repository_memory`, the id the compiler
/// emits the in-memory adapter under, so the example links whole.
#[test]
fn the_specification_complete_example_links_whole() {
    let document =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/01-jdl-v1.md"))
            .expect("docs/01-jdl-v1.md is checked in");
    let section = document
        .split("## 4. Complete example")
        .nth(1)
        .and_then(|rest| rest.split("\n## 5.").next())
        .expect("docs/01-jdl-v1.md still has a §4 complete example");
    let example = section
        .split("```jdl\n")
        .nth(1)
        .and_then(|rest| rest.split("```").next())
        .expect("§4 still carries one jdl block");
    assert!(
        example.starts_with("jdl 1"),
        "the extracted block is not a v1 document:\n{example}"
    );
    assert!(
        example.contains("eject Task.repo.fake"),
        "§4 no longer carries the readable ejection path this test pins:\n{example}"
    );

    // The whole example, as written. This is the assertion that fails when the
    // language and the linker drift apart.
    let root = jdl_project("spec-section-4-whole", example);
    let checked = jails_cmd(&root, None)
        .args(["model", "check"])
        .output()
        .unwrap();
    assert!(
        checked.status.success(),
        "the §4 example does not link:\n{}",
        String::from_utf8_lossy(&checked.stderr)
    );
    fs::remove_dir_all(&root).ok();
}

/// A boundary path an entity does not have refuses at link time, naming the
/// paths it does have: the registry, not the parser, decides what is valid.
#[test]
fn an_unregistered_boundary_path_refuses_with_the_paths_the_owner_has() {
    let root = jdl_project(
        "spec-boundary-path-unknown",
        &format!("{MODEL}\neject Note.repo.mysql\n"),
    );
    let checked = jails_cmd(&root, None)
        .args(["model", "check"])
        .output()
        .unwrap();
    let said = String::from_utf8_lossy(&checked.stderr).into_owned();
    assert!(!checked.status.success(), "{said}");
    assert!(
        said.contains("model-ejection-target") && said.contains("`Note.repo.fake`"),
        "{said}"
    );
    fs::remove_dir_all(&root).ok();
}

/// One model language. A `.jails/model.jdl` that does not open with `jdl 1`,
/// and a project that still has only `.jails/model.toml`, are each refused by
/// name with the file the model has to be.
#[test]
fn a_model_that_is_not_jdl_1_is_refused_by_name() {
    let root = jdl_project("model-not-jdl-1", "application Notes\n");
    let refused = jails_cmd(&root, None)
        .args(["g", "record", "Task", "id:uuid"])
        .output()
        .unwrap();
    assert!(!refused.status.success(), "{refused:?}");
    let told = String::from_utf8_lossy(&refused.stderr);
    assert!(
        told.contains("`.jails/model.jdl` does not start with `jdl 1`"),
        "{told}"
    );
    assert!(told.contains("fix: the model must be `jdl 1`"), "{told}");
    fs::remove_dir_all(&root).ok();

    let root = temp_dir("model-only-toml");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(
        root.join(".jails/model.toml"),
        "schema = \"jails.model.v1\"\n",
    )
    .unwrap();
    for command in [
        ["g", "record", "Task", "id:uuid"].as_slice(),
        ["model", "check"].as_slice(),
    ] {
        let refused = jails_cmd(&root, None).args(command).output().unwrap();
        assert!(!refused.status.success(), "{command:?}: {refused:?}");
        let told = String::from_utf8_lossy(&refused.stderr);
        assert!(
            told.contains("`.jails/model.toml` is not a model this jails reads"),
            "{command:?}: {told}"
        );
        assert!(told.contains("write the model as `jdl 1`"), "{told}");
    }
    assert!(!root.join(".jails/model.jdl").exists());
    fs::remove_dir_all(&root).ok();
}

/// **A deleted `.jails/` is a lost input, not a lost application** -- and the
/// difference has to be said, because jails' answer to "no model" is to seed
/// one.
///
/// The generated tree is under `src/` now, so it survives `rm -rf .jails`
/// whole. Seeding a fresh model beside it reads every generated file as the
/// reader's own: `model status` reports nothing managed, and the first
/// regeneration refuses over a path it wrote itself, several commands after
/// the mistake. The evidence that tells the two apart is the provenance
/// header the compiler writes into every managed file.
#[test]
fn a_project_whose_model_is_gone_is_refused_rather_than_seeded_afresh() {
    let root = temp_dir("model-gone");
    write_spring_fixture(&root);
    let scaffolded = jails_cmd(&root, None)
        .args(["g", "scaffold", "Note", "id:uuid@pk", "title:string!"])
        .output()
        .unwrap();
    assert!(
        scaffolded.status.success(),
        "{}",
        String::from_utf8_lossy(&scaffolded.stderr)
    );
    let entity = root.join("src/main/java/com/example/demo/domain/Note.java");
    assert!(entity.is_file());

    fs::remove_dir_all(root.join(".jails")).unwrap();
    for command in [
        ["g", "record", "Task", "id:uuid"].as_slice(),
        ["model", "init"].as_slice(),
        ["sync"].as_slice(),
    ] {
        let refused = jails_cmd(&root, None).args(command).output().unwrap();
        assert!(!refused.status.success(), "{command:?}: {refused:?}");
        let told = String::from_utf8_lossy(&refused.stderr);
        assert!(
            told.contains("no model at `.jails/model.jdl`") && told.contains("its model is gone"),
            "{command:?}: {told}"
        );
        // The fix is a git restore, and the evidence is a file the reader can
        // look at: a refusal that only says "no model" is one they cannot act
        // on.
        assert!(told.contains("git restore .jails/model.jdl"), "{told}");
        assert!(
            told.contains("src/main/java/com/example/demo/") && told.contains(".java"),
            "{told}"
        );
    }
    // Refused means refused: no second model was seeded on the way out.
    assert!(!root.join(".jails").exists(), "a refusal wrote state");
    assert!(entity.is_file());
    fs::remove_dir_all(&root).ok();
}
