//! `jails generate` and `jails destroy`: the per-kind artifacts.

use super::*;

#[test]
fn generate_standalone_and_destroy_roundtrip() {
    let root = temp_dir("standalone-roundtrip");
    write_project_skeleton(&root);

    let status = jails_cmd(&root, None)
        .args(["generate", "controller", "comment"])
        .status()
        .unwrap();
    assert!(status.success());
    let file = root.join("src/main/java/com/example/demo/web/CommentController.java");
    assert!(file.is_file());
    let contents = fs::read_to_string(&file).unwrap();
    assert!(contents.contains("class CommentController"));
    assert!(
        !contents.contains("public class"),
        "spring.md §2: a controller is an entry point, not module API"
    );
    // Rails generates a test alongside `generate controller`; jails matches that.
    let test_file = root.join("src/test/java/com/example/demo/web/CommentControllerTest.java");
    assert!(test_file.is_file(), "expected {}", test_file.display());

    let status = jails_cmd(&root, None)
        .args(["destroy", "controller", "comment", "--force"])
        .status()
        .unwrap();
    assert!(status.success());
    assert!(!file.is_file());
    assert!(!test_file.is_file());
}

#[test]
fn scaffold_refuses_invalid_or_reserved_derived_names_before_projection() {
    let root = temp_dir("scaffold-name-validation");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join("src/main/resources/db/migration")).unwrap();
    let before = snapshot_tree(&root);

    for (name, expected) in [
        ("class", "Java variable `class`"),
        ("Bad!Name", "not valid in a Java identifier"),
        ("A", "PostgreSQL table `as`"),
        ("I", "PostgreSQL table `is`"),
        // `bugs.md` B50. A package member outranks `java.lang`'s implicit
        // import, so `record String(String value)` types its own component as
        // itself -- and compiles, as does its generated test, which is why no
        // tier reported it.
        ("String", "is a type in `java.lang`"),
        ("Record", "is a type in `java.lang`"),
    ] {
        let output = jails_cmd(&root, None)
            .args(["g", "scaffold", name, "id:uuid@pk", "value:int"])
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.code(), Some(1), "{name}: {output:?}");
        assert!(stderr.contains(expected), "{name}: {stderr}");
        assert!(stderr.contains("fix:"), "{name}: {stderr}");
        assert_eq!(snapshot_tree(&root), before, "{name} wrote project files");
    }

    // And a `java.lang` name is refused only where it is *declared*: `Name`
    // validates references too, so refusing it there would have refused every
    // `value:String` in the tool.
    let referenced = jails_cmd(&root, None)
        .args(["g", "record", "Note", "body:String", "count:Integer"])
        .output()
        .unwrap();
    assert!(
        referenced.status.success(),
        "{}",
        String::from_utf8_lossy(&referenced.stderr)
    );
}

#[test]
fn machine_output_carries_failures_that_stop_before_an_outcome() {
    let root = temp_dir("machine-readable-refusals");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join("src/main/resources/db/migration")).unwrap();
    let generated = jails_cmd(&root, None)
        .args(["g", "scaffold", "Task", "id:uuid@pk", "value:int"])
        .output()
        .unwrap();
    assert!(generated.status.success(), "{generated:?}");

    for args in [
        vec![
            "destroy",
            "scaffold",
            "Task",
            "--force",
            "--pretend",
            "--output",
            "json",
        ],
        vec![
            "g",
            "record",
            "Broken",
            "value:nosuchtype",
            "--pretend",
            "--output",
            "json",
        ],
    ] {
        let output = jails_cmd(&root, None).args(&args).output().unwrap();
        assert_eq!(output.status.code(), Some(1), "{args:?}: {output:?}");
        assert!(output.stderr.is_empty(), "{args:?}: {output:?}");
        let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value["schema"], "jails.command-result.v2", "{value}");
        assert_eq!(value["status"], "refused", "{value}");
        assert_eq!(value["exit_code"], 1, "{value}");
        assert!(
            value["error"]["message"]
                .as_str()
                .is_some_and(|message| message.contains("fix:")),
            "{value}"
        );
    }

    let v1 = jails_cmd(&root, None)
        .args([
            "destroy",
            "scaffold",
            "Task",
            "--force",
            "--pretend",
            "--output",
            "json-v1",
        ])
        .output()
        .unwrap();
    assert_eq!(v1.status.code(), Some(1), "{v1:?}");
    assert!(v1.stderr.is_empty(), "{v1:?}");
    let value: serde_json::Value = serde_json::from_slice(&v1.stdout).unwrap();
    assert_eq!(value["schema"], "jails.command-result.v1", "{value}");
    assert_eq!(value["exit_code"], 1, "{value}");
    assert_eq!(value["status"], "refused", "{value}");
    assert!(value["error"]["message"].is_string(), "{value}");
}

/// plan.md P5.2. Once the schema carries the closed set, adding a constant to
/// the Java enum and stopping leaves a column that refuses a value every
/// other layer accepts.
#[test]
fn widening_an_enum_migrates_every_table_that_stores_it() {
    let root = temp_dir("enum-closed-set-widening");
    write_spring_fixture(&root);
    let migrations = root.join("src/main/resources/db/migration");
    fs::create_dir_all(&migrations).unwrap();

    for args in [
        vec!["g", "enum", "Status", "OPEN", "CLOSED"],
        vec!["g", "scaffold", "Ticket", "id:uuid@pk", "status:Status"],
        // A plain record with the same component: no table, so no migration.
        vec!["g", "record", "Draft", "id:uuid@pk", "status:Status"],
    ] {
        let output = jails_cmd(&root, None).args(args).output().unwrap();
        assert!(output.status.success(), "{output:?}");
    }
    let created = fs::read_to_string(migrations.join("V001__create_tickets.sql")).unwrap();
    assert!(
        created.contains("check (status in ('OPEN', 'CLOSED'))"),
        "{created}"
    );

    let widened = jails_cmd(&root, None)
        .args(["g", "enum", "Status", "OPEN", "CLOSED", "PENDING"])
        .output()
        .unwrap();
    assert!(widened.status.success(), "{widened:?}");
    let migration =
        fs::read_to_string(migrations.join("V002__allow_tickets_status_3.sql")).unwrap();
    assert!(
        migration.contains("drop constraint if exists tickets_status_allowed"),
        "{migration}"
    );
    assert!(
        migration.contains("check (status in ('OPEN', 'CLOSED', 'PENDING'))"),
        "{migration}"
    );
    // One table, not two: `Draft` has no `create_drafts` migration, and an
    // `alter table drafts` would be unappliable everywhere.
    assert!(!migration.contains("drafts"), "{migration}");
    assert_eq!(
        fs::read_dir(&migrations)
            .unwrap()
            .filter(|entry| entry
                .as_ref()
                .is_ok_and(|entry| entry.file_name().to_string_lossy().ends_with(".sql")))
            .count(),
        2
    );

    // Re-running the same declaration is idempotent: nothing changed, so
    // there is nothing to migrate.
    let again = jails_cmd(&root, None)
        .args(["g", "enum", "Status", "OPEN", "CLOSED", "PENDING"])
        .output()
        .unwrap();
    assert!(again.status.success(), "{again:?}");
    assert_eq!(
        fs::read_dir(&migrations)
            .unwrap()
            .filter(|entry| entry
                .as_ref()
                .is_ok_and(|entry| entry.file_name().to_string_lossy().ends_with(".sql")))
            .count(),
        2
    );

    // Dropping one is refused: a stored row may hold it, and jails cannot ask
    // the database from here.
    let before = snapshot_tree(&root);
    let dropped = jails_cmd(&root, None)
        .args(["g", "enum", "Status", "OPEN"])
        .output()
        .unwrap();
    assert!(!dropped.status.success(), "{dropped:?}");
    let stderr = String::from_utf8_lossy(&dropped.stderr);
    assert!(stderr.contains("drops CLOSED, PENDING"), "{stderr}");
    assert!(stderr.contains("fix:"), "{stderr}");
    assert_eq!(snapshot_tree(&root), before, "refusal wrote project files");
}

/// plan.md P3.2. `--column preserve` is the whole reason a field name is a
/// recorded pair rather than a derivation: the Java name moves, the column
/// stays where a live database already has it, and no migration is written
/// because there is nothing for one to run.
#[test]
fn preserving_a_column_renames_the_component_and_writes_no_migration() {
    let root = temp_dir("resource-field-column-preserve");
    write_spring_fixture(&root);
    let migrations = root.join("src/main/resources/db/migration");
    fs::create_dir_all(&migrations).unwrap();
    let scaffold = jails_cmd(&root, None)
        .args(["g", "scaffold", "Account", "id:uuid@pk", "userId:uuid"])
        .output()
        .unwrap();
    assert!(scaffold.status.success(), "{scaffold:?}");
    let before = fs::read_dir(&migrations).unwrap().count();

    let renamed = jails_cmd(&root, None)
        .args([
            "resource", "field", "rename", "Account", "userId", "ownerId", "--column", "preserve",
        ])
        .output()
        .unwrap();
    assert!(
        renamed.status.success(),
        "{}{}",
        String::from_utf8_lossy(&renamed.stdout),
        String::from_utf8_lossy(&renamed.stderr)
    );
    assert_eq!(
        fs::read_dir(&migrations).unwrap().count(),
        before,
        "preserve wrote a migration"
    );

    let record =
        fs::read_to_string(root.join("src/main/java/com/example/demo/domain/Account.java"))
            .unwrap();
    assert!(record.contains("UUID ownerId"), "{record}");
    assert!(!record.contains("userId"), "{record}");

    // The SQL half did not move, and that is the point: the adapter still
    // reads and writes `user_id` while the Java component is `ownerId`.
    let adapter = fs::read_to_string(
        root.join("src/main/java/com/example/demo/adapters/JdbcAccountRepository.java"),
    )
    .unwrap();
    assert!(adapter.contains("user_id"), "{adapter}");
    assert!(!adapter.contains("owner_id"), "{adapter}");

    // And the binding is recorded, so the next command derives nothing.
    let store = jails_commit::store::Store::at(&root)
        .observe()
        .unwrap()
        .ledger
        .unwrap();
    let recorded = store
        .models()
        .into_iter()
        .flat_map(|(_, fields)| fields)
        .collect::<Vec<_>>();
    assert!(
        recorded
            .iter()
            .any(|field| field == "ownerId:uuid@column(user_id)"),
        "{recorded:?}"
    );

    // A second command re-plans from that recorded token, so a binding that
    // did not survive the round trip would show up as a moved column here.
    let added = jails_cmd(&root, None)
        .args(["resource", "field", "add", "Account", "note:string?"])
        .output()
        .unwrap();
    assert!(
        added.status.success(),
        "{}{}",
        String::from_utf8_lossy(&added.stdout),
        String::from_utf8_lossy(&added.stderr)
    );
    let adapter = fs::read_to_string(
        root.join("src/main/java/com/example/demo/adapters/JdbcAccountRepository.java"),
    )
    .unwrap();
    assert!(adapter.contains("user_id"), "{adapter}");
    assert!(!adapter.contains("owner_id"), "{adapter}");
}

#[test]
fn resource_field_uses_scaffold_storage_identity_and_leaves_plain_records_source_only() {
    let root = temp_dir("resource-field-storage-identity");
    write_spring_fixture(&root);
    let migrations = root.join("src/main/resources/db/migration");
    fs::create_dir_all(&migrations).unwrap();
    let scaffold = jails_cmd(&root, None)
        .args(["g", "scaffold", "Customer", "id:uuid@pk", "phoen:string?"])
        .output()
        .unwrap();
    assert!(scaffold.status.success(), "{scaffold:?}");

    let renamed = jails_cmd(&root, None)
        .args([
            "resource",
            "field",
            "rename",
            "Customer",
            "phoen",
            "phone",
            "--column",
            "single-cutover",
        ])
        .output()
        .unwrap();
    assert!(renamed.status.success(), "{renamed:?}");
    assert!(
        migrations.join("V002__rename_phoen_to_phone.sql").is_file(),
        "{renamed:?}"
    );

    // The same command on a resource with no table renames the component and
    // appends nothing. It used to derive `tags` from the entity name and write
    // `alter table tags` into a project that has never created that table --
    // unappliable everywhere, and invisible to `doctor`, because a migration
    // written this way is not recorded output.
    let record = jails_cmd(&root, None)
        .args(["g", "record", "Tag", "id:uuid@pk", "label:string?"])
        .output()
        .unwrap();
    assert!(record.status.success(), "{record:?}");
    let before = fs::read_dir(&migrations).unwrap().count();
    let renamed = jails_cmd(&root, None)
        .args([
            "resource",
            "field",
            "rename",
            "Tag",
            "label",
            "name",
            "--column",
            "single-cutover",
        ])
        .output()
        .unwrap();
    assert!(renamed.status.success(), "{renamed:?}");
    assert_eq!(fs::read_dir(&migrations).unwrap().count(), before);
    let tag =
        fs::read_to_string(root.join("src/main/java/com/example/demo/domain/Tag.java")).unwrap();
    assert!(tag.contains("Optional<String> name"), "{tag}");
    assert!(!tag.contains("label"), "{tag}");

    // And the data-plan flags are refused by name rather than silently
    // planning an update against a table that is not there.
    let refused = jails_cmd(&root, None)
        .args([
            "resource",
            "field",
            "add",
            "Tag",
            "createdAt:instant",
            "--default-literal",
            "2026-08-25T12:00:00Z",
        ])
        .output()
        .unwrap();
    assert_eq!(refused.status.code(), Some(1), "{refused:?}");
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(stderr.contains("--default-literal"), "{stderr}");
    assert!(stderr.contains("record"), "{stderr}");
    assert!(stderr.contains("fix:"), "{stderr}");
}

#[test]
fn package_overrides_normalize_the_base_and_unique_names_resolve_without_the_flag() {
    let root = temp_dir("package-override-resolution");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join("src/main/resources/db/migration")).unwrap();
    let generated = jails_cmd(&root, None)
        .args([
            "g",
            "scaffold",
            "Invoice",
            "id:uuid@pk",
            "amount:decimal",
            "--package",
            "com.example.demo.billing",
        ])
        .output()
        .unwrap();
    assert!(generated.status.success(), "{generated:?}");
    let record = root.join("src/main/java/com/example/demo/billing/Invoice.java");
    assert!(record.is_file(), "missing {}", record.display());
    assert!(
        !root
            .join("src/main/java/com/example/demo/com/example/demo/billing")
            .exists()
    );

    let evolved = jails_cmd(&root, None)
        .args(["g", "field", "Invoice", "memo:string?"])
        .output()
        .unwrap();
    assert!(evolved.status.success(), "{evolved:?}");
    assert!(
        fs::read_to_string(&record).unwrap().contains("memo"),
        "{}",
        record.display()
    );
}

/// Adding a field regenerates everything that *constructs* the resource.
///
/// `g field Order memo:string?` used to update `Order`'s own ten surfaces and
/// silently leave every `--on Order` companion calling the old constructor.
/// The operation list named none of them, `doctor` reported `all clear`
/// because each file was byte-identical to what jails wrote, and only `javac`
/// found it -- the single most common change there is, breaking the build of
/// any project that had generated a query, transition or use case.
///
/// Refusing instead, which is what the first attempt did, is worse in a
/// different way: one generated query would make "this entity needs one more
/// column" permanently impossible.
#[test]
fn a_field_regenerates_the_companions_that_construct_the_resource() {
    let root = temp_dir("field-stale-strategy-companions");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join("src/main/resources/db/migration")).unwrap();
    for command in [
        vec![
            "g",
            "scaffold",
            "Order",
            "id:uuid@pk",
            "total:decimal",
            "status:string",
            "version:long@nonnegative",
        ],
        vec!["g", "query", "FindOrders", "total:decimal", "--on", "Order"],
        vec![
            "g",
            "transition",
            "ShipOrder",
            "id:uuid",
            "status:string",
            "version:long@nonnegative",
            "--on",
            "Order",
        ],
        vec![
            "g",
            "usecase",
            "PlaceOrder",
            "total:decimal",
            "status:string",
            "--on",
            "Order",
        ],
    ] {
        let output = jails_cmd(&root, None).args(&command).output().unwrap();
        assert!(output.status.success(), "{command:?}: {output:?}");
    }

    let evolved = jails_cmd(&root, None)
        .args(["g", "field", "Order", "memo:string?"])
        .output()
        .unwrap();
    assert!(
        evolved.status.success(),
        "{}{}",
        String::from_utf8_lossy(&evolved.stdout),
        String::from_utf8_lossy(&evolved.stderr)
    );
    let plan = String::from_utf8_lossy(&evolved.stdout);

    // Named in the operation list, not silently touched and not silently
    // skipped. Each one constructs `Order`, so each one had to move.
    let companions = [
        "src/main/java/com/example/demo/adapters/JdbcFindOrdersQuery.java",
        "src/main/java/com/example/demo/adapters/JdbcShipOrderTransition.java",
        "src/main/java/com/example/demo/service/StoringPlaceOrderUseCase.java",
    ];
    for companion in companions {
        assert!(
            plan.contains(companion),
            "{companion} missing from:\n{plan}"
        );
    }

    // And the regenerated bytes carry the *new* component list. These
    // generators read the target's components back out of `Order.java`, so
    // this only holds if they planned against the bytes this same transition
    // wrote rather than the ones that were on disk.
    let query = fs::read_to_string(root.join(companions[0])).unwrap();
    assert!(query.contains("new Order("), "{query}");
    assert!(query.contains("rows.getString(\"memo\")"), "{query}");
    for companion in &companions[1..] {
        let source = fs::read_to_string(root.join(companion)).unwrap();
        assert!(source.contains("new Order("), "{companion}:\n{source}");
    }
}

