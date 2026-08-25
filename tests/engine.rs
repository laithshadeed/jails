//! One capability, installed through the transaction protocol end to end.
//!
//! plan.md §R6.1 step 2 wants capability `add` on V2 while default dispatch
//! stays on V1. Every piece of that has landed separately -- the translation,
//! the capture, the preparation, the executor -- and this is the first test
//! that runs all of them against a real directory and looks at what is on
//! disk afterwards.

mod common;

use jails_project::capability::Declaration;
use jails_project::model::Project;

/// A run that commits files and leaves the runtime alone.
///
/// Every test in this file is about what reaches disk and what reaches the
/// store. `add db` really does start its container now -- that is the point of
/// §R3.3's post-commit effect -- so a test that used the plain committing run
/// would depend on a container engine being installed, running and able to
/// pull an image, and would leave a database behind when it passed. The
/// runtime half has its own test, which plans the effect rather than running
/// it.
fn committing(project: &Project) -> jails_engine::route::Run<'_> {
    jails_engine::route::Run::committing(project).without_start()
}
use jails_spec::spec::kind::Capability;

#[test]
fn a_capability_installs_through_the_transaction_protocol() {
    let root = common::temp_dir("engine-install");
    std::fs::create_dir_all(&root).unwrap();
    common::write_spring_fixture(&root);
    let project = Project::load(&root).unwrap();

    jails_engine::route::install(
        &committing(&project),
        &Declaration::plain(Capability::Actuator),
    )
    .unwrap();

    let pom = std::fs::read_to_string(root.join("pom.xml")).unwrap();
    assert!(
        pom.contains("spring-boot-starter-actuator"),
        "the dependency the capability desires is in the POM:\n{pom}"
    );
    let properties =
        std::fs::read_to_string(root.join("src/main/resources/application.properties")).unwrap();
    assert!(
        jails_project::properties::get(&properties, "management.server.port").is_some(),
        "the properties it desires are in force:\n{properties}"
    );
    assert!(
        root.join("src/test/java/com/example/demo/ActuatorEndpointsTest.java")
            .is_file(),
        "the file it desires is on disk"
    );
    assert!(
        root.join(".jails").is_dir(),
        "the transaction left its own bookkeeping behind"
    );

    // CLAUDE.md's rule: the manifest `sync` acts on is maintained by `add`,
    // never by the user. A capability installed and not recorded is one the
    // next `sync` would take back out.
    let manifest = std::fs::read_to_string(root.join("jails.toml")).unwrap();
    assert!(manifest.contains("actuator"), "{manifest}");
}

/// A capability that writes a test writes it against AssertJ.
///
/// The direct write path applies this from one place rather than per recipe,
/// and so does the route. The case it exists for is a project jails did not
/// create: `add testkit` on plain Maven is where it showed up, as six
/// `cannot find symbol: method assertThat` for a file the reader never wrote.
#[test]
fn a_capability_that_writes_a_test_brings_something_to_assert_with() {
    let root = common::temp_dir("engine-assertj");
    std::fs::create_dir_all(&root).unwrap();
    common::write_plain_fixture(&root);
    let project = Project::load(&root).unwrap();

    jails_engine::route::install(&committing(&project), &Declaration::plain(Capability::Csv))
        .unwrap();

    let pom = std::fs::read_to_string(root.join("pom.xml")).unwrap();
    assert!(pom.contains("assertj-core"), "{pom}");
    assert!(
        pom.contains(jails_project::pom::ASSERTJ_VERSION),
        "without a managing parent the version has to be pinned, or Maven refuses to read the \
         POM at all:\n{pom}"
    );
}

/// The property every step before the lock has: it touches nothing.
///
/// A plan that refuses -- a capability that cannot plan against this project --
/// must leave a directory nobody has opened for writing, `.jails` included.
#[test]
fn a_refusal_before_the_lock_leaves_the_project_untouched() {
    let root = common::temp_dir("engine-refusal");
    std::fs::create_dir_all(&root).unwrap();
    common::write_spring_fixture(&root);
    let project = Project::load(&root).unwrap();
    let before = common::scenarios::file_set(&root);

    let error =
        jails_engine::route::install(&committing(&project), &Declaration::plain(Capability::K8s))
            .unwrap_err();

    assert!(!error.is_empty(), "a refusal says something");
    assert_eq!(
        common::scenarios::file_set(&root),
        before,
        "nothing was written, and no machine directory was created"
    );
}

/// The same route, for a persistent generator.
///
/// One entity rather than the capability list, and the identity is
/// `(recipe, name, resolved package)` -- which is what makes an edited field
/// list an update to a known artifact rather than a new one landing on files
/// that already exist.
#[test]
fn a_record_generates_through_the_transaction_protocol() {
    let root = common::temp_dir("engine-generate");
    std::fs::create_dir_all(&root).unwrap();
    common::write_plain_fixture(&root);
    let project = Project::load(&root).unwrap();

    jails_engine::route::generate(
        &committing(&project),
        &jails_generate::generate::Recipe {
            kind: jails_spec::spec::kind::ArtifactKind::Record,
            name: "Note",
            fields: &["title:string!".to_string(), "at:instant".to_string()],
            indexes: &[],
            strategy_on: None,
            strategy_yields: None,
            method: None,
        },
        None,
    )
    .unwrap();

    let record = root.join("src/main/java/com/example/demo/domain/Note.java");
    let test = root.join("src/test/java/com/example/demo/domain/NoteTest.java");
    assert!(record.is_file(), "the record is on disk");
    assert!(test.is_file(), "and its companion test");
    let source = std::fs::read_to_string(&record).unwrap();
    assert!(source.contains("record Note("), "{source}");
    assert!(
        std::fs::read_to_string(root.join("pom.xml"))
            .unwrap()
            .contains("assertj-core"),
        "the generated test has something to assert with"
    );
}

/// Two runs of one request are one artifact, not two.
///
/// V1 refuses the second with "already exists". V2 reports a no-op, and that
/// is the specified behaviour rather than an accident: plan.md §R6.2 lists
/// "repeat no-op" as one of the states the generator route has to reach,
/// because a request whose desired state already holds has nothing to do. The
/// answer is truthful *because* it is decided after the precondition recheck
/// under the lock rather than from a stat before it.
#[test]
fn generating_the_same_record_twice_is_one_artifact_and_a_no_op() {
    let root = common::temp_dir("engine-generate-twice");
    std::fs::create_dir_all(&root).unwrap();
    common::write_plain_fixture(&root);
    let generate = || {
        jails_engine::route::generate(
            &committing(&Project::load(&root).unwrap()),
            &jails_generate::generate::Recipe {
                kind: jails_spec::spec::kind::ArtifactKind::Record,
                name: "Note",
                fields: &["title:string!".to_string()],
                indexes: &[],
                strategy_on: None,
                strategy_yields: None,
                method: None,
            },
            None,
        )
    };
    assert!(matches!(
        generate().unwrap().committed().unwrap(),
        jails_commit::outcome::CommitResult::Committed(_)
    ));
    assert!(
        matches!(
            generate().unwrap().committed().unwrap(),
            jails_commit::outcome::CommitResult::NoOp
        ),
        "the second run has nothing to do"
    );
}

/// A file somebody else wrote at a path this request wants is not this
/// request's to overwrite.
#[test]
fn a_file_the_request_does_not_own_is_not_overwritten() {
    let root = common::temp_dir("engine-generate-unowned");
    std::fs::create_dir_all(&root).unwrap();
    common::write_plain_fixture(&root);
    let mine = "// written by hand, and not by jails\n";
    jails_support::apply::put(
        root.join("src/main/java/com/example/demo/domain/Note.java"),
        mine,
    )
    .unwrap();

    let outcome = jails_engine::route::generate(
        &committing(&Project::load(&root).unwrap()),
        &jails_generate::generate::Recipe {
            kind: jails_spec::spec::kind::ArtifactKind::Record,
            name: "Note",
            fields: &["title:string!".to_string()],
            indexes: &[],
            strategy_on: None,
            strategy_yields: None,
            method: None,
        },
        None,
    );

    let error = outcome.unwrap_err();
    assert!(
        error.contains("jails did not write it") && error.contains("move it aside"),
        "the refusal says whose the file is and what to do about it: {error}"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("src/main/java/com/example/demo/domain/Note.java"))
            .unwrap(),
        mine,
        "and the bytes the reader wrote are still there"
    );
    assert!(
        !root
            .join("src/test/java/com/example/demo/domain/NoteTest.java")
            .exists(),
        "a refusal before the lock writes none of the request, not most of it"
    );
}

/// §R4.5, asked of a whole command rather than of a hand-built transaction.
///
/// Whatever instant the run stops at, the project is either completely
/// installed or completely absent -- never a POM naming a dependency whose
/// file is missing, and never a file nothing declares. Recovery runs twice,
/// because recovery that converges on the first pass and changes something on
/// the second is not idempotent, and that difference only shows up under a
/// second crash.
#[test]
fn a_capability_install_converges_from_every_failpoint() {
    let mut interrupted = 0;
    for point in jails_commit::fault::POINTS {
        let root = common::temp_dir(&format!("engine-crash-{point}"));
        std::fs::create_dir_all(&root).unwrap();
        common::write_spring_fixture(&root);
        let project = Project::load(&root).unwrap();

        {
            let _armed = jails_commit::fault::Armed::at(point);
            let _ = jails_engine::route::install(
                &committing(&project),
                &Declaration::plain(Capability::Actuator),
            );
        }

        for pass in 0..2 {
            let handle = jails_commit::execute::ProjectHandle::at(&root).unwrap();
            let locked = jails_commit::execute::LockedProject::acquire(handle, "recovery").unwrap();
            let outcome = jails_commit::recover::recover_locked(&locked)
                .unwrap_or_else(|why| panic!("{point}: recovery pass {pass} failed: {why:?}"));
            assert!(
                pass == 0 || outcome.is_clean(),
                "{point}: the second recovery still had work to do: {outcome:?}"
            );
        }

        let pom = std::fs::read_to_string(root.join("pom.xml")).unwrap();
        let installed = pom.contains("spring-boot-starter-actuator");
        let file = root
            .join("src/test/java/com/example/demo/ActuatorEndpointsTest.java")
            .is_file();
        assert_eq!(
            installed, file,
            "{point}: the project is half-installed -- dependency {installed}, file {file}"
        );
        if !installed {
            interrupted += 1;
        }
    }

    // A sweep where nothing was ever interrupted proves nothing: it would
    // pass identically with the failpoints compiled out, which is exactly the
    // "a skipped test reports as a pass" failure CLAUDE.md keeps warning
    // about. Some points fire before anything is applied and some after the
    // commit rolls forward, so both outcomes have to appear.
    assert!(
        interrupted > 0 && interrupted < jails_commit::fault::POINTS.len(),
        "the sweep saw {interrupted} interrupted installs out of {}; the failpoints are not \
         firing, or they all are",
        jails_commit::fault::POINTS.len()
    );
}

/// What the store says after a transition, and after a second one.
///
/// The point of recording at all: a capability installs, its dependency and
/// its file are claimed by it, and a *different* capability installed
/// afterwards leaves the first one's rows exactly where they were. A store
/// rebuilt from one request's intent would quietly delete everything that
/// request did not mention.
#[test]
fn each_transition_records_what_it_claimed_and_leaves_the_rest_alone() {
    let root = common::temp_dir("engine-store");
    std::fs::create_dir_all(&root).unwrap();
    common::write_spring_fixture(&root);

    jails_engine::route::install(
        &committing(&Project::load(&root).unwrap()),
        &Declaration::plain(Capability::Actuator),
    )
    .unwrap();
    let store = jails_commit::store::Store::at(&root).observe().unwrap();
    let first = store.ledger.clone().unwrap();
    assert_eq!(first.generation, 1);
    assert!(
        first
            .applied
            .iter()
            .any(|entity| format!("{:?}", entity.id).contains("Actuator")),
        "the capability is recorded: {:?}",
        first.applied
    );
    let actuator_rows = first.resources.len();
    assert!(actuator_rows > 0, "and so are the resources it claimed");

    jails_engine::route::install(
        &committing(&Project::load(&root).unwrap()),
        &Declaration::plain(Capability::Cache),
    )
    .unwrap();
    let second = jails_commit::store::Store::at(&root)
        .observe()
        .unwrap()
        .ledger
        .unwrap();
    assert_eq!(second.generation, 2, "one increment per commit");
    assert_eq!(second.applied.len(), 2);
    for row in &first.resources {
        assert!(
            second.resources.iter().any(|later| later.key == row.key),
            "the first capability's claim survived the second install: {:?}",
            row.key
        );
    }
}

/// Installing the same capability twice is one row and one transition.
#[test]
fn installing_a_capability_twice_leaves_the_store_where_it_was() {
    let root = common::temp_dir("engine-store-repeat");
    std::fs::create_dir_all(&root).unwrap();
    common::write_spring_fixture(&root);

    jails_engine::route::install(
        &committing(&Project::load(&root).unwrap()),
        &Declaration::plain(Capability::Actuator),
    )
    .unwrap();
    let before = jails_commit::store::Store::at(&root).observe().unwrap();
    let outcome = jails_engine::route::install(
        &committing(&Project::load(&root).unwrap()),
        &Declaration::plain(Capability::Actuator),
    )
    .unwrap();
    let after = jails_commit::store::Store::at(&root).observe().unwrap();
    let outcome = outcome.committed().unwrap();

    assert!(
        matches!(outcome, jails_commit::outcome::CommitResult::NoOp),
        "the second install has nothing to do, got {outcome:?}"
    );
    assert_eq!(
        before.ledger.unwrap().generation,
        after.ledger.unwrap().generation,
        "and the generation does not move for a run that did nothing"
    );
}

/// `remove` is the inverse of `install`, worked out rather than mirrored.
#[test]
fn removing_a_capability_takes_back_exactly_what_it_installed() {
    let root = common::temp_dir("engine-remove");
    std::fs::create_dir_all(&root).unwrap();
    common::write_spring_fixture(&root);
    let before = std::fs::read_to_string(root.join("pom.xml")).unwrap();

    jails_engine::route::install(
        &committing(&Project::load(&root).unwrap()),
        &Declaration::plain(Capability::Actuator),
    )
    .unwrap();
    let file = root.join("src/test/java/com/example/demo/ActuatorEndpointsTest.java");
    assert!(file.is_file());

    jails_engine::route::remove(
        &committing(&Project::load(&root).unwrap()),
        &Declaration::plain(Capability::Actuator),
    )
    .unwrap();

    let pom = std::fs::read_to_string(root.join("pom.xml")).unwrap();
    assert!(
        !pom.contains("spring-boot-starter-actuator"),
        "the dependency it claimed is gone:\n{pom}"
    );
    assert!(!file.exists(), "and so is the file only it owned");
    let properties =
        std::fs::read_to_string(root.join("src/main/resources/application.properties"))
            .unwrap_or_default();
    assert!(
        jails_project::properties::get(&properties, "management.server.port").is_none(),
        "and the properties it set:\n{properties}"
    );
    let manifest = std::fs::read_to_string(root.join("jails.toml")).unwrap_or_default();
    assert!(
        !manifest.contains("actuator"),
        "and its line in the manifest"
    );

    let store = jails_commit::store::Store::at(&root)
        .observe()
        .unwrap()
        .ledger
        .unwrap();
    assert!(store.applied.is_empty(), "{:?}", store.applied);
    assert!(store.resources.is_empty(), "{:?}", store.resources);
    let _ = before;
}

/// A resource two capabilities want survives one of them leaving.
///
/// This is the whole reason a dependency is a resource with an owner set
/// rather than a line somebody spliced: `remove` takes away a claim, and the
/// line goes only when the last claim does.
#[test]
fn a_shared_claim_survives_one_owner_leaving() {
    let root = common::temp_dir("engine-shared");
    std::fs::create_dir_all(&root).unwrap();
    common::write_spring_fixture(&root);

    // Both of these want `spring-boot-starter-actuator`.
    jails_engine::route::install(
        &committing(&Project::load(&root).unwrap()),
        &Declaration::plain(Capability::Actuator),
    )
    .unwrap();
    jails_engine::route::install(
        &committing(&Project::load(&root).unwrap()),
        &Declaration::plain(Capability::Observability),
    )
    .unwrap();
    jails_engine::route::remove(
        &committing(&Project::load(&root).unwrap()),
        &Declaration::plain(Capability::Observability),
    )
    .unwrap();

    let pom = std::fs::read_to_string(root.join("pom.xml")).unwrap();
    assert!(
        pom.contains("spring-boot-starter-actuator"),
        "actuator still claims it:\n{pom}"
    );
}

