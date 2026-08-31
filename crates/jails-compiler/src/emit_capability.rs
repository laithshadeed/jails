//! Declarative, merge-managed capability packs.
//!
//! A capability pack is a model node projected into independently identified
//! files plus build dependencies. It is deliberately data-shaped: adding a
//! small adapter or test utility should not require a route, journal protocol,
//! or bespoke executor. Complex capabilities can still own typed lowering
//! modules, but the common dependency + Java-files shape lives here once.

use crate::CompileError;
use jails_contracts::{
    BuildDependency, BuildFeature, FileKind, FileMode, ProjectPath, PropertyEntry, Provenance,
    RenderedFile, RenderedTree,
};
use jails_model::{AppModel, Capability, DependencyScope, Package, SettingTarget, StableId};
use std::collections::BTreeSet;

mod basic;
mod messaging;
mod project_file;
mod reader_facet;
mod spring;
mod storage;

use basic::{
    COVERAGE_PACK, CSV_PACK, FAKE_PACK, FORMAT_PACK, HTTP_PACK, JSON_PACK, SQLITE_PACK,
    TESTKIT_PACK, TOXIPROXY_PACK, sqlite_database_class, sqlite_migrations_class,
};
use messaging::{KAFKA_PACK, MAIL_PACK};
use reader_facet::ComposeService;

use spring::{
    ACTUATOR_PACK, API_PACK, CACHE_PACK, CORS_PACK, K8S_PACK, OBSERVABILITY_PACK, REDIS_PACK,
    SECURITY_PACK, SSE_PACK,
};

use storage::{DB_PACK, H2_PACK};

const MAIN_ROOT: &str = ".jails/generated/main/java";
const TEST_ROOT: &str = ".jails/generated/test/java";
const MAIN_RESOURCE_ROOT: &str = ".jails/generated/main/resources";
const TEST_RESOURCE_ROOT: &str = ".jails/generated/test/resources";

#[derive(Clone, Copy)]
enum SourceSet {
    Main,
    Test,
    IntegrationTest,
}