/// The same rule, for the two companions `--on` alone did not reach.
///
/// An `association` names its parent with `--yields` and its child with
/// `--on`, and a `durable-job` names the resource it produces with
/// `--yields` -- so a match on `--on` only left both of them planning against
/// the component list the resource had before the evolution. Both read it off
/// `<Name>.java`: the association's probe builds a row from the child's
/// columns, and the job's store maps one back. modern.md §11.1, plan.md P7.1.
#[test]
fn a_field_reaches_the_companions_named_by_yields_as_well_as_on() {
    let root = temp_dir("field-stale-yields-companions");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join("src/main/resources/db/migration")).unwrap();
    for command in [
        vec!["add", "db", "--no-start"],
        vec![
            "g",
            "scaffold",
            "Owner",
            "id:uuid@pk",
            "name:string!",
            "createdAt:instant",
        ],
        vec![
            "g",
            "scaffold",
            "Item",
            "id:uuid@pk",
            "ownerId:uuid@index",
            "name:string!",
            "createdAt:instant",
        ],
        vec![
            "g",
            "association",
            "ItemOwner",
            "ownerId=id",
            "--on",
            "Item",
            "--yields",
            "Owner",
        ],
        vec![
            "g",
            "usecase",
            "AddItem",
            "id:uuid",
            "ownerId:uuid",
            "name:string!",
            "--on",
            "Item",
        ],
        vec![
            "g",
            "durable-job",
            "ItemDispatcher",
            "id:uuid",
            "ownerId:uuid",
            "name:string!",
            "--on",
            "AddItem",
            "--yields",
            "Item",
        ],
    ] {
        let output = jails_cmd(&root, None).args(&command).output().unwrap();
        assert!(output.status.success(), "{command:?}: {output:?}");
    }

    let evolved = jails_cmd(&root, None)
        .args(["g", "field", "Item", "memo:string?"])
        .output()
        .unwrap();
    assert!(
        evolved.status.success(),
        "{}{}",
        String::from_utf8_lossy(&evolved.stdout),
        String::from_utf8_lossy(&evolved.stderr)
    );
    let plan = String::from_utf8_lossy(&evolved.stdout);

    // The association's probe builds a row out of the child's column list, so
    // it goes stale the moment the child gains one. `--on Item` named it all
    // along; what was missing is that `association` was not in the set of
    // recipes a field evolution re-plans.
    let probe = "src/test/java/com/example/demo/adapters/ItemOwnerAssociationIT.java";
    assert!(plan.contains(probe), "{probe} missing from:\n{plan}");
    let source = fs::read_to_string(root.join(probe)).unwrap();
    assert!(source.contains("memo"), "{probe}:\n{source}");

    // The evolution owns the schema change. A regenerated companion must not
    // re-emit the `create table` its first generation already applied.
    let migrations: Vec<String> = fs::read_dir(root.join("src/main/resources/db/migration"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    assert_eq!(
        migrations
            .iter()
            .filter(|name| name.contains("item"))
            .count(),
        migrations
            .iter()
            .filter(|name| name.contains("item"))
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        "{migrations:?}"
    );

    // The parent side, reached through `--yields`. The association re-plans
    // from a `child=parent` *mapping* rather than a field list, and reading
    // its arguments as fields refused the whole evolution with "association
    // ItemOwner needs at least one `childField=parentField` mapping" -- in the
    // middle of a `g field` that had nothing to do with it.
    let parent = jails_cmd(&root, None)
        .args(["g", "field", "Owner", "nickname:string?"])
        .output()
        .unwrap();
    assert!(
        parent.status.success(),
        "{}{}",
        String::from_utf8_lossy(&parent.stdout),
        String::from_utf8_lossy(&parent.stderr)
    );
}

/// An index on a table that already exists.
///
/// `--index` and `@index` are both creation-time and there was nothing
/// afterwards -- `missing.md` M9, measured against a real project whose third
/// migration is exactly `addIndex('messages', ['customer_id'])`. `g field` can
/// already add a *column* to a live table, which is the harder problem: an
/// index has no data plan to argue about.
#[test]
fn an_index_can_be_added_to_a_table_that_already_exists() {
    let root = temp_dir("resource-index-add");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join("src/main/resources/db/migration")).unwrap();
    let scaffold = jails_cmd(&root, None)
        .args([
            "g",
            "scaffold",
            "Message",
            "id:uuid@pk",
            "customerId:uuid",
            "body:string!",
            "createdAt:instant",
        ])
        .output()
        .unwrap();
    assert!(scaffold.status.success(), "{scaffold:?}");

    let added = jails_cmd(&root, None)
        .args([
            "resource",
            "index",
            "add",
            "Message",
            "customer_id, created_at desc",
        ])
        .output()
        .unwrap();
    assert!(
        added.status.success(),
        "{}{}",
        String::from_utf8_lossy(&added.stdout),
        String::from_utf8_lossy(&added.stderr)
    );

    let migration = fs::read_dir(root.join("src/main/resources/db/migration"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| path.to_string_lossy().contains("index_messages"))
        .expect("the index migration was not written");
    let sql = fs::read_to_string(&migration).unwrap();
    // The columns the table has, not the components jails records.
    assert!(
        sql.contains("on messages (customer_id, created_at desc)"),
        "{sql}"
    );

    // Recorded, so a re-plan reproduces it -- and so a second attempt at the
    // same index is refused rather than writing a duplicate migration.
    let again = jails_cmd(&root, None)
        .args([
            "resource",
            "index",
            "add",
            "Message",
            "customer_id, created_at desc",
        ])
        .output()
        .unwrap();
    assert!(!again.status.success());
    assert!(
        String::from_utf8_lossy(&again.stderr).contains("already declares an index"),
        "{}",
        String::from_utf8_lossy(&again.stderr)
    );

    // A typo fails here rather than at `flyway migrate` on whichever machine
    // runs it first.
    let typo = jails_cmd(&root, None)
        .args(["resource", "index", "add", "Message", "custmoer_id"])
        .output()
        .unwrap();
    assert!(!typo.status.success());
    assert!(
        String::from_utf8_lossy(&typo.stderr).contains("not a declared field"),
        "{}",
        String::from_utf8_lossy(&typo.stderr)
    );
}

/// A route the caller names, because the URLs are somebody else's contract.
///
/// `missing.md` M8: the ported originals answer `/customer_api/ping`,
/// `/admin_api/issues`, `/api/conversations/`, and none of those is derivable
/// from any name jails would accept for the class. Derived paths stay the
/// default -- they are a virtue greenfield -- and `--path` is how a port meets
/// a fixed contract.
#[test]
fn a_named_route_replaces_the_derived_one_everywhere_it_appears() {
    let root = temp_dir("named-route");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join("src/main/resources/db/migration")).unwrap();
    for command in [
        vec!["add", "db", "--no-start"],
        vec![
            "g",
            "scaffold",
            "Ping",
            "id:uuid@pk",
            "email:string!",
            "createdAt:instant",
        ],
        vec![
            "g",
            "usecase",
            "RecordPing",
            "email:string!",
            "--on",
            "Ping",
            "--path",
            "/customer_api/ping",
        ],
        vec![
            "g",
            "query",
            "PingsByEmail",
            "email:string!",
            "--on",
            "Ping",
            "--path",
            "/customer_api/read",
        ],
        vec!["g", "controller", "Bar", "--path", "/bar"],
    ] {
        let output = jails_cmd(&root, None).args(&command).output().unwrap();
        assert!(
            output.status.success(),
            "{command:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    for (file, expected) in [
        (
            "src/main/java/com/example/demo/web/RecordPingController.java",
            "\"/customer_api/ping\"",
        ),
        (
            "src/main/java/com/example/demo/web/PingsByEmailQueryController.java",
            "\"/customer_api/read\"",
        ),
        (
            "src/main/java/com/example/demo/web/BarController.java",
            "\"/bar\"",
        ),
    ] {
        let source = fs::read_to_string(root.join(file)).unwrap();
        assert!(source.contains(expected), "{file}:\n{source}");
        assert!(!source.contains("/actions/"), "{file}:\n{source}");
        assert!(!source.contains("/queries/"), "{file}:\n{source}");
    }

    // A path that is not one is refused rather than written into an
    // annotation: this is text jails puts in a Java file.
    for (bad, expected) in [
        ("customer_api/ping", "does not start with"),
        ("/customer api", "contains ` `"),
        ("/api/../secret", "contains `..`"),
    ] {
        let refused = jails_cmd(&root, None)
            .args(["g", "controller", "Other", "--path", bad])
            .output()
            .unwrap();
        assert!(!refused.status.success(), "{bad} was accepted");
        assert!(
            String::from_utf8_lossy(&refused.stderr).contains(expected),
            "{bad}: {}",
            String::from_utf8_lossy(&refused.stderr)
        );
    }

    // And a kind that answers no single route says so instead of ignoring it.
    let wrong = jails_cmd(&root, None)
        .args(["g", "record", "Thing", "id:uuid", "--path", "/thing"])
        .output()
        .unwrap();
    assert!(!wrong.status.success());
    assert!(
        String::from_utf8_lossy(&wrong.stderr).contains("`--path` applies to"),
        "{}",
        String::from_utf8_lossy(&wrong.stderr)
    );
}

/// What `jails explain query` says about a filter is what `g query` does.
///
/// `bugs.md` B51: the explanation said "required scalar equality filters only"
/// long after optional filters shipped, because `every_kind_has_an_explanation`
/// checks that a kind *has* a row and nothing checks the row is still true.
/// A rationale cannot be derived -- that is why the table is hand-written --
/// but a claim about a shape jails emits can be checked against the shape, and
/// the two claims that drifted are pinned here.
#[test]
fn what_explain_says_about_a_query_is_what_a_query_does() {
    let root = temp_dir("explain-agrees-with-query");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join("src/main/resources/db/migration")).unwrap();

    let explained = jails_cmd(&root, None)
        .args(["explain", "query"])
        .output()
        .unwrap();
    assert!(explained.status.success());
    let explained = String::from_utf8_lossy(&explained.stdout).to_string();

    assert!(
        jails_cmd(&root, None)
            .args([
                "g",
                "scaffold",
                "Loan",
                "id:uuid@pk",
                "memberId:uuid",
                "settled:boolean",
            ])
            .output()
            .unwrap()
            .status
            .success()
    );
    assert!(
        jails_cmd(&root, None)
            .args([
                "g",
                "query",
                "OpenLoans",
                "--on",
                "Loan",
                "settled:boolean?"
            ])
            .output()
            .unwrap()
            .status
            .success()
    );
    let adapter = fs::read_to_string(
        root.join("src/main/java/com/example/demo/adapters/JdbcOpenLoansQuery.java"),
    )
    .unwrap();

    // An optional filter widens rather than matching null, and the
    // explanation says so instead of saying filters must be required.
    assert!(
        adapter.contains("(cast(:settled as boolean) is null or settled = :settled)"),
        "{adapter}"
    );
    assert!(explained.contains("is null or"), "{explained}");

    // The cap the caller cannot see from the response is stated where they
    // can see it.
    let cap = adapter
        .lines()
        .find(|line| line.contains("MAX_RESULTS ="))
        .expect("the adapter bounds its result");
    let cap = cap
        .rsplit('=')
        .next()
        .and_then(|rest| rest.trim().trim_end_matches(';').parse::<usize>().ok())
        .expect("a numeric bound");
    assert!(
        explained.contains(&cap.to_string()),
        "explain does not name the {cap}-row cap the adapter applies:\n{explained}"
    );
}

/// A verb a recipe derives is not also a verb a flag can set.
///
/// `bugs.md` B49: `g query X --method post` emitted `@GetMapping` and said
/// nothing. A query's verb follows its request -- GET when every filter comes
/// from `--path`, POST when it carries a body -- so `--method` there is not a
/// preference jails declines to honour, it is a claim about the request that
/// contradicts the request. That is `missing.md` M7's shape, which this module
/// already refuses for `--path`, `--via` and `--consumes`.
#[test]
fn a_verb_a_recipe_derives_is_not_one_a_flag_can_set() {
    let root = temp_dir("method-derived-not-set");
    write_spring_fixture(&root);

    // The two recipes that name a verb keep taking it.
    for kind in ["controller", "client"] {
        let named = jails_cmd(&root, None)
            .args(["g", kind, "Verify", "--method", "post"])
            .output()
            .unwrap();
        assert!(
            named.status.success(),
            "g {kind} --method post: {}",
            String::from_utf8_lossy(&named.stderr)
        );
    }

    // Every other recipe says so rather than generating a different verb.
    let refused = jails_cmd(&root, None)
        .args(["g", "record", "Thing", "id:uuid", "--method", "post"])
        .output()
        .unwrap();
    assert!(
        !refused.status.success(),
        "`--method` on a record was accepted"
    );
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(stderr.contains("`--method` applies to"), "{stderr}");

    // And the three whose verb is derived say what it is derived from, so the
    // refusal answers the question that produced the flag.
    let derived = jails_cmd(&root, None)
        .args([
            "g", "query", "Thing", "--on", "Loan", "id:uuid", "--method", "post",
        ])
        .output()
        .unwrap();
    assert!(!derived.status.success());
    let stderr = String::from_utf8_lossy(&derived.stderr);
    assert!(stderr.contains("verb follows its request"), "{stderr}");
}

/// A second client does not take the first one's configuration with it.
///
/// `missing.md` M13: `@ImportHttpServices` carries one group name, and one
/// shared `HttpClientsConfig` scanned by package meant the newest client
/// always worked and every older one was broken -- silently at generate time,
/// and visibly only as the older client's own test calling
/// `https://example.invalid`. One config class per client, listed by type,
/// makes it additive by construction.
#[test]
fn a_second_client_keeps_the_first_one_registered() {
    let root = temp_dir("clients-are-additive");
    write_spring_fixture(&root);
    for name in ["Alpha", "Beta"] {
        let output = jails_cmd(&root, None)
            .args(["g", "client", name])
            .output()
            .unwrap();
        assert!(output.status.success(), "{name}: {output:?}");
    }

    assert!(
        !root
            .join("src/main/java/com/example/demo/clients/HttpClientsConfig.java")
            .exists(),
        "the shared registration is what made this break"
    );
    for (name, group) in [("Alpha", "alpha"), ("Beta", "beta")] {
        let config = fs::read_to_string(root.join(format!(
            "src/main/java/com/example/demo/clients/{name}ClientConfig.java"
        )))
        .unwrap();
        assert!(config.contains(&format!("group = \"{group}\"")), "{config}");
        assert!(
            config.contains(&format!("types = {name}Client.class")),
            "{config}"
        );
    }
}

#[test]
fn generate_scaffold_writes_a_raw_jdbc_slice() {
    let root = temp_dir("scaffold-files");
    write_spring_fixture(&root);

    let status = jails_cmd(&root, None)
        .args([
            "generate",
            "scaffold",
            "Post",
            "id:uuid@pk",
            "title:string",
            "body:text",
        ])
        .status()
        .unwrap();
    assert!(status.success());

    let pkg = root.join("src/main/java/com/example/demo");
    assert!(pkg.join("domain/Post.java").is_file());
    assert!(pkg.join("app/PostRepository.java").is_file());
    assert!(pkg.join("adapters/JdbcPostRepository.java").is_file());
    assert!(pkg.join("service/PostService.java").is_file());
    assert!(pkg.join("web/PostController.java").is_file());
    assert!(
        root.join("src/test/java/com/example/demo/adapters/JdbcPostRepositoryIT.java")
            .is_file()
    );
    assert!(
        root.join("src/test/java/com/example/demo/web/PostControllerTest.java")
            .is_file()
    );
}

#[test]
fn scaffold_refuses_an_implicit_or_composite_identity_before_writing() {
    let root = temp_dir("scaffold-primary-key-contract");
    write_spring_fixture(&root);

    for (name, fields, expected) in [
        (
            "Book",
            vec!["title:string!", "author:string"],
            "needs exactly one `@pk` field",
        ),
        (
            "OrgMember",
            vec!["orgId:uuid@pk", "userId:uuid@pk", "role:string"],
            "composite primary key",
        ),
    ] {
        let before = snapshot_tree(&root);
        let output = jails_cmd(&root, None)
            .args(
                ["generate", "scaffold", name]
                    .into_iter()
                    .chain(fields)
                    .collect::<Vec<_>>(),
            )
            .output()
            .unwrap();
        assert!(!output.status.success(), "{name} unexpectedly succeeded");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains(expected), "{name}: {stderr}");
        assert!(stderr.contains("fix:"), "{name}: {stderr}");
        assert_eq!(snapshot_tree(&root), before, "{name} refusal wrote files");
    }
}

/// plan.md P3.1. These pairs used to be two fields sharing one column; they
/// are now one field spelled twice, which is why the refusal names a single
/// Java component and a single column rather than two of each.
#[test]
fn field_names_that_collapse_to_one_sql_column_refuse_before_writing() {
    let root = temp_dir("scaffold-column-collision");
    write_spring_fixture(&root);

    for (name, fields, java, column) in [
        ("Weird", ["id:uuid@pk", "Id:string"], "id", "id"),
        (
            "Pair",
            ["userId:uuid@pk", "user_id:string"],
            "userId",
            "user_id",
        ),
    ] {
        let before = snapshot_tree(&root);
        let output = jails_cmd(&root, None)
            .args(
                ["generate", "scaffold", name]
                    .into_iter()
                    .chain(fields)
                    .collect::<Vec<_>>(),
            )
            .output()
            .unwrap();
        assert!(!output.status.success(), "{name} unexpectedly succeeded");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains(java), "{name}: {stderr}");
        assert!(stderr.contains(column), "{name}: {stderr}");
        assert!(stderr.contains("declared twice"), "{name}: {stderr}");
        assert!(stderr.contains("fix:"), "{name}: {stderr}");
        assert_eq!(snapshot_tree(&root), before, "{name} refusal wrote files");
    }
}

/// The other half of the same convergence: one spelling in, the other out.
/// A snake_case declaration produces a lowerCamelCase Java component and a
/// snake_case column, and a camelCase one produces exactly the same pair.
#[test]
fn a_snake_case_field_declaration_produces_a_camel_case_java_component() {
    let root = temp_dir("field-name-convergence");
    write_spring_fixture(&root);

    for (name, field) in [("Snake", "user_id:uuid"), ("Camel", "userId:uuid")] {
        let output = jails_cmd(&root, None)
            .args(["generate", "record", name, "id:uuid@pk", field])
            .output()
            .unwrap();
        assert!(output.status.success(), "{name}: {output:?}");
        let record = std::fs::read_to_string(
            root.join("src/main/java/com/example/demo/domain")
                .join(format!("{name}.java")),
        )
        .unwrap();
        assert!(record.contains("UUID userId"), "{name}: {record}");
        assert!(!record.contains("user_id"), "{name}: {record}");
    }
}

