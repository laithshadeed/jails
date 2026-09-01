//! Framework-neutral declarative capability packs.

use super::*;

const CSV_FILES: &[JavaFile] = &[
    JavaFile {
        suffix: "main",
        template: include_str!("../../../../templates/add/csv_reader_java.java"),
        before_boot: None,
        source_set: SourceSet::Main,
        class_name: csv_class,
        template_class: csv_class,
    },
    JavaFile {
        suffix: "test",
        template: include_str!("../../../../templates/add/csv_reader_test_java.java"),
        before_boot: None,
        source_set: SourceSet::Test,
        class_name: csv_test_class,
        template_class: csv_class,
    },
];

const JSON_FILES: &[JavaFile] = &[
    JavaFile {
        suffix: "main",
        template: include_str!("../../../../templates/add/json_java.java"),
        before_boot: None,
        source_set: SourceSet::Main,
        class_name: json_class,
        template_class: json_class,
    },
    JavaFile {
        suffix: "test",
        template: include_str!("../../../../templates/add/json_test_java.java"),
        before_boot: None,
        source_set: SourceSet::Test,
        class_name: json_test_class,
        template_class: json_class,
    },
];

const HTTP_FILES: &[JavaFile] = &[
    JavaFile {
        suffix: "main",
        template: include_str!("../../../../templates/add/http_server_java.java"),
        before_boot: None,
        source_set: SourceSet::Main,
        class_name: http_class,
        template_class: http_class,
    },
    JavaFile {
        suffix: "test",
        template: include_str!("../../../../templates/add/http_server_test_java.java"),
        before_boot: None,
        source_set: SourceSet::Test,
        class_name: http_test_class,
        template_class: http_class,
    },
];

const FAKE_FILES: &[JavaFile] = &[
    JavaFile {
        suffix: "script",
        template: include_str!("../../../../templates/add/scripted_java.java"),
        before_boot: None,
        source_set: SourceSet::Test,
        class_name: fake_class,
        template_class: fake_class,
    },
    JavaFile {
        suffix: "script_test",
        template: include_str!("../../../../templates/add/scripted_test_java.java"),
        before_boot: None,
        source_set: SourceSet::Test,
        class_name: fake_test_class,
        template_class: fake_class,
    },
];

const TOXIPROXY_FILES: &[JavaFile] = &[
    JavaFile {
        suffix: "faults",
        template: include_str!("../../../../templates/add/faults_java.java"),
        before_boot: None,
        source_set: SourceSet::Test,
        class_name: faults_class,
        template_class: faults_class,
    },
    JavaFile {
        suffix: "faults_test",
        template: include_str!("../../../../templates/add/faults_test_java.java"),
        before_boot: None,
        source_set: SourceSet::Test,
        class_name: faults_test_class,
        template_class: faults_class,
    },
];

const TESTKIT_FILES: &[JavaFile] = &[
    JavaFile {
        suffix: "clocks",
        template: include_str!("../../../../templates/add/clocks_java.java"),
        before_boot: None,
        source_set: SourceSet::Test,
        class_name: clocks_class,
        template_class: clocks_class,
    },
    JavaFile {
        suffix: "ids",
        template: include_str!("../../../../templates/add/ids_java.java"),
        before_boot: None,
        source_set: SourceSet::Test,
        class_name: ids_class,
        template_class: ids_class,
    },
    JavaFile {
        suffix: "fixtures",
        template: include_str!("../../../../templates/add/fixtures_java.java"),
        before_boot: None,
        source_set: SourceSet::Test,
        class_name: fixtures_class,
        template_class: fixtures_class,
    },
    JavaFile {
        suffix: "cli",
        template: include_str!("../../../../templates/add/testkit_cli_java.java"),
        before_boot: None,
        source_set: SourceSet::Test,
        class_name: cli_class,
        template_class: cli_class,
    },
    JavaFile {
        suffix: "test",
        template: include_str!("../../../../templates/add/testkit_test_java.java"),
        before_boot: None,
        source_set: SourceSet::Test,
        class_name: testkit_test_class,
        template_class: testkit_test_class,
    },
];

const TESTKIT_RESOURCES: &[ResourceFile] = &[ResourceFile {
    suffix: "fixture_example",
    path: "fixtures/example.json",
    bytes: "{\n  \"name\": \"bolt\",\n  \"qty\": 7\n}\n",
    source_set: SourceSet::Test,
}];

