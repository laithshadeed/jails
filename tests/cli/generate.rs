//! `jails generate` and `jails destroy`: the per-kind artifacts.

use super::*;

#[test]
fn generate_standalone_and_destroy_roundtrip() {
    let root = temp_dir("standalone-roundtrip");
    write_spring_fixture(&root);

    let status = jails_cmd(&root, None)
        .args(["generate", "controller", "comment"])
        .status()
        .unwrap();
    assert!(status.success());
    let file = common::generated(
        &root,
        "src/main/java/com/example/demo/web/CommentController.java",
    );
    assert!(file.is_file());
    let contents = fs::read_to_string(&file).unwrap();
    assert!(contents.contains("class CommentController"));
    assert!(
        !contents.contains("public class"),
        "spring.md §2: a controller is an entry point, not module API"
    );
    // Rails generates a test alongside `generate controller`; jails matches that.
    let test_file = common::generated(
        &root,
        "src/test/java/com/example/demo/web/CommentControllerTest.java",
    );
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
    common::declare_storage(&root);
    let before = snapshot_tree(&root);

    for (name, expected) in [
        ("class", "Java variable `class`"),
        ("Bad!Name", "not valid in a Java identifier"),
        ("A", "PostgreSQL table `as`"),
        ("I", "PostgreSQL table `is`"),
        // A package member outranks `java.lang`'s implicit import, so `record
        // String(String value)` types its own component as itself -- and
        // compiles, as does its generated test, so no tier reports it.
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
    common::declare_storage(&root);
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
}

/// Once the schema carries the closed set, adding a constant to the Java enum
/// and stopping leaves a column that refuses a value every other layer
/// accepts.
#[test]
fn widening_an_enum_migrates_every_table_that_stores_it() {
    let root = temp_dir("enum-closed-set-widening");
    write_spring_fixture(&root);
    common::declare_storage(&root);
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

/// `--column preserve` is the whole reason a field name is a recorded pair
/// rather than a derivation: the Java name moves, the column stays where a
/// live database already has it, and no migration is written because there
/// is nothing for one to run.
#[test]
fn preserving_a_column_renames_the_component_and_writes_no_migration() {
    let root = temp_dir("resource-field-column-preserve");
    write_spring_fixture(&root);
    // The JDBC adapter is what `storage postgres` renders; without a
    // database there is an in-memory repository and nothing to bind SQL to.
    assert!(
        jails_cmd(&root, None)
            .args(["add", "db", "--no-start"])
            .status()
            .unwrap()
            .success()
    );
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
        common::read_generated(&root, "src/main/java/com/example/demo/domain/Account.java");
    assert!(record.contains("UUID ownerId"), "{record}");
    assert!(!record.contains("userId"), "{record}");

    // The SQL half did not move, and that is the point: the adapter still
    // reads and writes `user_id` while the Java component is `ownerId`.
    let adapter = common::read_generated(
        &root,
        "src/main/java/com/example/demo/adapters/JdbcAccountRepository.java",
    );
    assert!(adapter.contains("user_id"), "{adapter}");
    assert!(!adapter.contains("owner_id"), "{adapter}");

    // And the binding is recorded, so the next command derives nothing: the
    // stable id is unchanged and the SQL projection is pinned beside the new
    // Java name.
    let model = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(model.contains("ownerId"), "{model}");
    assert!(model.contains("user_id"), "{model}");
}

#[test]
fn resource_field_uses_scaffold_storage_identity_and_leaves_plain_records_source_only() {
    let root = temp_dir("resource-field-storage-identity");
    write_spring_fixture(&root);
    // Storage is a model declaration now, so the scaffold below is table-backed
    // because the project said so -- not because a migrations directory exists.
    common::declare_storage(&root);
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
    // appends nothing: an `alter table tags` against a table nothing created
    // is unappliable everywhere, and invisible to `doctor`, because a
    // migration written that way is not recorded output.
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
    let tag = common::read_generated(&root, "src/main/java/com/example/demo/domain/Tag.java");
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
            "createdAt:instant@default(now())",
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
    common::declare_storage(&root);
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
    let record = common::generated(&root, "src/main/java/com/example/demo/billing/Invoice.java");
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
/// Every `--on Order` companion calls `Order`'s constructor, so one left on
/// the old list is a build break `doctor` cannot see -- each file is
/// byte-identical to what jails wrote -- and only `javac` finds. Refusing
/// instead would make "this entity needs one more column" permanently
/// impossible once a query exists.
#[test]
fn a_field_regenerates_the_companions_that_construct_the_resource() {
    let root = temp_dir("field-stale-strategy-companions");
    write_spring_fixture(&root);
    common::declare_storage(&root);
    for command in [
        vec![
            "g",
            "scaffold",
            "Order",
            "id:uuid@pk",
            "total:decimal",
            "status:string",
            "version:long@version",
        ],
        vec!["g", "query", "FindOrders", "total:decimal", "--on", "Order"],
        vec![
            "g",
            "transition",
            "ShipOrder",
            "id:uuid",
            "status:string",
            "version:long@version",
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
        let reported = common::generated_relative(&root, companion);
        assert!(plan.contains(&reported), "{reported} missing from:\n{plan}");
    }

    // And the regenerated bytes carry the *new* column list. Each of these
    // reads every component of `Order` -- the query selects them, the
    // transition returns them, the command inserts them -- so `memo` appearing
    // in all three is what says they planned against the model this same
    // transition wrote rather than the one that was on disk.
    for companion in companions {
        let source = common::read_generated(&root, companion);
        assert!(source.contains("memo"), "{companion}:\n{source}");
    }
}

/// The same rule, for the two companions that name the resource through
/// `--yields`.
///
/// An `association` names its parent with `--yields` and its child with
/// `--on`, and a `durable-job` names the resource it produces with
/// `--yields`. Both read the component list off `<Name>.java`: the
/// association's probe builds a row from the child's columns, and the job's
/// store maps one back.
#[test]
fn a_field_reaches_the_companions_named_by_yields_as_well_as_on() {
    let root = temp_dir("field-stale-yields-companions");
    write_spring_fixture(&root);
    common::declare_storage(&root);
    for command in [
        vec!["add", "db", "--no-start"],
        vec![
            "g",
            "scaffold",
            "Owner",
            "id:uuid@pk",
            "name:string!",
            "createdAt:instant@default(now())",
        ],
        vec![
            "g",
            "scaffold",
            "Item",
            "id:uuid@pk",
            "ownerId:uuid@index",
            "name:string!",
            "createdAt:instant@default(now())",
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
        // A durable job stores its payload as JSON, so the reader it uses has
        // to be declared before the job that needs it.
        vec!["add", "json"],
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
    // it goes stale the moment the child gains one.
    let probe =
        ".jails/generated/test/java/com/example/demo/adapters/jdbc/ItemOwnerAssociationIT.java";
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
    // from a `child=parent` *mapping* rather than a field list, so its
    // arguments must not be read as fields.
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
/// `--index` and `@index` are both creation-time; this is the one afterwards.
/// `g field` can already add a *column* to a live table, which is the harder
/// problem: an index has no data plan to argue about.
#[test]
fn an_index_can_be_added_to_a_table_that_already_exists() {
    let root = temp_dir("resource-index-add");
    write_spring_fixture(&root);
    common::declare_storage(&root);
    let scaffold = jails_cmd(&root, None)
        .args([
            "g",
            "scaffold",
            "Message",
            "id:uuid@pk",
            "customerId:uuid",
            "body:string!",
            "createdAt:instant@default(now())",
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
        .find(|path| path.to_string_lossy().contains("idx_messages"))
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
    // The identity is the entity plus the ordered column list, so the second
    // attempt is refused by that id rather than writing a duplicate migration
    // Flyway would run twice.
    assert!(!again.status.success());
    assert!(
        String::from_utf8_lossy(&again.stderr).contains("already exists on `ent_message`"),
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
        String::from_utf8_lossy(&typo.stderr).contains("does not exist on `message`"),
        "{}",
        String::from_utf8_lossy(&typo.stderr)
    );
}

/// A route the caller names, because the URLs are somebody else's contract.
///
/// A ported application answers `/customer_api/ping`, `/admin_api/issues`,
/// `/api/conversations/`, and none of those is derivable from any name jails
/// would accept for the class. Derived paths stay the default -- they are a
/// virtue greenfield -- and `--path` is how a port meets a fixed contract.
#[test]
fn a_named_route_replaces_the_derived_one_everywhere_it_appears() {
    let root = temp_dir("named-route");
    write_spring_fixture(&root);
    common::declare_storage(&root);
    for command in [
        vec!["add", "db", "--no-start"],
        vec![
            "g",
            "scaffold",
            "Ping",
            "id:uuid@pk",
            "email:string!",
            "createdAt:instant@default(now())",
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

    // **The route lives on the operation, not on an adapter.** An operation's
    // port carries `String ROUTE`, and the Spring controller the `api`
    // capability writes reads it from there -- so the named route is checked
    // where it is *decided*, and a project that never declares `api` still has
    // one answer to "where does this operation answer".
    for (file, expected) in [
        (
            "src/main/java/com/example/demo/service/RecordPingUseCase.java",
            "\"POST /customer_api/ping\"",
        ),
        (
            "src/main/java/com/example/demo/service/PingsByEmailUseCase.java",
            "\"GET /customer_api/read\"",
        ),
        (
            "src/main/java/com/example/demo/web/BarController.java",
            "\"/bar\"",
        ),
    ] {
        let source = common::read_generated(&root, file);
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

/// A transition can update a row keyed by something other than `id`.
///
/// A resource whose natural key is `user_id` -- a conversation per customer, a
/// row a URL addresses by the customer -- is updated by that key, so the
/// selector is not the literal `"id"` in the port or the SQL predicate.
///
/// The URL half is `a_transition_can_take_its_key_from_the_url`; what is
/// refused here is a variable that names something *other* than the selector,
/// because the only value a URL can identify a row with is the one the row is
/// selected by.
#[test]
fn a_transition_can_select_by_a_component_other_than_id() {
    let root = temp_dir("transition-select");
    write_spring_fixture(&root);
    common::declare_storage(&root);

    assert!(
        jails_cmd(&root, None)
            .args([
                "g",
                "scaffold",
                "Conversation",
                "id:long@pk",
                "userId:long@unique",
                "status:string",
                "version:long",
            ])
            .output()
            .unwrap()
            .status
            .success()
    );

    let selected = jails_cmd(&root, None)
        .args([
            "g",
            "transition",
            "SetStatus",
            "--on",
            "Conversation",
            "userId:long",
            "version:long",
            "status:string",
            "--select",
            "userId",
        ])
        .output()
        .unwrap();
    assert!(
        selected.status.success(),
        "{}",
        String::from_utf8_lossy(&selected.stderr)
    );

    // The SQL is what proves it: the predicate, not just the record.
    let adapter = common::read_generated(
        &root,
        "src/main/java/com/example/demo/adapters/JdbcSetStatusTransition.java",
    );
    // The predicate is assembled from a list, so the column appears in the
    // seed rather than glued to the `where` -- an optional guard adds to the
    // same list, and one place decides how they are joined.
    assert!(adapter.contains("\"user_id = :user_id\""), "{adapter}");
    assert!(!adapter.contains("\"id = :id\""), "{adapter}");

    // A selector that names no component says which ones there are.
    let missing = jails_cmd(&root, None)
        .args([
            "g",
            "transition",
            "SetOther",
            "--on",
            "Conversation",
            "userId:long",
            "version:long",
            "status:string",
            "--select",
            "nothing",
        ])
        .output()
        .unwrap();
    assert!(!missing.status.success());
    let stderr = String::from_utf8_lossy(&missing.stderr);
    assert!(stderr.contains("`nothing`"), "{stderr}");
    assert!(stderr.contains("--select"), "{stderr}");

    // A path variable that names something other than the selector has nowhere
    // to go, so it is refused rather than mounted and ignored.
    let wrong = jails_cmd(&root, None)
        .args([
            "g",
            "transition",
            "SetFromUrl",
            "--on",
            "Conversation",
            "userId:long",
            "version:long",
            "status:string",
            "--select",
            "userId",
            "--path",
            "/admin_api/conversations/{id}/status",
        ])
        .output()
        .unwrap();
    assert!(
        !wrong.status.success(),
        "a stray path variable was accepted"
    );
    let stderr = String::from_utf8_lossy(&wrong.stderr);
    assert!(
        stderr.contains("cannot take `{id}` from the URL"),
        "{stderr}"
    );
    assert!(stderr.contains("--select id"), "{stderr}");

    // Two variables: only one value identifies a row, and the message says
    // which one to keep rather than picking.
    let two = jails_cmd(&root, None)
        .args([
            "g",
            "transition",
            "SetFromTwo",
            "--on",
            "Conversation",
            "userId:long",
            "version:long",
            "status:string",
            "--select",
            "userId",
            "--path",
            "/admin_api/{userId}/conversations/{status}",
        ])
        .output()
        .unwrap();
    assert!(!two.status.success(), "two path variables were accepted");
    let stderr = String::from_utf8_lossy(&two.stderr);
    assert!(stderr.contains("can bind one path variable"), "{stderr}");
    assert!(stderr.contains("`{userId}`"), "{stderr}");
}

/// The key in the URL, which is where every admin frontend puts it -- `PATCH
/// /admin_api/conversations/{userId}/status`.
///
/// Three things move together or the route is quietly broken: the command
/// record drops the selector (a component bound
/// from two places can disagree with itself), the port takes the key beside
/// the command, and the generated proof expands the variable. The port shape
/// is deliberately the *same* one a body-carried key gets -- `execute(key,
/// command, expectedVersion)` either way -- so the adapter and the controller
/// cannot come to different conclusions about where the key was.
#[test]
fn a_transition_can_take_its_key_from_the_url() {
    let root = temp_dir("transition-path-bound");
    write_spring_fixture(&root);
    common::declare_storage(&root);

    assert!(
        jails_cmd(&root, None)
            .args([
                "g",
                "scaffold",
                "Conversation",
                "id:long@pk",
                "userId:long@unique",
                "status:string",
                "version:long",
            ])
            .output()
            .unwrap()
            .status
            .success()
    );

    // The route is served by the `api` capability, which is what emits a
    // Spring controller for an operation. Declaring one without it leaves a
    // linked route nothing answers -- and `db` is what implements the port
    // that controller takes.
    for capability in [["add", "db", "--no-start"], ["add", "api", "--no-start"]] {
        assert!(
            jails_cmd(&root, None)
                .args(capability)
                .status()
                .unwrap()
                .success(),
            "{capability:?}"
        );
    }

    let output = jails_cmd(&root, None)
        .args([
            "g",
            "transition",
            "SetStatus",
            "--on",
            "Conversation",
            "userId:long",
            "version:long",
            "status:string",
            "--select",
            "userId",
            "--method",
            "patch",
            "--path",
            "/admin_api/conversations/{userId}/status",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    // The input record no longer carries the key: it is addressed by the URL,
    // and a value in two places is a value that can disagree with itself.
    let transition = common::read_generated(
        &root,
        "src/main/java/com/example/demo/application/transitions/SetStatusTransition.java",
    );
    let declaration = transition
        .split_once("record Input(")
        .and_then(|(_, rest)| rest.split_once(')'))
        .map(|(components, _)| components.to_string())
        .unwrap_or_else(|| panic!("no Input record:\n{transition}"));
    assert!(!declaration.contains("userId"), "{transition}");

    // Mounted *and* bound.
    let controller = common::read_generated(
        &root,
        "src/main/java/com/example/demo/adapters/http/SetStatusController.java",
    );
    // The route is on the mapping, and the key is bound out of the URL by
    // name rather than read off a body component that is no longer there.
    assert!(
        controller.contains("path = \"/admin_api/conversations/{userId}/status\""),
        "{controller}"
    );
    assert!(
        controller.contains("@PathVariable(\"userId\") long userId"),
        "{controller}"
    );
    assert!(
        controller.contains("import org.springframework.web.bind.annotation.PathVariable;"),
        "{controller}"
    );
    assert!(
        controller.contains("operation.execute(userId, input, expectedVersion)"),
        "{controller}"
    );
    assert!(controller.contains("RequestMethod.PATCH"), "{controller}");

    // One port shape: the selector is a parameter of `execute`, and the route
    // it is addressed by is a constant on the port rather than a second
    // spelling in the adapter.
    assert!(
        transition
            .contains("Conversation execute(long userId, Input input, long expectedVersion);"),
        "{transition}"
    );
    assert!(
        transition.contains("ROUTE = \"PATCH /admin_api/conversations/{userId}/status\""),
        "{transition}"
    );

    // And the proof expands the variable.
    let test = common::read_generated(
        &root,
        "src/test/java/com/example/demo/adapters/http/SetStatusControllerTest.java",
    );
    assert!(
        test.contains(".uri(\"/admin_api/conversations/{userId}/status\", "),
        "{test}"
    );
    assert!(!test.contains("\"userId\":"), "{test}");
}

/// A resource can be dropped, re-created and dropped again, indefinitely: the
/// drop planner reaches the whole sealed lineage, and every drop allocates a
/// new migration rather than reusing an existing `drop_` file whose
/// description matches.
#[test]
fn a_resource_can_be_dropped_and_recreated_more_than_once() {
    let root = temp_dir("drop-recreate-cycle");
    write_spring_fixture(&root);
    common::declare_storage(&root);

    let scaffold = || {
        jails_cmd(&root, None)
            .args(["g", "scaffold", "Book", "id:uuid@pk", "title:string"])
            .output()
            .unwrap()
    };
    let drop = || {
        jails_cmd(&root, None)
            .args([
                "destroy",
                "scaffold",
                "Book",
                "--storage",
                "drop",
                "--confirm-table",
                "books",
                "--force",
            ])
            .output()
            .unwrap()
    };

    // Two full cycles: the second is the one that meets an existing lineage.
    for cycle in 1..=2 {
        let created = scaffold();
        assert!(
            created.status.success(),
            "cycle {cycle} create: {}",
            String::from_utf8_lossy(&created.stderr)
        );
        let dropped = drop();
        assert!(
            dropped.status.success(),
            "cycle {cycle} drop: {}",
            String::from_utf8_lossy(&dropped.stderr)
        );
    }

    // Forward-only and strictly alternating: nothing was rewritten, and every
    // step got its own version.
    let mut migrations = fs::read_dir(root.join("src/main/resources/db/migration"))
        .unwrap()
        .filter_map(|entry| {
            let name = entry.ok()?.file_name().to_string_lossy().into_owned();
            name.ends_with(".sql").then_some(name)
        })
        .collect::<Vec<_>>();
    migrations.sort();
    assert_eq!(
        migrations,
        vec![
            "V001__create_books.sql",
            "V002__drop_books.sql",
            "V003__create_books.sql",
            "V004__drop_books.sql",
        ]
    );
}

/// What `jails explain query` says about a filter is what `g query` does.
///
/// `every_kind_has_an_explanation` checks that a kind *has* a row and nothing
/// checks the row is still true. A rationale cannot be derived -- that is why
/// the table is hand-written -- but a claim about a shape jails emits can be
/// checked against the shape, and two such claims are pinned here.
#[test]
fn what_explain_says_about_a_query_is_what_a_query_does() {
    let root = temp_dir("explain-agrees-with-query");
    write_spring_fixture(&root);
    common::declare_storage(&root);

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
    let adapter = common::read_generated(
        &root,
        "src/main/java/com/example/demo/adapters/JdbcOpenLoansQuery.java",
    );

    // An optional filter widens rather than matching null, and the
    // explanation says so instead of saying filters must be required. The
    // predicate is *appended* when the caller sent a value rather than
    // written as `cast(:settled as boolean) is null or ...`, so the index on
    // that column is still usable for the request that names it.
    assert!(
        adapter.contains("if (input.settled().isPresent())"),
        "{adapter}"
    );
    assert!(
        adapter.contains("predicates.add(\"settled = :settled\")"),
        "{adapter}"
    );
    assert!(!adapter.contains("cast(:settled"), "{adapter}");
    assert!(
        explained.contains("only when the caller sent one"),
        "{explained}"
    );

    // The cap the caller cannot see from the response is stated where they
    // can see it.
    let cap = adapter
        .lines()
        .find(|line| line.contains("limit "))
        .expect("the adapter bounds its result");
    let cap = cap
        .rsplit("limit ")
        .next()
        .and_then(|rest| {
            rest.trim_matches(|c: char| !c.is_ascii_digit())
                .parse::<usize>()
                .ok()
        })
        .expect("a numeric bound");
    assert!(
        explained.contains(&cap.to_string()),
        "explain does not name the {cap}-row cap the adapter applies:\n{explained}"
    );
}

/// A verb a recipe derives is not also a verb a flag can set.
///
/// A query's verb follows its request -- GET when every filter comes from
/// `--path`, POST when it carries a body -- so `--method` there is not a
/// preference jails declines to honour, it is a claim about the request that
/// contradicts the request, and it is refused the way `--path`, `--via` and
/// `--consumes` are refused on recipes that cannot carry them.
#[test]
fn a_verb_a_recipe_derives_is_not_one_a_flag_can_set() {
    let root = temp_dir("method-derived-not-set");
    write_spring_fixture(&root);

    // The two recipes that name a verb keep taking it. **Different names,
    // because a component name is one namespace**: `on` and `yields`
    // reference a component by that name, so two of them would make every
    // such reference ambiguous.
    for (kind, name) in [("controller", "VerifyEndpoint"), ("client", "VerifyClient")] {
        let named = jails_cmd(&root, None)
            .args(["g", kind, name, "--method", "post"])
            .output()
            .unwrap();
        assert!(
            named.status.success(),
            "g {kind} --method post: {}",
            String::from_utf8_lossy(&named.stderr)
        );
    }

    // And declaring one twice under one name is refused rather than silently
    // taking whichever came last.
    let duplicate = jails_cmd(&root, None)
        .args(["g", "client", "VerifyEndpoint", "--method", "post"])
        .output()
        .unwrap();
    assert!(!duplicate.status.success(), "{duplicate:?}");

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
/// `@ImportHttpServices` carries one group name, so one shared
/// `HttpClientsConfig` scanned by package would leave only the newest client
/// configured -- silently at generate time, and visibly only as the older
/// client's own test calling `https://example.invalid`. One config class per
/// client, listed by type, makes it additive by construction.
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
        let config = fs::read_to_string(common::generated(
            &root,
            &format!("src/main/java/com/example/demo/clients/{name}ClientConfig.java"),
        ))
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
    // The JDBC half is `storage postgres`': without it a scaffold gets the
    // in-memory adapter and nothing to bind SQL to.
    assert!(
        jails_cmd(&root, None)
            .args(["add", "db", "--no-start"])
            .status()
            .unwrap()
            .success()
    );

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

    for file in [
        "src/main/java/com/example/demo/domain/Post.java",
        "src/main/java/com/example/demo/repository/PostRepository.java",
        "src/main/java/com/example/demo/adapters/jdbc/JdbcPostRepository.java",
        "src/main/java/com/example/demo/service/PostService.java",
        "src/main/java/com/example/demo/web/PostController.java",
    ] {
        assert!(
            common::generated(&root, file).is_file(),
            "{file} is missing:\n{}",
            common::managed_listing(&root)
        );
    }
    assert!(
        common::generated(
            &root,
            "src/test/java/com/example/demo/adapters/jdbc/JdbcPostRepositoryIT.java"
        )
        .is_file(),
        "{}",
        common::managed_listing(&root)
    );
    assert!(
        common::generated(
            &root,
            "src/test/java/com/example/demo/web/PostControllerTest.java"
        )
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

/// These pairs are one field spelled twice, not two fields sharing one
/// column, which is why the refusal names a single Java component and a
/// single column rather than two of each.
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
            common::generated(&root, "src/main/java/com/example/demo/domain")
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
    write_spring_fixture(&root);
    // Canonical before the first snapshot: initialising the model is its own
    // announced step, so a refusal that ran it first would look like a write.
    common::become_canonical(&root);

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

    let test = common::generated(&root, "src/test/java/com/example/demo/domain/NoteTest.java");
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
            "createdAt:instant@default(now())",
            "--pretend",
            "--diff",
            "--ast",
        ])
        .output()
        .unwrap();
    assert!(changed.status.success(), "{changed:?}");
    let shown = String::from_utf8_lossy(&changed.stdout);
    // The managed path, because that is where the file is: generated output
    // lives under `.jails/generated` and the reader's own tree is theirs.
    assert!(
        shown.contains(
            "diff --jails replace .jails/generated/main/java/com/example/demo/domain/Note.java"
        ),
        "{shown}"
    );
    assert!(shown.contains("@@ -"), "{shown}");
    assert!(shown.contains("+import java.time.Instant;"), "{shown}");
    // **The three-way merge, asserted by its result rather than by a label.**
    // The reader's line is in the file's BASE and not in the compiler's
    // THEIRS, so a diff that removed it would be a merge that dropped it --
    // which is the thing worth checking, and a marker saying "three-way" is
    // not.
    assert!(
        shown.contains(".jails/generated/test/java/com/example/demo/domain/NoteTest.java"),
        "{shown}"
    );
    assert!(
        !shown.contains("-// reader-owned context"),
        "the merge dropped the reader's line:\n{shown}"
    );
    // `--ast` is the transition as values: the exact operations the executor
    // would run, named as the closed set they come from.
    assert!(shown.contains("PublishMergedTree { root:"), "{shown}");
    assert!(shown.contains("ReplaceModelFile { path:"), "{shown}");
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
    assert!(shown.contains("diff --jails create "), "{shown}");
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
    // Named for the pipeline, because that is what runs: capture the
    // workspace, compile the patched model, materialize the exact plan.
    for phase in ["capture", "compile", "materialize"] {
        assert!(
            debug.contains(&format!("timing  {phase}")),
            "missing {phase} timing in {debug}"
        );
    }
    // And the absence is the point: `--pretend` stopped before the only step
    // that writes.
    assert!(!debug.contains("timing  execute"), "{debug}");
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
    // The exact plan itself, not an envelope describing one: the answer to
    // `--output json` is the `PlanBundle` -- the reviewed transition, its
    // digest, its operations and every blob they name -- because that is the
    // value `--plan-out` writes and `apply` refers to. A second shape
    // describing it could disagree with it.
    assert_eq!(value["schema"], "jails.plan-bundle.v1", "{json}");
    assert!(
        value["plan"]["digest"]
            .as_str()
            .is_some_and(|digest| digest.starts_with("sha256:") && digest.len() == 71),
        "{json}"
    );
    assert!(
        value["plan"]["id"]
            .as_str()
            .is_some_and(|digest| digest.starts_with("sha256:")),
        "{json}"
    );
    let operations = value["plan"]["operations"].as_array().unwrap();
    assert!(
        operations
            .iter()
            .any(|operation| operation["kind"] == "publish-merged-tree"),
        "{json}"
    );
    assert!(
        operations
            .iter()
            .any(|operation| operation["kind"] == "replace-model-file"),
        "{json}"
    );
    assert!(
        json.contains(".jails/generated/main/java/com/example/demo/domain/Fresh.java"),
        "{json}"
    );
    // The JSON is the machine's answer, so the human's postscript is not in
    // it.
    assert!(!json.contains("nothing was written"), "{json}");
    assert!(!json.contains("timing"), "{json}");
    assert_eq!(
        snapshot_tree(&root),
        before,
        "JSON review preview wrote files"
    );

    // A regeneration over an edited file is a three-way merge, and the plan
    // carries its *result* rather than a timing for it: the merged bytes are
    // in the bundle's blobs, which is what makes the digest above the thing
    // `apply` refers to.
    let merged_json = jails_cmd(&root, None)
        .args([
            "g",
            "field",
            "Note",
            "createdAt:instant@default(now())",
            "--pretend",
            "--output",
            "json",
        ])
        .output()
        .unwrap();
    assert!(merged_json.status.success(), "{merged_json:?}");
    let merged_json = String::from_utf8(merged_json.stdout).unwrap();
    let merged_value: serde_json::Value = serde_json::from_str(&merged_json).unwrap();
    let merged_tree = merged_value["plan"]["operations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|operation| operation["kind"] == "publish-merged-tree")
        .expect("the managed tree is published");
    let after = merged_tree["after"].as_str().unwrap();
    let entries = &merged_value["trees"][after]["entries"];
    let blob = entries[".jails/generated/test/java/com/example/demo/domain/NoteTest.java"]["blob"]
        .as_str()
        .expect("the merged companion is in the published tree");
    let merged_test = merged_value["blobs"][blob].as_array().unwrap();
    let merged_test = String::from_utf8(
        merged_test
            .iter()
            .map(|byte| byte.as_u64().unwrap() as u8)
            .collect::<Vec<_>>(),
    )
    .unwrap();
    assert!(
        merged_test.contains("// reader-owned context"),
        "the merge dropped the reader's line:\n{merged_test}"
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
    // The same review after the fact: "what did that change" is one question
    // whether it is asked before or after, and the plan is the same value
    // either way -- which is what makes the two answers agree.
    assert!(
        applied.contains(
            "diff --jails create .jails/generated/main/java/com/example/demo/domain/Fresh.java"
        ),
        "{applied}"
    );
    assert!(applied.contains("PublishMergedTree { root:"), "{applied}");
    assert!(
        common::generated(&root, "src/main/java/com/example/demo/domain/Fresh.java").is_file(),
        "an applied reviewed transition did not commit"
    );

    let committed = jails_cmd(&root, None)
        .args(["g", "record", "Timed", "id:uuid", "--output", "json"])
        .output()
        .unwrap();
    assert!(committed.status.success(), "{committed:?}");
    let committed = String::from_utf8(committed.stdout).unwrap();
    let committed_value: serde_json::Value = serde_json::from_str(&committed).unwrap();
    // The plan digest, and it is the same one the preview printed: `apply`
    // never replans, so what the execution reports is the digest of the
    // bundle it was handed -- which is what makes "preview, review, apply"
    // refer to one transition rather than three.
    assert_eq!(
        committed_value["schema"], "jails.execution.v1",
        "{committed}"
    );
    assert!(
        committed_value["plan_digest"]
            .as_str()
            .is_some_and(|digest| digest.starts_with("sha256:") && digest.len() == 71),
        "{committed}"
    );
    assert!(
        committed_value["files_written"]
            .as_u64()
            .is_some_and(|n| n > 0),
        "{committed}"
    );
    // The machine's answer carries no timings: they are a human diagnostic
    // behind `--debug`, and a caller parsing this did not ask how long it took.
    assert!(committed_value.get("timings").is_none(), "{committed}");
}

#[test]
fn task_scaffold_cannot_rewrite_or_delete_its_published_v001() {
    let root = temp_dir("task-migration-seal");
    write_spring_fixture(&root);
    common::declare_storage(&root);

    let generated = jails_cmd(&root, None)
        .args([
            "g",
            "scaffold",
            "Task",
            "id:uuid@pk",
            "title:string!",
            "createdAt:instant@default(now())@index",
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
    // **Refused for the change, not for the seal.** Re-declaring the scaffold
    // without `createdAt` and with `completed` is a rename, a drop and an add,
    // or a type change, and each keeps the rows differently -- so it is
    // refused before it can reach the published `V001` at all.
    assert!(!resync.status.success(), "{resync:?}");
    let resync_stderr = String::from_utf8_lossy(&resync.stderr);
    assert!(
        resync_stderr.contains("gained `completed` and lost `created_at`"),
        "{resync_stderr}"
    );
    assert!(resync_stderr.contains("fix:"), "{resync_stderr}");
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
    // Retired but preserved: the semantic node stays inactive, the table
    // stays, and no migration is appended -- there is nothing to retire in
    // SQL. The history it already has is untouched.
    let retired = common::resource_status(&root, "Task");
    assert_eq!(retired["state"], "retired", "{retired}");
    assert_eq!(retired["table"], "tasks", "{retired}");
    assert_eq!(
        retired["migrations"],
        serde_json::json!(["001"]),
        "{retired}"
    );

    let status = jails_cmd(&root, None)
        .args(["resource", "status", "Task", "--output", "json"])
        .output()
        .unwrap();
    assert!(status.status.success(), "{status:?}");
    let status_json = String::from_utf8(status.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&status_json).unwrap();
    assert_eq!(parsed["state"], "retired", "{status_json}");
    assert_eq!(parsed["table"], "tasks", "{status_json}");
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
        common::generated(
            &root,
            "src/main/java/com/example/demo/web/TaskController.java"
        )
        .is_file()
    );
    // Revived onto the same table, with the same one migration: an exact
    // revival reuses the inactive node rather than creating a second one, so
    // the history does not restart.
    let revived = common::resource_status(&root, "Task");
    assert_eq!(revived["state"], "consistent", "{revived}");
    assert_eq!(revived["table"], "tasks", "{revived}");
    assert_eq!(
        revived["migrations"],
        serde_json::json!(["001"]),
        "{revived}"
    );

    let active_status = jails_cmd(&root, None)
        .args(["resource", "status", "Task", "--output", "json"])
        .output()
        .unwrap();
    assert!(active_status.status.success(), "{active_status:?}");
    let active_json = String::from_utf8_lossy(&active_status.stdout).to_string();
    let parsed: serde_json::Value = serde_json::from_str(&active_json).unwrap();
    assert_eq!(parsed["state"], "consistent", "{active_json}");
}

/// Regenerating a dropped resource revives its lifecycle, so the recovery
/// commands agree about it rather than each refusing over a resource that is
/// present on disk.
#[test]
fn regenerating_a_dropped_resource_returns_it_to_a_consistent_lifecycle() {
    let root = temp_dir("recreate-revives-lifecycle");
    write_spring_fixture(&root);
    common::declare_storage(&root);

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

    // And the entity can be evolved again.
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
    common::declare_storage(&root);

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

    // The coordinated rename takes a bare name, not `<slice>.<name>`.
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
    let adapter = common::read_generated(
        &root,
        "src/main/java/com/example/demo/adapters/JdbcReaderRepository.java",
    );
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
        common::read_generated(&root, "src/main/java/com/example/demo/domain/Reader.java")
            .contains("nickname")
    );

    // A source-only resource has no storage to carry -- and the textual
    // rename is still wrong for it, because the declaration is what the next
    // compilation renders from: `rename` would move the Java and leave the
    // model saying `Note`, so `jails sync` writes `Note.java` straight back.
    // What "source-only" buys is that every strategy means the same thing.
    let record = jails_cmd(&root, None)
        .args(["g", "record", "Note", "body:string!"])
        .output()
        .unwrap();
    assert!(record.status.success(), "{record:?}");
    let textual = jails_cmd(&root, None)
        .args(["rename", "Note", "Memo", "--force"])
        .output()
        .unwrap();
    assert_eq!(textual.status.code(), Some(1), "{textual:?}");
    let stderr = String::from_utf8_lossy(&textual.stderr);
    assert!(
        stderr.contains("declared in this project's application model"),
        "{stderr}"
    );
    // ...and it does not name a table, because there is not one.
    assert!(!stderr.contains("backed by table"), "{stderr}");
    let renamed = jails_cmd(&root, None)
        .args([
            "rename",
            "resource",
            "Note",
            "Memo",
            "--strategy",
            "preserve-table",
        ])
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
    common::declare_storage(&root);

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
    // Storage is untouched, so there is nothing to append: the accepted
    // migration is byte-identical and no second one appears.
    let stdout = String::from_utf8_lossy(&renamed.stdout);
    assert!(!stdout.contains("append"), "{stdout}");
    assert_eq!(fs::read(&migration).unwrap(), sealed);
    assert!(
        !root
            .join("src/main/resources/db/migration/V002__rename_tasks.sql")
            .exists()
    );
    assert!(
        common::generated(&root, "src/main/java/com/example/demo/domain/WorkItem.java").is_file()
    );
    assert!(
        !root
            .join("src/main/java/com/example/demo/domain/Task.java")
            .exists()
    );
    let controller = common::read_generated(
        &root,
        "src/main/java/com/example/demo/web/WorkItemController.java",
    );
    assert!(controller.contains("/tasks"), "{controller}");

    // Renamed in Java, unmoved in storage, and no second migration -- which
    // is the whole of what preserve-table means.
    let status = common::resource_status(&root, "WorkItem");
    assert_eq!(status["resource"], "WorkItem", "{status}");
    assert_eq!(status["table"], "tasks", "{status}");
    assert_eq!(status["migrations"], serde_json::json!(["001"]), "{status}");
}

#[test]
fn coordinated_single_cutover_appends_one_migration_and_switches_the_binding() {
    let root = temp_dir("resource-rename-single-cutover");
    write_spring_fixture(&root);
    common::declare_storage(&root);

    let generated = jails_cmd(&root, None)
        .args([
            "g",
            "scaffold",
            "Task",
            "id:uuid@pk",
            "title:string!",
            "createdAt:instant@default(now())@index",
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
    // The plan says what moved: one migration appended, and the managed tree
    // rewritten under the new name.
    assert!(
        stdout.contains("V002__rename_tasks_to_work_items.sql"),
        "{stdout}"
    );
    assert_eq!(fs::read(first).unwrap(), sealed);
    let cutover = root.join("src/main/resources/db/migration/V002__rename_tasks_to_work_items.sql");
    // Everything the old table's name was baked into. PostgreSQL renames the
    // table and leaves its indexes and primary-key constraint saying `tasks`,
    // which is drift nobody sees until they read the schema a year later.
    let statements = fs::read_to_string(&cutover).unwrap();
    for statement in [
        "alter table tasks rename to work_items;",
        "alter table work_items rename constraint tasks_pkey to work_items_pkey;",
    ] {
        assert!(statements.contains(statement), "{statements}");
    }
    let adapter = common::read_generated(
        &root,
        "src/main/java/com/example/demo/adapters/JdbcWorkItemRepository.java",
    );
    assert!(adapter.contains("work_items"), "{adapter}");
    assert!(!adapter.contains("from tasks"), "{adapter}");
    assert!(!adapter.contains("into tasks"), "{adapter}");
    let controller = common::read_generated(
        &root,
        "src/main/java/com/example/demo/web/WorkItemController.java",
    );
    assert!(controller.contains("/tasks"), "{controller}");

    // The lifecycle, read through the command that reports it: the resource
    // is `WorkItem` over `work_items`, and its history is both migrations --
    // the create under the old table name and the cutover that moved it.
    let status = common::resource_status(&root, "WorkItem");
    assert_eq!(status["resource"], "WorkItem", "{status}");
    assert_eq!(status["table"], "work_items", "{status}");
    assert_eq!(
        status["migrations"],
        serde_json::json!(["001", "002"]),
        "{status}"
    );
}

#[test]
fn single_cutover_reports_reader_owned_storage_object_names_without_writing() {
    let root = temp_dir("resource-rename-reader-owned-object");
    write_spring_fixture(&root);
    common::declare_storage(&root);
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
    common::declare_storage(&root);
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
    common::declare_storage(&root);
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
fn a_rolling_rename_is_refused_as_the_campaign_it_is() {
    let root = temp_dir("resource-rename-rolling");
    write_spring_fixture(&root);
    common::declare_storage(&root);
    let generated = jails_cmd(&root, None)
        .args(["g", "scaffold", "Task", "id:uuid@pk", "title:string!"])
        .output()
        .unwrap();
    assert!(generated.status.success(), "{generated:?}");
    let before = snapshot_tree(&root);

    // A campaign is several reviewed plans, and the compiler plans one. Each
    // step of a rolling rename is an ordinary plan the reader runs when their
    // readers are ready, so the tool refuses to own the waiting rather than
    // carrying a state machine whose whole content is "not yet".
    let refused = jails_cmd(&root, None)
        .args([
            "rename",
            "resource",
            "Task",
            "WorkItem",
            "--strategy",
            "rolling",
            "--force",
        ])
        .output()
        .unwrap();
    assert_eq!(refused.status.code(), Some(1), "{refused:?}");
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(stderr.contains("campaign"), "{stderr}");
    // And it names the two it does implement rather than leaving the reader
    // to find out which strategies exist by trying them.
    assert!(stderr.contains("preserve-table"), "{stderr}");
    assert!(stderr.contains("single-cutover"), "{stderr}");
    assert_eq!(snapshot_tree(&root), before, "refusal wrote project files");

    // The cutover it points at is the one that works.
    let cutover = jails_cmd(&root, None)
        .args([
            "rename",
            "resource",
            "Task",
            "WorkItem",
            "--strategy",
            "single-cutover",
            "--force",
        ])
        .output()
        .unwrap();
    assert!(
        cutover.status.success(),
        "{}{}",
        String::from_utf8_lossy(&cutover.stdout),
        String::from_utf8_lossy(&cutover.stderr)
    );
}

#[test]
fn coordinated_resource_rename_reports_reader_owned_java_without_rewriting_it() {
    let root = temp_dir("resource-rename-manual-java");
    write_spring_fixture(&root);
    common::declare_storage(&root);
    let generated = jails_cmd(&root, None)
        .args(["g", "scaffold", "Task", "id:uuid@pk", "title:string!"])
        .output()
        .unwrap();
    assert!(generated.status.success(), "{generated:?}");
    let manual = common::generated(&root, "src/main/java/com/example/demo/Manual.java");
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
    common::declare_storage(&root);
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
    let controller = common::generated(
        &root,
        "src/main/java/com/example/demo/web/TaskController.java",
    );
    fs::remove_file(&controller).unwrap();

    let repaired = jails_cmd(&root, None)
        .args(["resource", "repair"])
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
        .args(["resource", "repair"])
        .output()
        .unwrap();
    assert!(repaired_missing.status.success(), "{repaired_missing:?}");
    assert_eq!(fs::read(&migration).unwrap(), sealed);

    // Repair restores the managed tree from the model and leaves history
    // alone: both migrations are still the resource's, and no third appears.
    let status = common::resource_status(&root, "Task");
    assert_eq!(
        status["migrations"],
        serde_json::json!(["001", "002"]),
        "{status}"
    );
}

#[test]
fn task_drop_keeps_v001_and_appends_an_exact_forward_migration() {
    let root = temp_dir("task-drop-migration");
    write_spring_fixture(&root);
    common::declare_storage(&root);
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
        "-- Generated by jails from the accepted semantic schema.\ndrop table tasks;\n"
    );
    assert!(
        !root
            .join("src/main/java/com/example/demo/web/TaskController.java")
            .exists()
    );
    // The resource is retired and its history is both migrations: the create
    // and the drop that retires it. Reporting one would read as a resource
    // whose creation was never recorded.
    let status = common::resource_status(&root, "Task");
    assert_eq!(status["state"], "retired", "{status}");
    assert_eq!(
        status["migrations"],
        serde_json::json!(["001", "002"]),
        "{status}"
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
        common::generated(
            &root,
            "src/main/java/com/example/demo/web/TaskController.java"
        )
        .is_file()
    );
}

/// A migration whose delivery is somebody else's business.
///
/// Retirement appends one forward migration and stops; running it against a
/// database is `jails migrate`, a separate command with its own failure. The
/// retirement is durable whether or not a database ever sees it, and it does
/// not reach for one.
#[test]
fn a_retirement_is_durable_without_a_database_to_apply_it_to() {
    let root = temp_dir("task-drop-without-a-database");
    write_spring_fixture(&root);
    common::declare_storage(&root);
    assert!(
        jails_cmd(&root, None)
            .args(["g", "scaffold", "Task", "id:uuid@pk", "title:string!"])
            .status()
            .unwrap()
            .success()
    );

    // No `flyway`, no `psql`, and no `DATABASE_URL`: a retirement that needed
    // any of them would fail here.
    let bare = temp_dir("task-drop-no-tools-bin");
    let ignored_log = bare.join("ignored.log");
    write_fake_maven(&bare, &[], &ignored_log);
    let output = jails_cmd(&root, Some(&bare))
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
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        root.join("src/main/resources/db/migration/V002__drop_tasks.sql")
            .is_file()
    );
    assert!(
        read_log(&ignored_log).is_empty(),
        "{}",
        read_log(&ignored_log)
    );
    // Dropped, not preserved: the declaration is gone from the model and the
    // forward migration is the only record of the table it used to have.
    let status = common::resource_status(&root, "Task");
    assert_eq!(status["declaration"], "absent", "{status}");
    let model = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(!model.contains("entity Task"), "{model}");
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

    let record_path = common::generated(&root, "src/main/java/com/example/demo/domain/Post.java");
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
    // The repository adapter follows the field. With no SQL storage declared
    // that adapter is the in-memory one, which is the whole of what a
    // repository facet means on a project with no database.
    let adapter = common::read_generated(
        &root,
        "src/main/java/com/example/demo/adapters/InMemoryPostRepository.java",
    );
    assert!(adapter.contains("Post"), "{adapter}");
    let response = common::read_generated(
        &root,
        "src/main/java/com/example/demo/web/PostResponse.java",
    );
    assert!(response.contains("createdAt"), "{response}");

    // Removal is model subtraction, so it takes the declaration with it.
    // There is one declaration and the projections are views of it -- so
    // `destroy scaffold` removes the entity, and keeping the record means not
    // removing it.
    let destroy = jails_cmd(&root, None)
        .args(["destroy", "scaffold", "Post", "--force"])
        .status()
        .unwrap();
    assert!(destroy.success());
    assert!(
        !record_path.exists(),
        "the declaration survived its removal"
    );
    assert!(
        !common::generated(
            &root,
            "src/main/java/com/example/demo/web/PostController.java"
        )
        .exists()
    );

    // And the way back is the way in: declaring it again reuses nothing and
    // refuses nothing, because the model no longer describes it.
    let again = jails_cmd(&root, None)
        .args(["generate", "record", "Post", "id:uuid@pk", "title:string!"])
        .output()
        .unwrap();
    assert!(again.status.success(), "{again:?}");
    assert!(record_path.is_file());
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
    let status = common::generated(&root, "src/main/java/com/example/demo/domain/Status.java");
    let before = snapshot_tree(&root);

    let refused = jails_cmd(&root, None)
        .args(["destroy", "enum", "Status", "--force"])
        .output()
        .unwrap();

    // The refusal names the field that would be left pointing at nothing, and
    // the two ways out: declare the type again, or write it yourself.
    assert!(!refused.status.success(), "{refused:?}");
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(stderr.contains("status:Status"), "{stderr}");
    assert!(stderr.contains("nothing declares"), "{stderr}");
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

    // And the way out exists: retiring the association *appends* `drop
    // constraint`, which is the next migration rather than the un-running of
    // one, so neither half of an association is stuck behind the other.
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
    // **`drop constraint`, not `drop constraint if exists`.** A forward
    // migration runs once against the schema the ones before it built, so
    // `if exists` cannot make it safer -- it can only turn "this constraint is
    // not what the accepted model says it is" into a silent success.
    let drop_constraint =
        root.join("src/main/resources/db/migration/V004__drop_fk_children_child_parent.sql");
    let sql = fs::read_to_string(&drop_constraint).unwrap();
    assert!(sql.contains("alter table children"), "{sql}");
    assert!(
        sql.contains("drop constraint fk_children_child_parent"),
        "{sql}"
    );
    assert!(
        root.join("src/main/resources/db/migration/V003__add_fk_children_child_parent.sql")
            .is_file(),
        "the migration that added the constraint is append-only and stays"
    );

    // The refusal is gone once the dependant is retired.
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

    // `dto` and `repo` project an entity that has to exist first, and say so
    // by name. `scaffold` declares one, so what it refuses on is the shape:
    // an entity with no fields has no primary key for a repository to store
    // rows by, which is the more specific answer of the two.
    for kind in ["dto", "repo"] {
        let output = jails_cmd(&root, None)
            .args(["generate", kind, "Missing"])
            .output()
            .unwrap();
        assert!(!output.status.success(), "{kind} unexpectedly succeeded");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("fix:"), "{kind}: {stderr}");
        assert!(stderr.contains("g record Missing"), "{kind}: {stderr}");
    }
    let scaffolded = jails_cmd(&root, None)
        .args(["generate", "scaffold", "Missing"])
        .output()
        .unwrap();
    assert!(!scaffolded.status.success());
    let stderr = String::from_utf8_lossy(&scaffolded.stderr);
    assert!(stderr.contains("needs exactly one `@pk` field"), "{stderr}");
    assert!(stderr.contains("fix:"), "{stderr}");
}

#[test]
fn generate_field_updates_unchanged_derivatives_preserves_edits_and_adds_a_migration() {
    let root = temp_dir("generate-field");
    write_spring_fixture(&root);
    common::declare_storage(&root);

    let scaffold = jails_cmd(&root, None)
        .args(["g", "scaffold", "Note", "id:uuid@pk", "title:string!"])
        .status()
        .unwrap();
    assert!(scaffold.success());

    let request = common::generated(&root, "src/main/java/com/example/demo/web/NoteRequest.java");
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
        String::from_utf8_lossy(&refused.stderr).contains("needs a backfill"),
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
            "createdAt:instant@default(now())",
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
    // and the migration that adds the column. The verb is `write`: the file
    // is managed, it exists, and the plan is rewriting jails' own output over
    // it.
    assert!(stdout.contains("write "), "{stdout}");
    assert!(stdout.contains(".sql"), "{stdout}");

    let record = common::read_generated(&root, "src/main/java/com/example/demo/domain/Note.java");
    assert!(record.contains("Instant createdAt"), "{record}");
    let jdbc = common::read_generated(
        &root,
        "src/main/java/com/example/demo/adapters/JdbcNoteRepository.java",
    );
    assert!(jdbc.contains("created_at"), "{jdbc}");
    // The edited derivative is *merged*, not skipped: skipping would preserve
    // the edit and leave the DTO missing the component the record just grew.
    // Both halves survive: the reader's line and the new field.
    let merged = fs::read_to_string(&request).unwrap();
    assert!(merged.contains("// user-owned validation"), "{merged}");
    // The new component reaches the *response*, and deliberately not the
    // request: `@default(now())` is server-assigned, so a request record
    // carrying it would invite a caller to declare its own creation time.
    assert!(!merged.contains("Instant createdAt"), "{merged}");
    let response = common::read_generated(
        &root,
        "src/main/java/com/example/demo/web/NoteResponse.java",
    );
    assert!(response.contains("Instant createdAt"), "{response}");
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
    // And the declared default reaches the column. `create table` renders
    // `default current_timestamp` for the same `@default(now())` field, so a
    // column added later has to carry it too -- otherwise the schema depends
    // on when the field was declared rather than on what it declares.
    assert!(
        migration.contains("alter column created_at set default current_timestamp"),
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
    common::declare_storage(&root);
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

    let record = common::read_generated(&root, "src/main/java/com/example/demo/domain/Task.java");
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
    // The suggestion is in jails' own field syntax, where a lowercase type
    // names a builtin: `author:UUID` would be read as a project type called
    // `UUID`, which is the mistake this refusal exists to prevent.
    assert!(stderr.contains("author:uuid"), "{stderr}");
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
    common::declare_storage(&root);

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
    let record = common::read_generated(&root, "src/main/java/com/example/demo/domain/Note.java");
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
    let adapter = common::read_generated(
        &root,
        "src/main/java/com/example/demo/adapters/JdbcRenameNoteTransition.java",
    );
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
                "createdAt:instant@default(now())",
            ])
            .status()
            .unwrap()
            .success()
    );
    let requests = common::read_generated(&root, "requests/notes.http");
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
    // **A server-assigned column is not in the request body.** `createdAt`
    // carries `@default(now())`, so the endpoint mints it and the request
    // record has no component for it -- a collection that offered one would
    // be teaching the reader to send a value the API ignores.
    assert!(!requests.contains("createdAt"), "{requests}");
    assert!(
        requests.contains("\"title\": \"sample-title\""),
        "{requests}"
    );

    assert!(
        jails_cmd(&root, None)
            .args(["g", "factory", "Note"])
            .status()
            .unwrap()
            .success()
    );
    let factory = common::read_generated(
        &root,
        "src/test/java/com/example/demo/testkit/NoteFactory.java",
    );
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
/// actually executes it: a skipped test reports as a pass.
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
    common::declare_storage(&root);

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
/// had done.
#[test]
fn planning_pretending_and_inspecting_leave_machine_state_byte_for_byte() {
    let root = temp_dir("read-purity");
    write_spring_fixture(&root);
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
        vec!["destroy", "record", "Note", "--pretend", "--force"],
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
    // The committed store is the model and the lock that seals the projection
    // it was compiled from, and nothing else.
    assert!(root.join(".jails/model.jdl").is_file());
    assert!(root.join(".jails/compiler.lock.json").is_file());
    assert!(!root.join(".jails/ledger.toml").exists());
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
    // both", so the timestamps must not arrive as `@NotNull` wire components:
    // the documented POST would answer 400 naming two fields the caller has
    // no business setting.
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
        common::read_generated(&root, "src/main/java/com/example/demo/web/NoteRequest.java");
    assert!(!request.contains("Instant createdAt"), "{request}");
    assert!(!request.contains("Instant updatedAt"), "{request}");
    assert!(
        request.contains("Instant now = Instant.now();"),
        "{request}"
    );

    // The record still declares them, and the response still returns them: the
    // server sets these, it does not hide them.
    let record = common::read_generated(&root, "src/main/java/com/example/demo/domain/Note.java");
    assert!(record.contains("Instant createdAt"), "{record}");
    let response = common::read_generated(
        &root,
        "src/main/java/com/example/demo/web/NoteResponse.java",
    );
    assert!(response.contains("Instant createdAt"), "{response}");

    // And the sendable collection describes a request that can be made -- as
    // does the generated controller test, which sends that same body.
    let requests = common::read_generated(&root, "requests/notes.http");
    assert!(!requests.contains("createdAt"), "{requests}");
    let controller_test = common::read_generated(
        &root,
        "src/test/java/com/example/demo/web/NoteControllerTest.java",
    );
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
    // it is a `jails g query`, and the collection must not end with a `###
    // List` block for the GET the generated controller test asserts is a 405.
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

    let requests = common::read_generated(&root, "requests/notes.http");
    assert!(requests.contains("POST {{baseUrl}}/notes"), "{requests}");
    assert!(!requests.contains("GET {{baseUrl}}"), "{requests}");

    let controller_test = common::read_generated(
        &root,
        "src/test/java/com/example/demo/web/NoteControllerTest.java",
    );
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
    // Refused for the reason that comes first: nothing declares `Author`, so
    // the record naming it would not compile whether or not it can be stored.
    assert!(stderr.contains("nothing declares"), "{stderr}");
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

/// The confirmation is on the thing that is irreversible. Removing a
/// generated class is model subtraction: the declaration goes, the next
/// compilation renders the tree without it, and running the command again puts
/// it back -- so there is nothing to confirm, and a prompt on every removal is
/// a prompt people learn to answer without reading.
///
/// Dropping a table is not reversible, and that is where the question is
/// asked: `--storage drop` states the policy and `--confirm-table` names the
/// table. A retirement with neither is refused rather than assumed.
#[test]
fn destroying_stored_data_needs_the_policy_and_the_table_named() {
    let root = temp_dir("destroy-confirm");
    write_spring_fixture(&root);
    common::declare_storage(&root);
    assert!(
        jails_cmd(&root, None)
            .args(["g", "scaffold", "Note", "id:uuid@pk", "title:string!"])
            .status()
            .unwrap()
            .success()
    );
    let record = common::generated(&root, "src/main/java/com/example/demo/domain/Note.java");
    assert!(record.is_file());

    let refused = jails_cmd(&root, None)
        .args(["destroy", "scaffold", "Note", "--force"])
        .output()
        .unwrap();
    assert!(!refused.status.success(), "{refused:?}");
    let told = String::from_utf8_lossy(&refused.stderr);
    assert!(told.contains("storage-policy-required"), "{told}");
    assert!(told.contains("--storage preserve"), "{told}");
    assert!(told.contains("--confirm-table notes"), "{told}");
    assert!(record.is_file(), "a refused retirement wrote nothing");

    // Naming the wrong table is the same refusal, not a typo jails forgives.
    let mistyped = jails_cmd(&root, None)
        .args([
            "destroy",
            "scaffold",
            "Note",
            "--storage",
            "drop",
            "--confirm-table",
            "note",
            "--force",
        ])
        .output()
        .unwrap();
    assert!(!mistyped.status.success(), "{mistyped:?}");
    assert!(record.is_file(), "a refused retirement wrote nothing");

    let dropped = jails_cmd(&root, None)
        .args([
            "destroy",
            "scaffold",
            "Note",
            "--storage",
            "drop",
            "--confirm-table",
            "notes",
            "--force",
        ])
        .output()
        .unwrap();
    assert!(dropped.status.success(), "{dropped:?}");
    assert!(!record.is_file());
}

#[test]
fn generate_twice_writes_nothing_the_second_time() {
    let root = temp_dir("duplicate");
    write_spring_fixture(&root);
    jails_cmd(&root, None)
        .args(["generate", "service", "comment"])
        .status()
        .unwrap();
    // Resolved after the write, not before: which tree holds a generated file
    // is a fact about the project as it is now.
    let service = common::generated(
        &root,
        "src/main/java/com/example/demo/service/CommentService.java",
    );
    let before = fs::read_to_string(&service).unwrap();
    let output = jails_cmd(&root, None)
        .args(["generate", "service", "comment"])
        .output()
        .unwrap();
    // A second identical generate is a no-op, not a refusal: the file is
    // owned by the declaration that wrote it, so "nothing changed" is the
    // honest answer -- and an edited file is three-way merged rather than
    // refused, which `app_manifest_merges_an_edited_intent_over_user_changes`
    // pins.
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
    let told = String::from_utf8_lossy(&output.stderr);
    assert!(told.contains("not a Java project"), "{told}");
    assert!(told.contains("jails new"), "{told}");
}

#[test]
fn short_generators_cover_raw_sql_and_test_seams() {
    let root = temp_dir("simple-generators");
    write_spring_fixture(&root);
    common::declare_storage(&root);

    for args in [
        vec!["g", "interface", "IdGenerator"],
        vec!["g", "integration-test", "DatabaseSmoke"],
        vec!["g", "migration", "createRewardCore"],
        vec!["g", "mig", "add_outbox"],
        // The record first, then the port over it: a repository derives every
        // column from the entity it stores, so naming fields here would be a
        // second place for the shape to be stated and disagree.
        vec!["g", "record", "Reward", "id:uuid@pk", "name:string!"],
        vec!["g", "repository", "Reward"],
    ] {
        let output = jails_cmd(&root, None).args(&args).output().unwrap();
        assert!(
            output.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    assert!(common::generated(&root, "src/main/java/com/example/demo/IdGenerator.java").is_file());
    assert!(
        common::generated(&root, "src/test/java/com/example/demo/DatabaseSmokeIT.java").is_file()
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
        common::generated(
            &root,
            "src/main/java/com/example/demo/repository/RewardRepository.java"
        )
        .is_file()
    );
    let adapter = common::read_generated(
        &root,
        "src/main/java/com/example/demo/adapters/JdbcRewardRepository.java",
    );
    // **Spring's `JdbcClient`, and no ORM.** The statements are written out --
    // one column list feeding the select, the insert and the row mapper -- so
    // what the adapter does is readable in the file; what it is not is a
    // mapping layer that decides for you.
    assert!(adapter.contains("JdbcClient"), "{adapter}");
    assert!(
        adapter.contains("select id, name from rewards"),
        "{adapter}"
    );
    assert!(adapter.contains("insert into rewards"), "{adapter}");
    assert!(!adapter.contains("EntityManager"), "{adapter}");
    assert!(!adapter.contains("@Entity"), "{adapter}");
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

/// A standalone `generate controller` produces a test, and the
/// controller/service/record companion tests compile on real Maven.
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
        // both are about the session registry.
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
    // Under `.jails/generated`: reproducible output is merge-managed there,
    // compiled through an added source root. A project created from an
    // application manifest writes into `src`, which is what
    // `verified_plain_toolbox` below is.
    assert!(
        root.join(".jails/generated/main/java/com/example/demo/MoneyMoved.java")
            .exists()
    );
    assert!(
        root.join(".jails/generated/test/java/com/example/demo/MoneyMovedTest.java")
            .exists()
    );
    // And the negatives, which are what a regression actually trips over: the
    // reader's own tree is untouched, and no `.jails/ledger.toml` appears.
    assert!(
        !root
            .join("src/main/java/com/example/demo/MoneyMoved.java")
            .exists(),
        "the class was written into the reader's own tree"
    );
    assert!(
        !root.join(".jails/ledger.toml").exists(),
        "a canonical project was given a legacy ledger"
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
        vec!["generate", "enum", "ccy", "GBP", "EUR"],
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
            "currency:Ccy",
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
        root.join(".jails/generated/main/java/com/example/gym/domain/CanonicalTransaction.java"),
    )
    .unwrap();
    assert!(
        value.contains("Ccy currency"),
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
    let output = jails_cmd_with_path(&root, &path)
        .args(["generate", "value", "bad", "occurredOn:date!"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let refusal = String::from_utf8_lossy(&output.stderr);
    // The refusal comes from the model's own diagnostics. What has to hold is
    // that the mistake is named and carries a `fix:` line rather than being
    // silently ignored.
    assert!(
        refusal.contains("valid only for builtin `string` fields"),
        "{refusal}"
    );
    assert!(refusal.contains("fix: remove `non_blank`"), "{refusal}");

    // An enum-typed component can be sampled by reading the enum, and a
    // component whose type is a record *this project already has* by reading
    // the record: `SourceRef` was generated two commands ago, so refusing to
    // build one would be the tool forgetting what it just wrote.
    let test =
        fs::read_to_string(root.join(
            ".jails/generated/test/java/com/example/gym/domain/CanonicalTransactionTest.java",
        ))
        .unwrap();
    // The constant by name rather than by position: `values()[0]` starts
    // standing for a different value the moment somebody reorders the enum,
    // and nothing in the diff says so.
    // `Ccy`, not `Currency`: the builtin table lists `Currency` as an alias of
    // the `currency` builtin, so `currency:Currency` resolves to
    // `java.util.Currency`. The property this test is about -- an enum
    // component sampled *by name* -- does not depend on which enum it is.
    assert!(test.contains("Ccy.GBP"), "{test}");
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
    let stamped = fs::read_to_string(
        root.join(".jails/generated/test/java/com/example/gym/domain/StampedTest.java"),
    )
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
    // The JDBC adapter is what `storage postgres` renders; without a
    // database there is an in-memory repository and nothing to bind SQL to.
    assert!(
        jails_cmd(&root, None)
            .args(["add", "db", "--no-start"])
            .status()
            .unwrap()
            .success()
    );

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

    let adapter = common::read_generated(
        &root,
        "src/main/java/com/example/demo/adapters/jdbc/JdbcPayoutRepository.java",
    );
    // Derived, not left as a TODO.
    assert!(
        !adapter.contains("UnsupportedOperationException"),
        "{adapter}"
    );
    // The write expression bakes in the receiver rather than letting the
    // caller prefix it: `Timestamp.from` puts the receiver in the middle, and
    // gluing it on the front yields `value.Timestamp.from(paidAt())`, which
    // reads fine and does not compile.
    assert!(
        adapter.contains("Timestamp.from(value.paidAt())"),
        "{adapter}"
    );
    // An Optional component is unwrapped on the way out; the way back in is
    // the mapper's, and the round trip is proved against a real database by
    // the integration test below rather than by reading the SQL.
    assert!(adapter.contains("value.note().orElse(null)"), "{adapter}");
    // One column list feeds the select, the insert and the upsert, so they
    // cannot drift -- `amount` in one against `amount_minor` in another
    // compiles and fails at run time.
    let columns = "id, amount, currency, paid_at, note";
    assert!(
        adapter.contains(&format!("insert into payouts ({columns})")),
        "{adapter}"
    );
    assert!(
        adapter.contains(&format!("select {columns} from payouts where id = :id")),
        "{adapter}"
    );
    assert!(
        adapter.contains(&format!("returning {columns}")),
        "{adapter}"
    );

    // The DTOs name the project's own enum, so they have to import it --
    // `field.imports` only carries the built-in types' packages.
    let request = common::read_generated(
        &root,
        "src/main/java/com/example/demo/web/PayoutRequest.java",
    );
    assert!(
        request.contains("import com.example.demo.domain.Currency;"),
        "{request}"
    );

    let verified = verified_spring_db_toolbox(&path);
    assert!(
        common::compiled_class(
            verified,
            "src/main/java/com/example/demo/adapters/JdbcPayoutRepository.java"
        )
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
    common::declare_storage(&root);

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
    assert!(migration.contains("note text\n"), "{migration}");
    assert_eq!(
        migration.matches("not null").count(),
        3,
        "only the nullable component may lack `not null`: {migration}"
    );
    // The key is declared on its own column rather than as a separate table
    // constraint. A composite key would need the clause; a single-column one
    // reads as what it is.
    assert!(
        migration.contains("id uuid not null primary key"),
        "{migration}"
    );

    // The same column names the adapter selects and inserts.
    let adapter = common::read_generated(
        &root,
        "src/main/java/com/example/demo/adapters/jdbc/JdbcPayoutRepository.java",
    );
    for column in ["id", "amount", "paid_at", "note"] {
        assert!(migration.contains(column), "migration missing {column}");
        assert!(adapter.contains(column), "adapter missing {column}");
    }
}

/// **One precondition, both absences.** Neither the migration nor the fixture
/// is withheld because its directory is missing -- nothing here creates or
/// removes a directory. Both are withheld because the model declares no
/// storage, which is the same condition reached by the same command, so
/// asking it twice under two names was one test written out twice.
#[test]
fn a_project_with_no_declared_storage_gets_neither_migration_nor_fixture() {
    let root = temp_dir("scaffold-no-storage");
    write_spring_fixture(&root);

    let output = jails_cmd(&root, None)
        .args(["generate", "scaffold", "Payout", "id:uuid@pk"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(!root.join("src/main/resources/db/migration").exists());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("created migration"), "{stdout}");
    assert!(!stdout.contains("created fixture"), "{stdout}");
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
    let file = common::generated(&root, "src/main/java/com/example/demo/domain/Payout.java");
    assert!(file.is_file());

    let output = jails_cmd(&root, None)
        .args(["destroy", "record", "Payout", "--pretend", "--force"])
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
    // The fixture is the `seed` projection, which needs a database to seed
    // into -- so it is declared rather than implied by the profile.
    assert!(
        jails_cmd(&root, None)
            .args(["add", "db", "--no-start"])
            .status()
            .unwrap()
            .success()
    );
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
    assert!(
        jails_cmd(&root, None)
            .args(["add", "json"])
            .status()
            .unwrap()
            .success()
    );
    let seeded = jails_cmd(&root, None)
        .args(["g", "seed", "Payout"])
        .output()
        .unwrap();
    assert!(seeded.status.success(), "{seeded:?}");

    let fixture = common::read_generated(&root, "src/main/resources/db/seeds/payouts.json");
    // **Component names, not column names.** The seeder reads the file into
    // the record and saves it through the port, so a row the record rejects
    // fails at start-up rather than sitting in the table -- and the keys are
    // the ones the record declares.
    assert!(fixture.contains("\"paidAt\""), "{fixture}");
    assert!(!fixture.contains("paid_at"), "{fixture}");
    // A real constant read off the generated enum, not a guess.
    assert!(fixture.contains("\"currency\": \"GBP\""), "{fixture}");
    // Two rows, and the nullable one is null in the second.
    assert!(fixture.contains("\"note\": \"sample\""), "{fixture}");
    assert!(fixture.contains("\"note\": null"), "{fixture}");
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

    let test = common::read_generated(
        &root,
        "src/test/java/com/example/demo/web/PayoutControllerTest.java",
    );
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

    let request = common::read_generated(
        &root,
        "src/main/java/com/example/demo/web/PayoutRequest.java",
    );
    // Constraints come from the field spec, so a bad request is rejected at
    // the edge rather than deep in the domain.
    assert!(request.contains("@NotNull UUID id"), "{request}");
    // An Optional domain component is a plain nullable field on the wire, and
    // carries no constraint -- `?` said it was optional.
    assert!(request.contains("String note"), "{request}");
    assert!(!request.contains("@NotNull String note"), "{request}");
    assert!(request.contains("Optional.ofNullable(note)"), "{request}");

    let client = common::read_generated(
        &root,
        "src/main/java/com/example/demo/clients/BillingClient.java",
    );
    assert!(client.contains("@GetExchange"), "{client}");
    // No base URL in the annotation: it belongs to the group's configuration.
    assert!(!client.contains("@HttpExchange(url"), "{client}");
    // One registration per client, listed by type: a shared one carries a
    // single group name, so a second client would take the first's
    // configuration with it.
    let config = common::read_generated(
        &root,
        "src/main/java/com/example/demo/clients/BillingClientConfig.java",
    );
    assert!(
        config.contains("@ImportHttpServices(group = \"billing\", types = BillingClient.class)"),
        "{config}"
    );

    let job = common::read_generated(&root, "src/main/java/com/example/demo/jobs/SweepJob.java");
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
    // The field grammar is the thing you need while typing the command, so it
    // is in `--help`. clap reflows a doc comment into one
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

/// A generator whose whole subject is an outbound HTTP call must not leave the
/// call unbounded: a base URL, a connect timeout and a read timeout are the
/// one place this cannot be left to the reader. The prefix and both timeout
/// keys are `HttpClientProperties extends HttpClientSettingsProperties` in
/// `spring-boot-http-client`, checked in `deps/spring-boot`.
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

/// `add api`'s error model is reachable from the adapters, not only from the
/// generated controller.
///
/// The *adapter* names the outcome in `spring-dao`'s own vocabulary --
/// `OptimisticLockingFailureException` for a stated `If-Match` that no longer
/// matches, `EmptyResultDataAccessException` for a row that is not there --
/// and the error model maps both. Nothing has to declare a type, the
/// controller keeps only the one status that is genuinely its own, and a
/// hand-written adapter raising the same pair gets the same answer. An error
/// model only generated controllers could throw is one a reader finds,
/// believes in, and is wrong about.
#[test]
fn a_transition_names_its_outcome_where_the_error_model_can_read_it() {
    let with_api = temp_dir("transition-api-error-model");
    write_spring_fixture(&with_api);
    let build = |root: &std::path::Path, api: bool| {
        common::declare_storage(root);
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
                "version:long@version",
            ][..],
            &[
                "g",
                "transition",
                "MarkRead",
                "id:uuid",
                "isRead:boolean",
                "version:long@version",
                "--on",
                "Msg",
            ][..],
        ] {
            let out = jails_cmd(root, None).args(args).output().unwrap();
            assert!(out.status.success(), "{out:?}");
        }
        fs::read_to_string(common::generated(
            root,
            "src/main/java/com/example/demo/adapters/JdbcMarkReadTransition.java",
        ))
        .unwrap()
    };

    // The adapter decides, in Spring's own vocabulary. Zero rows updated has
    // two causes and they are different answers -- a stated `If-Match` that
    // no longer matches is a 412, a row that is not there is a 404 -- and
    // `.single()` cannot tell them apart; one unclassified failure would
    // reach the client as a 500, which is what alerting pages on and what
    // client libraries retry, and the retry can never succeed because the
    // version has moved on.
    let adapter = build(&with_api, true);
    assert!(adapter.contains(".optional();"), "{adapter}");
    assert!(
        adapter.contains("new OptimisticLockingFailureException("),
        "{adapter}"
    );
    assert!(
        adapter.contains("new EmptyResultDataAccessException("),
        "{adapter}"
    );

    // Both are `spring-dao`'s, on the classpath the moment the JDBC starter
    // is -- so the error model maps them without either side declaring a type,
    // and a hand-written adapter raising the same pair gets the same answer.
    let advice = common::read_generated(
        &with_api,
        "src/main/java/com/example/demo/api/ApiExceptionHandler.java",
    );
    assert!(
        advice.contains("@ExceptionHandler(OptimisticLockingFailureException.class)"),
        "{advice}"
    );
    assert!(
        advice.contains("HttpStatus.PRECONDITION_FAILED"),
        "{advice}"
    );
    assert!(
        advice.contains("@ExceptionHandler(EmptyResultDataAccessException.class)"),
        "{advice}"
    );
    assert!(advice.contains("HttpStatus.NOT_FOUND"), "{advice}");

    // The controller keeps exactly one status of its own, and only that one:
    // a malformed `If-Match` is a 400 because jails could not read the
    // request, while every outcome above is about a request it read.
    let controller = common::read_generated(
        &with_api,
        "src/main/java/com/example/demo/web/MarkReadController.java",
    );
    assert!(
        controller.contains("If-Match is not a version this resource issued"),
        "{controller}"
    );
    assert!(
        !controller.contains("ResponseStatusException(HttpStatus.NOT_FOUND"),
        "{controller}"
    );
    assert!(
        !controller.contains("ResponseStatusException(HttpStatus.CONFLICT"),
        "{controller}"
    );

    // Without the error model the adapter is unchanged: what it raises is a
    // fact about the database, and which HTTP status that becomes is the
    // `api` capability's business. Nothing here names `ApiException`.
    let without = temp_dir("transition-no-error-model");
    write_spring_fixture(&without);
    let plain = build(&without, false);
    assert!(!plain.contains("ApiException"), "{plain}");
    assert!(
        plain.contains("new OptimisticLockingFailureException("),
        "{plain}"
    );
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
    // do not contradict each other. The placement is the same on plain
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
    write_spring_fixture(&root);

    // **The type the port takes has to be one something declares.** A
    // strategy over a `Transaction` nothing declares is an interface naming a
    // type that is neither in the model nor in the reader's own sources, and
    // the file does not compile -- so it is refused rather than written.
    assert!(
        jails_cmd(&root, None)
            .args(["g", "record", "Transaction", "id:uuid@pk", "amount:long"])
            .status()
            .unwrap()
            .success()
    );
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
    let domain = common::generated(&root, "src/main/java/com/example/demo/domain");
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
}

/// A reader's own implementation is theirs, and the removal says so.
///
/// An implementation of a deleted interface stops the project compiling, and
/// it is not jails' file: the port lives under `.jails/generated` and a class
/// in `src/main/java` implementing it is the reader's. So the file survives
/// and the removal names it.
#[test]
fn destroy_names_the_reader_source_a_removal_strands() {
    let root = temp_dir("destroy-strategy-reader");
    write_spring_fixture(&root);

    for args in [
        vec!["g", "record", "Transaction", "id:uuid@pk", "amount:long"],
        vec![
            "g",
            "strategy",
            "RewardRule",
            "Coffee",
            "--on",
            "Transaction",
        ],
    ] {
        assert!(
            jails_cmd(&root, None)
                .args(&args)
                .status()
                .unwrap()
                .success(),
            "{args:?}"
        );
    }

    let mine = root.join("src/main/java/com/example/demo/domain");
    fs::create_dir_all(&mine).unwrap();
    fs::write(
        mine.join("HandWrittenRewardRule.java"),
        "package com.example.demo.domain;\n\n\
         public final class HandWrittenRewardRule implements RewardRule {\n\
         \x20   @Override\n\
         \x20   public boolean matches(Transaction transaction) {\n\
         \x20       return false;\n\
         \x20   }\n\
         }\n",
    )
    .unwrap();

    let removed = jails_cmd(&root, None)
        .args(["destroy", "strategy", "RewardRule", "--force"])
        .output()
        .unwrap();
    assert!(removed.status.success(), "{removed:?}");
    let stderr = String::from_utf8_lossy(&removed.stderr);
    assert!(stderr.contains("HandWrittenRewardRule.java"), "{stderr}");
    assert!(stderr.contains("`RewardRule`"), "{stderr}");
    assert!(
        mine.join("HandWrittenRewardRule.java").is_file(),
        "jails deleted a file it does not own"
    );
}

/// `--pretend` has to name every write, `package-info.java` included: the
/// preview and the apply consume the same list, rather than the preview
/// predicting a side effect, because a second piece of code guessing what the
/// first will do is drift.
#[test]
fn pretend_names_the_package_info_it_will_write() {
    let root = temp_dir("pkginfo-preview");
    write_spring_fixture(&root);
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
        common::generated(
            &root,
            "src/main/java/com/example/demo/domain/package-info.java"
        )
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

/// A template override: the want is "change what the generated code *looks
/// like*" -- not a new generator, just this class shaped differently.
#[test]
fn a_project_template_override_replaces_the_built_in_and_doctor_names_it() {
    let root = temp_dir("template-override");
    write_spring_fixture(&root);

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
    let generated = common::read_generated(
        &root,
        "src/test/java/com/example/demo/cli/SyncCommandTest.java",
    );
    // The override's own first line, under the provenance line every managed
    // file carries: that header is how `model eject` and the three-way merge
    // find the artifact, so it is jails' whatever the body says.
    let mut lines = generated.lines();
    assert!(
        lines
            .next()
            .is_some_and(|line| line.contains("Generated by jails from art_")),
        "{generated}"
    );
    assert_eq!(
        lines.next(),
        Some("// generated by an overridden template"),
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
    write_spring_fixture(&root);

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

/// Every companion test picks its MockMvc form by version: `MockMvcTester` is
/// Spring Framework 6.2 (Boot 3.4), and `jails new --gradle --boot 2.7.18`
/// exists so that a Boot 2 project can be worked in, so the classic form is
/// written there rather than a refusal.
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
    let test = common::read_generated(
        &root,
        "src/test/java/com/acme/svc/web/FooControllerTest.java",
    );
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
    let cors_test = common::read_generated(&root, "src/test/java/com/acme/svc/CorsConfigTest.java");
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

    // `g scaffold` writes the classic form, and the whole of what it
    // generates compiles -- proved against real Maven by
    // `what_jails_generates_for_boot_2_compiles_and_what_cannot_refuses_by_name`.
    // `add api` and `add security` refuse, for a reason that is not about
    // MockMvc at all: their *main* source set is Boot 3 code.
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
    let source = common::read_generated(
        &root,
        "src/test/java/com/acme/svc/web/NoteControllerTest.java",
    );
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

/// `g auth`. Both claims behind it are behavioural, so a
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

/// `g webhook`. Seven tests, and each is one of the ways an
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

/// `g search`. The generated column is the whole point, and
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
        common::compiled_class(
            verified,
            "src/main/java/com/example/demo/adapters/JdbcArticleRepository.java"
        )
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
/// The Boot floor is in the generated *code*, not its tests: `add api` writes
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
            "version:long@version",
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

    // The ones that cannot: their *main* source set is Boot 3 code, which no
    // test variant can help. The refusal names the type the compiler would
    // have named.
    //
    // `g query` and `g transition` are not on this list, structurally: the
    // `JdbcClient` adapter behind an operation port arrives with the `db`
    // capability, and
    // `db` is itself refused here -- `spring-boot-testcontainers` first
    // appeared in Boot 3.1. So the floor is enforced one step earlier, on the
    // declaration that would bring the Framework 6.1 type in, and the
    // operations stay declarable as what they are: ports.
    for (command, needs) in [
        (vec!["add", "api", "--no-start"], "ProblemDetail"),
        (vec!["add", "security", "--no-start"], "requestMatchers"),
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
    let no_storage = jails_cmd_with_path(&root, &path)
        .args(["add", "db", "--no-start"])
        .output()
        .unwrap();
    assert!(!no_storage.status.success(), "{no_storage:?}");
    let stderr = String::from_utf8_lossy(&no_storage.stderr);
    assert!(stderr.contains("spring-boot-testcontainers"), "{stderr}");
    assert!(stderr.contains("Spring Boot 2.7"), "{stderr}");

    // And with no adapter to render, the two operation declarations are
    // ordinary: a port, its `Input`, and nothing that names Framework 6.1.
    for command in [
        vec![
            "g",
            "query",
            "NotesByStatus",
            "status:NoteStatus",
            "--on",
            "Note",
        ],
        vec![
            "g",
            "transition",
            "ChangeNoteStatus",
            "id:uuid",
            "status:NoteStatus",
            "version:long@version",
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
    assert!(
        !common::managed_listing(&root).contains("JdbcClient"),
        "{}",
        common::managed_listing(&root)
    );

    // Not one of them may name the Framework 6.2 entry point. Both trees:
    // the fixture's own test is reader source and everything jails wrote is
    // under the managed tree, and a walk of one of them would report a clean
    // result over the half that could not contain the problem.
    let mut checked = 0;
    let mut stack = vec![
        root.join("src/test/java"),
        root.join(".jails/generated/test/java"),
    ];
    while let Some(dir) = stack.pop() {
        if !dir.is_dir() {
            continue;
        }
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

/// A project whose schema is `schema.sql` is told, not written into.
///
/// `schema.sql` is a file jails does not manage, so it stays the reader's
/// byte for byte, and the resource says out loud that its rows live in memory
/// -- silence would hand the reader a repository, a JDBC adapter and an `IT`
/// against a table that does not exist. `storage postgres` is the declaration
/// that makes jails responsible for a schema, and it keeps that history in
/// forward migrations where a re-applied script cannot exist.
///
/// **A project with no destination at all takes the same branch**, so this is
/// the whole of it. The diagnostic keys on the model's dialect and never
/// looks at `schema.sql`, which is why running the identical command against
/// a project without one asserted nothing this does not.
#[test]
fn a_scaffold_leaves_an_unmanaged_schema_sql_alone_and_says_where_its_rows_live() {
    let root = temp_dir("schema-sql-ddl");
    write_spring_fixture(&root);
    let resources = root.join("src/main/resources");
    fs::create_dir_all(&resources).unwrap();
    let schema_path = resources.join("schema.sql");
    let reader_schema = "create table users (\n  id bigint primary key\n);";
    fs::write(&schema_path, reader_schema).unwrap();

    let output = jails_cmd(&root, None)
        .args(["g", "scaffold", "Ticket", "id:long@pk", "subject:string!"])
        .output()
        .unwrap();
    // Not a refusal: an in-memory resource is a legitimate shape to want, and
    // it is also what a reader who has not run `jails add db` yet gets, with
    // nothing else to tell the two apart. The diagnostic names the table the
    // reader does not have, which is the part that makes it actionable.
    let told = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(output.status.success(), "{told}");
    assert!(told.contains("stored in memory only"), "{told}");
    assert!(told.contains("create table tickets"), "{told}");
    assert!(told.contains("jails add db"), "{told}");

    // Byte for byte, marker-free: a file jails does not manage is one it does
    // not touch, and a `-- jails:` marker in somebody else's schema is a claim
    // on bytes nothing would ever take back.
    assert_eq!(fs::read_to_string(&schema_path).unwrap(), reader_schema);
    assert!(!root.join("src/main/resources/db/migration").exists());
}

/// The wire contract, both directions, on the project that needs it.
///
/// Spring's **data binder** has no naming strategy:
/// `spring.jackson.property-naming-strategy` configures Jackson, so a project
/// whose JSON is `user_id` still binds a form field called `userId` unless
/// the record's component says otherwise. Two names for one value on one
/// wire, and it is silent -- the component simply arrives null.
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
        fs::read_to_string(common::generated(
            root,
            "src/main/java/com/example/demo/service/OpenTicketCommand.java",
        ))
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

/// The wire value of an enum, in its three shapes: a JSON body, a form field,
/// and a path or query parameter.
///
/// The Java name and the wire value are two different things, and treating
/// them as one fails quietly: an enum whose constants are `OPEN` and
/// `IN_PROGRESS` serialises as `"OPEN"`, the page reads `"open"`, and the
/// badge is simply blank. A form carrying `status=open` binds, the response
/// carries `"status":"open"`, and `status=nope` is a 400 rather than a null.
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

    let source = common::read_generated(
        &root,
        "src/main/java/com/example/demo/domain/IssueStatus.java",
    );
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
    let converter = common::read_generated(
        &root,
        "src/main/java/com/example/demo/web/IssueStatusConverter.java",
    );
    assert!(
        converter.contains("Converter<String, IssueStatus>"),
        "{converter}"
    );
    assert!(
        converter.contains("IssueStatus.fromWire(source)"),
        "{converter}"
    );

    // The generated test asserts the round trip rather than restating it.
    let test = common::read_generated(
        &root,
        "src/test/java/com/example/demo/domain/IssueStatusTest.java",
    );
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
    let source = common::read_generated(
        &plain,
        "src/main/java/com/example/demo/domain/Currency.java",
    );
    assert!(source.contains("    GBP,\n    EUR\n}"), "{source}");
    assert!(!source.contains("JsonValue"), "{source}");
    assert!(
        !plain
            .join("src/main/java/com/example/demo/web/CurrencyConverter.java")
            .exists()
    );
}

/// A path that names its filters, and the refusal that keeps it honest.
///
/// Every variable in `--path /admin_api/messages/{userId}` must bind to a
/// declared filter: a template Spring matches with nothing bound behind it
/// makes the controller look for a request body nobody sent, and a path jails
/// cannot honour is a path jails must not accept.
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

    // The route is served by the `api` capability, which is what emits a
    // Spring controller for an operation. Declaring one without it leaves a
    // linked route nothing answers -- and `db` is what implements the port
    // that controller takes.
    for capability in [["add", "db", "--no-start"], ["add", "api", "--no-start"]] {
        assert!(
            jails_cmd(&root, None)
                .args(capability)
                .status()
                .unwrap()
                .success(),
            "{capability:?}"
        );
    }

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
    let controller = common::read_generated(
        &root,
        "src/main/java/com/example/demo/adapters/http/TicketsForController.java",
    );
    // A GET with no body, and the variable actually bound. `@ModelAttribute`
    // is what binds it: Spring's `ExtendedServletRequestDataBinder` adds the
    // URI template variables to the binding values, so one annotation answers
    // `/tickets/{userId}` and `?userId=1` alike. A second `@PathVariable`
    // parameter beside it would be the same value bound twice.
    assert!(
        controller.contains("method = RequestMethod.GET"),
        "{controller}"
    );
    assert!(
        controller.contains("execute(@ModelAttribute TicketsForQuery.Input input)"),
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

    // A mix is ordinary rather than refused: one `@ModelAttribute` reads
    // both, so `/x/{userId}` with `?subject=x` is one binding with one rule.
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
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let mixed = common::read_generated(
        &root,
        "src/main/java/com/example/demo/adapters/http/MixedController.java",
    );
    assert!(mixed.contains("path = \"/x/{userId}\""), "{mixed}");
    assert!(
        mixed.contains("execute(@ModelAttribute MixedQuery.Input input)"),
        "{mixed}"
    );
}

/// A `GET` whose filters come from the query string -- the one shape a
/// browser actually sends, `GET /admin_api/users?status=open&category=Billing`.
/// `--consumes form` binds `@ModelAttribute`, and Spring fills that from
/// request *parameters*, which on a GET are the query string.
///
/// The generated test has to move with it: the controller renderer and the
/// test renderer must not decide the wire separately.
#[test]
fn a_form_bound_query_answers_a_get_and_reads_the_query_string() {
    let root = temp_dir("query-form");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join("src/main/resources")).unwrap();
    fs::write(root.join("src/main/resources/schema.sql"), "-- schema\n").unwrap();

    let status = jails_cmd(&root, None)
        .args([
            "g",
            "scaffold",
            "Ticket",
            "id:long@pk",
            "subject:string!",
            "status:string?",
        ])
        .status()
        .unwrap();
    assert!(status.success());

    // The route is served by the `api` capability, which is what emits a
    // Spring controller for an operation. Declaring one without it leaves a
    // linked route nothing answers -- and `db` is what implements the port
    // that controller takes.
    for capability in [["add", "db", "--no-start"], ["add", "api", "--no-start"]] {
        assert!(
            jails_cmd(&root, None)
                .args(capability)
                .status()
                .unwrap()
                .success(),
            "{capability:?}"
        );
    }

    let output = jails_cmd(&root, None)
        .args([
            "g",
            "query",
            "OpenTickets",
            "status:string?",
            "--on",
            "Ticket",
            "--consumes",
            "form",
            "--path",
            "/admin_api/tickets",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let controller = common::read_generated(
        &root,
        "src/main/java/com/example/demo/adapters/http/OpenTicketsController.java",
    );
    assert!(
        controller.contains("method = RequestMethod.GET"),
        "{controller}"
    );
    assert!(
        controller.contains("import org.springframework.web.bind.annotation.ModelAttribute;"),
        "{controller}"
    );
    assert!(
        controller.contains("execute(@ModelAttribute OpenTicketsQuery.Input input)"),
        "{controller}"
    );
    assert!(!controller.contains("RequestMethod.POST"), "{controller}");
    assert!(!controller.contains("RequestBody"), "{controller}");

    // The proof moves with the wire: parameters, not a JSON body, and no
    // `status=null` -- a filter is sampled as if it were present, because a
    // request that sends the four-character string proves nothing.
    let test = common::read_generated(
        &root,
        "src/test/java/com/example/demo/web/OpenTicketsQueryControllerTest.java",
    );
    assert!(test.contains("mvc.get()"), "{test}");
    assert!(test.contains(".param(\"status\", \"sample\")"), "{test}");
    assert!(!test.contains("mvc.post()"), "{test}");
    assert!(!test.contains("APPLICATION_JSON"), "{test}");

    // An enum filter is sampled by its **wire** value, not its constant. A
    // `g enum Status OPEN=open` renders `@JsonValue`, and the converter jails
    // generates beside it rejects `OPEN` -- so a proof sending the constant
    // would send a value the generated code refuses. Both wires: the JSON body of a
    // POST query reads the same sampler.
    let status = jails_cmd(&root, None)
        .args(["g", "enum", "Stage", "OPEN=open", "SHUT=shut"])
        .status()
        .unwrap();
    assert!(status.success());
    let status = jails_cmd(&root, None)
        .args([
            "g",
            "scaffold",
            "Matter",
            "id:long@pk",
            "stage:Stage",
            "note:string!",
        ])
        .status()
        .unwrap();
    assert!(status.success());
    let output = jails_cmd(&root, None)
        .args([
            "g",
            "query",
            "MattersByStage",
            "stage:Stage?",
            "--on",
            "Matter",
            "--consumes",
            "form",
            "--path",
            "/admin_api/cases",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let staged = common::read_generated(
        &root,
        "src/test/java/com/example/demo/web/MattersByStageQueryControllerTest.java",
    );
    assert!(staged.contains(".param(\"stage\", \"open\")"), "{staged}");
    assert!(!staged.contains("\"OPEN\""), "{staged}");

    // A query with no declared wire is a GET too: `@ModelAttribute` reads the
    // query string, so there is no body to carry and no reason for a verb
    // with one.
    let output = jails_cmd(&root, None)
        .args([
            "g",
            "query",
            "TicketsBySubject",
            "subject:string!",
            "--on",
            "Ticket",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let default_wire = common::read_generated(
        &root,
        "src/main/java/com/example/demo/web/TicketsBySubjectQueryController.java",
    );
    assert!(
        default_wire.contains("method = RequestMethod.GET"),
        "{default_wire}"
    );
    assert!(!default_wire.contains("RequestBody"), "{default_wire}");

    // JSON is the one shape with a body, and it is a POST: a GET with a body
    // is dropped somewhere between the caller and the handler by most of the
    // stack in between.
    let output = jails_cmd(&root, None)
        .args([
            "g",
            "query",
            "TicketsByBody",
            "subject:string!",
            "--on",
            "Ticket",
            "--consumes",
            "json",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let json = common::read_generated(
        &root,
        "src/main/java/com/example/demo/web/TicketsByBodyQueryController.java",
    );
    assert!(json.contains("method = RequestMethod.POST"), "{json}");
    assert!(
        json.contains("execute(@RequestBody TicketsByBodyQuery.Input input)"),
        "{json}"
    );
}

/// The dependency reader on a Gradle project.
///
/// A reader that answers a confident *no* to every question on Gradle gives
/// the scaffold the in-memory adapter as its `@Component` while a generated
/// query keeps its JDBC adapter, so one generated project writes to a HashMap
/// and reads from an empty database: a POST returns 201 and the matching GET
/// returns `[]`.
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

    let jdbc = common::read_generated(
        &root,
        "src/main/java/com/example/demo/adapters/jdbc/JdbcTicketRepository.java",
    );
    // The port has exactly one implementation and it is the one that talks to
    // the database the query adapter also reads. `@Repository` rather than
    // `@Component`: both halves of that annotation are true here -- it is a
    // bean, and persistence-exception translation has a `SQLException` to
    // translate.
    assert!(jdbc.contains("@Repository"), "{jdbc}");
    assert!(jdbc.contains("JdbcClient"), "{jdbc}");
    // **The in-memory double is not emitted beside it.** Two implementations
    // of one port is the ambiguity `jails beans` reports, and the compiler
    // resolves it by writing one: `jails add fake` is how a project asks for
    // the double, and then the real adapter keeps the annotation.
    assert!(
        !common::managed_listing(&root).contains("InMemoryTicketRepository"),
        "{}",
        common::managed_listing(&root)
    );
}

/// A generated integration test degrades rather than failing to compile when
/// the project has a database of its own and no Testcontainers.
///
/// A Spring app on an H2 file has JDBC on the classpath and no
/// `TestcontainersConfig`, so an unconditional
/// `@Import(TestcontainersConfig.class)` is a `cannot find symbol` in a test
/// jails wrote seconds earlier -- a compile error for a file the reader did
/// not write.
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

    let it = common::read_generated(
        &root,
        "src/test/java/com/example/demo/adapters/JdbcRoomPresenceIT.java",
    );
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
/// test, and the request record itself -- built from one list: a standalone
/// `MockMvcTester` has a plain `ObjectMapper`, which rejects a property the
/// record has no component for, so a documented body carrying the primary key
/// would be answered 400 by the test that posts it.
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
                // `@version` is what makes it the optimistic-lock column: a
                // caller who could set it would choose the version their own
                // next write is checked against.
                "version:long@version",
            ])
            .status()
            .unwrap()
            .success()
    );

    let request = common::read_generated(
        &root,
        "src/main/java/com/example/demo/web/TicketRequest.java",
    );
    let declared: Vec<&str> = ["id", "subject", "version"]
        .into_iter()
        .filter(|name| {
            request.contains(&format!(" {name})")) || request.contains(&format!(" {name},"))
        })
        .collect();
    assert_eq!(declared, vec!["subject"], "{request}");

    for path in [
        "requests/tickets.http",
        "src/test/java/com/example/demo/web/TicketControllerTest.java",
    ] {
        let text = common::read_generated(&root, path);
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
    let collection = common::read_generated(&root, "requests/tickets.http");
    assert!(collection.contains("@id = 1"), "{collection}");
    assert!(
        collection.contains("GET {{baseUrl}}/tickets/{{id}}"),
        "{collection}"
    );
}

/// A resource whose collection URL is given rather than derived.
///
/// `g scaffold User` serves `/users` and a frontend that is the contract calls
/// `/admin_api/users`; the alternative is hand-editing the controller jails
/// just wrote, which is the plumbing this tool exists to remove.
///
/// The item routes hang off the collection rather than being separately
/// derived -- `PATH + "/" + id` -- so naming one route names all four.
#[test]
fn a_scaffold_answers_on_the_collection_route_it_was_given() {
    let root = temp_dir("scaffold-named-route");
    write_spring_fixture(&root);

    assert!(
        jails_cmd(&root, None)
            .args([
                "g",
                "scaffold",
                "User",
                "id:long@pk",
                "email:string!@unique",
                "--path",
                "/admin_api/users",
            ])
            .status()
            .unwrap()
            .success()
    );

    let controller = common::read_generated(
        &root,
        "src/main/java/com/example/demo/web/UserController.java",
    );
    assert!(
        controller.contains(r#"public static final String PATH = "/admin_api/users";"#),
        "{controller}"
    );
    // Derived nowhere else: the item routes are built from PATH, so a named
    // collection cannot leave a `/users/{id}` behind it.
    assert!(!controller.contains("\"/users\""), "{controller}");

    // The editor collection is the same one value, not a second derivation --
    // which is the drift `sql::table_name` being the only pluraliser exists to
    // stop.
    let requests = common::read_generated(&root, "requests/users.http");
    assert!(
        requests.contains("POST {{baseUrl}}/admin_api/users"),
        "{requests}"
    );
    assert!(
        requests.contains("GET {{baseUrl}}/admin_api/users/{{id}}"),
        "{requests}"
    );
    assert!(!requests.contains("{{baseUrl}}/users"), "{requests}");
}

/// The component the endpoint decides, not the caller.
///
/// `POST /admin_api/messages` must write `ADMIN` and `POST
/// /customer_api/messages` must write `CUSTOMER`. With the component in the
/// request both endpoints take it from whoever calls them, so either can forge
/// the other's rows -- and no validation closes that, because a well-formed
/// request is exactly what the forgery looks like.
#[test]
fn a_pinned_component_is_written_by_the_endpoint_and_not_by_the_caller() {
    let root = temp_dir("usecase-pinned-component");
    write_spring_fixture(&root);

    for args in [
        vec!["add", "db", "--no-start"],
        vec!["g", "enum", "SenderType", "CUSTOMER", "ADMIN"],
        vec![
            "g",
            "scaffold",
            "Message",
            "id:long@pk",
            "userId:long",
            "content:string!",
            "senderType:SenderType",
        ],
        vec![
            "g",
            "usecase",
            "SendAdminMessage",
            "userId:long",
            "content:string!",
            "--on",
            "Message",
            "--set",
            "senderType=ADMIN",
        ],
    ] {
        assert!(
            jails_cmd(&root, None)
                .args(&args)
                .status()
                .unwrap()
                .success(),
            "{args:?}"
        );
    }

    // **The pin is a literal in the insert, not a constant in Java.** It has
    // one value for every call, so there is nothing for the adapter to bind
    // and nothing for a caller to override -- the column is written by the
    // statement and never leaves SQL. Its spelling is the enum's `name()`,
    // which is what a bound `SenderType` would have been converted to anyway.
    let implementation = common::read_generated(
        &root,
        "src/main/java/com/example/demo/service/StoringSendAdminMessageUseCase.java",
    );
    assert!(
        implementation.contains("sender_type) values (:user_id, :content, 'ADMIN')"),
        "{implementation}"
    );
    assert!(
        !implementation.contains(".param(\"sender_type\""),
        "a pinned column is not bound: {implementation}"
    );

    // And the request cannot carry it: a command component would be a way for
    // the caller to say something else.
    let command = common::read_generated(
        &root,
        "src/main/java/com/example/demo/service/SendAdminMessageCommand.java",
    );
    assert!(!command.contains("senderType"), "{command}");
}

/// Every way a pin can be wrong is refused by name, before anything is
/// written.
///
/// The literal is resolved against the component's *declared type*, which is
/// the whole reason `--set` is not a passthrough: `SenderType.SHOUTING` would
/// compile as text and fail as Java, in a file the reader did not write.
#[test]
fn a_pin_that_cannot_be_resolved_is_refused_and_names_what_would_work() {
    let root = temp_dir("usecase-pin-refusals");
    write_spring_fixture(&root);

    for args in [
        vec!["add", "db", "--no-start"],
        vec!["g", "enum", "SenderType", "CUSTOMER", "ADMIN"],
        vec![
            "g",
            "scaffold",
            "Message",
            "id:long@pk",
            "userId:long",
            "content:string!",
            "senderType:SenderType",
            "sentAt:instant@default(now())",
        ],
    ] {
        assert!(
            jails_cmd(&root, None)
                .args(&args)
                .status()
                .unwrap()
                .success(),
            "{args:?}"
        );
    }

    for (pin, expected) in [
        // Not a constant of that enum: the refusal lists the ones that are.
        ("senderType=SHOUTING", "CUSTOMER, ADMIN"),
        // Not a component of the target: the refusal lists the ones that are.
        ("nope=ADMIN", "has no component with that name"),
        // In the request *and* pinned: a pin the caller can override is not a
        // pin, so one of the two has to go.
        ("content=hello", "both accepts `content` and pins it"),
        // A value with a lifetime of its own. A pinned instant is a timestamp
        // frozen at generation time, which is never what anyone means.
        ("sentAt=2024-01-01T00:00:00Z", "not a value with a lifetime"),
        // Anything an expression could hide in never reaches a type at all.
        ("senderType=Sender.of", "not a constant of SenderType"),
    ] {
        let output = jails_cmd(&root, None)
            .args([
                "g",
                "usecase",
                "Probe",
                "userId:long",
                "content:string!",
                "--on",
                "Message",
                "--set",
                pin,
            ])
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success(), "{pin} was accepted");
        assert!(stderr.contains(expected), "{pin}: {stderr}");
        assert!(stderr.contains("fix:"), "{pin}: {stderr}");
    }

    // A literal that is not one is refused before any type is consulted, so
    // the message is about the value rather than about the component.
    let output = jails_cmd(&root, None)
        .args([
            "g",
            "usecase",
            "Probe",
            "userId:long",
            "--on",
            "Message",
            "--set",
            "senderType=Sender.of(x)",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("contains `(`"), "{stderr}");

    // And a recipe with no row to pin a component of refuses the flag rather
    // than accepting and ignoring it.
    let output = jails_cmd(&root, None)
        .args(["g", "record", "Probe", "a:string", "--set", "a=x"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("`--set` applies to a use case or a transition"),
        "{stderr}"
    );
}

/// A transition an ordinary browser page can reach.
///
/// `If-Match` is a conditional request header, and requiring it is a policy
/// rather than a reading of HTTP. It is jails' default policy -- the
/// compare-and-swap is what a transition *is* -- but it made every generated
/// transition unreachable from a page that sends `$.ajax({type: 'PATCH'})`,
/// because Spring answers 400 for a missing required header before any code
/// jails wrote runs.
///
/// Three things have to move together for the permissive form to be honest:
/// the header becomes optional, the version arrives boxed so `null` can mean
/// "no precondition", and the SQL guard becomes conditional.
#[test]
fn an_optional_precondition_reaches_the_sql_the_port_and_the_route() {
    let root = temp_dir("transition-optional-if-match");
    write_spring_fixture(&root);

    for args in [
        vec!["add", "db", "--no-start"],
        // The route is served by the `api` capability, which is what emits a
        // Spring controller for an operation.
        vec!["add", "api", "--no-start"],
        vec![
            "g",
            "scaffold",
            "Note",
            "id:long@pk",
            "body:string!",
            "seen:boolean",
            "version:long",
        ],
        vec![
            "g",
            "transition",
            "MarkSeen",
            "id:long",
            "version:long",
            "--on",
            "Note",
            "--set",
            "seen=true",
            "--if-match",
            "optional",
            "--consumes",
            "form",
            "--path",
            "/customer_api/seen/{id}",
        ],
    ] {
        assert!(
            jails_cmd(&root, None)
                .args(&args)
                .status()
                .unwrap()
                .success(),
            "{args:?}"
        );
    }

    let adapter = common::read_generated(
        &root,
        "src/main/java/com/example/demo/adapters/JdbcMarkSeenTransition.java",
    );
    // One statement, not two: with a version it reads `version = 5` and
    // guards, without one it reads `version = version` and does not. It is
    // also what gives the null parameter a type -- an untyped null compared
    // with `=` leaves PostgreSQL unable to infer one.
    assert!(
        adapter.contains("version = coalesce(:expected_version, version)"),
        "{adapter}"
    );
    // The pinned component is in the statement, and it is one value for every
    // call -- so it is written where it cannot be overridden rather than bound
    // from somewhere a request could reach.
    assert!(adapter.contains("set seen = true"), "{adapter}");
    assert!(!adapter.contains(".param(\"seen\""), "{adapter}");

    // Boxed, because `null` is a value the port has to be able to hold -- and
    // outside `Input`, because the version travels as `If-Match`.
    let port = common::read_generated(
        &root,
        "src/main/java/com/example/demo/service/MarkSeenUseCase.java",
    );
    assert!(
        port.contains("Note execute(long id, Input input, Long expectedVersion);"),
        "{port}"
    );

    // And the request carries neither the pinned flag nor the version.
    assert!(!port.contains("boolean seen"), "{port}");
    assert!(!port.contains("long version"), "{port}");

    let controller = common::read_generated(
        &root,
        "src/main/java/com/example/demo/web/MarkSeenController.java",
    );
    assert!(
        controller.contains("@RequestHeader(value = HttpHeaders.IF_MATCH, required = false)"),
        "{controller}"
    );

    // The proof moves with it. Without this test the unconditional branch is
    // generated, compiles, and is executed by nothing -- removing `coalesce`
    // would change no test.
    let integration = common::read_generated(
        &root,
        "src/test/java/com/example/demo/adapters/JdbcMarkSeenTransitionIT.java",
    );
    assert!(
        integration.contains("aCallerThatSendsNoPreconditionAppliesUnconditionallyAndCanRepeat"),
        "{integration}"
    );
    assert!(
        integration
            .contains("operation.execute(stored.id(), new MarkSeenTransition.Input(), null)"),
        "{integration}"
    );

    // The controller test names the answer rather than a status it might
    // reach for another reason: a form-bound transition sent a JSON body is
    // answered 400 too.
    let proof = common::read_generated(
        &root,
        "src/test/java/com/example/demo/web/MarkSeenControllerTest.java",
    );
    assert!(
        proof.contains("aRequestWithNoIfMatchIsAppliedUnconditionally"),
        "{proof}"
    );
    // The row is addressed through the URL, and the version through the
    // header the strict branch needs -- so both branches are driven and
    // neither request carries a body.
    assert!(
        proof.contains(".uri(\"/customer_api/seen/{id}\", \"1\")"),
        "{proof}"
    );
    assert!(
        proof.contains(".header(HttpHeaders.IF_MATCH, \"\\\"1\\\"\")"),
        "{proof}"
    );
    assert!(!proof.contains("APPLICATION_JSON"), "{proof}");
}

/// The strict default is unchanged, and its proof still says so.
#[test]
fn a_transition_insists_on_the_precondition_unless_it_was_asked_not_to() {
    let root = temp_dir("transition-required-if-match");
    write_spring_fixture(&root);

    for args in [
        vec!["add", "db", "--no-start"],
        vec![
            "g",
            "scaffold",
            "Note",
            "id:long@pk",
            "body:string!",
            "version:long",
        ],
        vec![
            "g",
            "transition",
            "Rename",
            "id:long",
            "body:string!",
            "version:long",
            "--on",
            "Note",
        ],
    ] {
        assert!(
            jails_cmd(&root, None)
                .args(&args)
                .status()
                .unwrap()
                .success(),
            "{args:?}"
        );
    }

    let adapter = common::read_generated(
        &root,
        "src/main/java/com/example/demo/adapters/JdbcRenameTransition.java",
    );
    assert!(adapter.contains("version = :expected_version"), "{adapter}");
    assert!(!adapter.contains("coalesce"), "{adapter}");

    // The precondition is required and it is not part of the body: the caller
    // states the version they believe they are replacing, `where version =
    // :expected_version` makes a stale one a no-op rather than a blind
    // overwrite, and the value travels as `If-Match` -- which every cache,
    // proxy and client library already understands.
    let port = common::read_generated(
        &root,
        "src/main/java/com/example/demo/application/transitions/RenameTransition.java",
    );
    assert!(!port.contains("long version"), "{port}");
    assert!(
        port.contains("Note execute(long id, Input input, long expectedVersion);"),
        "{port}"
    );

    // And the flag is refused where there is no version to check against,
    // rather than accepted and ignored.
    let output = jails_cmd(&root, None)
        .args([
            "g",
            "usecase",
            "Probe",
            "body:string!",
            "--on",
            "Note",
            "--if-match",
            "optional",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("`--if-match` applies to a transition"),
        "{stderr}"
    );

    // A pin on the component that *finds* the row is refused too: it is not
    // something this transition changes.
    let output = jails_cmd(&root, None)
        .args([
            "g",
            "transition",
            "Probe",
            "id:long",
            "body:string!",
            "version:long",
            "--on",
            "Note",
            "--set",
            "id=7",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    // The refusal names the field and the two roles it cannot hold at once,
    // in the reader's spelling rather than the linker's stable id.
    assert!(stderr.contains("`id` identifies the row"), "{stderr}");
    assert!(stderr.contains("fix:"), "{stderr}");
}

/// A form-bound endpoint's own generated test passes.
///
/// Spring's data binder reads request *parameters*, so a proof posting a JSON
/// body at an `@ModelAttribute` parameter has every component arrive null and
/// is answered 400 -- and on a transition the second generated test asserts
/// 400, so it would pass for exactly the wrong reason. Only a real build sees
/// it; the byte goldens do not.
///
/// The assertion that matters is the shared toolbox's own `mvn test`, which
/// this fixture runs. What is checked here is that the two files say what a
/// form post is, so a regression is localised rather than reported as "the
/// toolbox failed".
#[test]
fn a_form_bound_endpoint_is_proved_by_a_form_post() {
    if !real_mvn_available() {
        skip("mvn not found on PATH");
        return;
    }
    let path = real_path_without_mvnd();
    let root = verified_spring_db_toolbox(&path);

    for (file, sample) in [
        (
            "PostNoteControllerTest.java",
            ".param(\"body\", \"sample\")",
        ),
        // The transition addresses its row through the URL, so what the form
        // carries is the rest of the request -- and the key is expanded into
        // the template rather than posted beside it.
        (
            "MarkNoteSeenControllerTest.java",
            ".uri(\"/actions/mark-note-seen/{id}\", \"1\")",
        ),
    ] {
        let proof =
            common::read_generated(root, &format!("src/test/java/com/example/demo/web/{file}"));
        assert!(proof.contains(sample), "{file}: {proof}");
        // A JSON body at a `@ModelAttribute` parameter binds nothing.
        assert!(!proof.contains("APPLICATION_JSON"), "{file}: {proof}");
    }

    // And the transition's second test names the answer rather than a status
    // it could reach for another reason.
    let proof = common::read_generated(
        root,
        "src/test/java/com/example/demo/web/MarkNoteSeenControllerTest.java",
    );
    assert!(
        proof.contains("aRequestWithNoIfMatchIsAppliedUnconditionally"),
        "{proof}"
    );
}

/// A write that resolves its foreign key from a component of the parent.
///
/// The customer sends the email they logged in with and the row needs a
/// `user_id`. `g query --via` reads across that reference and `g command
/// --via` writes across it; the alternative is an endpoint that trusts the
/// caller for a key that is not theirs to choose -- the same class of defect
/// `--set` closes on the other side of the same request.
#[test]
fn a_use_case_can_resolve_its_key_from_the_parent_the_caller_names() {
    let root = temp_dir("usecase-resolved-key");
    write_spring_fixture(&root);

    for args in [
        vec!["add", "db", "--no-start"],
        vec!["g", "enum", "SenderType", "CUSTOMER", "ADMIN"],
        vec![
            "g",
            "scaffold",
            "Author",
            "id:long@pk",
            "email:string!@unique",
        ],
        vec![
            "g",
            "scaffold",
            "Note",
            "id:long@pk",
            "authorId:long@index",
            "body:string!",
            "senderType:SenderType",
        ],
        // The route is served by the `api` capability, which is what emits a
        // Spring controller for an operation.
        vec!["add", "api", "--no-start"],
        vec![
            "g",
            "usecase",
            "PostNote",
            "email:string!",
            "body:string!",
            "--on",
            "Note",
            "--via",
            "Author",
            "--set",
            "senderType=CUSTOMER",
        ],
    ] {
        assert!(
            jails_cmd(&root, None)
                .args(&args)
                .status()
                .unwrap()
                .success(),
            "{args:?}"
        );
    }

    let adapter = common::read_generated(
        &root,
        "src/main/java/com/example/demo/adapters/ResolvingPostNoteUseCase.java",
    );
    // One statement: the key is *selected* from the parent's row rather than
    // read first and inserted second, so there is no window where the parent
    // is deleted between the two and no way to name a key you do not own.
    assert!(
        adapter.contains("insert into notes (author_id, body, sender_type)"),
        "{adapter}"
    );
    assert!(
        adapter.contains("select authors.id, :body, 'CUSTOMER'"),
        "{adapter}"
    );
    assert!(
        adapter.contains("where authors.email = :resolve_author_id_0"),
        "{adapter}"
    );
    // The pinned component is a constant of the statement, so an enum is
    // stored by the name the column already holds and nothing binds it.
    assert!(!adapter.contains(".param(\"sender_type\""), "{adapter}");
    // And there is no binding for the reference: it never was a parameter.
    assert!(!adapter.contains(".param(\"author_id\""), "{adapter}");

    // "No such parent" is an expected outcome, so it is a return value.
    let port = common::read_generated(
        &root,
        "src/main/java/com/example/demo/service/PostNoteUseCase.java",
    );
    assert!(
        port.contains("Optional<Note> execute(Input input);"),
        "{port}"
    );
    let controller = common::read_generated(
        &root,
        "src/main/java/com/example/demo/web/PostNoteController.java",
    );
    assert!(
        controller.contains(".orElseGet(() -> ResponseEntity.notFound().build())"),
        "{controller}"
    );

    // The proof is the only thing that observes the empty result.
    let integration = common::read_generated(
        &root,
        "src/test/java/com/example/demo/adapters/ResolvingPostNoteUseCaseIT.java",
    );
    assert!(
        integration.contains("answersEmptyWhenNoParentMatches"),
        "{integration}"
    );
    assert!(integration.contains(")).isEmpty();"), "{integration}");
    assert!(
        integration.contains(".authorId()).isEqualTo(authorRow.id())"),
        "{integration}"
    );
}

/// Every way `--via` can be ambiguous is refused, naming the alternatives.
#[test]
fn a_lookup_that_cannot_be_resolved_is_refused_and_names_what_would_work() {
    let root = temp_dir("usecase-via-refusals");
    write_spring_fixture(&root);

    for args in [
        vec!["add", "db", "--no-start"],
        vec![
            "g",
            "scaffold",
            "Author",
            "id:long@pk",
            "email:string!@unique",
            "handle:string!",
        ],
        vec![
            "g",
            "scaffold",
            "Note",
            "id:long@pk",
            "authorId:long@index",
            "body:string!",
        ],
    ] {
        assert!(
            jails_cmd(&root, None)
                .args(&args)
                .status()
                .unwrap()
                .success(),
            "{args:?}"
        );
    }

    for (fields, expected) in [
        // No field names a parent component: the refusal lists the ones that
        // could have.
        (
            vec!["body:string!"],
            "none of its fields names a component of Author",
        ),
        // Two do: one identifies the parent, and picking is not jails' to do.
        (
            vec!["email:string!", "handle:string!", "body:string!"],
            "names 2 components of Author",
        ),
    ] {
        let mut args = vec!["g", "usecase", "Probe"];
        args.extend(fields.iter().copied());
        args.extend(["--on", "Note", "--via", "Author"]);
        let output = jails_cmd(&root, None).args(&args).output().unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success(), "{fields:?} was accepted");
        assert!(stderr.contains(expected), "{fields:?}: {stderr}");
        assert!(stderr.contains("fix:"), "{fields:?}: {stderr}");
    }

    // And a recipe with no reference to cross refuses the flag.
    let output = jails_cmd(&root, None)
        .args(["g", "record", "Probe", "a:string", "--via", "Author"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("`--via` applies to a query or a use case"),
        "{stderr}"
    );
}

/// The same value under two names on two wires.
///
/// Spring's data binder has no naming strategy. Jackson has one and applies it
/// to JSON without help, which is why `@BindParam` is derived from the
/// project's Jackson setting at all -- and that derivation covers
/// `userId` -> `user_id` and cannot cover `id` -> `message_id`, because
/// neither name follows from the other. The brief's own customer page reads
/// `message.id` out of the response and posts `message_id` back.
#[test]
fn a_component_can_be_bound_from_a_parameter_of_another_name() {
    let root = temp_dir("bound-parameter-name");
    write_spring_fixture(&root);

    for args in [
        vec!["add", "db", "--no-start"],
        vec![
            "g",
            "scaffold",
            "Note",
            "id:long@pk",
            "body:string!",
            "seen:boolean",
            "version:long",
        ],
        vec!["add", "api", "--no-start"],
        vec![
            "g",
            "transition",
            "MarkSeen",
            "id:long",
            "body:string!",
            "version:long",
            "--on",
            "Note",
            "--set",
            "seen=true",
            "--if-match",
            "optional",
            "--consumes",
            "form",
            "--bind",
            "body=note_body",
        ],
    ] {
        assert!(
            jails_cmd(&root, None)
                .args(&args)
                .status()
                .unwrap()
                .success(),
            "{args:?}"
        );
    }

    // **A component of the request, not the row's key.** The key is a path
    // variable and the version is a header, so the two values that are not the
    // caller's to name are the two `--bind` has nothing to say about.
    let command = common::read_generated(
        &root,
        "src/main/java/com/example/demo/service/MarkSeenUseCase.java",
    );
    assert!(
        command.contains(r#"@BindParam("note_body") String body"#),
        "{command}"
    );
    assert!(
        command.contains("import org.springframework.web.bind.annotation.BindParam;"),
        "{command}"
    );

    // The proof posts what the record binds. They are one fact, and a proof
    // posting the other name passes or fails for the wrong reason.
    let proof = common::read_generated(
        &root,
        "src/test/java/com/example/demo/web/MarkSeenControllerTest.java",
    );
    assert!(proof.contains(r#".param("note_body", "#), "{proof}");
    assert!(!proof.contains(r#".param("body""#), "{proof}");

    // A binding is an instruction to the data binder, and the data binder only
    // reads a form. On JSON it would be silently ignored.
    let output = jails_cmd(&root, None)
        .args([
            "g",
            "transition",
            "Probe",
            "id:long",
            "body:string!",
            "version:long",
            "--on",
            "Note",
            "--bind",
            "id=note_id",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("this endpoint reads a JSON body"),
        "{stderr}"
    );

    // And a recipe that binds no request refuses it.
    let output = jails_cmd(&root, None)
        .args(["g", "record", "Probe", "a:string", "--bind", "a=b"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("`--bind` applies to a controller"),
        "{stderr}"
    );
}

/// The kinds with no real-toolchain fixture of their own, compiled in one
/// project.
///
/// The golden suite checks **bytes**, not compilability, so a kind no real
/// compiler sees could emit Java that does not compile with every test green.
/// `no_new_generator_kind_escapes_the_real_toolchain` is the ratchet that
/// names such kinds; this is what keeps it empty.
///
/// One project, one `mvn test`: what needs proving is that each kind's output
/// compiles -- not that it does so in isolation. Their prerequisites are
/// taken from the scenario table, which already records the smallest
/// invocation that exercises each.
#[test]
fn every_remaining_generator_kind_compiles_in_one_spring_project() {
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
    let root = temp_dir("real-remaining-kinds");
    write_spring_fixture(&root);

    for step in [
        &["add", "db", "--no-start"][..],
        // The use case below yields an event, and a durable payload needs it.
        &["add", "json"][..],
        // The event's listener, error handler and dead-letter routing belong
        // to the `kafka` capability, so it is asked for explicitly to compile
        // the whole slice.
        &["add", "kafka", "--no-start"][..],
        // Records first: the association, use case and durable job below all
        // name them.
        &[
            "g",
            "scaffold",
            "Owner",
            "id:uuid@pk",
            "name:string!",
            "createdAt:instant@default(now())",
        ][..],
        &[
            "g",
            "scaffold",
            "Item",
            "id:uuid@pk",
            "ownerId:uuid@index",
            "name:string!",
            "createdAt:instant@default(now())",
        ][..],
        &[
            "g",
            "scaffold",
            "Message",
            "id:uuid@pk",
            "body:string!",
            "createdAt:instant@default(now())",
        ][..],
        // `repo` on a record of its own: the scaffolds above emit a repository
        // too, but through `g scaffold`, so the standalone kind's output is
        // what nothing else here compiles.
        &["g", "record", "Ledger", "id:uuid@pk", "note:string!"][..],
        &["g", "repo", "Ledger"][..],
        // **And once on an integral key**, which is the shape `@pk` takes
        // whenever the database assigns it. It is the only key whose Java
        // spelling differs by position -- `long` as a parameter, `Long` as a
        // type argument -- so the in-memory adapter's `Map` is a file that
        // compiles for every other key and not for this one. Nothing but real
        // `javac` catches it: the goldens compare bytes.
        &["g", "record", "Tally", "id:long@pk", "note:string!"][..],
        &["g", "repo", "Tally"][..],
        &[
            "g",
            "association",
            "ItemOwner",
            "ownerId=id",
            "--on",
            "Item",
            "--yields",
            "Owner",
        ][..],
        &[
            "g",
            "usecase",
            "AddItem",
            "id:uuid",
            "ownerId:uuid",
            "name:string!",
            "--on",
            "Item",
        ][..],
        &[
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
        ][..],
        &[
            "g",
            "event",
            "MessageReceived",
            "id:uuid",
            "messageId:uuid",
            "occurredAt:instant",
            "--on",
            "Message",
        ][..],
        &[
            "g",
            "usecase",
            "ReceiveMessage",
            "id:uuid",
            "body:string!",
            "--on",
            "Message",
            "--yields",
            "MessageReceived",
        ][..],
        &[
            "g",
            "http-sink",
            "Provider",
            "--on",
            "ReceiveMessage",
            "--yields",
            "MessageReceived",
        ][..],
        &["g", "fetcher", "Page"][..],
        &["g", "http-workflow", "SiteWalk", "--on", "Page"][..],
        &["g", "cli", "Admin"][..],
        &["g", "handler", "WorkItem"][..],
        &["g", "interface", "Clock"][..],
        &["g", "migration", "add_note_index"][..],
        &["g", "presence", "Room"][..],
        //  reads the record it seeds, so the record has to exist.
        &["g", "scaffold", "Widget", "id:uuid@pk", "name:string!"][..],
        &["g", "seed", "Widget"][..],
        &["g", "test", "Parser"][..],
    ] {
        let output = jails_cmd_with_path(&root, &path)
            .args(step)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "`jails {}` failed: {}",
            step.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // Captured rather than inherited: "failed `mvn test`" with no compiler
    // output tells the next reader nothing, and this test exists precisely to
    // catch generated Java that does not compile.
    let output = real_maven_cmd(&root, &path)
        .args(["-B", "test"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "the project holding every remaining generator kind failed `mvn test`:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