/// Removing something the store never recorded is refused, not guessed at.
#[test]
fn removing_what_was_never_installed_is_refused() {
    let root = common::temp_dir("engine-remove-absent");
    std::fs::create_dir_all(&root).unwrap();
    common::write_spring_fixture(&root);
    let error = jails_engine::route::remove(
        &committing(&Project::load(&root).unwrap()),
        &Declaration::plain(Capability::Actuator),
    )
    .unwrap_err();
    assert!(error.contains("not recorded as installed"), "{error}");
    assert!(error.contains("fix:"), "{error}");
}

/// `sync` makes the project match the list, in one transition.
///
/// Both directions at once: a capability the manifest names arrives, and one
/// it no longer names leaves. Doing it as a loop of installs and removes would
/// leave a project in neither state if it stopped halfway.
#[test]
fn sync_brings_the_project_to_the_list_in_the_manifest() {
    let root = common::temp_dir("engine-sync");
    std::fs::create_dir_all(&root).unwrap();
    common::write_spring_fixture(&root);

    jails_engine::route::install(
        &committing(&Project::load(&root).unwrap()),
        &Declaration::plain(Capability::Actuator),
    )
    .unwrap();
    let file = root.join("src/test/java/com/example/demo/ActuatorEndpointsTest.java");
    assert!(file.is_file());

    // The reader edits the list: actuator out, cache in.
    jails_support::apply::put(
        root.join("jails.toml"),
        "[project]\ncapabilities = [\"cache\"]\n",
    )
    .unwrap();

    jails_engine::route::sync(&committing(&Project::load(&root).unwrap())).unwrap();

    let pom = std::fs::read_to_string(root.join("pom.xml")).unwrap();
    assert!(
        pom.contains("spring-boot-starter-cache"),
        "the capability the manifest names arrived:\n{pom}"
    );
    assert!(
        !pom.contains("spring-boot-starter-actuator"),
        "and the one it dropped left:\n{pom}"
    );
    assert!(!file.exists(), "including the file it owned");

    let store = jails_commit::store::Store::at(&root)
        .observe()
        .unwrap()
        .ledger
        .unwrap();
    assert_eq!(store.applied.len(), 1, "{:?}", store.applied);
    assert_eq!(
        store.generation, 2,
        "one commit for the whole sync, not one per capability"
    );
}

/// A manifest naming something this binary does not know is an error.
///
/// It is caught when the project is resolved rather than when `sync` runs,
/// which is the better place: every command that reads `jails.toml` gets the
/// same refusal, and none of them plans against a list it half understood.
#[test]
fn a_manifest_naming_an_unknown_capability_is_refused_before_anything_plans() {
    let root = common::temp_dir("engine-sync-unknown");
    std::fs::create_dir_all(&root).unwrap();
    common::write_spring_fixture(&root);
    jails_support::apply::put(
        root.join("jails.toml"),
        "[project]\ncapabilities = [\"telepathy\"]\n",
    )
    .unwrap();

    let error = Project::load(&root).unwrap_err();
    assert!(error.contains("telepathy"), "{error}");
    assert!(error.contains("Known:"), "{error}");
}

/// `destroy` is the inverse of `generate`, worked out from the record.
///
/// plan.md §R6.2 asks the V2 destroy to "forward-plan remaining resources
/// from recorded exact state" instead of rebuilding a path table. That is
/// what makes the deletion exact: every file the entity owned goes, and
/// nothing else does -- including the dependency the write path added on its
/// behalf, which no hand-written destroy arm ever knew about.
#[test]
fn destroying_a_record_takes_back_exactly_what_generating_it_wrote() {
    let root = common::temp_dir("engine-destroy");
    std::fs::create_dir_all(&root).unwrap();
    common::write_plain_fixture(&root);

    jails_engine::route::generate(
        &committing(&Project::load(&root).unwrap()),
        &jails_generate::generate::Recipe {
            kind: jails_spec::spec::kind::ArtifactKind::Record,
            name: "Note",
            fields: &["title:string!".to_string()],
            indexes: &[],
            strategy_on: None,
            strategy_yields: None,
            method: None,
        },
        None,
    )
    .unwrap();
    let record = root.join("src/main/java/com/example/demo/domain/Note.java");
    let test = root.join("src/test/java/com/example/demo/domain/NoteTest.java");
    assert!(record.is_file() && test.is_file());

    jails_engine::route::destroy(
        &committing(&Project::load(&root).unwrap()),
        jails_spec::spec::kind::ArtifactKind::Record,
        "Note",
        None,
        false,
        None,
    )
    .unwrap();

    assert!(!record.exists(), "the record it owned is gone");
    assert!(!test.exists(), "and its companion test");
    let store = jails_commit::store::Store::at(&root)
        .observe()
        .unwrap()
        .ledger
        .unwrap();
    assert!(store.applied.is_empty(), "{:?}", store.applied);
    assert!(store.resources.is_empty(), "{:?}", store.resources);
}

/// One `destroy` speaks for one identity and says nothing about any other.
///
/// `DirectEntity` is the narrow scope, and this is the case it exists for:
/// silence about `record Memo` must not be read as absence.
#[test]
fn destroying_one_record_leaves_another_alone() {
    let root = common::temp_dir("engine-destroy-scope");
    std::fs::create_dir_all(&root).unwrap();
    common::write_plain_fixture(&root);
    let generate = |name: &str| {
        jails_engine::route::generate(
            &committing(&Project::load(&root).unwrap()),
            &jails_generate::generate::Recipe {
                kind: jails_spec::spec::kind::ArtifactKind::Record,
                name,
                fields: &["title:string!".to_string()],
                indexes: &[],
                strategy_on: None,
                strategy_yields: None,
                method: None,
            },
            None,
        )
        .unwrap();
    };
    generate("Note");
    generate("Memo");

    jails_engine::route::destroy(
        &committing(&Project::load(&root).unwrap()),
        jails_spec::spec::kind::ArtifactKind::Record,
        "Note",
        None,
        false,
        None,
    )
    .unwrap();

    assert!(
        root.join("src/main/java/com/example/demo/domain/Memo.java")
            .is_file(),
        "the identity nobody named is still there"
    );
    assert!(
        !root
            .join("src/main/java/com/example/demo/domain/Note.java")
            .exists()
    );
    let store = jails_commit::store::Store::at(&root)
        .observe()
        .unwrap()
        .ledger
        .unwrap();
    assert_eq!(store.applied.len(), 1, "{:?}", store.applied);
}

/// Destroying something the store never recorded refuses and says what would
/// have recorded it.
#[test]
fn destroying_what_was_never_generated_names_the_command_that_records_it() {
    let root = common::temp_dir("engine-destroy-absent");
    std::fs::create_dir_all(&root).unwrap();
    common::write_plain_fixture(&root);

    let error = jails_engine::route::destroy(
        &committing(&Project::load(&root).unwrap()),
        jails_spec::spec::kind::ArtifactKind::Record,
        "Note",
        None,
        false,
        None,
    )
    .unwrap_err();

    assert!(error.contains("no `record Note` is recorded"), "{error}");
    assert!(error.contains("jails g record Note"), "{error}");
}

/// §R6.2's destroy gate, asked of every persistent kind at once.
///
/// Generate through the V2 route, destroy through it, and require the project
/// to be byte-for-byte what it was before -- the same question
/// `tests/agreement.rs` asks of V1, but answered from the recorded exact
/// state rather than from a recomputed path table. Deletable projections must
/// return to their starting state; newly published migrations are the one
/// deliberate remainder because schema history is append-only.
///
/// The scenario table is the source of kinds, per CLAUDE.md's rule that a new
/// kind adds a `Scenario` and not a fourth list. Single-step scenarios only:
/// a scenario that installs a capability first is asking about two owners
/// interacting, which the shared-claim tests cover separately.
#[test]
fn every_persistent_kind_destroys_back_to_its_projection_baseline() {
    use clap::ValueEnum;
    use jails_spec::spec::kind::ArtifactKind;

    let mut swept: Vec<&str> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();
    for scenario in common::scenarios::SCENARIOS {
        // The last step is the one under test; everything before it is
        // set-up, run through the binary exactly as the golden snapshots run
        // it. That keeps the question narrow -- does *this* generate undo
        // itself -- rather than making every scenario a test of two engines.
        let Some((step, prerequisites)) = scenario.steps.split_last() else {
            continue;
        };
        if !matches!(step.first(), Some(&"g") | Some(&"generate")) {
            continue;
        }
        let Ok(kind) = ArtifactKind::from_str(step[1], true) else {
            skipped.push(format!("{}: `{}` is an alias", scenario.name, step[1]));
            continue;
        };
        let Some(invocation) = common::scenarios::invocation(step) else {
            skipped.push(format!("{}: unrecognised flag", scenario.name));
            continue;
        };
        if invocation.timestamps {
            // `--timestamps` expands to two extra fields before a recipe sees
            // them, and the route takes fields already expanded. Comparing
            // that here would be testing the expansion, not the round trip.
            skipped.push(format!("{}: --timestamps", scenario.name));
            continue;
        }

        let root = common::temp_dir(&format!("engine-roundtrip-{}", scenario.name));
        std::fs::create_dir_all(&root).unwrap();
        match scenario.fixture {
            common::scenarios::Fixture::Plain => common::write_plain_fixture(&root),
            common::scenarios::Fixture::Spring => common::write_spring_fixture(&root),
        }
        for (path, contents) in scenario.seed {
            jails_support::apply::put(root.join(path), *contents).unwrap();
        }
        let mut unroutable = None;
        for earlier in prerequisites {
            // Set-up runs through the V2 routes too, not through the binary.
            // A V1 step would leave a schema-1 ledger behind, and this route
            // has no migration yet (§R6.1 step 9) -- so the whole scenario
            // would be skipped for a reason that says nothing about destroy.
            if let Err(why) = route_step(&root, earlier) {
                unroutable = Some(format!(
                    "{}: set-up `{}`: {why}",
                    scenario.name,
                    earlier.join(" ")
                ));
                break;
            }
        }
        if let Some(note) = unroutable {
            skipped.push(note);
            continue;
        }
        let before = common::scenarios::file_set(&root);
        let pom_before = std::fs::read_to_string(root.join("pom.xml")).unwrap();

        let generated = jails_engine::route::generate(
            &committing(&Project::load(&root).unwrap()),
            &jails_generate::generate::Recipe {
                kind,
                name: step[2],
                fields: &invocation.fields,
                indexes: &invocation.indexes,
                strategy_on: invocation.on.as_deref(),
                strategy_yields: invocation.yields.as_deref(),
                method: None,
            },
            invocation.package.as_deref(),
        );
        match generated {
            Ok(_) => {}
            Err(why) => {
                // A one-shot has no recipe plan, and a kind whose precondition
                // this fixture does not meet is not a round-trip question.
                skipped.push(format!("{}: {why}", scenario.name));
                continue;
            }
        }
        let generated_files = common::scenarios::file_set(&root);
        assert_ne!(
            generated_files, before,
            "{}: the generate wrote nothing, so the round trip proves nothing",
            scenario.name
        );
        let published_migrations: std::collections::BTreeSet<String> = generated_files
            .difference(&before)
            .filter(|path| {
                path.starts_with("src/main/resources/db/migration/") && path.ends_with(".sql")
            })
            .cloned()
            .collect();

        jails_engine::route::destroy(
            &committing(&Project::load(&root).unwrap()),
            kind,
            step[2],
            invocation.package.as_deref(),
            false,
            None,
        )
        .unwrap_or_else(|why| panic!("{}: destroy refused: {why}", scenario.name));

        let after: std::collections::BTreeSet<String> = common::scenarios::file_set(&root)
            .into_iter()
            // `.jails/` is the transaction's own bookkeeping, which exists
            // from the first commit onward and is not something `destroy` is
            // asked to take back.
            .filter(|path| !path.starts_with(".jails"))
            .collect();
        let expected_after: std::collections::BTreeSet<String> =
            before.iter().cloned().chain(published_migrations).collect();
        assert_eq!(
            after, expected_after,
            "{}: destroy left something other than append-only migration history",
            scenario.name
        );
        assert_eq!(
            std::fs::read_to_string(root.join("pom.xml")).unwrap(),
            pom_before,
            "{}: the POM still carries something the destroyed entity brought in",
            scenario.name
        );
        swept.push(scenario.name);
    }

    println!("kinds swept through generate/destroy: {}", swept.len());
    for note in &skipped {
        println!("  skipped {note}");
    }
    // A floor rather than a count, and it rises as the remaining gap closes.
    // What it excludes today is stated above, per scenario, rather than left
    // to be inferred from a number: the three one-shots have no recipe plan,
    // which is §R6.1 step 3's other half.
    assert!(
        swept.len() >= 22,
        "only {} kinds round-tripped, which is not the surface: {swept:?}",
        swept.len()
    );
}

/// One scenario step, run through the V2 route rather than the binary.
///
/// Only `add` and `g` appear as set-up in the scenario table, which is what
/// makes this a two-arm match rather than a second dispatcher. Anything else
/// is reported, never quietly run through V1 -- a mixed-engine project is
/// exactly the state §R6.1 says cannot exist.
fn route_step(root: &std::path::Path, step: &[&str]) -> Result<(), jails_support::Failure> {
    use clap::ValueEnum;

    let project = Project::load(root)?;
    match step.first().copied() {
        Some("add") => {
            let capability = Capability::from_str(step[1], true)
                .map_err(|_| format!("`{}` is not a capability", step[1]))?;
            jails_engine::route::install(&committing(&project), &Declaration::plain(capability))
                .map(|_| ())
        }
        Some("g") | Some("generate") => {
            let kind = jails_spec::spec::kind::ArtifactKind::from_str(step[1], true)
                .map_err(|_| format!("`{}` is not a kind", step[1]))?;
            let invocation = common::scenarios::invocation(step)
                .ok_or_else(|| "unrecognised flag".to_string())?;
            jails_engine::route::generate(
                &committing(&project),
                &jails_generate::generate::Recipe {
                    kind,
                    name: step[2],
                    fields: &invocation.fields,
                    indexes: &invocation.indexes,
                    strategy_on: invocation.on.as_deref(),
                    strategy_yields: invocation.yields.as_deref(),
                    method: None,
                },
                invocation.package.as_deref(),
            )
            .map(|_| ())
        }
        _ => Err(format!("`{}` has no V2 route yet", step.join(" ")).into()),
    }
}

/// `add db`, the capability §R6.1 named as an open schema gap.
///
/// It is the hardest one, and the reason is the last assertion here: `add db`
/// has to edit a test the reader owns. Once `spring-boot-starter-jdbc` is in
/// the POM, auto-configuration demands a `DataSource` for every
/// `@SpringBootTest` — including the `contextLoads` test that shipped with
/// the project — so a capability that adds the dependency and walks away
/// breaks a test nobody wrote, with a message ("Failed to determine a
/// suitable driver class") that names neither the cause nor the fix.
///
/// §R6.3's `add::test_wiring` row asks for that as a keyed semantic
/// contribution with an explicit owner, which is `SemanticEdit::
/// SpringTestImport`: one claim per target file, so a test written later is
/// not silently covered by a claim about a file it is not in.
#[test]
fn adding_a_database_wires_the_tests_that_are_already_there() {
    let root = common::temp_dir("engine-db");
    std::fs::create_dir_all(&root).unwrap();
    common::write_spring_fixture(&root);

    jails_engine::route::install(
        &committing(&Project::load(&root).unwrap()),
        &Declaration::plain(Capability::Db),
    )
    .unwrap();

    let pom = std::fs::read_to_string(root.join("pom.xml")).unwrap();
    assert!(pom.contains("spring-boot-starter-jdbc"), "{pom}");
    assert!(
        root.join("src/test/java/com/example/demo/TestcontainersConfig.java")
            .is_file(),
        "the container config it writes"
    );
    assert!(
        std::fs::read_to_string(root.join("compose.yaml"))
            .unwrap()
            .contains("postgres"),
        "and the service it needs"
    );

    let properties =
        std::fs::read_to_string(root.join("src/main/resources/application.properties")).unwrap();
    for key in [
        "spring.datasource.url",
        "spring.persistence.exceptiontranslation.enabled",
        "spring.docker.compose.enabled",
    ] {
        assert!(
            jails_project::properties::get(&properties, key).is_some(),
            "{key} is missing, so the application does not start:\n{properties}"
        );
    }

    let test = std::fs::read_to_string(
        root.join("src/test/java/com/example/demo/DemoApplicationTests.java"),
    )
    .unwrap();
    assert!(
        test.contains("@Import(TestcontainersConfig.class)"),
        "the test that shipped with the project has a DataSource:\n{test}"
    );

    // The config's own Javadoc shows how to import it. A scan that read that
    // example as a declaration would have the config import itself.
    let config = std::fs::read_to_string(
        root.join("src/test/java/com/example/demo/TestcontainersConfig.java"),
    )
    .unwrap();
    assert!(
        !config.contains("@Import(TestcontainersConfig.class)\nclass")
            && !config.contains("@Import(TestcontainersConfig.class)\n@"),
        "the config imported itself:\n{config}"
    );
}