#[test]
fn object_method_field_names_refuse_before_writing_but_record_is_allowed() {
    let root = temp_dir("record-component-name");
    write_project_skeleton(&root);

    for field in ["hashCode:string", "toString:string", "equals:string"] {
        let before = snapshot_tree(&root);
        let output = jails_cmd(&root, None)
            .args(["generate", "record", "Box", field])
            .output()
            .unwrap();
        assert!(!output.status.success(), "{field} unexpectedly succeeded");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(field.split(':').next().unwrap()),
            "{stderr}"
        );
        assert!(stderr.contains("java.lang.Object"), "{stderr}");
        assert!(stderr.contains("fix:"), "{stderr}");
        assert_eq!(snapshot_tree(&root), before, "{field} refusal wrote files");
    }

    let output = jails_cmd(&root, None)
        .args(["generate", "record", "Allowed", "record:string"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn plain_project_scaffold_refuses_without_writing_uncompilable_spring_java() {
    let root = temp_dir("scaffold-plain-refusal");
    write_plain_fixture(&root);
    let before = snapshot_tree(&root);

    let output = jails_cmd(&root, None)
        .args([
            "generate",
            "scaffold",
            "Note",
            "id:uuid@pk",
            "title:string!",
        ])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("`scaffold` is a Spring Boot capability"),
        "{stderr}"
    );
    assert_eq!(
        snapshot_tree(&root),
        before,
        "a refused scaffold wrote project or machine state"
    );
}

#[test]
fn prepared_diff_and_ast_show_create_replace_and_three_way_without_writing() {
    let root = temp_dir("prepared-review");
    write_spring_fixture(&root);
    let generated = jails_cmd(&root, None)
        .args(["g", "record", "Note", "title:string!"])
        .output()
        .unwrap();
    assert!(generated.status.success(), "{generated:?}");

    let test = root.join("src/test/java/com/example/demo/domain/NoteTest.java");
    let reader_edit = format!(
        "// reader-owned context\n{}",
        fs::read_to_string(&test).unwrap()
    );
    fs::write(&test, reader_edit).unwrap();
    let before = snapshot_tree(&root);

    let changed = jails_cmd(&root, None)
        .args([
            "g",
            "field",
            "Note",
            "createdAt:instant",
            "--pretend",
            "--diff",
            "--ast",
        ])
        .output()
        .unwrap();
    assert!(changed.status.success(), "{changed:?}");
    let shown = String::from_utf8_lossy(&changed.stdout);
    assert!(
        shown.contains("diff --jails replace src/main/java/com/example/demo/domain/Note.java"),
        "{shown}"
    );
    assert!(shown.contains("+import java.time.Instant;"), "{shown}");
    assert!(
        shown.contains("@@ -") && shown.contains(" three-way\n"),
        "{shown}"
    );
    assert!(
        shown.contains("MergeFile { path: src/test/java/com/example/demo/domain/NoteTest.java }"),
        "{shown}"
    );
    assert!(
        shown.contains("ReplaceFile { path: src/main/java/com/example/demo/domain/Note.java }"),
        "{shown}"
    );
    assert_eq!(snapshot_tree(&root), before, "review preview wrote files");

    let created = jails_cmd(&root, None)
        .args([
            "g",
            "record",
            "Fresh",
            "id:uuid",
            "--pretend",
            "--diff",
            "--ast",
        ])
        .output()
        .unwrap();
    assert!(created.status.success(), "{created:?}");
    let shown = String::from_utf8_lossy(&created.stdout);
    assert!(shown.contains("--- /dev/null\n+++ b/"), "{shown}");
    assert!(shown.contains("CreateFile { path:"), "{shown}");
    assert!(
        !shown.contains("  timing  "),
        "ordinary human output exposed debug timings: {shown}"
    );
    assert_eq!(snapshot_tree(&root), before, "create preview wrote files");

    let debug = jails_cmd(&root, None)
        .args(["g", "record", "Fresh", "id:uuid", "--pretend", "--debug"])
        .output()
        .unwrap();
    assert!(debug.status.success(), "{debug:?}");
    let debug = String::from_utf8(debug.stdout).unwrap();
    for phase in [
        "discover", "observe", "parse", "project", "prepare", "verify",
    ] {
        assert!(
            debug.contains(&format!("timing  {phase}")),
            "missing {phase} timing in {debug}"
        );
    }
    assert!(!debug.contains("timing  commit"), "{debug}");
    assert_eq!(snapshot_tree(&root), before, "debug preview wrote files");

    let json = jails_cmd(&root, None)
        .args([
            "g",
            "record",
            "Fresh",
            "id:uuid",
            "--pretend",
            "--diff",
            "--ast",
            "--output",
            "json",
        ])
        .output()
        .unwrap();
    assert!(json.status.success(), "{json:?}");
    let json = String::from_utf8(json.stdout).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "jails.command-result.v2", "{json}");
    assert_eq!(value["command"]["path"], serde_json::json!(["generate"]));
    assert!(
        value["report"]["data"]["operation_digest"]
            .as_str()
            .is_some_and(|digest| digest.starts_with("sha256:") && digest.len() == 71),
        "{json}"
    );
    assert!(
        value["report"]["data"]["prepared_after"]
            .as_str()
            .is_some_and(|digest| digest.starts_with("sha256:") && digest.len() == 71),
        "{json}"
    );
    assert!(value["report"]["data"]["diffs"].is_array(), "{json}");
    assert!(value["report"]["data"]["ast"].is_array(), "{json}");
    let timings = value["timings"].as_array().unwrap();
    for phase in [
        "discover", "observe", "parse", "project", "prepare", "verify",
    ] {
        assert!(
            timings.iter().any(|timing| timing["phase"] == phase),
            "missing {phase} timing in {json}"
        );
    }
    assert!(
        !timings.iter().any(|timing| timing["phase"] == "commit"),
        "{json}"
    );
    assert!(
        value["report"]["data"]["diffs"]
            .as_array()
            .unwrap()
            .iter()
            .any(|diff| diff["patch"]
                .as_str()
                .is_some_and(|patch| patch.starts_with("diff --jails create"))),
        "{json}"
    );
    assert!(
        json.contains("src/main/java/com/example/demo/domain/Fresh.java"),
        "{json}"
    );
    assert!(!json.contains("nothing was written"), "{json}");
    assert_eq!(
        snapshot_tree(&root),
        before,
        "JSON review preview wrote files"
    );

    let compatibility = jails_cmd(&root, None)
        .args([
            "g",
            "record",
            "Compatibility",
            "id:uuid",
            "--pretend",
            "--output",
            "json-v1",
        ])
        .output()
        .unwrap();
    assert!(compatibility.status.success(), "{compatibility:?}");
    let compatibility = String::from_utf8(compatibility.stdout).unwrap();
    let compatibility_value: serde_json::Value = serde_json::from_str(&compatibility).unwrap();
    assert_eq!(
        compatibility_value["schema"], "jails.command-result.v1",
        "{compatibility}"
    );
    assert!(
        compatibility_value.get("command").is_none(),
        "{compatibility}"
    );
    assert_eq!(compatibility.matches('\n').count(), 1, "{compatibility}");

    let merged_json = jails_cmd(&root, None)
        .args([
            "g",
            "field",
            "Note",
            "createdAt:instant",
            "--pretend",
            "--output",
            "json",
        ])
        .output()
        .unwrap();
    assert!(merged_json.status.success(), "{merged_json:?}");
    let merged_json = String::from_utf8(merged_json.stdout).unwrap();
    let merged_value: serde_json::Value = serde_json::from_str(&merged_json).unwrap();
    assert!(
        merged_value["timings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|timing| timing["phase"] == "process"),
        "three-way merge process was not timed: {merged_json}"
    );
    assert_eq!(
        snapshot_tree(&root),
        before,
        "timed merge preview wrote files"
    );

    let applied = jails_cmd(&root, None)
        .args(["g", "record", "Fresh", "id:uuid", "--diff", "--ast"])
        .output()
        .unwrap();
    assert!(applied.status.success(), "{applied:?}");
    let applied = String::from_utf8_lossy(&applied.stdout);
    assert!(
        applied.contains("diff --jails create src/main/java/com/example/demo/domain/Fresh.java"),
        "{applied}"
    );
    assert!(applied.contains("CreateFile { path:"), "{applied}");
    assert!(
        root.join("src/main/java/com/example/demo/domain/Fresh.java")
            .is_file(),
        "an applied reviewed transition did not commit"
    );

    let committed = jails_cmd(&root, None)
        .args(["g", "record", "Timed", "id:uuid", "--output", "json"])
        .output()
        .unwrap();
    assert!(committed.status.success(), "{committed:?}");
    let committed = String::from_utf8(committed.stdout).unwrap();
    let committed_value: serde_json::Value = serde_json::from_str(&committed).unwrap();
    for field in ["operation_digest", "prepared_after"] {
        assert!(
            committed_value["receipt"][field]
                .as_str()
                .is_some_and(|digest| digest.starts_with("sha256:") && digest.len() == 71),
            "committed JSON omitted {field}: {committed}"
        );
    }
    assert!(
        committed_value["timings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|timing| timing["phase"] == "commit"),
        "committed JSON omitted commit timing: {committed}"
    );
}

#[test]
fn task_scaffold_cannot_rewrite_or_delete_its_published_v001() {
    let root = temp_dir("task-migration-seal");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join("src/main/resources/db/migration")).unwrap();

    let generated = jails_cmd(&root, None)
        .args([
            "g",
            "scaffold",
            "Task",
            "id:uuid@pk",
            "title:string!",
            "createdAt:instant@index",
        ])
        .output()
        .unwrap();
    assert!(generated.status.success(), "{generated:?}");
    let migration = root.join("src/main/resources/db/migration/V001__create_tasks.sql");
    let sealed = fs::read_to_string(&migration).unwrap();
    let before_resync = snapshot_tree(&root);
    let resync = jails_cmd(&root, None)
        .args([
            "g",
            "scaffold",
            "Task",
            "id:uuid@pk",
            "title:string!",
            "completed:boolean",
        ])
        .output()
        .unwrap();
    assert!(!resync.status.success(), "{resync:?}");
    assert!(
        String::from_utf8_lossy(&resync.stderr).contains("migration-edited-after-seal"),
        "{}",
        String::from_utf8_lossy(&resync.stderr)
    );
    assert_eq!(snapshot_tree(&root), before_resync, "refusal wrote files");

    let before_retirement = snapshot_tree(&root);
    let missing_policy = jails_cmd(&root, None)
        .args(["destroy", "scaffold", "Task", "--force"])
        .output()
        .unwrap();
    assert!(!missing_policy.status.success(), "{missing_policy:?}");
    let missing_policy_stderr = String::from_utf8_lossy(&missing_policy.stderr);
    assert!(
        missing_policy_stderr.contains("storage-policy-required"),
        "{missing_policy_stderr}"
    );
    assert!(
        missing_policy_stderr.contains("--storage preserve"),
        "{missing_policy_stderr}"
    );
    assert!(
        missing_policy_stderr.contains("--storage drop --confirm-table tasks"),
        "{missing_policy_stderr}"
    );
    assert_eq!(
        snapshot_tree(&root),
        before_retirement,
        "refusal wrote files"
    );

    let destroyed = jails_cmd(&root, None)
        .args([
            "destroy",
            "scaffold",
            "Task",
            "--storage",
            "preserve",
            "--force",
        ])
        .output()
        .unwrap();
    assert!(destroyed.status.success(), "{destroyed:?}");
    assert_eq!(fs::read_to_string(&migration).unwrap(), sealed);
    assert!(migration.is_file());
    assert!(
        !root
            .join("src/main/java/com/example/demo/web/TaskController.java")
            .exists()
    );
    let store = jails_commit::store::Store::at(&root)
        .observe()
        .unwrap()
        .ledger
        .unwrap();
    assert!(store.applied.is_empty(), "{:?}", store.applied);
    assert_eq!(store.lifecycles.len(), 1, "{:?}", store.lifecycles);
    let lifecycle = &store.lifecycles[0];
    assert!(matches!(
        lifecycle.state,
        jails_protocol::lifecycle::ResourceState::RetiredPreservingStorage { .. }
    ));
    assert_eq!(lifecycle.table.as_ref().unwrap().table.as_str(), "tasks");
    assert_eq!(lifecycle.migrations.len(), 1);
    assert_eq!(lifecycle.migrations[0].version.get(), 1);
    let entity_before_revive = lifecycle.entity.clone();
    let seal_before_revive = lifecycle.migrations[0].clone();

    let status = jails_cmd(&root, None)
        .args(["resource", "status", "Task", "--output", "json"])
        .output()
        .unwrap();
    assert!(status.status.success(), "{status:?}");
    let status_json = String::from_utf8(status.stdout).unwrap();
    assert!(
        status_json.contains("\"state\":\"retired-storage-present\""),
        "{status_json}"
    );
    assert!(
        status_json.contains("jails resource revive Task --table tasks"),
        "{status_json}"
    );

    let before_wrong_table = snapshot_tree(&root);
    let wrong_table = jails_cmd(&root, None)
        .args(["resource", "revive", "Task", "--table", "task"])
        .output()
        .unwrap();
    assert!(!wrong_table.status.success(), "{wrong_table:?}");
    assert!(
        String::from_utf8_lossy(&wrong_table.stderr).contains("pass `--table tasks` exactly"),
        "{}",
        String::from_utf8_lossy(&wrong_table.stderr)
    );
    assert_eq!(
        snapshot_tree(&root),
        before_wrong_table,
        "refusal wrote files"
    );

    let revived = jails_cmd(&root, None)
        .args(["resource", "revive", "Task", "--table", "tasks"])
        .output()
        .unwrap();
    assert!(revived.status.success(), "{revived:?}");
    assert_eq!(fs::read_to_string(&migration).unwrap(), sealed);
    assert!(
        !root
            .join("src/main/resources/db/migration/V002__create_tasks.sql")
            .exists(),
        "revive published a second create migration"
    );
    assert!(
        root.join("src/main/java/com/example/demo/web/TaskController.java")
            .is_file()
    );
    let revived_store = jails_commit::store::Store::at(&root)
        .observe()
        .unwrap()
        .ledger
        .unwrap();
    let revived_lifecycle = &revived_store.lifecycles[0];
    assert!(matches!(
        revived_lifecycle.state,
        jails_protocol::lifecycle::ResourceState::Active
    ));
    assert_eq!(revived_lifecycle.entity, entity_before_revive);
    assert_eq!(revived_lifecycle.migrations, vec![seal_before_revive]);
    assert_eq!(revived_store.applied.len(), 1);

    let active_status = jails_cmd(&root, None)
        .args(["resource", "status", "Task", "--output", "json"])
        .output()
        .unwrap();
    assert!(active_status.status.success(), "{active_status:?}");
    assert!(
        String::from_utf8_lossy(&active_status.stdout).contains("\"state\":\"consistent\""),
        "{}",
        String::from_utf8_lossy(&active_status.stdout)
    );
}

/// Regenerating a dropped resource revives its lifecycle, so the recovery
/// commands agree about it.
///
/// The recreate appended `V003__create_books.sql` and left the lifecycle at
/// `drop-pending`, which every recovery command then read and refused on:
/// `doctor` named `resource repair`, repair said the resource was retired and
/// named `resource revive`, and revive leaked an instruction meant for whoever
/// was editing the route. A closed loop from an ordinary
/// destroy-then-regenerate.
#[test]
fn regenerating_a_dropped_resource_returns_it_to_a_consistent_lifecycle() {
    let root = temp_dir("recreate-revives-lifecycle");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join("src/main/resources/db/migration")).unwrap();

    for command in [
        vec!["g", "scaffold", "Book", "id:uuid@pk", "title:string"],
        vec![
            "destroy",
            "scaffold",
            "Book",
            "--storage",
            "drop",
            "--confirm-table",
            "books",
            "--force",
        ],
        vec!["g", "scaffold", "Book", "id:uuid@pk", "title:string"],
    ] {
        let output = jails_cmd(&root, None).args(&command).output().unwrap();
        assert!(
            output.status.success(),
            "{command:?}: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // The lineage is create, drop, create -- forward-only, and coherent with
    // the Java that queries the table.
    for migration in [
        "V001__create_books.sql",
        "V002__drop_books.sql",
        "V003__create_books.sql",
    ] {
        assert!(
            root.join("src/main/resources/db/migration")
                .join(migration)
                .is_file(),
            "{migration} is missing"
        );
    }

    let status = jails_cmd(&root, None)
        .args(["resource", "status", "Book"])
        .output()
        .unwrap();
    let status = String::from_utf8_lossy(&status.stdout);
    assert!(status.contains("state: consistent"), "{status}");
    assert!(status.contains("table: books"), "{status}");

    // And the entity can be evolved again, which is what the loop prevented.
    let evolved = jails_cmd(&root, None)
        .args(["g", "field", "Book", "pages:int?"])
        .output()
        .unwrap();
    assert!(
        evolved.status.success(),
        "{}{}",
        String::from_utf8_lossy(&evolved.stdout),
        String::from_utf8_lossy(&evolved.stderr)
    );
    assert!(
        root.join("src/main/resources/db/migration/V004__add_pages_to_books.sql")
            .is_file()
    );
}

/// Renaming a resource carries the storage, or refuses.
///
/// The textual rename carries the Java and nothing else. On a storage-backed
/// entity that is not a partial success but a divergence: the adapter is
/// rewritten to `select ... from readers`, the schema history still creates
/// `members`, and both oracles report health because every file is
/// byte-identical to what jails wrote and every migration applies. Flyway then
/// stops the application on the first query.
#[test]
fn renaming_a_storage_backed_resource_keeps_its_table_or_refuses() {
    let root = temp_dir("legacy-rename-recorded-bases");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join("src/main/resources/db/migration")).unwrap();

    let scaffold = jails_cmd(&root, None)
        .args(["g", "scaffold", "Member", "id:uuid@pk", "name:string!"])
        .output()
        .unwrap();
    assert!(scaffold.status.success(), "{scaffold:?}");
    let create = root.join("src/main/resources/db/migration/V001__create_members.sql");
    let sealed = fs::read(&create).unwrap();

    // The textual rename refuses, and names the command that plans both
    // halves rather than leaving the reader to discover a second verb.
    let refused = jails_cmd(&root, None)
        .args(["rename", "Member", "Reader", "--force"])
        .output()
        .unwrap();
    assert_eq!(refused.status.code(), Some(1), "{refused:?}");
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(stderr.contains("backed by table `members`"), "{stderr}");
    assert!(
        stderr.contains("jails rename resource Member Reader --strategy preserve-table"),
        "{stderr}"
    );

    // The coordinated rename takes a bare name. Demanding `<slice>.<name>`
    // made it unreachable from every imperative project, so the one path that
    // carries the storage could not be run at all.
    let renamed = jails_cmd(&root, None)
        .args([
            "rename",
            "resource",
            "Member",
            "Reader",
            "--strategy",
            "preserve-table",
        ])
        .output()
        .unwrap();
    assert!(
        renamed.status.success(),
        "{}{}",
        String::from_utf8_lossy(&renamed.stdout),
        String::from_utf8_lossy(&renamed.stderr)
    );
    assert_eq!(fs::read(&create).unwrap(), sealed, "V001 is append-only");

    // Coherent afterwards: the adapter still queries the table the migration
    // history creates, and the recorded identity says so.
    let adapter = fs::read_to_string(
        root.join("src/main/java/com/example/demo/adapters/JdbcReaderRepository.java"),
    )
    .unwrap();
    assert!(adapter.contains("from members"), "{adapter}");
    assert!(!adapter.contains("readers"), "{adapter}");
    let status = jails_cmd(&root, None)
        .args(["resource", "status", "Reader"])
        .output()
        .unwrap();
    let status = String::from_utf8_lossy(&status.stdout);
    assert!(status.contains("state: consistent"), "{status}");
    assert!(status.contains("table: members"), "{status}");

    // And the next field evolution migrates the table that is actually there.
    let evolved = jails_cmd(&root, None)
        .args(["g", "field", "Reader", "nickname:string?"])
        .output()
        .unwrap();
    assert!(evolved.status.success(), "{evolved:?}");
    assert!(
        root.join("src/main/resources/db/migration/V002__add_nickname_to_members.sql")
            .is_file(),
        "{}",
        String::from_utf8_lossy(&evolved.stdout)
    );
    assert!(
        fs::read_to_string(root.join("src/main/java/com/example/demo/domain/Reader.java"))
            .unwrap()
            .contains("nickname")
    );

    // A source-only resource has no storage to carry, so the textual rename
    // is still exactly right for it.
    let record = jails_cmd(&root, None)
        .args(["g", "record", "Note", "body:string!"])
        .output()
        .unwrap();
    assert!(record.status.success(), "{record:?}");
    let renamed = jails_cmd(&root, None)
        .args(["rename", "Note", "Memo", "--force"])
        .output()
        .unwrap();
    assert!(renamed.status.success(), "{renamed:?}");
    let destroyed = jails_cmd(&root, None)
        .args(["destroy", "record", "Memo", "--force"])
        .output()
        .unwrap();
    assert!(destroyed.status.success(), "{destroyed:?}");
    assert!(
        !root
            .join("src/main/java/com/example/demo/domain/Memo.java")
            .exists()
    );
}

#[test]
fn coordinated_preserve_table_rename_keeps_storage_and_moves_lifecycle_lineage() {
    let root = temp_dir("resource-rename-preserve-table");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join("src/main/resources/db/migration")).unwrap();

    let generated = jails_cmd(&root, None)
        .args(["g", "scaffold", "Task", "id:uuid@pk", "title:string!"])
        .output()
        .unwrap();
    assert!(generated.status.success(), "{generated:?}");
    let migration = root.join("src/main/resources/db/migration/V001__create_tasks.sql");
    let sealed = fs::read(&migration).unwrap();

    let renamed = jails_cmd(&root, None)
        .args([
            "rename",
            "resource",
            "Billing.Task",
            "WorkItem",
            "--strategy",
            "preserve-table",
            "--force",
        ])
        .output()
        .unwrap();
    assert!(renamed.status.success(), "{renamed:?}");
    let stdout = String::from_utf8_lossy(&renamed.stdout);
    assert!(
        stdout.contains("physical-table-preserved: tasks"),
        "{stdout}"
    );
    assert_eq!(fs::read(&migration).unwrap(), sealed);
    assert!(
        !root
            .join("src/main/resources/db/migration/V002__rename_tasks.sql")
            .exists()
    );
    assert!(
        root.join("src/main/java/com/example/demo/domain/WorkItem.java")
            .is_file()
    );
    assert!(
        !root
            .join("src/main/java/com/example/demo/domain/Task.java")
            .exists()
    );
    let controller =
        fs::read_to_string(root.join("src/main/java/com/example/demo/web/WorkItemController.java"))
            .unwrap();
    assert!(controller.contains("/tasks"), "{controller}");

    let store = jails_commit::store::Store::at(&root)
        .observe()
        .unwrap()
        .ledger
        .unwrap();
    let [lifecycle] = store.lifecycles.as_slice() else {
        panic!("expected one lifecycle: {:?}", store.lifecycles);
    };
    assert_eq!(lifecycle.expected_path.name().as_str(), "WorkItem");
    assert_eq!(lifecycle.table.as_ref().unwrap().table.as_str(), "tasks");
    assert_eq!(lifecycle.migrations.len(), 1);
    let jails_protocol::entity::EntityId::Intent(id) = &lifecycle.entity else {
        panic!("expected direct intent identity: {:?}", lifecycle.entity);
    };
    assert_eq!(id.name.as_str(), "WorkItem");
}

#[test]
fn coordinated_single_cutover_appends_one_migration_and_switches_the_binding() {
    let root = temp_dir("resource-rename-single-cutover");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join("src/main/resources/db/migration")).unwrap();

    let generated = jails_cmd(&root, None)
        .args([
            "g",
            "scaffold",
            "Task",
            "id:uuid@pk",
            "title:string!",
            "createdAt:instant@index",
        ])
        .output()
        .unwrap();
    assert!(generated.status.success(), "{generated:?}");
    let first = root.join("src/main/resources/db/migration/V001__create_tasks.sql");
    let sealed = fs::read(&first).unwrap();

    let renamed = jails_cmd(&root, None)
        .args([
            "rename",
            "resource",
            "Billing.Task",
            "WorkItem",
            "--strategy",
            "single-cutover",
            "--force",
        ])
        .output()
        .unwrap();
    assert!(renamed.status.success(), "{renamed:?}");
    let stdout = String::from_utf8_lossy(&renamed.stdout);
    assert!(
        stdout.contains("physical-table-cutover: tasks -> work_items"),
        "{stdout}"
    );
    assert_eq!(fs::read(first).unwrap(), sealed);
    let cutover = root.join("src/main/resources/db/migration/V002__rename_tasks_to_work_items.sql");
    assert_eq!(
        fs::read_to_string(&cutover).unwrap(),
        concat!(
            "alter table public.\"tasks\" rename to \"work_items\";\n",
            "alter table public.\"work_items\" rename constraint \"tasks_pk\" to \"work_items_pk\";\n",
            "alter index public.\"tasks_created_at_idx\" rename to \"work_items_created_at_idx\";\n",
        )
    );
    let adapter = fs::read_to_string(
        root.join("src/main/java/com/example/demo/adapters/JdbcWorkItemRepository.java"),
    )
    .unwrap();
    assert!(adapter.contains("work_items"), "{adapter}");
    assert!(!adapter.contains("from tasks"), "{adapter}");
    assert!(!adapter.contains("into tasks"), "{adapter}");
    let controller =
        fs::read_to_string(root.join("src/main/java/com/example/demo/web/WorkItemController.java"))
            .unwrap();
    assert!(controller.contains("/tasks"), "{controller}");

    let store = jails_commit::store::Store::at(&root)
        .observe()
        .unwrap()
        .ledger
        .unwrap();
    let [lifecycle] = store.lifecycles.as_slice() else {
        panic!("expected one lifecycle: {:?}", store.lifecycles);
    };
    assert_eq!(lifecycle.expected_path.name().as_str(), "WorkItem");
    assert_eq!(
        lifecycle.table.as_ref().unwrap().table.as_str(),
        "work_items"
    );
    assert_eq!(
        lifecycle
            .migrations
            .iter()
            .map(|seal| seal.version.get())
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
}

#[test]
fn single_cutover_reports_reader_owned_storage_object_names_without_writing() {
    let root = temp_dir("resource-rename-reader-owned-object");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join("src/main/resources/db/migration")).unwrap();
    let generated = jails_cmd(&root, None)
        .args(["g", "scaffold", "Task", "id:uuid@pk", "title:string!"])
        .output()
        .unwrap();
    assert!(generated.status.success(), "{generated:?}");
    fs::write(
        root.join("src/main/resources/db/migration/V002__manual_task_index.sql"),
        "create index tasks_manual_idx on tasks (title);\n",
    )
    .unwrap();
    let before = snapshot_tree(&root);

    let refused = jails_cmd(&root, None)
        .args([
            "rename",
            "resource",
            "Billing.Task",
            "WorkItem",
            "--strategy",
            "single-cutover",
            "--force",
        ])
        .output()
        .unwrap();
    assert!(!refused.status.success(), "{refused:?}");
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(stderr.contains("manual-edit-required"), "{stderr}");
    assert!(stderr.contains("tasks_manual_idx"), "{stderr}");
    assert_eq!(snapshot_tree(&root), before, "refusal wrote project files");
}

#[test]
fn single_cutover_reports_reader_owned_sql_without_writing() {
    let root = temp_dir("resource-rename-manual-sql");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join("src/main/resources/db/migration")).unwrap();
    let generated = jails_cmd(&root, None)
        .args(["g", "scaffold", "Task", "id:uuid@pk", "title:string!"])
        .output()
        .unwrap();
    assert!(generated.status.success(), "{generated:?}");
    let query = root.join("src/main/resources/db/queries/manual.sql");
    fs::create_dir_all(query.parent().unwrap()).unwrap();
    fs::write(&query, "select id from tasks where id = :id;\n").unwrap();
    let before = snapshot_tree(&root);

    let refused = jails_cmd(&root, None)
        .args([
            "rename",
            "resource",
            "Billing.Task",
            "WorkItem",
            "--strategy",
            "single-cutover",
            "--force",
        ])
        .output()
        .unwrap();
    assert!(!refused.status.success(), "{refused:?}");
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(stderr.contains("manual-edit-required"), "{stderr}");
    assert!(stderr.contains("manual.sql"), "{stderr}");
    assert_eq!(snapshot_tree(&root), before, "refusal wrote project files");
}

#[test]
fn single_cutover_refuses_opaque_database_dependencies_without_writing() {
    let root = temp_dir("resource-rename-opaque-database-dependency");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join("src/main/resources/db/migration")).unwrap();
    let generated = jails_cmd(&root, None)
        .args(["g", "scaffold", "Task", "id:uuid@pk", "title:string!"])
        .output()
        .unwrap();
    assert!(generated.status.success(), "{generated:?}");
    fs::write(
        root.join("src/main/resources/db/migration/V002__task_view.sql"),
        "create view public.open_tasks as select id from public.tasks;\n",
    )
    .unwrap();
    let before = snapshot_tree(&root);

    let refused = jails_cmd(&root, None)
        .args([
            "rename",
            "resource",
            "Billing.Task",
            "WorkItem",
            "--strategy",
            "single-cutover",
            "--force",
        ])
        .output()
        .unwrap();
    assert!(!refused.status.success(), "{refused:?}");
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(stderr.contains("opaque-dependency"), "{stderr}");
    assert!(stderr.contains("V002__task_view.sql"), "{stderr}");
    assert_eq!(snapshot_tree(&root), before, "refusal wrote project files");
}

#[test]
fn rolling_rename_waits_for_attestation_then_completes_storage_forward() {
    let root = temp_dir("resource-rename-rolling");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join("src/main/resources/db/migration")).unwrap();
    let generated = jails_cmd(&root, None)
        .args(["g", "scaffold", "Task", "id:uuid@pk", "title:string!"])
        .output()
        .unwrap();
    assert!(generated.status.success(), "{generated:?}");

    let staged = jails_cmd(&root, None)
        .args([
            "rename",
            "resource",
            "Billing.Task",
            "WorkItem",
            "--strategy",
            "rolling",
            "--force",
        ])
        .output()
        .unwrap();
    assert!(staged.status.success(), "{staged:?}");
    let stdout = String::from_utf8_lossy(&staged.stdout);
    let campaign = stdout
        .lines()
        .find_map(|line| line.strip_prefix("rename-campaign: "))
        .expect("rolling rename reports its campaign");
    assert_eq!(campaign.len(), 64, "{stdout}");
    assert!(stdout.contains("--old-version-retired"), "{stdout}");
    assert!(
        !root
            .join("src/main/resources/db/migration/V002__rename_tasks_to_work_items.sql")
            .exists()
    );
    let adapter_path =
        root.join("src/main/java/com/example/demo/adapters/JdbcWorkItemRepository.java");
    let staged_adapter = fs::read_to_string(&adapter_path).unwrap();
    assert!(staged_adapter.contains("from tasks"), "{staged_adapter}");

    let status = jails_cmd(&root, None)
        .args(["resource", "status", "WorkItem", "--output", "json"])
        .output()
        .unwrap();
    assert!(status.status.success(), "{status:?}");
    let status_json = String::from_utf8_lossy(&status.stdout);
    assert!(
        status_json.contains("\"state\":\"rename-pending\""),
        "{status_json}"
    );
    assert!(status_json.contains(campaign), "{status_json}");

    let before_refusal = snapshot_tree(&root);
    let refused = jails_cmd(&root, None)
        .args([
            "rename",
            "storage",
            "Billing.WorkItem",
            "--complete",
            campaign,
            "--force",
        ])
        .output()
        .unwrap();
    assert!(!refused.status.success(), "{refused:?}");
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("old-version-retired"),
        "{refused:?}"
    );
    assert_eq!(snapshot_tree(&root), before_refusal);

    let completed = jails_cmd(&root, None)
        .args([
            "rename",
            "storage",
            "Billing.WorkItem",
            "--complete",
            campaign,
            "--old-version-retired",
            "--force",
        ])
        .output()
        .unwrap();
    assert!(completed.status.success(), "{completed:?}");
    let migration =
        root.join("src/main/resources/db/migration/V002__rename_tasks_to_work_items.sql");
    assert!(migration.is_file());
    let completed_adapter = fs::read_to_string(adapter_path).unwrap();
    assert!(
        completed_adapter.contains("from work_items"),
        "{completed_adapter}"
    );
    assert!(
        !completed_adapter.contains("from tasks"),
        "{completed_adapter}"
    );
    let store = jails_commit::store::Store::at(&root)
        .observe()
        .unwrap()
        .ledger
        .unwrap();
    let [lifecycle] = store.lifecycles.as_slice() else {
        panic!("expected one lifecycle: {:?}", store.lifecycles);
    };
    assert!(matches!(
        lifecycle.state,
        jails_protocol::lifecycle::ResourceState::Active
    ));
    assert_eq!(
        lifecycle.table.as_ref().unwrap().table.as_str(),
        "work_items"
    );
    assert_eq!(lifecycle.migrations.len(), 2);
}

