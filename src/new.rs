mod gradle_project;
mod plain;
mod publish;
mod seed;
mod spring;

pub use plain::new_cli;
pub use spring::new;

use seed::{git_init, previewed, seed, write_agents, write_fixtures_dir, write_mise};
use spring::{write_default_properties, write_devtools_defaults};

use jails_support::Result;
use std::fs;
use std::path::Path;
use std::process::Command;

/// What `jails new` was asked for.
///
/// A parameter object rather than a fifteenth positional argument, and the
/// same move `abstract.md` §7 rung 1 records for `Project`: the four Gradle
/// flags are computed together, consumed together, and meaningless apart --
/// `--boot` without `--gradle` names a version nothing reads.
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
    pub debug: bool,
    pub pretend: bool,
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
/// The fallback is `Application`, which is what the Boot 2.7 project this was
/// built against is actually called. It is printed rather than done quietly:
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

fn camel_case(name: &str) -> String {
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
    use jails_testkit::CWD_LOCK;
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

    /// The entry point is a dispatcher, not a Hello World stub -- otherwise
    /// `generate command` has nothing to register into, which is the whole
    /// point of `new-cli`.
    #[test]
    fn app_java_is_a_command_dispatcher() {
        let src = crate::generate::cli_java("com.example.demo", "App", "demo");
        assert!(src.contains("package com.example.demo;"));
        assert!(src.contains("public static void main(String[] args)"));
        assert!(src.contains("public final class App"), "{src}");
        assert!(
            src.contains("usage: demo <command> [args]"),
            "the program name should be the project's"
        );
        assert!(
            crate::generate::is_dispatcher(&src),
            "generate command must be able to find this"
        );
    }

    #[test]
    fn app_test_java_drives_the_dispatcher() {
        let src = crate::generate::cli_test("com.example.demo", "App");
        assert!(src.contains("import org.junit.jupiter.api.Test;"));
        assert!(src.contains("class AppTest"));
        assert!(src.contains("App.run("));
    }

    #[test]
    fn new_cli_writes_pom_and_sources_under_the_target_directory() {
        let _guard = CWD_LOCK.lock().unwrap();
        let workdir = scratch("new-cli");
        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(&workdir).unwrap();
        let result = new_cli(
            "demo-app",
            None,
            None,
            crate::pom::TARGET_RELEASE,
            false,
            None,
            false,
            false,
        );
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
        let _guard = CWD_LOCK.lock().unwrap();
        let workdir = scratch("new-cli-exists");
        fs::create_dir_all(workdir.join("demo-app")).unwrap();
        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(&workdir).unwrap();
        let result = new_cli(
            "demo-app",
            None,
            None,
            crate::pom::TARGET_RELEASE,
            false,
            None,
            false,
            false,
        );
        std::env::set_current_dir(&original_cwd).unwrap();

        assert!(result.is_err());
    }

    #[test]
    fn new_cli_skips_git_setup_when_disabled() {
        let _guard = CWD_LOCK.lock().unwrap();
        let workdir = scratch("new-cli-no-git");
        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(&workdir).unwrap();
        let result = new_cli(
            "demo-app",
            None,
            None,
            crate::pom::TARGET_RELEASE,
            false,
            None,
            false,
            false,
        );
        std::env::set_current_dir(&original_cwd).unwrap();
        result.unwrap();

        let root = workdir.join("demo-app");
        assert!(!root.join(".gitignore").exists());
        assert!(!root.join(".git").exists());
    }

    #[test]
    fn new_cli_sets_up_git_by_default() {
        let _guard = CWD_LOCK.lock().unwrap();
        let workdir = scratch("new-cli-git");
        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(&workdir).unwrap();
        let result = new_cli(
            "demo-app",
            None,
            None,
            crate::pom::TARGET_RELEASE,
            true,
            None,
            false,
            false,
        );
        std::env::set_current_dir(&original_cwd).unwrap();
        result.unwrap();

        let root = workdir.join("demo-app");
        let gitignore = fs::read_to_string(root.join(".gitignore")).unwrap();
        assert!(gitignore.contains("target/"));
        assert!(root.join(".git").is_dir());
    }
}