const SQLITE_FILES: &[JavaFile] = &[
    JavaFile {
        suffix: "database",
        template: include_str!("../../../../templates/add/database_java.java"),
        before_boot: None,
        source_set: SourceSet::Main,
        class_name: sqlite_database_class,
        template_class: sqlite_database_class,
    },
    JavaFile {
        suffix: "migrations",
        template: include_str!("../../../../templates/add/migrations_java.java"),
        before_boot: None,
        source_set: SourceSet::Main,
        class_name: sqlite_migrations_class,
        template_class: sqlite_migrations_class,
    },
    JavaFile {
        suffix: "test",
        template: include_str!("../../../../templates/add/database_test_java.java"),
        before_boot: None,
        source_set: SourceSet::Test,
        class_name: sqlite_test_class,
        template_class: sqlite_database_class,
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

pub(super) const CSV_PACK: Pack = pack(CSV_FILES, CSV_DEPENDENCIES, adapters_package);
pub(super) const JSON_PACK: Pack = pack(JSON_FILES, JSON_DEPENDENCIES, adapters_package);
pub(super) const HTTP_PACK: Pack = pack(HTTP_FILES, ASSERTJ_DEPENDENCIES, api_package);
pub(super) const FAKE_PACK: Pack = pack(FAKE_FILES, ASSERTJ_DEPENDENCIES, testkit_package);
pub(super) const TOXIPROXY_PACK: Pack =
    pack(TOXIPROXY_FILES, TOXIPROXY_DEPENDENCIES, testkit_package);
pub(super) const SQLITE_PACK: Pack = pack(SQLITE_FILES, SQLITE_DEPENDENCIES, adapters_package);
pub(super) const TESTKIT_PACK: Pack = Pack {
    fragments: NO_FRAGMENTS,
    resources: TESTKIT_RESOURCES,
    ..pack(TESTKIT_FILES, ASSERTJ_DEPENDENCIES, testkit_package)
};

const COVERAGE_FEATURES: &[BuildFeature] = &[BuildFeature::Coverage];

/// `format` is a build feature plus one reader-facing file.
///
/// The `.editorconfig` comes through `project_file.rs`; this is the plugin.
/// Keyed by [`BuildFeature::Formatting`] rather than by a plugin coordinate,
/// because `spotless-maven-plugin` is not a name Gradle resolves.
pub(super) const FORMAT_PACK: Pack = Pack {
    fragments: NO_FRAGMENTS,
    files: &[],
    files_when: BootCondition::Any,
    resources: NO_RESOURCES,
    dependencies: &[],
    properties: NO_PROPERTIES,
    compose_services: NO_COMPOSE_SERVICES,
    build_features: FORMAT_FEATURES,
    default_package: testkit_package,
    package_overrides: NO_PACKAGE_OVERRIDES,
    minimum_boot: None,
};

const FORMAT_FEATURES: &[BuildFeature] = &[BuildFeature::Formatting];

pub(super) const COVERAGE_PACK: Pack = Pack {
    fragments: NO_FRAGMENTS,
    files: &[],
    files_when: BootCondition::Any,
    resources: NO_RESOURCES,
    dependencies: &[],
    properties: NO_PROPERTIES,
    compose_services: NO_COMPOSE_SERVICES,
    build_features: COVERAGE_FEATURES,
    default_package: testkit_package,
    package_overrides: NO_PACKAGE_OVERRIDES,
    minimum_boot: None,
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
    files: &'static [JavaFile],
    dependencies: &'static [DependencySpec],
    default_package: fn(&AppModel) -> String,
) -> Pack {
    Pack {
        fragments: NO_FRAGMENTS,
        files,
        files_when: BootCondition::Any,
        resources: NO_RESOURCES,
        dependencies,
        properties: NO_PROPERTIES,
        compose_services: NO_COMPOSE_SERVICES,
        build_features: NO_BUILD_FEATURES,
        default_package,
        package_overrides: NO_PACKAGE_OVERRIDES,
        minimum_boot: None,
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

fn fake_class(_: &Capability) -> String {
    "Fake".to_string()
}

fn fake_test_class(_: &Capability) -> String {
    "FakeTest".to_string()
}

fn faults_class(_: &Capability) -> String {
    "Faults".to_string()
}

fn faults_test_class(_: &Capability) -> String {
    "FaultsTest".to_string()
}

fn clocks_class(_: &Capability) -> String {
    "Clocks".to_string()
}

fn ids_class(_: &Capability) -> String {
    "Ids".to_string()
}

fn fixtures_class(_: &Capability) -> String {
    "Fixtures".to_string()
}

fn cli_class(_: &Capability) -> String {
    "Cli".to_string()
}

fn testkit_test_class(_: &Capability) -> String {
    "TestkitTest".to_string()
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
