//! `jails app plan|apply`: the declarative manifest, and the proof apps.

use super::*;

#[test]
fn app_init_creates_a_parseable_starter_manifest() {
    let root = temp_dir("app-init");
    write_project_skeleton(&root);

    let init = jails_cmd(&root, None)
        .args(["app", "init"])
        .output()
        .unwrap();
    assert!(
        init.status.success(),
        "{}",
        String::from_utf8_lossy(&init.stderr)
    );
    let manifest = fs::read_to_string(root.join(".jails/app.toml")).unwrap();
    assert!(manifest.contains("schema = 1"), "{manifest}");
    assert!(manifest.contains("timestamps = true"), "{manifest}");

    let plan = jails_cmd(&root, None)
        .args(["app", "plan"])
        .output()
        .unwrap();
    assert!(
        plan.status.success(),
        "{}",
        String::from_utf8_lossy(&plan.stderr)
    );
    let shown = String::from_utf8_lossy(&plan.stdout);
    assert!(shown.contains("nothing was written"), "{shown}");

    let duplicate = jails_cmd(&root, None)
        .args(["app", "init"])
        .output()
        .unwrap();
    assert!(!duplicate.status.success());
    assert!(String::from_utf8_lossy(&duplicate.stderr).contains("fix:"));
}

#[test]
fn app_manifest_plan_is_domain_blind_and_writes_nothing() {
    let root = temp_dir("app-manifest-plan");
    write_spring_fixture(&root);
    let manifest = root.join("crawler.toml");
    fs::write(
        &manifest,
        include_str!("../../examples/web-crawler/.jails/app.toml"),
    )
    .unwrap();

    let output = jails_cmd(&root, None)
        .args(["app", "plan", "--manifest"])
        .arg(&manifest)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    // On a project with no model yet, the plan reports what applying would
    // *declare*. It cannot name files: each row is planned against the model
    // on disk, and under `--pretend` nothing is written, so row two would
    // plan against a model missing row one's enum and refuse over a type the
    // apply declares a moment earlier. Once the model exists, `app plan`
    // names the files -- that is the case below.
    assert!(stdout.contains("would be created"), "{stdout}");
    // The kind the manifest names: `CrawlRun` is a `[[generate]]` row of kind
    // `scaffold`.
    assert!(stdout.contains("declare scaffold CrawlRun"), "{stdout}");
    assert!(stdout.contains("nothing was written"), "{stdout}");
    assert!(!root.join("jails.toml").exists());
    assert!(!root.join(".jails/app-state-v1").exists());
}

