//! Pure project and intent values shared by planning, rendering, and apply.
//!
//! The command modules used to pass a bare `root: &Path` and rediscover the
//! POM, flavor, base package, configured layers, and installed capabilities at
//! every layer of the call graph. `Project` is the single resolved snapshot
//! handed to planners instead. It is deliberately loaded at the CLI boundary;
//! planning code must not reach back into the filesystem for facts already
//! represented here.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::compose::Service as ComposeService;
use crate::config::Config;
use crate::pom::{self, Dependency, Flavor};
use jails_support::Result;

/// One file a recipe intends to create.
///
/// The rendered string is deliberately still eager at rung 2. Rung 4 changes
/// it to `Body` after every producer uses this one shape; doing both migrations
/// at once would make a behavioral regression harder to localise.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Artifact {
    pub kind: &'static str,
    pub path: PathBuf,
    pub contents: String,
}

impl Artifact {
    pub fn rendered(path: PathBuf, contents: String) -> Self {
        Self {
            kind: "capability file",
            path,
            contents,
        }
    }
}

/// A test-classpath import owned by a capability change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpringTestImport {
    pub pkg: String,
    pub class: &'static str,
}

impl SpringTestImport {
    pub fn fqcn(&self) -> String {
        format!("{}.{}", self.pkg, self.class)
    }
}

/// Everything one recipe intends to change, computed before it is applied.
///
/// This is the shared command value. Capabilities use all of it; generators
/// initially use the file subset and then migrate their dependency/codemod
/// tails into the same value.
#[derive(Clone, Debug, Default)]
pub struct Change {
    pub deps: Vec<Dependency>,
    pub plugins: Vec<(&'static str, String)>,
    pub files: Vec<Artifact>,
    pub compose: Vec<ComposeService>,
    pub properties: Vec<String>,
    pub legacy_deps: Vec<Dependency>,
    pub spring_test_import: Option<SpringTestImport>,
}

impl Change {
    /// Associatively combine independently planned recipe changes.
    ///
    /// Equal contributions collapse; two different contributions claiming
    /// the same identity are rejected before either reaches disk. This is the
    /// algebra used by multi-capability and whole-manifest planning.
    pub fn merge(mut self, other: Self) -> Result<Self> {
        for dep in other.deps {
            match self.deps.iter().find(|current| {
                current.group_id == dep.group_id && current.artifact_id == dep.artifact_id
            }) {
                Some(current) if current != &dep => {
                    return Err(format!(
                        "conflicting dependency plans for {}:{}",
                        dep.group_id, dep.artifact_id
                    ));
                }
                Some(_) => {}
                None => self.deps.push(dep),
            }
        }
        for (artifact_id, body) in other.plugins {
            match self
                .plugins
                .iter()
                .find(|(current, _)| *current == artifact_id)
            {
                Some((_, current)) if current != &body => {
                    return Err(format!("conflicting plugin plans for {artifact_id}"));
                }
                Some(_) => {}
                None => self.plugins.push((artifact_id, body)),
            }
        }
        for file in other.files {
            match self.files.iter().find(|current| current.path == file.path) {
                Some(current) if current.contents != file.contents => {
                    return Err(format!(
                        "two recipes would write different contents to {}",
                        file.path.display()
                    ));
                }
                Some(_) => {}
                None => self.files.push(file),
            }
        }
        for service in other.compose {
            match self
                .compose
                .iter()
                .find(|current| current.name == service.name)
            {
                Some(current) if current != &service => {
                    return Err(format!(
                        "conflicting compose service plans for {}",
                        service.name
                    ));
                }
                Some(_) => {}
                None => self.compose.push(service),
            }
        }
        for property in other.properties {
            if !self.properties.contains(&property) {
                self.properties.push(property);
            }
        }
        for dep in other.legacy_deps {
            if !self.legacy_deps.iter().any(|current| {
                current.group_id == dep.group_id && current.artifact_id == dep.artifact_id
            }) {
                self.legacy_deps.push(dep);
            }
        }
        self.spring_test_import = match (self.spring_test_import, other.spring_test_import) {
            (Some(current), Some(next)) if current != next => {
                return Err("two recipes require different Spring test imports".to_string());
            }
            (Some(current), _) => Some(current),
            (None, next) => next,
        };
        Ok(self)
    }
}

/// The conventional package roles understood by jails.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Layer {
    Domain,
    App,
    Service,
    Web,
    Api,
    Messaging,
    Cli,
    Clients,
    Jobs,
    Adapters,
    Testkit,
}

