use crate::Result;
use std::fs;
use std::path::Path;
use std::process::Command;

/// Port of the `spring-init` bash function: wraps start.spring.io's
/// starter.zip API. baseDir wraps the archive in a `$name/` folder
/// server-side, so extracting to "." lands the project at `./$name`.
pub fn new(name: &str, deps: &str, java: &str, git: bool) -> Result<()> {
    if Path::new(name).exists() {
        return Err(format!("{name} already exists"));
    }

    let tmp = std::env::temp_dir().join(format!("jails-new-{}-{}", name, std::process::id()));
    fs::create_dir_all(&tmp).map_err(|e| format!("failed to create temp dir: {e}"))?;
    let zip_path = tmp.join("starter.zip");

    let status = Command::new("curl")
        .args(["-sf", "https://start.spring.io/starter.zip"])
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
        .arg(&zip_path)
        .status()
        .map_err(|e| format!("failed to run curl: {e}"))?;

    if !status.success() {
        let _ = fs::remove_dir_all(&tmp);
        return Err("starter.zip request failed".to_string());
    }

    let status = Command::new("unzip")
        .args(["-q"])
        .arg(&zip_path)
        .args(["-d", "."])
        .status()
        .map_err(|e| format!("failed to run unzip: {e}"))?;

    let _ = fs::remove_dir_all(&tmp);

    if !status.success() {
        return Err("failed to extract starter.zip".to_string());
    }

    // start.spring.io's zip already ships a .gitignore, so just init.
    if git {
        git_init(Path::new(name));
    }

    println!("Created ./{name} (deps: {deps}, Java {java})");
    Ok(())
}

/// Plain Maven CLI project, written directly -- no `mvn archetype:generate`
/// (slow, needs network, and falls into an interactive catalog picker
/// without exact archetype coordinates).
pub fn new_cli(name: &str, git: bool) -> Result<()> {
    let root = Path::new(name);
    if root.exists() {
        return Err(format!("{name} already exists"));
    }

    let package = sanitize_package(name);
    let java = "26";

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
    fs::write(src_dir.join("App.java"), app_java(&package))
        .map_err(|e| format!("failed to write App.java: {e}"))?;
    fs::write(test_dir.join("AppTest.java"), app_test_java(&package))
        .map_err(|e| format!("failed to write AppTest.java: {e}"))?;

    if git {
        fs::write(root.join(".gitignore"), GITIGNORE).map_err(|e| format!("failed to write .gitignore: {e}"))?;
        git_init(root);
    }

    println!("Created ./{name} (package: {package}, Java {java})");
    Ok(())
}

const GITIGNORE: &str = "target/\n*.class\n.idea/\n*.iml\n.DS_Store\n";

/// Best-effort: a missing/broken git shouldn't fail project creation, just
/// skip repo setup with a warning.
fn git_init(root: &Path) {
    match Command::new("git").args(["init", "-q"]).current_dir(root).status() {
        Ok(status) if status.success() => {}
        Ok(status) => eprintln!("jails: git init exited with {status}, skipping"),
        Err(e) => eprintln!("jails: failed to run git init: {e}"),
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
            <version>5.11.0</version>
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

fn app_java(package: &str) -> String {
    format!(
        r#"package {package};

public class App {{

    public static void main(String[] args) {{
        System.out.println("Hello, World!");
    }}
}}
"#
    )
}

fn app_test_java(package: &str) -> String {
    format!(
        r#"package {package};

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertTrue;

class AppTest {{

    @Test
    void shouldDoSomething() {{
        assertTrue(true);
    }}
}}
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
    fn sanitize_package_strips_non_alphanumerics_and_lowercases() {
        assert_eq!(sanitize_package("my-app"), "com.example.myapp");
        assert_eq!(sanitize_package("MyApp2"), "com.example.myapp2");
    }

    #[test]
    fn pom_xml_pins_the_requested_java_release_and_main_class() {
        let pom = pom_xml("demo", "com.example.demo", "26");
        assert!(pom.contains("<maven.compiler.release>26</maven.compiler.release>"));
        assert!(pom.contains("<mainClass>com.example.demo.App</mainClass>"));
        assert!(pom.contains("<artifactId>demo</artifactId>"));
    }

    #[test]
    fn app_java_prints_hello_world_from_main() {
        let src = app_java("com.example.demo");
        assert!(src.contains("package com.example.demo;"));
        assert!(src.contains("public static void main(String[] args)"));
        assert!(src.contains("Hello, World!"));
    }

    #[test]
    fn app_test_java_has_one_passing_junit5_test() {
        let src = app_test_java("com.example.demo");
        assert!(src.contains("import org.junit.jupiter.api.Test;"));
        assert!(src.contains("@Test"));
        assert!(src.contains("assertTrue(true)"));
    }

    #[test]
    fn new_cli_writes_pom_and_sources_under_the_target_directory() {
        let _guard = CWD_LOCK.lock().unwrap();
        let workdir = scratch("new-cli");
        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(&workdir).unwrap();
        let result = new_cli("demo-app", false);
        std::env::set_current_dir(&original_cwd).unwrap();
        result.unwrap();

        let root = workdir.join("demo-app");
        assert!(root.join("pom.xml").is_file());
        let app = root.join("src/main/java/com/example/demoapp/App.java");
        let test = root.join("src/test/java/com/example/demoapp/AppTest.java");
        assert!(app.is_file(), "expected {}", app.display());
        assert!(test.is_file(), "expected {}", test.display());
    }

    #[test]
    fn new_cli_refuses_to_overwrite_an_existing_directory() {
        let _guard = CWD_LOCK.lock().unwrap();
        let workdir = scratch("new-cli-exists");
        fs::create_dir_all(workdir.join("demo-app")).unwrap();
        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(&workdir).unwrap();
        let result = new_cli("demo-app", false);
        std::env::set_current_dir(&original_cwd).unwrap();

        assert!(result.is_err());
    }

    #[test]
    fn new_cli_skips_git_setup_when_disabled() {
        let _guard = CWD_LOCK.lock().unwrap();
        let workdir = scratch("new-cli-no-git");
        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(&workdir).unwrap();
        let result = new_cli("demo-app", false);
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
        let result = new_cli("demo-app", true);
        std::env::set_current_dir(&original_cwd).unwrap();
        result.unwrap();

        let root = workdir.join("demo-app");
        let gitignore = fs::read_to_string(root.join(".gitignore")).unwrap();
        assert!(gitignore.contains("target/"));
        assert!(root.join(".git").is_dir());
    }
}
