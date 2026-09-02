//! `jails new` and `jails new-cli` — an empty directory to a working project.
//!
//! `new` wraps start.spring.io and is the one command that uses the network;
//! `new-cli` writes a pom, an `App` and its test by hand and uses none. Both
//! also seed `src/test/resources/fixtures/.gitkeep`, and both accept
//! `--app <manifest>` to create the project and apply a whole manifest in one
//! command.
//!
//! **Nothing on the apply path may call `Project::discover()`**, and this is
//! the command the rule exists for. `discover` reads the *process* working
//! directory, which during `new --app` is the parent of the project that was
//! just created — so every route takes an explicit `Run` carrying an already
//! resolved `Project` instead.
//!
//! Split by what is being written: `spring.rs` and `plain.rs` are the two
//! project shapes, `gradle_project.rs` the third build system, `seed.rs` the
//! files both shapes share, and `publish.rs` the write path.

mod gradle_project;
mod plain;
mod publish;
mod seed;
mod spring;
mod write;

pub use plain::new_cli;
pub use spring::new;

use seed::{git_init, previewed, reported, seed, write_agents, write_fixtures_dir, write_mise};
use spring::write_devtools_defaults;

use jails_support::Result;
use std::fs;
use std::path::Path;
use std::process::Command;

/// What `jails new` was asked for.
///
/// A parameter object rather than a fifteenth positional argument: the four
/// Gradle flags are computed together, consumed together, and meaningless
/// apart -- `--boot` without `--gradle` names a version nothing reads.
pub struct Request<'a> {
    pub name: &'a str,
    pub group: Option<&'a str>,
    pub package: Option<&'a str>,
    pub deps: &'a str,
    pub java: &'a str,
    pub git: bool,
    pub devtools: bool,
    pub offline: bool,
    /// Write a Groovy Gradle build rather than fetching a Maven project.
    pub gradle: bool,
    /// The Spring Boot version to pin. `None` is `pom::TARGET_BOOT`.
    pub boot: Option<&'a str>,
    /// The Gradle distribution the wrapper pins. `None` is derived from the
    /// Boot major, because the Boot plugin refuses some pairings outright.
    pub gradle_version: Option<&'a str>,
    /// `bootJar { archiveBaseName }`. `None` leaves the block out.
    pub jar_name: Option<&'a str>,
    /// `bootJar { archiveVersion }`. Only read when `jar_name` is set.
    pub jar_version: Option<&'a str>,
    pub app: Option<&'a Path>,
    /// Write the manifest's Compose services without starting them.
    pub no_start: bool,
    pub debug: bool,
    pub pretend: bool,
}

/// `jails new`'s arguments as the request the creation path takes.
///
/// The translation lives here rather than in `main.rs` because `Request` is
/// this module's value: dispatch names a command, it does not build one.
pub fn request<'a>(args: &'a crate::cli::NewArgs, debug: bool, pretend: bool) -> Request<'a> {
    Request {
        name: &args.name,
        group: args.group.as_deref(),
        package: args.package.as_deref(),
        deps: &args.deps,
        java: &args.java,
        git: !args.no_git,
        devtools: !args.no_devtools,
        offline: args.offline,
        gradle: args.gradle,
        boot: args.boot.as_deref(),
        gradle_version: args.gradle_version.as_deref(),
        jar_name: args.jar_name.as_deref(),
        jar_version: args.jar_version.as_deref(),
        app: args.app.as_deref(),
        no_start: args.no_start,
        debug,
        pretend,
    }
}

/// `jails new-cli`'s arguments as the same request.
///
/// Every Spring-shaped field is spelled out rather than defaulted: `new-cli`
/// writes a plain Maven project and has no flag for any of them, so a field
/// added to `Request` arrives here as a compile error asking what `new-cli`
/// should do with it rather than as a silent `Default`.
pub fn cli_request<'a>(
    args: &'a crate::cli::NewCliArgs,
    debug: bool,
    pretend: bool,
) -> Request<'a> {
    Request {
        name: &args.name,
        group: args.group.as_deref(),
        package: args.package.as_deref(),
        java: &args.release,
        git: !args.no_git,
        app: args.app.as_deref(),
        no_start: args.no_start,
        debug,
        pretend,
        deps: "",
        devtools: false,
        offline: true,
        gradle: false,
        boot: None,
        gradle_version: None,
        jar_name: None,
        jar_version: None,
    }
}

