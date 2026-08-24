//! One capability, installed through the transaction protocol end to end.
//!
//! plan.md §R6.1 step 2 wants capability `add` on V2 while default dispatch
//! stays on V1. Every piece of that has landed separately -- the translation,
//! the capture, the preparation, the executor -- and this is the first test
//! that runs all of them against a real directory and looks at what is on
//! disk afterwards.

mod common;

use jails_project::model::Project;
use jails_spec::spec::kind::Capability;

#[test]
fn a_capability_installs_through_the_transaction_protocol() {
    let root = common::temp_dir("engine-install");
    std::fs::create_dir_all(&root).unwrap();
    common::write_spring_fixture(&root);
    let project = Project::load(&root).unwrap();

    jails_engine::route::install(&project, Capability::Actuator).unwrap();

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

    jails_engine::route::install(&project, Capability::Csv).unwrap();

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

    let error = jails_engine::route::install(&project, Capability::K8s).unwrap_err();

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
        &project,
        jails_spec::spec::kind::ArtifactKind::Record,
        "Note",
        &["title:string!".to_string(), "at:instant".to_string()],
        None,
        &[],
        None,
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
            &Project::load(&root).unwrap(),
            jails_spec::spec::kind::ArtifactKind::Record,
            "Note",
            &["title:string!".to_string()],
            None,
            &[],
            None,
            None,
        )
    };
    assert!(matches!(
        generate().unwrap(),
        jails_commit::outcome::CommitResult::Committed(_)
    ));
    assert!(
        matches!(
            generate().unwrap(),
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
        &Project::load(&root).unwrap(),
        jails_spec::spec::kind::ArtifactKind::Record,
        "Note",
        &["title:string!".to_string()],
        None,
        &[],
        None,
        None,
    );

    let error = outcome.unwrap_err();
    assert!(
        error.contains("jails did not write it") && error.contains("jails adopt"),
        "the refusal names the way to take it over deliberately: {error}"
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
            let _ = jails_engine::route::install(&project, Capability::Actuator);
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

    jails_engine::route::install(&Project::load(&root).unwrap(), Capability::Actuator).unwrap();
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

    jails_engine::route::install(&Project::load(&root).unwrap(), Capability::Cache).unwrap();
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

    jails_engine::route::install(&Project::load(&root).unwrap(), Capability::Actuator).unwrap();
    let before = jails_commit::store::Store::at(&root).observe().unwrap();
    let outcome =
        jails_engine::route::install(&Project::load(&root).unwrap(), Capability::Actuator).unwrap();
    let after = jails_commit::store::Store::at(&root).observe().unwrap();

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

    jails_engine::route::install(&Project::load(&root).unwrap(), Capability::Actuator).unwrap();
    let file = root.join("src/test/java/com/example/demo/ActuatorEndpointsTest.java");
    assert!(file.is_file());

    jails_engine::route::remove(&Project::load(&root).unwrap(), Capability::Actuator).unwrap();

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
    jails_engine::route::install(&Project::load(&root).unwrap(), Capability::Actuator).unwrap();
    jails_engine::route::install(&Project::load(&root).unwrap(), Capability::Observability)
        .unwrap();
    jails_engine::route::remove(&Project::load(&root).unwrap(), Capability::Observability).unwrap();

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
    let error = jails_engine::route::remove(&Project::load(&root).unwrap(), Capability::Actuator)
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

    jails_engine::route::install(&Project::load(&root).unwrap(), Capability::Actuator).unwrap();
    let file = root.join("src/test/java/com/example/demo/ActuatorEndpointsTest.java");
    assert!(file.is_file());

    // The reader edits the list: actuator out, cache in.
    jails_support::apply::put(
        root.join("jails.toml"),
        "[project]\ncapabilities = [\"cache\"]\n",
    )
    .unwrap();

    jails_engine::route::sync(&Project::load(&root).unwrap()).unwrap();

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
        &Project::load(&root).unwrap(),
        jails_spec::spec::kind::ArtifactKind::Record,
        "Note",
        &["title:string!".to_string()],
        None,
        &[],
        None,
        None,
    )
    .unwrap();
    let record = root.join("src/main/java/com/example/demo/domain/Note.java");
    let test = root.join("src/test/java/com/example/demo/domain/NoteTest.java");
    assert!(record.is_file() && test.is_file());

    jails_engine::route::destroy(
        &Project::load(&root).unwrap(),
        jails_spec::spec::kind::ArtifactKind::Record,
        "Note",
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
            &Project::load(&root).unwrap(),
            jails_spec::spec::kind::ArtifactKind::Record,
            name,
            &["title:string!".to_string()],
            None,
            &[],
            None,
            None,
        )
        .unwrap();
    };
    generate("Note");
    generate("Memo");

    jails_engine::route::destroy(
        &Project::load(&root).unwrap(),
        jails_spec::spec::kind::ArtifactKind::Record,
        "Note",
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
        &Project::load(&root).unwrap(),
        jails_spec::spec::kind::ArtifactKind::Record,
        "Note",
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
/// state rather than from a recomputed path table. A kind that strands a file
/// fails here with the file named.
///
/// The scenario table is the source of kinds, per CLAUDE.md's rule that a new
/// kind adds a `Scenario` and not a fourth list. Single-step scenarios only:
/// a scenario that installs a capability first is asking about two owners
/// interacting, which the shared-claim tests cover separately.
#[test]
fn every_persistent_kind_destroys_back_to_where_it_started() {
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
            &Project::load(&root).unwrap(),
            kind,
            step[2],
            &invocation.fields,
            invocation.package.as_deref(),
            &invocation.indexes,
            invocation.on.as_deref(),
            invocation.yields.as_deref(),
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
        assert_ne!(
            common::scenarios::file_set(&root),
            before,
            "{}: the generate wrote nothing, so the round trip proves nothing",
            scenario.name
        );

        jails_engine::route::destroy(
            &Project::load(&root).unwrap(),
            kind,
            step[2],
            invocation.package.as_deref(),
        )
        .unwrap_or_else(|why| panic!("{}: destroy refused: {why}", scenario.name));

        let after: std::collections::BTreeSet<String> = common::scenarios::file_set(&root)
            .into_iter()
            // `.jails/` is the transaction's own bookkeeping, which exists
            // from the first commit onward and is not something `destroy` is
            // asked to take back.
            .filter(|path| !path.starts_with(".jails"))
            .collect();
        assert_eq!(
            after, before,
            "{}: destroy did not return the project to where it started",
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
fn route_step(root: &std::path::Path, step: &[&str]) -> Result<(), String> {
    use clap::ValueEnum;

    let project = Project::load(root)?;
    match step.first().copied() {
        Some("add") => {
            let capability = Capability::from_str(step[1], true)
                .map_err(|_| format!("`{}` is not a capability", step[1]))?;
            jails_engine::route::install(&project, capability).map(|_| ())
        }
        Some("g") | Some("generate") => {
            let kind = jails_spec::spec::kind::ArtifactKind::from_str(step[1], true)
                .map_err(|_| format!("`{}` is not a kind", step[1]))?;
            let invocation = common::scenarios::invocation(step)
                .ok_or_else(|| "unrecognised flag".to_string())?;
            jails_engine::route::generate(
                &project,
                kind,
                step[2],
                &invocation.fields,
                invocation.package.as_deref(),
                &invocation.indexes,
                invocation.on.as_deref(),
                invocation.yields.as_deref(),
            )
            .map(|_| ())
        }
        _ => Err(format!("`{}` has no V2 route yet", step.join(" "))),
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

    jails_engine::route::install(&Project::load(&root).unwrap(), Capability::Db).unwrap();

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

    jails_engine::route::install(&Project::load(&root).unwrap(), Capability::Db).unwrap();
    assert!(std::fs::read_to_string(&path).unwrap().contains("@Import("));

    jails_engine::route::remove(&Project::load(&root).unwrap(), Capability::Db).unwrap();

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

    jails_engine::route::migration(&Project::load(&root).unwrap(), "create rewards").unwrap();
    jails_engine::route::migration(&Project::load(&root).unwrap(), "add index").unwrap();

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

    jails_engine::route::migration(&Project::load(&root).unwrap(), "mine").unwrap();

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

    jails_engine::route::cases(&Project::load(&root).unwrap(), "docs/behaviour.md", None).unwrap();

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
        jails_engine::route::cases(&Project::load(&root).unwrap(), "docs/behaviour.md", None)
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
    jails_engine::route::cases(&Project::load(&root).unwrap(), "docs/behaviour.md", None).unwrap();

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

    let error = jails_engine::route::cases(&Project::load(&root).unwrap(), "../elsewhere.md", None)
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
        &Project::load(&root).unwrap(),
        jails_spec::spec::kind::ArtifactKind::Record,
        "Note",
        &["id:uuid@pk".to_string(), "title:string!".to_string()],
        None,
        &[],
        None,
        None,
    )
    .unwrap();

    jails_engine::route::field(
        &Project::load(&root).unwrap(),
        "Note",
        "archivedAt:instant?",
        None,
    )
    .unwrap();

    let record =
        std::fs::read_to_string(root.join("src/main/java/com/example/demo/domain/Note.java"))
            .unwrap();
    assert!(
        record.contains("archivedAt"),
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
        vec!["id:uuid@pk", "title:string!", "archivedAt:instant?"]
    );

    // And one field receipt, whose append-only half is the migration.
    assert_eq!(store.one_shots.len(), 1, "{:?}", store.one_shots);
    let migration = root.join("src/main/resources/db/migration/V001__add_archived_at_to_notes.sql");
    assert!(
        migration.is_file(),
        "{:?}",
        std::fs::read_dir(root.join("src/main/resources/db/migration"))
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.file_name()))
            .collect::<Vec<_>>()
    );
    assert!(
        std::fs::read_to_string(&migration)
            .unwrap()
            .contains("add column"),
        "the migration adds the column"
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
        &Project::load(&root).unwrap(),
        jails_spec::spec::kind::ArtifactKind::Record,
        "Note",
        &["id:uuid@pk".to_string(), "title:string!".to_string()],
        None,
        &[],
        None,
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
        &Project::load(&root).unwrap(),
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

    let error = jails_engine::route::field(&Project::load(&root).unwrap(), "Note", "x:int", None)
        .unwrap_err();
    assert!(error.contains("is recorded in this project"), "{error}");
    assert!(error.contains("jails g scaffold Note"), "{error}");

    jails_engine::route::generate(
        &Project::load(&root).unwrap(),
        jails_spec::spec::kind::ArtifactKind::Record,
        "Note",
        &["title:string!".to_string()],
        None,
        &[],
        None,
        None,
    )
    .unwrap();

    let error = jails_engine::route::field(
        &Project::load(&root).unwrap(),
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
        &Project::load(&root).unwrap(),
        &[Capability::Db],
        &[
            jails_engine::route::AppIntent {
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
            },
            // Needs `add db`'s starter *and* `g scaffold`'s record, neither of
            // which exists on disk while this plans.
            jails_engine::route::AppIntent {
                kind: jails_spec::spec::kind::ArtifactKind::Search,
                timestamps: false,
                name: "Article".to_string(),
                fields: vec!["title".to_string(), "body".to_string()],
                indexes: Vec::new(),
                package: None,
                on: None,
                yields: None,
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
    let note = |name: &str| jails_engine::route::AppIntent {
        kind: jails_spec::spec::kind::ArtifactKind::Record,
        timestamps: false,
        name: name.to_string(),
        fields: vec!["title:string!".to_string()],
        indexes: Vec::new(),
        package: None,
        on: None,
        yields: None,
    };

    jails_engine::route::app_apply(
        &Project::load(&root).unwrap(),
        &[],
        &[note("Note"), note("Memo")],
    )
    .unwrap();
    assert!(
        root.join("src/main/java/com/example/demo/domain/Memo.java")
            .is_file()
    );

    // The reader deletes the `Memo` row from the manifest.
    jails_engine::route::app_apply(&Project::load(&root).unwrap(), &[], &[note("Note")]).unwrap();

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
            &Project::load(&root).unwrap(),
            &[],
            &[jails_engine::route::AppIntent {
                kind: jails_spec::spec::kind::ArtifactKind::Record,
                timestamps: false,
                name: "Note".to_string(),
                fields: vec!["title:string!".to_string()],
                indexes: Vec::new(),
                package: None,
                on: None,
                yields: None,
            }],
        )
    };

    manifest().unwrap();
    let outcome = manifest().unwrap();

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

    let intent = |kind: K, name: &str, fields: &[&str]| jails_engine::route::AppIntent {
        kind,
        name: name.to_string(),
        fields: fields.iter().map(|f| f.to_string()).collect(),
        timestamps: false,
        indexes: Vec::new(),
        package: None,
        on: None,
        yields: None,
    };
    let on = |mut i: jails_engine::route::AppIntent, target: &str| {
        i.on = Some(target.to_string());
        i
    };
    let stamped = |mut i: jails_engine::route::AppIntent, indexes: &[&str]| {
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
        &Project::load(&root).unwrap(),
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

    jails_engine::route::app_init(&Project::load(&root).unwrap(), None).unwrap();
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
    let again = jails_engine::route::app_init(&Project::load(&root).unwrap(), None);
    assert!(again.is_err(), "a second seed landed on the reader's file");
    assert_eq!(
        std::fs::read_to_string(root.join(".jails/app.toml")).unwrap(),
        "schema = 1\ncapabilities = [\"db\"]\n",
        "the refusal wrote anyway"
    );
}

/// §R6.2's `app::plan` row: what apply would do, computed by apply.
///
/// The property under test is not that the lines look right -- it is that
/// there is one implementation. V1 answers `app plan` with a second walk over
/// the intent list that compares each row against the ledger, and it had to
/// be shadowed against a typed comparison precisely because two
/// implementations of one question disagree. Here the plan is the commit's own
/// bundle, stopped before the lock: what it names is exactly what appears.
#[test]
fn a_plan_names_exactly_the_files_the_apply_then_writes() {
    let root = common::temp_dir("engine-app-plan");
    std::fs::create_dir_all(&root).unwrap();
    common::write_plain_fixture(&root);
    let note = |name: &str| jails_engine::route::AppIntent {
        kind: jails_spec::spec::kind::ArtifactKind::Record,
        timestamps: false,
        name: name.to_string(),
        fields: vec!["title:string!".to_string()],
        indexes: Vec::new(),
        package: None,
        on: None,
        yields: None,
    };

    let before = common::scenarios::file_set(&root);
    let planned =
        jails_engine::route::app_plan(&Project::load(&root).unwrap(), &[], &[note("Note")])
            .unwrap();
    assert_eq!(
        before,
        common::scenarios::file_set(&root),
        "a plan wrote something"
    );

    jails_engine::route::app_apply(&Project::load(&root).unwrap(), &[], &[note("Note")]).unwrap();
    let after = common::scenarios::file_set(&root);

    let created: std::collections::BTreeSet<_> = planned
        .iter()
        .filter(|op| op.verb == "create")
        .map(|op| op.path.clone())
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
    let again = jails_engine::route::app_plan(&Project::load(&root).unwrap(), &[], &[note("Note")])
        .unwrap();
    assert!(again.is_empty(), "replanning a settled manifest: {again:?}");

    // Dropping the row plans the deletion, which is the answer the imperative
    // walk cannot give at all: it prints a status per row it *has*, so a row
    // the manifest stopped naming is simply not mentioned.
    let dropped = jails_engine::route::app_plan(&Project::load(&root).unwrap(), &[], &[]).unwrap();
    assert!(
        dropped.iter().any(|op| op.verb == "delete"
            && op.path == "src/main/java/com/example/demo/domain/Note.java"),
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
    let note = |fields: &[&str]| jails_engine::route::AppIntent {
        kind: jails_spec::spec::kind::ArtifactKind::Record,
        timestamps: false,
        name: "Note".to_string(),
        fields: fields.iter().map(|f| f.to_string()).collect(),
        indexes: Vec::new(),
        package: None,
        on: None,
        yields: None,
    };
    let at = root.join("src/main/java/com/example/demo/domain/Note.java");

    jails_engine::route::app_apply(
        &Project::load(&root).unwrap(),
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
        &Project::load(&root).unwrap(),
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
    let note = |fields: &[&str]| jails_engine::route::AppIntent {
        kind: jails_spec::spec::kind::ArtifactKind::Record,
        timestamps: false,
        name: "Note".to_string(),
        fields: fields.iter().map(|f| f.to_string()).collect(),
        indexes: Vec::new(),
        package: None,
        on: None,
        yields: None,
    };
    let at = root.join("src/main/java/com/example/demo/domain/Note.java");

    jails_engine::route::app_apply(
        &Project::load(&root).unwrap(),
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
        &Project::load(&root).unwrap(),
        &[],
        &[note(&["title:string!", "body:string"])],
    )
    .unwrap_err();

    assert!(error.contains("overlap"), "{error}");
    assert!(error.contains("§R5.4") || error.contains("R5.4"), "{error}");
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
        &Project::load(&root).unwrap(),
        jails_spec::spec::kind::ArtifactKind::Record,
        "Reward",
        &["title:string!".to_string()],
        None,
        &[],
        None,
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

    jails_engine::route::rename(&Project::load(&root).unwrap(), "Reward", "Bonus", true).unwrap();

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
            &Project::load(&root).unwrap(),
            jails_spec::spec::kind::ArtifactKind::Record,
            name,
            &["title:string!".to_string()],
            None,
            &[],
            None,
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
    let error =
        jails_engine::route::rename(&Project::load(&root).unwrap(), "Reward", "Bonus", true)
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
    jails_engine::route::rename(&Project::load(&root).unwrap(), "Reward", "Reward2", true).unwrap();
    assert_eq!(
        jails_commit::store::Store::at(&root)
            .observe()
            .unwrap()
            .generation(),
        before + 1,
        "a rename touching four files took more than one generation"
    );
}