#[test]
fn coordinated_resource_rename_reports_reader_owned_java_without_rewriting_it() {
    let root = temp_dir("resource-rename-manual-java");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join("src/main/resources/db/migration")).unwrap();
    let generated = jails_cmd(&root, None)
        .args(["g", "scaffold", "Task", "id:uuid@pk", "title:string!"])
        .output()
        .unwrap();
    assert!(generated.status.success(), "{generated:?}");
    let manual = root.join("src/main/java/com/example/demo/Manual.java");
    fs::write(
        &manual,
        "package com.example.demo;\nimport com.example.demo.domain.Task;\nfinal class Manual { Task task; }\n",
    )
    .unwrap();
    let before = snapshot_tree(&root);

    let refused = jails_cmd(&root, None)
        .args([
            "rename",
            "resource",
            "Billing.Task",
            "WorkItem",
            "--strategy",
            "preserve-table",
            "--force",
        ])
        .output()
        .unwrap();
    assert!(!refused.status.success(), "{refused:?}");
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(stderr.contains("manual-edit-required"), "{stderr}");
    assert!(stderr.contains("Manual.java"), "{stderr}");
    assert_eq!(snapshot_tree(&root), before, "refusal wrote project files");
    assert!(fs::read_to_string(manual).unwrap().contains("Task task"));
}

#[test]
fn resource_repair_restores_sealed_history_and_missing_owned_projections() {
    let root = temp_dir("resource-repair");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join("src/main/resources/db/migration")).unwrap();
    let generated = jails_cmd(&root, None)
        .args(["g", "scaffold", "Task", "id:uuid@pk", "title:string!"])
        .output()
        .unwrap();
    assert!(generated.status.success(), "{generated:?}");
    let evolved = jails_cmd(&root, None)
        .args(["resource", "field", "add", "Task", "priority:int?"])
        .output()
        .unwrap();
    assert!(evolved.status.success(), "{evolved:?}");

    let migration = root.join("src/main/resources/db/migration/V001__create_tasks.sql");
    let sealed = fs::read(&migration).unwrap();
    fs::write(&migration, b"-- accidentally edited\n").unwrap();
    let controller = root.join("src/main/java/com/example/demo/web/TaskController.java");
    fs::remove_file(&controller).unwrap();

    let repaired = jails_cmd(&root, None)
        .args(["resource", "repair", "Task", "--strategy", "roll-forward"])
        .output()
        .unwrap();
    assert!(
        repaired.status.success(),
        "{}{}",
        String::from_utf8_lossy(&repaired.stdout),
        String::from_utf8_lossy(&repaired.stderr)
    );
    assert_eq!(fs::read(&migration).unwrap(), sealed);
    assert!(
        controller.is_file(),
        "repair did not recreate {controller:?}"
    );

    fs::remove_file(&migration).unwrap();
    let repaired_missing = jails_cmd(&root, None)
        .args(["resource", "repair", "Task", "--strategy", "roll-forward"])
        .output()
        .unwrap();
    assert!(repaired_missing.status.success(), "{repaired_missing:?}");
    assert_eq!(fs::read(&migration).unwrap(), sealed);

    let store = jails_commit::store::Store::at(&root)
        .observe()
        .unwrap()
        .ledger
        .unwrap();
    assert_eq!(store.lifecycles.len(), 1);
    assert_eq!(store.lifecycles[0].migrations.len(), 2);
}

#[test]
fn live_resource_repair_requires_the_applied_flyway_checksum_to_match_the_seal() {
    fn write_psql(bin: &Path, checksum: i32) {
        let ignored_log = bin.join("ignored.log");
        write_fake_maven(bin, &["psql"], &ignored_log);
        fs::write(
            bin.join("psql"),
            format!(
                "#!/bin/sh\ninput=''\nwhile IFS= read -r line; do input=\"${{input}}${{line}}\"; done\ncase \"$input\" in\n  *server_version_num*) printf '170000\\n' ;;\n  *to_regclass*) printf 't\\n' ;;\n  *flyway_schema_history*) printf '1\\t1\\t637265617465207461736b73\\t563030315f5f6372656174655f7461736b732e73716c\\t{checksum}\\tt\\n' ;;\n  *\"FROM observed\"*) printf 'table\\t7075626c6963\\t7461736b73\\t\\t\\t\\t\\t\\t\\t\\t\\n' ;;\n  *) printf '' ;;\nesac\n"
            ),
        )
        .unwrap();
    }

    let root = temp_dir("live-repair-flyway-checksum");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join("src/main/resources/db/migration")).unwrap();
    assert!(
        jails_cmd(&root, None)
            .args(["g", "scaffold", "Task", "id:uuid@pk", "title:string!"])
            .status()
            .unwrap()
            .success()
    );
    let migration = root.join("src/main/resources/db/migration/V001__create_tasks.sql");
    let sealed = fs::read(&migration).unwrap();
    let sealed_checksum = jails_drive::live_sql::flyway_checksum(&sealed).unwrap();
    fs::write(&migration, b"-- locally edited after application\n").unwrap();

    let fake = temp_dir("live-repair-psql-bin");
    write_psql(&fake, sealed_checksum);
    let repaired = jails_cmd(&root, Some(&fake))
        .env(
            "DATABASE_URL",
            "postgresql://app:secret@127.0.0.1:5432/demo",
        )
        .args([
            "resource",
            "repair",
            "Task",
            "--strategy",
            "roll-forward",
            "--datasource",
            "DATABASE_URL",
        ])
        .output()
        .unwrap();
    assert!(
        repaired.status.success(),
        "{}{}",
        String::from_utf8_lossy(&repaired.stdout),
        String::from_utf8_lossy(&repaired.stderr)
    );
    assert_eq!(fs::read(&migration).unwrap(), sealed);

    let edited = b"-- a different image was applied elsewhere\n";
    fs::write(&migration, edited).unwrap();
    write_psql(
        &fake,
        jails_drive::live_sql::flyway_checksum(edited).unwrap(),
    );
    let before = snapshot_tree(&root);
    let refused = jails_cmd(&root, Some(&fake))
        .env(
            "DATABASE_URL",
            "postgresql://app:secret@127.0.0.1:5432/demo",
        )
        .args([
            "resource",
            "repair",
            "Task",
            "--strategy",
            "roll-forward",
            "--datasource",
            "DATABASE_URL",
        ])
        .output()
        .unwrap();
    assert!(!refused.status.success(), "{refused:?}");
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(stderr.contains("flyway-checksum-divergent"), "{stderr}");
    assert!(stderr.contains("will not invoke Flyway repair"), "{stderr}");
    assert_eq!(snapshot_tree(&root), before, "refused repair wrote files");
}

#[test]
fn task_drop_keeps_v001_and_appends_an_exact_forward_migration() {
    let root = temp_dir("task-drop-migration");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join("src/main/resources/db/migration")).unwrap();
    assert!(
        jails_cmd(&root, None)
            .args(["g", "scaffold", "Task", "id:uuid@pk", "title:string!"])
            .status()
            .unwrap()
            .success()
    );

    let create = root.join("src/main/resources/db/migration/V001__create_tasks.sql");
    let sealed = fs::read_to_string(&create).unwrap();
    let before_wrong_confirmation = snapshot_tree(&root);
    let wrong = jails_cmd(&root, None)
        .args([
            "destroy",
            "scaffold",
            "Task",
            "--storage",
            "drop",
            "--confirm-table",
            "task",
            "--force",
        ])
        .output()
        .unwrap();
    assert!(!wrong.status.success(), "{wrong:?}");
    let wrong_stderr = String::from_utf8_lossy(&wrong.stderr);
    assert!(wrong_stderr.contains("is not `tasks`"), "{wrong_stderr}");
    assert_eq!(
        snapshot_tree(&root),
        before_wrong_confirmation,
        "wrong confirmation wrote files"
    );

    let dropped = jails_cmd(&root, None)
        .args([
            "destroy",
            "scaffold",
            "Task",
            "--storage",
            "drop",
            "--confirm-table",
            "tasks",
            "--force",
        ])
        .output()
        .unwrap();
    assert!(dropped.status.success(), "{dropped:?}");
    assert_eq!(fs::read_to_string(&create).unwrap(), sealed);
    assert_eq!(
        fs::read_to_string(root.join("src/main/resources/db/migration/V002__drop_tasks.sql"))
            .unwrap(),
        "drop table tasks;\n"
    );
    assert!(
        !root
            .join("src/main/java/com/example/demo/web/TaskController.java")
            .exists()
    );
    let store = jails_commit::store::Store::at(&root)
        .observe()
        .unwrap()
        .ledger
        .unwrap();
    assert_eq!(store.lifecycles.len(), 1, "{:?}", store.lifecycles);
    let lifecycle = &store.lifecycles[0];
    assert!(matches!(
        &lifecycle.state,
        jails_protocol::lifecycle::ResourceState::RetiredDropPlanned { migration, .. }
            if migration.as_str() == "src/main/resources/db/migration/V002__drop_tasks.sql"
    ));
    assert_eq!(
        lifecycle
            .migrations
            .iter()
            .map(|seal| seal.version.get())
            .collect::<Vec<_>>(),
        vec![1, 2]
    );

    let before_revive = snapshot_tree(&root);
    let revive = jails_cmd(&root, None)
        .args(["resource", "revive", "Task", "--table", "tasks"])
        .output()
        .unwrap();
    assert!(!revive.status.success(), "{revive:?}");
    assert!(
        String::from_utf8_lossy(&revive.stderr).contains("append-only drop planned"),
        "{}",
        String::from_utf8_lossy(&revive.stderr)
    );
    assert_eq!(snapshot_tree(&root), before_revive, "refusal wrote files");

    let recreated = jails_cmd(&root, None)
        .args(["g", "scaffold", "Task", "id:uuid@pk", "title:string!"])
        .output()
        .unwrap();
    assert!(recreated.status.success(), "{recreated:?}");
    let recreated_migration = root.join("src/main/resources/db/migration/V003__create_tasks.sql");
    assert!(recreated_migration.is_file(), "{recreated:?}");
    assert_eq!(fs::read_to_string(&create).unwrap(), sealed);
    assert!(
        fs::read_to_string(&recreated_migration)
            .unwrap()
            .contains("create table tasks"),
        "{}",
        recreated_migration.display()
    );
    assert!(
        root.join("src/main/java/com/example/demo/web/TaskController.java")
            .is_file()
    );
}

#[test]
fn task_drop_can_explicitly_apply_the_frozen_history_after_commit() {
    let root = temp_dir("task-drop-apply-migrations");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join("src/main/resources/db/migration")).unwrap();
    assert!(
        jails_cmd(&root, None)
            .args(["g", "scaffold", "Task", "id:uuid@pk", "title:string!"])
            .status()
            .unwrap()
            .success()
    );

    let fake = temp_dir("task-drop-flyway-bin");
    let log = fake.join("flyway.log");
    write_fake_maven(&fake, &["flyway"], &log);
    let before = snapshot_tree(&root);
    let preview = jails_cmd(&root, Some(&fake))
        .env(
            "DATABASE_URL",
            "postgresql://app:secret@127.0.0.1:5432/demo",
        )
        .args([
            "destroy",
            "scaffold",
            "Task",
            "--storage",
            "drop",
            "--confirm-table",
            "tasks",
            "--force",
            "--migrate",
            "--datasource",
            "DATABASE_URL",
            "--pretend",
            "--output",
            "json",
        ])
        .output()
        .unwrap();
    assert!(preview.status.success(), "{preview:?}");
    assert_eq!(snapshot_tree(&root), before, "preview committed the drop");
    assert!(read_log(&log).is_empty(), "preview ran Flyway");
    let json = String::from_utf8(preview.stdout).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    let effect = &value["report"]["data"]["post_commit"][0]["effect"];
    let effect = &effect["apply-migrations"];
    assert_eq!(effect["datasource"], "DATABASE_URL", "{json}");
    let migrations = effect["migrations"].as_array().unwrap();
    assert_eq!(migrations.len(), 2, "{json}");
    assert_eq!(migrations[0]["version"], 1, "{json}");
    assert_eq!(migrations[1]["version"], 2, "{json}");

    let committed = jails_cmd(&root, Some(&fake))
        .env(
            "DATABASE_URL",
            "postgresql://app:secret@127.0.0.1:5432/demo",
        )
        .args([
            "destroy",
            "scaffold",
            "Task",
            "--storage",
            "drop",
            "--confirm-table",
            "tasks",
            "--force",
            "--migrate",
            "--datasource",
            "DATABASE_URL",
        ])
        .output()
        .unwrap();
    assert!(
        committed.status.success(),
        "{}{}",
        String::from_utf8_lossy(&committed.stdout),
        String::from_utf8_lossy(&committed.stderr)
    );
    let committed_stdout = String::from_utf8_lossy(&committed.stdout);
    assert!(committed_stdout.contains("(done)"), "{committed_stdout}");
    let invoked = read_log(&log);
    assert!(invoked.contains("flyway migrate"), "{invoked}");
    assert!(!invoked.contains("secret"), "credential leaked: {invoked}");
    assert!(
        root.join("src/main/resources/db/migration/V002__drop_tasks.sql")
            .is_file(),
        "effect ran without the migration commit"
    );
}

#[test]
fn failed_migration_effect_keeps_the_committed_drop_and_retryable_receipt() {
    let root = temp_dir("task-drop-failed-migration-effect");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join("src/main/resources/db/migration")).unwrap();
    assert!(
        jails_cmd(&root, None)
            .args(["g", "scaffold", "Task", "id:uuid@pk", "title:string!"])
            .status()
            .unwrap()
            .success()
    );

    let fake = temp_dir("task-drop-failing-flyway-bin");
    let ignored_log = fake.join("ignored.log");
    write_fake_maven(&fake, &["flyway"], &ignored_log);
    fs::write(fake.join("flyway"), "#!/bin/sh\nexit 9\n").unwrap();
    let output = jails_cmd(&root, Some(&fake))
        .env(
            "DATABASE_URL",
            "postgresql://app:secret@127.0.0.1:5432/demo",
        )
        .args([
            "destroy",
            "scaffold",
            "Task",
            "--storage",
            "drop",
            "--confirm-table",
            "tasks",
            "--force",
            "--migrate",
            "--datasource",
            "DATABASE_URL",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success(), "{output:?}");
    let rendered = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !rendered.contains("secret"),
        "credential leaked: {rendered}"
    );
    assert!(
        root.join("src/main/resources/db/migration/V002__drop_tasks.sql")
            .is_file(),
        "failed effect rolled back the committed migration"
    );
    assert!(
        !root
            .join("src/main/java/com/example/demo/web/TaskController.java")
            .exists(),
        "failed effect rolled back retired projections"
    );
    let receipts = jails_commit::store::Store::at(&root)
        .read_receipts()
        .unwrap();
    let effect = receipts
        .first()
        .and_then(|receipt| receipt.post_commit.first())
        .expect("the newest receipt keeps its migration effect");
    assert!(
        matches!(
            effect.state,
            jails_protocol::effect::EffectState::Failed { attempt: 1, .. }
        ),
        "{:?}",
        effect.state
    );
}

#[test]
fn scaffold_reuses_an_existing_record_and_destroy_preserves_it() {
    let root = temp_dir("scaffold-model-first");
    write_spring_fixture(&root);

    let record = jails_cmd(&root, None)
        .args(["generate", "record", "Post", "id:uuid@pk", "title:string!"])
        .status()
        .unwrap();
    assert!(record.success());

    let scaffold = jails_cmd(&root, None)
        .args([
            "generate",
            "scaffold",
            "Post",
            "id:uuid@pk",
            "title:string!",
        ])
        .status()
        .unwrap();
    assert!(scaffold.success());

    let record_path = root.join("src/main/java/com/example/demo/domain/Post.java");
    let source = fs::read_to_string(&record_path).unwrap();
    assert!(source.contains("UUID id"), "{source}");
    assert!(source.contains("String title"), "{source}");

    let field = jails_cmd(&root, None)
        .args(["g", "field", "Post", "createdAt:instant?"])
        .output()
        .unwrap();
    assert!(field.status.success(), "{field:?}");
    let evolved = fs::read_to_string(&record_path).unwrap();
    assert!(evolved.contains("Optional<Instant> createdAt"), "{evolved}");
    let jdbc = fs::read_to_string(
        root.join("src/main/java/com/example/demo/adapters/JdbcPostRepository.java"),
    )
    .unwrap();
    assert!(jdbc.contains("created_at"), "{jdbc}");

    let destroy = jails_cmd(&root, None)
        .args(["destroy", "scaffold", "Post", "--force"])
        .status()
        .unwrap();
    assert!(destroy.success());
    assert!(
        record_path.is_file(),
        "destroy scaffold must not remove the record created by a prior intent"
    );
    let preserved = fs::read_to_string(&record_path).unwrap();
    assert!(
        preserved.contains("Optional<Instant> createdAt"),
        "destroy scaffold reverted the field shared with the record intent: {preserved}"
    );
    assert!(
        root.join("src/test/java/com/example/demo/domain/PostTest.java")
            .is_file(),
        "the record intent itself remains tracked and intact"
    );
    assert!(
        !root
            .join("src/main/java/com/example/demo/web/PostController.java")
            .exists()
    );
}

#[test]
fn destroy_refuses_to_remove_a_type_used_by_a_retained_entity() {
    let root = temp_dir("destroy-referenced-type");
    write_spring_fixture(&root);

    assert!(
        jails_cmd(&root, None)
            .args(["g", "enum", "Status", "Draft", "Published"])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        jails_cmd(&root, None)
            .args(["g", "scaffold", "Post", "id:uuid@pk", "status:Status",])
            .status()
            .unwrap()
            .success()
    );
    let status = root.join("src/main/java/com/example/demo/domain/Status.java");
    let before = snapshot_tree(&root);

    let refused = jails_cmd(&root, None)
        .args(["destroy", "enum", "Status", "--force"])
        .output()
        .unwrap();

    assert!(!refused.status.success(), "{refused:?}");
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(stderr.contains("pointing at nothing"), "{stderr}");
    assert!(stderr.contains("scaffold Post"), "{stderr}");
    assert!(status.is_file());
    assert_eq!(snapshot_tree(&root), before, "refusal mutated the project");
}

#[test]
fn an_association_blocks_a_drop_until_it_is_itself_retired_forward() {
    let root = temp_dir("destroy-incoming-association");
    write_spring_fixture(&root);
    assert!(
        jails_cmd(&root, None)
            .args(["add", "db", "--no-start"])
            .status()
            .unwrap()
            .success()
    );
    for command in [
        vec!["g", "scaffold", "Parent", "id:uuid@pk", "name:string!"],
        vec![
            "g",
            "scaffold",
            "Child",
            "id:uuid@pk",
            "parentId:uuid",
            "title:string!",
        ],
        vec![
            "g",
            "association",
            "ChildParent",
            "parentId=id",
            "--on",
            "Child",
            "--yields",
            "Parent",
        ],
    ] {
        let output = jails_cmd(&root, None).args(command).output().unwrap();
        assert!(output.status.success(), "{output:?}");
    }
    let before = snapshot_tree(&root);

    let refused = jails_cmd(&root, None)
        .args([
            "destroy",
            "scaffold",
            "Parent",
            "--storage",
            "drop",
            "--confirm-table",
            "parents",
            "--force",
        ])
        .output()
        .unwrap();

    assert!(!refused.status.success(), "{refused:?}");
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(stderr.contains("pointing at nothing"), "{stderr}");
    assert!(stderr.contains("association ChildParent"), "{stderr}");
    assert_eq!(snapshot_tree(&root), before, "refusal mutated the project");
    assert!(
        !root
            .join("src/main/resources/db/migration/V004__drop_parents.sql")
            .exists()
    );

    // And the way out exists. "Remove the dependant first" used to name a
    // command that refused on principle, so neither half of an association
    // could ever be destroyed -- a hard deadlock reachable in three commands.
    // Retiring the association *appends* `drop constraint`, which is the next
    // migration rather than the un-running of one.
    let retired = jails_cmd(&root, None)
        .args(["destroy", "association", "ChildParent", "--force"])
        .output()
        .unwrap();
    assert!(
        retired.status.success(),
        "{}{}",
        String::from_utf8_lossy(&retired.stdout),
        String::from_utf8_lossy(&retired.stderr)
    );
    let drop_constraint =
        root.join("src/main/resources/db/migration/V004__drop_child_parent_association.sql");
    let sql = fs::read_to_string(&drop_constraint).unwrap();
    assert!(sql.contains("alter table children"), "{sql}");
    assert!(
        sql.contains("drop constraint if exists children_child_parent_fk"),
        "{sql}"
    );
    assert!(
        root.join("src/main/resources/db/migration/V003__add_child_parent_association.sql")
            .is_file(),
        "the migration that added the constraint is append-only and stays"
    );

    // The refusal it was blocking is now gone.
    let dropped = jails_cmd(&root, None)
        .args([
            "destroy",
            "scaffold",
            "Parent",
            "--storage",
            "drop",
            "--confirm-table",
            "parents",
            "--force",
        ])
        .output()
        .unwrap();
    assert!(
        dropped.status.success(),
        "{}{}",
        String::from_utf8_lossy(&dropped.stdout),
        String::from_utf8_lossy(&dropped.stderr)
    );
}

#[test]
fn field_driven_generators_refuse_an_absent_model_with_a_fix() {
    let root = temp_dir("missing-model-fix");
    write_spring_fixture(&root);

    for kind in ["scaffold", "dto", "repo"] {
        let output = jails_cmd(&root, None)
            .args(["generate", kind, "Missing"])
            .output()
            .unwrap();
        assert!(!output.status.success(), "{kind} unexpectedly succeeded");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("fix:"), "{kind}: {stderr}");
        assert!(stderr.contains("g record Missing"), "{kind}: {stderr}");
    }
}

