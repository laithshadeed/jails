//! `jails new` and `new-cli`: an empty directory to a project that builds.

use super::*;

#[test]
fn new_cli_creates_expected_project_layout() {
    let workdir = temp_dir("new-cli-layout");
    let status = jails_cmd(&workdir, None)
        .args(["new-cli", "demo"])
        .status()
        .unwrap();
    assert!(status.success());

    let root = workdir.join("demo");
    assert!(root.join("pom.xml").is_file());
    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    assert!(
        pom.contains(&format!(
            "<maven.compiler.release>{TARGET_RELEASE}</maven.compiler.release>"
        )),
        "{pom}"
    );
    assert!(
        root.join("src/main/java/com/example/demo/App.java")
            .is_file()
    );
    assert!(
        root.join("src/test/java/com/example/demo/AppTest.java")
            .is_file()
    );
    let agents = fs::read_to_string(root.join("AGENTS.md")).unwrap();
    assert!(agents.contains("jails check"), "{agents}");
    assert!(agents.contains("@MockBean"), "{agents}");
}

#[test]
fn new_offline_defaults_to_the_workspace_target_release() {
    let workdir = temp_dir("new-offline-default-release");
    let output = jails_cmd(&workdir, None)
        .args(["new", "demo-app", "--offline", "--no-git"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let root = workdir.join("demo-app");
    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    assert!(
        pom.contains(&format!("<java.version>{TARGET_RELEASE}</java.version>")),
        "{pom}"
    );
    assert_eq!(
        fs::read_to_string(root.join("mise.toml")).unwrap(),
        format!("[tools]\njava = \"{TARGET_RELEASE}\"\n")
    );
}

#[test]
fn new_offline_creates_a_complete_spring_project_without_network() {
    let workdir = temp_dir("new-offline");
    let output = jails_cmd(&workdir, None)
        .args([
            "new",
            "demo-app",
            "--offline",
            "--no-git",
            "--no-devtools",
            "--deps",
            "web,actuator",
            "--java",
            "21",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let root = workdir.join("demo-app");
    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    assert!(pom.contains("spring-boot-starter-webmvc"), "{pom}");
    assert!(pom.contains("spring-boot-starter-actuator"), "{pom}");
    assert!(pom.contains("<java.version>21</java.version>"), "{pom}");
    assert!(pom.contains("maven-enforcer-plugin"), "{pom}");
    assert!(pom.contains("<requireJavaVersion>"), "{pom}");
    assert!(pom.contains("<requireMavenVersion>"), "{pom}");
    assert!(
        root.join("src/main/java/com/example/demoapp/DemoAppApplication.java")
            .is_file()
    );
    assert!(
        root.join("src/test/java/com/example/demoapp/DemoAppApplicationTests.java")
            .is_file()
    );
    assert_eq!(
        fs::read_to_string(root.join("mise.toml")).unwrap(),
        "[tools]\njava = \"21\"\n"
    );
    let agents = fs::read_to_string(root.join("AGENTS.md")).unwrap();
    assert!(
        agents.contains("base package is `com.example.demoapp`"),
        "{agents}"
    );
    assert!(agents.contains("jails lint"), "{agents}");
    let properties =
        fs::read_to_string(root.join("src/main/resources/application.properties")).unwrap();
    assert!(
        properties.contains("server.shutdown=graceful"),
        "{properties}"
    );
    assert!(!root.join(".git").exists());
}

#[test]
fn new_cli_fails_if_the_directory_already_exists() {
    let workdir = temp_dir("new-cli-exists");
    fs::create_dir(workdir.join("demo")).unwrap();
    let output = jails_cmd(&workdir, None)
        .args(["new-cli", "demo"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("already exists"));
}

#[test]
fn new_cli_with_an_app_manifest_is_one_command_from_an_empty_directory() {
    // plan.md §18 closes by asking which two commands should have been one,
    // and answers itself: `new` + `mkdir .jails` + `cp app.toml` + `app apply`
    // is four steps that only ever appear together. §0.4 tracks the count as a
    // scorecard metric with a target of 1.
    let workspace = temp_dir("new-app-manifest");
    fs::create_dir_all(&workspace).unwrap();
    let manifest = workspace.join("app.toml");
    fs::write(
        &manifest,
        "schema = 1

[[generate]]
kind = \"record\"
name = \"Entry\"
fields = [\"id:uuid\", \"label:string!\"]
",
    )
    .unwrap();

    let created = std::process::Command::new(env!("CARGO_BIN_EXE_jails"))
        .current_dir(&workspace)
        .args(["new-cli", "demo", "--no-git", "--app"])
        .arg(&manifest)
        .output()
        .unwrap();
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );

    let root = workspace.join("demo");
    // The manifest is seeded where `app apply` will find it next time...
    assert!(root.join(".jails/app.toml").is_file());
    // ...and its intents are already applied, against the project that was
    // just created rather than whatever encloses the process CWD.
    assert!(
        root.join("src/main/java/com/example/demo/domain/Entry.java")
            .is_file(),
        "the manifest's intent should have been applied"
    );
    assert!(
        root.join("src/test/java/com/example/demo/domain/EntryTest.java")
            .is_file()
    );
}

#[test]
fn new_with_an_unreadable_app_manifest_says_so_with_a_fix() {
    let workspace = temp_dir("new-app-missing");
    fs::create_dir_all(&workspace).unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_jails"))
        .current_dir(&workspace)
        .args(["new-cli", "demo", "--no-git", "--app", "nope.toml"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("application manifest"), "{stderr}");
    assert!(stderr.contains("fix:"), "{stderr}");

    // plan.md §R6.5: the destination is absent or complete. Everything
    // `new-cli` writes goes into a scratch sibling and becomes real in one
    // rename, so a manifest that cannot be read leaves nothing that reads
    // like a project -- which is what the previous behaviour left: a pom, an
    // App.java, and no manifest, in a directory the next run then refused to
    // reuse.
    assert!(
        !workspace.join("demo").exists(),
        "a refused `new-cli --app` leaves no half-written project behind"
    );
    let leftovers: Vec<String> = fs::read_dir(&workspace)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name != ".jails-new.lock")
        .collect();
    assert!(
        leftovers.is_empty(),
        "the scratch tree is swept too, found {leftovers:?}"
    );
}

/// The refusal a second `jails new` gets while the first is still publishing.
///
/// Not a hypothetical: the check that a destination is free and the rename
/// that takes it are separated by a download and a whole project's worth of
/// writes, so without the parent lock two runs can both pass the check.
#[test]
fn a_new_project_publishes_atomically_into_a_directory_that_is_watched() {
    let workspace = temp_dir("new-publication");
    fs::create_dir_all(&workspace).unwrap();
    let status = std::process::Command::new(env!("CARGO_BIN_EXE_jails"))
        .current_dir(&workspace)
        .args(["new-cli", "demo", "--no-git"])
        .status()
        .unwrap();
    assert!(status.success());
    assert!(workspace.join("demo/pom.xml").is_file());

    // The lock file is left in the parent on purpose (a lock is on an inode,
    // so deleting it lets two holders exist at once), but nothing else is.
    let leftovers: Vec<String> = fs::read_dir(&workspace)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name != ".jails-new.lock" && name != "demo")
        .collect();
    assert!(leftovers.is_empty(), "found {leftovers:?}");

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_jails"))
        .current_dir(&workspace)
        .args(["new-cli", "demo", "--no-git"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("already exists"), "{stderr}");
}

/// Can Maven *read* what jails wrote? (`plan.md` §8.8.)
///
/// `mvn -o validate` parses the pom and stops -- about two seconds a cell, no
/// downloads, no compilation -- and it is the check nothing in this suite was
/// doing. The 293-second manifest gate compiles far more, but every cell of it
/// is a Spring Boot project, so a versionless dependency (correct under
/// `spring-boot-starter-parent`, fatal without one) survived it and shipped:
/// `g scaffold` on a plain Maven project wrote a `spring-boot-starter-validation`
/// with no version, and *every* Maven goal then failed with
/// `'dependencies.dependency.version' ... is missing` -- including `validate`
/// itself. The golden suite had a snapshot of that pom and ratified it.
///
/// So the matrix is over the thing that differed: the flavour of project.
#[test]
fn every_generated_pom_is_one_maven_can_read() {
    if !real_mvn_available() {
        skip("mvn not found on PATH");
        return;
    }
    let path = real_path_without_mvnd();
    let matrix = temp_dir("pom-readable-matrix");
    // Kinds that splice something into the pom, which is where the defect
    // lives: a dependency, a plugin, or a test that needs AssertJ.
    let cells: &[(&str, &[&str])] = &[
        (
            "scaffold",
            &["g", "scaffold", "Note", "id:uuid@pk", "title:string!"],
        ),
        ("record", &["g", "record", "Note", "title:string!"]),
        ("integration-test", &["g", "integration-test", "Checkout"]),
        ("cli", &["g", "cli", "Admin"]),
    ];
    let mut modules = Vec::new();
    let mut generated = Vec::new();
    for spring in [false, true] {
        for (label, args) in cells {
            // A scaffold is a Spring projection. Plain-project refusal is
            // covered by the no-write contract in `cli::generate`; there is
            // intentionally no plain scaffold POM to include in this matrix.
            if !spring && *label == "scaffold" {
                continue;
            }
            let flavor = if spring { "spring" } else { "plain" };
            let module = format!("{flavor}-{label}");
            let root = matrix.join(&module);
            if spring {
                write_spring_fixture(&root);
            } else {
                write_plain_fixture(&root);
            }
            let output = jails_cmd_with_path(&root, &path)
                .args(*args)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{label} failed: {}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );

            // Reactor coordinates must be unique even though every fixture
            // deliberately starts as `com.example:demo`.
            let pom_path = root.join("pom.xml");
            let pom = fs::read_to_string(&pom_path).unwrap().replace(
                "<artifactId>demo</artifactId>",
                &format!("<artifactId>{module}</artifactId>"),
            );
            fs::write(pom_path, pom).unwrap();
            modules.push(module);
            generated.push((flavor, *label, args.join(" ")));
        }
    }

    let module_xml = modules
        .iter()
        .map(|module| format!("        <module>{module}</module>"))
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(
        matrix.join("pom.xml"),
        format!(
            "<project xmlns=\"http://maven.apache.org/POM/4.0.0\">\n    <modelVersion>4.0.0</modelVersion>\n    <groupId>com.example</groupId>\n    <artifactId>jails-pom-matrix</artifactId>\n    <version>1</version>\n    <packaging>pom</packaging>\n    <modules>\n{module_xml}\n    </modules>\n</project>\n"
        ),
    )
    .unwrap();

    let output = real_maven_cmd(&matrix, &path)
        .args(["-o", "-q", "validate"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "Maven could not read the generated POM matrix {generated:?}:\n{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn new_cli_project_passes_real_mvn_test() {
    if !real_mvn_available() {
        skip("mvn not found on PATH");
        return;
    }
    if !real_java_supports_target_release() {
        skip(&format!(
            "javac on PATH does not support --release {TARGET_RELEASE}"
        ));
        return;
    }
    let path = real_path_without_mvnd();
    let root = verified_plain_toolbox(&path);
    assert!(
        root.join("target/classes/com/example/demo/App.class")
            .is_file()
    );
}

/// `add format` installs a formatter that checks the build. If jails' own
/// output does not already satisfy it, a freshly generated project fails
/// `jails check` on the first run -- a bad first impression, and the reason
/// import order is normalised at write time.
#[test]
fn a_freshly_generated_project_passes_check_with_no_manual_formatting() {
    if !real_mvn_available() {
        skip("mvn not found on PATH");
        return;
    }
    if !real_java_supports_target_release() {
        skip(&format!(
            "javac on PATH does not support --release {TARGET_RELEASE}"
        ));
        return;
    }
    let path = real_path_without_mvnd();
    let root = verified_plain_toolbox(&path);
    let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
    assert!(pom.contains("spotless-maven-plugin"), "{pom}");
}

/// A file writer must not rediscover the project. `write_new_file` used to
/// find it from process CWD, which is not the project being written to when
/// `new-cli` creates a directory the CWD is not inside.
///
/// The visible cost: a `new-cli` project's own base package never got the
/// null-marked `package-info.java` every other package gets. Run from
/// nowhere, the lookup found no project and skipped; run from inside another
/// Maven project, it read *that* project's pom and package. The audit's
/// "every package jails writes a class into gets one" was simply not true for
/// `App.java`.
#[test]
fn new_cli_gives_its_own_base_package_a_package_info() {
    let workdir = temp_dir("new-cli-base-pkginfo");
    fs::create_dir_all(&workdir).unwrap();
    jails_cmd(&workdir, None)
        .args(["new-cli", "demo"])
        .status()
        .unwrap();

    let info = workdir.join("demo/src/main/java/com/example/demo/package-info.java");
    assert!(
        info.is_file(),
        "the project's own base package did not get a package-info"
    );
    assert!(fs::read_to_string(&info).unwrap().contains("@NullMarked"));
}

/// The same, from inside another Maven project: the root that matters is the
/// one being written to, so the package-info must describe the *new*
/// project's package rather than the surrounding one's.
#[test]
fn new_cli_inside_another_project_uses_the_new_projects_root() {
    let outer = temp_dir("new-cli-nested-root");
    fs::create_dir_all(outer.join("src/main/java/com/outer")).unwrap();
    fs::write(
        outer.join("pom.xml"),
        "<project><properties>\
         <maven.compiler.release>27</maven.compiler.release>\
         </properties><dependencies></dependencies></project>",
    )
    .unwrap();
    fs::write(
        outer.join("src/main/java/com/outer/Outer.java"),
        "package com.outer;\nclass Outer {}\n",
    )
    .unwrap();

    jails_cmd(&outer, None)
        .args(["new-cli", "demo"])
        .status()
        .unwrap();

    let info = outer.join("demo/src/main/java/com/example/demo/package-info.java");
    assert!(info.is_file(), "no package-info in the nested new project");
    let text = fs::read_to_string(&info).unwrap();
    assert!(
        text.contains("package com.example.demo;"),
        "the package-info names the surrounding project's package:\n{text}"
    );
    // And the outer project is left alone.
    assert!(
        !outer
            .join("src/main/java/com/outer/package-info.java")
            .exists()
    );
}

/// The whole point of `--gradle`: the project `build.rs`'s header names as the
/// reason jails learned to read Gradle at all is now one jails can also write.
///
/// Offline throughout. The wrapper jar is the one file this path fetches, and a
/// test that needed the network would be a test that fails on a train.
#[test]
fn new_gradle_writes_a_legacy_boot_build_file_the_maven_path_cannot_produce() {
    let parent = temp_dir("new-gradle-legacy");
    let created = jails_cmd(&parent, None)
        .args([
            "new",
            "spring",
            "--gradle",
            "--offline",
            "--boot",
            "2.7.18",
            "--java",
            "21",
            "--package",
            "com.intercom.spring",
            "--jar-name",
            "gs-rest-service",
            "--jar-version",
            "0.1.0",
            "--deps",
            "web,data-jdbc,h2",
            "--no-devtools",
            "--no-git",
        ])
        .output()
        .unwrap();
    assert!(
        created.status.success(),
        "{}{}",
        String::from_utf8_lossy(&created.stdout),
        String::from_utf8_lossy(&created.stderr)
    );
    let root = parent.join("spring");
    let build = fs::read_to_string(root.join("build.gradle")).unwrap();

    // The `buildscript {}` shape, which is the only one that applies the Boot 2
    // plugin -- `plugins { id ... version ... }` resolves through the portal.
    assert!(build.contains("buildscript {"), "{build}");
    assert!(
        build.contains("classpath(\"org.springframework.boot:spring-boot-gradle-plugin:2.7.18\")"),
        "{build}"
    );
    assert!(!build.contains("plugins {"), "{build}");
    assert!(
        build.contains("archiveBaseName = 'gs-rest-service'"),
        "{build}"
    );
    assert!(build.contains("archiveVersion = '0.1.0'"), "{build}");
    assert!(build.contains("sourceCompatibility = 21"), "{build}");

    // Boot 2 predates the `spring-boot-starter-web` -> `-webmvc` rename, and
    // the Boot 4 name resolves to nothing here.
    assert!(
        build.contains("implementation 'org.springframework.boot:spring-boot-starter-web'"),
        "{build}"
    );
    assert!(!build.contains("starter-webmvc"), "{build}");
    assert!(build.contains("runtimeOnly 'com.h2database:h2'"), "{build}");

    // Without this Gradle runs the JUnit 4 provider, collects nothing, and
    // reports success. The first version of this template omitted it and the
    // generated project was green over zero tests.
    assert!(build.contains("useJUnitPlatform()"), "{build}");

    assert_eq!(
        fs::read_to_string(root.join("settings.gradle"))
            .unwrap()
            .trim(),
        "rootProject.name = 'spring'"
    );
    assert!(
        !root.join("pom.xml").exists(),
        "--gradle must not write a pom"
    );

    // The properties are the ones this Boot line actually has. Boot 4 spellings
    // here would be silently unbound: a file that reads as configured and
    // configures nothing.
    let properties =
        fs::read_to_string(root.join("src/main/resources/application.properties")).unwrap();
    assert!(
        properties.contains("server.max-http-header-size="),
        "{properties}"
    );
    assert!(
        !properties.contains("server.max-http-request-header-size="),
        "renamed at Boot 3.0: {properties}"
    );
    assert!(
        !properties.contains("spring.mvc.problemdetails"),
        "Boot 3+: {properties}"
    );
    assert!(
        !properties.contains("spring.threads.virtual"),
        "Boot 3.2+: {properties}"
    );
    // A file written from nothing must not open with a blank line.
    assert!(
        !properties.starts_with('\n'),
        "leading blank line: {properties:?}"
    );

    // `SpringApplication` is org.springframework.boot's, and a class cannot
    // shadow the type its own main() calls.
    assert!(
        root.join("src/main/java/com/intercom/spring/Application.java")
            .is_file(),
        "expected Application.java, not SpringApplication.java"
    );
    assert!(
        !root
            .join("src/main/java/com/intercom/spring/SpringApplication.java")
            .exists()
    );
}

/// A current pin gets the other shape -- and, more importantly, a dependency
/// list jails can read back, since it has to splice this file itself.
#[test]
fn new_gradle_at_a_current_boot_uses_the_plugins_block_and_a_readable_dependency_list() {
    let parent = temp_dir("new-gradle-modern");
    let created = jails_cmd(&parent, None)
        .args([
            "new",
            "shop",
            "--gradle",
            "--offline",
            "--package",
            "com.acme.shop",
            "--no-devtools",
            "--no-git",
        ])
        .output()
        .unwrap();
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    let build = fs::read_to_string(parent.join("shop/build.gradle")).unwrap();
    assert!(
        build.contains("id 'org.springframework.boot' version"),
        "{build}"
    );
    assert!(
        build.contains(&format!("JavaLanguageVersion.of({TARGET_RELEASE})")),
        "{build}"
    );
    // Applied by id, never with a version -- the one number jails has no
    // source for.
    assert!(
        build.contains("apply plugin: 'io.spring.dependency-management'"),
        "{build}"
    );
    assert!(
        !build.contains("id 'io.spring.dependency-management'"),
        "{build}"
    );
    // The expression form `gradle::declared` cannot read. A build jails writes
    // and then cannot splice is worse than an older spelling.
    assert!(!build.contains("BOM_COORDINATES"), "{build}");
    assert!(build.contains("spring-boot-starter-webmvc"), "{build}");
    // Nobody named a jar, so there is no block to disagree with the project.
    assert!(!build.contains("bootJar"), "{build}");

    // The property that matters and is easy to lose: jails has to be able to
    // operate on the build it just wrote. `add` splices the dependency list, so
    // a build whose coordinates jails cannot read makes every capability refuse
    // on a project created thirty seconds earlier.
    let added = jails_cmd(&parent.join("shop"), None)
        .args(["add", "cors", "--no-start"])
        .output()
        .unwrap();
    assert!(
        added.status.success(),
        "jails must be able to splice its own Gradle output:\n{}{}",
        String::from_utf8_lossy(&added.stdout),
        String::from_utf8_lossy(&added.stderr)
    );
}

/// A flag that silently does nothing is the `--fast`-on-Gradle failure: it
/// looks like it worked. `--boot` especially -- the Maven path takes its Boot
/// version from start.spring.io and cannot honour a pin at all.
#[test]
fn the_gradle_only_flags_are_refused_rather_than_ignored_on_the_maven_path() {
    let parent = temp_dir("new-gradle-stray");
    for flag in [
        ["--boot", "2.7.18"],
        ["--gradle-version", "8.5"],
        ["--jar-name", "x"],
    ] {
        let refused = jails_cmd(&parent, None)
            .args(["new", "demo", "--offline"])
            .args(flag)
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&refused.stderr);
        assert!(!refused.status.success(), "{flag:?} should refuse");
        assert!(stderr.contains(flag[0]), "{flag:?}: {stderr}");
        assert!(stderr.contains("--gradle"), "{flag:?}: {stderr}");
        assert!(!parent.join("demo").exists(), "{flag:?} must write nothing");
    }
}

/// `--pretend` can be honest here and cannot be on the Maven path, because
/// jails writes this file set itself rather than unpacking whatever Initializr
/// returns.
#[test]
fn new_gradle_previews_for_real() {
    let parent = temp_dir("new-gradle-pretend");
    let preview = jails_cmd(&parent, None)
        .args(["new", "shop", "--gradle", "--pretend", "--no-git"])
        .output()
        .unwrap();
    let report = String::from_utf8_lossy(&preview.stdout);
    assert!(
        preview.status.success(),
        "{}",
        String::from_utf8_lossy(&preview.stderr)
    );
    assert!(report.contains("build.gradle"), "{report}");
    assert!(report.contains("settings.gradle"), "{report}");
    assert!(report.contains("gradle-wrapper.properties"), "{report}");
    assert!(!parent.join("shop").exists(), "--pretend wrote a project");
}
