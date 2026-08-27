//! Project-level architecture fitness projection.
//!
//! The suite is generated once, on the first scaffold, and lives entirely on
//! the test classpath. New projects are strict; adopted projects opt into an
//! explicit, reviewable `.jails/architecture-baseline` store.

use crate::model::{Artifact, Layer, Project};
use crate::pom::Dependency;

pub(crate) const ARCHUNIT_JUNIT5: Dependency = Dependency {
    group_id: "com.tngtech.archunit",
    artifact_id: "archunit-junit5",
    version: Some("1.5.0"),
    scope: Some("test"),
    optional: false,
};

/// Main sources outside `adapters`/`jobs` that already reach `java.sql`.
///
/// `plan.md` P10.8: the first scaffold into an existing project writes
/// `RAW_JDBC_STAYS_IN_ADAPTERS`, and on `minicom-15-01-2026` it went red 24
/// times over the reader's own `Message`, `User` and controllers -- code jails
/// did not write and was not asked about. A generated test that fails on
/// pre-existing code turns "try jails on this project" into "jails broke my
/// build", which is the adoption story in one line.
///
/// The mechanism to accept that already exists and nothing pointed at it: the
/// suite calls `FreezingArchRule.freeze` when `.jails/architecture-baseline`
/// is present, which records today's violations and fails only on new ones.
/// `allowStoreCreation=false` keeps creating it a deliberate, reviewable act
/// -- that part is right, and the missing half was saying so.
///
/// Deliberately *this* rule and not a general audit: it is the one whose
/// violations are ordinary in a project written before jails arrived, and a
/// scan that tried to predict every rule would be re-implementing ArchUnit in
/// Rust. Unknown widens to silence, so a project this misses simply gets the
/// failure it would have got before.
fn preexisting_raw_jdbc(project: &Project) -> Vec<String> {
    let adapters = project.package_named(jails_spec::spec::layout::ADAPTERS, None);
    let jobs = project.package_named(jails_spec::spec::layout::JOBS, None);
    let mut found = Vec::new();
    for (path, source) in project.projected_main_sources() {
        let package = jails_java::java::package_of(&source).unwrap_or_default();
        if package.starts_with(&adapters) || package.starts_with(&jobs) {
            continue;
        }
        // Through `blanked()`: `java.sql` inside a Javadoc example is not a
        // dependency on it.
        if jails_java::java::blanked(&source).contains("java.sql.") {
            found.push(
                path.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default(),
            );
        }
    }
    found.sort();
    found
}

/// What the reader has to know before the strict suite runs on their project.
///
/// Returned rather than printed: this module decides what to write, and
/// `generate.rs` owns the terminal, which is the boundary
/// `only_deliberate_output_modules_print_to_the_terminal` holds.
pub(crate) fn adoption_note(project: &Project) -> Option<String> {
    let architecture_test =
        crate::generate::test_dir(project.root(), project.base()).join("ArchitectureTest.java");
    if architecture_test.is_file() {
        return None;
    }
    let existing = preexisting_raw_jdbc(project);
    if existing.is_empty() {
        return None;
    }
    let mut note = vec![
        format!(
            "note: {} file(s) here outside `adapters` already use `java.sql`, so the",
            existing.len()
        ),
        "      generated `RAW_JDBC_STAYS_IN_ADAPTERS` rule will fail on code jails did not"
            .to_string(),
        "      write:".to_string(),
    ];
    note.extend(existing.iter().map(|name| format!("        {name}")));
    note.extend(
        [
            "      The suite freezes today's violations and fails only on new ones once",
            "      `.jails/architecture-baseline` exists, which is deliberately a decision",
            "      you make rather than one jails makes for you.",
            "      fix: in `src/test/resources/archunit.properties` set BOTH",
            "           `freeze.store.default.allowStoreCreation=true` and",
            "           `freeze.store.default.allowStoreUpdate=true`, run the suite once, set",
            "           both back to `false`, and commit `.jails/architecture-baseline`.",
            "           Creation alone writes an empty index and every rule still fails --",
            "           ArchUnit needs update permission to record what it froze.",
        ]
        .map(str::to_string),
    );
    Some(note.join("\n"))
}

pub(crate) fn artifacts(project: &Project) -> Vec<Artifact> {
    let packages = Packages::of(project);
    let architecture_test =
        crate::generate::test_dir(project.root(), project.base()).join("ArchitectureTest.java");
    if architecture_test.is_file() {
        return Vec::new();
    }
    vec![
        Artifact {
            kind: "architecture test",
            path: architecture_test,
            contents: test_java(&packages),
        },
        Artifact {
            kind: "architecture policy",
            path: project.root().join(".jails/architecture.toml"),
            contents: policy(&packages),
        },
        Artifact {
            kind: "architecture baseline configuration",
            path: project
                .root()
                .join("src/test/resources/archunit.properties"),
            contents: "freeze.store.default.path=.jails/architecture-baseline\n\
                       freeze.store.default.allowStoreCreation=false\n\
                       freeze.store.default.allowStoreUpdate=false\n"
                .to_string(),
        },
    ]
}

