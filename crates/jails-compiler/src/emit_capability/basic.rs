//! Framework-neutral declarative capability packs.

use super::*;

const CSV_FILES: &[JavaFile<Capability>] = &[
    JavaFile {
        role: "main",
        template: crate::template!("add/csv_reader_java.java"),
        before_boot: None,
        imports: &[],
        only_when: None,
        source_set: SourceSet::Main,
        placement: Placement::Default,
        ejectable: true,
        class: Naming::By(csv_class),
        template_class: Naming::By(csv_class),
    },
    JavaFile {
        role: "test",
        template: crate::template!("add/csv_reader_test_java.java"),
        before_boot: None,
        imports: &[],
        only_when: None,
        source_set: SourceSet::Test,
        placement: Placement::Default,
        ejectable: true,
        class: Naming::By(csv_test_class),
        template_class: Naming::By(csv_class),
    },
];

const JSON_FILES: &[JavaFile<Capability>] = &[
    JavaFile {
        role: "main",
        template: crate::template!("add/json_java.java"),
        before_boot: None,
        imports: &[],
        only_when: None,
        source_set: SourceSet::Main,
        placement: Placement::Default,
        ejectable: true,
        class: Naming::By(json_class),
        template_class: Naming::By(json_class),
    },
    JavaFile {
        role: "test",
        template: crate::template!("add/json_test_java.java"),
        before_boot: None,
        imports: &[],
        only_when: None,
        source_set: SourceSet::Test,
        placement: Placement::Default,
        ejectable: true,
        class: Naming::By(json_test_class),
        template_class: Naming::By(json_class),
    },
];

const HTTP_FILES: &[JavaFile<Capability>] = &[
    JavaFile {
        role: "main",
        template: crate::template!("add/http_server_java.java"),
        before_boot: None,
        imports: &[],
        only_when: None,
        source_set: SourceSet::Main,
        placement: Placement::Default,
        ejectable: true,
        class: Naming::By(http_class),
        template_class: Naming::By(http_class),
    },
    JavaFile {
        role: "test",
        template: crate::template!("add/http_server_test_java.java"),
        before_boot: None,
        imports: &[],
        only_when: None,
        source_set: SourceSet::Test,
        placement: Placement::Default,
        ejectable: true,
        class: Naming::By(http_test_class),
        template_class: Naming::By(http_class),
    },
];

const FAKE_FILES: &[JavaFile<Capability>] = &[
    JavaFile {
        role: "script",
        template: crate::template!("add/scripted_java.java"),
        before_boot: None,
        imports: &[],
        only_when: None,
        source_set: SourceSet::Test,
        placement: Placement::Default,
        ejectable: true,
        class: Naming::Fixed("Fake"),
        template_class: Naming::Fixed("Fake"),
    },
    JavaFile {
        role: "script_test",
        template: crate::template!("add/scripted_test_java.java"),
        before_boot: None,
        imports: &[],
        only_when: None,
        source_set: SourceSet::Test,
        placement: Placement::Default,
        ejectable: true,
        class: Naming::Fixed("FakeTest"),
        template_class: Naming::Fixed("Fake"),
    },
];

const TOXIPROXY_FILES: &[JavaFile<Capability>] = &[
    JavaFile {
        role: "faults",
        template: crate::template!("add/faults_java.java"),
        before_boot: None,
        imports: &[],
        only_when: None,
        source_set: SourceSet::Test,
        placement: Placement::Default,
        ejectable: true,
        class: Naming::Fixed("Faults"),
        template_class: Naming::Fixed("Faults"),
    },
    JavaFile {
        role: "faults_test",
        template: crate::template!("add/faults_test_java.java"),
        before_boot: None,
        imports: &[],
        only_when: None,
        source_set: SourceSet::Test,
        placement: Placement::Default,
        ejectable: true,
        class: Naming::Fixed("FaultsTest"),
        template_class: Naming::Fixed("Faults"),
    },
];

