//! One resolved project: where it is, what builds it, and what its build states.
//!
//! [`Project`] is a root plus the [`ProjectFacts`] the capture boundary
//! observes -- the same reader `capture` fills a `WorkspaceSnapshot` from --
//! so a command that starts a JVM and a compiler that emits Java read one
//! answer to "what is this project". Nothing here parses a build file: the
//! facts are read once by `capture::observe`, and the captured text of the
//! build file travels with them for the questions `gradle.rs` answers exactly
//! or refuses.
//!
//! `Project` is a parameter object, and the `root: &Path` ratchet measures
//! the absence of one: a root threaded through a call graph lets every level
//! re-derive the same facts, differently. Resolving once and passing the
//! value is the cure.
//!
//! **`base_package()` falls back to the shallowest `.java` file** rather than
//! requiring `*Application.java`, which only Spring projects have; requiring
//! it fails `add` on exactly the projects it is most useful for.

use std::fs;
use std::path::{Path, PathBuf};

use crate::build::Build;
use crate::layout::{Head, Layer};
use crate::pom;
use jails_contracts::ProjectFacts;
use jails_support::Result;

/// One immutable view of a project, resolved once at the command boundary.
#[derive(Clone, Debug)]
pub struct Project {
    root: PathBuf,
    build: Build,
    facts: ProjectFacts,
    /// The build file's text, captured beside the facts.
    ///
    /// Kept for the one reader that answers exactly or refuses: a Gradle
    /// script is asked about a dependency through `gradle.rs`, whose `None`
    /// is a real answer no coordinate set can carry. Empty for a build jails
    /// does not read.
    build_file: String,
}