struct JavaFile {
    suffix: &'static str,
    template: &'static str,
    before_boot: Option<(u32, &'static str)>,
    source_set: SourceSet,
    class_name: fn(&Capability) -> String,
    template_class: fn(&Capability) -> String,
}

struct DependencySpec {
    group: &'static str,
    artifact: &'static str,
    version: Option<&'static str>,
    scope: DependencyScope,
    spring_managed_version: bool,
    only_when_build_exists: bool,
    boot: BootCondition,
}

struct ResourceFile {
    suffix: &'static str,
    path: &'static str,
    bytes: &'static str,
    source_set: SourceSet,
}

struct PropertySpec {
    key: &'static str,
    value: &'static str,
    target: SettingTarget,
    boot: BootCondition,
}

struct PackageOverride {
    suffix: &'static str,
    project_subpackage: Package,
}

#[derive(Clone, Copy)]
enum BootCondition {
    Any,
    Spring,
    Plain,
    AtLeast(u32),
    Before(u32),
}

impl BootCondition {
    fn matches(self, major: Option<u32>) -> bool {
        match self {
            Self::Any => true,
            Self::Spring => major.is_some(),
            Self::Plain => major.is_none(),
            Self::AtLeast(minimum) => major.is_some_and(|major| major >= minimum),
            Self::Before(limit) => major.is_some_and(|major| major < limit),
        }
    }
}

struct Pack {
    files: &'static [JavaFile],
    files_when: BootCondition,
    resources: &'static [ResourceFile],
    dependencies: &'static [DependencySpec],
    properties: &'static [PropertySpec],
    compose_services: &'static [ComposeService],
    build_features: &'static [BuildFeature],
    default_package: fn(&AppModel) -> String,
    package_overrides: &'static [PackageOverride],
    minimum_boot: Option<u32>,
}

const NO_RESOURCES: &[ResourceFile] = &[];
const NO_PROPERTIES: &[PropertySpec] = &[];
const NO_PACKAGE_OVERRIDES: &[PackageOverride] = &[];
const NO_COMPOSE_SERVICES: &[ComposeService] = &[];
const NO_BUILD_FEATURES: &[BuildFeature] = &[];

pub(crate) fn dependencies(
    model: &AppModel,
    spring_boot: Option<&str>,
    build_exists: bool,
) -> Vec<BuildDependency> {
    let boot_major = boot_major(spring_boot);
    model
        .capabilities
        .values()
        .filter_map(|capability| pack(&capability.kind))
        .flat_map(|pack| pack.dependencies)
        .filter(|dependency| build_exists || !dependency.only_when_build_exists)
        .filter(|dependency| dependency.boot.matches(boot_major))
        .map(|dependency| BuildDependency {
            group: dependency.group.to_string(),
            artifact: dependency.artifact.to_string(),
            version: dependency
                .version
                .filter(|_| spring_boot.is_none() || !dependency.spring_managed_version)
                .map(str::to_string),
            scope: dependency.scope,
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

pub(crate) fn properties(
    model: &AppModel,
    target: SettingTarget,
    spring_boot: Option<&str>,
) -> Vec<PropertyEntry> {
    let boot_major = boot_major(spring_boot);
    model
        .capabilities
        .values()
        .filter_map(|capability| pack(&capability.kind))
        .flat_map(|pack| pack.properties)
        .filter(|property| property.target == target && property.boot.matches(boot_major))
        .map(|property| PropertyEntry {
            key: property.key.to_string(),
            value: render_property_value(property.value, model),
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

pub(crate) fn build_features(model: &AppModel) -> BTreeSet<BuildFeature> {
    let mut features = model
        .capabilities
        .values()
        .filter_map(|capability| pack(&capability.kind))
        .flat_map(|pack| pack.build_features.iter().copied())
        .collect::<BTreeSet<_>>();
    features.extend(
        model
            .capabilities
            .values()
            .filter_map(|capability| pack(&capability.kind))
            .flat_map(|pack| pack.files)
            .filter_map(|file| {
                matches!(file.source_set, SourceSet::IntegrationTest)
                    .then_some(BuildFeature::IntegrationTests)
            }),
    );
    features
}

pub(crate) fn lower_and_emit(
    model: &AppModel,
    output: &mut RenderedTree,
    observed: &crate::emit::Observed<'_>,
) -> Result<(), CompileError> {
    let boot_major = boot_major(observed.spring_boot);
    for capability in model.capabilities.values() {
        if let Some(pack) = pack(&capability.kind) {
            let default_package = capability
                .java_package
                .clone()
                .unwrap_or_else(|| (pack.default_package)(model));
            for file in pack
                .files
                .iter()
                .filter(|_| pack.files_when.matches(boot_major))
            {
                let package = pack
                    .package_overrides
                    .iter()
                    .find(|placement| placement.suffix == file.suffix)
                    .map(|placement| model.project.package_for(placement.project_subpackage))
                    .unwrap_or_else(|| default_package.clone());
                let class = (file.class_name)(capability);
                let template_class = (file.template_class)(capability);
                let (root, kind) = match file.source_set {
                    SourceSet::Main => (MAIN_ROOT, FileKind::JavaMain),
                    SourceSet::Test | SourceSet::IntegrationTest => (TEST_ROOT, FileKind::JavaTest),
                };
                emit(
                    output,
                    capability,
                    &package,
                    &class,
                    root,
                    kind,
                    file.suffix,
                    with_test_container(
                        model,
                        file.source_set,
                        &package,
                        render(
                            template_for(file, boot_major),
                            &package,
                            &default_package,
                            &template_class,
                            capability,
                            boot_major,
                            model,
                        ),
                    ),
                )?;
            }
            for resource in pack.resources {
                emit_resource(output, capability, resource)?;
            }
            for service in pack.compose_services {
                reader_facet::emit_compose_service(
                    output,
                    capability,
                    observed.compose_path,
                    service,
                )?;
            }
        }
        project_file::lower_and_emit(model, capability, output, observed)?;
    }
    Ok(())
}

pub(crate) fn external_project_paths(model: &AppModel) -> Vec<ProjectPath> {
    project_file::paths(model)
}

fn pack(kind: &str) -> Option<&'static Pack> {
    match kind {
        "csv" => Some(&CSV_PACK),
        "json" => Some(&JSON_PACK),
        "http" => Some(&HTTP_PACK),
        "fake" => Some(&FAKE_PACK),
        "testkit" => Some(&TESTKIT_PACK),
        "toxiproxy" => Some(&TOXIPROXY_PACK),
        "coverage" => Some(&COVERAGE_PACK),
        "format" => Some(&FORMAT_PACK),
        "sqlite" => Some(&SQLITE_PACK),
        "db" => Some(&DB_PACK),
        "h2" => Some(&H2_PACK),
        "actuator" => Some(&ACTUATOR_PACK),
        "cache" => Some(&CACHE_PACK),
        "api" => Some(&API_PACK),
        "cors" => Some(&CORS_PACK),
        "observability" => Some(&OBSERVABILITY_PACK),
        "security" => Some(&SECURITY_PACK),
        "sse" => Some(&SSE_PACK),
        "redis" => Some(&REDIS_PACK),
        "kafka" => Some(&KAFKA_PACK),
        "mail" => Some(&MAIL_PACK),
        "k8s" => Some(&K8S_PACK),
        _ => None,
    }
}

pub(crate) fn minimum_boot(kind: &str) -> Option<u32> {
    pack(kind).and_then(|pack| pack.minimum_boot)
}

fn emit_resource(
    output: &mut RenderedTree,
    capability: &Capability,
    resource: &ResourceFile,
) -> Result<(), CompileError> {
    let root = match resource.source_set {
        SourceSet::Main => MAIN_RESOURCE_ROOT,
        SourceSet::Test | SourceSet::IntegrationTest => TEST_RESOURCE_ROOT,
    };
    let path =
        ProjectPath::parse(format!("{root}/{}", resource.path)).map_err(CompileError::new)?;
    output
        .insert(
            path,
            RenderedFile {
                kind: FileKind::Resource,
                mode: FileMode::Regular,
                bytes: resource.bytes.as_bytes().to_vec(),
                provenance: Provenance {
                    artifact_id: format!("art_{}_{}", capability.id.as_str(), resource.suffix),
                    ejection_id: Some(capability.id.as_str().to_string()),
                    ejectable: true,
                    semantic_ids: BTreeSet::from([capability.id.as_str().to_string()]),
                    compiler_pass: format!("capability-pack-{}", capability.kind),
                },
            },
        )
        .map_err(CompileError::new)
}

#[allow(clippy::too_many_arguments)]
fn emit(
    output: &mut RenderedTree,
    capability: &Capability,
    package: &str,
    java_type: &str,
    root: &str,
    kind: FileKind,
    suffix: &str,
    bytes: String,
) -> Result<(), CompileError> {
    let path = ProjectPath::parse(format!(
        "{root}/{}/{}.java",
        package.replace('.', "/"),
        java_type
    ))
    .map_err(CompileError::new)?;
    let artifact_id = format!("art_{}_{}", capability.id.as_str(), suffix);
    output
        .insert(
            path,
            RenderedFile {
                kind,
                mode: FileMode::Regular,
                bytes: bytes.into_bytes(),
                provenance: Provenance {
                    artifact_id,
                    ejection_id: Some(capability.id.as_str().to_string()),
                    ejectable: true,
                    semantic_ids: BTreeSet::from([capability.id.as_str().to_string()]),
                    compiler_pass: format!("capability-pack-{}", capability.kind),
                },
            },
        )
        .map_err(CompileError::new)
}

/// Import the container config into a generated `@SpringBootTest`.
///
/// **Once `spring-boot-starter-jdbc` is in the build, JDBC auto-configuration
/// demands a `DataSource` for every `@SpringBootTest`** -- including a
/// capability's own test, which never touches a database. The compiler already
/// asks the materializer to splice this into the tests *on disk*
/// (`EnsureSpringTestImport`), and that intent cannot reach the tree the
/// compiler is producing in the same pass. So the ones it renders carry it
/// from here.
///
/// Without it the generated test has no `DataSource` and falls back to
/// whatever `spring.datasource.url` names -- which on a developer's machine is
/// a real database on `:5432`, so the test passes or fails against somebody's
/// local schema instead of its own container. That is how it surfaced: a proof
/// application's `CorsConfigTest` failed a Flyway checksum against a database
/// three days older than the run.
fn with_test_container(
    model: &AppModel,
    source_set: SourceSet,
    package: &str,
    body: String,
) -> String {
    if !matches!(source_set, SourceSet::Test) {
        return body;
    }
    if !model
        .capabilities
        .values()
        .any(|capability| capability.kind == "db")
    {
        return body;
    }
    let base = model.project.package_for(Package::Base);
    // `extra` is the import *statement*, and only when the config is not in
    // this file's own package. Passing the package name itself puts a bare
    // `com.example.app` line in the middle of the imports, which is the shape
    // the first draft produced and javac reported as "class, interface, enum,
    // or record expected".
    let extra = match package == base {
        true => String::new(),
        false => format!("import {base}.TestcontainersConfig;\n"),
    };
    jails_codemod::annotate::splice_import(&body, "TestcontainersConfig", &extra).unwrap_or(body)
}

fn template_for(file: &JavaFile, boot_major: Option<u32>) -> &'static str {
    match file.before_boot {
        Some((limit, template)) if boot_major.is_some_and(|major| major < limit) => template,
        _ => file.template,
    }
}

fn render(
    template: &str,
    package: &str,
    default_package: &str,
    class: &str,
    capability: &Capability,
    boot_major: Option<u32>,
    model: &AppModel,
) -> String {
    let (duplicate_key_import, duplicate_key_handler, duplicate_key_test, duplicate_key_route) =
        duplicate_key_parts(model);
    let hub_import = if package == default_package {
        String::new()
    } else {
        format!("import {default_package}.EventHub;\n")
    };
    template
        .replace("{{pkg}}", package)
        .replace("{{web}}", package)
        .replace("{{class}}", class)
        .replace("{{name}}", class)
        .replace("{{hub_import}}", &hub_import)
        .replace("{{path}}", "events")
        .replace("{{REDIS_IMAGE}}", "redis:7-alpine")
        .replace("{{image}}", "axllent/mailpit:v1.21")
        .replace("{{KAFKA_TESTCONTAINERS_CONFIG}}", class)
        .replace("{{TESTCONTAINERS_CONFIG}}", class)
        .replace("{{POSTGRES_IMAGE}}", "postgres:17-alpine")
        .replace("{{database}}", &sqlite_database_class(capability))
        .replace("{{migrations}}", &sqlite_migrations_class(capability))
        .replace("{{test_url}}", "jdbc:h2:mem:test")
        .replace(
            "{{mockmvc_import}}",
            mockmvc_autoconfigure_import(boot_major),
        )
        .replace(
            "{{customizer_import}}",
            meter_registry_customizer_import(boot_major),
        )
        .replace("{{webmvc_test_import}}", webmvc_test_import(boot_major))
        .replace("{{duplicate_key_import}}", duplicate_key_import)
        .replace("{{duplicate_key_handler}}", duplicate_key_handler)
        .replace("{{duplicate_key_test}}", duplicate_key_test)
        .replace("{{duplicate_key_route}}", duplicate_key_route)
}

/// The `DuplicateKeyException` arm, as rendered text or nothing.
///
/// **The exception is Spring's from `spring-tx`**, so an `api`-without-JDBC
/// project given the arm unconditionally gets a compile error for a file it
/// did not write -- which is the trap `CLAUDE.md` records for exactly this
/// slice. It is rendered only where a JDBC capability put the type on the
/// classpath.
///
/// Structural variation stays in Rust rather than becoming a template engine,
/// which is `template.rs`'s rule: the template has a hole and this decides
/// what goes in it. The *text* is shared with the legacy engine as files under
/// `templates/spring/fragments/`, because two copies of one generated block
/// drift where nobody looks.
fn duplicate_key_parts(
    model: &AppModel,
) -> (&'static str, &'static str, &'static str, &'static str) {
    let jdbc = model
        .capabilities
        .values()
        .any(|capability| matches!(capability.kind.as_str(), "db" | "sqlite" | "h2"));
    if jdbc {
        (
            "import org.springframework.dao.DuplicateKeyException;",
            include_str!("../../../templates/spring/fragments/api_duplicate_key_handler.java.txt"),
            include_str!("../../../templates/spring/fragments/api_duplicate_key_test.java.txt"),
            include_str!("../../../templates/spring/fragments/api_duplicate_key_route.java.txt"),
        )
    } else {
        ("", "", "", "")
    }
}

fn render_property_value(value: &str, model: &AppModel) -> String {
    value
        .replace("{{base_package}}", &model.project.base_package)
        .replace(
            "{{project_group}}",
            &model.project.name.to_ascii_lowercase(),
        )
}

fn mockmvc_autoconfigure_import(boot_major: Option<u32>) -> &'static str {
    if boot_major.is_some_and(|major| major >= 4) {
        "org.springframework.boot.webmvc.test.autoconfigure.AutoConfigureMockMvc"
    } else {
        "org.springframework.boot.test.autoconfigure.web.servlet.AutoConfigureMockMvc"
    }
}

fn meter_registry_customizer_import(boot_major: Option<u32>) -> &'static str {
    if boot_major.is_some_and(|major| major >= 4) {
        "org.springframework.boot.micrometer.metrics.autoconfigure.MeterRegistryCustomizer"
    } else {
        "org.springframework.boot.actuate.autoconfigure.metrics.MeterRegistryCustomizer"
    }
}

fn webmvc_test_import(boot_major: Option<u32>) -> &'static str {
    if boot_major.is_some_and(|major| major >= 4) {
        "org.springframework.boot.webmvc.test.autoconfigure.WebMvcTest"
    } else {
        "org.springframework.boot.test.autoconfigure.web.servlet.WebMvcTest"
    }
}

fn adapters_package(model: &AppModel) -> String {
    model.project.package_for(Package::Adapters)
}

fn api_package(model: &AppModel) -> String {
    model.project.package_for(Package::Api)
}

fn testkit_package(model: &AppModel) -> String {
    model.project.package_for(Package::Testkit)
}

pub(crate) fn boot_major(version: Option<&str>) -> Option<u32> {
    version?.split('.').next()?.parse().ok()
}

fn capitalize(value: &str) -> String {
    let mut characters = value.chars();
    match characters.next() {
        Some(first) => first.to_uppercase().chain(characters).collect(),
        None => String::new(),
    }
}
