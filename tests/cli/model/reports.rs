//! What the read-only commands say about a modelled project: `doctor`,
//! `resource status`, `routes` and `beans`.
//!
use super::*;

/// `jails resource status` answers about an entity the model describes, from
/// every authority it names. This drives all four -- declaration, generated,
/// migration history, and the SQL table -- and the two states that are not
/// `consistent`.
#[test]
fn resource_status_answers_from_the_model_when_the_project_is_canonical() {
    let root = jdl_project(
        "jdl-v1-resource-status",
        r#"jdl 1
app Shop {
 pkg com.example.shop
 java 26
 platform spring
 build maven
 storage postgres
}
entity Order {
 id: uuid @pk
 total: long
 use repo
}
entity Memo {
 title: string
}
"#,
    );
    write_spring_fixture(&root);

    // Declared and not yet accepted: an ordinary state, not a fault.
    let pending = jails_cmd(&root, None)
        .args(["resource", "status", "Order"])
        .output()
        .unwrap();
    let pending = String::from_utf8_lossy(&pending.stdout).to_string();
    assert!(
        pending.contains("state: pending") && pending.contains("next: jails sync"),
        "an entity the lock has not accepted should say so and name the fix:\n{pending}"
    );

    let synced = jails_cmd(&root, None).arg("sync").output().unwrap();
    assert!(
        synced.status.success(),
        "{}",
        String::from_utf8_lossy(&synced.stderr)
    );

    let stored = jails_cmd(&root, None)
        .args(["resource", "status", "Order"])
        .output()
        .unwrap();
    let stored = String::from_utf8_lossy(&stored.stdout).to_string();
    for expected in [
        "resource: Order",
        "state: consistent",
        "declaration: present",
        "generated: present",
        "migration-history: present",
        "table: orders",
    ] {
        assert!(
            stored.contains(expected),
            "expected `{expected}` in:\n{stored}"
        );
    }
    assert!(
        stored.contains("Order.java"),
        "the entity's own generated file is attributed to it:\n{stored}"
    );

    // A source-only entity has no table, so the migration authority was never
    // consulted -- `unknown`, which widens, rather than `absent`, which would
    // claim a missing migration nobody asked for.
    let source_only = jails_cmd(&root, None)
        .args(["resource", "status", "Memo"])
        .output()
        .unwrap();
    let source_only = String::from_utf8_lossy(&source_only.stdout).to_string();
    assert!(
        source_only.contains("migration-history: unknown") && !source_only.contains("table:"),
        "a source-only entity reports no table and an unconsulted migration authority:\n{source_only}"
    );

    // Editing a managed file is drift against the accepted image, and is
    // reported as such rather than being re-rendered away.
    let generated = root.join("src/main/java/com/example/shop/domain/Order.java");
    let edited = format!(
        "{}\n// touched by the reader\n",
        fs::read_to_string(&generated).unwrap()
    );
    fs::write(&generated, edited).unwrap();
    let drifted = jails_cmd(&root, None)
        .args(["resource", "status", "Order"])
        .output()
        .unwrap();
    let drifted = String::from_utf8_lossy(&drifted.stdout).to_string();
    assert!(
        drifted.contains("state: drifted") && drifted.contains("finding: generated-drift"),
        "an edited managed file is drift against the accepted image:\n{drifted}"
    );

    // `--json` is the same report, and names its schema.
    let json = jails_cmd(&root, None)
        .args(["--output", "json", "resource", "status", "Order"])
        .output()
        .unwrap();
    let json: serde_json::Value =
        serde_json::from_slice(&json.stdout).expect("resource status --json is JSON");
    assert_eq!(json["schema"], "jails.resource-status.v1");
    assert_eq!(json["table"], "orders");
    assert_eq!(json["state"], "drifted");

    let unknown = jails_cmd(&root, None)
        .args(["resource", "status", "Nobody"])
        .output()
        .unwrap();
    let unknown = String::from_utf8_lossy(&unknown.stdout).to_string();
    assert!(
        unknown.contains("declaration: absent") && unknown.contains("resource-not-declared"),
        "a selector naming nothing says so rather than reporting an empty resource:\n{unknown}"
    );
}

