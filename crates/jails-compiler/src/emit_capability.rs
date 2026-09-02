//! Declarative, merge-managed capability packs.
//!
//! A capability pack is a model node projected into independently identified
//! files plus build dependencies. It is deliberately data-shaped: adding a
//! small adapter or test utility should not require a route or a bespoke
//! executor. Complex capabilities can still own typed lowering
//! modules, but the common dependency + Java-files shape lives here once.

use crate::CompileError;
use crate::emit_java::JavaUnit;
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
    template: crate::Template,
    before_boot: Option<(u32, crate::Template)>,
    /// What this file imports that its template cannot state for itself.
    imports: &'static [Import],
    source_set: SourceSet,
    class_name: fn(&Capability) -> String,
    template_class: fn(&Capability) -> String,
}

/// An import a pack's row names rather than its template.
///
/// **A `.java` template cannot carry a conditional import line**, and the two
/// cases below are both conditional. Naming them on the row keeps the template
/// a real Java file and keeps the decision beside the rest of the pack's data,
/// where the next reader looks.
enum Import {
    /// A class of the pack's *default* package this file names. The statement
    /// is needed only when `package_overrides` puts the file somewhere else,
    /// which `JavaUnit::import_from` decides.
    Own(&'static str),
    /// A type whose package the captured Boot version decides.
    Moved(MovedImport),
}

/// A type whose package a Spring Boot major moved.
///
/// **A version fact read off the captured project, never assumed.** Boot 4
/// moved `@AutoConfigureMockMvc`, `@WebMvcTest` and `MeterRegistryCustomizer`
/// with no shim, so a file naming the wrong one fails on a package that does
/// not exist -- in a file the reader did not write, which is exactly the
/// compile error a generator exists to remove. Both spellings sit here side by
/// side so the pair cannot be edited one at a time.
#[derive(Clone, Copy)]
pub(crate) struct MovedImport {
    /// The Boot major that moved it.
    moved_at: u32,
    /// Where it lives from that major up.
    at_or_above: &'static str,
    /// Where it lived below -- and the answer when the version cannot be read
    /// at all, because a project too old to have the new package is exactly
    /// the project that would fail to compile.
    below: &'static str,
}

impl MovedImport {
    pub(crate) fn resolve(self, boot_major: Option<u32>) -> &'static str {
        match boot_major.is_some_and(|major| major >= self.moved_at) {
            true => self.at_or_above,
            false => self.below,
        }
    }
}

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