#[test]
fn generate_field_updates_unchanged_derivatives_preserves_edits_and_adds_a_migration() {
    let root = temp_dir("generate-field");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join("src/main/resources/db/migration")).unwrap();

    let scaffold = jails_cmd(&root, None)
        .args(["g", "scaffold", "Note", "id:uuid@pk", "title:string!"])
        .status()
        .unwrap();
    assert!(scaffold.success());

    let request = root.join("src/main/java/com/example/demo/web/NoteRequest.java");
    let edited = format!(
        "{}// user-owned validation\n",
        fs::read_to_string(&request).unwrap()
    );
    fs::write(&request, &edited).unwrap();

    let refused = jails_cmd(&root, None)
        .args(["resource", "field", "add", "Note", "createdAt:instant"])
        .output()
        .unwrap();
    assert!(!refused.status.success());
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("needs a data plan"),
        "{}",
        String::from_utf8_lossy(&refused.stderr)
    );
    assert!(
        !root
            .join("src/main/resources/db/migration/V002__add_created_at_to_notes.sql")
            .exists(),
        "a refused data plan must not mutate the project"
    );

    let output = jails_cmd(&root, None)
        .args([
            "resource",
            "field",
            "add",
            "Note",
            "createdAt:instant",
            "--default-literal",
            "2026-08-25T12:00:00Z",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    // The plan names the files a new component touches: the record, its test
    // and the migration that adds the column. V1 printed a per-derivative
    // `skipped`/`add component` line from a walk of its own.
    assert!(stdout.contains("replace "), "{stdout}");
    assert!(stdout.contains(".sql"), "{stdout}");

    let record =
        fs::read_to_string(root.join("src/main/java/com/example/demo/domain/Note.java")).unwrap();
    assert!(record.contains("Instant createdAt"), "{record}");
    let jdbc = fs::read_to_string(
        root.join("src/main/java/com/example/demo/adapters/JdbcNoteRepository.java"),
    )
    .unwrap();
    assert!(jdbc.contains("created_at"), "{jdbc}");
    // The edited derivative is *merged*, not skipped. V1 left it alone, which
    // preserved the edit and left the request record missing the component the
    // record had just grown -- a DTO that no longer describes its own domain
    // type. Both halves survive here: the reader's line and the new field.
    let merged = fs::read_to_string(&request).unwrap();
    assert!(merged.contains("// user-owned validation"), "{merged}");
    assert!(merged.contains("Instant createdAt"), "{merged}");
    let _ = &edited;

    let migration = fs::read_to_string(
        root.join("src/main/resources/db/migration/V002__add_created_at_to_notes.sql"),
    )
    .unwrap();
    assert!(
        migration.contains("add column created_at timestamptz"),
        "{migration}"
    );
    assert!(
        migration.contains("set created_at = '2026-08-25T12:00:00Z'"),
        "{migration}"
    );
    assert!(
        migration.contains("alter column created_at set not null"),
        "{migration}"
    );
    assert!(
        !migration.contains("default current_timestamp"),
        "{migration}"
    );

    assert!(
        common::ledger_mentions(
            &root,
            "src/main/resources/db/migration/V002__add_created_at_to_notes.sql"
        ),
        "the new migration is recorded against the intent that wrote it"
    );
    assert!(
        common::ledger_mentions(&root, env!("CARGO_PKG_VERSION")),
        "the ledger records which jails wrote it"
    );

    let duplicate = jails_cmd(&root, None)
        .args(["g", "field", "Note", "createdAt:instant"])
        .output()
        .unwrap();
    assert!(!duplicate.status.success());
    assert!(
        String::from_utf8_lossy(&duplicate.stderr).contains("already has a `createdAt` component")
    );

    let alias = jails_cmd(&root, None)
        .args([
            "g",
            "field",
            "Note",
            "priority:int",
            "--default-literal",
            "7",
        ])
        .output()
        .unwrap();
    assert!(alias.status.success(), "{alias:?}");
    let alias_migration = fs::read_to_string(
        root.join("src/main/resources/db/migration/V003__add_priority_to_notes.sql"),
    )
    .unwrap();
    assert!(
        alias_migration.contains("set priority = 7"),
        "{alias_migration}"
    );
}

#[test]
fn resource_field_commands_use_the_risk_specific_cli_contracts() {
    let root = temp_dir("resource-field-commands");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join("src/main/resources/db/migration")).unwrap();
    // A scaffold: these are column operations, and the kind that owns a table
    // is the kind they apply to. `V001` is its own `create table`, so the
    // evolutions below start at `V002`.
    let generated = jails_cmd(&root, None)
        .args([
            "g",
            "scaffold",
            "Task",
            "id:uuid@pk",
            "title:string!",
            "priority:int",
            "description:string",
            "legacyCode:string?",
        ])
        .output()
        .unwrap();
    assert!(generated.status.success(), "{generated:?}");
    fs::create_dir_all(root.join("db/backfills")).unwrap();
    fs::write(
        root.join("db/backfills/task_description.sql"),
        "update tasks set description = 'unknown' where description is null;\n",
    )
    .unwrap();

    for args in [
        vec![
            "resource",
            "field",
            "rename",
            "Task",
            "title",
            "headline",
            "--column",
            "single-cutover",
        ],
        vec![
            "resource",
            "field",
            "type",
            "Task",
            "priority",
            "--to",
            "long",
            "--strategy",
            "safe",
        ],
        vec![
            "resource",
            "field",
            "nullability",
            "Task",
            "description",
            "--nullable",
        ],
        vec![
            "resource",
            "field",
            "drop",
            "Task",
            "legacyCode",
            "--confirm-column",
            "legacy_code",
        ],
    ] {
        let output = jails_cmd(&root, None).args(args).output().unwrap();
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let refused = jails_cmd(&root, None)
        .args([
            "resource",
            "field",
            "nullability",
            "Task",
            "description",
            "--required",
        ])
        .output()
        .unwrap();
    assert!(!refused.status.success(), "{refused:?}");
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("--backfill-file"),
        "{}",
        String::from_utf8_lossy(&refused.stderr)
    );
    let required = jails_cmd(&root, None)
        .args([
            "resource",
            "field",
            "nullability",
            "Task",
            "description",
            "--required",
            "--backfill-file",
            "db/backfills/task_description.sql",
        ])
        .output()
        .unwrap();
    assert!(required.status.success(), "{required:?}");
    let migration = fs::read_to_string(
        root.join("src/main/resources/db/migration/V006__make_description_required.sql"),
    )
    .unwrap();
    assert!(
        migration.contains("set description = 'unknown'"),
        "{migration}"
    );
    assert!(
        migration.contains("alter column description set not null"),
        "{migration}"
    );

    let record =
        fs::read_to_string(root.join("src/main/java/com/example/demo/domain/Task.java")).unwrap();
    assert!(record.contains("String headline"), "{record}");
    assert!(record.contains("long priority"), "{record}");
    assert!(record.contains("String description"), "{record}");
    assert!(!record.contains("legacyCode"), "{record}");
}

#[test]
fn scaffold_refuses_to_silently_flatten_a_project_record_component() {
    let root = temp_dir("scaffold-project-record");
    write_spring_fixture(&root);
    assert!(
        jails_cmd(&root, None)
            .args(["g", "record", "User", "id:uuid@pk", "name:string!"])
            .status()
            .unwrap()
            .success()
    );

    let output = jails_cmd(&root, None)
        .args(["g", "scaffold", "Post", "id:uuid@pk", "author:User"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("cannot be persisted"), "{stderr}");
    assert!(stderr.contains("author:UUID"), "{stderr}");
    assert!(stderr.contains("g association"), "{stderr}");
    assert!(stderr.contains("--on Post --yields User"), "{stderr}");
    assert!(
        !root
            .join("src/main/java/com/example/demo/domain/Post.java")
            .exists(),
        "the refusal must happen before the first write"
    );
}

#[test]
fn scaffold_timestamps_flow_through_ddl_create_and_optimistic_updates() {
    let root = temp_dir("scaffold-timestamps");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join("src/main/resources/db/migration")).unwrap();

    let scaffold = jails_cmd(&root, None)
        .args([
            "g",
            "scaffold",
            "Note",
            "id:uuid@pk",
            "title:string!",
            "version:long",
            "--timestamps",
        ])
        .status()
        .unwrap();
    assert!(scaffold.success());
    let record =
        fs::read_to_string(root.join("src/main/java/com/example/demo/domain/Note.java")).unwrap();
    assert!(record.contains("Instant createdAt"), "{record}");
    assert!(record.contains("Instant updatedAt"), "{record}");
    let ddl =
        fs::read_to_string(root.join("src/main/resources/db/migration/V001__create_notes.sql"))
            .unwrap();
    assert!(ddl.contains("created_at"), "{ddl}");
    assert!(ddl.contains("updated_at"), "{ddl}");

    let transition = jails_cmd(&root, None)
        .args([
            "g",
            "transition",
            "RenameNote",
            "id:uuid",
            "title:string!",
            "version:long",
            "--on",
            "Note",
        ])
        .status()
        .unwrap();
    assert!(transition.success());
    let adapter = fs::read_to_string(
        root.join("src/main/java/com/example/demo/adapters/JdbcRenameNoteTransition.java"),
    )
    .unwrap();
    assert!(
        adapter.contains("updated_at = current_timestamp"),
        "{adapter}"
    );
}

#[test]
fn scaffold_writes_http_requests_and_factory_builds_typed_test_data() {
    let root = temp_dir("requests-factory");
    write_spring_fixture(&root);

    assert!(
        jails_cmd(&root, None)
            .args([
                "g",
                "scaffold",
                "Note",
                "id:uuid@pk",
                "title:string!",
                "createdAt:instant",
            ])
            .status()
            .unwrap()
            .success()
    );
    let requests = fs::read_to_string(root.join("requests/note.http")).unwrap();
    assert!(requests.contains("POST {{baseUrl}}/notes"), "{requests}");
    assert!(requests.contains("GET {{baseUrl}}/notes"), "{requests}");
    assert!(
        requests.contains("GET {{baseUrl}}/notes/{{id}}"),
        "{requests}"
    );
    assert!(
        requests.contains("DELETE {{baseUrl}}/notes/{{id}}"),
        "{requests}"
    );
    assert!(
        requests.contains("@id = 00000000-0000-0000-0000-000000000001"),
        "{requests}"
    );
    assert!(
        requests.contains("\"createdAt\": \"2026-01-01T00:00:00Z\""),
        "{requests}"
    );

    assert!(
        jails_cmd(&root, None)
            .args(["g", "factory", "Note"])
            .status()
            .unwrap()
            .success()
    );
    let factory =
        fs::read_to_string(root.join("src/test/java/com/example/demo/testkit/NoteFactory.java"))
            .unwrap();
    assert!(
        factory.contains("public static NoteFactory aNote()"),
        "{factory}"
    );
    assert!(factory.contains("withTitle(String value)"), "{factory}");
    assert!(factory.contains("Instant.parse("), "{factory}");
    assert!(factory.contains("return new Note("), "{factory}");
}

/// The generated guard has to *run*, not merely compile.
///
/// Every claim this generator makes is behavioural -- a retry replays rather
/// than conflicts, a reused key is refused, an in-flight key is told to wait --
/// and none of it is visible to a compile check. The generated unit test
/// asserts all four outcomes, so the thing to verify here is that Surefire
/// actually executes it: `plan.md` §18's standing warning is that a skipped
/// tier-3 test reports as a pass.
#[test]
fn generate_idempotency_produces_tests_that_run_and_pass() {
    if !real_mvn_available() {
        skip("mvn not found on PATH");
        return;
    }
    if !real_java_supports_target_release() {
        skip("javac does not accept the target release");
        return;
    }
    let root = temp_dir("idempotency-real");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join("src/main/resources/db/migration")).unwrap();

    assert!(
        jails_cmd(&root, None)
            .args(["add", "db", "--no-start"])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        jails_cmd(&root, None)
            .args(["g", "idempotency", "Request"])
            .status()
            .unwrap()
            .success()
    );

    let path = real_path_without_mvnd();
    let verified = verified_spring_db_toolbox(&path);
    assert_surefire_test_count(verified, "RequestGuardTest", 5);
}

/// Reading is not writing.
///
/// `app plan`, `--pretend` and inspection all reach the store, and a reader
/// that also tidied up would make asking jails what it would do change what it
/// had done. It is not a hypothetical: the provenance reader used to fold a
/// pre-ledger `.jails/` and delete the old files from inside the read, so
/// asking "is there anything to destroy" consumed the evidence for the answer.
#[test]
fn planning_pretending_and_inspecting_leave_machine_state_byte_for_byte() {
    let root = temp_dir("read-purity");
    write_plain_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(
        root.join(".jails/app.toml"),
        "schema = 1\ncapabilities = []\n\n[[generate]]\nkind = \"record\"\nname = \"Note\"\n\
         fields = [\"title:string!\"]\n",
    )
    .unwrap();

    // A project with no store yet. No read may create one -- an empty `.jails`
    // would make a project that has never been touched look like one that has.
    let arguments = [
        vec!["app", "plan"],
        vec!["destroy", "record", "Note", "--pretend"],
        vec!["generate", "record", "Other", "title:string!", "--pretend"],
        vec!["routes"],
    ];
    let before = snapshot_tree(&root.join(".jails"));
    for argv in &arguments {
        jails_cmd(&root, None).args(argv).output().unwrap();
        assert_eq!(
            before,
            snapshot_tree(&root.join(".jails")),
            "`jails {}` changed machine state while only being asked to report",
            argv.join(" ")
        );
    }
    assert!(
        !root.join(".jails/ledger.toml").exists(),
        "and no read created the ledger either"
    );

    // The same once there *is* a store, which is the case with something to
    // lose: a read that rewrote it would move the generation nothing committed.
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
    assert!(root.join(".jails/ledger.toml").is_file());
    let before = snapshot_tree(&root.join(".jails"));
    for argv in &arguments {
        jails_cmd(&root, None).args(argv).output().unwrap();
        assert_eq!(
            before,
            snapshot_tree(&root.join(".jails")),
            "`jails {}` changed a committed store while only being asked to report",
            argv.join(" ")
        );
    }
}

#[test]
fn a_timestamped_scaffold_does_not_ask_the_caller_for_its_audit_columns() {
    // `jails g scaffold --help` promises "the generated create path supplies
    // both". It did not: they arrived as `@NotNull` wire components, so the
    // documented POST answered 400 naming two fields the caller has no
    // business setting -- found by sending it at a running application.
    let root = temp_dir("scaffold-timestamps-wire");
    write_spring_fixture(&root);
    assert!(
        jails_cmd(&root, None)
            .args([
                "g",
                "scaffold",
                "Note",
                "id:uuid@pk",
                "title:string!",
                "--timestamps",
            ])
            .status()
            .unwrap()
            .success()
    );

    let request =
        fs::read_to_string(root.join("src/main/java/com/example/demo/web/NoteRequest.java"))
            .unwrap();
    assert!(!request.contains("Instant createdAt"), "{request}");
    assert!(!request.contains("Instant updatedAt"), "{request}");
    assert!(
        request.contains("Instant now = Instant.now();"),
        "{request}"
    );

    // The record still declares them, and the response still returns them: the
    // server sets these, it does not hide them.
    let record =
        fs::read_to_string(root.join("src/main/java/com/example/demo/domain/Note.java")).unwrap();
    assert!(record.contains("Instant createdAt"), "{record}");
    let response =
        fs::read_to_string(root.join("src/main/java/com/example/demo/web/NoteResponse.java"))
            .unwrap();
    assert!(response.contains("Instant createdAt"), "{response}");

    // And the sendable collection describes a request that can be made -- as
    // does the generated controller test, which sends that same body.
    let requests = fs::read_to_string(root.join("requests/note.http")).unwrap();
    assert!(!requests.contains("createdAt"), "{requests}");
    let controller_test =
        fs::read_to_string(root.join("src/test/java/com/example/demo/web/NoteControllerTest.java"))
            .unwrap();
    assert!(
        controller_test.contains("theDocumentedCreateRequestIsAccepted"),
        "{controller_test}"
    );
    // The JSON key, not the word: the test's own Javadoc names the defect.
    assert!(
        !controller_test.contains("\"createdAt\":"),
        "{controller_test}"
    );
}

#[test]
fn a_scoped_scaffold_documents_only_the_request_its_controller_answers() {
    // A scoped resource is create-only: every read has to carry the tenant, so
    // it is a `jails g query`. The collection used to end with a `### List`
    // block regardless, and the generated controller test already asserted
    // that same GET is a 405 -- the test knew and the collection did not.
    let root = temp_dir("scoped-scaffold-requests");
    write_spring_fixture(&root);
    assert!(
        jails_cmd(&root, None)
            .args(["add", "security"])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        jails_cmd(&root, None)
            .args(["g", "scaffold", "Note", "id:uuid@pk@scope", "title:string!"])
            .status()
            .unwrap()
            .success()
    );

    let requests = fs::read_to_string(root.join("requests/note.http")).unwrap();
    assert!(requests.contains("POST {{baseUrl}}/notes"), "{requests}");
    assert!(!requests.contains("GET {{baseUrl}}"), "{requests}");

    let controller_test =
        fs::read_to_string(root.join("src/test/java/com/example/demo/web/NoteControllerTest.java"))
            .unwrap();
    assert!(
        controller_test.contains("hasStatus(405)"),
        "{controller_test}"
    );
    assert!(
        controller_test.contains("theDocumentedCreateRequestIsAccepted"),
        "{controller_test}"
    );
}