const TESTKIT_FILES: &[JavaFile<Capability>] = &[
    JavaFile {
        role: "clocks",
        template: crate::template!("add/clocks_java.java"),
        before_boot: None,
        imports: &[],
        only_when: None,
        source_set: SourceSet::Test,
        placement: Placement::Default,
        ejectable: true,
        class: Naming::Fixed("Clocks"),
        template_class: Naming::Fixed("Clocks"),
    },
    JavaFile {
        role: "ids",
        template: crate::template!("add/ids_java.java"),
        before_boot: None,
        imports: &[],
        only_when: None,
        source_set: SourceSet::Test,
        placement: Placement::Default,
        ejectable: true,
        class: Naming::Fixed("Ids"),
        template_class: Naming::Fixed("Ids"),
    },
    JavaFile {
        role: "fixtures",
        template: crate::template!("add/fixtures_java.java"),
        before_boot: None,
        imports: &[],
        only_when: None,
        source_set: SourceSet::Test,
        placement: Placement::Default,
        ejectable: true,
        class: Naming::Fixed("Fixtures"),
        template_class: Naming::Fixed("Fixtures"),
    },
    JavaFile {
        role: "cli",
        template: crate::template!("add/testkit_cli_java.java"),
        before_boot: None,
        imports: &[],
        only_when: None,
        source_set: SourceSet::Test,
        placement: Placement::Default,
        ejectable: true,
        class: Naming::Fixed("Cli"),
        template_class: Naming::Fixed("Cli"),
    },
    JavaFile {
        role: "test",
        template: crate::template!("add/testkit_test_java.java"),
        before_boot: None,
        imports: &[],
        only_when: None,
        source_set: SourceSet::Test,
        placement: Placement::Default,
        ejectable: true,
        class: Naming::Fixed("TestkitTest"),
        template_class: Naming::Fixed("TestkitTest"),
    },
];

const TESTKIT_RESOURCES: &[ResourceFile] = &[ResourceFile {
    suffix: "fixture_example",
    path: "fixtures/example.json",
    bytes: "{\n  \"name\": \"bolt\",\n  \"qty\": 7\n}\n",
    source_set: SourceSet::Test,
}];

const SQLITE_FILES: &[JavaFile<Capability>] = &[
    JavaFile {
        role: "database",
        template: crate::template!("add/database_java.java"),
        before_boot: None,
        imports: &[],
        only_when: None,
        source_set: SourceSet::Main,
        placement: Placement::Default,
        ejectable: true,
        class: Naming::By(sqlite_database_class),
        template_class: Naming::By(sqlite_database_class),
    },
    JavaFile {
        role: "migrations",
        template: crate::template!("add/migrations_java.java"),
        before_boot: None,
        imports: &[],
        only_when: None,
        source_set: SourceSet::Main,
        placement: Placement::Default,
        ejectable: true,
        class: Naming::By(sqlite_migrations_class),
        template_class: Naming::By(sqlite_migrations_class),
    },
    JavaFile {
        role: "test",
        template: crate::template!("add/database_test_java.java"),
        before_boot: None,
        imports: &[],
        only_when: None,
        source_set: SourceSet::Test,
        placement: Placement::Default,
        ejectable: true,
        class: Naming::By(sqlite_test_class),
        template_class: Naming::By(sqlite_database_class),
    },
];

const CSV_DEPENDENCIES: &[DependencySpec] = &[dependency(
    "org.apache.commons",
    "commons-csv",
    Some("1.14.1"),
    DependencyScope::Compile,
    false,
)];
const JSON_DEPENDENCIES: &[DependencySpec] = &[dependency(
    "tools.jackson.core",
    "jackson-databind",
    Some("3.0.1"),
    DependencyScope::Compile,
    true,
)];
const TOXIPROXY_DEPENDENCIES: &[DependencySpec] = &[
    dependency(
        "org.testcontainers",
        "testcontainers-toxiproxy",
        Some("2.0.5"),
        DependencyScope::Test,
        false,
    ),
    dependency(
        "eu.rekawek.toxiproxy",
        "toxiproxy-java",
        Some("2.1.11"),
        DependencyScope::Test,
        false,
    ),
];
const SQLITE_DEPENDENCIES: &[DependencySpec] = &[dependency(
    "org.xerial",
    "sqlite-jdbc",
    Some("3.49.1.0"),
    DependencyScope::Compile,
    false,
)];
const ASSERTJ_DEPENDENCIES: &[DependencySpec] = &[DependencySpec {
    only_when_build_exists: true,
    optional: false,
    ..dependency(
        "org.assertj",
        "assertj-core",
        Some("3.27.7"),
        DependencyScope::Test,
        true,
    )
}];

pub(super) const CSV_PACK: Recipe<Capability> = pack(CSV_FILES, CSV_DEPENDENCIES, adapters_package);
pub(super) const JSON_PACK: Recipe<Capability> =
    pack(JSON_FILES, JSON_DEPENDENCIES, adapters_package);
pub(super) const HTTP_PACK: Recipe<Capability> =
    pack(HTTP_FILES, ASSERTJ_DEPENDENCIES, api_package);
pub(super) const FAKE_PACK: Recipe<Capability> =
    pack(FAKE_FILES, ASSERTJ_DEPENDENCIES, testkit_package);
pub(super) const TOXIPROXY_PACK: Recipe<Capability> =
    pack(TOXIPROXY_FILES, TOXIPROXY_DEPENDENCIES, testkit_package);
