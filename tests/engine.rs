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
