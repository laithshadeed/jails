//! Pure project and intent values shared by planning, rendering, and apply.
//!
//! The command modules used to pass a bare `root: &Path` and rediscover the
//! POM, flavor, base package, configured layers, and installed capabilities at
//! every layer of the call graph. `Project` is the single resolved snapshot
//! handed to planners instead. It is deliberately loaded at the CLI boundary;
//! planning code must not reach back into the filesystem for facts already
//! represented here.

use jails_protocol::identity::ProjectPath;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::build::Build;
use crate::compose::Service as ComposeService;
use crate::config::Config;
use crate::pom::{self, Dependency, Flavor};
use jails_support::Result;

/// What a project calls its wire properties.
///
/// Two answers, because Boot's Jackson auto-configuration offers one knob that
/// matters here and a project either turned it on or did not. Anything jails
/// cannot read is `AsWritten`, which is what every project got before this
/// existed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WireNaming {
    /// The component name, as the field spec spells it.
    AsWritten,
    /// `snake_case`, because `spring.jackson.property-naming-strategy` says so.
    SnakeCase,
}

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
    /// The build features this change needs, each with its Maven rendering.
    ///
    /// Keyed by what the build has to *do* rather than by the Maven plugin
    /// that does it — `pending.md` §3. It was `(&'static str, String)`, an
    /// artifact id and an XML block, which meant a Gradle project's claim was
    /// filed under a plugin it does not have, and two places had to map the
    /// coordinate back onto its purpose before they could act.
    pub plugins: Vec<(jails_protocol::feature::BuildFeature, String)>,
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
    /// splices `commands.put(...)` into a dispatcher it does not own, and V1
    /// did it with a `std::fs` call after the plan -- so the routes wrote the
    /// command class and left it unreachable.
    pub registrations: Vec<CommandRegistration>,
    /// Marked blocks in files this change does not own whole.
    ///
    /// `src/test/resources/config/application.properties` is the case: one
    /// durable job's scheduler limits, in a file every other durable job also
    /// writes into and the reader may add to. It was a side effect of the V1
    /// write path -- a `std::fs` call after the plan, outside the `Change` --
    /// so a route planning from the same recipe simply did not know about it,
    /// and the file stopped being generated.
    pub marked: Vec<MarkedBlock>,
    /// The class this change wants the packaged jar to start.
    ///
    /// `generate cli` writes a second dispatcher, and Maven still starts
    /// whichever class the POM names -- so without this a manifest that
    /// generated a CLI and registered its commands into it produced a jar
    /// answering only `help`. Stated as an intent rather than performed,
    /// because V1 performed it with a `std::fs` write after the plan and the
    /// routes therefore never knew the entry point had moved.
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
    pub dispatcher: jails_protocol::identity::JavaType,
    pub command: jails_protocol::identity::JavaType,
}

impl CommandRegistration {
    /// From two qualified names, which is what a recipe has.
    pub fn parse(dispatcher: &str, command: &str) -> Result<Self> {
        Ok(Self {
            dispatcher: jails_protocol::identity::JavaType::parse(dispatcher)?,
            command: jails_protocol::identity::JavaType::parse(command)?,
        })
    }
}

/// One `# jails:<marker>` block, as a change states it.
///
/// Keyed by path and marker rather than by content, because that is what makes
/// removal exact: two durable jobs write two blocks into one file, and taking
/// one out must leave the other and anything the reader put between them.
///
/// `settings` is a list of lines and deliberately not a body. abstract.md §4.1
/// allows exactly one struct to carry the bytes of a file, and that is
/// [`Artifact`]; this is a fragment *inside* somebody else's file, and holding
/// its content as lines is what keeps it structurally incapable of being
/// mistaken for the other thing. Every marked block jails writes is a list of
/// settings, so nothing is lost by saying so.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkedBlock {
    pub path: String,
    pub marker: String,
    pub settings: Vec<String>,
}

impl MarkedBlock {
    /// The block's content, as `codemod` renders it between the markers.
    pub fn rendered(&self) -> String {
        let mut out = String::new();
        for line in &self.settings {
            out.push_str(line);
            out.push('\n');
        }
        out
    }
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
                        "conflicting `# jails:{}` block plans for {}",
                        block.marker, block.path
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