impl Layer {
    const ALL: [Self; 11] = [
        Self::Domain,
        Self::App,
        Self::Service,
        Self::Web,
        Self::Api,
        Self::Messaging,
        Self::Cli,
        Self::Clients,
        Self::Jobs,
        Self::Adapters,
        Self::Testkit,
    ];

    pub const fn key(self) -> &'static str {
        use crate::spec::layout;
        match self {
            Self::Domain => layout::DOMAIN,
            Self::App => layout::APP,
            Self::Service => layout::SERVICE,
            Self::Web => layout::WEB,
            Self::Api => layout::API,
            Self::Messaging => layout::MESSAGING,
            Self::Cli => layout::CLI,
            Self::Clients => layout::CLIENTS,
            Self::Jobs => layout::JOBS,
            Self::Adapters => layout::ADAPTERS,
            Self::Testkit => layout::TESTKIT,
        }
    }
}

/// Layer package names with every `jails.toml` override already applied.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layers {
    packages: BTreeMap<String, String>,
}

impl Layers {
    fn from_config(config: &Config) -> Self {
        Self {
            packages: Layer::ALL
                .into_iter()
                .map(|layer| {
                    (
                        layer.key().to_string(),
                        config.layer(layer.key()).to_string(),
                    )
                })
                .collect(),
        }
    }

    /// Resolve a typed conventional layer.
    pub fn get(&self, layer: Layer) -> &str {
        self.named(layer.key())
    }

    /// Transitional adapter for recipe code still expressed with the public
    /// layer key strings. Keeping it here makes the configuration decision a
    /// secret of `Layers` while those call sites move to [`Layer`].
    pub fn named<'a>(&'a self, default: &'a str) -> &'a str {
        self.packages
            .get(default)
            .map(String::as_str)
            .unwrap_or(default)
    }

    /// Compatibility spelling while renderer call sites move from `Config`
    /// to this resolved value.
    pub fn layer<'a>(&'a self, default: &'a str) -> &'a str {
        self.named(default)
    }
}

/// One immutable snapshot of the project facts every recipe needs.
#[derive(Clone, Debug)]
pub struct Project {
    root: PathBuf,
    base: String,
    flavor: Flavor,
    java_release: Option<u32>,
    layers: Layers,
    pom: String,
    installed: Vec<String>,
    build: crate::build::Build,
}

impl Project {
    /// Resolve project facts exactly once from a known Maven module root.
    pub fn load(root: &Path) -> Result<Self> {
        // Every command that writes resolves a project first, so this is the
        // one place template overrides have to be pointed at a root -- and no
        // generator has to remember to do it.
        crate::template::install(root);
        let build = crate::build::detect(root);
        // A foreign build has no pom to read and jails will not read its own
        // build file (`build.rs` says why), so every fact the pom carries takes
        // its default. That is not silent: `Project::build` is what
        // `generate` reports and `doctor` names, and `require_maven` is what
        // stops a command that needs the real answer from running on a guess.
        let pom = match build {
            crate::build::Build::Maven => pom::read(root)?,
            _ => String::new(),
        };
        let config = Config::load(root)?;
        Ok(Self {
            root: root.to_path_buf(),
            base: crate::spec::base_package(root)?,
            flavor: pom::flavor(&pom),
            java_release: pom::release_level(&pom),
            layers: Layers::from_config(&config),
            installed: config.capabilities().to_vec(),
            build,
            pom,
        })
    }

    /// Resolve what can be resolved, for the read-only commands.
    ///
    /// `doctor`, `routes`, `beans` and `stats` must work on a project that
    /// does not build -- that is the case they exist for, and `inspect.rs`
    /// says so out loud. A missing base package is therefore a *fact to
    /// record* here rather than an error to raise: there is no `.java` file
    /// under `src/main/java` yet, and the honest snapshot of that is an empty
    /// base.
    ///
    /// Generators keep using [`Project::load`], which refuses, because writing
    /// a file into a package jails had to guess is the failure this
    /// distinction exists to prevent. Anything that *writes* must not reach
    /// for this constructor.
    pub fn inspect(root: &Path) -> Result<Self> {
        crate::template::install(root);
        let pom = pom::read(root).unwrap_or_default();
        let config = Config::load(root)?;
        Ok(Self {
            root: root.to_path_buf(),
            base: crate::spec::base_package(root).unwrap_or_default(),
            flavor: pom::flavor(&pom),
            java_release: pom::release_level(&pom),
            layers: Layers::from_config(&config),
            installed: config.capabilities().to_vec(),
            build: crate::build::detect(root),
            pom,
        })
    }