struct DependencySpec {
    group: &'static str,
    artifact: &'static str,
    version: Option<&'static str>,
    scope: DependencyScope,
    spring_managed_version: bool,
    only_when_build_exists: bool,
    /// Maven's `<optional>true</optional>`. Boot's own starters mark
    /// `spring-boot-docker-compose` and devtools this way and Spring
    /// Initializr copies them, so a pom that omits it differs from the one the
    /// same choices produce on start.spring.io.
    optional: bool,
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

/// A block of a template that only belongs in the file when the model also
/// declares some other capability.
///
/// **The advice's `DuplicateKeyException` arm is why this exists.** jails puts
/// `@unique` in the schema and generates an `ApiException.Conflict` documented
/// "becomes a 409", and the arm is what joins the two -- without it a
/// duplicate insert answers 500, which is what alerting pages on and what
/// clients retry. The arm cannot be unconditional: `DuplicateKeyException` is
/// Spring's, from `spring-tx`, which arrives with the JDBC starter, and `api`
/// does not require a database.
///
/// There is no ordering trap: the compiler compiles the whole model at once,
/// so "does this model declare `db`" is a question with one answer.
struct Fragment {
    key: &'static str,
    when_capability: &'static str,
    body: &'static str,
}

struct Pack {
    /// What this pack's own templates spell as `{{key}}`: an image tag, a
    /// URL, a route segment.
    ///
    /// **On the row, not in one bag every pack is substituted through.** A
    /// shared list applies `redis:7-alpine` to `mail`'s templates and
    /// `axllent/mailpit` to `redis`'s -- harmless only for as long as no two
    /// packs pick the same key, which is a property nothing checks and which
    /// the next pinned image breaks. A pack's own facts belong beside its
    /// files, its dependencies and its properties.
    substitutions: &'static [(&'static str, &'static str)],
    fragments: &'static [Fragment],
    files: &'static [JavaFile],
    files_when: BootCondition,
    resources: &'static [ResourceFile],
    dependencies: &'static [DependencySpec],
    properties: &'static [PropertySpec],
    compose_services: &'static [ComposeService],
    build_features: &'static [BuildFeature],
    default_package: fn(&AppModel) -> String,
    package_overrides: &'static [PackageOverride],
    /// The Boot major this pack's *main* source needs, and the type that
    /// needs it.
    ///
    /// **The type, because that is what the compiler would have said.** "this
    /// project uses Boot 2" is true of everything jails refuses on an old
    /// project; `ProblemDetail` is the one line a reader can act on.
    minimum_boot: Option<(u32, &'static str)>,
}

const NO_SUBSTITUTIONS: &[(&str, &str)] = &[];
const NO_RESOURCES: &[ResourceFile] = &[];
const NO_PROPERTIES: &[PropertySpec] = &[];
const NO_PACKAGE_OVERRIDES: &[PackageOverride] = &[];
const NO_FRAGMENTS: &[Fragment] = &[];
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
            optional: dependency.optional,
        })
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
    // Every capability the *model* declares, which is a question the compiler
    // can answer once for the whole tree -- asking the project's pom one
    // capability at a time would let `add api` before `add db` leave an advice
    // describing a project that no longer exists.
    let declared = model
        .capabilities
        .values()
        .map(|capability| capability.kind.as_str())
        .collect::<BTreeSet<_>>();
    for capability in model.capabilities.values() {
        if let Some(pack) = pack(&capability.kind) {
            let default_package = capability
                .java_package
                .clone()
                .unwrap_or_else(|| (pack.default_package)(model));
            // Resolved once per pack: a fragment whose capability the model
            // does not declare substitutes to nothing, rather than being left
            // in the file as a literal `{{key}}`.
            let fragments = pack
                .fragments
                .iter()
                .map(|fragment| {
                    let body = match declared.contains(fragment.when_capability) {
                        true => fragment.body,
                        false => "",
                    };
                    (fragment.key, body)
                })
                .collect::<Vec<_>>();
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
                let mut unit = JavaUnit::from_source(&substitute(
                    template_for(file, boot_major).resolve(observed.templates)?,
                    &package,
                    &template_class,
                    capability,
                    pack.substitutions,
                    &fragments,
                ));
                for import in file.imports {
                    match import {
                        Import::Own(class) => unit.import_from(&default_package, class),
                        Import::Moved(moved) => unit.import(moved.resolve(boot_major)),
                    }
                }
                with_test_container(model, file.source_set, &mut unit);
                emit(
                    output,
                    capability,
                    &package,
                    &class,
                    root,
                    kind,
                    file.suffix,
                    // `source`, not `render`: a capability's Java carries no
                    // provenance header, because `remove` retires the whole
                    // file rather than reconciling its bytes.
                    unit.source(),
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
/// local schema instead of its own container.
fn with_test_container(model: &AppModel, source_set: SourceSet, unit: &mut JavaUnit) {
    if !matches!(source_set, SourceSet::Test) {
        return;
    }
    imported_test_container(model, unit);
}

/// Splice `@Import(TestcontainersConfig.class)` into a generated test.
///
/// Separate from the source-set gate above because an emitter outside the
/// capability packs -- an operation's proof, say -- knows perfectly well that
/// its file is a test and needs this, and would otherwise reimplement the
/// splice. Without it a generated `@SpringBootTest` has no DataSource: JDBC
/// auto-config demands one the moment the starter is present, so the context
/// fails to start and every case in the class errors.
pub(crate) fn imported_test_container(model: &AppModel, unit: &mut JavaUnit) {
    if !model
        .capabilities
        .values()
        .any(|capability| capability.kind == "db")
    {
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

fn template_for(file: &JavaFile, boot_major: Option<u32>) -> crate::Template {
    match file.before_boot {
        Some((limit, template)) if boot_major.is_some_and(|major| major < limit) => template,
        _ => file.template,
    }
}

/// Fill in a pack template's placeholders. Substitution only: what varies
/// structurally is a fragment the caller rendered, and an import the pack
/// needs is a name on its row that `JavaUnit` adds to the one import block --
/// never a placeholder here, because a rendered `import` statement is exactly
/// what makes two emitters able to write one twice.
fn substitute(
    template: &str,
    package: &str,
    class: &str,
    capability: &Capability,
    substitutions: &[(&str, &str)],
    fragments: &[(&str, &str)],
) -> String {
    let mut template = template.to_string();
    for (key, body) in fragments.iter().chain(substitutions) {
        template = template.replace(&format!("{{{{{key}}}}}"), body);
    }
    template
        .replace("{{pkg}}", package)
        .replace("{{web}}", package)
        .replace("{{class}}", class)
        .replace("{{name}}", class)
        .replace("{{KAFKA_TESTCONTAINERS_CONFIG}}", class)
        .replace("{{TESTCONTAINERS_CONFIG}}", class)
        .replace("{{database}}", &sqlite_database_class(capability))
        .replace("{{migrations}}", &sqlite_migrations_class(capability))
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