/// The entry point's class name, and the one name it must not be.
///
/// `<Name>Application` is Initializr's convention and jails follows it -- but a
/// project called `spring` derives `SpringApplication`, which is the name of
/// the Boot class the generated `main` calls. Java resolves the type in the
/// same compilation unit ahead of the import, so `SpringApplication.run(...)`
/// binds to the generated class, which has no `run`. The project does not
/// compile, and the error names a method rather than the collision.
///
/// The fallback is `Application`. It is printed rather than done quietly:
/// the reader asked for a name and got a different one.
fn application_class(name: &str) -> String {
    // The *stem*: the templates and the paths both append `Application`, so
    // `Demo` here is `DemoApplication.java` there, and an empty stem is plain
    // `Application.java`.
    let derived = camel_case(name);
    if derived == "Spring" {
        println!(
            "  naming the entry point Application: SpringApplication is \
             org.springframework.boot.SpringApplication, and a class cannot \
             shadow the type its own main() calls"
        );
        return String::new();
    }
    derived
}

pub(crate) fn camel_case(name: &str) -> String {
    let mut out = String::new();
    let mut uppercase = true;
    for character in name.chars() {
        if !character.is_ascii_alphanumeric() {
            uppercase = true;
            continue;
        }
        if uppercase {
            out.extend(character.to_uppercase());
            uppercase = false;
        } else {
            out.push(character);
        }
    }
    // Empty rather than `"Application"`: the caller appends `Application`, so
    // returning it here spells `ApplicationApplication`.
    out
}

/// The base package this project will use.
///
/// `--package` wins outright; `--group` alone puts the sanitised name under
/// the given group, which is what somebody migrating a service usually means
/// -- they have a group and the last segment is the service. Neither given
/// falls back to Initializr's own default so the two paths agree.
///
/// This is the first thing anyone migrating an existing service hits, because
/// an existing service already has a package and it is never `com.example`.
fn resolved_package(name: &str, group: Option<&str>, package: Option<&str>) -> String {
    match (package, group) {
        (Some(package), _) => package.to_string(),
        (None, Some(group)) => format!("{group}.{}", package_segment(name)),
        (None, None) => format!("com.example.{}", package_segment(name)),
    }
}

/// artifactId -> a lowercase, dot-free Java package segment.
fn package_segment(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_lowercase()
}

fn validate_project_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name.chars().any(|character| {
            !character.is_ascii_alphanumeric() && !matches!(character, '-' | '_' | '.')
        })
    {
        return Err(format!(
            "project name `{name}` is not a valid Maven artifact id.\n       \
             fix: use only ASCII letters, digits, `.`, `-`, or `_`; for example `my-app`."
        )
        .into());
    }
    Ok(())
}

