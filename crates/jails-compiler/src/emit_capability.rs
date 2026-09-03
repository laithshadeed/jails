//! The capability recipes, and the walk that renders them.
//!
//! A capability is a model node projected into independently identified
//! files plus build dependencies, and it is one [`Recipe`] row: adding a
//! small adapter or test utility is a row, not a route or a bespoke executor.
//! The Java files render through [`crate::recipe::render`], the loop every
//! recipe shares; what is capability-specific -- resources, compose services
//! and the reader-owned project files -- is rendered beside the rows here.

use crate::CompileError;
use crate::emit_java::JavaUnit;
use crate::recipe::{
    BootCondition, ComposeService, DependencySpec, Fragment, Import, JavaFile, MovedImport, Naming,
    Node, Placement, PropertySpec, Recipe, ResourceFile, SourceSet,
};
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

use spring::{
    ACTUATOR_PACK, API_PACK, CACHE_PACK, CORS_PACK, K8S_PACK, OBSERVABILITY_PACK, REDIS_PACK,
    SECURITY_PACK, SSE_PACK,
};

use storage::{DB_PACK, H2_PACK};

const MAIN_RESOURCE_ROOT: &str = jails_contracts::SourceRoot::MainResources.path();
const TEST_RESOURCE_ROOT: &str = jails_contracts::SourceRoot::TestResources.path();

/// `@AutoConfigureMockMvc`, which Boot 4 moved out of the servlet test slice.
pub(crate) const AUTOCONFIGURE_MOCKMVC: MovedImport = MovedImport {
    moved_at: 4,
    at_or_above: "org.springframework.boot.webmvc.test.autoconfigure.AutoConfigureMockMvc",
    below: "org.springframework.boot.test.autoconfigure.web.servlet.AutoConfigureMockMvc",
};

/// `@WebMvcTest`, which moved with it.
const WEBMVC_TEST: MovedImport = MovedImport {
    moved_at: 4,
    at_or_above: "org.springframework.boot.webmvc.test.autoconfigure.WebMvcTest",
    below: "org.springframework.boot.test.autoconfigure.web.servlet.WebMvcTest",
};

/// `MeterRegistryCustomizer`, which Boot 4 moved out of `actuate.autoconfigure`.
const METER_REGISTRY_CUSTOMIZER: MovedImport = MovedImport {
    moved_at: 4,
    at_or_above: "org.springframework.boot.micrometer.metrics.autoconfigure.MeterRegistryCustomizer",
    below: "org.springframework.boot.actuate.autoconfigure.metrics.MeterRegistryCustomizer",
};

const NO_SUBSTITUTIONS: &[(&str, &str)] = &[];
const NO_RESOURCES: &[ResourceFile] = &[];
const NO_PROPERTIES: &[PropertySpec] = &[];
const NO_FRAGMENTS: &[Fragment<Capability>] = &[];
const NO_COMPOSE_SERVICES: &[ComposeService] = &[];
const NO_BUILD_FEATURES: &[BuildFeature] = &[];

/// A capability's templates spell no typed value of the node beyond the
/// class names on its rows; what varies is on the row as a substitution.
#[derive(Clone, Copy)]
pub(crate) enum NoKey {}

impl Node for Capability {
    type Key = NoKey;

    fn id(&self) -> &str {
        self.id.as_str()
    }

    fn name(&self) -> &str {
        self.name.as_deref().unwrap_or_default()
    }

    fn describe(&self) -> String {
        format!("capability `{}`", self.kind)
    }

    fn key(&self, _: &AppModel, key: NoKey) -> Result<(&'static str, String), CompileError> {
        match key {}
    }

    fn file_keys(&self, package: &str, template_class: &str) -> Vec<(&'static str, String)> {
        vec![
            ("web", package.to_string()),
            ("class", template_class.to_string()),
            ("name", template_class.to_string()),
            ("KAFKA_TESTCONTAINERS_CONFIG", template_class.to_string()),
            ("TESTCONTAINERS_CONFIG", template_class.to_string()),
            ("database", sqlite_database_class(self)),
            ("migrations", sqlite_migrations_class(self)),
        ]
    }

    fn provenance(&self, artifact_id: String, _: bool, _: &'static str) -> Provenance {
        Provenance {
            artifact_id,
            ejection_id: Some(self.id.as_str().to_string()),
            // Every file of a capability is transferable: `eject` moves the
            // whole pack, and the ports it names are the reader's from then on.
            ejectable: true,
            semantic_ids: BTreeSet::from([self.id.as_str().to_string()]),
            compiler_pass: format!("capability-pack-{}", self.kind),
        }
    }

    /// `source`, not `render`: a capability's Java carries no provenance
    /// header, because `remove` retires the whole file rather than
    /// reconciling its bytes.
    fn header(&self) -> bool {
        false
    }

    /// **Once `spring-boot-starter-jdbc` is in the build, JDBC
    /// auto-configuration demands a `DataSource` for every
    /// `@SpringBootTest`** -- including a capability's own test, which never
    /// touches a database. The compiler already asks the materializer to
    /// splice this into the tests *on disk* (`EnsureSpringTestImport`), and
    /// that intent cannot reach the tree the compiler is producing in the
    /// same pass. So the ones it renders carry it from here.
    ///
    /// Without it the generated test has no `DataSource` and falls back to
    /// whatever `spring.datasource.url` names -- which on a developer's
    /// machine is a real database on `:5432`, so the test passes or fails
    /// against somebody's local schema instead of its own container.
    fn splices_test_container(&self, source_set: SourceSet) -> bool {
        source_set == SourceSet::Test
    }
}