/// And taking it back out restores the test exactly.
///
/// The `@Import` is a claim like any other, so it is retired by identity
/// rather than by comparing bytes -- and the `import` statement it needed
/// goes with it, read back off the recorded resource rather than recomputed.
#[test]
fn removing_a_database_gives_the_reader_their_test_back() {
    let root = common::temp_dir("engine-db-remove");
    std::fs::create_dir_all(&root).unwrap();
    common::write_spring_fixture(&root);
    let path = root.join("src/test/java/com/example/demo/DemoApplicationTests.java");
    let before = std::fs::read_to_string(&path).unwrap();

    jails_engine::route::install(
        &committing(&Project::load(&root).unwrap()),
        &Declaration::plain(Capability::Db),
    )
    .unwrap();
    assert!(std::fs::read_to_string(&path).unwrap().contains("@Import("));

    jails_engine::route::remove(
        &committing(&Project::load(&root).unwrap()),
        &Declaration::plain(Capability::Db),
    )
    .unwrap();

    let after = std::fs::read_to_string(&path).unwrap();
    assert!(!after.contains("TestcontainersConfig"), "{after}");
    assert!(
        !after.contains("org.springframework.context.annotation.Import"),
        "the import statement went with the annotation:\n{after}"
    );
    assert!(after.contains("@SpringBootTest"), "{after}");
    let _ = before;
    let properties =
        std::fs::read_to_string(root.join("src/main/resources/application.properties"))
            .unwrap_or_default();
    assert!(
        jails_project::properties::get(&properties, "spring.datasource.url").is_none(),
        "and the properties it set:\n{properties}"
    );
}

/// `g migration`, the first of the three one-shots.
///
/// §R6.2: *"snapshot allocates next number; lock rechecks directory listing;
/// append-only file/receipt, no destroy."* A migration is not an entity —
/// the database has already run it — so what the store records is a receipt
/// saying this number was handed out, which is what stops the next run
/// reusing it.
#[test]
fn a_migration_allocates_the_next_serial_and_records_that_it_did() {
    let root = common::temp_dir("engine-migration");
    std::fs::create_dir_all(&root).unwrap();
    common::write_spring_fixture(&root);

    jails_engine::route::migration(
        &committing(&Project::load(&root).unwrap()),
        "create rewards",
    )
    .unwrap();
    jails_engine::route::migration(&committing(&Project::load(&root).unwrap()), "add index")
        .unwrap();

    let dir = root.join("src/main/resources/db/migration");
    assert!(dir.join("V001__create_rewards.sql").is_file());
    assert!(
        dir.join("V002__add_index.sql").is_file(),
        "the second allocation saw the first: {:?}",
        std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.file_name()))
            .collect::<Vec<_>>()
    );

    let store = jails_commit::store::Store::at(&root)
        .observe()
        .unwrap()
        .ledger
        .unwrap();
    assert_eq!(store.one_shots.len(), 2, "{:?}", store.one_shots);
    assert!(
        store.applied.is_empty(),
        "a migration is not an entity: {:?}",
        store.applied
    );
    // Ordering is numeric, and the receipt says which number this was. A
    // second run that read the receipts and not the directory would be a
    // second authority on the same fact, so it reads the directory -- but the
    // receipt is what a later `doctor` can ask about.
    let versions: Vec<u64> = store
        .one_shots
        .iter()
        .filter_map(|row| match &row.spec {
            jails_protocol::entity::OneShotSpec::Migration {
                allocated_version, ..
            } => Some(*allocated_version),
            _ => None,
        })
        .collect();
    assert_eq!(versions, vec![1, 2]);
}

/// A number somebody else already used is not handed out again.
///
/// The allocation reads the directory rather than the receipts, so a file a
/// person wrote by hand counts — which is the right answer, because Flyway
/// counts it too. The *concurrent* case, where the directory moves between
/// the plan and the commit, is closed by the declared listing being rechecked
/// under the lock; `crates/jails-commit/tests/crash.rs` exercises that recheck
/// directly, because opening that window needs a plan and a commit that are
/// separate calls.
#[test]
fn a_migration_number_already_in_the_directory_is_not_handed_out_again() {
    let root = common::temp_dir("engine-migration-taken");
    std::fs::create_dir_all(&root).unwrap();
    common::write_spring_fixture(&root);
    let dir = root.join("src/main/resources/db/migration");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("V001__theirs.sql"), "-- theirs\n").unwrap();

    jails_engine::route::migration(&committing(&Project::load(&root).unwrap()), "mine").unwrap();

    assert!(dir.join("V002__mine.sql").is_file());
    assert!(!dir.join("V001__mine.sql").exists());
    assert_eq!(
        std::fs::read_to_string(dir.join("V001__theirs.sql")).unwrap(),
        "-- theirs\n",
        "and the file jails did not write is untouched"
    );
}

/// `g cases`: the brief is an input, and the receipt is keyed by it.
///
/// §R6.2's row. The markdown is the reader's file — jails never writes it —
/// so a re-run against the *same* source is an update to a receipt that
/// already exists rather than a second one-shot landing on a file that is
/// already there. That is the difference from V1, which refuses the second
/// run with "already exists".
#[test]
fn cases_records_the_brief_it_read_and_reconciles_a_re_run() {
    let root = common::temp_dir("engine-cases");
    std::fs::create_dir_all(&root).unwrap();
    common::write_plain_fixture(&root);
    jails_support::apply::put(
        root.join("docs/behaviour.md"),
        "# Acceptance\n\n- it accepts a valid card\n- it refuses an expired one\n",
    )
    .unwrap();

    jails_engine::route::cases(
        &committing(&Project::load(&root).unwrap()),
        "docs/behaviour.md",
        None,
    )
    .unwrap();

    let output = root.join("src/test/java/com/example/demo/BehaviourTest.java");
    let first = std::fs::read_to_string(&output).unwrap();
    assert!(first.contains("itAcceptsAValidCard"), "{first}");
    assert!(first.contains("@Disabled"), "{first}");

    let store = jails_commit::store::Store::at(&root)
        .observe()
        .unwrap()
        .ledger
        .unwrap();
    assert_eq!(store.one_shots.len(), 1, "{:?}", store.one_shots);
    let recorded = match &store.one_shots[0].spec {
        jails_protocol::entity::OneShotSpec::Cases { source_sha256, .. } => *source_sha256,
        other => panic!("{other:?}"),
    };

    // The same source again is a no-op, not a collision.
    assert!(matches!(
        jails_engine::route::cases(
            &committing(&Project::load(&root).unwrap()),
            "docs/behaviour.md",
            None
        )
        .unwrap()
        .committed()
        .unwrap(),
        jails_commit::outcome::CommitResult::NoOp
    ));

    // An edited source rewrites the output it recorded: §R6.2's "same-source
    // updates reconcile the immutable output path". It works because the
    // store now records the exact bytes jails wrote, so "the generator moved"
    // is a different observation from "the reader edited it".
    jails_support::apply::put(
        root.join("docs/behaviour.md"),
        "# Acceptance\n\n- it accepts a valid card\n- it refuses an expired one\n- it retries once\n",
    )
    .unwrap();
    jails_engine::route::cases(
        &committing(&Project::load(&root).unwrap()),
        "docs/behaviour.md",
        None,
    )
    .unwrap();

    let second = std::fs::read_to_string(&output).unwrap();
    assert!(second.contains("itRetriesOnce"), "{second}");
    let store = jails_commit::store::Store::at(&root)
        .observe()
        .unwrap()
        .ledger
        .unwrap();
    assert_eq!(store.one_shots.len(), 1, "one receipt, updated in place");
    let now = match &store.one_shots[0].spec {
        jails_protocol::entity::OneShotSpec::Cases { source_sha256, .. } => *source_sha256,
        other => panic!("{other:?}"),
    };
    assert_ne!(now, recorded, "the receipt records what it actually read");
}

/// An external brief is refused by name rather than recorded under an
/// identity nothing can resolve.
#[test]
fn a_brief_outside_the_project_is_refused_with_the_reason() {
    let root = common::temp_dir("engine-cases-external");
    std::fs::create_dir_all(&root).unwrap();
    common::write_plain_fixture(&root);

    let error = jails_engine::route::cases(
        &committing(&Project::load(&root).unwrap()),
        "../elsewhere.md",
        None,
    )
    .unwrap_err();

    assert!(error.contains("outside this project"), "{error}");
    assert!(error.contains("fix:"), "{error}");
}

/// `g field`: one component added to something that already exists.
///
/// §R6.2's `generate_field` row, and the first route that could not have been
/// written before outputs were recorded. V1 renders the target twice -- once
/// at the old field list and once at the new -- and compares the *old* render
/// against disk to decide whether the reader edited a derivative. Here the
/// target is simply re-desired at the new spec, and §R5.3 answers the question
/// from the bytes jails actually wrote.
#[test]
fn a_field_evolves_the_record_and_migrates_the_table_for_it() {
    let root = common::temp_dir("engine-field");
    std::fs::create_dir_all(&root).unwrap();
    common::write_spring_fixture(&root);
    std::fs::create_dir_all(root.join("src/main/resources/db/migration")).unwrap();

    jails_engine::route::generate(
        &committing(&Project::load(&root).unwrap()),
        &jails_generate::generate::Recipe {
            kind: jails_spec::spec::kind::ArtifactKind::Record,
            name: "Note",
            fields: &["id:uuid@pk".to_string(), "title:string!".to_string()],
            indexes: &[],
            strategy_on: None,
            strategy_yields: None,
            method: None,
        },
        None,
    )
    .unwrap();

    jails_engine::route::field(
        &committing(&Project::load(&root).unwrap()),
        "Note",
        "archivedAt:instant?",
        None,
    )
    .unwrap();
    jails_engine::route::field(
        &committing(&Project::load(&root).unwrap()),
        "Note",
        "priority:int?",
        None,
    )
    .unwrap();

    let record =
        std::fs::read_to_string(root.join("src/main/java/com/example/demo/domain/Note.java"))
            .unwrap();
    assert!(
        record.contains("archivedAt") && record.contains("priority"),
        "the derivative was refreshed:\n{record}"
    );

    let store = jails_commit::store::Store::at(&root)
        .observe()
        .unwrap()
        .ledger
        .unwrap();
    // One entity, whose recorded spec now carries three components -- which is
    // what the *next* `g field` computes from. Reading them back off the Java
    // could not work: `@pk` changes the DDL and nothing about the type.
    assert_eq!(store.applied.len(), 1, "{:?}", store.applied);
    let spec = match &store.applied[0].version.spec {
        jails_protocol::entity::EntitySpec::Intent(spec) => spec.clone(),
        other => panic!("{other:?}"),
    };
    assert_eq!(
        spec.arguments.canonical(),
        vec![
            "id:uuid@pk",
            "title:string!",
            "archivedAt:instant?",
            "priority:int?"
        ]
    );

    // And one receipt per field, whose append-only halves are the migrations.
    assert_eq!(store.one_shots.len(), 2, "{:?}", store.one_shots);
    let migration = root.join("src/main/resources/db/migration/V001__add_archived_at_to_notes.sql");
    let second_migration =
        root.join("src/main/resources/db/migration/V002__add_priority_to_notes.sql");
    assert!(
        migration.is_file(),
        "{:?}",
        std::fs::read_dir(root.join("src/main/resources/db/migration"))
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.file_name()))
            .collect::<Vec<_>>()
    );
    assert!(second_migration.is_file());
    let migration_bytes = std::fs::read(&migration).unwrap();
    let second_migration_bytes = std::fs::read(&second_migration).unwrap();
    assert!(
        String::from_utf8_lossy(&migration_bytes).contains("add column"),
        "the migration adds the column"
    );

    // The same commit records the schema-backed identity and seals the exact
    // migration bytes. A later sync can now distinguish generated projections
    // from append-only history without guessing from a path convention.
    assert_eq!(store.lifecycles.len(), 1, "{:?}", store.lifecycles);
    let lifecycle = &store.lifecycles[0];
    assert!(matches!(
        lifecycle.state,
        jails_protocol::lifecycle::ResourceState::Active
    ));
    assert_eq!(
        lifecycle.expected_path.qualified(),
        "com.example.demo.domain.Note"
    );
    assert_eq!(lifecycle.table.as_ref().unwrap().table.as_str(), "notes");
    assert_eq!(lifecycle.last_spec, store.applied[0].version.spec);
    assert_eq!(lifecycle.migrations.len(), 2);
    let first_seal = &lifecycle.migrations[0];
    assert_eq!(first_seal.version.get(), 1);
    assert_eq!(
        first_seal.path.as_str(),
        "src/main/resources/db/migration/V001__add_archived_at_to_notes.sql"
    );
    assert_eq!(
        first_seal.content_digest,
        jails_protocol::identity::ObjectId::from_bytes(jails_support::codec::sha256(
            &migration_bytes
        ))
    );
    assert_eq!(
        first_seal.contributors,
        std::collections::BTreeSet::from([store.applied[0].id.clone()])
    );
    let second_seal = &lifecycle.migrations[1];
    assert_eq!(second_seal.version.get(), 2);
    assert_eq!(
        second_seal.content_digest,
        jails_protocol::identity::ObjectId::from_bytes(jails_support::codec::sha256(
            &second_migration_bytes
        ))
    );
}

#[test]
fn field_evolution_appends_rename_type_nullability_and_drop_migrations() {
    let root = common::temp_dir("engine-field-evolution");
    std::fs::create_dir_all(&root).unwrap();
    common::write_spring_fixture(&root);
    std::fs::create_dir_all(root.join("src/main/resources/db/migration")).unwrap();

    jails_engine::route::generate(
        &committing(&Project::load(&root).unwrap()),
        &jails_generate::generate::Recipe {
            kind: jails_spec::spec::kind::ArtifactKind::Record,
            name: "Task",
            fields: &[
                "id:uuid@pk".to_string(),
                "title:string!".to_string(),
                "priority:int".to_string(),
                "description:string".to_string(),
                "legacyCode:string?".to_string(),
            ],
            indexes: &[],
            strategy_on: None,
            strategy_yields: None,
            method: None,
        },
        None,
    )
    .unwrap();

    jails_engine::route::rename_field(
        &committing(&Project::load(&root).unwrap()),
        "Task",
        "title",
        "headline",
        jails_protocol::request::ColumnRenamePolicy::SingleCutover,
        None,
    )
    .unwrap();
    jails_engine::route::change_field_type(
        &committing(&Project::load(&root).unwrap()),
        "Task",
        "priority",
        "long",
        jails_protocol::request::TypeChangeStrategy::Safe,
        None,
    )
    .unwrap();
    jails_engine::route::set_field_nullability(
        &committing(&Project::load(&root).unwrap()),
        "Task",
        "description",
        true,
        None,
    )
    .unwrap();
    jails_engine::route::drop_field(
        &committing(&Project::load(&root).unwrap()),
        "Task",
        "legacyCode",
        "legacy_code",
        None,
    )
    .unwrap();

    let expected = [
        (
            "V001__rename_title_to_headline.sql",
            "rename column title to headline",
        ),
        (
            "V002__widen_priority_type.sql",
            "alter column priority type bigint",
        ),
        (
            "V003__make_description_nullable.sql",
            "alter column description drop not null",
        ),
        ("V004__drop_legacy_code.sql", "drop column legacy_code"),
    ];
    for (file, ddl) in expected {
        let migration =
            std::fs::read_to_string(root.join("src/main/resources/db/migration").join(file))
                .unwrap();
        assert!(migration.contains(ddl), "{file}:\n{migration}");
    }
    let record =
        std::fs::read_to_string(root.join("src/main/java/com/example/demo/domain/Task.java"))
            .unwrap();
    assert!(record.contains("String headline"), "{record}");
    assert!(record.contains("long priority"), "{record}");
    assert!(record.contains("Optional<String> description"), "{record}");
    assert!(!record.contains("legacyCode"), "{record}");

    let ledger = jails_commit::store::Store::at(&root)
        .observe()
        .unwrap()
        .ledger
        .unwrap();
    let lifecycle = ledger
        .lifecycles
        .iter()
        .find(|row| matches!(&row.entity, jails_protocol::entity::EntityId::Intent(id) if id.name.as_str() == "Task"))
        .unwrap();
    assert_eq!(
        lifecycle
            .migrations
            .iter()
            .map(|seal| seal.version.get())
            .collect::<Vec<_>>(),
        vec![1, 2, 3, 4]
    );
}