/// `jails doctor` reports a generated tree that has been edited and one that
/// is partly deleted.
///
/// Both rows are worded from what the binary does: `sync` refuses while an
/// accepted file is missing, and merges an edited one forward without writing
/// anything.
#[test]
fn doctor_reports_the_generated_tree_of_a_canonical_project() {
    let root = jdl_project(
        "jdl-v1-doctor-managed",
        r#"jdl 1
app Clinic {
 pkg com.example.clinic
 java 26
 platform spring
 build maven
 storage postgres
}
entity Visit {
 id: uuid @pk
 reason: string
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

    let clean = jails_cmd(&root, None).arg("doctor").output().unwrap();
    let clean = String::from_utf8_lossy(&clean.stdout).to_string();
    assert!(
        clean.contains("every file the lock accepted is on disk")
            && clean.contains("no generated file has been changed"),
        "a freshly synced project reports both managed rows clean:\n{clean}"
    );

    // The `jails.toml` capability row says where a modelled project's
    // capabilities live, and the accepted-model row is what reconciles them.
    let capability = jails_cmd(&root, None)
        .args(["add", "json"])
        .output()
        .unwrap();
    assert!(
        capability.status.success(),
        "{}",
        String::from_utf8_lossy(&capability.stderr)
    );
    let recorded = jails_cmd(&root, None).arg("doctor").output().unwrap();
    let recorded = String::from_utf8_lossy(&recorded.stdout).to_string();
    assert!(
        recorded.contains("declared in the model"),
        "the capability row must not claim a model-declared capability is absent:\n{recorded}"
    );
    assert!(
        !recorded.contains("jails.toml records none"),
        "and must not report the legacy manifest as the authority:\n{recorded}"
    );
    assert!(
        recorded.contains("the lock has accepted everything the model declares"),
        "a synced capability is accepted, entities and capabilities alike:\n{recorded}"
    );

    // Every column the record carries exists in the migrations. `doctor`
    // answers "are these the bytes jails wrote"; this is the half that answers
    // "is this project coherent".
    assert!(
        clean.contains("`visits` has every column the record carries"),
        "the stored entity's lineage should be checked:\n{clean}"
    );

    let record = root.join("src/main/java/com/example/clinic/domain/Visit.java");
    fs::write(
        &record,
        format!(
            "{}\n// a reader's note\n",
            fs::read_to_string(&record).unwrap()
        ),
    )
    .unwrap();
    let edited = jails_cmd(&root, None).arg("doctor").output().unwrap();
    let edited = String::from_utf8_lossy(&edited.stdout).to_string();
    assert!(
        edited.contains("changed since generation")
            && edited.contains("merges the edit forward on every sync"),
        "an edited managed file is a warning that says what happens next:\n{edited}"
    );
    // A record is managed ABI: `jails model eject` refuses for it, so the row
    // must not offer ejection as the fix.
    assert!(
        !edited.contains("jails model eject art_ent_visit_record"),
        "an artifact that cannot be ejected must not be offered as one:\n{edited}"
    );

    let test = root.join("src/test/java/com/example/clinic/domain/VisitTest.java");
    fs::remove_file(&test).unwrap();
    let deleted = jails_cmd(&root, None).arg("doctor").output().unwrap();
    let deleted_out = String::from_utf8_lossy(&deleted.stdout).to_string();
    assert!(
        deleted_out.contains("deleted") && deleted_out.contains("refuses while it is gone"),
        "a deleted managed file is a failure, because sync cannot converge past it:\n{deleted_out}"
    );
    assert!(
        !deleted.status.success(),
        "a FAIL row must leave doctor with a non-zero status"
    );

    // And the refusal the row describes is the one `sync` actually gives.
    let refused = jails_cmd(&root, None).arg("sync").output().unwrap();
    assert!(
        !refused.status.success(),
        "sync should refuse while an accepted file is missing:\n{}",
        String::from_utf8_lossy(&refused.stdout)
    );
}

/// `jails routes` and `jails beans` read the managed tree as well as
/// `src/main/java`: a route jails emitted and cannot see is worse than a gap,
/// because the reader cannot tell an unlisted route from an absent one.
#[test]
fn routes_and_beans_see_the_tree_a_canonical_project_generates_into() {
    let root = jdl_project(
        "jdl-v1-inspect-generated",
        r#"jdl 1
app Depot {
 pkg com.example.depot
 java 26
 platform spring
 build maven
 storage postgres
}
"#,
    );
    write_spring_fixture(&root);
    let scaffolded = jails_cmd(&root, None)
        .args(["g", "scaffold", "Crate", "id:uuid@pk", "label:string"])
        .output()
        .unwrap();
    assert!(
        scaffolded.status.success(),
        "{}",
        String::from_utf8_lossy(&scaffolded.stderr)
    );

    // The scaffold emits the port; a *route* is a routed semantic operation's,
    // and the Spring adapter carrying the mapping annotation is the `api`
    // capability's. Both are needed before there is anything to list.
    let query = jails_cmd(&root, None)
        .args([
            "g",
            "usecase",
            "AddCrate",
            "label",
            "--on",
            "Crate",
            "--path",
            "/depot/crates",
        ])
        .output()
        .unwrap();
    assert!(
        query.status.success(),
        "{}",
        String::from_utf8_lossy(&query.stderr)
    );
    let api = jails_cmd(&root, None)
        .args(["add", "api"])
        .output()
        .unwrap();
    assert!(
        api.status.success(),
        "{}",
        String::from_utf8_lossy(&api.stderr)
    );

    let routes = jails_cmd(&root, None).arg("routes").output().unwrap();
    let routes = String::from_utf8_lossy(&routes.stdout).to_string();
    assert!(
        routes.contains("/depot/crates"),
        "the operation's own route should be listed:\n{routes}"
    );
    assert!(
        routes.contains("AddCrateController"),
        "named by the controller the compiler wrote into `src/main/java`:\n{routes}"
    );

    let beans = jails_cmd(&root, None).arg("beans").output().unwrap();
    let beans = String::from_utf8_lossy(&beans.stdout).to_string();
    assert!(
        beans.contains("CrateController") || beans.contains("CrateService"),
        "the scaffold's beans should be listed:\n{beans}"
    );

    // `jails src` walks all four roots. It is the one command that
    // deliberately works outside a build file, so it is the last place a
    // reader would expect to be told their own generated type does not exist.
    let located = jails_cmd(&root, None)
        .args(["src", "Crate"])
        .output()
        .unwrap();
    let located = String::from_utf8_lossy(&located.stdout).to_string();
    assert!(
        located.contains("src/main/java") && located.contains("Crate.java"),
        "`jails src` should find a type the compiler wrote:\n{located}"
    );

    // `stats` counts the same tree, summed per layer across both roots.
    let stats = jails_cmd(&root, None).arg("stats").output().unwrap();
    let stats = String::from_utf8_lossy(&stats.stdout).to_string();
    assert!(
        !stats.contains("No Java sources under"),
        "a project whose every Java file is generated is not an empty project:\n{stats}"
    );

    // An empty report has to say where it looked, or the reader cannot tell a
    // searched-and-empty tree from an unopened one.
    let bare = jdl_project(
        "jdl-v1-inspect-bare",
        r#"jdl 1
app Bare {
 pkg com.example.bare
 java 26
 platform spring
 build maven
 storage none
}
"#,
    );
    write_spring_fixture(&bare);
    let empty = jails_cmd(&bare, None).arg("routes").output().unwrap();
    let empty = String::from_utf8_lossy(&empty.stdout).to_string();
    assert!(
        empty.contains("No routes found under src/main/java."),
        "a project with no generated tree reports exactly the roots it walked:\n{empty}"
    );
}

/// An incoherence `doctor` reports: the record carries a component the
/// accepted schema does not.
///
/// Every file is byte-identical to what jails wrote, so nothing else has a
/// reason to complain, and only a query at runtime would find it. Both sides
/// are `jails_compiler::storage_columns` -- asked of the declared model and of
/// the same entity in the model the lock accepted -- rather than a reader of
/// the migration text, which was a second description of the same decision.
#[test]
fn doctor_names_a_column_the_record_carries_and_the_accepted_schema_does_not() {
    let model = |components: &str| {
        format!(
            r#"jdl 1
app Ward {{
 pkg com.example.ward
 java 26
 platform spring
 build maven
 storage postgres
}}
entity Bed {{
 id: uuid @pk
 label: string
{components} use repo
}}
"#
        )
    };
    let root = jdl_project("jdl-v1-doctor-lineage", &model(""));
    write_spring_fixture(&root);
    let synced = jails_cmd(&root, None).arg("sync").output().unwrap();
    assert!(
        synced.status.success(),
        "{}",
        String::from_utf8_lossy(&synced.stderr)
    );

    let accepted = jails_cmd(&root, None).arg("doctor").output().unwrap();
    let accepted = String::from_utf8_lossy(&accepted.stdout).to_string();
    assert!(
        accepted
            .lines()
            .any(|line| line.starts_with("ok") && line.contains("schema Bed")),
        "a synced project is coherent:\n{accepted}"
    );

    // A component the lock has not accepted is a column the table does not
    // have, whatever the Java says.
    fs::write(root.join(".jails/model.jdl"), model(" occupant: string\n")).unwrap();
    let torn = jails_cmd(&root, None).arg("doctor").output().unwrap();
    let reported = String::from_utf8_lossy(&torn.stdout).to_string();
    assert!(
        reported.contains("is missing occupant") && reported.contains("which `Bed` carries"),
        "the missing column should be named, with the record that needs it:\n{reported}"
    );
    // A failure, not a note. Asserted on the row rather than on the exit
    // status, which this fixture also fails for an unrelated reason.
    assert!(
        reported
            .lines()
            .any(|line| line.contains("is missing occupant") && line.starts_with("FAIL")),
        "the incoherence is a failure, not a note:\n{reported}"
    );
}

/// A migration whose bytes were changed by hand is the *seal's* question, and
/// it is reported there rather than by re-reading the SQL for a column list.
#[test]
fn doctor_names_a_migration_edited_after_it_was_published() {
    let root = jdl_project(
        "jdl-v1-doctor-seal",
        r#"jdl 1
app Ward {
 pkg com.example.ward
 java 26
 platform spring
 build maven
 storage postgres
}
entity Bed {
 id: uuid @pk
 label: string
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

    let migrations = root.join("src/main/resources/db/migration");
    let migration = fs::read_dir(&migrations)
        .unwrap()
        .flatten()
        .map(|entry| entry.path())
        .find(|path| path.extension().is_some_and(|extension| extension == "sql"))
        .expect("the stored entity's migration");
    let sql = fs::read_to_string(&migration).unwrap();
    assert!(
        sql.contains("label"),
        "the migration declares the column:\n{sql}"
    );

    // Drop the column from the history while the record keeps the component.
    let torn = sql
        .lines()
        .filter(|line| !line.contains("label"))
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(&migration, format!("{torn}\n")).unwrap();

    let edited = jails_cmd(&root, None).arg("doctor").output().unwrap();
    let reported = String::from_utf8_lossy(&edited.stdout).to_string();
    assert!(
        reported.lines().any(|line| line.starts_with("FAIL")
            && line.contains("sealed migrations")
            && line.contains("V001__create_beds.sql")),
        "an edited migration is named by the seal, as a failure:\n{reported}"
    );
}