    /// Whether a change should plan to add this dependency.
    ///
    /// **Reads may be optimistic here, because the write is authoritative.**
    /// Planning to add something already present is a no-op -- both splices
    /// return "nothing to do" -- and planning to add it into a build file
    /// jails cannot read is a *refusal* raised by the splice, naming the file.
    /// So an unreadable build resolves to "plan it" and the honest error
    /// arrives from the one place that can be sure, rather than from a guess
    /// made here.
    ///
    /// The opposite composition is what would hurt: treating "cannot read" as
    /// "already there" silently drops a dependency the generated code needs,
    /// and the reader meets it as a compile error in a file they did not write.
    pub fn lacks_dependency(&self, group_id: &str, artifact_id: &str) -> bool {
        self.declares_dependency(group_id, artifact_id) != Some(true)
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
        // The build file this project actually has, not `pom.xml` unconditionally.
        // `load` and `inspect` both go through `read_build_file` for exactly
        // this reason and `projected` did not, so a projected Gradle project
        // read its Groovy through the POM parser: `flavor` came back
        // `PlainMaven` and `release_level` came back `None`, on a project whose
        // live value had both right. That is why `jails app apply` refused
        // every Spring capability on a Gradle build with "this is a plain Maven
        // project" while `jails about` on the same directory said Spring Boot.
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
        // not a difference, and is asked once so the two cannot drift --
        // `inspect` reading `pom.xml` unconditionally is what made `doctor`
        // report "build.gradle is missing" about a file that was right there.
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

    /// The class the packaged artifact starts, if this build names one.
    ///
    /// Dispatched on the build tool, because [`Self::pom`] returns whichever
    /// build file the project has. `pom::main_class` handed `build.gradle`
    /// finds no `<mainClass>` element and answers `None` -- confidently and
    /// wrongly, which is the failure shape `read_build_file`'s doc comment
    /// already records twice and `pending.md` §1.2 predicted a fourth of. It
    /// was this: `g cli` on a Gradle project silently declined to retarget the
    /// entry point, because declining is what "no entry point declared" means.
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

    /// Transitional string-key form for recipes not yet moved to [`Layer`].
    pub fn package_named(&self, default: &str, package: Option<&str>) -> String {
        resolve_package(&self.base, package.unwrap_or(self.layers.named(default)))
    }

    /// Whether this project is known to declare a dependency.
    ///
    /// **`Some(true)` and nothing else.** This read `self.pom` as XML whatever
    /// the build tool was, so on a Gradle project it answered a confident
    /// *no* to every question -- and the consequences were not small: the
    /// scaffold's repository bean became the in-memory one while a query's
    /// adapter read the real table, so a generated project wrote to a HashMap
    /// and read from an empty database. Both halves ran, neither complained,
    /// and the list simply came back empty.
    ///
    /// "Cannot tell" stays *no* here, which is where it was already: a Gradle
    /// file this module cannot read is one jails must not claim things about.
    /// What changes is that a file it *can* read is now believed.
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
    /// Postgres is the answer when neither is there. It is what `README.md`
    /// documents, and a DDL guessed toward the smaller dialect would be
    /// silently narrower than the project the reader is building.
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
        // the narrower name reported "no JDBC" on a project built entirely on
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
    /// so every Boot-4-only artifact and property name jails picks off this
    /// answer was picked correctly by accident, and a Boot 2.7 Gradle build got
    /// the same answer as a Boot 4 one.
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

    /// The components of a record that already exists in this project.
    ///
    /// Was `fields_from_record(root, pkg, name)` at thirteen call sites that
    /// disagreed about failure. `Project` owns the one window onto disk, so
    /// the recipes above it stay pure. Recipes reach it through
    /// `spring::Slice::record`, which knows which layer owns the resource.
    pub fn record_in(&self, package: &str, ty: &str) -> Option<Vec<crate::spec::Field>> {
        crate::spec::fields_of_record(&self.source_of(package, ty)?)
    }

    /// The source of a type this project owns, through the projection first.
    ///
    /// The one window, widened from "the components of a record" to "the
    /// text", because an aggregate transition has more than one question to
    /// ask about a type that exists only in the plan -- an enum's first
    /// constant is the other one.
    pub fn source_of(&self, package: &str, ty: &str) -> Option<String> {
        let relative = format!("src/main/java/{}/{ty}.java", package.replace('.', "/"));
        self.projected_text(&relative)
    }

    /// Whether this project has a type at all, as the plan leaves it.
    ///
    /// A recipe that checks `Path::is_file` instead refuses a manifest row
    /// whose prerequisite is two rows above it -- present in the plan, absent
    /// on disk, and about to be written by the same commit.
    pub fn has_type(&self, package: &str, ty: &str) -> bool {
        self.source_of(package, ty).is_some()
    }

    /// Whether a type this project owns is an enum.
    ///
    /// Through the same window as [`Project::record_in`], and for the same
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
    pub fn projected_text(&self, path: &str) -> Option<String> {
        let projected = self.overlay.as_ref().and_then(|overlay| {
            let key = ProjectPath::parse(path).ok()?;
            let bytes = overlay.get(&key)?;
            String::from_utf8(bytes.clone()).ok()
        });
        projected.or_else(|| std::fs::read_to_string(self.root.join(path)).ok())
    }

    /// What this project calls its JSON properties on the wire.
    ///
    /// **Read off the property that actually decides it**, never configured
    /// twice: `spring.jackson.property-naming-strategy` is what Boot hands the
    /// mapper, so a project that says `SNAKE_CASE` there is a project whose
    /// wire is snake_case, and jails does not need to be told again. Same rule
    /// as `sql_dialect`, which reads the driver rather than the manifest.
    ///
    /// It matters beyond JSON: Spring's *data binder* has no naming strategy,
    /// so a form post at a `@ModelAttribute` endpoint binds by the component
    /// name unless each one carries `@BindParam`. Without this, a project
    /// whose JSON is `user_id` would have its form fields silently arrive as
    /// `null` -- the same value on the wire reaching two different names.
    pub fn wire_naming(&self) -> WireNaming {
        match self
            .projected_text("src/main/resources/application.properties")
            .and_then(|text| {
                text.lines().rev().find_map(|line| {
                    line.trim()
                        .strip_prefix("spring.jackson.property-naming-strategy")?
                        .trim_start()
                        .strip_prefix('=')
                        .map(|value| value.trim().to_string())
                })
            })
            .as_deref()
        {
            Some("SNAKE_CASE") => WireNaming::SnakeCase,
            _ => WireNaming::AsWritten,
        }
    }

    /// Every Java source under `src/main/java`, as the plan leaves them.
    ///
    /// Disk plus the overlay, with the overlay winning. A recipe that has to
    /// *find* something in the project -- the dispatcher a generated command
    /// registers itself in -- cannot walk disk alone: in an aggregate apply
    /// the `g cli` row that creates the dispatcher and the `g command` row
    /// that registers into it are one transition, and the file the second
    /// needs has not been written when the second plans.
    /// A map rather than a list of pairs: the two halves are a path and its
    /// text, and abstract.md §4.1's fourth shape is exactly a positional pair
    /// of those two that compiles when you swap them.
    pub fn projected_main_sources(&self) -> BTreeMap<PathBuf, String> {
        self.projected_sources("src/main/java")
    }

    /// Whether this project declares a top-level type by that simple name.
    ///
    /// Read through the projection, so a transaction that is *about* to write
    /// the type sees it -- the same rule every other planning read follows.
    /// Used to ask whether a capability's own class is present: `add api`
    /// installs a sealed `ApiException` and an exhaustive handler, and until
    /// something threw one the whole RFC 9457 surface was unreachable code.
    pub fn declares_type(&self, name: &str) -> bool {
        self.projected_main_sources()
            .values()
            .filter_map(|source| crate::java::type_info(source))
            .any(|info| info.name == name)
    }

    /// The same, for the test tree.
    pub fn projected_test_sources(&self) -> BTreeMap<PathBuf, String> {
        self.projected_sources("src/test/java")
    }

    fn projected_sources(&self, tree: &str) -> BTreeMap<PathBuf, String> {
        let root = self.root.join(tree);
        let mut found: BTreeMap<PathBuf, String> = BTreeMap::new();
        for path in crate::java::source_files(&root) {
            if let Ok(text) = std::fs::read_to_string(&path) {
                found.insert(path, text);
            }
        }
        let prefix = format!("{tree}/");
        for (path, bytes) in self.overlay.iter().flat_map(|overlay| overlay.iter()) {
            if !path.as_str().starts_with(&prefix) || !path.as_str().ends_with(".java") {
                continue;
            }
            if let Ok(text) = String::from_utf8(bytes.clone()) {
                found.insert(self.root.join(path.as_str()), text);
            }
        }
        found
    }

    /// Every file name directly under a project-relative directory, as the
    /// plan will leave it.
    ///
    /// Disk plus this transition's own writes. A recipe that listed only disk
    /// could not see a file an earlier row of the same `app apply` is about to
    /// write -- which is how two migrations in one transition both came out
    /// numbered `V001`, and one of them then vanished.
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

    /// Whether this project-relative directory exists, as the plan will leave
    /// it.
    ///
    /// A directory an earlier row of the same transition creates counts. It is
    /// the same question `projected_names_in` answers, asked of the directory
    /// rather than of its contents.
    pub fn has_directory(&self, relative: &str) -> bool {
        self.root.join(relative).is_dir() || !self.projected_names_in(relative).is_empty()
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

fn resolve_package(base: &str, requested: &str) -> String {
    let prefix = format!("{base}.");
    if requested == base || requested.starts_with(&prefix) {
        requested.to_string()
    } else {
        crate::spec::subpackage(base, requested)
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

/// The build file's text, or an empty string when this build has none jails
/// reads.
///
/// The single owner of "which file is the build file". Both constructors go
/// through it, because they had drifted: `load` learned to read `build.gradle`
/// and `inspect` did not, so `doctor` -- which uses `inspect` -- reported a
/// Gradle project as having no build file at all.
/// Which file a build's dependencies are declared in.
///
/// Split out of `read_build_file` so the projection can name the same path
/// without touching disk -- a projected project reads its build file out of the
/// overlay, and asking for the wrong name there is indistinguishable from the
/// plan not having touched it.
fn build_file_name(build: Build) -> &'static str {
    match build {
        Build::Gradle => crate::gradle::FILE,
        _ => "pom.xml",
    }
}

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