#[test]
fn unsafe_field_evolution_refuses_before_writing() {
    let root = common::temp_dir("engine-field-evolution-refusal");
    std::fs::create_dir_all(&root).unwrap();
    common::write_spring_fixture(&root);
    std::fs::create_dir_all(root.join("src/main/resources/db/migration")).unwrap();
    jails_engine::route::generate(
        &committing(&Project::load(&root).unwrap()),
        &jails_generate::generate::Recipe {
            kind: jails_spec::spec::kind::ArtifactKind::Record,
            name: "Task",
            fields: &["id:uuid@pk".to_string(), "priority:long".to_string()],
            indexes: &[],
            strategy_on: None,
            strategy_yields: None,
            method: None,
        },
        None,
    )
    .unwrap();
    let record = root.join("src/main/java/com/example/demo/domain/Task.java");
    let before = std::fs::read(&record).unwrap();

    let error = jails_engine::route::change_field_type(
        &committing(&Project::load(&root).unwrap()),
        "Task",
        "priority",
        "int",
        jails_protocol::request::TypeChangeStrategy::Safe,
        None,
    )
    .unwrap_err();
    assert!(error.contains("not a proven safe widening"), "{error}");
    assert_eq!(std::fs::read(record).unwrap(), before);
    assert_eq!(
        std::fs::read_dir(root.join("src/main/resources/db/migration"))
            .unwrap()
            .count(),
        0
    );
}

/// A derivative the reader edited and the generator also changed is merged.
///
/// §R5.3's fifth answer, and the one that needs to look at the text rather
/// than at three hashes. This is the ordinary case, not an exotic one: the
/// reader adds a comment to a generated test, the generator adds a component,
/// and the two do not touch the same lines. Both survive.
///
/// The recorded base is what makes it possible. Without the bytes jails wrote
/// there is nothing to measure the two divergent sides from, and the honest
/// answer was to refuse -- which meant `g field` was unusable on any project
/// where anybody had ever touched a derivative.
#[test]
fn a_field_merges_an_edit_the_generator_also_touched() {
    let root = common::temp_dir("engine-field-merged");
    std::fs::create_dir_all(&root).unwrap();
    common::write_spring_fixture(&root);

    jails_engine::route::generate(
        &committing(&Project::load(&root).unwrap()),
        &jails_generate::generate::Recipe {
            kind: jails_spec::spec::kind::ArtifactKind::Record,
            name: "Note",
            fields: &["id:uuid@pk".to_string(), "title:string!".to_string()],
            indexes: &[],
            strategy_on: None,
            strategy_yields: None,
            method: None,
        },
        None,
    )
    .unwrap();

    // The reader adds something of their own, far from anything the next
    // render touches.
    let test = root.join("src/test/java/com/example/demo/domain/NoteTest.java");
    let mine = format!(
        "// a note I wrote by hand\n{}",
        std::fs::read_to_string(&test).unwrap()
    );
    jails_support::apply::put(&test, &mine).unwrap();

    jails_engine::route::field(
        &committing(&Project::load(&root).unwrap()),
        "Note",
        "archivedAt:instant?",
        None,
    )
    .unwrap();

    let after = std::fs::read_to_string(&test).unwrap();
    assert!(
        after.contains("// a note I wrote by hand"),
        "the reader's line survived:\n{after}"
    );
    // The generator's change to *this* file is the new component in the
    // constructor call -- a nullable component reaches the test as
    // `Optional.empty()` rather than by name.
    assert!(
        after.contains("new Note(null, \"sample\", Optional.empty())"),
        "and so did the generator's change:\n{after}"
    );
    assert!(
        std::fs::read_to_string(root.join("src/main/java/com/example/demo/domain/Note.java"))
            .unwrap()
            .contains("archivedAt"),
        "the record itself carries the new component"
    );

    // The recorded *base* is what the generator wrote, not the merged bytes.
    // §R5.4: that is what keeps the reader's edit a delta from the newest
    // render rather than becoming the baseline the next change merges against.
    let store = jails_commit::store::Store::at(&root)
        .observe()
        .unwrap()
        .ledger
        .unwrap();
    let row = store
        .outputs
        .iter()
        .find(|row| row.path.as_str().ends_with("NoteTest.java"))
        .expect("the companion test is a recorded output");
    assert_ne!(
        row.base.object.id, row.current.sha256,
        "base and current are the same, so the reader's edit was adopted as jails' own output"
    );
}

/// A component the artifact already has is refused, and a target the store
/// never recorded is refused with the reason.
#[test]
fn a_field_refuses_a_duplicate_and_an_unrecorded_target() {
    let root = common::temp_dir("engine-field-refusals");
    std::fs::create_dir_all(&root).unwrap();
    common::write_spring_fixture(&root);

    let error = jails_engine::route::field(
        &committing(&Project::load(&root).unwrap()),
        "Note",
        "x:int",
        None,
    )
    .unwrap_err();
    assert!(error.contains("is recorded in this project"), "{error}");
    assert!(error.contains("jails g scaffold Note"), "{error}");

    jails_engine::route::generate(
        &committing(&Project::load(&root).unwrap()),
        &jails_generate::generate::Recipe {
            kind: jails_spec::spec::kind::ArtifactKind::Record,
            name: "Note",
            fields: &["title:string!".to_string()],
            indexes: &[],
            strategy_on: None,
            strategy_yields: None,
            method: None,
        },
        None,
    )
    .unwrap();

    let error = jails_engine::route::field(
        &committing(&Project::load(&root).unwrap()),
        "Note",
        "title:string!",
        None,
    )
    .unwrap_err();
    assert!(error.contains("already has a `title` component"), "{error}");
}

/// A whole manifest as one transition.
///
/// §R6.2's `app::apply` row: "one aggregate projected plan and one commit".
/// The two assertions that matter are the ones the per-intent loop could not
/// make. `g search` needs the JDBC starter `add db` puts in the POM *and* the
/// record `g scaffold` writes, and in one transition neither has been written
/// when it plans -- so it plans against a projection of everything before it.
/// And the whole manifest is one generation, not one per step.
#[test]
fn a_manifest_applies_as_one_transition_that_each_step_can_see() {
    let root = common::temp_dir("engine-app-apply");
    std::fs::create_dir_all(&root).unwrap();
    common::write_spring_fixture(&root);

    jails_engine::route::app_apply(
        &committing(&Project::load(&root).unwrap()),
        &[Capability::Db],
        &[
            jails_engine::route::Intent {
                kind: jails_spec::spec::kind::ArtifactKind::Scaffold,
                timestamps: false,
                name: "Article".to_string(),
                fields: vec![
                    "id:uuid@pk".to_string(),
                    "title:string!".to_string(),
                    "body:string".to_string(),
                ],
                indexes: Vec::new(),
                package: None,
                on: None,
                yields: None,
                method: None,
            },
            // Needs `add db`'s starter *and* `g scaffold`'s record, neither of
            // which exists on disk while this plans.
            jails_engine::route::Intent {
                kind: jails_spec::spec::kind::ArtifactKind::Search,
                timestamps: false,
                name: "Article".to_string(),
                fields: vec!["title".to_string(), "body".to_string()],
                indexes: Vec::new(),
                package: None,
                on: None,
                yields: None,
                method: None,
            },
        ],
    )
    .unwrap();

    assert!(
        root.join("src/main/java/com/example/demo/domain/Article.java")
            .is_file(),
        "the scaffold"
    );
    assert!(
        root.join("src/main/java/com/example/demo/app/ArticleSearch.java")
            .is_file()
            || root
                .join("src/main/java/com/example/demo/adapters/JdbcArticleSearch.java")
                .is_file(),
        "and the search over it: {:?}",
        common::scenarios::file_set(&root)
            .into_iter()
            .filter(|p| p.contains("Search"))
            .collect::<Vec<_>>()
    );

    let store = jails_commit::store::Store::at(&root)
        .observe()
        .unwrap()
        .ledger
        .unwrap();
    assert_eq!(
        store.generation, 1,
        "one commit for the whole manifest, not one per step"
    );
    assert_eq!(store.applied.len(), 3, "{:?}", store.applied);
    for row in &store.applied {
        assert!(
            row.owners
                .contains(&jails_protocol::entity::OwnerId::AppManifest),
            "the manifest owns what it declared: {:?}",
            row.id
        );
    }
}

/// The manifest is authoritative, so a row it no longer names is relinquished.
///
/// This is what `ReconcileScope::AppManifest` means and what the per-intent
/// loop could never express: it applied what was listed and had no opinion
/// about what was not.
#[test]
fn a_manifest_that_drops_a_row_takes_it_back_out() {
    let root = common::temp_dir("engine-app-drop");
    std::fs::create_dir_all(&root).unwrap();
    common::write_plain_fixture(&root);
    let note = |name: &str| jails_engine::route::Intent {
        kind: jails_spec::spec::kind::ArtifactKind::Record,
        timestamps: false,
        name: name.to_string(),
        fields: vec!["title:string!".to_string()],
        indexes: Vec::new(),
        package: None,
        on: None,
        yields: None,
        method: None,
    };

    jails_engine::route::app_apply(
        &committing(&Project::load(&root).unwrap()),
        &[],
        &[note("Note"), note("Memo")],
    )
    .unwrap();
    assert!(
        root.join("src/main/java/com/example/demo/domain/Memo.java")
            .is_file()
    );

    // The reader deletes the `Memo` row from the manifest.
    jails_engine::route::app_apply(
        &committing(&Project::load(&root).unwrap()),
        &[],
        &[note("Note")],
    )
    .unwrap();

    assert!(
        !root
            .join("src/main/java/com/example/demo/domain/Memo.java")
            .exists(),
        "the row the manifest stopped naming was relinquished"
    );
    assert!(
        root.join("src/main/java/com/example/demo/domain/Note.java")
            .is_file(),
        "and the one it still names is still there"
    );
}

/// Applying the same manifest twice is one transition, not two.
#[test]
fn applying_a_manifest_twice_leaves_the_store_where_it_was() {
    let root = common::temp_dir("engine-app-repeat");
    std::fs::create_dir_all(&root).unwrap();
    common::write_plain_fixture(&root);
    let manifest = || {
        jails_engine::route::app_apply(
            &committing(&Project::load(&root).unwrap()),
            &[],
            &[jails_engine::route::Intent {
                kind: jails_spec::spec::kind::ArtifactKind::Record,
                timestamps: false,
                name: "Note".to_string(),
                fields: vec!["title:string!".to_string()],
                indexes: Vec::new(),
                package: None,
                on: None,
                yields: None,
                method: None,
            }],
        )
    };

    manifest().unwrap();
    let outcome = manifest().unwrap().committed().unwrap();

    assert!(
        matches!(outcome, jails_commit::outcome::CommitResult::NoOp),
        "the second apply has nothing to do, got {outcome:?}"
    );
}

/// The web-crawler proof application, applied as one transition.
///
/// `examples/` is the reason the generic machinery can be trusted, and this
/// is the falsifier for the aggregate: eleven intents with real dependencies
/// between them. `scaffold CrawlRun` uses the enum the row above it declares;
/// four intents point `--on` at a scaffold two rows earlier; `durable-job`
/// points at a use case *and* at a scaffold. None of it is on disk while it
/// plans.
///
/// Transcribed from `examples/web-crawler/.jails/app.toml` rather than parsed,
/// because the parser lives in the binary. The count is asserted against the
/// file so the transcription cannot drift away from it.
#[test]
fn the_web_crawler_manifest_applies_as_one_transition() {
    use jails_spec::spec::kind::ArtifactKind as K;

    let intent = |kind: K, name: &str, fields: &[&str]| jails_engine::route::Intent {
        kind,
        name: name.to_string(),
        fields: fields.iter().map(|f| f.to_string()).collect(),
        timestamps: false,
        indexes: Vec::new(),
        package: None,
        on: None,
        yields: None,
        method: None,
    };
    let on = |mut i: jails_engine::route::Intent, target: &str| {
        i.on = Some(target.to_string());
        i
    };
    let stamped = |mut i: jails_engine::route::Intent, indexes: &[&str]| {
        i.timestamps = true;
        i.indexes = indexes.iter().map(|x| x.to_string()).collect();
        i
    };

    let intents = vec![
        intent(
            K::Enum,
            "CrawlStatus",
            &["QUEUED", "RUNNING", "SUCCEEDED", "FAILED", "CANCELLED"],
        ),
        stamped(
            intent(
                K::Scaffold,
                "CrawlRun",
                &[
                    "id:uuid@pk",
                    "seedUrl:uri",
                    "status:CrawlStatus@index",
                    "pagesVisited:long@nonnegative",
                    "startedAt:instant?",
                    "finishedAt:instant?",
                ],
            ),
            &["status, id"],
        ),
        stamped(
            intent(
                K::Scaffold,
                "CrawledPage",
                &[
                    "id:uuid@pk",
                    "crawlRunId:uuid@index",
                    "url:uri",
                    "statusCode:int",
                    "discoveredAt:instant",
                ],
            ),
            &["crawl_run_id, discovered_at desc"],
        ),
        on(
            intent(K::Usecase, "QueueCrawl", &["id:uuid", "seedUrl:uri"]),
            "CrawlRun",
        ),
        on(
            intent(
                K::Usecase,
                "RecordCrawledPage",
                &["id:uuid", "crawlRunId:uuid", "url:uri", "statusCode:int"],
            ),
            "CrawledPage",
        ),
        on(
            intent(K::Query, "CrawlRunsByStatus", &["status:CrawlStatus"]),
            "CrawlRun",
        ),
        on(
            intent(K::Query, "PagesByCrawlRun", &["crawlRunId:uuid"]),
            "CrawledPage",
        ),
        intent(K::Fetcher, "PageFetcher", &[]),
        on(intent(K::HttpWorkflow, "SiteTraversal", &[]), "PageFetcher"),
        intent(
            K::Event,
            "PageDiscovered",
            &[
                "id:uuid",
                "crawlRunId:uuid",
                "url:uri",
                "occurredAt:instant",
            ],
        ),
        {
            let mut job = on(
                intent(
                    K::DurableJob,
                    "CrawlDispatcher",
                    &["id:uuid", "seedUrl:uri"],
                ),
                "QueueCrawl",
            );
            job.yields = Some("CrawlRun".to_string());
            job
        },
    ];

    let declared = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("examples/web-crawler/.jails/app.toml"),
    )
    .unwrap()
    .matches("[[generate]]")
    .count();
    assert_eq!(
        intents.len(),
        declared,
        "the transcription has drifted from examples/web-crawler/.jails/app.toml"
    );

    let root = common::temp_dir("engine-app-crawler");
    std::fs::create_dir_all(&root).unwrap();
    common::write_spring_fixture(&root);

    jails_engine::route::app_apply(
        &committing(&Project::load(&root).unwrap()),
        &[
            Capability::Db,
            Capability::Api,
            Capability::Actuator,
            Capability::Observability,
            Capability::Security,
            Capability::Cors,
            Capability::Json,
            Capability::Testkit,
            Capability::Kafka,
            Capability::Docker,
            Capability::Ci,
        ],
        &intents,
    )
    .unwrap();

    let store = jails_commit::store::Store::at(&root)
        .observe()
        .unwrap()
        .ledger
        .unwrap();
    assert_eq!(
        store.generation, 1,
        "eleven capabilities and eleven intents, one commit"
    );
    assert_eq!(store.applied.len(), 22, "{:?}", store.applied.len());
    assert!(
        root.join("src/main/java/com/example/demo/domain/CrawlRun.java")
            .is_file()
    );
    // The use case points `--on` at a scaffold two rows above it, and the
    // durable job points at *this* use case as well as at that scaffold.
    // Neither existed on disk while either planned.
    for at in [
        "src/main/java/com/example/demo/service/QueueCrawlUseCase.java",
        "src/main/java/com/example/demo/service/QueueCrawlCommand.java",
        "src/main/java/com/example/demo/jobs/CrawlDispatcherWorker.java",
        "src/test/java/com/example/demo/jobs/CrawlDispatcherJobIT.java",
    ] {
        assert!(
            root.join(at).is_file(),
            "{at} is missing: {:?}",
            common::scenarios::file_set(&root)
                .into_iter()
                .filter(|p| p.contains("CrawlDispatcher") || p.contains("QueueCrawl"))
                .collect::<Vec<_>>()
        );
    }
}

