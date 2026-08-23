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