#[test]
fn app_manifest_formats_the_complete_generated_tree_once() {
    let root = temp_dir("app-format-once");
    write_plain_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(
        root.join(".jails/app.toml"),
        "schema = 1\ncapabilities = [\"format\"]\n\n[[generate]]\nkind = \"record\"\nname = \"Note\"\nfields = [\"title:string!\"]\n",
    )
    .unwrap();
    let fake = temp_dir("app-format-once-bin");
    let log = fake.join("maven.log");
    write_fake_maven(&fake, &["mvn"], &log);

    let output = jails_cmd(&root, Some(&fake))
        .args(["app", "apply", "--no-start"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let invocations = read_log(&log);
    assert_eq!(
        invocations.lines().count(),
        1,
        "format should run after generation, once: {invocations}"
    );
    assert!(invocations.contains("spotless:apply"), "{invocations}");
    assert!(common::generated(&root, "src/main/java/com/example/demo/domain/Note.java").is_file());
}

/// `app plan` names an entity the manifest has stopped asking for, so a
/// reader can tell "this manifest is fully applied" from "this manifest has
/// quietly dropped something it used to declare".
#[test]
fn app_plan_names_an_entity_the_manifest_no_longer_declares() {
    let root = temp_dir("app-plan-orphan");
    write_plain_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    let manifest = root.join(".jails/app.toml");
    fs::write(
        &manifest,
        "schema = 1\ncapabilities = []\n\n\
         [[generate]]\nkind = \"record\"\nname = \"Keep\"\n\n\
         [[generate]]\nkind = \"record\"\nname = \"Dropped\"\n",
    )
    .unwrap();

    let applied = jails_cmd(&root, None)
        .args(["app", "apply", "--no-start"])
        .output()
        .unwrap();
    assert!(
        applied.status.success(),
        "{}{}",
        String::from_utf8_lossy(&applied.stdout),
        String::from_utf8_lossy(&applied.stderr)
    );

    fs::write(
        &manifest,
        "schema = 1\ncapabilities = []\n\n\
         [[generate]]\nkind = \"record\"\nname = \"Keep\"\n",
    )
    .unwrap();

    let planned = jails_cmd(&root, None)
        .args(["app", "plan"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&planned.stdout);
    assert!(planned.status.success(), "{stdout}");
    // Named by the files it would relinquish: the plan is the apply, so the
    // entity leaving *is* two deletes.
    assert!(
        stdout.contains("delete ") && stdout.contains("Dropped.java"),
        "the dropped entity is named: {stdout}"
    );
    // And the retained one is *not* named: a plan lists what changes, and an
    // entity the manifest still declares and disk already matches changes
    // nothing.
    assert!(
        !stdout
            .lines()
            .any(|line| line.contains("delete") && line.contains("Keep")),
        "{stdout}"
    );
    // Planning stays non-mutating: nothing was removed.
    assert!(
        common::generated(&root, "src/main/java/com/example/demo/domain/Dropped.java").is_file(),
        "`app plan` may not delete anything"
    );
    assert!(
        !stdout.contains("disagree"),
        "the typed and imperative plans must agree: {stdout}"
    );
}

/// Appending a component to a `[[generate]]` block is a forward migration,
/// not a re-render of the sealed create migration: schema history is
/// append-only, and the manifest and the entity must agree about the field
/// list on the next apply.
#[test]
fn a_manifest_field_appended_to_a_scaffold_becomes_a_forward_migration() {
    let root = temp_dir("app-field-evolution");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    let manifest = root.join(".jails/app.toml");
    let write = |fields: &str| {
        fs::write(
            &manifest,
            format!(
                "schema = 1\ncapabilities = [\"db\"]\n\n                 [[generate]]\nkind = \"scaffold\"\nname = \"Deal\"\nfields = [{fields}]\n"
            ),
        )
        .unwrap();
    };
    let apply = |root: &std::path::Path| {
        jails_cmd(root, None)
            .args(["app", "apply", "--no-start"])
            .output()
            .unwrap()
    };

    write("\"id:uuid@pk\", \"amount:decimal\"");
    let applied = apply(&root);
    assert!(
        applied.status.success(),
        "{}{}",
        String::from_utf8_lossy(&applied.stdout),
        String::from_utf8_lossy(&applied.stderr)
    );
    let create = root.join("src/main/resources/db/migration/V001__create_deals.sql");
    let sealed = fs::read(&create).unwrap();

    write("\"id:uuid@pk\", \"amount:decimal\", \"memo:string?\"");
    let evolved = apply(&root);
    assert!(
        evolved.status.success(),
        "{}{}",
        String::from_utf8_lossy(&evolved.stdout),
        String::from_utf8_lossy(&evolved.stderr)
    );
    assert_eq!(fs::read(&create).unwrap(), sealed, "V001 is append-only");
    let added = fs::read_to_string(
        root.join("src/main/resources/db/migration/V002__add_memo_to_deals.sql"),
    )
    .unwrap();
    assert!(added.contains("alter table deals"), "{added}");
    assert!(added.contains("add column memo text"), "{added}");
    let record = common::read_generated(&root, "src/main/java/com/example/demo/domain/Deal.java");
    assert!(record.contains("Optional<String> memo"), "{record}");

    // Re-applying the same manifest changes nothing, which is what makes the
    // evolution a state the manifest describes rather than an event it fires.
    let again = apply(&root);
    assert!(again.status.success(), "{again:?}");
    assert!(
        String::from_utf8_lossy(&again.stdout).contains("nothing to do"),
        "{}",
        String::from_utf8_lossy(&again.stdout)
    );

    // A required component has no backfill the manifest can carry.
    write("\"id:uuid@pk\", \"amount:decimal\", \"memo:string?\", \"note:string\"");
    let refused = apply(&root);
    assert_eq!(refused.status.code(), Some(1), "{refused:?}");
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(stderr.contains("required field `note`"), "{stderr}");
    assert!(stderr.contains("--default-literal"), "{stderr}");

    // And a change that is not an append is refused by name: dropping one
    // component and adding another reads exactly like renaming it.
    write("\"id:uuid@pk\", \"total:decimal\", \"memo:string?\"");
    let refused = apply(&root);
    assert_eq!(refused.status.code(), Some(1), "{refused:?}");
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(stderr.contains("cannot say which change it is"), "{stderr}");
    assert!(
        stderr.contains("jails entity field rename|type|nullability|drop Deal"),
        "{stderr}"
    );
}

/// Removing a table-backed row from the manifest demands the same care the
/// imperative destroy does: `jails destroy scaffold Deal` refuses without a
/// storage policy, because deleting the Java says nothing about what happens
/// to the rows, and deleting the `[[generate]]` block is the same intent.
#[test]
fn a_manifest_removal_of_a_table_backed_row_needs_a_storage_policy() {
    let root = temp_dir("app-remove-storage-policy");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    let manifest = root.join(".jails/app.toml");
    let declared = "schema = 1\ncapabilities = [\"db\"]\n\n         [[generate]]\nkind = \"scaffold\"\nname = \"Deal\"\n         fields = [\"id:uuid@pk\", \"amount:decimal\"]\n";
    fs::write(&manifest, declared).unwrap();
    let applied = jails_cmd(&root, None)
        .args(["app", "apply", "--no-start"])
        .output()
        .unwrap();
    assert!(
        applied.status.success(),
        "{}{}",
        String::from_utf8_lossy(&applied.stdout),
        String::from_utf8_lossy(&applied.stderr)
    );
    let create = root.join("src/main/resources/db/migration/V001__create_deals.sql");
    assert!(create.is_file());

    fs::write(&manifest, "schema = 1\ncapabilities = [\"db\"]\n").unwrap();
    let before = snapshot_tree(&root);
    let refused = jails_cmd(&root, None)
        .args(["app", "apply", "--no-start"])
        .output()
        .unwrap();
    assert_eq!(refused.status.code(), Some(1), "{refused:?}");
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(stderr.contains("storage-policy-required"), "{stderr}");
    assert!(stderr.contains("table `deals`"), "{stderr}");
    // Both options named, exactly as the imperative refusal names them.
    assert!(
        stderr.contains("jails destroy scaffold Deal --storage preserve"),
        "{stderr}"
    );
    assert!(
        stderr.contains("--storage drop --confirm-table deals"),
        "{stderr}"
    );
    assert_eq!(snapshot_tree(&root), before, "the refusal mutated the tree");

    // And the way through is the command it names.
    let retired = jails_cmd(&root, None)
        .args([
            "destroy",
            "scaffold",
            "Deal",
            "--storage",
            "drop",
            "--confirm-table",
            "deals",
            "--force",
        ])
        .output()
        .unwrap();
    assert!(
        retired.status.success(),
        "{}{}",
        String::from_utf8_lossy(&retired.stdout),
        String::from_utf8_lossy(&retired.stderr)
    );
    assert!(
        root.join("src/main/resources/db/migration/V002__drop_deals.sql")
            .is_file(),
        "the retirement appends the drop the manifest could not express"
    );
    assert!(create.is_file(), "V001 is append-only and stays");

    let reapplied = jails_cmd(&root, None)
        .args(["app", "apply", "--no-start"])
        .output()
        .unwrap();
    assert!(
        reapplied.status.success(),
        "{}{}",
        String::from_utf8_lossy(&reapplied.stdout),
        String::from_utf8_lossy(&reapplied.stderr)
    );
}

#[test]
fn app_manifest_merges_an_edited_intent_over_user_changes() {
    let root = temp_dir("app-intent-merge");
    write_plain_fixture(&root);
    let git = std::process::Command::new("git")
        .args(["init", "--quiet"])
        .current_dir(&root)
        .status();
    if !git.is_ok_and(|status| status.success()) {
        skip("git not found on PATH");
        return;
    }
    fs::create_dir_all(root.join(".jails")).unwrap();
    let manifest = root.join(".jails/app.toml");
    fs::write(
        &manifest,
        "schema = 1\ncapabilities = []\n\n[[generate]]\nkind = \"record\"\nname = \"Note\"\nfields = [\"id:uuid@pk\"]\n",
    )
    .unwrap();

    let first = jails_cmd(&root, None)
        .args(["app", "apply", "--no-start"])
        .output()
        .unwrap();
    assert!(
        first.status.success(),
        "{}{}",
        String::from_utf8_lossy(&first.stdout),
        String::from_utf8_lossy(&first.stderr)
    );
    // An older intent registry beside the model. It must be folded into the
    // one bookkeeping file and removed -- two registries that disagree are
    // worse than one.
    fs::write(
        root.join(".jails/app-state-v1"),
        "schema=1\nrecord|Note||id:uuid@pk|false|||\n",
    )
    .unwrap();
    let record = common::generated(&root, "src/main/java/com/example/demo/domain/Note.java");
    let source = fs::read_to_string(&record).unwrap();
    let edited = source.replacen(
        "\n}\n",
        "\n\n    public String userLabel() { return id.toString(); }\n}\n",
        1,
    );
    assert_ne!(edited, source);
    fs::write(&record, edited).unwrap();
    fs::write(
        &manifest,
        "schema = 1\ncapabilities = []\n\n[[generate]]\nkind = \"record\"\nname = \"Note\"\nfields = [\"id:uuid@pk\", \"title:string!\"]\n",
    )
    .unwrap();

    let plan = jails_cmd(&root, None)
        .args(["app", "plan"])
        .output()
        .unwrap();
    assert!(plan.status.success());
    // The plan names the file the edit would rewrite.
    let shown = String::from_utf8_lossy(&plan.stdout);
    // The verb is `write`: the file is managed, it exists, and the plan is
    // rewriting jails' own output over it.
    assert!(shown.contains("write "), "{shown}");
    assert!(shown.contains("Note.java"), "{shown}");
    let update = jails_cmd(&root, None)
        .args(["app", "apply", "--no-start"])
        .output()
        .unwrap();
    assert!(
        update.status.success(),
        "{}{}",
        String::from_utf8_lossy(&update.stdout),
        String::from_utf8_lossy(&update.stderr)
    );
    let merged = fs::read_to_string(&record).unwrap();
    assert!(merged.contains("String title"), "{merged}");
    assert!(merged.contains("userLabel()"), "{merged}");
    assert!(!merged.contains("<<<<<<<"), "{merged}");
    assert!(
        common::ledger_mentions(&root, "entity Note")
            && common::ledger_mentions(&root, "Note.java"),
        "the applied intent is in the model, and it owns the file"
    );

    let second = jails_cmd(&root, None)
        .args(["app", "apply", "--no-start"])
        .output()
        .unwrap();
    assert!(second.status.success());
    // The second apply changes nothing: the merge already happened, the store
    // records it, and re-running a manifest whose rows all match is a no-op.
    assert!(
        String::from_utf8_lossy(&second.stdout).contains("nothing to do"),
        "{}",
        String::from_utf8_lossy(&second.stdout)
    );
}

/// An intent update proceeds outside a git repository: the previous bytes are
/// reproducible from the model and the accepted projection, so git is not
/// needed as the way back.
#[test]
fn app_manifest_updates_an_intent_without_needing_a_git_repository() {
    let root = temp_dir("app-intent-no-git");
    write_plain_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    let manifest = root.join(".jails/app.toml");
    fs::write(
        &manifest,
        "schema = 1\ncapabilities = []\n\n[[generate]]\nkind = \"record\"\nname = \"Note\"\nfields = [\"id:uuid@pk\"]\n",
    )
    .unwrap();
    assert!(
        jails_cmd(&root, None)
            .args(["app", "apply", "--no-start"])
            .status()
            .unwrap()
            .success()
    );
    let record = common::generated(&root, "src/main/java/com/example/demo/domain/Note.java");
    let before = fs::read_to_string(&record).unwrap();
    fs::write(
        &manifest,
        "schema = 1\ncapabilities = []\n\n[[generate]]\nkind = \"record\"\nname = \"Note\"\nfields = [\"id:uuid@pk\", \"title:string!\"]\n",
    )
    .unwrap();

    let output = jails_cmd(&root, None)
        .args(["app", "apply", "--no-start"])
        .env("GIT_CEILING_DIRECTORIES", "/tmp")
        .output()
        .unwrap();

    // No git, and the update proceeds: demanding a repository jails does not
    // need is a refusal with nothing behind it.
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let after = fs::read_to_string(&record).unwrap();
    assert_ne!(after, before, "the update happened");
    // Recoverable without a repository: the model states the shape and the
    // accepted projection in `.jails/compiler.lock.json` is the merge base, so
    // the previous bytes are reproducible by compiling rather than by keeping
    // a copy of them.
    assert!(root.join(".jails/compiler.lock.json").is_file());
    assert!(
        !root.join(".jails/objects").exists(),
        "a canonical project must not grow a legacy object store"
    );
}

#[test]
fn app_apply_keys_a_suffixed_name_to_the_row_generate_writes() {
    // `generate` strips a suffix its kind already implies, so `fetcher
    // AcquirerFetcher` writes files under `Acquirer`. `app apply` records the
    // manifest's spec onto the same row -- it has to normalise identically, or
    // one entity gets two half-rows: files with no spec, and a spec with no
    // files.
    let root = temp_dir("app-suffixed-name");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(
        root.join(".jails/app.toml"),
        "schema = 1\ncapabilities = []\n\n[[generate]]\nkind = \"fetcher\"\nname = \"AcquirerFetcher\"\n",
    )
    .unwrap();
    assert!(
        jails_cmd(&root, None)
            .args(["app", "apply", "--no-start"])
            .status()
            .unwrap()
            .success()
    );

    // One entity, one row: the files landed under the name `generate`
    // normalises to, and the manifest's spec was recorded onto that same row
    // rather than onto a second one keyed by the spelling the manifest used.
    assert!(
        common::ledger_mentions(&root, "Acquirer"),
        "the row is there"
    );
    assert!(
        common::ledger_mentions(&root, "AcquirerFetcher.java"),
        "and it owns the files"
    );
    assert!(
        !common::ledger_mentions(&root, "\u{0}AcquirerFetcher\u{0}"),
        "and there is no second row keyed by the manifest's spelling"
    );
}

#[test]
fn app_manifest_refuses_two_names_that_generate_into_one_entity() {
    let root = temp_dir("app-suffix-collision");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(
        root.join(".jails/app.toml"),
        "schema = 1\ncapabilities = []\n\n[[generate]]\nkind = \"fetcher\"\nname = \"Acquirer\"\n\n[[generate]]\nkind = \"fetcher\"\nname = \"AcquirerFetcher\"\n",
    )
    .unwrap();

    let output = jails_cmd(&root, None)
        .args(["app", "apply", "--no-start"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("declared twice"), "{stderr}");
    assert!(stderr.contains("fix:"), "{stderr}");
    assert!(
        !root
            .join("src/main/java/com/example/demo/clients/AcquirerFetcher.java")
            .exists(),
        "the duplicate gate refuses before any write"
    );
}

#[test]
fn app_manifest_builds_the_crawler_skeleton_and_is_resumable() {
    let root = temp_dir("app-manifest-crawler");
    write_spring_fixture(&root);
    let manifest_dir = root.join(".jails");
    fs::create_dir_all(&manifest_dir).unwrap();
    fs::write(
        manifest_dir.join("app.toml"),
        include_str!("../../examples/web-crawler/.jails/app.toml"),
    )
    .unwrap();

    for attempt in 1..=2 {
        let output = jails_cmd(&root, None)
            .args(["app", "apply", "--no-start"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "attempt {attempt}: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    assert!(
        common::generated(
            &root,
            "src/main/java/com/example/demo/domain/CrawlStatus.java"
        )
        .is_file()
    );
    assert!(
        common::generated(&root, "src/main/java/com/example/demo/domain/CrawlRun.java").is_file()
    );
    assert!(
        common::generated(
            &root,
            "src/main/java/com/example/demo/domain/CrawledPage.java"
        )
        .is_file()
    );
    assert!(
        common::generated(
            &root,
            "src/main/java/com/example/demo/service/QueueCrawlUseCase.java"
        )
        .is_file()
    );
    assert!(
        common::generated(
            &root,
            "src/main/java/com/example/demo/service/StoringQueueCrawlUseCase.java"
        )
        .is_file()
    );
    assert!(
        common::generated(
            &root,
            "src/main/java/com/example/demo/web/QueueCrawlController.java"
        )
        .is_file()
    );
    assert!(
        common::generated(
            &root,
            "src/main/java/com/example/demo/service/RecordCrawledPageUseCase.java"
        )
        .is_file()
    );
    assert!(
        common::generated(
            &root,
            "src/main/java/com/example/demo/service/CrawlRunsByStatusQuery.java"
        )
        .is_file()
    );
    assert!(
        common::generated(
            &root,
            "src/main/java/com/example/demo/adapters/JdbcCrawlRunsByStatusQuery.java"
        )
        .is_file()
    );
    assert!(
        common::generated(
            &root,
            "src/main/java/com/example/demo/web/CrawlRunsByStatusQueryController.java"
        )
        .is_file()
    );
    assert!(
        common::generated(
            &root,
            "src/main/java/com/example/demo/service/PagesByCrawlRunQuery.java"
        )
        .is_file()
    );
    assert!(
        common::generated(
            &root,
            "src/main/java/com/example/demo/adapters/JdbcPagesByCrawlRunQuery.java"
        )
        .is_file()
    );
    assert!(
        common::generated(
            &root,
            "src/main/java/com/example/demo/clients/PageFetcher.java"
        )
        .is_file()
    );
    let safe_fetcher = fs::read_to_string(common::generated(
        &root,
        "src/main/java/com/example/demo/clients/SafePageFetcher.java",
    ))
    .unwrap();
    assert!(
        safe_fetcher.contains("new PinnedResolver"),
        "{safe_fetcher}"
    );
    assert!(
        safe_fetcher.contains("private or reserved address"),
        "{safe_fetcher}"
    );
    assert!(
        safe_fetcher.contains("acceptedStatuses.contains(response.statusCode())"),
        "{safe_fetcher}"
    );
    assert!(
        common::generated(
            &root,
            "src/main/java/com/example/demo/jobs/SiteTraversalWorkflow.java"
        )
        .is_file()
    );
    assert!(
        common::generated(
            &root,
            "src/main/java/com/example/demo/web/SiteTraversalWorkflowController.java"
        )
        .is_file()
    );
    assert!(
        common::generated(
            &root,
            "src/test/java/com/example/demo/jobs/SiteTraversalWorkflowIT.java"
        )
        .is_file()
    );
    assert!(
        common::generated(
            &root,
            "src/main/java/com/example/demo/messaging/PageDiscoveredEvent.java"
        )
        .is_file()
    );
    assert!(
        common::generated(
            &root,
            "src/main/java/com/example/demo/jobs/CrawlDispatcherWork.java"
        )
        .is_file()
    );
    assert!(
        common::generated(
            &root,
            "src/main/java/com/example/demo/jobs/SchedulingConfig.java"
        )
        .is_file()
    );
    assert!(
        common::generated(
            &root,
            "src/main/java/com/example/demo/jobs/JdbcCrawlDispatcherStore.java"
        )
        .is_file()
    );
    assert!(
        common::generated(
            &root,
            "src/main/java/com/example/demo/jobs/CrawlDispatcherWorker.java"
        )
        .is_file()
    );
    assert!(
        common::generated(
            &root,
            "src/main/java/com/example/demo/web/CrawlDispatcherJobController.java"
        )
        .is_file()
    );
    // `.jails/` holds the reader's manifest, the one editable model, the lock
    // sealing the projection it was compiled from, the executor's own lock,
    // and the two files that tell git what to do with them -- and nothing
    // else: no output lives here. Closed rather than counted, so any other
    // bookkeeping appearing here fails.
    let bookkeeping = fs::read_dir(root.join(".jails"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        bookkeeping,
        [
            ".gitattributes",
            ".gitignore",
            "app.toml",
            "apply.lock",
            // The merge base, one file per managed path: what the next
            // compile diffs against, and what makes a merge a merge.
            "base",
            "compiler.lock.json",
            "model.jdl",
            // One run's scratch: the executor's lock lives beside it and the
            // last applied plan inside it, which is what `jails undo`
            // reverses. The state `.gitignore` keeps both out of every
            // commit.
            "run",
        ]
        .map(str::to_string)
        .into(),
        "{bookkeeping:?}"
    );
    assert!(root.join("Dockerfile").is_file());
    assert!(root.join(".github/workflows/ci.yml").is_file());
    assert!(root.join(".github/workflows/image.yml").is_file());
    // Four tables: two entities, the workflow and the durable job. An index
    // asked for at creation is part of its `create table`, so it adds no
    // migration of its own. Counted rather than named because the point is
    // that nothing appends twice on a replay -- what each one is, is in the
    // file.
    assert_eq!(
        fs::read_dir(root.join("src/main/resources/db/migration"))
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "sql"))
            .count(),
        4
    );
}

#[test]
fn app_manifest_builds_the_support_inbox_from_the_same_generic_intents() {
    let root = temp_dir("app-manifest-inbox");
    write_spring_fixture(&root);
    let manifest_dir = root.join(".jails");
    fs::create_dir_all(&manifest_dir).unwrap();
    fs::write(
        manifest_dir.join("app.toml"),
        include_str!("../../examples/support-inbox/.jails/app.toml"),
    )
    .unwrap();

    let output = jails_cmd(&root, None)
        .args(["app", "apply", "--no-start"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    for name in [
        "Workspace",
        "Member",
        "Inbox",
        "InboxMember",
        "Contact",
        "Conversation",
        "Message",
        "ConversationAssignment",
    ] {
        assert!(
            common::generated(
                &root,
                &format!("src/main/java/com/example/demo/domain/{name}.java")
            )
            .is_file(),
            "{name}"
        );
        assert!(
            common::generated(
                &root,
                &format!("src/main/java/com/example/demo/web/{name}Controller.java")
            )
            .is_file(),
            "{name} controller"
        );
    }
    for name in [
        "ConversationStatus",
        "MessageDirection",
        "MemberRole",
        "InboxChannel",
        "AssignmentStatus",
    ] {
        assert!(
            common::generated(
                &root,
                &format!("src/main/java/com/example/demo/domain/{name}.java")
            )
            .is_file(),
            "{name}"
        );
    }
    for name in [
        "CreateWorkspace",
        "CreateMember",
        "CreateInbox",
        "AddInboxMember",
        "CreateContact",
        "OpenConversation",
        "AssignConversation",
        "ReceiveMessage",
    ] {
        assert!(
            common::generated(
                &root,
                &format!("src/main/java/com/example/demo/service/{name}UseCase.java")
            )
            .is_file(),
            "{name} usecase"
        );
        assert!(
            common::generated(
                &root,
                &format!("src/main/java/com/example/demo/web/{name}Controller.java")
            )
            .is_file(),
            "{name} controller"
        );
    }
    for name in [
        "ContactsByWorkspace",
        "MembersByWorkspace",
        "InboxesByWorkspace",
        "InboxMembersByInbox",
        "ConversationsByWorkspace",
        "MessagesByConversation",
        "AssignmentByConversation",
    ] {
        assert!(
            common::generated(
                &root,
                &format!("src/main/java/com/example/demo/service/{name}Query.java")
            )
            .is_file(),
            "{name} query"
        );
        assert!(
            common::generated(
                &root,
                &format!("src/main/java/com/example/demo/adapters/Jdbc{name}Query.java")
            )
            .is_file(),
            "{name} JDBC adapter"
        );
        assert!(
            common::generated(
                &root,
                &format!("src/main/java/com/example/demo/web/{name}QueryController.java")
            )
            .is_file(),
            "{name} controller"
        );
    }
    assert!(
        common::generated(
            &root,
            "src/main/java/com/example/demo/messaging/MessageReceivedEvent.java"
        )
        .is_file()
    );
    // One bean, one transaction: the command *port* is the ABI and
    // `Jdbc<X>Command` is its one implementation, so staging goes in the
    // statement's own method under `@Transactional` -- with no `@Primary`
    // deciding which of two implementations Spring injects.
    assert!(
        !common::generated(
            &root,
            "src/main/java/com/example/demo/service/OutboxReceiveMessageUseCase.java"
        )
        .exists(),
        "the outbox must not be a second bean in front of the command"
    );
    let command = fs::read_to_string(common::generated(
        &root,
        "src/main/java/com/example/demo/adapters/jdbc/JdbcReceiveMessageCommand.java",
    ))
    .unwrap();
    assert!(command.contains("@Transactional"), "{command}");
    assert!(command.contains("outbox.stage("), "{command}");
    let outbox = fs::read_to_string(common::generated(
        &root,
        "src/main/java/com/example/demo/jobs/JdbcReceiveMessageOutbox.java",
    ))
    .unwrap();
    assert!(outbox.contains("for update skip locked"), "{outbox}");
    assert!(
        common::generated(
            &root,
            "src/main/java/com/example/demo/jobs/ReceiveMessageOutboxWorker.java"
        )
        .is_file()
    );
    assert!(
        common::generated(
            &root,
            "src/main/java/com/example/demo/jobs/SchedulingConfig.java"
        )
        .is_file()
    );
    // The port, not a service in front of it: `application/transitions` is the
    // ABI every adapter and controller names.
    assert!(
        common::generated(
            &root,
            "src/main/java/com/example/demo/application/transitions/ChangeConversationStatusTransition.java"
        )
        .is_file()
    );
    let transition = fs::read_to_string(common::generated(
        &root,
        "src/main/java/com/example/demo/adapters/JdbcChangeConversationStatusTransition.java",
    ))
    .unwrap();
    assert!(transition.contains("version = version + 1"), "{transition}");
    assert!(
        transition.contains("public class JdbcChangeConversationStatusTransition"),
        "{transition}"
    );
    // `:scope_` prefixes the bound claim rather than the column name, so a
    // scoped entity that also has a `workspaceId` *field* binds two distinct
    // parameters instead of one that silently wins.
    assert!(
        transition.contains("workspace_id = :scope_workspace_id"),
        "{transition}"
    );
    assert!(
        common::generated(
            &root,
            "src/main/java/com/example/demo/web/ChangeConversationStatusController.java"
        )
        .is_file()
    );
    let assignment_transition = fs::read_to_string(common::generated(
        &root,
        "src/main/java/com/example/demo/adapters/JdbcReassignConversationTransition.java",
    ))
    .unwrap();
    assert!(
        assignment_transition.contains("member_id = :member_id"),
        "{assignment_transition}"
    );
    assert!(
        assignment_transition.contains("workspace_id = :scope_workspace_id"),
        "{assignment_transition}"
    );
    assert!(
        common::generated(
            &root,
            "src/main/java/com/example/demo/jobs/ReceiveMessageOutboxSink.java"
        )
        .is_file()
    );
    assert!(
        common::generated(
            &root,
            "src/main/java/com/example/demo/jobs/ReceiveMessageKafkaOutboxSink.java"
        )
        .is_file()
    );
    let provider = fs::read_to_string(common::generated(
        &root,
        "src/main/java/com/example/demo/jobs/ProviderHttpOutboxSink.java",
    ))
    .unwrap();
    assert!(provider.contains("Idempotency-Key"), "{provider}");
    assert!(provider.contains("HttpClient.Redirect.NEVER"), "{provider}");
    assert!(
        common::generated(
            &root,
            "src/test/java/com/example/demo/jobs/ProviderHttpOutboxSinkTest.java"
        )
        .is_file()
    );
    for name in [
        "ContactWorkspace",
        "MemberWorkspace",
        "InboxWorkspace",
        "ConversationContact",
        "ConversationInbox",
        "InboxMemberInbox",
        "InboxMemberMember",
        "MessageConversation",
        "AssignmentConversation",
        "AssignmentMember",
    ] {
        assert!(
            common::generated(
                &root,
                &format!("src/test/java/com/example/demo/adapters/{name}AssociationIT.java")
            )
            .is_file(),
            "{name} association test"
        );
    }
    let contacts = fs::read_to_string(common::generated(
        &root,
        "src/main/java/com/example/demo/web/ContactsByWorkspaceQueryController.java",
    ))
    .unwrap();
    // The claim reaches the query as an `ExecutionContext` entry rather than
    // as an authorization call the controller makes and discards: the scoped
    // predicate is in the statement, so the value has to travel to it.
    assert!(
        contacts.contains(r#"scopes.claim(authentication, "workspaceId")"#),
        "{contacts}"
    );
    assert!(contacts.contains("ExecutionContext"), "{contacts}");
    // One controller per operation, so a scoped creation endpoint cannot
    // accidentally also serve a read the model never declared.
    let contact_controller = fs::read_to_string(common::generated(
        &root,
        "src/main/java/com/example/demo/web/CreateContactController.java",
    ))
    .unwrap();
    assert!(
        contact_controller.contains("RequestMethod.POST"),
        "{contact_controller}"
    );
    assert!(
        !contact_controller.contains("RequestMethod.GET"),
        "{contact_controller}"
    );
    assert!(root.join("Dockerfile").is_file());
    assert!(root.join(".github/workflows/ci.yml").is_file());
    // One forward migration per change rather than one per replayed row: a
    // table and each declared relation's foreign key are separate statements
    // with separate names, so a history reads as a list of what happened
    // rather than as `evolve_application_schema` twenty times. An index asked
    // for at creation is inside its own `create table`, because a table and
    // an index requested in one command are one plan.
    let migrations = fs::read_dir(root.join("src/main/resources/db/migration"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().to_string())
        .collect::<std::collections::BTreeSet<_>>();
    let inbox = migrations
        .iter()
        .find(|name| name.ends_with("__create_inboxes.sql"))
        .expect("the inbox table has its own migration");
    let inbox_migration =
        fs::read_to_string(root.join("src/main/resources/db/migration").join(inbox)).unwrap();
    assert!(
        inbox_migration.contains("create table inboxes"),
        "{inbox_migration}"
    );
    // The indexes are here rather than in migrations of their own.
    assert!(
        migrations
            .iter()
            .filter(|name| name.contains("__create_"))
            .any(|name| {
                fs::read_to_string(root.join("src/main/resources/db/migration").join(name))
                    .unwrap()
                    .contains("create index ")
            }),
        "an index asked for at creation belongs in its `create table`: {migrations:?}"
    );
    // Eight entity tables plus the outbox, ten foreign keys, and no
    // `add_idx` migration at all: every index here was asked for at creation.
    assert_eq!(
        (
            migrations
                .iter()
                .filter(|name| name.contains("__create_"))
                .count(),
            migrations
                .iter()
                .filter(|name| name.contains("__add_idx_"))
                .count(),
            migrations
                .iter()
                .filter(|name| name.contains("__add_fk_"))
                .count(),
        ),
        (9, 0, 10),
        "{migrations:?}"
    );
}

#[test]
fn app_manifests_compile_without_manual_source_edits() {
    if !real_mvn_available() {
        skip("mvn not found on PATH");
        return;
    }
    if !real_java_available() {
        skip("java/javac not found on PATH");
        return;
    }
    let path = real_path_without_mvnd();
    // `verify` contains compile and test-compile. Both Rust tests share this
    // exact execution through the OnceLock above; no generated test is
    // omitted.
    let verified = verified_app_unit_fixtures(&path);
    assert_eq!(verified.len(), SPRING_APP_MANIFESTS.len());
    for (name, root) in verified {
        assert!(
            root.join("target/classes").is_dir(),
            "{name} main sources did not compile"
        );
        assert!(
            root.join("target/test-classes").is_dir(),
            "{name} test sources did not compile"
        );
    }
}

#[test]
fn app_manifests_pass_the_full_generated_verification_gate() {
    if !real_mvn_available() {
        skip("mvn not found on PATH");
        return;
    }
    if !real_java_available() {
        skip("java/javac not found on PATH");
        return;
    }
    if !real_docker_available() {
        skip("a running Docker-compatible container runtime is required");
        return;
    }
    let path = real_path_without_mvnd();
    let fixtures = verified_app_fixtures(&path);
    verified_app_images(fixtures);
}

/// The control application: the crawler, the inbox and the payments gateway
/// are all Spring Boot, so a Spring-shaped assumption in the generic machinery
/// is invisible to every one of them.
///
/// It runs against the **plain** fixture -- no parent POM, no starters, no
/// container -- and asks for `value`, `sealed`, `strategy`, `record`, `cli`
/// and `command`, which the three Spring manifests never touch. `mvn verify`
/// here is seconds rather than minutes, so this is the cheapest gate in the
/// suite and the one that catches "it only works because Spring".
#[test]
fn ledger_cli_manifest_builds_without_spring() {
    if !real_mvn_available() {
        skip("mvn not found on PATH");
        return;
    }
    if !real_java_supports_target_release() {
        skip(&format!(
            "javac on PATH does not support --release {TARGET_RELEASE}"
        ));
        return;
    }
    let path = real_path_without_mvnd();
    let root = verified_plain_toolbox(&path);

    // The manifest names the dispatcher its command belongs to, so the
    // registration is part of what this gate proves rather than a note. The
    // dispatcher is compiler output, and this assertion is about what the
    // compiler put in it.
    let dispatcher =
        fs::read_to_string(root.join("src/main/java/com/example/demo/cli/LedgerCli.java")).unwrap();
    assert!(
        dispatcher.contains("ReconcileCommand::run"),
        "the manifest named its dispatcher, so the command must be registered in it: {dispatcher}"
    );

    // And the jar starts *that* dispatcher. `new-cli` writes `App.java` and
    // names it as the entry point; a manifest that then generates `LedgerCli`
    // and registers `reconcile` into it must move the entry point, or the jar
    // answers only `help`. The entry point moves when it is still jails' own
    // stub and nobody has registered anything there.
    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    assert!(
        pom.contains("<mainClass>com.example.demo.cli.LedgerCli</mainClass>"),
        "the packaged jar does not start the dispatcher the manifest built:\n{pom}"
    );

    assert!(root.join("target/classes").is_dir());
}

/// One command is one report, however many rows it replays.
///
/// **A replay is many mutations in one process, and it used to print like
/// many commands.** Each row printed its own header, its own plan digest and
/// its whole file list: the web-crawler manifest was 887 lines of them, and
/// none of the fourteen digests answered the question a reader runs `new
/// --app` to ask. A row files one summary line now and the replay prints them
/// together, so the report is as long as the manifest rather than as long as
/// the tree.
///
/// The ceiling is generous on purpose -- it is a shape test, not a golden --
/// and a manifest of a dozen rows landing anywhere near it means the per-row
/// report has come back.
#[test]
fn a_manifest_replay_prints_one_report_however_many_rows_it_has() {
    let workspace = temp_dir("app-one-report");
    fs::create_dir_all(&workspace).unwrap();
    let manifest = workspace.join("app.toml");
    let mut rows = String::from("schema = 1\ncapabilities = [\"json\"]\n");
    for name in ["Alpha", "Bravo", "Charlie", "Delta", "Echo", "Foxtrot"] {
        rows.push_str(&format!(
            "\n[[generate]]\nkind = \"record\"\nname = \"{name}\"\nfields = [\"id:uuid@pk\", \"label:string!\"]\n"
        ));
    }
    fs::write(&manifest, rows).unwrap();

    let created = std::process::Command::new(env!("CARGO_BIN_EXE_jails"))
        .current_dir(&workspace)
        .args(["new-cli", "demo", "--no-git", "--app"])
        .arg(&manifest)
        .output()
        .unwrap();
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    let report = String::from_utf8_lossy(&created.stdout).to_string();
    assert_eq!(
        report.matches("applied").count(),
        1,
        "one command, one `applied` line:\n{report}"
    );
    assert!(
        report.lines().count() < 150,
        "a replay of seven rows printed {} lines:\n{report}",
        report.lines().count()
    );
    // Grouped by row, one line each, and the totals under them.
    for name in ["Alpha", "Bravo", "Charlie", "Delta", "Echo", "Foxtrot"] {
        assert!(
            report
                .lines()
                .any(|line| line.starts_with(&format!("  {name} "))),
            "no line for `{name}`:\n{report}"
        );
    }
    assert!(report.contains("applied 7 manifest rows"), "{report}");
    // A digest belongs to the plan, and `--output json` is where a reader who
    // wants one is already looking.
    assert!(!report.contains("sha256:"), "{report}");
}