    /// Discover the containing Maven module and resolve it once.
    pub fn discover() -> Result<Self> {
        Self::load(&crate::spec::find_project_root()?)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// What builds this project. `plan.md` §12.
    pub fn build(&self) -> crate::build::Build {
        self.build
    }

    /// Refuse a command that cannot work without Maven, naming what still can.
    pub fn require_maven(&self, command: &str) -> Result<()> {
        crate::build::require_maven(self.build, command)
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    pub fn flavor(&self) -> Flavor {
        self.flavor
    }

    pub fn java_release(&self) -> Option<u32> {
        self.java_release
    }

    pub fn pom(&self) -> &str {
        &self.pom
    }

    pub fn layers(&self) -> &Layers {
        &self.layers
    }

    pub fn capabilities(&self) -> &[String] {
        &self.installed
    }

    /// Resolve a package override, or the configured conventional layer.
    pub fn package(&self, layer: Layer, package: Option<&str>) -> String {
        crate::spec::subpackage(
            &self.base,
            package.unwrap_or_else(|| self.layers.get(layer)),
        )
    }

    /// Transitional string-key form for recipes not yet moved to [`Layer`].
    pub fn package_named(&self, default: &str, package: Option<&str>) -> String {
        crate::spec::subpackage(&self.base, package.unwrap_or(self.layers.named(default)))
    }

    /// Whether the resolved pom declares this dependency.
    ///
    /// Reads the cached pom. A renderer asking this question used to call
    /// `pom::read` mid-render, which is what made rendering impure and so made
    /// a path impossible to compute without a body.
    pub fn has_dependency(&self, group_id: &str, artifact_id: &str) -> bool {
        pom::has_dependency(&self.pom, group_id, artifact_id)
    }

    /// True once `add db` has put the JDBC starter on the classpath, which is
    /// the fact that decides transaction boundaries and adapter shape.
    pub fn has_jdbc(&self) -> bool {
        self.has_dependency("org.springframework.boot", "spring-boot-starter-jdbc")
    }

    /// The Spring Boot major version, defaulting to 3 when the parent cannot
    /// be read -- the conservative choice, since pre-4 package names still
    /// exist as deprecated aliases while the 4 ones simply do not exist before 4.
    pub fn boot_major(&self) -> u32 {
        crate::pom::spring_boot_major_of(&self.pom)
    }

    /// `@AutoConfigureMockMvc`'s package, moved in the same Boot 4 change.
    pub fn mockmvc_autoconfigure_import(&self) -> &'static str {
        crate::pom::mockmvc_autoconfigure_import_for(self.boot_major())
    }

    /// The components of a record that already exists in this project.
    ///
    /// Was `fields_from_record(root, pkg, name)` at thirteen call sites that
    /// disagreed about failure. `Project` owns the one window onto disk, so
    /// the recipes above it stay pure. Recipes reach it through
    /// `spring::Slice::record`, which knows which layer owns the resource.
    pub fn record_in(&self, package: &str, ty: &str) -> Option<Vec<crate::spec::Field>> {
        crate::spec::fields_from_record(&self.root, package, ty)
    }

    pub fn main(&self, layer: Layer, package: Option<&str>) -> PathBuf {
        crate::spec::main_dir(&self.root, &self.package(layer, package))
    }

    pub fn test(&self, layer: Layer, package: Option<&str>) -> PathBuf {
        crate::spec::test_dir(&self.root, &self.package(layer, package))
    }

    /// Main/test source roots for a package already resolved by the caller.
    /// Transitional, for recipes mid-move off the layer strings.
    pub fn main_in(&self, package: &str) -> PathBuf {
        crate::spec::main_dir(&self.root, package)
    }

    pub fn test_in(&self, package: &str) -> PathBuf {
        crate::spec::test_dir(&self.root, package)
    }
}

/// Where one generated slice's classes go, and the rule that decides.
///
/// `--package` places the **operation** being generated. The resource that
/// operation targets already exists in the project's configured scaffold
/// layers, so moving the operation must not make jails look for a second copy
/// of the resource in the override package. That rule used to be re-stated at
/// every call site as the difference between `place(layout::WEB)` and
/// `subpackage(&base, config.layer(layout::DOMAIN))`, and the layer names then
/// travelled into each renderer one string at a time -- the Data Clump that
/// gave `spring.rs` sixteen functions of eight to twelve parameters.
///
/// One value carries the project, the override, and the rule.
#[derive(Clone, Copy)]
pub struct Slice<'a> {
    project: &'a Project,
    package: Option<&'a str>,
}