/// The group a project's own artifact is published under.
///
/// Derived from the package when it is not stated, because the two disagreeing
/// is a thing nobody notices until they publish: a project in
/// `com.intercom.spring` whose pom says `com.example` is wrong in the one
/// field a repository indexes it by. The last segment is the artifact, so the
/// group is everything above it.
fn group_of(group: Option<&str>, package: &str) -> String {
    match group {
        Some(group) => group.to_string(),
        None => match package.rsplit_once('.') {
            Some((above, _)) => above.to_string(),
            None => package.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::plain::pom_xml;
    use super::spring::{effective_deps, initializr_java, set_java_release};
    use super::*;
    use jails_testkit::hold_cwd;
    use std::path::PathBuf;

    fn scratch(label: &str) -> PathBuf {
        jails_support::scratch::ScratchDir::in_temp(&format!("jails-new-test-{label}"))
            .unwrap()
            .keep()
    }

    #[test]
    fn effective_deps_appends_devtools_by_default() {
        assert_eq!(effective_deps("web", true), "web,devtools");
        assert_eq!(effective_deps("web,jdbc", true), "web,jdbc,devtools");
    }

    #[test]
    fn effective_deps_skips_devtools_when_disabled() {
        assert_eq!(effective_deps("web", false), "web");
    }

    #[test]
    fn effective_deps_does_not_duplicate_an_explicit_devtools() {
        assert_eq!(effective_deps("web,devtools", true), "web,devtools");
    }

    #[test]
    fn effective_deps_handles_an_empty_deps_string() {
        assert_eq!(effective_deps("", true), "devtools");
    }

    #[test]
    fn initializr_uses_its_newest_supported_release_for_java_27() {
        assert_eq!(initializr_java("27"), "26");
        assert_eq!(initializr_java("26"), "26");
        assert_eq!(initializr_java("21"), "21");
    }

    #[test]
    fn generated_spring_pom_is_retargeted_after_bootstrapping() {
        let root = scratch("retarget-java");
        fs::write(
            root.join("pom.xml"),
            "<project><properties><java.version>26</java.version></properties></project>",
        )
        .unwrap();

        set_java_release(&publish::Tree::at(&root), "26", "27").unwrap();

        let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
        assert!(pom.contains("<java.version>27</java.version>"));
        assert!(!pom.contains("<java.version>26</java.version>"));
    }

    #[test]
    fn a_package_is_stripped_lowercased_and_placed_under_the_default_group() {
        assert_eq!(resolved_package("my-app", None, None), "com.example.myapp");
        assert_eq!(resolved_package("MyApp2", None, None), "com.example.myapp2");
    }

    /// The case this exists for: an existing service already has a package,
    /// and it is never `com.example`.
    #[test]
    fn an_explicit_package_wins_and_a_group_alone_keeps_the_name_as_the_last_segment() {
        assert_eq!(
            resolved_package("spring-4", None, Some("com.intercom.spring")),
            "com.intercom.spring"
        );
        assert_eq!(
            resolved_package("spring-4", Some("com.intercom"), None),
            "com.intercom.spring4"
        );
        // `--package` outranks `--group`: it says the whole answer, so a group
        // beside it is a second opinion about the same thing.
        assert_eq!(
            resolved_package("spring-4", Some("com.ignored"), Some("com.intercom.spring")),
            "com.intercom.spring"
        );
    }

    #[test]
    fn pom_xml_pins_the_requested_java_release_and_main_class() {
        let pom = pom_xml(
            "demo",
            "com.example",
            "com.example.demo",
            crate::pom::TARGET_RELEASE,
        );
        assert!(pom.contains(&format!(
            "<maven.compiler.release>{}</maven.compiler.release>",
            crate::pom::TARGET_RELEASE
        )));
        // The release is whatever the caller asked for, not a baked-in constant.
        assert!(
            pom_xml("demo", "com.example", "com.example.demo", "21")
                .contains("<maven.compiler.release>21</maven.compiler.release>")
        );
        assert!(pom.contains("<mainClass>com.example.demo.App</mainClass>"));
        assert!(pom.contains("<artifactId>demo</artifactId>"));
    }

    #[test]
    fn pom_xml_declares_junit_and_assertj_as_test_dependencies() {
        let pom = pom_xml(
            "demo",
            "com.example",
            "com.example.demo",
            crate::pom::TARGET_RELEASE,
        );
        assert!(pom.contains("<artifactId>junit-jupiter</artifactId>"));
        assert!(pom.contains("<artifactId>assertj-core</artifactId>"));
    }

    /// What `jails new-cli` is asked for, with every Spring-shaped field at
    /// the value that means "not asked" -- the same spelling `main.rs` uses.
    fn plain_request<'a>(name: &'a str, git: bool) -> Request<'a> {
        Request {
            name,
            group: None,
            package: None,
            java: crate::pom::TARGET_RELEASE,
            git,
            app: None,
            no_start: true,
            debug: false,
            pretend: false,
            deps: "",
            devtools: false,
            offline: true,
            gradle: false,
            boot: None,
            gradle_version: None,
            jar_name: None,
            jar_version: None,
        }
    }

    #[test]
    fn new_cli_writes_pom_and_sources_under_the_target_directory() {
        let _guard = hold_cwd();
        let workdir = scratch("new-cli");
        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(&workdir).unwrap();
        let result = new_cli(&plain_request("demo-app", false));
        std::env::set_current_dir(&original_cwd).unwrap();
        result.unwrap();

        let root = workdir.join("demo-app");
        assert!(root.join("pom.xml").is_file());
        let app = root.join("src/main/java/com/example/demoapp/App.java");
        let test = root.join("src/test/java/com/example/demoapp/AppTest.java");
        assert!(app.is_file(), "expected {}", app.display());
        assert!(test.is_file(), "expected {}", test.display());
        let fixtures = root.join("src/test/resources/fixtures");
        assert!(fixtures.is_dir(), "expected {}", fixtures.display());
        assert!(
            fixtures.join(".gitkeep").is_file(),
            "fixtures dir needs a .gitkeep to survive a clone"
        );
    }

    #[test]
    fn new_cli_refuses_to_overwrite_an_existing_directory() {
        let _guard = hold_cwd();
        let workdir = scratch("new-cli-exists");
        fs::create_dir_all(workdir.join("demo-app")).unwrap();
        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(&workdir).unwrap();
        let result = new_cli(&plain_request("demo-app", false));
        std::env::set_current_dir(&original_cwd).unwrap();

        assert!(result.is_err());
    }

    #[test]
    fn new_cli_skips_git_setup_when_disabled() {
        let _guard = hold_cwd();
        let workdir = scratch("new-cli-no-git");
        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(&workdir).unwrap();
        let result = new_cli(&plain_request("demo-app", false));
        std::env::set_current_dir(&original_cwd).unwrap();
        result.unwrap();

        let root = workdir.join("demo-app");
        assert!(!root.join(".gitignore").exists());
        assert!(!root.join(".git").exists());
    }

    #[test]
    fn new_cli_sets_up_git_by_default() {
        let _guard = hold_cwd();
        let workdir = scratch("new-cli-git");
        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(&workdir).unwrap();
        let result = new_cli(&plain_request("demo-app", true));
        std::env::set_current_dir(&original_cwd).unwrap();
        result.unwrap();

        let root = workdir.join("demo-app");
        let gitignore = fs::read_to_string(root.join(".gitignore")).unwrap();
        assert!(gitignore.contains("target/"));
        assert!(root.join(".git").is_dir());
    }
}
