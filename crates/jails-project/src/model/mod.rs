//! Pure project and intent values shared by planning, rendering, and apply.
//!
//! A bare `root: &Path` threaded through a call graph lets every level
//! rediscover the POM, flavor, base package, configured layers and installed
//! capabilities, differently. `Project` is the single resolved snapshot
//! handed to planners instead. It is deliberately loaded at the CLI boundary;
//! planning code must not reach back into the filesystem for facts already
//! represented here.

use jails_support::identity::ProjectPath;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::build::Build;
use crate::compose::Service as ComposeService;
use crate::config::Config;
use crate::pom::{self, Dependency, Flavor};
use jails_support::Result;

/// One file a recipe intends to create, with its bytes already rendered.
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

impl SpringTestImport {}

/// Everything one recipe intends to change, computed before it is applied.
///
/// This is the shared command value. Capabilities use all of it; generators
/// initially use the file subset and then migrate their dependency/codemod
/// tails into the same value.
#[derive(Clone, Debug, Default)]
pub struct Change {
    pub deps: Vec<Dependency>,
    /// The build features this change needs, each with its Maven rendering.
    ///
    /// Keyed by what the build has to *do* rather than by the Maven plugin
    /// that does it: keyed by artifact id, a Gradle project's claim is filed
    /// under a plugin it does not have, and every consumer has to map the
    /// coordinate back onto its purpose before it can act.
    pub plugins: Vec<(crate::feature::BuildFeature, String)>,
    pub files: Vec<Artifact>,
    pub compose: Vec<ComposeService>,
    pub properties: Vec<String>,
    /// Settings for `src/test/resources/config/application.properties`.
    ///
    /// A separate list rather than a path on each line, because there is
    /// exactly one other file a setting can go in and the reason is a
    /// mechanism rather than a preference: `classpath:/config/` outranks
    /// `classpath:/` **and is additive**, so one key here overrides one key
    /// there and leaves the rest of the application's configuration standing.
    /// `src/test/resources/application.properties` -- the spelling people
    /// reach for -- shadows the main file wholesale instead, which silently
    /// unsets everything the tests did not restate.
    pub test_properties: Vec<String>,
    pub legacy_deps: Vec<Dependency>,
    pub spring_test_import: Option<SpringTestImport>,
    /// The dispatcher lines this change registers.
    ///
    /// Same shape as the marked block below and the same reason: `g command`
    /// splices `commands.put(...)` into a dispatcher it does not own, and a
    /// splice performed outside the plan leaves the command class written and
    /// unreachable.
    pub registrations: Vec<CommandRegistration>,
    /// Marked blocks in files this change does not own whole.
    ///
    /// `src/test/resources/config/application.properties` is the case: one
    /// durable job's scheduler limits, in a file every other durable job also
    /// writes into and the reader may add to. Stated in the change so a
    /// planner reading the same recipe knows about it.
    pub marked: Vec<MarkedBlock>,
    /// The class this change wants the packaged jar to start.
    ///
    /// `generate cli` writes a second dispatcher, and Maven still starts
    /// whichever class the POM names -- so without this a manifest that
    /// generated a CLI and registered its commands into it produces a jar
    /// answering only `help`. Stated as an intent rather than performed, so
    /// the plan knows the entry point moved.
    ///
    /// Fully qualified, and `None` whenever the recipe decided not to claim
    /// it: a POM naming no entry point at all is a Spring Boot project, and a
    /// POM naming a class somebody chose is their decision.
    pub main_class: Option<String>,
}

/// One command, registered in one dispatcher.
///
/// Both are types rather than paths, because that is what the recorded claim
/// is keyed by -- a dispatcher that moves package is a different registration,
/// and a path would make it look like the same one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandRegistration {
    pub dispatcher: jails_support::identity::JavaType,
    pub command: jails_support::identity::JavaType,
}

impl CommandRegistration {}