pub(crate) fn dependencies(
    model: &AppModel,
    spring_boot: Option<&str>,
    build_exists: bool,
) -> Vec<BuildDependency> {
    model
        .capabilities
        .values()
        .filter_map(|capability| pack(&capability.kind))
        .flat_map(|pack| crate::recipe::dependencies(pack, spring_boot, build_exists))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

pub(crate) fn properties(
    model: &AppModel,
    target: SettingTarget,
    spring_boot: Option<&str>,
    artifact_id: Option<&str>,
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
            value: render_property_value(property.value, model, artifact_id),
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

pub(crate) fn build_features(model: &AppModel) -> BTreeSet<BuildFeature> {
    model
        .capabilities
        .values()
        .filter_map(|capability| pack(&capability.kind))
        .flat_map(crate::recipe::build_features)
        .collect()
}

pub(crate) fn emit(
    model: &AppModel,
    output: &mut RenderedTree,
    snapshot: &jails_contracts::WorkspaceSnapshot,
) -> Result<(), CompileError> {
    let compose_path = crate::emit::compose_path(snapshot)?;
    for capability in model.capabilities.values() {
        if let Some(pack) = pack(&capability.kind) {
            crate::recipe::render(model, capability, pack, snapshot, output)?;
            for resource in pack.resources {
                emit_resource(output, capability, resource)?;
            }
            for service in pack.compose_services {
                reader_facet::emit_compose_service(output, capability, &compose_path, service)?;
            }
        }
        project_file::emit(
            model,
            capability,
            output,
            &snapshot.project,
            &snapshot.template_overrides,
        )?;
    }
    Ok(())
}

pub(crate) fn external_project_paths(model: &AppModel) -> Vec<ProjectPath> {
    project_file::paths(model)
}

/// The recipe registry for capabilities: one row per kind that renders Java.
///
/// `ci`, `docker` and `loadtest` write reader-owned project files rather
/// than Java and are `project_file`'s; `format` and `k8s` are both.
fn pack(kind: &str) -> Option<&'static Recipe<Capability>> {
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

pub(crate) fn minimum_boot(kind: &str) -> Option<(u32, &'static str)> {
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
                provenance: capability.provenance(
                    format!("art_{}_{}", capability.id.as_str(), resource.suffix),
                    true,
                    "",
                ),
            },
        )
        .map_err(CompileError::new)
}

/// Splice `@Import(TestcontainersConfig.class)` into a generated test.
///
/// Separate from the source-set gate in [`Node::splices_test_container`]
/// because an emitter outside the recipes -- an operation's proof, say --
/// knows perfectly well that its file is a test and needs this, and would
/// otherwise reimplement the splice. Without it a generated `@SpringBootTest`
/// has no DataSource: JDBC auto-config demands one the moment the starter is
/// present, so the context fails to start and every case in the class errors.
pub(crate) fn imported_test_container(model: &AppModel, unit: &mut JavaUnit) {
    if !crate::recipe::declares(model, "db") {
        return;
    }
    // The splice needs a whole compilation unit -- it puts the annotation above
    // the type and `Import` beside the other imports -- so it is handed the
    // unit's source and the result read back. `extra` is empty because the
    // config's own import is added below, to the set.
    let Some(spliced) =
        jails_codemod::annotate::splice_import(&unit.source(), "TestcontainersConfig", "")
    else {
        // No `@SpringBootTest` to anchor to: nothing is annotated, so nothing
        // is imported either.
        return;
    };
    *unit = JavaUnit::from_source(&spliced);
    // Skipped when the config is in this file's own package: importing a
    // sibling is redundant and, with `--package ''`, would not parse.
    unit.import_from(
        &model.project.package_for(Package::Base),
        "TestcontainersConfig",
    );
}

/// **The build's own identity wins over the model's application name.** A
/// consumer group is durable in the broker, and the model's name is derived
/// from the directory whenever a model is seeded beside an existing build --
/// so two clones of one service under different directory names would each
/// get their own group and both receive every message.
fn render_property_value(value: &str, model: &AppModel, artifact_id: Option<&str>) -> String {
    let group = artifact_id
        .map(str::to_string)
        .unwrap_or_else(|| model.project.name.clone());
    value
        .replace("{{base_package}}", &model.project.base_package)
        .replace("{{project_group}}", &group.to_ascii_lowercase())
}

/// `jakarta` or `javax`, which Spring Boot crossed at 3.0.
///
/// **A version fact read off the project, never assumed.** Boot 2 is Java EE
/// and the annotations are `javax.validation`; emitting the Jakarta package
/// there hands the reader `package jakarta.validation does not exist` in a
/// file they did not write, which is exactly the compile error a generator
/// exists to remove.
pub(crate) fn validation_package(boot_major: Option<u32>) -> &'static str {
    match boot_major {
        Some(major) if major < 3 => "javax",
        _ => "jakarta",
    }
}

/// Where a capability's files go by default: the package the reader named
/// with `--package`, or the layer the recipe's rows name.
fn placed(model: &AppModel, capability: &Capability, package: Package) -> String {
    capability
        .java_package
        .clone()
        .unwrap_or_else(|| model.project.package_for(package))
}

fn root_package(model: &AppModel, capability: &Capability) -> String {
    capability
        .java_package
        .clone()
        .unwrap_or_else(|| model.project.base_package.clone())
}

fn adapters_package(model: &AppModel, capability: &Capability) -> String {
    placed(model, capability, Package::Adapters)
}

fn api_package(model: &AppModel, capability: &Capability) -> String {
    placed(model, capability, Package::Api)
}

fn testkit_package(model: &AppModel, capability: &Capability) -> String {
    placed(model, capability, Package::Testkit)
}

fn messaging_package(model: &AppModel, capability: &Capability) -> String {
    placed(model, capability, Package::Messaging)
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
