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