struct Packages {
    base: String,
    domain: String,
    app: String,
    service: String,
    web: String,
    adapters: String,
    messaging: String,
    clients: String,
    jobs: String,
}

impl Packages {
    fn of(project: &Project) -> Self {
        Self {
            base: project.base().to_string(),
            domain: project.package(Layer::Domain, None),
            app: project.package(Layer::App, None),
            service: project.package(Layer::Service, None),
            web: project.package(Layer::Web, None),
            adapters: project.package(Layer::Adapters, None),
            messaging: project.package(Layer::Messaging, None),
            clients: project.package(Layer::Clients, None),
            jobs: project.package(Layer::Jobs, None),
        }
    }
}

fn test_java(packages: &Packages) -> String {
    crate::template::render(
        crate::template_here!("spring/architecture_test_java.java"),
        &[
            ("pkg", &packages.base),
            ("domain", &packages.domain),
            ("app", &packages.app),
            ("service", &packages.service),
            ("web", &packages.web),
            ("adapters", &packages.adapters),
            ("messaging", &packages.messaging),
            ("clients", &packages.clients),
            ("jobs", &packages.jobs),
        ],
    )
}

fn policy(packages: &Packages) -> String {
    format!(
        "# Reviewed cross-slice exceptions. Every allowance must be bounded, used,\n\
         # justified, and removed or renewed before its ISO-8601 expiry date.\n\
         # Blanket package patterns (the base package, `..`, or every slice) are refused.\n\
         #\n\
         # [[architecture.allow]]\n\
         # from = \"billing\"\n\
         # to = \"shared\"\n\
         # packages = [\"{}.shared.money..\"]\n\
         # reason = \"Money is the reviewed shared-kernel value\"\n\
         # expires = \"2099-01-31\"\n",
        packages.domain
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_suite_is_strict_without_an_adoption_baseline() {
        let suite = test_java(&Packages {
            base: "com.example.demo".into(),
            domain: "com.example.demo.domain".into(),
            app: "com.example.demo.app".into(),
            service: "com.example.demo.service".into(),
            web: "com.example.demo.web".into(),
            adapters: "com.example.demo.adapters".into(),
            messaging: "com.example.demo.messaging".into(),
            clients: "com.example.demo.clients".into(),
            jobs: "com.example.demo.jobs".into(),
        });
        for rule in [
            "DOMAIN_HAS_NO_FRAMEWORK_DEPENDENCIES",
            "APPLICATION_PORTS_DEPEND_INWARD",
            "ADAPTERS_DO_NOT_DEPEND_ON_WEB",
            "RAW_JDBC_STAYS_IN_ADAPTERS",
            "CONTROLLERS_DO_NOT_EXPOSE_PERSISTENCE",
            "TOP_LEVEL_SLICES_ARE_ACYCLIC",
        ] {
            assert!(suite.contains(rule), "missing architecture rule {rule}");
        }
        assert!(suite.contains("@AnalyzeClasses(packages = \"com.example.demo\")"));
        assert!(suite.contains("com.example.demo.web.."));
        assert!(suite.contains("matching(\"com.example.demo.domain.(*)..\")"));
        assert!(suite.contains(
            "resideOutsideOfPackages(\"com.example.demo.adapters..\", \"com.example.demo.jobs..\")"
        ));
        assert!(suite.contains("allowEmptyShould(true)"));
        assert!(suite.contains("Files.exists(Path.of(\".jails/architecture-baseline\"))"));
    }

    #[test]
    fn policy_documents_bounded_expiring_allowances() {
        let policy = policy(&Packages {
            base: "com.example.demo".into(),
            domain: "com.example.demo.domain".into(),
            app: "com.example.demo.app".into(),
            service: "com.example.demo.service".into(),
            web: "com.example.demo.web".into(),
            adapters: "com.example.demo.adapters".into(),
            messaging: "com.example.demo.messaging".into(),
            clients: "com.example.demo.clients".into(),
            jobs: "com.example.demo.jobs".into(),
        });
        for key in ["from", "to", "packages", "reason", "expires"] {
            assert!(
                policy.contains(key),
                "missing architecture allowance key {key}"
            );
        }
        assert!(policy.contains("com.example.demo.domain.shared.money.."));
    }

    #[test]
    fn dependency_is_test_only_and_explicitly_versioned() {
        assert_eq!(ARCHUNIT_JUNIT5.scope, Some("test"));
        assert!(ARCHUNIT_JUNIT5.version.is_some());
    }
}
