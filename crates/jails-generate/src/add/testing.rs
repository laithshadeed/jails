//! The capabilities that exist to make tests possible: `testkit`, `fake`
//! and `toxiproxy`.
//!
//! All three write into the test tree only. `toxiproxy` is the odd one --
//! it puts a proxy in front of a dependency so a test can cut the
//! connection or add latency, which is the failure everything else assumes
//! away.

use super::*;

// ---------------------------------------------------------------------------
// testkit
// ---------------------------------------------------------------------------

/// The four things every testable CLI needs and nobody enjoys writing twice.
/// No dependency: JUnit and AssertJ are already there, and everything here is
/// plain JDK.
///
/// These helpers also apply pressure in the right direction. `Clocks` and
/// `Ids` are only usable by code that *takes* a `Clock` and a
/// `Supplier<String>` instead of calling `Instant.now()` and
/// `UUID.randomUUID()` -- so generating them nudges the design toward the one
/// that can be tested deterministically at all.
pub(super) fn testkit_plan(slice: &Slice) -> Result<Change> {
    let root: &Path = slice.root();
    let testkit: &str = &slice.placed(Layer::Testkit);
    let dir = test_dir(root, testkit);

    Ok(Change {
        files: vec![
            Artifact {
                kind: "capability file",
                path: dir.join("Clocks.java"),
                contents: clocks_java(testkit),
            },
            Artifact {
                kind: "capability file",
                path: dir.join("Ids.java"),
                contents: ids_java(testkit),
            },
            Artifact {
                kind: "capability file",
                path: dir.join("Fixtures.java"),
                contents: fixtures_java(testkit),
            },
            Artifact {
                kind: "capability file",
                path: dir.join("Cli.java"),
                contents: testkit_cli_java(testkit),
            },
            Artifact {
                kind: "capability file",
                path: dir.join("TestkitTest.java"),
                contents: testkit_test_java(testkit),
            },
            Artifact {
                kind: "capability file",
                path: root.join("src/test/resources/fixtures/example.json"),
                contents: EXAMPLE_FIXTURE.to_string(),
            },
        ],
        ..Change::default()
    })
}

pub(super) const EXAMPLE_FIXTURE: &str = "{\n  \"name\": \"bolt\",\n  \"qty\": 7\n}\n";

pub(super) fn clocks_java(pkg: &str) -> String {
    crate::template::render(
        crate::template_here!("add/clocks_java.java"),
        &[("pkg", pkg)],
    )
}

pub(super) fn ids_java(pkg: &str) -> String {
    crate::template::render(crate::template_here!("add/ids_java.java"), &[("pkg", pkg)])
}

pub(super) fn fixtures_java(pkg: &str) -> String {
    crate::template::render(
        crate::template_here!("add/fixtures_java.java"),
        &[("pkg", pkg)],
    )
}

pub(super) fn testkit_cli_java(pkg: &str) -> String {
    crate::template::render(
        crate::template_here!("add/testkit_cli_java.java"),
        &[("pkg", pkg)],
    )
}

pub(super) fn testkit_test_java(pkg: &str) -> String {
    crate::template::render(
        crate::template_here!("add/testkit_test_java.java"),
        &[("pkg", pkg)],
    )
}

// ---------------------------------------------------------------------------
// fake
// ---------------------------------------------------------------------------

/// A scripted test double. Generic by construction: jails has no Java parser
/// and no business acquiring one, so rather than generating a fake *of* some
/// interface, this generates the replay engine and you attach it to any
/// interface with a lambda. One class covers every collaborator in the project.
pub(super) fn fake_plan(slice: &Slice) -> Result<Change> {
    let root: &Path = slice.root();
    let testkit: &str = &slice.placed(Layer::Testkit);
    let dir = test_dir(root, testkit);

    Ok(Change {
        files: vec![
            Artifact {
                kind: "capability file",
                path: dir.join("Fake.java"),
                contents: scripted_java(testkit),
            },
            Artifact {
                kind: "capability file",
                path: dir.join("FakeTest.java"),
                contents: scripted_test_java(testkit),
            },
        ],
        ..Change::default()
    })
}

// ---------------------------------------------------------------------------
// toxiproxy -- network failure as something a test can switch on
// ---------------------------------------------------------------------------

pub(super) const TESTCONTAINERS_TOXIPROXY: Dependency = Dependency {
    group_id: "org.testcontainers",
    artifact_id: "testcontainers-toxiproxy",
    version: Some("2.0.5"),
    scope: Some("test"),
    optional: false,
};
/// The client the container speaks to. Testcontainers 2.x ships the container
/// and nothing else -- `getProxy` lived on the 1.x class and is gone -- so the
/// control API has to be driven directly.
pub(super) const TOXIPROXY_JAVA: Dependency = Dependency {
    group_id: "eu.rekawek.toxiproxy",
    artifact_id: "toxiproxy-java",
    version: Some("2.1.11"),
    scope: Some("test"),
    optional: false,
};

pub(super) fn toxiproxy_plan(slice: &Slice) -> Result<Change> {
    let root: &Path = slice.root();
    let testkit: &str = &slice.placed(Layer::Testkit);
    let dir = test_dir(root, testkit);

    Ok(Change {
        // Deliberately not TESTCONTAINERS_JUNIT: the generated test drives the
        // container itself, and claiming a dependency another capability also
        // owns means `remove toxiproxy` takes it away from `db` too.
        deps: vec![TESTCONTAINERS_TOXIPROXY, TOXIPROXY_JAVA],
        files: vec![
            Artifact {
                kind: "capability file",
                path: dir.join("Faults.java"),
                contents: faults_java(testkit),
            },
            Artifact {
                kind: "capability file",
                path: dir.join("FaultsTest.java"),
                contents: faults_test_java(testkit),
            },
        ],
        ..Change::default()
    })
}

pub(super) fn faults_java(pkg: &str) -> String {
    crate::template::render(
        crate::template_here!("add/faults_java.java"),
        &[("pkg", pkg)],
    )
}

pub(super) fn faults_test_java(pkg: &str) -> String {
    crate::template::render(
        crate::template_here!("add/faults_test_java.java"),
        &[("pkg", pkg)],
    )
}
pub(super) fn scripted_java(pkg: &str) -> String {
    crate::template::render(
        crate::template_here!("add/scripted_java.java"),
        &[("pkg", pkg)],
    )
}

pub(super) fn scripted_test_java(pkg: &str) -> String {
    crate::template::render(
        crate::template_here!("add/scripted_test_java.java"),
        &[("pkg", pkg)],
    )
}