impl<'a> Slice<'a> {
    pub fn new(project: &'a Project, package: Option<&'a str>) -> Self {
        Self { project, package }
    }

    /// The package this slice's own classes go in, honouring `--package`.
    pub fn placed(&self, layer: Layer) -> String {
        self.project.package(layer, self.package)
    }

    /// The package a layer conventionally owns, ignoring `--package`.
    ///
    /// This is where an already-generated resource lives, and it is a
    /// different question from where this slice's classes go.
    pub fn owned(&self, layer: Layer) -> String {
        self.project.package(layer, None)
    }

    /// The application's base package, which is where `add security` writes
    /// `ScopeAuthorizer`.
    pub fn base(&self) -> &'a str {
        self.project.base()
    }

    pub fn project(&self) -> &'a Project {
        self.project
    }

    /// The application's own base package, with `--package` applied.
    ///
    /// Capabilities that write one configuration class per application
    /// (`actuator`, `security`, `cors`, `observability`) live here rather than
    /// in a layer: there is one of each, and it belongs beside the app.
    pub fn root_package(&self) -> String {
        self.project.package_named("", self.package)
    }

    /// The `--package` override itself, for the few helpers that still key
    /// recorded state on it rather than on a resolved package.
    pub fn override_package(&self) -> Option<&'a str> {
        self.package
    }

    /// The components of an already-generated record in its conventional home.
    pub fn record(&self, layer: Layer, ty: &str) -> Option<Vec<crate::spec::Field>> {
        self.project.record_in(&self.owned(layer), ty)
    }

    /// Source roots for this slice's own classes in a layer.
    pub fn main(&self, layer: Layer) -> PathBuf {
        self.project.main(layer, self.package)
    }

    pub fn test(&self, layer: Layer) -> PathBuf {
        self.project.test(layer, self.package)
    }

    /// The project root, for the apply-side helpers that genuinely address
    /// the filesystem rather than plan against it.
    pub fn root(&self) -> &'a Path {
        self.project.root()
    }

    /// The project's build flavour, which decides versioned-vs-managed
    /// dependencies and therefore whether a spliced pom is readable at all.
    pub fn flavor(&self) -> Flavor {
        self.project.flavor()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);

    fn fixture() -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "jails-model-project-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("src/main/java/com/example/demo")).unwrap();
        fs::write(
            root.join("pom.xml"),
            "<project><properties><maven.compiler.release>25</maven.compiler.release></properties></project>\n",
        )
        .unwrap();
        fs::write(
            root.join("src/main/java/com/example/demo/App.java"),
            "package com.example.demo;\npublic final class App {}\n",
        )
        .unwrap();
        fs::write(
            root.join("jails.toml"),
            "[layout]\ndomain = \"model\"\n\n[project]\ncapabilities = [\"json\"]\n",
        )
        .unwrap();
        root
    }

    #[test]
    fn resolves_project_facts_once_into_values() {
        let root = fixture();
        let project = Project::load(&root).unwrap();
        assert_eq!(project.root(), root);
        assert_eq!(project.base(), "com.example.demo");
        assert_eq!(project.flavor(), Flavor::PlainMaven);
        assert_eq!(project.java_release(), Some(25));
        assert_eq!(project.layers().get(Layer::Domain), "model");
        assert_eq!(project.capabilities(), &["json"]);
        assert_eq!(
            project.main(Layer::Domain, None),
            root.join("src/main/java/com/example/demo/model")
        );
    }

    #[test]
    fn change_merge_deduplicates_equal_contributions() {
        let path = PathBuf::from("src/main/java/Thing.java");
        let first = Change {
            files: vec![Artifact::rendered(path.clone(), "same\n".to_string())],
            properties: vec!["feature.enabled=true".to_string()],
            ..Change::default()
        };
        let second = Change {
            files: vec![Artifact::rendered(path, "same\n".to_string())],
            properties: vec!["feature.enabled=true".to_string()],
            ..Change::default()
        };
        let merged = first.merge(second).unwrap();
        assert_eq!(merged.files.len(), 1);
        assert_eq!(merged.properties.len(), 1);
    }

    #[test]
    fn change_merge_refuses_two_bodies_for_one_path() {
        let path = PathBuf::from("src/main/java/Thing.java");
        let first = Change {
            files: vec![Artifact::rendered(path.clone(), "one\n".to_string())],
            ..Change::default()
        };
        let second = Change {
            files: vec![Artifact::rendered(path, "two\n".to_string())],
            ..Change::default()
        };
        let error = first.merge(second).unwrap_err();
        assert!(error.contains("different contents"), "{error}");
    }
}