pub(super) const SQLITE_PACK: Recipe<Capability> =
    pack(SQLITE_FILES, SQLITE_DEPENDENCIES, adapters_package);
pub(super) const TESTKIT_PACK: Recipe<Capability> = Recipe {
    substitutions: NO_SUBSTITUTIONS,
    fragments: NO_FRAGMENTS,
    keys: &[],
    requires: &[],
    resources: TESTKIT_RESOURCES,
    ..pack(TESTKIT_FILES, ASSERTJ_DEPENDENCIES, testkit_package)
};

const COVERAGE_FEATURES: &[BuildFeature] = &[BuildFeature::Coverage];

/// `format` is a build feature plus one reader-facing file.
///
/// The `.editorconfig` comes through `project_file.rs`; this is the plugin.
/// Keyed by [`BuildFeature::Formatting`] rather than by a plugin coordinate,
/// because `spotless-maven-plugin` is not a name Gradle resolves.
pub(super) const FORMAT_PACK: Recipe<Capability> = Recipe {
    substitutions: NO_SUBSTITUTIONS,
    fragments: NO_FRAGMENTS,
    keys: &[],
    requires: &[],
    files: &[],
    files_when: BootCondition::Any,
    resources: NO_RESOURCES,
    dependencies: &[],
    properties: NO_PROPERTIES,
    compose_services: NO_COMPOSE_SERVICES,
    build_features: FORMAT_FEATURES,
    default_package: testkit_package,
    minimum_boot: None,
    pass: "",
};

const FORMAT_FEATURES: &[BuildFeature] = &[BuildFeature::Formatting];

pub(super) const COVERAGE_PACK: Recipe<Capability> = Recipe {
    substitutions: NO_SUBSTITUTIONS,
    fragments: NO_FRAGMENTS,
    keys: &[],
    requires: &[],
    files: &[],
    files_when: BootCondition::Any,
    resources: NO_RESOURCES,
    dependencies: &[],
    properties: NO_PROPERTIES,
    compose_services: NO_COMPOSE_SERVICES,
    build_features: COVERAGE_FEATURES,
    default_package: testkit_package,
    minimum_boot: None,
    pass: "",
};

const fn dependency(
    group: &'static str,
    artifact: &'static str,
    version: Option<&'static str>,
    scope: DependencyScope,
    spring_managed_version: bool,
) -> DependencySpec {
    DependencySpec {
        group,
        artifact,
        version,
        scope,
        spring_managed_version,
        only_when_build_exists: false,
        optional: false,
        boot: BootCondition::Any,
    }
}

const fn pack(
    files: &'static [JavaFile<Capability>],
    dependencies: &'static [DependencySpec],
    default_package: fn(&AppModel, &Capability) -> String,
) -> Recipe<Capability> {
    Recipe {
        substitutions: NO_SUBSTITUTIONS,
        fragments: NO_FRAGMENTS,
        keys: &[],
        requires: &[],
        files,
        files_when: BootCondition::Any,
        resources: NO_RESOURCES,
        dependencies,
        properties: NO_PROPERTIES,
        compose_services: NO_COMPOSE_SERVICES,
        build_features: NO_BUILD_FEATURES,
        default_package,
        minimum_boot: None,
        pass: "",
    }
}

fn csv_class(capability: &Capability) -> String {
    format!(
        "{}Reader",
        capitalize(capability.name.as_deref().unwrap_or("Csv"))
    )
}

fn csv_test_class(capability: &Capability) -> String {
    format!("{}Test", csv_class(capability))
}

fn json_class(capability: &Capability) -> String {
    format!(
        "{}Json",
        capability
            .name
            .as_deref()
            .map(capitalize)
            .unwrap_or_default()
    )
}

fn json_test_class(capability: &Capability) -> String {
    format!("{}Test", json_class(capability))
}

fn http_class(capability: &Capability) -> String {
    format!(
        "{}Server",
        capability
            .name
            .as_deref()
            .map(capitalize)
            .unwrap_or_default()
    )
}

fn http_test_class(capability: &Capability) -> String {
    format!("{}Test", http_class(capability))
}

pub(super) fn sqlite_database_class(capability: &Capability) -> String {
    format!(
        "{}Database",
        capability
            .name
            .as_deref()
            .map(capitalize)
            .unwrap_or_default()
    )
}

pub(super) fn sqlite_migrations_class(capability: &Capability) -> String {
    format!(
        "{}Migrations",
        capability
            .name
            .as_deref()
            .map(capitalize)
            .unwrap_or_default()
    )
}

fn sqlite_test_class(capability: &Capability) -> String {
    format!("{}Test", sqlite_database_class(capability))
}