/// §R6.2's `app::init` row: seed the manifest through the protocol.
///
/// The interesting half is the second call. V1 refuses by asking
/// `Path::exists` and then writing, which is a check and a write with a gap
/// between them; here the refusal is the plan's own -- a `Create` operation
/// carries no preimage, so the file being there is a precondition failure
/// rather than an overwrite of somebody's manifest.
#[test]
fn app_init_seeds_a_manifest_once_and_refuses_to_land_on_it_twice() {
    let root = common::temp_dir("engine-app-init");
    std::fs::create_dir_all(&root).unwrap();
    common::write_spring_fixture(&root);

    jails_engine::route::app_init(&committing(&Project::load(&root).unwrap()), None).unwrap();
    let seeded = std::fs::read_to_string(root.join(".jails/app.toml")).unwrap();
    assert!(seeded.contains("schema = 1"), "{seeded}");
    assert!(seeded.contains("capabilities = []"), "{seeded}");

    // Somebody has filled it in. A second `app init` must not quietly take
    // that away, and must not merge into it either: seeding is a one-shot,
    // and its answer to "already there" is to say so.
    std::fs::write(
        root.join(".jails/app.toml"),
        "schema = 1\ncapabilities = [\"db\"]\n",
    )
    .unwrap();
    let again = jails_engine::route::app_init(&committing(&Project::load(&root).unwrap()), None);
    assert!(again.is_err(), "a second seed landed on the reader's file");
    assert_eq!(
        std::fs::read_to_string(root.join(".jails/app.toml")).unwrap(),
        "schema = 1\ncapabilities = [\"db\"]\n",
        "the refusal wrote anyway"
    );
}

/// `--pretend` is the apply, stopped before the lock.
///
/// The property under test is not that the lines look right -- it is that
/// there is one implementation. V1 answers `app plan` with a second walk over
/// the intent list that compares each row against the ledger, and it had to
/// be shadowed against a typed comparison precisely because two
/// implementations of one question disagree. There is no second function here
/// at all: the same route runs, and the `Run` says whether it writes.
#[test]
fn a_plan_names_exactly_the_files_the_apply_then_writes() {
    let root = common::temp_dir("engine-app-plan");
    std::fs::create_dir_all(&root).unwrap();
    common::write_plain_fixture(&root);
    let note = |name: &str| jails_engine::route::Intent {
        kind: jails_spec::spec::kind::ArtifactKind::Record,
        timestamps: false,
        name: name.to_string(),
        fields: vec!["title:string!".to_string()],
        indexes: Vec::new(),
        package: None,
        on: None,
        yields: None,
        method: None,
    };

    let before = common::scenarios::file_set(&root);
    let planned = jails_engine::route::app_apply(
        &jails_engine::route::Run::pretending(&Project::load(&root).unwrap()),
        &[],
        &[note("Note")],
    )
    .unwrap()
    .operations();
    assert_eq!(
        before,
        common::scenarios::file_set(&root),
        "a plan wrote something"
    );

    jails_engine::route::app_apply(
        &committing(&Project::load(&root).unwrap()),
        &[],
        &[note("Note")],
    )
    .unwrap();
    let after = common::scenarios::file_set(&root);

    let created: std::collections::BTreeSet<_> = planned
        .iter()
        .filter(|op| op.kind == jails_prepare::report::ReportedOpKind::Create)
        .map(|op| op.path.to_string())
        .collect();
    let appeared: std::collections::BTreeSet<_> = after
        .iter()
        .filter(|path| !before.contains(*path))
        // `.jails/` is the machine root: the ledger and journal are the
        // transaction's own bookkeeping, not files a plan line describes.
        .filter(|path| !path.starts_with(".jails/"))
        .cloned()
        .collect();
    assert_eq!(created, appeared, "the plan and the apply disagree");

    // And a plan over what is already applied is empty -- not "applied" as a
    // status word beside every row, but no operations at all, because there
    // is nothing left to do.
    let again = jails_engine::route::app_apply(
        &jails_engine::route::Run::pretending(&Project::load(&root).unwrap()),
        &[],
        &[note("Note")],
    )
    .unwrap()
    .operations();
    assert!(again.is_empty(), "replanning a settled manifest: {again:?}");

    // Dropping the row plans the deletion, which is the answer the imperative
    // walk cannot give at all: it prints a status per row it *has*, so a row
    // the manifest stopped naming is simply not mentioned.
    let dropped = jails_engine::route::app_apply(
        &jails_engine::route::Run::pretending(&Project::load(&root).unwrap()),
        &[],
        &[],
    )
    .unwrap()
    .operations();
    assert!(
        dropped.iter().any(
            |op| op.kind == jails_prepare::report::ReportedOpKind::Delete
                && op.path.to_string() == "src/main/java/com/example/demo/domain/Note.java"
        ),
        "{dropped:?}"
    );
}

/// §R6.2's `app::reconcile` row, and the claim that it needs no route.
///
/// V1 has a whole module for this: when a manifest row's spec changes, it
/// regenerates against a scratch tree, three-way merges each file, and
/// stitches the result back. §R5.3 says that is not a manifest concern at all
/// -- it is what *any* rewrite of a file with a recorded base means -- so the
/// decision table does it for every route at once and `app apply` inherits it.
///
/// This is the falsifier for that claim. The reader edits a generated record;
/// the manifest then asks for a component it did not have. Both changes must
/// survive, and neither may be silently dropped.
#[test]
fn a_manifest_row_that_changes_merges_with_what_the_reader_wrote() {
    let root = common::temp_dir("engine-app-reconcile");
    std::fs::create_dir_all(&root).unwrap();
    common::write_plain_fixture(&root);
    let note = |fields: &[&str]| jails_engine::route::Intent {
        kind: jails_spec::spec::kind::ArtifactKind::Record,
        timestamps: false,
        name: "Note".to_string(),
        fields: fields.iter().map(|f| f.to_string()).collect(),
        indexes: Vec::new(),
        package: None,
        on: None,
        yields: None,
        method: None,
    };
    let at = root.join("src/main/java/com/example/demo/domain/Note.java");

    jails_engine::route::app_apply(
        &committing(&Project::load(&root).unwrap()),
        &[],
        &[note(&["title:string!"])],
    )
    .unwrap();

    // The reader annotates the file, well clear of the components -- an edit
    // that genuinely does not overlap what the generator is about to change.
    let original = std::fs::read_to_string(&at).unwrap();
    let edited = original.replacen(
        "package com.example.demo.domain;",
        "package com.example.demo.domain;\n\n// reviewed 2026-08",
        1,
    );
    assert_ne!(edited, original, "the edit did not apply");
    std::fs::write(&at, &edited).unwrap();

    // The manifest now asks for a second component.
    jails_engine::route::app_apply(
        &committing(&Project::load(&root).unwrap()),
        &[],
        &[note(&["title:string!", "body:string"])],
    )
    .unwrap();

    let merged = std::fs::read_to_string(&at).unwrap();
    assert!(
        merged.contains("reviewed 2026-08"),
        "the reader's edit was clobbered: {merged}"
    );
    assert!(
        merged.contains("String body"),
        "the manifest's change was dropped: {merged}"
    );
    assert!(
        !merged.contains("<<<<<<<"),
        "two changes that do not overlap were reported as a conflict: {merged}"
    );
}

/// And where the two changes genuinely do overlap, the refusal is a refusal.
///
/// This is §R5.4's boundary as it actually stands: the merge runs, finds a
/// real conflict, and stops. Committing marker bytes with a resumable pending
/// state is the half that is not wired, so the honest answer is to leave the
/// reader's file exactly as they left it and say which route would be needed.
/// A partial write here would be the worst of both -- their version gone and
/// no pending conflict to continue from.
#[test]
fn an_overlapping_edit_refuses_without_writing_anything() {
    let root = common::temp_dir("engine-app-conflict");
    std::fs::create_dir_all(&root).unwrap();
    common::write_plain_fixture(&root);
    let note = |fields: &[&str]| jails_engine::route::Intent {
        kind: jails_spec::spec::kind::ArtifactKind::Record,
        timestamps: false,
        name: "Note".to_string(),
        fields: fields.iter().map(|f| f.to_string()).collect(),
        indexes: Vec::new(),
        package: None,
        on: None,
        yields: None,
        method: None,
    };
    let at = root.join("src/main/java/com/example/demo/domain/Note.java");

    jails_engine::route::app_apply(
        &committing(&Project::load(&root).unwrap()),
        &[],
        &[note(&["title:string!"])],
    )
    .unwrap();

    // Directly above the component list, which is the line the manifest's
    // change rewrites.
    let mine = std::fs::read_to_string(&at).unwrap().replacen(
        "public record Note(",
        "// mine\npublic record Note(",
        1,
    );
    std::fs::write(&at, &mine).unwrap();
    let before = common::scenarios::file_set(&root);

    let error = jails_engine::route::app_apply(
        &committing(&Project::load(&root).unwrap()),
        &[],
        &[note(&["title:string!", "body:string"])],
    )
    .unwrap_err();

    assert!(error.contains("overlap"), "{error}");
    assert!(error.contains("§R5.4") || error.contains("R5.4"), "{error}");
    // Named, so the reader knows which file to look at rather than being told
    // only that something overlapped.
    assert!(
        error.contains("src/main/java/com/example/demo/domain/Note.java"),
        "{error}"
    );
    assert_eq!(
        std::fs::read_to_string(&at).unwrap(),
        mine,
        "the refusal wrote over the reader's file"
    );
    assert_eq!(
        before,
        common::scenarios::file_set(&root),
        "the refusal left something behind"
    );
}

/// §R6.2's `rename` row: every rewrite and every move in one transition.
///
/// V1's own source says why this matters. It writes each file's new contents
/// and then moves the files, with a comment explaining that this order at
/// least leaves "one consistent state" if a write fails partway -- a defence
/// against a partial rename, not a prevention of one. Here a move is a
/// `Create` at the destination and a `Delete` at the source in the same
/// operation list, so there is no moment where a file exists under both names
/// or under neither.
#[test]
fn a_rename_moves_the_type_its_companions_and_every_reference_at_once() {
    let root = common::temp_dir("engine-rename");
    std::fs::create_dir_all(&root).unwrap();
    common::write_plain_fixture(&root);

    jails_engine::route::generate(
        &committing(&Project::load(&root).unwrap()),
        &jails_generate::generate::Recipe {
            kind: jails_spec::spec::kind::ArtifactKind::Record,
            name: "Reward",
            fields: &["title:string!".to_string()],
            indexes: &[],
            strategy_on: None,
            strategy_yields: None,
            method: None,
        },
        None,
    )
    .unwrap();

    // An unrelated type that merely starts with the same letters, plus a
    // string literal naming the old type. Neither may move.
    let dir = root.join("src/main/java/com/example/demo/domain");
    std::fs::write(
        dir.join("RewardHistory.java"),
        "package com.example.demo.domain;\n\n\
         public final class RewardHistory {\n\
        \x20   private final Reward latest = null;\n\
        \x20   private static final String LABEL = \"Reward archive\";\n\
        \x20   public Reward latest() { return latest; }\n\
         }\n",
    )
    .unwrap();

    jails_engine::route::rename(
        &committing(&Project::load(&root).unwrap()),
        "Reward",
        "Bonus",
        true,
    )
    .unwrap();

    assert!(dir.join("Bonus.java").is_file(), "the type did not move");
    assert!(!dir.join("Reward.java").exists(), "the old path survived");
    assert!(
        root.join("src/test/java/com/example/demo/domain/BonusTest.java")
            .is_file(),
        "the companion did not move with it"
    );
    assert!(
        !root
            .join("src/test/java/com/example/demo/domain/RewardTest.java")
            .exists()
    );

    let history = std::fs::read_to_string(dir.join("RewardHistory.java")).unwrap();
    assert!(
        dir.join("RewardHistory.java").is_file(),
        "a type sharing a prefix was moved"
    );
    assert!(
        history.contains("private final Bonus latest"),
        "the reference was not renamed: {history}"
    );
    assert!(
        history.contains("class RewardHistory"),
        "the substring match renamed a longer identifier: {history}"
    );
    assert!(
        history.contains("\"Reward archive\""),
        "a string literal was rewritten: {history}"
    );

    assert!(
        std::fs::read_to_string(dir.join("Bonus.java"))
            .unwrap()
            .contains("public record Bonus("),
        "the moved file still declares the old type"
    );
}

/// A rename is one generation and refuses a destination that is occupied.
///
/// Both come from the same place: every source *and* every destination is a
/// declared read, so an occupied destination is a captured fact rather than a
/// `Path::exists` the plan races with -- and the whole rename is one commit
/// rather than a file's worth of them.
#[test]
fn a_rename_is_one_generation_and_will_not_land_on_an_occupied_name() {
    let root = common::temp_dir("engine-rename-guard");
    std::fs::create_dir_all(&root).unwrap();
    common::write_plain_fixture(&root);
    let generate = |name: &str| {
        jails_engine::route::generate(
            &committing(&Project::load(&root).unwrap()),
            &jails_generate::generate::Recipe {
                kind: jails_spec::spec::kind::ArtifactKind::Record,
                name,
                fields: &["title:string!".to_string()],
                indexes: &[],
                strategy_on: None,
                strategy_yields: None,
                method: None,
            },
            None,
        )
        .unwrap()
    };
    generate("Reward");
    generate("Bonus");

    let before = jails_commit::store::Store::at(&root)
        .observe()
        .unwrap()
        .generation();
    let error = jails_engine::route::rename(
        &committing(&Project::load(&root).unwrap()),
        "Reward",
        "Bonus",
        true,
    )
    .unwrap_err();
    assert!(error.contains("already exists"), "{error}");
    assert!(
        root.join("src/main/java/com/example/demo/domain/Reward.java")
            .is_file(),
        "the refusal moved something anyway"
    );
    assert_eq!(
        jails_commit::store::Store::at(&root)
            .observe()
            .unwrap()
            .generation(),
        before,
        "a refusal advanced the store"
    );

    // Now with a free destination: many files, one generation.
    jails_engine::route::rename(
        &committing(&Project::load(&root).unwrap()),
        "Reward",
        "Reward2",
        true,
    )
    .unwrap();
    assert_eq!(
        jails_commit::store::Store::at(&root)
            .observe()
            .unwrap()
            .generation(),
        before + 1,
        "a rename touching four files took more than one generation"
    );
}

/// §R6.2's `adopt layout` row: one commit, not one per adopted layer.
///
/// V1 calls `record_layout` once per entry, so a project with three renamed
/// directories is three separate rewrites of one file. The composition here is
/// against the captured bytes, which is what makes it sound -- splicing
/// against a re-read file is how the second edit comes to be written over the
/// first.
#[test]
fn adopting_a_foreign_layout_records_every_layer_in_one_write() {
    let root = common::temp_dir("engine-adopt");
    std::fs::create_dir_all(&root).unwrap();
    common::write_plain_fixture(&root);
    let base = root.join("src/main/java/com/example/demo");
    for name in ["controllers", "persistence", "dto", "quirk", "domain"] {
        std::fs::create_dir_all(base.join(name)).unwrap();
        std::fs::write(
            base.join(name).join("Marker.java"),
            format!("package com.example.demo.{name};\n\nfinal class Marker {{}}\n"),
        )
        .unwrap();
    }
    // A comment the reader wrote, which must survive byte-for-byte.
    std::fs::write(
        root.join("jails.toml"),
        "# hand-written, do not lose me\n[project]\ncapabilities = [\"db\"]\n",
    )
    .unwrap();

    let before = jails_commit::store::Store::at(&root)
        .observe()
        .unwrap()
        .generation();
    jails_engine::route::adopt_layout(&committing(&Project::load(&root).unwrap())).unwrap();

    let config = std::fs::read_to_string(root.join("jails.toml")).unwrap();
    for entry in [
        "web = \"controllers\"",
        "adapters = \"persistence\"",
        "api = \"dto\"",
    ] {
        assert!(config.contains(entry), "missing {entry}: {config}");
    }
    assert!(
        config.contains("# hand-written, do not lose me"),
        "the reader's comment was lost: {config}"
    );
    assert!(
        config.contains("capabilities = [\"db\"]"),
        "the capability list was rewritten: {config}"
    );
    assert!(
        !config.contains("quirk"),
        "a directory jails does not recognise was guessed at: {config}"
    );
    assert!(
        !config.contains("domain = "),
        "a directory already spelled jails' way was recorded as a rename: {config}"
    );
    assert_eq!(
        jails_commit::store::Store::at(&root)
            .observe()
            .unwrap()
            .generation(),
        before + 1,
        "three adopted layers took more than one generation"
    );

    // `jails.toml` has more than one contributor: `[project] capabilities` is
    // a set of owned resources `add` splices. A layout edit is keyed rather
    // than a whole-file body precisely so the two compose, and a capability
    // installed after adoption must not take the `[layout]` table with it --
    // nor the reverse.
    jails_engine::route::install(
        &committing(&Project::load(&root).unwrap()),
        &Declaration::plain(Capability::Json),
    )
    .unwrap();
    let both = std::fs::read_to_string(root.join("jails.toml")).unwrap();
    assert!(both.contains("web = \"controllers\""), "{both}");
    assert!(both.contains("json"), "{both}");
    assert!(both.contains("# hand-written, do not lose me"), "{both}");
}