impl Project {
    /// Resolve project facts exactly once from a known module root.
    ///
    /// Refuses what a generator cannot proceed without: a build file jails
    /// reads but cannot open, or a source tree with no package to write into.
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
        let build_file = read_build_file(build, root)?;
        let facts = crate::capture::facts(root).map_err(crate::diagnosed)?;
        if facts.base_package.is_empty() {
            // The refusal names the tree it looked in.
            crate::spec::base_package(root)?;
        }
        Ok(Self {
            root: root.to_path_buf(),
            build,
            facts,
            build_file,
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
        let build_file = read_build_file(build, root).unwrap_or_default();
        let facts = crate::capture::facts(root).map_err(crate::diagnosed)?;
        Ok(Self {
            root: root.to_path_buf(),
            build,
            facts,
            build_file,
        })
    }

    /// Discover the containing module and resolve it once.
    pub fn discover() -> Result<Self> {
        Self::load(&crate::spec::find_project_root()?)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// What builds this project.
    pub fn build(&self) -> Build {
        self.build
    }

    /// Every fact the capture boundary observed, as the compiler would see it.
    pub fn facts(&self) -> &ProjectFacts {
        &self.facts
    }

    pub fn base(&self) -> &str {
        &self.facts.base_package
    }

    /// Whether Spring Boot's dependency management is what this build uses.
    ///
    /// A `bool` rather than a two-variant enum, because that is all the
    /// question ever was: capabilities wire themselves up one way under Boot
    /// and another without it. Inherited through the reactor, so a module
    /// under a Boot aggregator is a Boot module.
    pub fn is_spring_boot(&self) -> bool {
        self.facts.reactor.spring_boot
    }

    /// The Java release the build states, or `None` when it states none.
    ///
    /// The build's answer rather than the model's: this is what the JDK on
    /// the machine is compared against.
    pub fn java_release(&self) -> Option<u32> {
        self.facts.reactor.java_release.map(u32::from)
    }

    /// The captured text of the build file jails reads, empty for one it
    /// does not.
    pub fn build_file(&self) -> &str {
        &self.build_file
    }

    /// Whether this project is known to declare a dependency already.
    ///
    /// `None` is a real answer and means *the build file says something jails
    /// cannot read*, which a Gradle build can do and a POM cannot. It is
    /// deliberately not collapsed here, so a caller that needs certainty can
    /// ask for it. Jails' own dependency block counts: the question is what
    /// is on the classpath, not who put it there.
    pub fn declares_dependency(&self, group_id: &str, artifact_id: &str) -> Option<bool> {
        match self.build {
            Build::Gradle => crate::gradle::has_dependency(&self.build_file, group_id, artifact_id),
            _ => Some(
                self.facts
                    .build_dependencies
                    .contains(&format!("{group_id}:{artifact_id}")),
            ),
        }
    }

    /// Whether this project is known to declare a dependency.
    ///
    /// **`Some(true)` and nothing else.** "Cannot tell" is *no* here: a Gradle
    /// file jails cannot read is one jails must not claim things about, and
    /// the consequences of claiming are not small -- the scaffold's repository
    /// bean becomes the in-memory one while a query's adapter reads the real
    /// table, so a generated project writes to a HashMap and reads from an
    /// empty database. Both halves run, neither complains.
    pub fn has_dependency(&self, group_id: &str, artifact_id: &str) -> bool {
        self.declares_dependency(group_id, artifact_id) == Some(true)
    }

    /// Whether any declared dependency belongs to `group_id`.
    ///
    /// The question `doctor` asks about Testcontainers, whose 2.0 release
    /// renamed every module: matching on the group alone is the only match
    /// that survives it.
    pub fn declares_group(&self, group_id: &str) -> bool {
        let prefix = format!("{group_id}:");
        self.facts
            .build_dependencies
            .iter()
            .any(|coordinate| coordinate.starts_with(&prefix))
    }

    /// True once `add db` has put the JDBC starter on the classpath, which is
    /// the fact that decides transaction boundaries and adapter shape.
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

    /// The package a layer's code lives in, after `jails.toml`'s renames.
    ///
    /// A rename that is already fully qualified is taken as written; anything
    /// else is a subpackage of the base.
    pub fn package(&self, layer: Layer) -> String {
        let head = self.facts.layout.head(Head::Layer(layer));
        let prefix = format!("{}.", self.base());
        if head == self.base() || head.starts_with(&prefix) {
            head.to_string()
        } else {
            crate::spec::subpackage(self.base(), head)
        }
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
            Ok(fs::read_to_string(&path)
                .map_err(|error| format!("failed to read {}: {error}", path.display()))?)
        }
        _ => Ok(String::new()),
    }
}

/// `jails about`: the project as its build files describe it.
///
/// Starts from the process directory and walks up to the nearest build file
/// jails reads, so it works from any directory below a module. A foreign
/// build on the way up is stepped over rather than reported as the project.
pub fn about(json: bool) -> Result<()> {
    let cwd = std::env::current_dir().map_err(|e| format!("failed to get cwd: {e}"))?;
    let start =
        fs::canonicalize(&cwd).map_err(|e| format!("failed to resolve {}: {e}", cwd.display()))?;
    let root = nearest_build_root(&start)?;
    let project = Project::inspect(&root)?;
    let about = About::of(&project);
    if json {
        println!("{}", about.to_json(&project));
    } else {
        about.print_human();
    }
    Ok(())
}

/// The nearest ancestor holding a build file jails reads.
///
/// Through `build::detect`, so this cannot drift from what every other command
/// considers a project root.
fn nearest_build_root(start: &Path) -> Result<PathBuf> {
    for dir in start.ancestors() {
        if crate::build::is_readable(crate::build::detect(dir)) {
            return Ok(dir.to_path_buf());
        }
    }
    Err(jails_support::Failure::Told(
        "no pom.xml or build.gradle found in this or any parent directory".to_string(),
    ))
}

/// What `about` says, laid out for printing.
struct About {
    reactor_root: PathBuf,
    reactor_label: String,
    module_root: PathBuf,
    module_label: String,
    java_release: Option<u32>,
    spring_boot: bool,
    gradle: bool,
    /// The command that drives this build. Named for the job rather than for
    /// Maven: it is `gradlew` on a Gradle project, and a field called
    /// `maven_command` holding a path to `gradlew` is a lie the JSON then
    /// repeats to every consumer.
    build_command: PathBuf,
    modules: Vec<(String, String)>,
}

impl About {
    fn of(project: &Project) -> Self {
        let facts = project.facts();
        let module_root = project.root().to_path_buf();
        let reactor_root = match facts.reactor.root.is_empty() {
            true => module_root.clone(),
            false => fs::canonicalize(module_root.join(&facts.reactor.root))
                .unwrap_or_else(|_| module_root.clone()),
        };
        let gradle = project.build() == Build::Gradle;
        // A Gradle project with no `settings.gradle` is named after its
        // directory, which is the fallback Gradle itself uses.
        let directory = || {
            gradle
                .then(|| {
                    module_root
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                })
                .flatten()
        };
        let module_label = facts
            .artifact_id
            .clone()
            .or_else(directory)
            .unwrap_or_else(|| "unknown".to_string());
        let reactor_label = match facts.reactor.root.is_empty() {
            true => module_label.clone(),
            false => facts
                .reactor
                .artifact_id
                .clone()
                .unwrap_or_else(|| "unknown".to_string()),
        };
        Self {
            // What `about` reports is what `run`/`test` will actually
            // execute, because it is the same function; two copies disagree
            // about `mvnd.cmd` on Windows.
            build_command: match gradle {
                true => match reactor_root.join("gradlew").is_file() {
                    true => reactor_root.join("gradlew"),
                    false => PathBuf::from("gradle"),
                },
                false => crate::maven::binary(&reactor_root),
            },
            reactor_root,
            reactor_label,
            module_root,
            module_label,
            java_release: project.java_release(),
            spring_boot: project.is_spring_boot(),
            gradle,
            modules: facts
                .reactor
                .modules
                .iter()
                .map(|module| {
                    (
                        module
                            .artifact_id
                            .clone()
                            .unwrap_or_else(|| "unknown".to_string()),
                        module.path.clone(),
                    )
                })
                .collect(),
        }
    }