/// One `# jails:<marker>` block, as a change states it.
///
/// Keyed by path and marker rather than by content, because that is what makes
/// removal exact: two durable jobs write two blocks into one file, and taking
/// one out must leave the other and anything the reader put between them.
///
/// `settings` is a list of lines and deliberately not a body. Exactly one
/// struct carries the bytes of a file, and that is [`Artifact`]; this is a
/// fragment *inside* somebody else's file, and holding its content as lines
/// is what keeps it structurally incapable of being mistaken for the other
/// thing. Every marked block jails writes is a list of settings, so nothing
/// is lost by saying so.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkedBlock {
    pub path: String,
    pub marker: String,
    pub settings: Vec<String>,
}

impl MarkedBlock {}

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
                    )
                    .into());
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
                    return Err(format!("conflicting plugin plans for {artifact_id}").into());
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
                    )
                    .into());
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
                    return Err(
                        format!("conflicting compose service plans for {}", service.name).into(),
                    );
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
        for registration in other.registrations {
            if !self.registrations.contains(&registration) {
                self.registrations.push(registration);
            }
        }
        for block in other.marked {
            match self
                .marked
                .iter()
                .find(|current| current.path == block.path && current.marker == block.marker)
            {
                Some(current) if current.settings != block.settings => {
                    return Err(format!(
                        "conflicting `{}{}` block plans for {}",
                        jails_codemod::Marked::OPEN_PREFIX,
                        block.marker,
                        block.path
                    )
                    .into());
                }
                Some(_) => {}
                None => self.marked.push(block),
            }
        }
        self.main_class = match (self.main_class, other.main_class) {
            (Some(current), Some(next)) if current != next => {
                return Err(format!(
                    "two recipes would point the packaged jar at different classes: {current} and \
                     {next}"
                )
                .into());
            }
            (Some(current), _) => Some(current),
            (None, next) => next,
        };
        self.spring_test_import = match (self.spring_test_import, other.spring_test_import) {
            (Some(current), Some(next)) if current != next => {
                return Err(jails_support::Failure::Told(
                    "two recipes require different Spring test imports".to_string(),
                ));
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

    /// The string-key form, for recipe code expressed with the public layer
    /// key strings. Keeping it here makes the configuration decision a secret
    /// of `Layers`.
    pub fn named<'a>(&'a self, default: &'a str) -> &'a str {
        self.packages
            .get(default)
            .map(String::as_str)
            .unwrap_or(default)
    }
}

/// What an unreadable build file means. See `Project::boot_major`.
const DEFAULT_BOOT_MAJOR: u32 = 3;

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
    /// The same list with the parameters each capability was declared with.
    ///
    /// Kept beside `installed` rather than replacing it: a label is what most
    /// readers want and two named instances of one capability are two
    /// declarations and one label, so deriving either from the other at the
    /// call site is how the two counts come to disagree.
    declared: Vec<crate::capability::Declaration>,
    build: crate::build::Build,
    /// Files this project has, as a plan says it will have them.
    ///
    /// `Project` is the one window onto disk, which is what keeps the recipes
    /// above it pure -- so it is also the one place a *projected* tree can be
    /// substituted for the live one. An aggregate `app apply` generates a
    /// scaffold and then a search over it in a single transition, and the
    /// second recipe has to see the first one's record without either having
    /// been written yet.
    ///
    /// `None` is the live tree, which is every ordinary command.
    overlay: Option<std::sync::Arc<BTreeMap<ProjectPath, Vec<u8>>>>,
}