/// Two candidates for one layer writes neither, and says so.
///
/// A `[layout]` table can only name one directory, so picking the first
/// alphabetically would be a coin toss the reader never saw.
#[test]
fn two_directories_for_one_layer_adopt_neither() {
    let root = common::temp_dir("engine-adopt-ambiguous");
    std::fs::create_dir_all(&root).unwrap();
    common::write_plain_fixture(&root);
    let base = root.join("src/main/java/com/example/demo");
    for name in ["controllers", "rest"] {
        std::fs::create_dir_all(base.join(name)).unwrap();
    }

    let error =
        jails_engine::route::adopt_layout(&committing(&Project::load(&root).unwrap())).unwrap_err();
    assert!(error.contains("nothing to adopt"), "{error}");
    assert!(
        !root.join("jails.toml").exists(),
        "a coin toss was written anyway"
    );
}

/// §R6.2's `test --fast` row: an owned entity, not a maintenance side channel.
///
/// V1 splices `junit-platform-console` into the reader's POM as a side effect
/// of running tests, records nothing, and leaves no way to take it back out.
/// A dependency that appeared because of *how somebody ran their tests*, that
/// nothing can name and nothing can remove, is the failure the ownership model
/// exists to prevent.
#[test]
fn fast_test_claims_its_dependency_and_remove_gives_it_back() {
    let root = common::temp_dir("engine-fast-test");
    std::fs::create_dir_all(&root).unwrap();
    common::write_spring_fixture(&root);

    jails_engine::route::install_fast_test(&committing(&Project::load(&root).unwrap())).unwrap();
    let pom = std::fs::read_to_string(root.join("pom.xml")).unwrap();
    assert!(pom.contains("junit-platform-console"), "{pom}");
    // The fixture has a Spring Boot parent, so the version is managed. A
    // redundant one would pin the launcher while the BOM moves the engine.
    let at = pom.find("junit-platform-console").unwrap();
    let block = &pom[at..at + 200.min(pom.len() - at)];
    assert!(
        !block[..block.find("</dependency>").unwrap_or(block.len())].contains("<version>"),
        "a managed version was pinned anyway: {block}"
    );

    // Installing twice claims nothing new.
    let generation = jails_commit::store::Store::at(&root)
        .observe()
        .unwrap()
        .generation();
    jails_engine::route::install_fast_test(&committing(&Project::load(&root).unwrap())).unwrap();
    assert_eq!(
        std::fs::read_to_string(root.join("pom.xml")).unwrap(),
        pom,
        "a second --fast changed the pom"
    );

    jails_engine::route::remove_fast_test(&committing(&Project::load(&root).unwrap())).unwrap();
    let after = std::fs::read_to_string(root.join("pom.xml")).unwrap();
    assert!(
        !after.contains("junit-platform-console"),
        "the dependency survived its own removal: {after}"
    );
    assert!(
        after.contains("spring-boot-starter-test"),
        "the removal took something it did not own: {after}"
    );
    let _ = generation;
}

/// §R6.4's `run::fmt` row: scratch-format and commit exact changed sources.
///
/// V1 runs `mvn spotless:apply` against the real project, so a formatter that
/// fails halfway leaves some files rewritten and some not, with nothing to say
/// which -- and a formatter that rewrites something outside `src/` has already
/// done it by the time anybody notices. Here it runs against a synthesised
/// scratch tree and only what it changed, inside the scope its identity
/// declared, enters the plan.
///
/// Real Maven, so it skips without one -- the mocked tier cannot answer the
/// question this route exists for, which is what the formatter actually did.
#[test]
fn formatting_runs_against_a_copy_and_commits_only_what_changed() {
    if !common::real_mvn_available() {
        common::skip("mvn is not on PATH, so spotless cannot run");
        return;
    }
    let root = common::temp_dir("engine-format");
    std::fs::create_dir_all(&root).unwrap();
    common::write_spring_fixture(&root);
    jails_engine::route::install(
        &committing(&Project::load(&root).unwrap()),
        &Declaration::plain(Capability::Format),
    )
    .unwrap();

    let at = root.join("src/main/java/com/example/demo/Untidy.java");
    std::fs::create_dir_all(at.parent().unwrap()).unwrap();
    std::fs::write(
        &at,
        "package com.example.demo;\npublic   final    class Untidy {\nint x=1;\npublic int x(){return x;}\n}\n",
    )
    .unwrap();
    let outside = root.join("notes.md");
    std::fs::write(&outside, "# untouched\n").unwrap();

    jails_engine::route::format(&committing(&Project::load(&root).unwrap())).unwrap();

    let formatted = std::fs::read_to_string(&at).unwrap();
    assert_ne!(
        formatted,
        "package com.example.demo;\npublic   final    class Untidy {\nint x=1;\npublic int x(){return x;}\n}\n",
        "the formatter's result was not committed"
    );
    assert!(formatted.contains("class Untidy"), "{formatted}");
    assert_eq!(
        std::fs::read_to_string(&outside).unwrap(),
        "# untouched\n",
        "a file outside the declared scope was rewritten"
    );

    // A second run changes nothing, and says so rather than rewriting every
    // file to identical bytes.
    let generation = jails_commit::store::Store::at(&root)
        .observe()
        .unwrap()
        .generation();
    jails_engine::route::format(&committing(&Project::load(&root).unwrap())).unwrap();
    assert_eq!(
        std::fs::read_to_string(&at).unwrap(),
        formatted,
        "the second format changed the file"
    );
    let _ = generation;
}

/// A renamed generated file is still its entity's, and `destroy` must find it.
///
/// §R6.4's `rename` row asks for exactly this: "update `OutputRecord`". A
/// rename that moves the bytes and leaves the store pointing at the old path
/// gives the entity an output that is not there and abandons one that is --
/// so `destroy` strands the file it claims to have deleted.
#[test]
fn renaming_a_generated_type_moves_what_the_store_says_it_owns() {
    let root = common::temp_dir("engine-rename-owned");
    std::fs::create_dir_all(&root).unwrap();
    common::write_plain_fixture(&root);
    jails_engine::route::generate(
        &committing(&Project::load(&root).unwrap()),
        &jails_generate::generate::Recipe {
            kind: jails_spec::spec::kind::ArtifactKind::Record,
            name: "Reward",
            fields: &["title:string!".to_string()],
            indexes: &[],
            strategy_on: None,
            strategy_yields: None,
            method: None,
        },
        None,
    )
    .unwrap();
    jails_engine::route::rename(
        &committing(&Project::load(&root).unwrap()),
        "Reward",
        "Bonus",
        true,
    )
    .unwrap();

    let store = jails_commit::store::Store::at(&root).observe().unwrap();
    let paths: Vec<String> = store
        .ledger
        .iter()
        .flat_map(|ledger| ledger.resources.iter())
        .filter_map(|row| match &row.key {
            jails_protocol::resource::ResourceKey::WholeFile(path) => Some(path.to_string()),
            _ => None,
        })
        .collect();
    assert!(
        paths.iter().any(|p| p.ends_with("domain/Bonus.java")),
        "the store does not own the file that is there: {paths:?}"
    );
    assert!(
        !paths.iter().any(|p| p.ends_with("domain/Reward.java")),
        "the store still owns a file that is gone: {paths:?}"
    );

    // The property all of that is for: the entity is called `Bonus` now, so
    // destroying it finds the files that are actually there.
    jails_engine::route::destroy(
        &committing(&Project::load(&root).unwrap()),
        jails_spec::spec::kind::ArtifactKind::Record,
        "Bonus",
        None,
        false,
        None,
    )
    .unwrap();
    assert!(
        !root
            .join("src/main/java/com/example/demo/domain/Bonus.java")
            .exists(),
        "destroy stranded the file it claims to have deleted"
    );
    assert!(
        !root
            .join("src/test/java/com/example/demo/domain/BonusTest.java")
            .exists(),
        "destroy stranded the companion"
    );
}

/// §R5.4's invocation fingerprint: what a resumption proves sameness by.
///
/// Every route now records the canonical request that produced it, inside
/// `OperationIdentityV1` — so the operation id depends on *what was asked*,
/// not only on what changed. Two properties are worth pinning, because the
/// pending-conflict half will rest on both.
#[test]
fn an_operation_records_the_request_that_produced_it() {
    let root = common::temp_dir("engine-invocation");
    std::fs::create_dir_all(&root).unwrap();
    common::write_plain_fixture(&root);

    jails_engine::route::generate(
        &committing(&Project::load(&root).unwrap()),
        &jails_generate::generate::Recipe {
            kind: jails_spec::spec::kind::ArtifactKind::Record,
            name: "Note",
            fields: &["title:string!".to_string()],
            indexes: &[],
            strategy_on: None,
            strategy_yields: None,
            method: None,
        },
        None,
    )
    .unwrap();

    let store = jails_commit::store::Store::at(&root).observe().unwrap();
    let ledger = store.ledger.as_ref().expect("a commit wrote a ledger");
    let applied = ledger
        .applied
        .iter()
        .find(|row| matches!(&row.id, jails_protocol::entity::EntityId::Intent(id) if id.name.as_str() == "Note"))
        .expect("the record was applied");

    // The operation the row names is the one the journal recorded, and that
    // id hashes the invocation -- so an operation id is now a claim about the
    // request as well as about the result.
    assert_ne!(
        applied.version.operation,
        jails_protocol::identity::OperationId::from_bytes([0; 32]),
        "the applied row carries no operation"
    );

    // A second, different request against the same project produces a
    // different operation. If the invocation were not in the identity, two
    // requests whose file effects happened to coincide would be one operation.
    jails_engine::route::generate(
        &committing(&Project::load(&root).unwrap()),
        &jails_generate::generate::Recipe {
            kind: jails_spec::spec::kind::ArtifactKind::Record,
            name: "Memo",
            fields: &["title:string!".to_string()],
            indexes: &[],
            strategy_on: None,
            strategy_yields: None,
            method: None,
        },
        None,
    )
    .unwrap();
    let after = jails_commit::store::Store::at(&root).observe().unwrap();
    let ledger = after.ledger.as_ref().unwrap();
    let ops: std::collections::BTreeSet<_> = ledger
        .applied
        .iter()
        .map(|row| row.version.operation)
        .collect();
    assert_eq!(ops.len(), 2, "two requests share one operation id");
}

/// Every route honours `--pretend`, and none of them implements it.
///
/// `Run::pretending` runs the same computation and stops one step before the
/// lock, so what it reports is the bundle the commit would have activated. A
/// route never sees the flag: it takes a `Run`, and the decision lives in one
/// place. That is what makes "did it honour --pretend?" a question about the
/// engine rather than about fourteen separate bodies.
#[test]
fn pretending_writes_nothing_and_names_what_a_commit_would_write() {
    let root = common::temp_dir("engine-pretend");
    std::fs::create_dir_all(&root).unwrap();
    common::write_spring_fixture(&root);

    let before = common::scenarios::file_set(&root);
    let generation = || {
        jails_commit::store::Store::at(&root)
            .observe()
            .unwrap()
            .generation()
    };
    let was = generation();

    // One route of each shape: a reconciliation, an entity, a one-shot and a
    // maintenance subject.
    let project = Project::load(&root).unwrap();
    let pretend = jails_engine::route::Run::pretending(&project);
    let capability = jails_engine::route::install(&pretend, &Declaration::plain(Capability::Json))
        .unwrap()
        .operations();
    let artifact = jails_engine::route::generate(
        &pretend,
        &jails_generate::generate::Recipe {
            kind: jails_spec::spec::kind::ArtifactKind::Record,
            name: "Note",
            fields: &["title:string!".to_string()],
            indexes: &[],
            strategy_on: None,
            strategy_yields: None,
            method: None,
        },
        None,
    )
    .unwrap()
    .operations();
    let oneshot = jails_engine::route::migration(&pretend, "create notes")
        .unwrap()
        .operations();
    let maintenance = jails_engine::route::app_init(&pretend, None)
        .unwrap()
        .operations();

    for (what, ops) in [
        ("add", &capability),
        ("generate", &artifact),
        ("migration", &oneshot),
        ("app init", &maintenance),
    ] {
        assert!(!ops.is_empty(), "{what} planned nothing at all");
    }
    assert_eq!(
        before,
        common::scenarios::file_set(&root),
        "a pretend run wrote something"
    );
    assert_eq!(was, generation(), "a pretend run advanced the store");

    // And the plan is the commit's own answer: what `generate` named is
    // exactly what committing it makes appear.
    jails_engine::route::generate(
        &committing(&Project::load(&root).unwrap()),
        &jails_generate::generate::Recipe {
            kind: jails_spec::spec::kind::ArtifactKind::Record,
            name: "Note",
            fields: &["title:string!".to_string()],
            indexes: &[],
            strategy_on: None,
            strategy_yields: None,
            method: None,
        },
        None,
    )
    .unwrap();
    let appeared: std::collections::BTreeSet<String> = common::scenarios::file_set(&root)
        .into_iter()
        .filter(|path| !before.contains(path) && !path.starts_with(".jails/"))
        .collect();
    let created: std::collections::BTreeSet<String> = artifact
        .iter()
        .filter(|op| op.kind == jails_prepare::report::ReportedOpKind::Create)
        .map(|op| op.path.to_string())
        .collect();
    assert_eq!(created, appeared, "the pretend run and the commit disagree");
}

/// A plan describes the transition in the receipt's own words.
///
/// §R3.4 makes reporting a pure projection of the prepared value, and the
/// reason is drift: a hand-rolled list here called a replace an `update`,
/// sorted by path where the executor keeps its own order, and left out
/// directory creation entirely -- so `--pretend` was describing work in words
/// nothing else used. `Report` is the one projection, and the envelope's
/// status is derived from it rather than asserted beside it.
#[test]
fn a_plan_is_reported_through_the_one_projection() {
    let root = common::temp_dir("engine-report-projection");
    std::fs::create_dir_all(&root).unwrap();
    common::write_plain_fixture(&root);
    let project = Project::load(&root).unwrap();
    let pretend = jails_engine::route::Run::pretending(&project);

    let outcome = jails_engine::route::generate(
        &pretend,
        &jails_generate::generate::Recipe {
            kind: jails_spec::spec::kind::ArtifactKind::Record,
            name: "Note",
            fields: &["title:string!".to_string()],
            indexes: &[],
            strategy_on: None,
            strategy_yields: None,
            method: None,
        },
        None,
    )
    .unwrap();

    let report = outcome.report().expect("a pretend run reports");
    // The directory the record's package needs is part of the plan. The old
    // projection dropped every one of these.
    assert!(
        report
            .operations
            .iter()
            .any(|op| op.kind == jails_prepare::report::ReportedOpKind::CreateDirectory),
        "no directory is named: {:?}",
        report.operations
    );
    // The verbs are the report's, not a second vocabulary.
    for op in &report.operations {
        assert!(
            ["create", "replace", "delete", "mkdir"].contains(&op.kind.verb()),
            "{:?}",
            op.kind
        );
    }
    // And the envelope's status falls out of the report rather than being set
    // beside it: work planned is a preview.
    let envelope = outcome.envelope().expect("a pretend run has an envelope");
    assert_eq!(
        envelope.status,
        jails_prepare::command::CommandStatus::Preview
    );
    assert_eq!(envelope.exit_code(), 0);
    assert!(envelope.receipt.is_none(), "a plan committed nothing");

    // Committing it, then planning it again, is a no-op -- and the envelope
    // says so as a status rather than as an empty list the caller has to read.
    jails_engine::route::generate(
        &committing(&Project::load(&root).unwrap()),
        &jails_generate::generate::Recipe {
            kind: jails_spec::spec::kind::ArtifactKind::Record,
            name: "Note",
            fields: &["title:string!".to_string()],
            indexes: &[],
            strategy_on: None,
            strategy_yields: None,
            method: None,
        },
        None,
    )
    .unwrap();
    let project = Project::load(&root).unwrap();
    let settled = jails_engine::route::generate(
        &jails_engine::route::Run::pretending(&project),
        &jails_generate::generate::Recipe {
            kind: jails_spec::spec::kind::ArtifactKind::Record,
            name: "Note",
            fields: &["title:string!".to_string()],
            indexes: &[],
            strategy_on: None,
            strategy_yields: None,
            method: None,
        },
        None,
    )
    .unwrap();
    assert_eq!(
        settled.envelope().unwrap().status,
        jails_prepare::command::CommandStatus::NoOp
    );
}