#[test]
fn scaffold_refuses_an_unmapped_project_type_before_writing() {
    let root = temp_dir("scaffold-unmapped-type");
    write_spring_fixture(&root);
    let output = jails_cmd(&root, None)
        .args(["g", "scaffold", "Book", "id:uuid@pk", "author:Author"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("author:Author"), "{stderr}");
    assert!(stderr.contains("cannot persist"), "{stderr}");
    assert!(stderr.contains("fix:"), "{stderr}");
    assert!(
        !root
            .join("src/main/java/com/example/demo/domain/Book.java")
            .exists()
    );
    assert!(!root.join("src/main/resources/db/migration").exists());
}

#[test]
fn generated_http_sink_delivers_typed_json_with_a_stable_idempotency_key() {
    if !real_mvn_available() {
        skip("mvn not found on PATH");
        return;
    }
    if !real_java_available() {
        skip("java/javac not found on PATH");
        return;
    }
    let path = real_path_without_mvnd();
    let root = temp_dir("http-outbox-sink-real");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(
        root.join(".jails/app.toml"),
        include_str!("../../examples/support-inbox/.jails/app.toml"),
    )
    .unwrap();

    let output = jails_cmd_with_path(&root, &path)
        .args(["app", "apply", "--no-start"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "apply: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let support = verified_app_unit_fixtures(&path)
        .iter()
        .find(|(name, _)| *name == "support-inbox")
        .map(|(_, root)| root)
        .unwrap();
    assert_surefire_test_count(support, "ProviderHttpOutboxSinkTest", 1);
}

/// The other direction: a dispatcher somebody is already using keeps the
/// entry point.
///
/// `App.java` with a command registered in it is the project's real CLI, and
/// a second `generate cli` must not move the jar out from under it.
#[test]
fn a_dispatcher_already_in_use_keeps_the_entry_point() {
    let workdir = temp_dir("cli-entry-point-kept");
    assert!(
        jails_cmd(&workdir, None)
            .args(["new-cli", "demo", "--no-git"])
            .status()
            .unwrap()
            .success()
    );
    let root = workdir.join("demo");

    // A command registered into `App` makes it the project's own CLI.
    assert!(
        jails_cmd(&root, None)
            .args(["g", "command", "Import"])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        jails_cmd(&root, None)
            .args(["g", "cli", "Admin"])
            .status()
            .unwrap()
            .success()
    );

    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    assert!(
        pom.contains("<mainClass>com.example.demo.App</mainClass>"),
        "the entry point moved out from under a dispatcher in use:\n{pom}"
    );
}

#[test]
fn destroy_without_force_prompts_and_aborts_on_no() {
    let root = temp_dir("destroy-prompt");
    write_project_skeleton(&root);
    jails_cmd(&root, None)
        .args(["generate", "controller", "comment"])
        .status()
        .unwrap();
    let file = root.join("src/main/java/com/example/demo/web/CommentController.java");
    assert!(file.is_file());

    let mut child = jails_cmd(&root, None)
        .args(["destroy", "controller", "comment"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"n\n").unwrap();
    let status = child.wait().unwrap();

    assert!(status.success());
    assert!(
        file.is_file(),
        "file should survive a declined confirmation"
    );
}

#[test]
fn generate_twice_writes_nothing_the_second_time() {
    let root = temp_dir("duplicate");
    write_project_skeleton(&root);
    let service = root.join("src/main/java/com/example/demo/service/CommentService.java");
    jails_cmd(&root, None)
        .args(["generate", "service", "comment"])
        .status()
        .unwrap();
    let before = fs::read_to_string(&service).unwrap();
    let output = jails_cmd(&root, None)
        .args(["generate", "service", "comment"])
        .output()
        .unwrap();
    // A second identical generate is a **no-op**, not a refusal. V1 refused
    // because it would otherwise clobber whatever was there; here the file is
    // owned by the intent that wrote it, so "nothing changed" is the honest
    // answer -- and an edited file is three-way merged rather than refused,
    // which `app_manifest_merges_an_edited_intent_over_user_changes` pins.
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("nothing to do"),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert_eq!(fs::read_to_string(&service).unwrap(), before);
}

#[test]
fn generate_errors_on_unknown_field_type() {
    let root = temp_dir("unknown-field-type");
    write_project_skeleton(&root);
    let output = jails_cmd(&root, None)
        .args(["generate", "record", "widget", "id:nope"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown field type"));
}

#[test]
fn generate_errors_outside_a_project() {
    let root = temp_dir("no-project");
    let output = jails_cmd(&root, None)
        .args(["generate", "record", "widget"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("pom.xml"));
}

#[test]
fn short_generators_cover_raw_sql_and_test_seams() {
    let root = temp_dir("simple-generators");
    write_project_skeleton(&root);

    for args in [
        vec!["g", "interface", "IdGenerator"],
        vec!["g", "integration-test", "DatabaseSmoke"],
        vec!["g", "migration", "createRewardCore"],
        vec!["g", "mig", "add_outbox"],
        vec!["g", "repository", "Reward", "id:uuid"],
    ] {
        let output = jails_cmd(&root, None).args(&args).output().unwrap();
        assert!(
            output.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    assert!(
        root.join("src/main/java/com/example/demo/IdGenerator.java")
            .is_file()
    );
    assert!(
        root.join("src/test/java/com/example/demo/DatabaseSmokeIT.java")
            .is_file()
    );
    assert!(
        root.join("src/main/resources/db/migration/V001__create_reward_core.sql")
            .is_file()
    );
    assert!(
        root.join("src/main/resources/db/migration/V002__add_outbox.sql")
            .is_file()
    );
    assert!(
        root.join("src/main/java/com/example/demo/app/RewardRepository.java")
            .is_file()
    );
    let adapter = fs::read_to_string(
        root.join("src/main/java/com/example/demo/adapters/JdbcRewardRepository.java"),
    )
    .unwrap();
    assert!(adapter.contains("PreparedStatement") || adapter.contains("prepareStatement"));
    assert!(
        adapter.contains("\"\"\""),
        "raw SQL should be emitted as text blocks: {adapter}"
    );
    assert!(!adapter.contains("org.springframework"));
}

#[test]
fn generate_scaffold_produces_a_project_that_compiles_and_passes_tests() {
    if !real_mvn_available() {
        skip("mvn not found on PATH");
        return;
    }
    let path = real_path_without_mvnd();
    let root = temp_dir("real-scaffold-compiles");
    write_spring_fixture(&root);

    let status = jails_cmd_with_path(&root, &path)
        .args([
            "generate",
            "scaffold",
            "Post",
            "id:uuid@pk",
            "title:string",
            "body:text",
            "published:boolean",
        ])
        .status()
        .unwrap();
    assert!(status.success());

    let verified = verified_spring_toolbox(&path);
    assert!(
        verified
            .join("target/test-classes/com/example/demo/web/PostControllerTest.class")
            .is_file(),
        "the shared Spring toolbox did not compile the scaffold tests"
    );
}

/// Regression coverage for the reported bug (standalone `generate
/// controller` not producing a test) plus real-compile verification of the
/// new controller/service/record companion test templates.
#[test]
fn standalone_generators_companion_tests_compile_and_pass() {
    if !real_mvn_available() {
        skip("mvn not found on PATH");
        return;
    }
    let path = real_path_without_mvnd();
    let root = temp_dir("real-standalone-companion-tests");
    write_spring_fixture(&root);

    for args in [
        vec!["generate", "controller", "Health"],
        vec!["generate", "service", "Billing"],
        vec![
            "generate",
            "record",
            "Tag",
            "name:string",
            "createdAt:datetime",
        ],
    ] {
        let status = jails_cmd_with_path(&root, &path)
            .args(&args)
            .status()
            .unwrap();
        assert!(status.success(), "{args:?} failed");
    }

    let verified = verified_spring_toolbox(&path);
    for class in [
        "com/example/demo/web/HealthControllerTest.class",
        "com/example/demo/service/BillingServiceTest.class",
        "com/example/demo/domain/TagTest.class",
        // The socket's two behaviours run here rather than in a container:
        // both are about the session registry. plan.md P8.4.
        "com/example/demo/web/ChatSocketHandlerTest.class",
    ] {
        assert!(
            verified.join("target/test-classes").join(class).is_file(),
            "the Spring toolbox did not compile {class}"
        );
    }
}

/// `record`, `command` and `class` are the plain-Java kinds, so the bar for
/// them is a `new-cli` project -- no Spring anywhere -- that still compiles and
/// passes the tests they generate.
#[test]
fn record_and_command_compile_and_pass_in_a_plain_cli_project() {
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
    let workdir = temp_dir("real-record-command");
    jails_cmd_with_path(&workdir, &path)
        .args(["new-cli", "demo"])
        .status()
        .unwrap();
    let root = workdir.join("demo");

    for args in [
        vec![
            "generate",
            "record",
            "Money",
            "amount:long",
            "currency:string",
            "occurredOn:date",
        ],
        vec!["generate", "command", "Greet"],
        vec!["generate", "class", "MoneyMoved"],
    ] {
        let status = jails_cmd_with_path(&root, &path)
            .args(&args)
            .status()
            .unwrap();
        assert!(status.success(), "{args:?} failed");
    }

    // `class` is the one kind that lands in the base package rather than a
    // subpackage -- a wrong `place()` here would compile and still be wrong.
    assert!(
        root.join("src/main/java/com/example/demo/MoneyMoved.java")
            .exists()
    );
    assert!(
        root.join("src/test/java/com/example/demo/MoneyMovedTest.java")
            .exists()
    );

    let verified = verified_plain_toolbox(&path);
    for class in ["MoneyMoved", "cli/GreetCommand", "domain/Tally"] {
        assert!(
            verified
                .join(format!("target/classes/com/example/demo/{class}.class"))
                .is_file(),
            "shared toolchain matrix did not compile {class}"
        );
    }
}

/// The whole toolbox at once: every capability and every generator in one
/// project, then its own suite. This is the only tier that answers "does what
/// jails writes actually compile and pass" for the generated *test* code as
/// well as the main code -- a template that emits an uncompilable assertion
/// looks perfectly fine to every other tier.
#[test]
fn every_generator_and_capability_together_compiles_and_passes_tests() {
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
    for path in [
        "target/classes/com/example/demo/cli/GreetCommand.class",
        "target/classes/com/example/demo/domain/Money.class",
        "target/classes/com/example/demo/domain/Tally.class",
        "target/test-classes/com/example/demo/BriefTest.class",
    ] {
        assert!(root.join(path).is_file(), "matrix did not compile {path}");
    }
}

/// The generators composing: an enum and a record, then a value type that
/// references both by name. Proves the three halves of the field syntax --
/// capitalised = a type this project owns, `!`/`?` optionality, and the
/// enum-aware sample values -- produce a project that actually compiles.
#[test]
fn generators_compose_through_user_owned_field_types() {
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
    let workdir = temp_dir("real-compose");
    jails_cmd_with_path(&workdir, &path)
        .args(["new-cli", "gym"])
        .status()
        .unwrap();
    let root = workdir.join("gym");

    for args in [
        vec!["generate", "enum", "currency", "GBP", "EUR"],
        vec![
            "generate",
            "record",
            "sourceRef",
            "system:string",
            "externalId:string",
        ],
        vec![
            "generate",
            "value",
            "canonicalTransaction",
            "id:string!",
            "date:date",
            "amountMinor:long",
            "currency:Currency",
            "source:SourceRef",
            "note:string?",
        ],
    ] {
        let status = jails_cmd_with_path(&root, &path)
            .args(&args)
            .status()
            .unwrap();
        assert!(status.success(), "{args:?} failed");
    }

    let value = fs::read_to_string(
        root.join("src/main/java/com/example/gym/domain/CanonicalTransaction.java"),
    )
    .unwrap();
    assert!(
        value.contains("Currency currency"),
        "an owned type is used verbatim: {value}"
    );
    assert!(value.contains("SourceRef source"), "{value}");
    assert!(
        value.contains("long amountMinor"),
        "built-ins stay primitive: {value}"
    );
    assert!(
        value.contains(r#"throw new IllegalArgumentException("id must not be blank")"#),
        "! means non-blank: {value}"
    );
    assert!(
        value.contains("Optional<String> note"),
        "? puts absence in the type: {value}"
    );
    assert!(
        value.contains("requireNonNullElse(note, Optional.empty())"),
        "a null Optional is normalised: {value}"
    );

    // `!` is a text rule; asking for it on a date is a mistake worth naming
    // rather than silently ignoring.
    //
    // The wording is the surviving parser's. `pending.md` §6.3 merged the two,
    // and the one that lives carries a `fix:` line -- which is why this asserts
    // on the sentence rather than on the older "only applies to text".
    let output = jails_cmd_with_path(&root, &path)
        .args(["generate", "value", "bad", "occurredOn:date!"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let refusal = String::from_utf8_lossy(&output.stderr);
    assert!(refusal.contains("is not text"), "{refusal}");
    assert!(refusal.contains("fix: drop the `!`"), "{refusal}");

    // An enum-typed component can be sampled by reading the enum, and a
    // component whose type is a record *this project already has* by reading
    // the record: `SourceRef` was generated two commands ago, so refusing to
    // build one would be the tool forgetting what it just wrote.
    let test = fs::read_to_string(
        root.join("src/test/java/com/example/gym/domain/CanonicalTransactionTest.java"),
    )
    .unwrap();
    // The constant by name rather than by position: `values()[0]` starts
    // standing for a different value the moment somebody reorders the enum,
    // and nothing in the diff says so. plan.md P6.5.
    assert!(test.contains("Currency.GBP"), "{test}");
    assert!(!test.contains("values()[0]"), "{test}");
    assert!(
        test.contains("new SourceRef("),
        "a component whose type is a record on disk is sampled from it: {test}"
    );
    assert!(
        !test.contains("@Disabled"),
        "every component is fabricable now, so nothing should be disabled: {test}"
    );

    // A generated sealed interface has no constructor, but its generated
    // variants are zero-component records. Any one is a valid non-null sample
    // for testing Stamped's own validation, so Jails can construct it without
    // guessing at business data.
    let status = jails_cmd_with_path(&root, &path)
        .args(["generate", "sealed", "outcome", "Accepted", "Rejected"])
        .status()
        .unwrap();
    assert!(status.success(), "generate sealed failed");
    let status = jails_cmd_with_path(&root, &path)
        .args([
            "generate",
            "value",
            "stamped",
            "at:string!",
            "result:Outcome",
        ])
        .status()
        .unwrap();
    assert!(status.success(), "generate value with a sealed type failed");
    let stamped =
        fs::read_to_string(root.join("src/test/java/com/example/gym/domain/StampedTest.java"))
            .unwrap();
    assert!(stamped.contains("new Outcome.Accepted()"), "{stamped}");
    assert!(
        !stamped.contains("@Disabled"),
        "a generated zero-component variant is a complete sample: {stamped}"
    );

    let verified = verified_plain_toolbox(&path);
    for class in ["Currency", "SourceRef", "CanonicalTransaction", "Stamped"] {
        assert!(
            verified
                .join(format!(
                    "target/classes/com/example/demo/domain/{class}.class"
                ))
                .is_file(),
            "shared toolchain matrix did not compile {class}"
        );
    }
}

#[test]
fn a_scaffold_with_database_types_compiles_including_its_derived_jdbc_adapter() {
    // The tier that answers the question the tool exists for. A unit test on
    // the SQL mapping cannot catch a generated expression that is merely
    // *nearly* right: `Timestamp.from(x.createdAt())` has the receiver in
    // the middle, so gluing the receiver on the front yields
    // `x.Timestamp.from(createdAt())` -- which reads fine and does not
    // compile. Only javac finds that.
    if !real_mvn_available() {
        skip("mvn not found on PATH");
        return;
    }
    let path = real_path_without_mvnd();
    let root = temp_dir("real-scaffold-jdbc");
    write_spring_fixture(&root);

    // The enum has to exist before the record names it, or the mapping falls
    // back to "unmappable" and the interesting branch never runs.
    let status = jails_cmd_with_path(&root, &path)
        .args(["generate", "enum", "Currency", "GBP", "USD"])
        .status()
        .unwrap();
    assert!(status.success());

    let status = jails_cmd_with_path(&root, &path)
        .args([
            "generate",
            "scaffold",
            "Payout",
            "id:uuid@pk",
            "amount:bigdecimal",
            "currency:Currency",
            "paidAt:instant",
            "note:string?",
        ])
        .status()
        .unwrap();
    assert!(status.success());

    let adapter = fs::read_to_string(
        root.join("src/main/java/com/example/demo/adapters/JdbcPayoutRepository.java"),
    )
    .unwrap();
    // Derived, not left as a TODO.
    assert!(
        !adapter.contains("UnsupportedOperationException"),
        "{adapter}"
    );
    assert!(
        adapter.contains("Timestamp.from(payout.paidAt())"),
        "{adapter}"
    );
    assert!(
        adapter.contains("Currency.valueOf(rows.getString(\"currency\"))"),
        "{adapter}"
    );
    // An Optional component is unwrapped on the way out and rebuilt on the way in.
    assert!(
        adapter.contains("Optional.ofNullable(rows.getString(\"note\"))"),
        "{adapter}"
    );
    assert!(adapter.contains("payout.note().orElse(null)"), "{adapter}");
    // The column list is shared by the select and the insert, so they agree.
    assert!(
        adapter.contains("insert into payouts (id, amount, currency, paid_at, note)"),
        "{adapter}"
    );

    // The DTOs name the project's own enum, so they have to import it --
    // `field.imports` only carries the built-in types' packages.
    let request =
        fs::read_to_string(root.join("src/main/java/com/example/demo/web/PayoutRequest.java"))
            .unwrap();
    assert!(
        request.contains("import com.example.demo.domain.Currency;"),
        "{request}"
    );

    let verified = verified_spring_db_toolbox(&path);
    assert!(
        verified
            .join("target/classes/com/example/demo/adapters/JdbcPayoutRepository.class")
            .is_file(),
        "the shared JDBC toolbox did not compile the derived adapter"
    );
}

#[test]
fn a_scaffold_emits_a_migration_whose_columns_match_the_adapter() {
    let root = temp_dir("scaffold-migration");
    write_spring_fixture(&root);
    // `add db` is what creates this directory; jails emits a migration only
    // when the project has somewhere to put one.
    fs::create_dir_all(root.join("src/main/resources/db/migration")).unwrap();

    let output = jails_cmd(&root, None)
        .args([
            "generate",
            "scaffold",
            "Payout",
            "id:uuid@pk",
            "amount:bigdecimal",
            "paidAt:instant",
            "note:string?",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");

    let migration =
        fs::read_to_string(root.join("src/main/resources/db/migration/V001__create_payouts.sql"))
            .unwrap();
    assert!(migration.contains("create table payouts ("), "{migration}");
    assert!(
        migration.contains("uuid") && migration.contains("numeric"),
        "{migration}"
    );
    // An Instant needs a zone-aware column or it comes back reinterpreted.
    assert!(migration.contains("timestamptz not null"), "{migration}");
    // The nullable component is the only one without `not null`.
    assert!(migration.contains("text,"), "{migration}");
    assert_eq!(
        migration.matches("not null").count(),
        3,
        "only the nullable component may lack `not null`: {migration}"
    );
    assert!(migration.contains("primary key (id)"), "{migration}");

    // The same column names the adapter selects and inserts.
    let adapter = fs::read_to_string(
        root.join("src/main/java/com/example/demo/adapters/JdbcPayoutRepository.java"),
    )
    .unwrap();
    for column in ["id", "amount", "paid_at", "note"] {
        assert!(migration.contains(column), "migration missing {column}");
        assert!(adapter.contains(column), "adapter missing {column}");
    }
}

#[test]
fn a_project_without_a_migration_directory_gets_no_migration() {
    let root = temp_dir("scaffold-no-migration");
    write_spring_fixture(&root);

    let output = jails_cmd(&root, None)
        .args(["generate", "scaffold", "Payout", "id:uuid@pk"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(!root.join("src/main/resources/db/migration").exists());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("created migration"), "{stdout}");
}

#[test]
fn pretend_writes_nothing_but_still_reports_the_whole_plan() {
    let root = temp_dir("pretend");
    write_spring_fixture(&root);

    let output = jails_cmd(&root, None)
        .args(["generate", "scaffold", "Payout", "id:uuid@pk", "--pretend"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("create "), "{stdout}");
    assert!(stdout.contains("nothing was written"), "{stdout}");
    assert!(
        !root
            .join("src/main/java/com/example/demo/domain/Payout.java")
            .exists()
    );
}

#[test]
fn pretend_is_global_and_reaches_destroy_too() {
    let root = temp_dir("pretend-destroy");
    write_spring_fixture(&root);
    let created = jails_cmd(&root, None)
        .args(["generate", "record", "Payout", "id:uuid"])
        .output()
        .unwrap();
    assert!(created.status.success());
    let file = root.join("src/main/java/com/example/demo/domain/Payout.java");
    assert!(file.is_file());

    let output = jails_cmd(&root, None)
        .args(["destroy", "record", "Payout", "--pretend"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("delete "), "{stdout}");
    // --pretend must not stop to ask for confirmation: nothing is at risk.
    assert!(!stdout.contains("proceed?"), "{stdout}");
    assert!(file.is_file(), "--pretend deleted a file");
}

#[test]
fn a_scaffold_writes_a_two_row_fixture_keyed_by_column_name() {
    let root = temp_dir("scaffold-fixture");
    write_spring_fixture(&root);
    // `new`/`new-cli` seed this directory; `add testkit` writes the loader
    // that reads it.
    fs::create_dir_all(root.join("src/test/resources/fixtures")).unwrap();

    let status = jails_cmd(&root, None)
        .args(["generate", "enum", "Currency", "GBP", "USD"])
        .status()
        .unwrap();
    assert!(status.success());

    let output = jails_cmd(&root, None)
        .args([
            "generate",
            "scaffold",
            "Payout",
            "id:uuid@pk",
            "paidAt:instant",
            "currency:Currency",
            "note:string?",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");

    let fixture =
        fs::read_to_string(root.join("src/test/resources/fixtures/payouts.json")).unwrap();
    // Column names, not component names -- the fixture describes what the
    // database holds, next to a JDBC adapter that reads those same columns.
    assert!(fixture.contains("\"paid_at\""), "{fixture}");
    assert!(!fixture.contains("paidAt"), "{fixture}");
    // A real constant read off the generated enum, not a guess.
    assert!(fixture.contains("\"currency\": \"GBP\""), "{fixture}");
    // Two rows, and the nullable one is absent in the second.
    assert!(fixture.contains("\"note\": \"sample-1\""), "{fixture}");
    assert!(fixture.contains("\"note\": null"), "{fixture}");
}

#[test]
fn a_project_without_a_fixtures_directory_gets_no_fixture() {
    let root = temp_dir("scaffold-no-fixture");
    write_spring_fixture(&root);

    let output = jails_cmd(&root, None)
        .args(["generate", "scaffold", "Payout", "id:uuid@pk"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("created fixture"), "{stdout}");
}

#[test]
fn the_generated_controller_test_uses_the_assertj_mockmvc_entry_point() {
    // Spring Framework 7 / Boot 4 favour MockMvcTester over plain MockMvc:
    // one fluent chain instead of two families of static imports, and no
    // `throws Exception` on the test method.
    let root = temp_dir("controller-test-style");
    write_spring_fixture(&root);

    let status = jails_cmd(&root, None)
        .args(["generate", "controller", "Payout"])
        .status()
        .unwrap();
    assert!(status.success());

    let test = fs::read_to_string(
        root.join("src/test/java/com/example/demo/web/PayoutControllerTest.java"),
    )
    .unwrap();
    assert!(
        test.contains("org.springframework.test.web.servlet.assertj.MockMvcTester"),
        "{test}"
    );
    assert!(test.contains("assertThat(mvc.get().uri("), "{test}");
    assert!(test.contains("hasStatusOk()"), "{test}");
    // The old style would need these; the new one does not.
    assert!(!test.contains("MockMvcResultMatchers"), "{test}");
    assert!(!test.contains("throws Exception"), "{test}");
}

#[test]
fn generate_dto_client_and_job_compile_and_pass_against_real_spring() {
    // These target Spring Boot 4 / Framework 7 APIs that moved recently
    // (@ImportHttpServices, ProblemDetail, MockMvcTester), so a unit test on
    // the template text proves nothing worth knowing. javac and the real
    // context are the check.
    if !real_mvn_available() {
        skip("mvn not found on PATH");
        return;
    }
    let path = real_path_without_mvnd();
    let root = temp_dir("real-spring-generators");
    write_spring_fixture(&root);

    // A domain record for the DTO to describe.
    assert!(
        jails_cmd_with_path(&root, &path)
            .args([
                "generate",
                "record",
                "Payout",
                "id:uuid",
                "amount:long",
                "note:string?"
            ])
            .status()
            .unwrap()
            .success()
    );

    for args in [
        vec!["generate", "dto", "Payout"],
        vec!["generate", "client", "Billing"],
        vec!["generate", "job", "Sweep"],
    ] {
        assert!(
            jails_cmd_with_path(&root, &path)
                .args(&args)
                .status()
                .unwrap()
                .success(),
            "{args:?} failed"
        );
    }

    let request =
        fs::read_to_string(root.join("src/main/java/com/example/demo/web/PayoutRequest.java"))
            .unwrap();
    // Constraints come from the field spec, so a bad request is rejected at
    // the edge rather than deep in the domain.
    assert!(request.contains("@NotNull UUID id"), "{request}");
    // An Optional domain component is a plain nullable field on the wire, and
    // carries no constraint -- `?` said it was optional.
    assert!(request.contains("String note"), "{request}");
    assert!(!request.contains("@NotNull String note"), "{request}");
    assert!(request.contains("Optional.ofNullable(note)"), "{request}");

    let client =
        fs::read_to_string(root.join("src/main/java/com/example/demo/clients/BillingClient.java"))
            .unwrap();
    assert!(client.contains("@GetExchange"), "{client}");
    // No base URL in the annotation: it belongs to the group's configuration.
    assert!(!client.contains("@HttpExchange(url"), "{client}");
    // One registration per client, listed by type: a shared one carries a
    // single group name, so a second client took the first's configuration
    // with it. missing.md M13.
    let config = fs::read_to_string(
        root.join("src/main/java/com/example/demo/clients/BillingClientConfig.java"),
    )
    .unwrap();
    assert!(
        config.contains("@ImportHttpServices(group = \"billing\", types = BillingClient.class)"),
        "{config}"
    );

    let job =
        fs::read_to_string(root.join("src/main/java/com/example/demo/jobs/SweepJob.java")).unwrap();
    // fixedDelay, not fixedRate: a slow run must not queue another on top.
    assert!(job.contains("fixedDelayString"), "{job}");
    // An exception escaping a @Scheduled method cancels every future run.
    assert!(job.contains("catch (RuntimeException"), "{job}");

    let verified = verified_spring_toolbox(&path);
    assert!(verified.join("target/test-classes").is_dir());
}

#[test]
fn generating_an_integration_test_also_configures_something_to_run_it() {
    // Surefire runs `*Test`; `*IT` belongs to Failsafe, and Failsafe is not
    // part of the Spring Boot parent's default build. Without this, every
    // generated IT is dead code and `mvn verify` still reports success --
    // a test that silently does not run is worse than no test, because the
    // green build claims it passed.
    let root = temp_dir("failsafe");
    write_spring_fixture(&root);
    assert!(
        !fs::read_to_string(root.join("pom.xml"))
            .unwrap()
            .contains("failsafe")
    );

    assert!(
        jails_cmd(&root, None)
            .args(["generate", "integration-test", "Payment"])
            .status()
            .unwrap()
            .success()
    );

    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    assert!(pom.contains("maven-failsafe-plugin"), "{pom}");
    // Both goals: `integration-test` runs them, `verify` is what makes a
    // failure fail the build. Binding only the first ignores the result.
    assert!(pom.contains("<goal>integration-test</goal>"), "{pom}");
    assert!(pom.contains("<goal>verify</goal>"), "{pom}");
    // No version: the Spring Boot parent manages it, and pinning one here
    // would drift from the platform.
    let plugin_block = &pom[pom.find("maven-failsafe-plugin").unwrap()..];
    let block_end = plugin_block.find("</plugin>").unwrap();
    assert!(!plugin_block[..block_end].contains("<version>"), "{pom}");

    // Idempotent: a second IT must not splice a second plugin block.
    assert!(
        jails_cmd(&root, None)
            .args(["generate", "integration-test", "Refund"])
            .status()
            .unwrap()
            .success()
    );
    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    assert_eq!(pom.matches("maven-failsafe-plugin").count(), 1, "{pom}");
}

#[test]
fn generate_help_documents_the_field_syntax_at_the_point_of_typing() {
    // The field grammar is the thing you need while typing the command, and
    // it lived only in the README. clap reflows a doc comment into one
    // paragraph unless told not to, which turns the table and the examples
    // into a run-on -- so the formatting is worth asserting, not just the
    // presence of the words.
    let workdir = temp_dir("generate-help");
    let output = jails_cmd(&workdir, None)
        .args(["generate", "--help"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let help = String::from_utf8_lossy(&output.stdout);

    assert!(help.contains("name:string!"), "{help}");
    assert!(help.contains("name:string?"), "{help}");
    assert!(help.contains("Case is the rule"), "{help}");
    assert!(help.contains("list<string>"), "{help}");
    assert!(help.contains("decimal, duration"), "{help}");
    assert!(help.contains("The aliases timestamp"), "{help}");
    assert!(help.contains("bigdecimal, and zoneid"), "{help}");
    // Line breaks survived: the table is indented lines, not one paragraph.
    assert!(help.contains("\n  name:string      required"), "{help}");
    assert!(help.contains("\n  jails g sealed Outcome"), "{help}");
    // Every kind carries a description rather than a bare name.
    assert!(help.contains("- scaffold:"), "{help}");
    assert!(help.contains("- sealed:"), "{help}");
}

/// The tier that answers what the tool is for. A strategy's interface,
/// implementations and tests have to agree on one method signature across
/// five files, and a mismatch is a compile error the user did not write.
///
/// Both modes are covered in one project because they generate different
/// signatures: `--yields` returns `Optional<T>`, a bare strategy `boolean`.
/// A generator whose whole subject is an outbound HTTP call must not leave the
/// call unbounded.
///
/// modern.md §13.6. A real generated client had no base URL, no connect
/// timeout, no read timeout, no retry and no auth --
/// `grep timeout application.properties` found only Hikari's. `backend.md` §1
/// makes this the fourth of five reflexes and admits no exceptions, and this
/// is the one place it cannot be left to the reader. The prefix and both
/// timeout keys are `HttpClientProperties extends HttpClientSettingsProperties`
/// in `spring-boot-http-client`, checked in `deps/spring-boot` at v4.0.0.
#[test]
fn generate_client_bounds_the_call_it_generates() {
    let root = temp_dir("client-timeouts");
    write_spring_fixture(&root);
    assert!(
        jails_cmd(&root, None)
            .args(["g", "client", "OpenAiChat"])
            .status()
            .unwrap()
            .success()
    );
    let properties =
        fs::read_to_string(root.join("src/main/resources/application.properties")).unwrap();
    for key in [
        "spring.http.serviceclient.open-ai-chat.base-url=",
        "spring.http.serviceclient.open-ai-chat.connect-timeout=",
        "spring.http.serviceclient.open-ai-chat.read-timeout=",
    ] {
        assert!(properties.contains(key), "{properties}");
    }
    // The same placeholder convention `add cors` uses: reserved by RFC 2606,
    // so it can never resolve and is unmistakably a value to replace. The
    // alternative failure is a first call dying on `URI with undefined
    // scheme`, which says nothing about a missing setting.
    assert!(
        properties.contains("https://example.invalid"),
        "{properties}"
    );
}

/// `add api` installed an error model nothing threw.
///
/// modern.md §6.1, §11.7 and §13.10: a sealed `ApiException`, an exhaustive
/// handler with no `default` arm, RFC 9457 responses and forty lines of
/// Javadoc explaining the exhaustiveness -- unreachable in **0 of 7** real
/// projects, while the one operation with real failure modes hand-rolled its
/// own status mapping with `ResponseStatusException` and bypassed the whole
/// thing. A reader finds `ApiException`, believes it is the error model, and
/// is wrong.
///
/// Both branches matter: without `add api` the class does not exist, so the
/// other rendering is not a tidiness fallback -- it is the only one that
/// compiles. Same rule `repository_wiring` follows for `JdbcClient`.
#[test]
fn a_transition_throws_into_the_error_model_the_project_installed() {
    let with_api = temp_dir("transition-api-error-model");
    write_spring_fixture(&with_api);
    let build = |root: &std::path::Path, api: bool| {
        if api {
            assert!(
                jails_cmd(root, None)
                    .args(["add", "api", "--no-start"])
                    .status()
                    .unwrap()
                    .success()
            );
        }
        for args in [
            &[
                "g",
                "scaffold",
                "Msg",
                "id:uuid@pk",
                "body:string!",
                "isRead:boolean",
                "version:long",
            ][..],
            &[
                "g",
                "transition",
                "MarkRead",
                "id:uuid",
                "isRead:boolean",
                "version:long",
                "--on",
                "Msg",
            ][..],
        ] {
            let out = jails_cmd(root, None).args(args).output().unwrap();
            assert!(out.status.success(), "{out:?}");
        }
        fs::read_to_string(root.join("src/main/java/com/example/demo/web/MarkReadController.java"))
            .unwrap()
    };

    let installed = build(&with_api, true);
    assert!(
        installed.contains("import com.example.demo.api.ApiException;"),
        "{installed}"
    );
    assert!(
        installed.contains("throw new ApiException.NotFound("),
        "{installed}"
    );
    assert!(
        installed.contains("throw new ApiException.Conflict("),
        "{installed}"
    );
    // Neither *outcome* becomes a `ResponseStatusException` where the project
    // has an error model.
    assert!(
        !installed.contains("ResponseStatusException(NOT_FOUND"),
        "{installed}"
    );
    assert!(
        !installed.contains("ResponseStatusException(CONFLICT"),
        "{installed}"
    );
    // It survives for one thing, and only that: a malformed `If-Match` is a
    // 400 -- jails could not read the request -- while every `ApiException`
    // variant is about a request it read. plan.md P4.5.
    assert!(
        installed.contains("If-Match is not a version this resource issued"),
        "{installed}"
    );

    let without = temp_dir("transition-no-error-model");
    write_spring_fixture(&without);
    let plain = build(&without, false);
    assert!(!plain.contains("ApiException"), "{plain}");
    assert!(plain.contains("PRECONDITION_FAILED"), "{plain}");
}

#[test]
fn generate_strategy_produces_a_project_that_compiles_and_passes_tests() {
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
    let workdir = temp_dir("real-strategy-compiles");
    jails_cmd_with_path(&workdir, &path)
        .args(["new-cli", "demo"])
        .status()
        .unwrap();
    let root = workdir.join("demo");

    // The types the generated signature names. Without them the strategy
    // would not compile, which is what the note at generation time says.
    for record in ["Transaction", "Reward"] {
        assert!(
            jails_cmd_with_path(&root, &path)
                .args(["g", "record", record, "id:uuid", "amount:long"])
                .status()
                .unwrap()
                .success()
        );
    }

    assert!(
        jails_cmd_with_path(&root, &path)
            .args([
                "g",
                "strategy",
                "RewardRule",
                "Coffee",
                "LargeTransaction",
                "--on",
                "Transaction",
                "--yields",
                "Reward",
            ])
            .status()
            .unwrap()
            .success()
    );
    // The predicate mode, whose method returns `boolean` rather than Optional.
    assert!(
        jails_cmd_with_path(&root, &path)
            .args([
                "g",
                "strategy",
                "Eligibility",
                "Domestic",
                "--on",
                "Transaction"
            ])
            .status()
            .unwrap()
            .success()
    );

    // The port is framework-free and stays in `domain`; the beans that
    // implement it carry `@Component` on Spring and live a layer up, so the
    // ArchUnit rule `g scaffold` writes and the annotation this pattern needs
    // no longer contradict each other. The placement is the same on plain
    // Maven, where there is neither -- one layout is easier to explain than
    // one that depends on the build file.
    let verified = verified_plain_toolbox(&path);
    for (layer, class) in [
        ("domain", "Eligibility"),
        ("service", "DomesticEligibility"),
    ] {
        assert!(
            verified
                .join(format!(
                    "target/classes/com/example/demo/{layer}/{class}.class"
                ))
                .is_file(),
            "shared toolchain matrix did not compile {class}"
        );
    }
}

/// `destroy strategy` reads the implementations back off disk rather than
/// rebuilding a variant list it was never given, so it takes out every class
/// implementing the interface -- including one added by hand afterwards.
/// Leaving that behind implementing a deleted interface stops the project
/// compiling, which is the failure the generate/destroy inverse rule exists
/// to prevent.
#[test]
fn destroy_strategy_removes_the_implementations_it_did_not_name() {
    let root = temp_dir("destroy-strategy");
    write_plain_fixture(&root);

    assert!(
        jails_cmd(&root, None)
            .args([
                "g",
                "strategy",
                "RewardRule",
                "Coffee",
                "--on",
                "Transaction"
            ])
            .status()
            .unwrap()
            .success()
    );

    // An implementation the generate call never knew about.
    let domain = root.join("src/main/java/com/example/demo/domain");
    fs::write(
        domain.join("HandWrittenRewardRule.java"),
        "package com.example.demo.domain;\n\n\
         public final class HandWrittenRewardRule implements RewardRule {\n\
         \x20   @Override\n\
         \x20   public boolean matches(Transaction transaction) {\n\
         \x20       return false;\n\
         \x20   }\n\
         }\n",
    )
    .unwrap();

    assert!(
        jails_cmd(&root, None)
            .args(["destroy", "strategy", "RewardRule", "--force"])
            .status()
            .unwrap()
            .success()
    );

    assert!(!domain.join("RewardRule.java").exists());
    assert!(!domain.join("CoffeeRewardRule.java").exists());
    assert!(
        !domain.join("HandWrittenRewardRule.java").exists(),
        "an implementation of a deleted interface was left behind"
    );
}

/// `--pretend` has to name every write. `package-info.java` was created as a
/// side effect of writing a class, so the preview listed two files and the
/// real run wrote three -- on the one command whose entire job is to tell you
/// what is about to happen.
///
/// The fix is that the preview and the apply consume the same list, rather
/// than the preview learning to predict a side effect: a second piece of code
/// guessing what the first will do is the drift this costs everywhere else.
#[test]
fn pretend_names_the_package_info_it_will_write() {
    let root = temp_dir("pkginfo-preview");
    write_plain_fixture(&root);
    // package-info is conditional on the annotation resolving.
    let pom = fs::read_to_string(root.join("pom.xml")).unwrap().replace(
        "</dependencies>",
        "<dependency><groupId>org.jspecify</groupId>\
         <artifactId>jspecify</artifactId><version>1.0.0</version></dependency>\
         </dependencies>",
    );
    fs::write(root.join("pom.xml"), pom).unwrap();

    let preview = jails_cmd(&root, None)
        .args(["g", "record", "Note", "title:string", "--pretend"])
        .output()
        .unwrap();
    assert!(preview.status.success());
    let shown = String::from_utf8_lossy(&preview.stdout);
    assert!(
        shown.contains("package-info"),
        "the preview hid a file the real run writes:\n{shown}"
    );

    let real = jails_cmd(&root, None)
        .args(["g", "record", "Note", "title:string"])
        .output()
        .unwrap();
    assert!(real.status.success());
    let done = String::from_utf8_lossy(&real.stdout);

    // The preview and the run must name the same set of files.
    let files = |text: &str| -> Vec<String> {
        text.lines()
            .filter_map(|l| l.rsplit_once(' ').map(|(_, p)| p.to_string()))
            .filter(|p| p.ends_with(".java"))
            .collect()
    };
    assert_eq!(
        files(&shown),
        files(&done),
        "preview and apply disagreed about what would be written"
    );
    assert!(
        root.join("src/main/java/com/example/demo/domain/package-info.java")
            .is_file()
    );
}

/// One per package, not one per class -- `scaffold` puts several classes in
/// the same package -- and never in the test tree, where a nullness contract
/// buys nothing.
#[test]
fn planned_package_infos_are_one_per_package() {
    let root = temp_dir("pkginfo-dedup");
    write_spring_fixture(&root);
    let pom = fs::read_to_string(root.join("pom.xml")).unwrap().replace(
        "</dependencies>",
        "<dependency><groupId>org.jspecify</groupId>\
         <artifactId>jspecify</artifactId><version>1.0.0</version></dependency>\
         </dependencies>",
    );
    fs::write(root.join("pom.xml"), pom).unwrap();

    let preview = jails_cmd(&root, None)
        .args([
            "g",
            "scaffold",
            "Task",
            "id:uuid@pk",
            "title:string",
            "--pretend",
        ])
        .output()
        .unwrap();
    assert!(preview.status.success());
    let shown = String::from_utf8_lossy(&preview.stdout);

    let infos: Vec<&str> = shown
        .lines()
        .filter(|l| l.contains("package-info"))
        .collect();
    assert!(
        infos.len() > 1,
        "scaffold should span several packages:\n{shown}"
    );

    // No package planned twice, however many classes land in it.
    let mut seen = std::collections::HashSet::new();
    for line in &infos {
        assert!(
            seen.insert(*line),
            "the same package-info was planned twice:\n{shown}"
        );
    }

    // Never in the test tree.
    assert!(
        !infos.iter().any(|l| l.contains("src/test/java")),
        "{shown}"
    );
}

/// `plan.md` §6.6 tier 2. The want is "change what the generated code *looks
/// like*" -- not a new generator, just this class shaped differently.
#[test]
fn a_project_template_override_replaces_the_built_in_and_doctor_names_it() {
    let root = temp_dir("template-override");
    write_plain_fixture(&root);

    let overrides = root.join(".jails/templates/generate");
    fs::create_dir_all(&overrides).unwrap();
    let built_in = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("templates/generate/command_test.java"),
    )
    .unwrap();
    // Same placeholders, different shape: the contract is the placeholder set,
    // not the text.
    fs::write(
        overrides.join("command_test.java"),
        format!("// generated by an overridden template\n{built_in}"),
    )
    .unwrap();

    let output = jails_cmd(&root, None)
        .args(["generate", "command", "Sync"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let generated =
        fs::read_to_string(root.join("src/test/java/com/example/demo/cli/SyncCommandTest.java"))
            .unwrap();
    assert!(
        generated.starts_with("// generated by an overridden template"),
        "{generated}"
    );
    assert!(generated.contains("class SyncCommandTest"), "{generated}");

    // The honesty half: an overridden template is not golden-tested, so
    // `doctor` names it rather than letting the reader find out from a build.
    let doctor = jails_cmd(&root, None).arg("doctor").output().unwrap();
    let report = String::from_utf8_lossy(&doctor.stdout);
    assert!(
        report.contains("generate/command_test.java"),
        "doctor must name the override: {report}"
    );
    assert!(
        report.contains("not covered by jails' snapshot tests"),
        "{report}"
    );
}

/// The placeholder set is the contract, and breaking it is the reader's typo --
/// so it is an error naming their file, not a panic naming jails'.
#[test]
fn a_template_override_missing_a_placeholder_is_refused_by_name() {
    let root = temp_dir("template-override-bad");
    write_plain_fixture(&root);

    let overrides = root.join(".jails/templates/generate");
    fs::create_dir_all(&overrides).unwrap();
    fs::write(
        overrides.join("command_test.java"),
        "package {{pkg}};\n\nclass Whatever {}\n",
    )
    .unwrap();

    let output = jails_cmd(&root, None)
        .args(["generate", "command", "Sync"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("command_test.java"), "{stderr}");
    assert!(stderr.contains("missing:"), "{stderr}");
    assert!(
        !root
            .join("src/test/java/com/example/demo/cli/SyncCommandTest.java")
            .exists(),
        "nothing is written when the override is refused"
    );
}

/// The generated *code* had a Boot floor the build file does not, and every
/// generator that hits it now writes the classic `MockMvc` form.
///
/// `pending.md` §1.2. Nine companion tests are written against `MockMvcTester`
/// (Spring Framework 6.2, Boot 3.4); two had a classic variant and seven
/// refused. The refusal was the right failure and the wrong feature —
/// `jails new --gradle --boot 2.7.18` exists so that a Boot 2 project can be
/// worked in — so all nine pick their form by version now.
#[test]
fn a_boot_2_project_gets_the_classic_mockmvc_for_every_generated_web_test() {
    let parent = temp_dir("gradle-boot2-generators");
    let created = jails_cmd(&parent, None)
        .args([
            "new",
            "svc",
            "--gradle",
            "--offline",
            "--boot",
            "2.7.18",
            "--java",
            "21",
            "--package",
            "com.acme.svc",
            "--no-devtools",
            "--no-git",
        ])
        .output()
        .unwrap();
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    let root = parent.join("svc");

    let generated = jails_cmd(&root, None)
        .args(["g", "controller", "Foo", "--method", "post"])
        .output()
        .unwrap();
    assert!(
        generated.status.success(),
        "{}{}",
        String::from_utf8_lossy(&generated.stdout),
        String::from_utf8_lossy(&generated.stderr)
    );
    let test =
        fs::read_to_string(root.join("src/test/java/com/acme/svc/web/FooControllerTest.java"))
            .unwrap();
    // MockMvcTester is Spring Framework 6.2; this project is Framework 5.3.
    // Asserted on the import rather than the word: the classic templates name
    // the type in a comment saying why it is not used.
    assert!(!test.contains("servlet.assertj.MockMvcTester"), "{test}");
    assert!(
        test.contains("import org.springframework.test.web.servlet.MockMvc;"),
        "{test}"
    );
    assert!(test.contains("mvc.perform(post(\"/foo\"))"), "{test}");
    assert!(test.contains("throws Exception"), "{test}");

    // `add cors` has a classic variant too.
    let cors = jails_cmd(&root, None)
        .args(["add", "cors", "--no-start"])
        .output()
        .unwrap();
    assert!(
        cors.status.success(),
        "{}",
        String::from_utf8_lossy(&cors.stderr)
    );
    let cors_test =
        fs::read_to_string(root.join("src/test/java/com/acme/svc/CorsConfigTest.java")).unwrap();
    assert!(
        !cors_test.contains("servlet.assertj.MockMvcTester"),
        "{cors_test}"
    );
    assert!(cors_test.contains("andExpect(status()"), "{cors_test}");

    // `add h2` must not declare a Boot 4 module or write a Boot 4 property
    // name. The first is unresolvable; the second is worse, because nothing
    // rejects it and exception translation simply stays on.
    let h2 = jails_cmd(&root, None)
        .args(["add", "h2", "--no-start"])
        .output()
        .unwrap();
    assert!(
        h2.status.success(),
        "{}",
        String::from_utf8_lossy(&h2.stderr)
    );
    let build = fs::read_to_string(root.join("build.gradle")).unwrap();
    assert!(
        !build.contains("spring-boot-h2console"),
        "Boot 4 module: {build}"
    );
    let properties =
        fs::read_to_string(root.join("src/main/resources/application.properties")).unwrap();
    assert!(
        properties.contains("spring.dao.exceptiontranslation.enabled=false"),
        "{properties}"
    );
    assert!(
        !properties.contains("spring.persistence.exceptiontranslation"),
        "{properties}"
    );

    // `g scaffold` used to refuse and writes the classic form now, and the
    // whole of what it generates compiles -- proved against real Maven by
    // `what_jails_generates_for_boot_2_compiles_and_what_cannot_refuses_by_name`.
    // `add api` and `add security` still refuse, for a reason that is not
    // about MockMvc at all: their *main* source set is Boot 3 code, which
    // `pending.md` §1.2's premise missed.
    let scaffolded = jails_cmd(&root, None)
        .args([
            "g",
            "scaffold",
            "Note",
            "id:uuid@pk",
            "title:string!",
            "version:long",
        ])
        .output()
        .unwrap();
    assert!(
        scaffolded.status.success(),
        "{}",
        String::from_utf8_lossy(&scaffolded.stderr)
    );
    let source =
        fs::read_to_string(root.join("src/test/java/com/acme/svc/web/NoteControllerTest.java"))
            .unwrap();
    assert!(
        !source.contains("servlet.assertj.MockMvcTester"),
        "the Framework 6.2 entry point: {source}"
    );
    assert!(
        source.contains("import org.springframework.test.web.servlet.MockMvc;"),
        "{source}"
    );
    assert!(source.contains("andExpect(status()"), "{source}");

    for (command, needs) in [
        (vec!["add", "api", "--no-start"], "ProblemDetail"),
        (vec!["add", "security", "--no-start"], "requestMatchers"),
    ] {
        let refused = jails_cmd(&root, None).args(&command).output().unwrap();
        let stderr = String::from_utf8_lossy(&refused.stderr);
        assert!(!refused.status.success(), "{command:?} should refuse");
        assert!(stderr.contains(needs), "{command:?}: {stderr}");
    }
}

/// `plan.md` §13.3's `g auth`. Both claims behind it are behavioural, so a
/// compile check would prove nothing: Boot auto-configures no `JwtEncoder`,
/// and `JwtTimestampValidator` accepts a token with no `exp` unless one line
/// says otherwise. The second is the reason `a_token_with_no_expiry_is_refused`
/// exists — delete that line and no other test notices.
#[test]
fn generate_auth_produces_tests_that_run_and_pass() {
    if !real_mvn_available() {
        skip("mvn not found on PATH");
        return;
    }
    let path = real_path_without_mvnd();
    let root = temp_dir("real-auth");
    write_spring_fixture(&root);

    let status = jails_cmd_with_path(&root, &path)
        .args(["add", "security", "--no-start"])
        .status()
        .unwrap();
    assert!(status.success(), "add security failed");

    let status = jails_cmd_with_path(&root, &path)
        .args(["generate", "auth", "Api"])
        .status()
        .unwrap();
    assert!(status.success(), "generate auth failed");

    let verified = verified_spring_services_toolbox(&path);
    assert_surefire_test_count(verified, "ApiTokensTest", 4);
}

/// Without Spring Security there is no filter chain to read the token, so the
/// encoder and decoder would be beans nothing consumes.
#[test]
fn generate_auth_refuses_without_the_security_capability() {
    let root = temp_dir("auth-no-security");
    write_spring_fixture(&root);

    let output = jails_cmd(&root, None)
        .args(["generate", "auth", "Api"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(stderr.contains("jails add security"), "{stderr}");
}

/// `plan.md` §13.3's `g webhook`. Seven tests, and each is one of the ways an
/// inbound webhook is normally trusted when it should not be — or rejected
/// when it should not be, which is the failure mode nobody predicts.
#[test]
fn generate_webhook_produces_tests_that_run_and_pass() {
    if !real_mvn_available() {
        skip("mvn not found on PATH");
        return;
    }
    let path = real_path_without_mvnd();
    let root = temp_dir("real-webhook");
    write_spring_fixture(&root);

    let status = jails_cmd_with_path(&root, &path)
        .args(["generate", "webhook", "Provider"])
        .status()
        .unwrap();
    assert!(status.success(), "generate webhook failed");

    let verified = verified_spring_services_toolbox(&path);
    assert_surefire_test_count(verified, "ProviderVerifierTest", 7);
}

/// `plan.md` §13.3's `g search`. The generated column is the whole point, and
/// the only thing that can prove the expression is right is PostgreSQL parsing
/// it — a Rust test on the string proves the string.
#[test]
fn generate_search_produces_a_project_that_compiles() {
    if !real_mvn_available() {
        skip("mvn not found on PATH");
        return;
    }
    let path = real_path_without_mvnd();
    let root = temp_dir("real-search");
    write_spring_fixture(&root);

    for step in [
        vec!["add", "db", "--no-start"],
        vec![
            "generate",
            "scaffold",
            "Article",
            "id:uuid@pk",
            "title:string!",
            "body:string",
        ],
        vec!["generate", "search", "Article", "title", "body"],
    ] {
        let output = jails_cmd_with_path(&root, &path)
            .args(&step)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "`{}` failed: {}{}",
            step.join(" "),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let verified = verified_spring_db_toolbox(&path);
    assert!(
        verified
            .join("target/classes/com/example/demo/adapters/JdbcArticleRepository.class")
            .is_file(),
        "the shared JDBC toolbox did not compile the search adapter"
    );
}

/// Indexing a non-text component is refused at generation time, where the
/// reader is, rather than at `flyway migrate` — which is the furthest possible
/// point from the mistake.
#[test]
fn generate_search_refuses_a_component_it_cannot_index() {
    let root = temp_dir("search-refusals");
    write_spring_fixture(&root);
    let status = jails_cmd(&root, None)
        .args(["add", "db", "--no-start"])
        .status()
        .unwrap();
    assert!(status.success());
    let status = jails_cmd(&root, None)
        .args([
            "generate",
            "scaffold",
            "Article",
            "id:uuid@pk",
            "views:long",
            "title:string!",
        ])
        .status()
        .unwrap();
    assert!(status.success());

    for (args, expected) in [
        (
            vec!["generate", "search", "Article"],
            "needs the components",
        ),
        (
            vec!["generate", "search", "Article", "views"],
            "full-text search indexes text",
        ),
        (
            vec!["generate", "search", "Article", "nosuch"],
            "has no component",
        ),
        (
            vec!["generate", "search", "Missing", "title"],
            "needs the record it searches",
        ),
    ] {
        let output = jails_cmd(&root, None).args(&args).output().unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success(), "{args:?} should refuse");
        assert!(stderr.contains(expected), "{args:?}: {stderr}");
    }
}

/// What jails generates for a real Boot 2 project compiles and passes, and
/// what cannot refuses by naming the type.
///
/// **This is the test `pending.md` §1.2 said the fix was waiting for, and it
/// contradicted the item.** §1.2 read the Boot floor as living in seven
/// generated *tests* written against `MockMvcTester`; the first real Boot
/// 2.7.18 compile said the tests were the smaller half. `add api` writes
/// `ProblemDetail` (Spring Framework 6), `add security` writes
/// `requestMatchers` (Spring Security 6), and `g query`/`g transition` write a
/// `JdbcClient` adapter (Framework 6.1) — all in the *main* source set, where
/// no test variant helps.
///
/// So the split is what compiles and what refuses, both asserted here:
/// `add cors`, `g enum`, `g scaffold` and `g usecase` are generated and run
/// through real `mvn test`; the other four must refuse and must say which type
/// is missing.
///
/// One project rather than four, which is also the arrangement that catches a
/// template compiling alone and colliding in company — two probe controllers
/// on one path, two classes with one name.
#[test]
fn what_jails_generates_for_boot_2_compiles_and_what_cannot_refuses_by_name() {
    if !real_mvn_available() {
        skip("mvn not found on PATH");
        return;
    }
    let path = real_path_without_mvnd();
    let root = temp_dir("real-boot2-classic");
    write_spring2_fixture(&root);

    // The same shape as the `usecase-query-transition` golden scenario, which
    // is what these three generators need: an enum, a scaffold declaring the
    // components they reference, and a `version` for optimistic locking.
    for command in [
        vec!["add", "cors", "--no-start"],
        vec!["g", "enum", "NoteStatus", "DRAFT", "PUBLISHED"],
        vec![
            "g",
            "scaffold",
            "Note",
            "id:uuid@pk",
            "title:string!",
            "status:NoteStatus@index",
            "version:long@nonnegative",
        ],
        vec![
            "g",
            "usecase",
            "DraftNote",
            "id:uuid",
            "title:string!",
            "--on",
            "Note",
        ],
    ] {
        let output = jails_cmd_with_path(&root, &path)
            .args(&command)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{command:?}: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // The four that cannot, and why the item's premise was incomplete: their
    // *main* source set is Boot 3 code, which no test variant can help. The
    // refusal names the type the compiler would have named.
    for (command, needs) in [
        (vec!["add", "api", "--no-start"], "ProblemDetail"),
        (vec!["add", "security", "--no-start"], "requestMatchers"),
        (
            vec![
                "g",
                "query",
                "NotesByStatus",
                "status:NoteStatus",
                "--on",
                "Note",
            ],
            "JdbcClient",
        ),
        (
            vec![
                "g",
                "transition",
                "ChangeNoteStatus",
                "id:uuid",
                "status:NoteStatus",
                "version:long@nonnegative",
                "--on",
                "Note",
            ],
            "JdbcClient",
        ),
    ] {
        let refused = jails_cmd_with_path(&root, &path)
            .args(&command)
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&refused.stderr);
        assert!(!refused.status.success(), "{command:?} should refuse");
        assert!(stderr.contains(needs), "{command:?}: {stderr}");
        assert!(stderr.contains("Spring Boot 2"), "{command:?}: {stderr}");
        assert!(stderr.contains("jails g scaffold"), "{command:?}: {stderr}");
    }

    // Not one of them may name the Framework 6.2 entry point.
    let mut checked = 0;
    let mut stack = vec![root.join("src/test/java")];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).unwrap() {
            let entry = entry.unwrap().path();
            if entry.is_dir() {
                stack.push(entry);
                continue;
            }
            let source = fs::read_to_string(&entry).unwrap();
            assert!(
                !source.contains("servlet.assertj.MockMvcTester"),
                "{}: MockMvcTester is Spring Framework 6.2, and this project is 5.3\n{source}",
                entry.display()
            );
            checked += 1;
        }
    }
    assert!(
        checked >= 4,
        "expected the generated tests plus the fixture's, found {checked}"
    );

    let output = std::process::Command::new("mvn")
        .current_dir(&root)
        .env("PATH", &path)
        .args(["-q", "-B", "test"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "mvn test failed on a Boot 2 project:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// A scaffold's `create table`, into a project whose schema is `schema.sql`.
///
/// The bug this closes was silent: the migration is conditional on
/// `src/main/resources/db/migration` existing, which is `add db`'s directory,
/// so `jails g scaffold` into a Spring project that initialises its datasource
/// with `spring.sql.init` wrote a repository, a JDBC adapter and an `IT`
/// against a table that does not exist -- and printed nothing. Found on
/// `minicom-15-01-2026/spring`, and verified fixed there: H2 2.x accepts the
/// block and the whole context starts.
///
/// It is a `codemod` marked block rather than an append, because that is what
/// makes `destroy` take out exactly the table jails wrote and leave the
/// reader's own tables alone.
#[test]
fn a_scaffold_writes_its_ddl_into_schema_sql_when_that_is_where_the_schema_lives() {
    let root = temp_dir("schema-sql-ddl");
    write_spring_fixture(&root);
    let resources = root.join("src/main/resources");
    fs::create_dir_all(&resources).unwrap();
    // No trailing newline, like the checkout this was found on: the opening
    // marker landed on the same line as `);` before the separator existed.
    fs::write(
        resources.join("schema.sql"),
        "create table users (\n  id bigint primary key\n);",
    )
    .unwrap();

    let output = jails_cmd(&root, None)
        .args(["g", "scaffold", "Ticket", "id:long@pk", "subject:string!"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let schema = fs::read_to_string(resources.join("schema.sql")).unwrap();
    assert!(schema.starts_with("create table users ("), "{schema}");
    // SQL comments, not `#`: a `# jails:` marker in a .sql file is a syntax
    // error rather than a comment in the wrong place.
    assert!(schema.contains("-- jails:create-tickets"), "{schema}");
    assert!(schema.contains("-- /jails:create-tickets"), "{schema}");
    assert!(schema.contains("create table tickets ("), "{schema}");
    assert!(schema.contains("\n-- jails:create-tickets"), "{schema}");
    // No Flyway here, so nothing was written there either.
    assert!(!root.join("src/main/resources/db/migration").exists());

    // The inverse: `destroy` takes out the block it wrote, and only that.
    let output = jails_cmd(&root, None)
        .args(["destroy", "scaffold", "Ticket", "--force"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let schema = fs::read_to_string(resources.join("schema.sql")).unwrap();
    assert!(!schema.contains("jails:create-tickets"), "{schema}");
    assert!(!schema.contains("create table tickets"), "{schema}");
    assert!(schema.contains("create table users ("), "{schema}");
}

/// A project with neither destination is told, rather than handed a resource
/// whose table nobody creates.
#[test]
fn a_scaffold_with_nowhere_to_put_ddl_says_so() {
    let root = temp_dir("schema-no-home");
    write_spring_fixture(&root);

    let output = jails_cmd(&root, None)
        .args(["g", "scaffold", "Ticket", "id:long@pk", "subject:string!"])
        .output()
        .unwrap();
    let report = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(output.status.success(), "{report}");
    assert!(
        report.contains("`create_tickets` was not written"),
        "{report}"
    );
    assert!(report.contains("jails add db"), "{report}");
    assert!(report.contains("schema.sql"), "{report}");
}

/// The wire contract, both directions, on the project that needs it.
///
/// `missing.md` M15's other half. Spring's **data binder** has no naming
/// strategy: `spring.jackson.property-naming-strategy` configures Jackson, so
/// a project whose JSON is `user_id` still binds a form field called `userId`
/// unless the record's component says otherwise. Two names for one value on
/// one wire, and it is silent -- the component simply arrives null.
///
/// Verified against a running server before it was written down: a form post
/// of `user_id=42` at a generated `--consumes form` endpoint comes back as
/// `{"id":1,"user_id":42,"status":"open"}`, which is the shape
/// `minicom-15-01-2026`'s two pages read.
#[test]
fn a_form_bound_record_answers_to_the_names_this_projects_wire_actually_uses() {
    let root = temp_dir("wire-naming");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join("src/main/resources")).unwrap();
    fs::write(root.join("src/main/resources/schema.sql"), "-- schema\n").unwrap();

    let generate = |root: &std::path::Path| {
        let status = jails_cmd(root, None)
            .args([
                "g",
                "scaffold",
                "Ticket",
                "id:long@pk",
                "userId:long",
                "subject:string!",
            ])
            .status()
            .unwrap();
        assert!(status.success());
        let output = jails_cmd(root, None)
            .args([
                "g",
                "usecase",
                "OpenTicket",
                "userId:long",
                "subject:string!",
                "--on",
                "Ticket",
                "--consumes",
                "form",
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        fs::read_to_string(
            root.join("src/main/java/com/example/demo/service/OpenTicketCommand.java"),
        )
        .unwrap()
    };

    // A project that says nothing about naming gets what it always got.
    let plain = generate(&root);
    assert!(!plain.contains("BindParam"), "{plain}");
    assert!(plain.contains("long userId"), "{plain}");

    // The same generation, in a project whose wire is snake_case.
    let snake = temp_dir("wire-naming-snake");
    write_spring_fixture(&snake);
    fs::create_dir_all(snake.join("src/main/resources")).unwrap();
    fs::write(snake.join("src/main/resources/schema.sql"), "-- schema\n").unwrap();
    let status = jails_cmd(&snake, None)
        .args(["set", "spring.jackson.property-naming-strategy=SNAKE_CASE"])
        .status()
        .unwrap();
    assert!(status.success());
    let bound = generate(&snake);
    assert!(
        bound.contains("@BindParam(\"user_id\") long userId"),
        "{bound}"
    );
    assert!(
        bound.contains("import org.springframework.web.bind.annotation.BindParam;"),
        "{bound}"
    );
    // Only where the two spellings differ: an annotation restating the default
    // is noise in every record with a one-word component.
    assert!(!bound.contains("@BindParam(\"subject\")"), "{bound}");
}

/// `missing.md` M14: the wire value, in the three shapes one real project
/// needed and `g enum` could spell none of.
///
/// The Java name and the wire value are two different things, and treating
/// them as one fails quietly: an enum whose constants are `OPEN` and
/// `IN_PROGRESS` serialises as `"OPEN"`, the page reads `"open"`, and the
/// badge is simply blank.
///
/// Proved against a running server before it was written down: a form
/// carrying `status=open` binds, the response carries `"status":"open"`, and
/// `status=nope` is a 400 rather than a null.
#[test]
fn an_enum_constant_can_be_called_something_else_on_the_wire() {
    let root = temp_dir("enum-wire");
    write_spring_fixture(&root);

    let output = jails_cmd(&root, None)
        .args([
            "g",
            "enum",
            "IssueStatus",
            "OPEN=open",
            "IN_PROGRESS=in_progress",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let source =
        fs::read_to_string(root.join("src/main/java/com/example/demo/domain/IssueStatus.java"))
            .unwrap();
    assert!(source.contains("OPEN(\"open\")"), "{source}");
    assert!(source.contains("IN_PROGRESS(\"in_progress\")"), "{source}");
    // The annotations are Jackson's and stayed at the 2.x package even in
    // Jackson 3 -- databind's own pom says so.
    assert!(
        source.contains("import com.fasterxml.jackson.annotation.JsonValue;"),
        "{source}"
    );
    assert!(source.contains("@JsonCreator"), "{source}");
    // The refusal lists what it would have taken.
    assert!(
        source.contains("expected one of open, in_progress"),
        "{source}"
    );

    // `@JsonValue` covers a JSON body and nothing else: a form field, a path
    // variable and a query parameter go through Spring's conversion service,
    // whose enum converter calls `valueOf`.
    let converter = fs::read_to_string(
        root.join("src/main/java/com/example/demo/web/IssueStatusConverter.java"),
    )
    .unwrap();
    assert!(
        converter.contains("Converter<String, IssueStatus>"),
        "{converter}"
    );
    assert!(
        converter.contains("IssueStatus.fromWire(source)"),
        "{converter}"
    );

    // The generated test asserts the round trip rather than restating it.
    let test =
        fs::read_to_string(root.join("src/test/java/com/example/demo/domain/IssueStatusTest.java"))
            .unwrap();
    assert!(test.contains("roundTripsEveryWireValue"), "{test}");
    assert!(test.contains("rejectsAnUnknownWireValue"), "{test}");

    // An enum whose constants are called their own names is byte-identical to
    // what it always was: no constructor, no annotations, no converter.
    let plain = temp_dir("enum-plain");
    write_spring_fixture(&plain);
    let status = jails_cmd(&plain, None)
        .args(["g", "enum", "Currency", "GBP", "EUR"])
        .status()
        .unwrap();
    assert!(status.success());
    let source =
        fs::read_to_string(plain.join("src/main/java/com/example/demo/domain/Currency.java"))
            .unwrap();
    assert!(source.contains("    GBP,\n    EUR\n}"), "{source}");
    assert!(!source.contains("JsonValue"), "{source}");
    assert!(
        !plain
            .join("src/main/java/com/example/demo/web/CurrencyConverter.java")
            .exists()
    );
}

/// A path that names its filters, and the two refusals that keep it honest.
///
/// `--path /admin_api/messages/{userId}` used to be accepted as *text*: the
/// controller carried the template in `@RequestMapping` and declared no
/// `@PathVariable`, so Spring matched the URL and then looked for a request
/// body nobody sent. A path jails cannot honour is a path jails must not
/// accept.
///
/// All-or-none, deliberately. A mix would need the controller to build the
/// criteria from a partial body plus some path variables, and "which half came
/// from where" is a rule nobody would remember.
#[test]
fn a_query_path_may_address_its_filters_by_name() {
    let root = temp_dir("query-path");
    write_spring_fixture(&root);
    let status = jails_cmd(&root, None)
        .args([
            "g",
            "scaffold",
            "Ticket",
            "id:long@pk",
            "userId:long",
            "subject:string!",
        ])
        .status()
        .unwrap();
    assert!(status.success());

    let output = jails_cmd(&root, None)
        .args([
            "g",
            "query",
            "TicketsFor",
            "userId:long",
            "--on",
            "Ticket",
            "--path",
            "/admin_api/tickets/{userId}",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let controller = fs::read_to_string(
        root.join("src/main/java/com/example/demo/web/TicketsForQueryController.java"),
    )
    .unwrap();
    // A GET with no body, and the variable actually bound.
    assert!(controller.contains("@GetMapping"), "{controller}");
    assert!(
        controller.contains("execute(@PathVariable long userId)"),
        "{controller}"
    );
    assert!(
        controller.contains("new TicketsForCriteria(userId)"),
        "{controller}"
    );
    assert!(!controller.contains("RequestBody"), "{controller}");

    // A variable that names no filter goes nowhere, so it is refused.
    let output = jails_cmd(&root, None)
        .args([
            "g",
            "query",
            "Bad",
            "userId:long",
            "--on",
            "Ticket",
            "--path",
            "/x/{nope}",
        ])
        .output()
        .unwrap();
    let error = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(
        error.contains("`{nope}`, which is not one of its filters"),
        "{error}"
    );

    // A mix is refused naming the ones that would have had nowhere to come
    // from.
    let output = jails_cmd(&root, None)
        .args([
            "g",
            "query",
            "Mixed",
            "userId:long",
            "subject:string!",
            "--on",
            "Ticket",
            "--path",
            "/x/{userId}",
        ])
        .output()
        .unwrap();
    let error = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(
        error.contains("subject would have to come from a request body"),
        "{error}"
    );
}

/// The dependency reader on a Gradle project, and what its blindness cost.
///
/// `Project::has_dependency` read the cached build file **as XML** whatever
/// the build tool was, so on Gradle it answered a confident *no* to every
/// question. The consequence was not small: `repository_wiring` gave the
/// scaffold the in-memory adapter as its `@Component` while a generated query
/// kept its JDBC adapter, so one generated project wrote to a HashMap and read
/// from an empty database. Both halves ran and the list simply came back
/// empty.
///
/// Measured on `minicom-15-01-2026` before it was written down: a POST
/// returned 201 and the matching GET returned `[]`.
#[test]
fn a_gradle_projects_dependencies_are_read_from_its_gradle_file() {
    let root = temp_dir("gradle-jdbc-wiring");
    write_project_skeleton(&root);
    fs::remove_file(root.join("pom.xml")).unwrap();
    fs::write(
        root.join("build.gradle"),
        concat!(
            "plugins {\n    id 'java'\n",
            "    id 'org.springframework.boot' version '4.1.0'\n}\n\n",
            "dependencies {\n",
            // The wider starter, which declares the narrow one -- verified in
            // `deps/spring-boot`.
            "    implementation 'org.springframework.boot:spring-boot-starter-data-jdbc'\n",
            "    implementation 'org.springframework.boot:spring-boot-starter-web'\n}\n",
        ),
    )
    .unwrap();

    let output = jails_cmd(&root, None)
        .args(["g", "scaffold", "Ticket", "id:long@pk", "subject:string!"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let jdbc = fs::read_to_string(
        root.join("src/main/java/com/example/demo/adapters/JdbcTicketRepository.java"),
    )
    .unwrap();
    let memory = fs::read_to_string(
        root.join("src/main/java/com/example/demo/adapters/InMemoryTicketRepository.java"),
    )
    .unwrap();
    // Exactly one of them is the bean, and it is the one that talks to the
    // database the query adapter also reads.
    assert!(jdbc.contains("@Component"), "{jdbc}");
    assert!(jdbc.contains("JdbcClient"), "{jdbc}");
    assert!(
        !memory
            .lines()
            .any(|line| line.trim_start().starts_with("@Component")),
        "{memory}"
    );
}

/// A generated integration test degrades rather than failing to compile when
/// the project has a database of its own and no Testcontainers.
///
/// The real one: `minicom`'s Spring app runs on an H2 file, so JDBC is on the
/// classpath and `TestcontainersConfig` is not. `g usecase --on-conflict` and
/// `g presence` both wrote `@Import(TestcontainersConfig.class)`
/// unconditionally, and `./gradlew build` stopped on `cannot find symbol` in a
/// test jails had written seconds earlier -- exactly the compile error for a
/// file the reader did not write that `CLAUDE.md` says a generator must not
/// hand out. `g scaffold`'s JDBC round trip already degraded; these did not.
#[test]
fn an_integration_test_is_disabled_rather_than_uncompilable_without_a_container_config() {
    let root = temp_dir("no-container-config");
    write_spring_fixture(&root);

    for args in [
        vec![
            "add",
            "dependency",
            "org.springframework.boot:spring-boot-starter-jdbc",
        ],
        vec!["g", "presence", "Room"],
    ] {
        let output = jails_cmd(&root, None).args(&args).output().unwrap();
        assert!(
            output.status.success(),
            "{args:?}: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let it = fs::read_to_string(
        root.join("src/test/java/com/example/demo/adapters/JdbcRoomPresenceIT.java"),
    )
    .unwrap();
    assert!(!it.contains("@Import(TestcontainersConfig.class)"), "{it}");
    assert!(
        !it.contains("import com.example.demo.TestcontainersConfig;"),
        "{it}"
    );
    // And nothing left over pointing at it either.
    assert!(
        !it.contains("import org.springframework.context.annotation.Import;"),
        "{it}"
    );
    assert!(it.contains("@Disabled(\"todo: run jails add db"), "{it}");
    assert!(
        it.contains("import org.junit.jupiter.api.Disabled;"),
        "{it}"
    );
    // The rest of the class is untouched: `@Disabled` is the whole
    // degradation, because the body never named the container config.
    assert!(it.contains("@SpringBootTest"), "{it}");
    assert!(
        it.contains("aMemberJoinedOnOneNodeIsPresentOnTheOther"),
        "{it}"
    );
}

/// The documented create body is exactly what the request record accepts.
///
/// One sample, three readers -- `requests/*.http`, the generated controller
/// test, and the request record itself -- and the third was built from a
/// different list than the first two. `sampled_request` dropped the audit
/// pair and nothing else, so a `@pk` scaffold documented the primary key, and
/// its own generated controller test posted that body and got 400: a
/// standalone `MockMvcTester` has a plain `ObjectMapper`, which rejects a
/// property the record has no component for. Found on the real project, where
/// five of six generated controller tests were red for this one reason.
#[test]
fn the_documented_body_carries_only_what_the_request_record_declares() {
    let root = temp_dir("documented-body");
    write_spring_fixture(&root);

    assert!(
        jails_cmd(&root, None)
            .args([
                "g",
                "scaffold",
                "Ticket",
                "id:long@pk",
                "subject:string!",
                "version:long",
            ])
            .status()
            .unwrap()
            .success()
    );

    let request =
        fs::read_to_string(root.join("src/main/java/com/example/demo/web/TicketRequest.java"))
            .unwrap();
    let declared: Vec<&str> = ["id", "subject", "version"]
        .into_iter()
        .filter(|name| {
            request.contains(&format!(" {name})")) || request.contains(&format!(" {name},"))
        })
        .collect();
    assert_eq!(declared, vec!["subject"], "{request}");

    for path in [
        "requests/ticket.http",
        "src/test/java/com/example/demo/web/TicketControllerTest.java",
    ] {
        let text = fs::read_to_string(root.join(path)).unwrap();
        assert!(
            text.contains("\"subject\": \"sample-subject\""),
            "{path}: {text}"
        );
        assert!(!text.contains("\"id\":"), "{path}: {text}");
        assert!(!text.contains("\"version\":"), "{path}: {text}");
    }

    // The `{{id}}` a GET and a DELETE need is still there: it is sampled from
    // the key's own type rather than read back out of a body that no longer
    // carries it.
    let collection = fs::read_to_string(root.join("requests/ticket.http")).unwrap();
    assert!(collection.contains("@id = 1"), "{collection}");
    assert!(
        collection.contains("GET {{baseUrl}}/tickets/{{id}}"),
        "{collection}"
    );
}
