use crate::Result;
use std::fs;
use std::path::Path;
use std::process::Command;

/// Port of the `spring-init` bash function: wraps start.spring.io's
/// starter.zip API. baseDir wraps the archive in a `$name/` folder
/// server-side, so extracting to "." lands the project at `./$name`.
pub fn new(name: &str, deps: &str, java: &str, git: bool, devtools: bool, debug: bool) -> Result<()> {
    if Path::new(name).exists() {
        return Err(format!("{name} already exists"));
    }

    let deps = effective_deps(deps, devtools);
    let deps = deps.as_str();

    let tmp = std::env::temp_dir().join(format!("jails-new-{}-{}", name, std::process::id()));
    fs::create_dir_all(&tmp).map_err(|e| format!("failed to create temp dir: {e}"))?;
    let zip_path = tmp.join("starter.zip");

    let mut curl = Command::new("curl");
    curl.args(["-sf", "https://start.spring.io/starter.zip"])
        .arg("-d")
        .arg(format!("dependencies={deps}"))
        .args(["-d", "type=maven-project"])
        .arg("-d")
        .arg(format!("javaVersion={java}"))
        .arg("-d")
        .arg(format!("artifactId={name}"))
        .arg("-d")
        .arg(format!("name={name}"))
        .arg("-d")
        .arg(format!("baseDir={name}"))
        .arg("-o")
        .arg(&zip_path);
    if debug {
        crate::debug_cmd(&curl);
    }
    let status = curl.status().map_err(|e| format!("failed to run curl: {e}"))?;

    if !status.success() {
        let _ = fs::remove_dir_all(&tmp);
        return Err("starter.zip request failed".to_string());
    }

    let mut unzip = Command::new("unzip");
    unzip.args(["-q"]).arg(&zip_path).args(["-d", "."]);
    if debug {
        crate::debug_cmd(&unzip);
    }
    let status = unzip.status().map_err(|e| format!("failed to run unzip: {e}"))?;

    let _ = fs::remove_dir_all(&tmp);

    if !status.success() {
        return Err("failed to extract starter.zip".to_string());
    }

    write_fixtures_dir(Path::new(name))?;

    // start.spring.io's zip already ships a .gitignore, so just init.
    if git {
        git_init(Path::new(name), debug);
    }

    println!("Created ./{name} (deps: {deps}, Java {java})");
    Ok(())
}

/// Plain Maven CLI project, written directly -- no `mvn archetype:generate`
/// (slow, needs network, and falls into an interactive catalog picker
/// without exact archetype coordinates).
pub fn new_cli(name: &str, java: &str, git: bool, debug: bool) -> Result<()> {
    let root = Path::new(name);
    if root.exists() {
        return Err(format!("{name} already exists"));
    }

    // A generic tool cannot hardcode one release level: the LTS most people
    // run and the newest JDK are rarely the same number.
    match java.parse::<u32>() {
        Ok(level) if level < crate::pom::MIN_RELEASE => {
            return Err(format!(
                "--release {java} is below Java {}, which is what jails' generated code needs",
                crate::pom::MIN_RELEASE
            ));
        }
        Ok(_) => {}
        Err(_) => return Err(format!("--release must be a number, got '{java}'")),
    }

    let package = sanitize_package(name);

    let src_dir = root
        .join("src/main/java")
        .join(package.replace('.', "/"));
    let test_dir = root
        .join("src/test/java")
        .join(package.replace('.', "/"));
    fs::create_dir_all(&src_dir).map_err(|e| format!("failed to create {}: {e}", src_dir.display()))?;
    fs::create_dir_all(&test_dir).map_err(|e| format!("failed to create {}: {e}", test_dir.display()))?;

    fs::write(root.join("pom.xml"), pom_xml(name, &package, java))
        .map_err(|e| format!("failed to write pom.xml: {e}"))?;
    // Through write_new_file, not fs::write, so the entry point and its test
    // get the same import ordering as everything jails generates later --
    // otherwise `add format` finds violations in files jails itself wrote.
    //
    // App.java *is* the command dispatcher, not a Hello World stub. A command
    // called `new-cli` that produces a project unable to dispatch commands
    // makes `jails generate command` -- the obvious next step -- report that
    // it has nothing to register into, and leaves you with two `main`s the
    // moment you fix that by hand.
    crate::generate::write_new_file(&src_dir.join("App.java"), &crate::generate::cli_java(&package, "App", name))?;
    crate::generate::write_new_file(&test_dir.join("AppTest.java"), &crate::generate::cli_test(&package, "App"))?;

    write_fixtures_dir(root)?;

    if git {
        fs::write(root.join(".gitignore"), GITIGNORE).map_err(|e| format!("failed to write .gitignore: {e}"))?;
        git_init(root, debug);
    }

    println!("Created ./{name} (package: {package}, Java {java})");
    Ok(())
}