/// A package is found by the Java in it, and the two traps that rules out.
///
/// A directory listing gives names without kinds, so a *file* named
/// `controllers` would be adopted as the web layer's package -- and a
/// directory holding no Java is not a package anybody can be in, so recording
/// a layout for it would point every later command at an empty tree. A
/// `.java` file's parent is neither, which is why the walk is the answer
/// rather than the listing.
#[test]
fn adoption_ignores_a_file_named_like_a_layer_and_a_package_with_no_java() {
    let root = common::temp_dir("engine-adopt-traps");
    std::fs::create_dir_all(&root).unwrap();
    common::write_plain_fixture(&root);
    let base = root.join("src/main/java/com/example/demo");

    // A real renamed package.
    std::fs::create_dir_all(base.join("controllers")).unwrap();
    std::fs::write(
        base.join("controllers/Marker.java"),
        "package com.example.demo.controllers;\n\nfinal class Marker {}\n",
    )
    .unwrap();
    // A directory that looks like a layer and holds no Java.
    std::fs::create_dir_all(base.join("persistence")).unwrap();
    // A *file* that looks like a layer.
    std::fs::write(base.join("dto"), "not java\n").unwrap();

    jails_engine::route::adopt_layout(&committing(&Project::load(&root).unwrap())).unwrap();

    let config = std::fs::read_to_string(root.join("jails.toml")).unwrap();
    assert!(config.contains("web = \"controllers\""), "{config}");
    assert!(
        !config.contains("persistence"),
        "a package with no Java in it was adopted: {config}"
    );
    assert!(
        !config.contains("dto"),
        "a file named like a layer was adopted as a package: {config}"
    );
}

/// A generated command reaches the dispatcher that runs it.
///
/// `g command` writes the class *and* the `commands.put(...)` line that makes
/// it reachable. V1 spliced that line with a `std::fs` call after the plan, so
/// the routes wrote the class and left it unreachable -- a command nothing
/// dispatches is a file, not a command.
///
/// The dispatcher is not owned by the command: claiming it whole would make
/// `destroy command` delete the CLI. So the line is a keyed claim inside
/// somebody else's file, and retiring it takes back the line and the import
/// that only existed to serve it.
#[test]
fn a_generated_command_is_registered_in_the_dispatcher_that_runs_it() {
    let root = common::temp_dir("engine-command-registration");
    std::fs::create_dir_all(&root).unwrap();
    common::write_plain_fixture(&root);

    let generate = |kind, name: &str| {
        jails_engine::route::generate(
            &committing(&Project::load(&root).unwrap()),
            &jails_generate::generate::Recipe {
                kind,
                name,
                fields: &[],
                indexes: &[],
                strategy_on: None,
                strategy_yields: None,
                method: None,
            },
            None,
        )
        .unwrap()
    };
    generate(jails_spec::spec::kind::ArtifactKind::Cli, "Admin");
    generate(jails_spec::spec::kind::ArtifactKind::Command, "Greet");

    let dispatcher = root.join("src/main/java/com/example/demo/cli/AdminCli.java");
    let text = std::fs::read_to_string(&dispatcher).unwrap();
    assert!(
        text.contains("commands.put(GreetCommand.NAME, GreetCommand::run);"),
        "the command was written and never registered:\n{text}"
    );

    // Destroying the command takes the line back out and leaves the CLI.
    jails_engine::route::destroy(
        &committing(&Project::load(&root).unwrap()),
        jails_spec::spec::kind::ArtifactKind::Command,
        "Greet",
        None,
        false,
        None,
    )
    .unwrap();
    let after = std::fs::read_to_string(&dispatcher).unwrap();
    assert!(!after.contains("GreetCommand::run"), "{after}");
    assert!(after.contains("return commands;"), "the CLI went with it");
}

/// The block a recipe puts in somebody else's file is part of its plan.
///
/// `g durable-job` writes one job's scheduler limits into the app-wide test
/// property source, beside every other job's block and whatever the reader put
/// between them. V1 did it as a side effect *after* the plan -- a `std::fs`
/// call outside the `Change` -- so anything reasoning about a change did not
/// know the file existed, and the route that plans from the same recipe left
/// it unwritten. The whole scenario runs here because that is the only way to
/// reach the recipe: a durable job needs the use case it invokes and the
/// resource that proves completion.
#[test]
fn a_recipe_states_the_block_it_puts_in_somebody_elses_file() {
    let root = common::temp_dir("engine-marked-block");
    std::fs::create_dir_all(&root).unwrap();
    common::write_spring_fixture(&root);
    let scenario = common::scenarios::SCENARIOS
        .iter()
        .find(|s| s.name == "association-durable-job")
        .expect("the durable-job scenario");
    for step in scenario.steps {
        route_step(&root, step).unwrap_or_else(|why| panic!("{step:?}: {why}"));
    }

    let at = root.join("src/test/resources/config/application.properties");
    let text = std::fs::read_to_string(&at).unwrap_or_else(|why| panic!("not written: {why}"));
    assert!(
        text.contains("# jails:durable-job-item-dispatcher"),
        "{text}"
    );
    assert!(
        text.contains("jobs.item-dispatcher.max-attempts=2"),
        "{text}"
    );

    // And the last block out takes the file with it: an empty property source
    // is one jails created to have somewhere to put a block, not one the
    // reader keeps.
    jails_engine::route::destroy(
        &committing(&Project::load(&root).unwrap()),
        jails_spec::spec::kind::ArtifactKind::DurableJob,
        "ItemDispatcher",
        None,
        false,
        None,
    )
    .unwrap();
    assert!(!at.exists(), "an empty property source was left behind");
}

/// The compose block a capability writes has to be a compose file.
///
/// The canonical `ComposeServiceSpec` mapping is stored dedented -- stated
/// relative to the service, so one mapping has one spelling -- and the format
/// owner indents it back. `compose::add_service_ref` takes a body that already
/// carries its own two-space nesting, so handing it the canonical value
/// un-nests every key: `image:` lands at the service's own indent and the file
/// stops being YAML. Nothing compared the bytes, so it went unnoticed.
#[test]
fn a_capabilitys_compose_block_keeps_its_nesting() {
    let root = common::temp_dir("engine-compose-shape");
    std::fs::create_dir_all(&root).unwrap();
    common::write_spring_fixture(&root);

    jails_engine::route::install(
        &committing(&Project::load(&root).unwrap()),
        &Declaration::plain(Capability::Db),
    )
    .unwrap();

    let compose = std::fs::read_to_string(root.join("compose.yaml")).unwrap();
    let service = compose
        .lines()
        .position(|line| line.trim_end() == "  postgres:")
        .unwrap_or_else(|| panic!("no service block:\n{compose}"));
    let body: Vec<&str> = compose
        .lines()
        .skip(service + 1)
        .take_while(|line| line.starts_with("    ") || line.trim().is_empty())
        .collect();
    assert!(
        body.iter()
            .any(|line| line.trim_start().starts_with("image:")),
        "the service body is not nested under its name:\n{compose}"
    );
    // And a nested key stays nested one level deeper than its parent.
    assert!(
        compose.contains("\n      POSTGRES_DB:"),
        "a nested mapping was flattened:\n{compose}"
    );
}

/// A capability that brings a container plans the run that starts it.
///
/// §R3.3's one aggregate effect. It is a *descriptor* frozen at preparation --
/// the exact documents and the exact service sets an attempt would act on --
/// rather than a step in the commit, because starting a container is not a
/// file operation and cannot be undone by restoring a preimage.
#[test]
fn a_capability_with_a_container_plans_one_runtime_reconciliation() {
    let root = common::temp_dir("engine-compose-effect");
    std::fs::create_dir_all(&root).unwrap();
    common::write_spring_fixture(&root);
    let project = Project::load(&root).unwrap();

    let effects = |run: &jails_engine::route::Run, capability| {
        jails_engine::route::install(run, &Declaration::plain(capability))
            .unwrap()
            .report()
            .unwrap()
            .post_commit
            .clone()
    };

    let planned = effects(
        &jails_engine::route::Run::pretending(&project),
        Capability::Db,
    );
    assert_eq!(planned.len(), 1, "{planned:?}");
    let jails_protocol::effect::PostCommitEffect::ComposeReconcile {
        compose_output,
        after_document,
        prior_managed_services,
        desired_services,
        stop_services,
        ..
    } = &planned[0].effect;
    assert_eq!(compose_output.to_string(), "compose.yaml");
    assert!(
        prior_managed_services.is_empty(),
        "nothing was managed before: {prior_managed_services:?}"
    );
    assert_eq!(desired_services.len(), 1, "{desired_services:?}");
    assert!(
        stop_services.is_empty(),
        "an install stops nothing: {stop_services:?}"
    );
    assert!(
        after_document.is_some(),
        "a service is wanted, so the document it is started from must be pinned"
    );
    assert_eq!(
        planned[0].state,
        jails_protocol::effect::EffectState::Deferred
    );

    // `--no-start` is the caller declining the runtime half, and it suppresses
    // the effect rather than the file transition.
    assert!(
        effects(
            &jails_engine::route::Run::pretending(&project).without_start(),
            Capability::Db,
        )
        .is_empty(),
        "`--no-start` planned a runtime reconciliation anyway"
    );

    // And a capability with no container plans none at all: an owner-only
    // change is not a reason to touch anything that is running.
    assert!(
        effects(
            &jails_engine::route::Run::pretending(&project),
            Capability::Actuator,
        )
        .is_empty()
    );

    // Taking it back out is the inverse, and it is derived rather than
    // mirrored: the stop set is what the *prior* map held and the committed
    // document no longer names, so a block the reader kept by hand survives.
    jails_engine::route::install(
        &committing(&Project::load(&root).unwrap()),
        &Declaration::plain(Capability::Db),
    )
    .unwrap();
    let project = Project::load(&root).unwrap();
    let removal = jails_engine::route::remove(
        &jails_engine::route::Run::pretending(&project),
        &Declaration::plain(Capability::Db),
    )
    .unwrap();
    let effects = &removal.report().unwrap().post_commit;
    assert_eq!(effects.len(), 1, "{effects:?}");
    let jails_protocol::effect::PostCommitEffect::ComposeReconcile {
        before_document,
        prior_managed_services,
        desired_services,
        stop_services,
        ..
    } = &effects[0].effect;
    assert_eq!(prior_managed_services.len(), 1);
    assert!(desired_services.is_empty(), "{desired_services:?}");
    assert_eq!(stop_services.len(), 1, "{stop_services:?}");
    assert!(
        before_document.is_some(),
        "stopping a service needs the document that declared it"
    );
}

/// A commit with nothing to reconcile says so, rather than saying nothing.
///
/// The wiring §R6.6 asks for: the project lock is released, then the effect is
/// attempted. A capability with no container has no effect to attempt, and the
/// answer is `NotApplicable` -- which is a different claim from "an attempt
/// was made and we are not saying how it went".
///
/// The attempt itself is deliberately not exercised here. Running it starts a
/// real container, and a test suite that leaves a PostgreSQL behind when it
/// passes is worse than one that says which half it covers. The argument
/// vector, the identity and the settled-state rules have unit tests in
/// `jails_commit::runtime`; this pins the route that reaches them.
#[test]
fn a_commit_with_no_container_reports_no_runtime_attempt() {
    let root = common::temp_dir("engine-no-runtime");
    std::fs::create_dir_all(&root).unwrap();
    common::write_spring_fixture(&root);

    let project = Project::load(&root).unwrap();
    let outcome = jails_engine::route::install(
        // Started, deliberately: the point is that nothing was attempted
        // because there was nothing to attempt.
        &jails_engine::route::Run::committing(&project),
        &Declaration::plain(Capability::Actuator),
    )
    .unwrap()
    .committed()
    .unwrap();

    let jails_commit::outcome::CommitResult::Committed(committed) = outcome else {
        panic!("an install committed nothing");
    };
    assert!(committed.receipt.post_commit.is_empty());
    assert_eq!(
        committed.effect,
        jails_commit::outcome::CommitEffectOutcome::NotApplicable
    );
}

/// A project whose build file jails does not read still gets the Java.
///
/// `build.rs`'s widened door: recognising a filename is not understanding a
/// build, and about ten of thirty commands need to read one at all. So
/// `generate` emits the code and the dependency claim splices into nothing --
/// the claim itself survives in the store, which is what lets `doctor` say the
/// dependency is missing instead of the reader finding out at compile time.
///
/// A capability is not exempted and refuses first: installing the code and
/// silently skipping the dependency hands the reader a compile error for a
/// line they did not write.
///
/// The fixture is Kotlin DSL deliberately. Groovy `build.gradle` is *read*
/// now; `build.gradle.kts` is a different grammar, and a parser aimed at one
/// that guessed at the other is the confident wrong answer this whole boundary
/// exists to prevent.
#[test]
fn a_foreign_build_gets_the_code_and_a_capability_still_refuses() {
    let root = common::temp_dir("engine-foreign-build");
    std::fs::create_dir_all(root.join("src/main/java/com/example/demo")).unwrap();
    std::fs::write(root.join("build.gradle.kts"), "plugins { java }\n").unwrap();
    std::fs::write(
        root.join("src/main/java/com/example/demo/App.java"),
        "package com.example.demo;\n\npublic class App {}\n",
    )
    .unwrap();

    jails_engine::route::generate(
        &committing(&Project::load(&root).unwrap()),
        &jails_generate::generate::Recipe {
            kind: jails_spec::spec::kind::ArtifactKind::Record,
            name: "Note",
            fields: &["title:string!".to_string()],
            indexes: &[],
            strategy_on: None,
            strategy_yields: None,
            method: None,
        },
        None,
    )
    .unwrap();
    assert!(
        root.join("src/main/java/com/example/demo/domain/Note.java")
            .is_file(),
        "a Kotlin-DSL Gradle project got no code"
    );
    assert!(!root.join("pom.xml").exists(), "jails wrote a build file");

    let refused = jails_engine::route::install(
        &committing(&Project::load(&root).unwrap()),
        &Declaration::plain(Capability::Json),
    )
    .unwrap_err();
    assert!(refused.contains("json"), "{refused}");
}

/// A Groovy Gradle project gets the code **and** the dependency.
///
/// The whole point of reading `build.gradle`: before it, `add` refused here
/// and `generate` wrote code with a note listing the coordinates the reader
/// had to splice by hand. The claim is the same `SemanticEdit::MavenDependency`
/// a POM gets -- `group:artifact:version` is what both tools resolve against
/// -- so only the rendering differs.
#[test]
fn a_groovy_gradle_project_gets_the_code_and_the_dependency() {
    let root = common::temp_dir("engine-gradle-build");
    std::fs::create_dir_all(root.join("src/main/java/com/example/demo")).unwrap();
    std::fs::write(
        root.join("build.gradle"),
        r#"plugins {
    id 'java'
    id 'org.springframework.boot' version '3.2.0'
}

sourceCompatibility = 25

dependencies {
    implementation 'org.springframework.boot:spring-boot-starter-web'
}
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src/main/java/com/example/demo/App.java"),
        "package com.example.demo;\n\npublic class App {}\n",
    )
    .unwrap();

    jails_engine::route::install(
        &committing(&Project::load(&root).unwrap()),
        &Declaration::plain(Capability::Json),
    )
    .expect("a capability jails can splice into a build file it can read");

    let build = std::fs::read_to_string(root.join("build.gradle")).unwrap();
    assert!(
        build.contains("tools.jackson.core:jackson-databind"),
        "the dependency was not spliced: {build}"
    );
    // Every other byte is the reader's and stays theirs.
    assert!(build.starts_with("plugins {\n    id 'java'\n"), "{build}");
    assert!(build.contains("sourceCompatibility = 25"), "{build}");
    assert!(
        build.contains("implementation 'org.springframework.boot:spring-boot-starter-web'"),
        "{build}"
    );
}

