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
    // The plan *is* the apply, stopped one step before the lock, so it names
    // the files rather than restating the manifest rows: what it lists is
    // exactly what an apply would then write.
    assert!(stdout.contains("plan "), "{stdout}");
    assert!(stdout.contains("CrawlRun.java"), "{stdout}");
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

/// `app plan` names an entity the manifest has stopped asking for.
///
/// It was silent about those: a reader could not tell "this manifest is fully
/// applied" from "this manifest has quietly dropped something it used to
/// declare". Verified against the previous binary, which printed only the
/// retained entity. This is what the owner model buys — the imperative plan
/// compares each declaration against a recorded row and so can only speak
/// about declarations that still exist.
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
    // Named by the files it would relinquish. V1 printed `orphan record
    // Dropped` from a separate walk over the intent list; here the plan is
    // the apply, so the entity leaving *is* two deletes.
    assert!(
        stdout.contains("delete ") && stdout.contains("Dropped.java"),
        "the dropped entity is named: {stdout}"
    );
    // And the retained one is *not* named: a plan lists what changes, and an
    // entity the manifest still declares and disk already matches changes
    // nothing. V1's walk printed a row per intent whether or not it had
    // anything to say.
    assert!(!stdout.contains("Keep.java"), "{stdout}");
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

/// Appending a component to a `[[generate]]` block is a forward migration.
///
/// The most common shape change there is, and the declarative path could not
/// express it: re-planning the scaffold at the new list re-rendered
/// `V001__create_deals.sql` with the extra column, the append-only seal
/// refused, and the offered fix named something the manifest has no syntax
/// for. `jails resource field add` was not an escape either -- it works on the
/// imperative identity, so the manifest and the entity disagreed about the
/// field list on the very next apply.
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
    let record = fs::read_to_string(common::generated(
        &root,
        "src/main/java/com/example/demo/domain/Deal.java",
    ))
    .unwrap();
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
    assert!(stderr.contains("required component `note`"), "{stderr}");
    assert!(stderr.contains("--default-literal"), "{stderr}");

    // And a change that is not an append is refused by name: dropping one
    // component and adding another reads exactly like renaming it.
    write("\"id:uuid@pk\", \"total:decimal\", \"memo:string?\"");
    let refused = apply(&root);
    assert_eq!(refused.status.code(), Some(1), "{refused:?}");
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(stderr.contains("cannot say which change it is"), "{stderr}");
    assert!(
        stderr.contains("jails resource field rename|type|nullability|drop Deal"),
        "{stderr}"
    );
}

/// Removing a table-backed row from the manifest demands the same care the
/// imperative destroy does.
///
/// `jails destroy scaffold Deal` refuses without a storage policy, because
/// deleting the Java says nothing about what happens to the rows. Deleting the
/// `[[generate]]` block did the same removal with no policy, no confirmation
/// and no `drop table` migration: the table survived with no code that knows
/// about it, and nothing reports an orphan. The same intent, expressed two
/// ways, got two different levels of care.
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
    // Simulate a project written by the schema-1 state format. The migration
    // must recover the old field spec from the recorded model (the legacy comma
    // join was ambiguous for map<K,V>) and fold this file into the one ledger,
    // removing it -- two registries that disagree are worse than one.
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
    // The plan names the file the edit would rewrite. V1 printed `update
    // generate record Note` from a walk of the intent list, which could not
    // see whether the file on disk actually differed.
    let shown = String::from_utf8_lossy(&plan.stdout);
    assert!(shown.contains("replace "), "{shown}");
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
        common::ledger_mentions(&root, "record") && common::ledger_mentions(&root, "Note"),
        "the applied intent is on the one ledger"
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

/// V1 refused an intent update outside a git repository, because it
/// overwrote the file irreversibly and git was the only way back. V2 records
/// the exact previous bytes as a guarded preimage before it writes, so the
/// recovery git was standing in for is jails' own.
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

    // No git, and the update proceeds. V1 refused, because it overwrote the
    // file irreversibly and git was the only way back. V2 records the exact
    // previous bytes as a guarded preimage in its own object store before it
    // writes, so the recovery git was standing in for is jails' own -- and
    // demanding a repository jails does not need is a refusal with nothing
    // behind it.
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let after = fs::read_to_string(&record).unwrap();
    assert_ne!(after, before, "the update happened");
    assert!(root.join(".jails/objects").is_dir(), "and is recoverable");
}