/// Test fixtures land on the test classpath, so they belong under
/// `src/test/resources`. Git can't track an empty directory, so seed it with
/// a `.gitkeep` -- otherwise the folder vanishes on the first clone.
fn write_fixtures_dir(root: &Path) -> Result<()> {
    let dir = root.join("src/test/resources/fixtures");
    fs::create_dir_all(&dir).map_err(|e| format!("failed to create {}: {e}", dir.display()))?;
    fs::write(dir.join(".gitkeep"), "")
        .map_err(|e| format!("failed to write {}/.gitkeep: {e}", dir.display()))?;
    Ok(())
}

const GITIGNORE: &str ="target/\n*.class\n.idea/\n*.iml\n.DS_Store\n";

/// Best-effort: a missing/broken git shouldn't fail project creation, just
/// skip repo setup with a warning.
fn git_init(root: &Path, debug: bool) {
    let mut cmd = Command::new("git");
    cmd.args(["init", "-q"]).current_dir(root);
    if debug {
        crate::debug_cmd(&cmd);
    }
    match cmd.status() {
        Ok(status) if status.success() => {}
        Ok(status) => eprintln!("jails: git init exited with {status}, skipping"),
        Err(e) => eprintln!("jails: failed to run git init: {e}"),
    }
}

/// devtools is on by default (fast restart-on-recompile + LiveReload,
/// needed for `jails run --watch` to do anything) -- append it unless
/// already present or explicitly opted out.
fn effective_deps(deps: &str, devtools: bool) -> String {
    if !devtools || deps.split(',').any(|d| d.trim() == "devtools") {
        return deps.to_string();
    }
    if deps.trim().is_empty() {
        "devtools".to_string()
    } else {
        format!("{deps},devtools")
    }
}

/// artifactId -> a lowercase, dot-free Java package segment under com.example.
fn sanitize_package(name: &str) -> String {
    let segment: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_lowercase();
    format!("com.example.{segment}")
}