impl Project {
    /// Whether this project is known to declare a dependency already.
    ///
    /// `None` is a real answer and means *the build file says something jails
    /// cannot read*, which a Gradle build can do and a POM cannot. It is
    /// deliberately not collapsed here, so a caller that needs certainty can
    /// ask for it.
    pub fn declares_dependency(&self, group_id: &str, artifact_id: &str) -> Option<bool> {
        match self.build {
            crate::build::Build::Gradle => {
                crate::gradle::has_dependency(&self.pom, group_id, artifact_id)
            }
            _ => Some(pom::has_dependency(&self.pom, group_id, artifact_id)),
        }
    }

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
        let pom = read_build_file(build, root)?;
        let config = Config::load(root)?;
        Ok(Self {
            root: root.to_path_buf(),
            base: crate::spec::base_package(root)?,
            flavor: build_flavor(build, &pom),
            java_release: build_release_level(build, &pom),
            layers: Layers::from_config(&config),
            installed: config.capabilities().to_vec(),
            declared: config.declarations().to_vec(),
            overlay: None,
            build,
            pom,
        })
    }

    /// The same project, as a plan says it will be.
    ///
    /// Every fact `load` reads off disk is read out of the projection instead:
    /// the POM, the human config, and the source files a later recipe in the
    /// same transition has to see. That is what makes one aggregate `app
    /// apply` possible at all -- `add db` puts the JDBC starter in the POM and
    /// `g search` refuses without it, and in one transition neither has been
    /// written when the second one plans.
    ///
    /// The overlay is consulted *before* disk and only for paths the plan
    /// touched, so a file nothing changed is still whatever is there. Reading
    /// past it would decide on a fact the snapshot did not record, which is
    /// the thing the whole capture exists to prevent.
    pub fn projected(live: &Self, overlay: BTreeMap<ProjectPath, Vec<u8>>) -> Result<Self> {
        let text = |path: &str| -> Option<String> {
            let key = ProjectPath::parse(path).ok()?;
            String::from_utf8(overlay.get(&key)?.clone()).ok()
        };
        // Nothing is re-read from disk. A path the plan touched comes from the
        // overlay and every other fact is the resolved value `live` already
        // holds -- which is what a projection *is*, and is why this takes a
        // project rather than a root.
        // The build file this project actually has, not `pom.xml`
        // unconditionally: a projected Gradle project read through the POM
        // parser comes back `PlainMaven` with no release, and `jails app apply`
        // then refuses every Spring capability on a build `jails about` calls
        // Spring Boot.
        let pom = text(build_file_name(live.build)).unwrap_or_else(|| live.pom.clone());
        let (layers, installed, declared) = match text("jails.toml") {
            Some(projected) => {
                let config = Config::parse(&projected)?;
                (
                    Layers::from_config(&config),
                    config.capabilities().to_vec(),
                    config.declarations().to_vec(),
                )
            }
            None => (
                live.layers.clone(),
                live.installed.clone(),
                live.declared.clone(),
            ),
        };
        Ok(Self {
            root: live.root.clone(),
            // The base package cannot move within a transition: nothing jails
            // writes renames the application class, and a manifest that did
            // would be describing a different project.
            base: live.base.clone(),
            flavor: build_flavor(live.build, &pom),
            java_release: build_release_level(live.build, &pom),
            layers,
            installed,
            declared,
            overlay: Some(std::sync::Arc::new(overlay)),
            build: live.build,
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
        let build = crate::build::detect(root);
        // Unreadable is tolerated here and fatal in `load`, which is the one
        // deliberate difference between the two: doctor's whole value is that
        // it works on a project that does not build. *Which* file to read is
        // not a difference, and is asked once so the two cannot drift -- an
        // `inspect` reading `pom.xml` unconditionally has `doctor` report
        // "build.gradle is missing" about a file that is right there.
        let pom = read_build_file(build, root).unwrap_or_default();
        let config = Config::load(root)?;
        Ok(Self {
            root: root.to_path_buf(),
            base: crate::spec::base_package(root).unwrap_or_default(),
            flavor: build_flavor(build, &pom),
            java_release: build_release_level(build, &pom),
            layers: Layers::from_config(&config),
            installed: config.capabilities().to_vec(),
            declared: config.declarations().to_vec(),
            overlay: None,
            build,
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

    /// Does this project hold an editable application model?
    ///
    /// A fact about the project rather than a path a caller re-derives.
    /// `doctor`'s capability check needs it: it reads `jails.toml`, and a
    /// modelled project records its capabilities in the model instead, so
    /// without this it reports "records none -- nothing to reconcile" about a
    /// project whose model declares them.
    ///
    /// Recognised by the file rather than by an import: this crate does not
    /// depend on the compiler ladder, and which file holds a project's
    /// declarations is the one fact about it a reader-facing report needs.
    pub fn is_modelled(&self) -> bool {
        self.root.join(".jails/model.jdl").is_file()
    }

    /// What builds this project.
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

    /// The class the packaged artifact starts, if this build names one.
    ///
    /// Dispatched on the build tool, because [`Self::pom`] returns whichever
    /// build file the project has. `pom::main_class` handed `build.gradle`
    /// finds no `<mainClass>` element and answers `None` -- confidently and
    /// wrongly -- and `g cli` on a Gradle project then silently declines to
    /// retarget the entry point, because declining is what "no entry point
    /// declared" means.
    pub fn main_class(&self) -> Option<String> {
        match self.build {
            crate::build::Build::Gradle => crate::gradle::main_class(&self.pom),
            _ => crate::pom::main_class(&self.pom).map(str::to_string),
        }
    }

    pub fn layers(&self) -> &Layers {
        &self.layers
    }

    pub fn capabilities(&self) -> &[String] {
        &self.installed
    }

    /// Every capability this project declares, with the parameters it was
    /// declared with -- the view anything forming an identity needs.
    pub fn declarations(&self) -> &[crate::capability::Declaration] {
        &self.declared
    }

    /// Resolve a package override, or the configured conventional layer.
    pub fn package(&self, layer: Layer, package: Option<&str>) -> String {
        resolve_package(
            &self.base,
            package.unwrap_or_else(|| self.layers.get(layer)),
        )
    }

    /// The string-key form of [`Self::package`].
    pub fn package_named(&self, default: &str, package: Option<&str>) -> String {
        resolve_package(&self.base, package.unwrap_or(self.layers.named(default)))
    }

    /// Whether this project is known to declare a dependency.
    ///
    /// **`Some(true)` and nothing else.** Reading `self.pom` as XML whatever
    /// the build tool is answers a confident *no* to every question on a
    /// Gradle project, and the consequences are not small: the scaffold's
    /// repository bean becomes the in-memory one while a query's adapter reads
    /// the real table, so a generated project writes to a HashMap and reads
    /// from an empty database. Both halves run, neither complains, and the
    /// list simply comes back empty.
    ///
    /// "Cannot tell" is *no* here: a Gradle file this module cannot read is
    /// one jails must not claim things about.
    pub fn has_dependency(&self, group_id: &str, artifact_id: &str) -> bool {
        self.declares_dependency(group_id, artifact_id) == Some(true)
    }

    /// True once `add db` has put the JDBC starter on the classpath, which is
    /// the fact that decides transaction boundaries and adapter shape.
    /// Which SQL this project's generated DDL should be written in.
    ///
    /// Read off the **driver**, not off `jails.toml`: a manifest is a record
    /// of what was asked for and a driver is a fact about what the schema will
    /// actually meet. Postgres wins when both are present, because that is the
    /// database `add db` migrates with Flyway and H2 is then the test double
    /// somebody added beside it.
    ///
    /// Postgres is the answer when neither is there: it is the documented
    /// default, and a DDL guessed toward the smaller dialect would be silently
    /// narrower than the project the reader is building.
    pub fn sql_dialect(&self) -> jails_spec::spec::kind::Dialect {
        use jails_spec::spec::kind::Dialect;
        match self.has_dependency("com.h2database", "h2")
            && !self.has_dependency("org.postgresql", "postgresql")
        {
            true => Dialect::H2,
            false => Dialect::Postgres,
        }
    }

    pub fn has_jdbc(&self) -> bool {
        // Either starter. `spring-boot-starter-data-jdbc` declares
        // `api(spring-boot-starter-jdbc)` -- verified in `deps/spring-boot` --
        // so a project with it has `JdbcClient`, the auto-configured
        // `DataSource` and everything else this answer decides. Matching only
        // the narrower name reports "no JDBC" on a project built entirely on
        // Spring Data JDBC.
        self.has_dependency("org.springframework.boot", "spring-boot-starter-jdbc")
            || self.has_dependency("org.springframework.boot", "spring-boot-starter-data-jdbc")
    }

    /// The Spring Boot major version, defaulting to 3 when the build file
    /// cannot be read -- the conservative choice, since pre-4 package names
    /// still exist as deprecated aliases while the 4 ones simply do not exist
    /// before 4.
    ///
    /// Routed by build kind for the same reason `build_flavor` and
    /// `build_release_level` are: on a Gradle project `self.pom` holds
    /// `build.gradle`, and handing Groovy to a reader looking for
    /// `<artifactId>spring-boot-starter-parent</artifactId>` finds nothing and
    /// returns the default. That is not "unknown", it is **3, confidently** --
    /// and a Boot 2.7 Gradle build then gets the same answer as a Boot 4 one.
    pub fn boot_major(&self) -> u32 {
        match self.build {
            crate::build::Build::Gradle => {
                crate::gradle::spring_boot_major(&self.pom).unwrap_or(DEFAULT_BOOT_MAJOR)
            }
            _ => crate::pom::spring_boot_major_of(&self.pom),
        }
    }

    /// This project's Boot `(major, minor)`, when it declares one.
    ///
    /// `None` on a project with no readable Boot version, which every caller
    /// has to treat as "cannot answer" rather than as a number.
    pub fn boot_version(&self) -> Option<(u32, u32)> {
        match self.build {
            crate::build::Build::Gradle => {
                let text = crate::gradle::boot_version(&self.pom)?;
                let mut parts = text.split('.');
                let major = parts.next()?.parse().ok()?;
                let minor = parts.next().and_then(|part| part.parse().ok()).unwrap_or(0);
                Some((major, minor))
            }
            _ => crate::pom::spring_boot_version_of(&self.pom),
        }
    }

    /// `@AutoConfigureMockMvc`'s package, moved in the same Boot 4 change.
    pub fn mockmvc_autoconfigure_import(&self) -> &'static str {
        crate::pom::mockmvc_autoconfigure_import_for(self.boot_major())
    }

    /// The `@WebMvcTest` import this project's Boot version has.
    pub fn webmvc_test_import(&self) -> &'static str {
        crate::pom::webmvc_test_import_for(self.boot_major())
    }

    /// The source of a type this project owns, through the projection first.
    ///
    /// The one window, and it hands back the text rather than the components
    /// of a record, because an aggregate transition has more than one question
    /// to ask about a type that exists only in the plan -- an enum's first
    /// constant is the other one.
    pub(crate) fn source_of(&self, package: &str, ty: &str) -> Option<String> {
        let relative = format!("src/main/java/{}/{ty}.java", package.replace('.', "/"));
        self.projected_text(&relative)
    }

    /// Whether a type this project owns is an enum.
    ///
    /// Through the same window as `source_of`, and for the same
    /// reason: a manifest declares an enum and then a scaffold whose column
    /// is that enum, and in one transition neither has been written when the
    /// second one plans. Reading past the projection would refuse a manifest
    /// that is perfectly well ordered.
    pub fn declares_enum(&self, package: &str, ty: &str) -> bool {
        self.source_of(package, ty)
            .is_some_and(|text| crate::java::blanked(&text).contains(&format!("enum {ty}")))
    }

    /// A file's text as the plan leaves it, when this project is projected.
    ///
    /// The projection wins for a path it changes; every other path is read
    /// from disk. This is public because SQL generators need the same truthful
    /// view as Java generators when an earlier manifest row creates a
    /// migration that a later row must inspect.
    pub(crate) fn projected_text(&self, path: &str) -> Option<String> {
        let projected = self.overlay.as_ref().and_then(|overlay| {
            let key = ProjectPath::parse(path).ok()?;
            let bytes = overlay.get(&key)?;
            String::from_utf8(bytes.clone()).ok()
        });
        projected.or_else(|| std::fs::read_to_string(self.root.join(path)).ok())
    }

    /// Every file name directly under a project-relative directory, as the
    /// plan will leave it.
    ///
    /// Disk plus this transition's own writes. A recipe that lists only disk
    /// cannot see a file an earlier row of the same `app apply` is about to
    /// write, so two migrations in one transition both come out numbered
    /// `V001` and one of them vanishes.
    pub fn projected_names_in(&self, relative: &str) -> std::collections::BTreeSet<String> {
        let mut found = std::collections::BTreeSet::new();
        if let Ok(entries) = std::fs::read_dir(self.root.join(relative)) {
            for entry in entries.flatten() {
                found.insert(entry.file_name().to_string_lossy().into_owned());
            }
        }
        let prefix = format!("{}/", relative.trim_end_matches('/'));
        for path in self.overlay.iter().flat_map(|overlay| overlay.keys()) {
            if let Some(rest) = path.as_str().strip_prefix(&prefix)
                && !rest.contains('/')
            {
                found.insert(rest.to_string());
            }
        }
        found
    }

    pub fn main(&self, layer: Layer, package: Option<&str>) -> PathBuf {
        crate::spec::main_dir(&self.root, &self.package(layer, package))
    }

    pub fn test(&self, layer: Layer, package: Option<&str>) -> PathBuf {
        crate::spec::test_dir(&self.root, &self.package(layer, package))
    }
}

fn resolve_package(base: &str, requested: &str) -> String {
    let prefix = format!("{base}.");
    if requested == base || requested.starts_with(&prefix) {
        requested.to_string()
    } else {
        crate::spec::subpackage(base, requested)
    }
}

/// Which file a build's dependencies are declared in.
///
/// Its own function so the projection can name the same path without
/// touching disk -- a projected project reads its build file out of the
/// overlay, and asking for the wrong name there is indistinguishable from the
/// plan not having touched it.
fn build_file_name(build: Build) -> &'static str {
    match build {
        Build::Gradle => crate::gradle::FILE,
        _ => "pom.xml",
    }
}

/// The build file's text, or an empty string when this build has none jails
/// reads.
///
/// The single owner of "which file is the build file". Both constructors go
/// through it, so `load` and `inspect` cannot disagree about whether a Gradle
/// project has a build file at all.
fn read_build_file(build: Build, root: &Path) -> Result<String> {
    match build {
        Build::Maven => pom::read(root),
        Build::Gradle => {
            let path = root.join(crate::gradle::FILE);
            Ok(std::fs::read_to_string(&path)
                .map_err(|error| format!("failed to read {}: {error}", path.display()))?)
        }
        _ => Ok(String::new()),
    }
}

/// Spring Boot's dependency management, whichever build file declares it.
fn build_flavor(build: Build, text: &str) -> Flavor {
    match build {
        Build::Gradle => crate::gradle::flavor(text),
        _ => pom::flavor(text),
    }
}

/// The Java release this project targets, whichever build file states it.
fn build_release_level(build: Build, text: &str) -> Option<u32> {
    match build {
        Build::Gradle => crate::gradle::release_level(text),
        _ => pom::release_level(text),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn fixture() -> PathBuf {
        let root = jails_support::scratch::ScratchDir::in_temp("jails-model-project")
            .unwrap()
            .keep();
        fs::create_dir_all(root.join("src/main/java/com/example/demo")).unwrap();
        fs::write(
            root.join("pom.xml"),
            "<project><properties><maven.compiler.release>21</maven.compiler.release></properties></project>\n",
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
        assert_eq!(project.java_release(), Some(21));
        assert_eq!(project.layers().get(Layer::Domain), "model");
        assert_eq!(project.capabilities(), &["json"]);
        assert_eq!(
            project.main(Layer::Domain, None),
            root.join("src/main/java/com/example/demo/model")
        );
    }

    #[test]
    fn a_fully_qualified_override_is_not_prefixed_with_the_base_again() {
        let root = fixture();
        let project = Project::load(&root).unwrap();
        assert_eq!(
            project.package_named("", Some("billing")),
            "com.example.demo.billing"
        );
        assert_eq!(
            project.package_named("", Some("com.example.demo.billing")),
            "com.example.demo.billing"
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