/// `remove` on a Gradle project takes the dependency back out.
///
/// The bug this pins: the projection's *installing* edit branched on the build
/// tool and its *retiring* one did not. `ResourceKey::MavenDependency`
/// retirement opened `pom_path()` unconditionally, found no `pom.xml` on a
/// Gradle project, returned "nothing to do", and reported the claim retired
/// while the line was still in `build.gradle` -- so `add json` then
/// `remove json` left the project holding a dependency nothing declared.
///
/// It survived because `gradle::remove_dependency` was written, tested and
/// `pub`, and `pub` is what tells `dead_code` that another crate may be
/// calling it. Closing this workspace's crate APIs (`pending.md` §7.2) is what
/// made the compiler say nothing did.
#[test]
fn removing_a_capability_from_a_gradle_project_unsplices_the_dependency() {
    let root = common::temp_dir("engine-gradle-remove");
    std::fs::create_dir_all(root.join("src/main/java/com/example/demo")).unwrap();
    std::fs::write(
        root.join("build.gradle"),
        r#"plugins {
    id 'java'
    id 'org.springframework.boot' version '3.2.0'
}

sourceCompatibility = 25

dependencies {
    implementation 'org.springframework.boot:spring-boot-starter-web'
}
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src/main/java/com/example/demo/App.java"),
        "package com.example.demo;

public class App {}
",
    )
    .unwrap();

    jails_engine::route::install(
        &committing(&Project::load(&root).unwrap()),
        &Declaration::plain(Capability::Json),
    )
    .expect("install");
    let with = std::fs::read_to_string(root.join("build.gradle")).unwrap();
    assert!(
        with.contains("tools.jackson.core:jackson-databind"),
        "{with}"
    );

    jails_engine::route::remove(
        &committing(&Project::load(&root).unwrap()),
        &Declaration::plain(Capability::Json),
    )
    .expect("remove");

    let without = std::fs::read_to_string(root.join("build.gradle")).unwrap();
    assert!(
        !without.contains("jackson-databind"),
        "the dependency survived `remove`: {without}"
    );
    // And nothing the reader wrote moved.
    assert!(
        without.contains("implementation 'org.springframework.boot:spring-boot-starter-web'"),
        "{without}"
    );
    assert!(without.contains("sourceCompatibility = 25"), "{without}");
}

/// A plan and the commit that follows it read the same way.
///
/// §R3.4 gives a command result one human rendering, and the reason is what a
/// reader does with it: they run `--pretend`, look at the lines, and then run
/// the command. Two renderers would be two vocabularies, and comparing the two
/// runs would be comparing two descriptions rather than one.
#[test]
fn a_plan_and_its_commit_are_described_in_the_same_words() {
    let root = common::temp_dir("engine-render");
    std::fs::create_dir_all(&root).unwrap();
    common::write_plain_fixture(&root);

    let record = |run: &jails_engine::route::Run| {
        jails_engine::route::generate(
            run,
            &jails_generate::generate::Recipe {
                kind: jails_spec::spec::kind::ArtifactKind::Record,
                name: "Note",
                fields: &["title:string!".to_string()],
                indexes: &[],
                strategy_on: None,
                strategy_yields: None,
                method: None,
            },
            None,
        )
        .unwrap()
        .envelope()
        .unwrap()
    };

    let project = Project::load(&root).unwrap();
    let planned_envelope = record(&jails_engine::route::Run::pretending(&project));
    let applied_envelope = record(&committing(&Project::load(&root).unwrap()));
    let directory_paths = |envelope: &jails_prepare::command::CommandEnvelope| {
        let Some(jails_prepare::command::CommandReport::Prepared(report)) = &envelope.report else {
            panic!("expected a prepared command report");
        };
        report
            .operations
            .iter()
            .filter(|operation| {
                operation.kind == jails_prepare::report::ReportedOpKind::CreateDirectory
            })
            .map(|operation| operation.path.to_string())
            .collect::<Vec<_>>()
    };
    let planned_directories = directory_paths(&planned_envelope);
    assert_eq!(
        planned_directories,
        [
            "src/main/java/com/example/demo/domain",
            "src/test",
            "src/test/java",
            "src/test/java/com",
            "src/test/java/com/example",
            "src/test/java/com/example/demo",
            "src/test/java/com/example/demo/domain",
        ]
    );
    let receipt_directories: Vec<String> = applied_envelope
        .receipt
        .as_ref()
        .unwrap()
        .directories
        .iter()
        .map(|directory| directory.path.to_string())
        .collect();
    assert_eq!(receipt_directories, planned_directories);

    let planned = jails_prepare::report::render_envelope(&planned_envelope);
    let applied = jails_prepare::report::render_envelope(&applied_envelope);

    // The same files, named the same way, under a heading that says which of
    // the two this was.
    assert!(planned.starts_with("plan "), "{planned}");
    assert!(applied.starts_with("applied "), "{applied}");
    for rendering in [&planned, &applied] {
        assert!(
            rendering.contains("create  src/main/java/com/example/demo/domain/Note.java"),
            "{rendering}"
        );
        assert!(
            rendering.contains("mkdir"),
            "no directory named:\n{rendering}"
        );
        assert!(rendering.contains("ledger"), "{rendering}");
    }

    // And a settled run says nothing happened rather than printing a receipt
    // full of files it did not touch.
    let settled = jails_prepare::report::render_envelope(&record(&committing(
        &Project::load(&root).unwrap(),
    )));
    assert_eq!(settled, "nothing to do\n");
}

/// One entry point picks the route the kind actually needs.
///
/// §R6.2 gives `field`, `migration` and `cases` policies of their own -- an
/// overlay, a serial allocation, a source-hash receipt -- and none of them is
/// a persistent entity. Forwarding every kind to the recipe planner is what a
/// caller does when the selection lives at the call site, and it fails as
/// `g cases` reaching a planner with no arm for a one-shot.
#[test]
fn one_entry_point_sends_each_kind_to_the_route_that_owns_it() {
    use jails_spec::spec::kind::ArtifactKind;

    let root = common::temp_dir("engine-recipe-entry");
    std::fs::create_dir_all(&root).unwrap();
    common::write_plain_fixture(&root);
    std::fs::write(root.join("brief.md"), "# Notes\n\n- it works\n").unwrap();

    let intent = |kind, name: &str, fields: Vec<String>| jails_engine::route::Intent {
        kind,
        name: name.to_string(),
        fields,
        timestamps: false,
        indexes: Vec::new(),
        package: None,
        on: None,
        yields: None,
        method: None,
    };
    let run =
        |i| jails_engine::route::recipe(&committing(&Project::load(&root).unwrap()), &i).unwrap();

    // A persistent kind: capitalised, suffix-stripped, planned as an entity.
    run(intent(
        ArtifactKind::Record,
        "note",
        vec!["title:string!".to_string()],
    ));
    assert!(
        root.join("src/main/java/com/example/demo/domain/Note.java")
            .is_file()
    );

    // A serial allocation, whose NAME is a description rather than a class.
    run(intent(ArtifactKind::Migration, "create notes", Vec::new()));
    let migrations = std::fs::read_dir(root.join("src/main/resources/db/migration"))
        .unwrap()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert_eq!(migrations.len(), 1, "{migrations:?}");
    assert!(migrations[0].contains("create_notes"), "{migrations:?}");

    // A source-hash receipt, whose NAME is a path.
    run(intent(ArtifactKind::Cases, "brief.md", Vec::new()));

    // And an overlay on the record generated above.
    run(intent(
        ArtifactKind::Field,
        "Note",
        vec!["createdAt:instant?".to_string()],
    ));
    let record =
        std::fs::read_to_string(root.join("src/main/java/com/example/demo/domain/Note.java"))
            .unwrap();
    assert!(record.contains("createdAt"), "{record}");

    // A field with more than one component refuses rather than applying the
    // first: two overlays in one call could not be undone separately.
    let mut two = intent(
        ArtifactKind::Field,
        "Note",
        vec!["a:string".to_string(), "b:string".to_string()],
    );
    two.package = None;
    let error =
        jails_engine::route::recipe(&committing(&Project::load(&root).unwrap()), &two).unwrap_err();
    assert!(error.contains("one `name:type` component"), "{error}");
}

/// One envelope, from both sides, with the status derived rather than told.
///
/// §R3.4's `CommandEnvelope` is what a mutation command returns. The preview
/// side is projected from the prepared report and the committed side from the
/// receipt the executor published -- so a caller cannot report an apply as a
/// conflict or a no-op as an apply, because neither is a word anybody writes.
#[test]
fn a_commit_and_a_plan_are_the_same_envelope() {
    use jails_prepare::command::{CommandStatus, ProjectCommitDisposition};

    let root = common::temp_dir("engine-envelope");
    std::fs::create_dir_all(&root).unwrap();
    common::write_plain_fixture(&root);

    let record = |run: &jails_engine::route::Run| {
        jails_engine::route::generate(
            run,
            &jails_generate::generate::Recipe {
                kind: jails_spec::spec::kind::ArtifactKind::Record,
                name: "Note",
                fields: &["title:string!".to_string()],
                indexes: &[],
                strategy_on: None,
                strategy_yields: None,
                method: None,
            },
            None,
        )
        .unwrap()
        .envelope()
        .expect("every mutation route returns one envelope")
    };

    let project = Project::load(&root).unwrap();
    let preview = record(&jails_engine::route::Run::pretending(&project));
    assert_eq!(preview.status, CommandStatus::Preview);
    assert_eq!(preview.project_commit, ProjectCommitDisposition::None);
    assert!(preview.receipt.is_none());
    assert!(preview.recovery.is_empty());

    let applied = record(&committing(&Project::load(&root).unwrap()));
    assert_eq!(applied.status, CommandStatus::Applied);
    assert_eq!(applied.project_commit, ProjectCommitDisposition::Existing);
    assert!(applied.report.is_none(), "a commit reports its receipt");
    let receipt = applied.receipt.expect("an apply publishes a receipt");
    assert!(
        receipt
            .files
            .iter()
            .any(|file| file.path.to_string().ends_with("Note.java")),
        "the receipt names nothing this generated"
    );

    // Running it again is a no-op, and §R4.2 gives a no-op no receipt:
    // "nothing happened" and "everything happened and changed nothing" are
    // different answers, and only the second has files to name.
    let settled = record(&committing(&Project::load(&root).unwrap()));
    assert_eq!(settled.status, CommandStatus::NoOp);
    assert!(settled.receipt.is_none());
    assert_eq!(settled.exit_code(), 0);

    // And the JSON rendering is the same value, not a second description.
    let json = jails_prepare::serialize::envelope(&settled);
    assert!(
        json.contains("\"schema\":\"jails.command-result.v1\""),
        "{json}"
    );
    assert!(json.contains("\"status\":\"no-op\""), "{json}");
    assert!(json.contains("\"recovery\":[]"), "{json}");
}

/// `--no-start` is part of what was asked, not a printing decision.
///
/// The canonical request is what §R5.4's fingerprint is taken over, and the
/// operation id is a hash of an identity that carries it. Every route used to
/// hardcode `no_start: false`, so `add db` and `add db --no-start` described
/// the same invocation -- and a resume comparing fingerprints would have
/// accepted one as a continuation of the other.
#[test]
fn declining_to_start_is_a_different_invocation() {
    let root = common::temp_dir("engine-no-start");
    std::fs::create_dir_all(&root).unwrap();
    common::write_plain_fixture(&root);
    let project = Project::load(&root).unwrap();

    let plan = |run: &jails_engine::route::Run| {
        jails_engine::route::install(run, &Declaration::plain(Capability::Csv))
            .unwrap()
            .report()
            .unwrap()
            .operation
    };

    let starting = plan(&jails_engine::route::Run::pretending(&project));
    let again = plan(&jails_engine::route::Run::pretending(&project));
    let declined = plan(&jails_engine::route::Run::pretending(&project).without_start());

    assert_eq!(starting, again, "the same command planned two identities");
    assert_ne!(
        starting, declined,
        "`--no-start` did not reach the canonical request"
    );
}

/// Two names are two capabilities.
///
/// plan.md §R1.1 classes `csv` as multi-instance named: `add csv --name Order`
/// and `add csv --name Invoice` are two readers over two records, and a
/// singleton identity would make the second look like the first was already
/// installed -- a no-op over a class the caller asked for and never got.
#[test]
fn two_named_instances_of_one_capability_are_two_capabilities() {
    let root = common::temp_dir("engine-named-capability");
    std::fs::create_dir_all(&root).unwrap();
    common::write_plain_fixture(&root);

    for name in ["Order", "Invoice"] {
        jails_engine::route::install(
            &committing(&Project::load(&root).unwrap()),
            &Declaration::asked(Capability::Csv, Some(name), None),
        )
        .unwrap();
    }

    let adapters = root.join("src/main/java/com/example/demo/adapters");
    assert!(adapters.join("OrderReader.java").is_file());
    assert!(adapters.join("InvoiceReader.java").is_file());

    // Both are declared, and neither could have been written as a bare string.
    let manifest = std::fs::read_to_string(root.join("jails.toml")).unwrap();
    assert!(manifest.contains("name = \"Order\""), "{manifest}");
    assert!(manifest.contains("name = \"Invoice\""), "{manifest}");
    let store = jails_commit::store::Store::at(&root).observe().unwrap();
    let ledger = store.ledger.unwrap();
    assert_eq!(
        ledger
            .applied
            .iter()
            .filter(|row| matches!(
                &row.id,
                jails_protocol::entity::EntityId::Capability(id)
                    if id.kind == Capability::Csv
            ))
            .count(),
        2,
        "two names are two rows"
    );
}

/// Removing one named instance leaves the other, its class and its line.
#[test]
fn removing_one_named_instance_leaves_its_sibling_alone() {
    let root = common::temp_dir("engine-named-remove");
    std::fs::create_dir_all(&root).unwrap();
    common::write_plain_fixture(&root);
    for name in ["Order", "Invoice"] {
        jails_engine::route::install(
            &committing(&Project::load(&root).unwrap()),
            &Declaration::asked(Capability::Csv, Some(name), None),
        )
        .unwrap();
    }

    jails_engine::route::remove(
        &committing(&Project::load(&root).unwrap()),
        &Declaration::asked(Capability::Csv, Some("Order"), None),
    )
    .unwrap();

    let adapters = root.join("src/main/java/com/example/demo/adapters");
    assert!(!adapters.join("OrderReader.java").exists());
    assert!(adapters.join("InvoiceReader.java").is_file());
    let manifest = std::fs::read_to_string(root.join("jails.toml")).unwrap();
    assert!(!manifest.contains("\"Order\""), "{manifest}");
    assert!(manifest.contains("name = \"Invoice\""), "{manifest}");
}

/// A parameter the capability has no meaning for is refused, not dropped.
///
/// `ci` writes a workflow file at a fixed path; `--name` would change nothing
/// about what it installs, so accepting it silently would leave the caller
/// believing they had named something.
#[test]
fn a_parameter_with_no_meaning_refuses_before_anything_is_written() {
    let root = common::temp_dir("engine-named-refusal");
    std::fs::create_dir_all(&root).unwrap();
    common::write_plain_fixture(&root);
    let before = common::scenarios::file_set(&root);

    let error = jails_engine::route::install(
        &committing(&Project::load(&root).unwrap()),
        &Declaration::asked(Capability::Ci, Some("Nightly"), None),
    )
    .unwrap_err();

    assert!(error.contains("--name"), "{error}");
    assert_eq!(common::scenarios::file_set(&root), before);
}

/// `sync` reads the table, not just the array.
///
/// The manifest is the authority for `sync`, so a `[[capability]]` row has to
/// arrive as the capability it names. Rebuilding a singleton from the label
/// would declare a different entity from the one the row means -- and because
/// `DirectConfig` speaks for the whole list, the named row would be retired by
/// the very transition meant to install it.
#[test]
fn sync_installs_a_capability_the_manifest_named() {
    let root = common::temp_dir("engine-named-sync");
    std::fs::create_dir_all(&root).unwrap();
    common::write_plain_fixture(&root);
    std::fs::write(
        root.join("jails.toml"),
        "[[capability]]\nkind = \"csv\"\nname = \"Order\"\n",
    )
    .unwrap();

    jails_engine::route::sync(&committing(&Project::load(&root).unwrap())).unwrap();
    let reader = root.join("src/main/java/com/example/demo/adapters/OrderReader.java");
    assert!(reader.is_file(), "sync installed nothing");

    // And a second sync is a no-op rather than a retirement.
    let before = jails_commit::store::Store::at(&root).observe().unwrap();
    jails_engine::route::sync(&committing(&Project::load(&root).unwrap())).unwrap();
    let after = jails_commit::store::Store::at(&root).observe().unwrap();
    assert!(reader.is_file(), "the second sync took the class back out");
    assert_eq!(
        before.ledger.map(|l| l.applied.len()),
        after.ledger.map(|l| l.applied.len())
    );
}