fn pom_xml(artifact: &str, package: &str, java: &str) -> String {
    format!(
        r#"<project xmlns="http://maven.apache.org/POM/4.0.0"
         xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
         xsi:schemaLocation="http://maven.apache.org/POM/4.0.0 http://maven.apache.org/xsd/maven-4.0.0.xsd">
    <modelVersion>4.0.0</modelVersion>

    <groupId>com.example</groupId>
    <artifactId>{artifact}</artifactId>
    <version>0.1.0</version>
    <packaging>jar</packaging>

    <properties>
        <maven.compiler.release>{java}</maven.compiler.release>
        <project.build.sourceEncoding>UTF-8</project.build.sourceEncoding>
    </properties>

    <dependencies>
        <dependency>
            <groupId>org.junit.jupiter</groupId>
            <artifactId>junit-jupiter</artifactId>
            <version>6.1.2</version>
            <scope>test</scope>
        </dependency>
        <dependency>
            <groupId>org.assertj</groupId>
            <artifactId>assertj-core</artifactId>
            <version>3.27.7</version>
            <scope>test</scope>
        </dependency>
    </dependencies>

    <build>
        <finalName>{artifact}</finalName>
        <plugins>
            <plugin>
                <groupId>org.apache.maven.plugins</groupId>
                <artifactId>maven-compiler-plugin</artifactId>
                <version>3.13.0</version>
            </plugin>
            <plugin>
                <groupId>org.apache.maven.plugins</groupId>
                <artifactId>maven-surefire-plugin</artifactId>
                <version>3.2.5</version>
            </plugin>
            <plugin>
                <groupId>org.apache.maven.plugins</groupId>
                <artifactId>maven-jar-plugin</artifactId>
                <version>3.4.1</version>
                <configuration>
                    <archive>
                        <manifest>
                            <mainClass>{package}.App</mainClass>
                        </manifest>
                    </archive>
                </configuration>
            </plugin>
        </plugins>
    </build>
</project>
"#
    )
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::CWD_LOCK;
    use std::path::PathBuf;

    fn scratch(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "jails-new-test-{label}-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
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
    fn sanitize_package_strips_non_alphanumerics_and_lowercases() {
        assert_eq!(sanitize_package("my-app"), "com.example.myapp");
        assert_eq!(sanitize_package("MyApp2"), "com.example.myapp2");
    }

    #[test]
    fn pom_xml_pins_the_requested_java_release_and_main_class() {
        let pom = pom_xml("demo", "com.example.demo", crate::pom::TARGET_RELEASE);
        assert!(pom.contains(&format!(
            "<maven.compiler.release>{}</maven.compiler.release>",
            crate::pom::TARGET_RELEASE
        )));
        // The release is whatever the caller asked for, not a baked-in constant.
        assert!(pom_xml("demo", "com.example.demo", "21").contains("<maven.compiler.release>21</maven.compiler.release>"));
        assert!(pom.contains("<mainClass>com.example.demo.App</mainClass>"));
        assert!(pom.contains("<artifactId>demo</artifactId>"));
    }

    #[test]
    fn pom_xml_declares_junit_and_assertj_as_test_dependencies() {
        let pom = pom_xml("demo", "com.example.demo", crate::pom::TARGET_RELEASE);
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
        assert!(src.contains("usage: demo <command> [args]"), "the program name should be the project's");
        assert!(crate::generate::is_dispatcher(&src), "generate command must be able to find this");
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
        let result = new_cli("demo-app", crate::pom::TARGET_RELEASE, false, false);
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
        assert!(fixtures.join(".gitkeep").is_file(), "fixtures dir needs a .gitkeep to survive a clone");
    }

    #[test]
    fn new_cli_refuses_to_overwrite_an_existing_directory() {
        let _guard = CWD_LOCK.lock().unwrap();
        let workdir = scratch("new-cli-exists");
        fs::create_dir_all(workdir.join("demo-app")).unwrap();
        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(&workdir).unwrap();
        let result = new_cli("demo-app", crate::pom::TARGET_RELEASE, false, false);
        std::env::set_current_dir(&original_cwd).unwrap();

        assert!(result.is_err());
    }

    #[test]
    fn new_cli_skips_git_setup_when_disabled() {
        let _guard = CWD_LOCK.lock().unwrap();
        let workdir = scratch("new-cli-no-git");
        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(&workdir).unwrap();
        let result = new_cli("demo-app", crate::pom::TARGET_RELEASE, false, false);
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
        let result = new_cli("demo-app", crate::pom::TARGET_RELEASE, true, false);
        std::env::set_current_dir(&original_cwd).unwrap();
        result.unwrap();

        let root = workdir.join("demo-app");
        let gitignore = fs::read_to_string(root.join(".gitignore")).unwrap();
        assert!(gitignore.contains("target/"));
        assert!(root.join(".git").is_dir());
    }
}