#[test]
fn app_apply_keys_a_suffixed_name_to_the_row_generate_writes() {
    // `generate` strips a suffix its kind already implies, so `fetcher
    // AcquirerFetcher` writes files under `Acquirer`. `app apply` records the
    // manifest's spec onto the same row -- it has to normalise identically, or
    // one entity gets two half-rows: files with no spec, and a spec with no
    // files. `doctor` then reports the empty half as an unowned legacy entity
    // and offers an adopt command for nothing.
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

    let main = common::generated(&root, "src/main/java/com/example/demo");
    assert!(main.join("domain/CrawlStatus.java").is_file());
    assert!(main.join("domain/CrawlRun.java").is_file());
    assert!(main.join("domain/CrawledPage.java").is_file());
    assert!(main.join("service/QueueCrawlUseCase.java").is_file());
    assert!(main.join("service/StoringQueueCrawlUseCase.java").is_file());
    assert!(main.join("web/QueueCrawlController.java").is_file());
    assert!(main.join("service/RecordCrawledPageUseCase.java").is_file());
    assert!(main.join("service/CrawlRunsByStatusQuery.java").is_file());
    assert!(
        main.join("adapters/JdbcCrawlRunsByStatusQuery.java")
            .is_file()
    );
    assert!(
        main.join("web/CrawlRunsByStatusQueryController.java")
            .is_file()
    );
    assert!(main.join("service/PagesByCrawlRunQuery.java").is_file());
    assert!(
        main.join("adapters/JdbcPagesByCrawlRunQuery.java")
            .is_file()
    );
    assert!(main.join("clients/PageFetcher.java").is_file());
    let safe_fetcher = fs::read_to_string(main.join("clients/SafePageFetcher.java")).unwrap();
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
    assert!(main.join("jobs/SiteTraversalWorkflow.java").is_file());
    assert!(
        main.join("web/SiteTraversalWorkflowController.java")
            .is_file()
    );
    assert!(
        common::generated(
            &root,
            "src/test/java/com/example/demo/jobs/SiteTraversalWorkflowIT.java"
        )
        .is_file()
    );
    assert!(main.join("messaging/PageDiscoveredEvent.java").is_file());
    assert!(main.join("jobs/CrawlDispatcherWork.java").is_file());
    assert!(main.join("jobs/SchedulingConfig.java").is_file());
    assert!(main.join("jobs/JdbcCrawlDispatcherStore.java").is_file());
    assert!(main.join("jobs/CrawlDispatcherWorker.java").is_file());
    assert!(main.join("web/CrawlDispatcherJobController.java").is_file());
    assert!(root.join(".jails/ledger.toml").is_file());
    // One *registry*, not four. `.jails/` holds the reader's manifest, jails'
    // one registry, and the executor's own state -- an object store, a
    // transaction log, receipts and a lock. Closed rather than counted, so a
    // second registry growing back still fails and so does an executor path
    // nobody wrote down.
    let bookkeeping = fs::read_dir(root.join(".jails"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        bookkeeping,
        [
            "app.toml",
            "architecture.toml",
            "ledger.toml",
            "lock",
            "objects",
            "receipts",
            "transactions",
        ]
        .map(str::to_string)
        .into(),
        "{bookkeeping:?}"
    );
    assert!(root.join("Dockerfile").is_file());
    assert!(root.join(".github/workflows/ci.yml").is_file());
    assert!(root.join(".github/workflows/image.yml").is_file());
    assert_eq!(
        fs::read_dir(root.join("src/main/resources/db/migration"))
            .unwrap()
            .count(),
        5
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

    let main = common::generated(&root, "src/main/java/com/example/demo");
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
        assert!(main.join(format!("domain/{name}.java")).is_file(), "{name}");
        assert!(
            main.join(format!("web/{name}Controller.java")).is_file(),
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
        assert!(main.join(format!("domain/{name}.java")).is_file(), "{name}");
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
            main.join(format!("service/{name}UseCase.java")).is_file(),
            "{name} usecase"
        );
        assert!(
            main.join(format!("web/{name}Controller.java")).is_file(),
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
            main.join(format!("service/{name}Query.java")).is_file(),
            "{name} query"
        );
        assert!(
            main.join(format!("adapters/Jdbc{name}Query.java"))
                .is_file(),
            "{name} JDBC adapter"
        );
        assert!(
            main.join(format!("web/{name}QueryController.java"))
                .is_file(),
            "{name} controller"
        );
    }
    assert!(main.join("messaging/MessageReceivedEvent.java").is_file());
    assert!(
        main.join("service/OutboxReceiveMessageUseCase.java")
            .is_file()
    );
    let outbox = fs::read_to_string(main.join("jobs/JdbcReceiveMessageOutbox.java")).unwrap();
    assert!(outbox.contains("for update skip locked"), "{outbox}");
    assert!(main.join("jobs/ReceiveMessageOutboxWorker.java").is_file());
    assert!(main.join("jobs/SchedulingConfig.java").is_file());
    assert!(
        main.join("service/ChangeConversationStatusUseCase.java")
            .is_file()
    );
    let transition =
        fs::read_to_string(main.join("adapters/JdbcChangeConversationStatusTransition.java"))
            .unwrap();
    assert!(transition.contains("version = version + 1"), "{transition}");
    assert!(
        transition.contains("public class JdbcChangeConversationStatusTransition"),
        "{transition}"
    );
    assert!(
        transition.contains("workspace_id = :workspace_id"),
        "{transition}"
    );
    assert!(
        main.join("web/ChangeConversationStatusController.java")
            .is_file()
    );
    let assignment_transition =
        fs::read_to_string(main.join("adapters/JdbcReassignConversationTransition.java")).unwrap();
    assert!(
        assignment_transition.contains("member_id = :member_id"),
        "{assignment_transition}"
    );
    assert!(
        assignment_transition.contains("workspace_id = :workspace_id"),
        "{assignment_transition}"
    );
    assert!(main.join("jobs/ReceiveMessageOutboxSink.java").is_file());
    assert!(
        main.join("jobs/ReceiveMessageKafkaOutboxSink.java")
            .is_file()
    );
    let provider = fs::read_to_string(main.join("jobs/ProviderHttpOutboxSink.java")).unwrap();
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
    let contacts =
        fs::read_to_string(main.join("web/ContactsByWorkspaceQueryController.java")).unwrap();
    assert!(contacts.contains("scopeAuthorizer.require"), "{contacts}");
    let contact_controller = fs::read_to_string(main.join("web/ContactController.java")).unwrap();
    assert!(contact_controller.contains("Scope-safe creation endpoint"));
    assert!(
        !contact_controller.contains("@GetMapping"),
        "{contact_controller}"
    );
    assert!(root.join("Dockerfile").is_file());
    assert!(root.join(".github/workflows/ci.yml").is_file());
    let inbox_migration =
        fs::read_to_string(root.join("src/main/resources/db/migration/V003__create_inboxes.sql"))
            .unwrap();
    assert!(
        inbox_migration.contains("create table inboxes"),
        "{inbox_migration}"
    );
    assert_eq!(
        fs::read_dir(root.join("src/main/resources/db/migration"))
            .unwrap()
            .count(),
        20
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
    // `verify` contains compile and test-compile and is therefore stronger
    // than the old preliminary lifecycle. Both Rust tests share this exact
    // execution through the OnceLock above; no generated test is omitted.
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

/// The control application: `plan.md` §4.4's whole point is that the crawler,
/// the inbox and the payments gateway are all Spring Boot, so a Spring-shaped
/// assumption in the generic machinery is invisible to every one of them.
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
    // registration is part of what this gate proves rather than a note.
    // Under `.jails/generated` since the toolbox became canonical: the
    // dispatcher is compiler output, and this assertion is about what the
    // compiler put in it.
    let dispatcher = fs::read_to_string(
        root.join(".jails/generated/main/java/com/example/demo/cli/LedgerCli.java"),
    )
    .unwrap();
    assert!(
        dispatcher.contains("ReconcileCommand::run"),
        "the manifest named its dispatcher, so the command must be registered in it: {dispatcher}"
    );

    // And the jar starts *that* dispatcher. `new-cli` writes `App.java` and
    // names it as the entry point; a manifest that then generates `LedgerCli`
    // and registers `reconcile` into it used to produce a jar answering only
    // `help`, with `jails run -- reconcile` reporting "unknown command". The
    // entry point moves when it is still jails' own stub and nobody has
    // registered anything there.
    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    assert!(
        pom.contains("<mainClass>com.example.demo.cli.LedgerCli</mainClass>"),
        "the packaged jar does not start the dispatcher the manifest built:\n{pom}"
    );

    assert!(root.join("target/classes").is_dir());
}