    fn print_human(&self) {
        println!("Reactor: {}", self.reactor_label);
        println!("  root: {}", self.reactor_root.display());
        println!("Module: {}", self.module_label);
        println!("  root: {}", self.module_root.display());
        println!(
            "Java: {}",
            self.java_release
                .map(|release| release.to_string())
                .unwrap_or_else(|| "unknown".to_string())
        );
        println!(
            "Framework: {}",
            match (self.spring_boot, self.gradle) {
                (true, _) => "Spring Boot",
                (false, true) => "plain Gradle",
                (false, false) => "plain Maven",
            }
        );
        println!(
            "{}: {}",
            match self.gradle {
                true => "Gradle",
                false => "Maven",
            },
            self.build_command.display()
        );
        // Named as unread rather than counted as none: Gradle's multi-project
        // model is `settings.gradle`'s `include` lines, and printing "(none)"
        // for a build with ten of them is the confident wrong answer.
        if self.gradle {
            println!("Modules: not read (Gradle multi-project is `settings.gradle` includes)");
            return;
        }
        println!("Modules ({}):", self.modules.len());
        if self.modules.is_empty() {
            println!("  (none)");
        } else {
            for (label, path) in &self.modules {
                println!("  {label}  {path}");
            }
        }
    }

    fn to_json(&self, project: &Project) -> String {
        let config = crate::config::Config::load(&self.module_root).unwrap_or_default();
        let base_package = Some(project.base()).filter(|base| !base.is_empty());
        let layout = config
            .layout_entries()
            .into_iter()
            .map(|(name, package)| {
                format!(
                    "{}:{}",
                    crate::json::string(name),
                    crate::json::string(&package)
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let capabilities = config
            .capabilities()
            .iter()
            .map(|capability| crate::json::string(capability))
            .collect::<Vec<_>>()
            .join(",");
        let java_root = self.module_root.join("src/main/java");
        let test_root = self.module_root.join("src/test/java");
        let modules = project
            .facts()
            .reactor
            .modules
            .iter()
            .map(|module| {
                format!(
                    "    {{\"artifact_id\": {}, \"path\": {}}}",
                    crate::json::optional_string(module.artifact_id.as_deref()),
                    crate::json::string(&module.path)
                )
            })
            .collect::<Vec<_>>()
            .join(",\n");
        let facts = project.facts();
        let reactor_artifact_id = match facts.reactor.root.is_empty() {
            true => facts.artifact_id.as_deref(),
            false => facts.reactor.artifact_id.as_deref(),
        };

        format!(
            concat!(
                "{{\n",
                "  \"schema_version\": 4,\n",
                "  \"reactor\": {{\"root\": {}, \"artifact_id\": {}}},\n",
                "  \"module\": {{\"root\": {}, \"artifact_id\": {}}},\n",
                "  \"base_package\": {},\n",
                "  \"java_root\": {},\n",
                "  \"test_root\": {},\n",
                "  \"layout\": {{{}}},\n",
                "  \"capabilities\": [{}],\n",
                "  \"java_release\": {},\n",
                "  \"spring_boot\": {},\n",
                "  \"build\": {}, \"build_command\": {},\n",
                "  \"modules\": [\n{}\n  ]\n",
                "}}"
            ),
            crate::json::string(&self.reactor_root.to_string_lossy()),
            crate::json::optional_string(reactor_artifact_id),
            crate::json::string(&self.module_root.to_string_lossy()),
            crate::json::optional_string(facts.artifact_id.as_deref()),
            crate::json::optional_string(base_package),
            crate::json::string(&java_root.to_string_lossy()),
            crate::json::string(&test_root.to_string_lossy()),
            layout,
            capabilities,
            self.java_release
                .map(|release| release.to_string())
                .unwrap_or_else(|| "null".to_string()),
            self.spring_boot,
            crate::json::string(project.build().name()),
            crate::json::string(&self.build_command.to_string_lossy()),
            modules
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_dir(label: &str) -> PathBuf {
        jails_support::scratch::ScratchDir::in_temp(&format!("jails-project-{label}"))
            .unwrap()
            .keep()
    }

    fn write_pom(root: &Path, body: &str) {
        fs::create_dir_all(root).unwrap();
        fs::write(
            root.join("pom.xml"),
            format!("<project><modelVersion>4.0.0</modelVersion>{body}</project>"),
        )
        .unwrap();
    }

    fn fixture() -> PathBuf {
        let root = fixture_dir("facts");
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
        assert!(!project.is_spring_boot());
        assert_eq!(project.java_release(), Some(21));
        assert_eq!(project.package(Layer::Domain), "com.example.demo.model");
        assert_eq!(project.package(Layer::Service), "com.example.demo.service");
        assert_eq!(
            project.facts().build_system,
            jails_contracts::BuildSystem::Maven
        );
    }

    #[test]
    fn a_fully_qualified_rename_is_not_prefixed_with_the_base_again() {
        let root = fixture();
        fs::write(
            root.join("jails.toml"),
            "[layout]\ndomain = \"com.example.demo.billing\"\n",
        )
        .unwrap();
        let project = Project::load(&root).unwrap();
        assert_eq!(project.package(Layer::Domain), "com.example.demo.billing");
    }

    /// Jails' own dependency block is on the classpath, and `doctor` asks
    /// about the classpath; the compiler's reader-only set is a different
    /// question and stays different.
    #[test]
    fn a_dependency_in_jails_own_block_is_declared_for_the_commands_and_not_for_the_compiler() {
        let root = fixture();
        fs::write(
            root.join("pom.xml"),
            "<project><dependencies><!-- jails:dependencies --><dependency><groupId>org.springframework.boot</groupId><artifactId>spring-boot-starter-jdbc</artifactId></dependency><!-- /jails:dependencies --></dependencies></project>\n",
        )
        .unwrap();
        let project = Project::load(&root).unwrap();
        assert!(project.has_jdbc());
        assert!(
            !project
                .facts()
                .dependencies
                .contains("org.springframework.boot:spring-boot-starter-jdbc")
        );
    }

    #[test]
    fn discovers_nested_reactor_and_active_module() {
        let root = fixture_dir("nested-reactor");
        write_pom(
            &root,
            "<groupId>dev.example</groupId><artifactId>sample-parent</artifactId><properties><java.version>26</java.version></properties><dependencyManagement><dependencies><dependency><groupId>org.springframework.boot</groupId><artifactId>spring-boot-dependencies</artifactId></dependency></dependencies></dependencyManagement><modules><module>sample-core</module><module>services</module></modules>",
        );
        write_pom(
            &root.join("sample-core"),
            "<parent><groupId>dev.example</groupId><artifactId>sample-parent</artifactId></parent><artifactId>sample-core</artifactId>",
        );
        write_pom(
            &root.join("services"),
            "<parent><groupId>dev.example</groupId><artifactId>sample-parent</artifactId></parent><artifactId>sample-services</artifactId><modules><module>sample-web</module></modules>",
        );
        write_pom(
            &root.join("services/sample-web"),
            "<parent><groupId>dev.example</groupId><artifactId>sample-services</artifactId></parent><artifactId>sample-web</artifactId>",
        );
        fs::write(root.join("mvnw"), "#!/bin/sh\n").unwrap();
        let module = fs::canonicalize(root.join("services/sample-web")).unwrap();
        fs::create_dir_all(module.join("src/main/java/dev/example")).unwrap();

        let project = Project::inspect(&module).unwrap();
        let reactor = &project.facts().reactor;

        assert_eq!(reactor.root, "../..");
        assert_eq!(reactor.artifact_id.as_deref(), Some("sample-parent"));
        assert_eq!(project.facts().artifact_id.as_deref(), Some("sample-web"));
        assert_eq!(project.java_release(), Some(26));
        assert!(project.is_spring_boot());
        assert_eq!(
            reactor
                .modules
                .iter()
                .map(|module| (module.artifact_id.as_deref(), module.path.as_str()))
                .collect::<Vec<_>>(),
            vec![
                (Some("sample-core"), "sample-core"),
                (Some("sample-services"), "services"),
                (Some("sample-web"), "services/sample-web"),
            ]
        );

        let about = About::of(&project);
        assert_eq!(about.reactor_root, fs::canonicalize(&root).unwrap());
        assert_eq!(about.reactor_label, "sample-parent");
        assert_eq!(about.module_label, "sample-web");
        assert_eq!(
            about.build_command,
            fs::canonicalize(&root).unwrap().join("mvnw")
        );
        let json = about.to_json(&project);
        assert!(json.contains("\"schema_version\": 4"), "{json}");
        assert!(json.contains("\"reactor\":"), "{json}");
        assert!(
            json.contains("\"artifact_id\": \"sample-parent\""),
            "{json}"
        );
        assert!(json.contains("\"base_package\":"), "{json}");
        assert!(json.contains("\"layout\":"), "{json}");
        assert!(json.contains("\"capabilities\":"), "{json}");
    }

    #[test]
    fn standalone_project_is_its_own_reactor() {
        let root = fixture_dir("standalone");
        write_pom(
            &root,
            "<groupId>dev.example</groupId><artifactId>sample-cli</artifactId><properties><maven.compiler.release>27</maven.compiler.release></properties>",
        );

        let project = Project::inspect(&root).unwrap();
        let about = About::of(&project);

        assert_eq!(project.facts().reactor.root, "");
        assert_eq!(about.reactor_root, about.module_root);
        assert_eq!(about.reactor_label, "sample-cli");
        assert_eq!(project.java_release(), Some(27));
        assert!(!project.is_spring_boot());
        assert!(project.facts().reactor.modules.is_empty());
    }

    #[test]
    fn json_escapes_strings() {
        assert_eq!(crate::json::string("a\\b\"c\n"), "\"a\\\\b\\\"c\\n\"");
    }
}
