//! `jails new-cli` — a plain Maven project, with no framework at all.
//!
//! A hand-written pom, an `App` with a dispatcher in it, and a test. No
//! network, no starter, no Spring: the projects `g record` and `g command`
//! were written for, and the ones `add`'s framework-free capabilities target.
//!
//! `pending.md` §8.1's split. This is the half that does not know what Spring
//! is; [`super::spring`] is the half that does.

use super::seed::GITIGNORE;
use super::*;

/// Plain Maven CLI project, written directly -- no `mvn archetype:generate`
/// (slow, needs network, and falls into an interactive catalog picker
/// without exact archetype coordinates).
pub fn new_cli(request: &Request<'_>) -> Result<()> {
    let Request {
        name,
        group,
        package,
        java,
        git,
        app,
        debug,
        pretend,
        ..
    } = *request;
    validate_project_name(name)?;
    if Path::new(name).exists() {
        return Err(jails_support::Failure::Told(publish::already_exists(
            Path::new(name),
        )));
    }

    // A generic tool cannot hardcode one release level: the LTS most people
    // run and the newest JDK are rarely the same number.
    match java.parse::<u32>() {
        Ok(level) if level < crate::pom::MIN_RELEASE => {
            return Err(format!(
                "--release {java} is below Java {}, which is what jails' generated code needs",
                crate::pom::MIN_RELEASE
            )
            .into());
        }
        Ok(_) => {}
        Err(_) => return Err(format!("--release must be a number, got '{java}'").into()),
    }

    let package = resolved_package(name, group, package);

    // Every path below is written unconditionally, so the preview is the
    // list itself rather than a second description of it that can drift.
    if pretend {
        let root = Path::new(name);
        let mut planned = vec![
            root.join("pom.xml"),
            root.join("src/main/java")
                .join(package.replace('.', "/"))
                .join("App.java"),
            root.join("src/test/java")
                .join(package.replace('.', "/"))
                .join("AppTest.java"),
            root.join("src/test/resources/fixtures/.gitkeep"),
            root.join("mise.toml"),
            root.join("AGENTS.md"),
        ];
        if git {
            planned.push(root.join(".gitignore"));
        }
        for path in planned {
            println!("would create {}", path.display());
        }
        if git {
            println!("would run git init in ./{name}");
        }
        println!();
        println!("--pretend: nothing was written. (package: {package}, Java {java})");
        return previewed(app);
    }

    let publication = publish::Publication::reserve(Path::new(name))?;
    let tree = publication.tree();
    let src_dir = tree.join("src/main/java").join(package.replace('.', "/"));
    let test_dir = tree.join("src/test/java").join(package.replace('.', "/"));

    tree.ensure_directory_at(&src_dir)
        .map_err(|e| format!("failed to create {}: {e}", src_dir.display()))?;
    tree.ensure_directory_at(&test_dir)
        .map_err(|e| format!("failed to create {}: {e}", test_dir.display()))?;

    tree.put_named(
        "pom.xml",
        pom_xml(name, &group_of(group, &package), &package, java),
        "pom.xml",
    )?;
    // Through write_new_file, not fs::write, so the entry point and its test
    // get the same import ordering as everything jails generates later --
    // otherwise `add format` finds violations in files jails itself wrote.
    //
    // App.java *is* the command dispatcher, not a Hello World stub. A command
    // called `new-cli` that produces a project unable to dispatch commands
    // makes `jails generate command` -- the obvious next step -- report that
    // it has nothing to register into, and leaves you with two `main`s the
    // moment you fix that by hand.
    // `tree.root()` is the project being created, not the process CWD. Passing it
    // is what gives a new-cli project's own base package the null-marked
    // `package-info.java` every other package gets -- the lookup this
    // replaced either found the surrounding project or found nothing.
    crate::generate::write_new_file(
        tree,
        &src_dir.join("App.java"),
        &crate::generate::cli_java(&package, "App", name),
    )?;
    crate::generate::write_new_file(
        tree,
        &test_dir.join("AppTest.java"),
        &crate::generate::cli_test(&package, "App"),
    )?;

    write_fixtures_dir(&tree)?;
    write_mise(&tree, java)?;
    write_agents(&tree, java)?;

    if git {
        tree.put(".gitignore", GITIGNORE)?;
        git_init(&tree, debug);
    }
    let applied = seed(&publication, app, request.no_start, debug)?;

    publication.publish()?;
    println!("Created ./{name} (package: {package}, Java {java})");
    reported(applied)
}

pub(super) fn ensure_enforcer(tree: &publish::Tree<'_>, java: &str) -> Result<()> {
    let pom = crate::pom::read(tree.root())?;
    let plugin = format!(
        r#"<plugin>
    <groupId>org.apache.maven.plugins</groupId>
    <artifactId>maven-enforcer-plugin</artifactId>
    <version>3.6.3</version>
    <executions>
        <execution>
            <id>jails-toolchain</id>
            <goals><goal>enforce</goal></goals>
            <configuration>
                <rules>
                    <requireJavaVersion>
                        <version>[{java},)</version>
                        <message>This project requires Java {java}+; select it with mise use java@{java}.</message>
                    </requireJavaVersion>
                    <requireMavenVersion>
                        <version>[3.9,)</version>
                        <message>This project requires Maven 3.9+; use the project wrapper or upgrade Maven.</message>
                    </requireMavenVersion>
                </rules>
            </configuration>
        </execution>
    </executions>
</plugin>"#
    );
    if let Some(updated) = crate::pom::add_plugin(&pom, "maven-enforcer-plugin", &plugin)? {
        tree.put_named("pom.xml", updated, "pom.xml")?;
    }
    Ok(())
}

pub(super) fn pom_xml(artifact: &str, group: &str, package: &str, java: &str) -> String {
    format!(
        r#"<project xmlns="http://maven.apache.org/POM/4.0.0"
         xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
         xsi:schemaLocation="http://maven.apache.org/POM/4.0.0 http://maven.apache.org/xsd/maven-4.0.0.xsd">
    <modelVersion>4.0.0</modelVersion>

    <groupId>{group}</groupId>
    <artifactId>{artifact}</artifactId>
    <version>0.1.0</version>
    <packaging>jar</packaging>

    <properties>
        <maven.compiler.release>{java}</maven.compiler.release>
        <project.build.sourceEncoding>UTF-8</project.build.sourceEncoding>
    </properties>

    <dependencies>
        <!--
          JSpecify's @NullMarked is a package-level opt-in, and every package
          jails generates carries one. Without this dependency those
          package-info.java files do not compile.
        -->
        <dependency>
            <groupId>org.jspecify</groupId>
            <artifactId>jspecify</artifactId>
            <version>1.0.0</version>
        </dependency>
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
