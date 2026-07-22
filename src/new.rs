use crate::Result;
use std::fs;
use std::path::Path;
use std::process::Command;

/// Port of the `spring-init` bash function: wraps start.spring.io's
/// starter.zip API. baseDir wraps the archive in a `$name/` folder
/// server-side, so extracting to "." lands the project at `./$name`.
pub fn new(name: &str, deps: &str, java: &str) -> Result<()> {
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

    println!("Created ./{name} (deps: {deps}, Java {java})");
    Ok(())
}

/// Plain Maven CLI project, written directly -- no `mvn archetype:generate`
/// (slow, needs network, and falls into an interactive catalog picker
/// without exact archetype coordinates).
pub fn new_cli(name: &str) -> Result<()> {
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

    println!("Created ./{name} (package: {package}, Java {java})");
    Ok(())
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
