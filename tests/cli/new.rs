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
    // The entry point is a dispatcher, not a Hello World stub -- otherwise
    // `generate command` has nothing to register into, which is the whole point
    // of `new-cli`. Asserted against the bytes `new-cli` actually writes.
    let app = fs::read_to_string(common::generated(
        &root,
        "src/main/java/com/example/demo/App.java",
    ))
    .unwrap();
    assert!(app.contains("package com.example.demo;"), "{app}");
    assert!(
        app.contains("public static void main(String[] args)"),
        "{app}"
    );
    assert!(app.contains("public final class App"), "{app}");
    assert!(
        app.contains("usage: demo <command> [args]"),
        "the program name should be the project's: {app}"
    );
    assert!(
        jails_codemod::dispatch::is_dispatcher(&app),
        "`generate command` must be able to find this: {app}"
    );
    let app_test = fs::read_to_string(common::generated(
        &root,
        "src/test/java/com/example/demo/AppTest.java",
    ))
    .unwrap();
    assert!(
        app_test.contains("import org.junit.jupiter.api.Test;"),
        "{app_test}"
    );
    assert!(app_test.contains("class AppTest"), "{app_test}");
    assert!(app_test.contains("App.run("), "{app_test}");
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
        common::generated(
            &root,
            "src/main/java/com/example/demoapp/DemoAppApplication.java"
        )
        .is_file()
    );
    assert!(
        common::generated(
            &root,
            "src/test/java/com/example/demoapp/DemoAppApplicationTests.java"
        )
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
fn new_refuses_a_project_name_that_maven_cannot_use() {
    let workdir = temp_dir("new-invalid-artifact-id");
    for args in [
        vec!["new", "my app with spaces", "--offline"],
        vec!["new-cli", "my app with spaces"],
    ] {
        let output = jails_cmd(&workdir, None).args(&args).output().unwrap();
        assert!(!output.status.success(), "{args:?} unexpectedly succeeded");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("valid Maven artifact id"), "{stderr}");
        assert!(stderr.contains("my-app"), "{stderr}");
    }
    assert!(!workdir.join("my app with spaces").exists());
}

#[test]
fn new_cli_with_an_app_manifest_is_one_command_from_an_empty_directory() {
    // `new` + `mkdir .jails` + `cp app.toml` + `app apply` is four steps that
    // only ever appear together, so `--app` is one command.
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
    // ...and the project it created holds a model, like every other project
    // `new-cli` makes. A manifest is not a second editable source because it
    // replays *into* the model; nothing is written outside the managed tree.
    assert!(
        root.join(".jails/model.jdl").is_file(),
        "a manifest-driven project should be canonical like any other"
    );
    assert!(
        !root.join(".jails/ledger.toml").exists(),
        "and must not also carry a legacy ledger"
    );
    // ...and its rows are already applied, against the project that was just
    // created rather than whatever encloses the process CWD.
    assert!(
        root.join("src/main/java/com/example/demo/domain/Entry.java")
            .is_file(),
        "the manifest's row should have been applied into the project"
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

    // The destination is absent or complete. Everything `new-cli` writes goes
    // into a scratch sibling and becomes real in one rename, so a manifest
    // that cannot be read leaves nothing that reads like a project.
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

/// Can Maven *read* what jails wrote?
///
/// `mvn -o validate` parses the pom and stops -- no downloads, no compilation.
/// A versionless dependency is correct under `spring-boot-starter-parent` and
/// fatal without one (every goal fails with `'dependencies.dependency.version'
/// ... is missing`, `validate` included), so the matrix is over the flavour of
/// project.
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

/// A creation names every file the reader did not ask for, and says what to
/// do next.
///
/// `Created ./demo` and nothing else leaves a reader to discover an
/// `AGENTS.md`, a `mise.toml` and a `.jails/` with `ls`. The list is read off
/// the staged tree, so a seed that gains a file says so without anyone
/// remembering to update a message.
#[test]
fn a_creation_names_the_files_the_reader_did_not_ask_for() {
    let parent = temp_dir("new-creation-report");
    let created = jails_cmd(&parent, None)
        .args(["new", "demo", "--offline", "--no-git"])
        .output()
        .unwrap();
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    let report = String::from_utf8_lossy(&created.stdout).to_string();
    for named in [
        "AGENTS.md",
        "mise.toml",
        ".jails/model.jdl",
        ".jails/compiler.lock.json",
        "source file(s) under src/",
        "next: cd demo && jails run",
    ] {
        assert!(
            report.contains(named),
            "the report names {named}:\n{report}"
        );
    }
    // The build file and the sources are what `new` was asked for; the
    // executor's own lock is scratch the ignore file already names.
    assert!(!report.contains("pom.xml"), "{report}");
    assert!(!report.contains("apply.lock"), "{report}");

    // Every file outside `src/` and the build file is named, which is the
    // item's own bar: read the tree and hold the report to it.
    let root = parent.join("demo");
    let mut unnamed = Vec::new();
    let mut pending = vec![root.clone()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory).unwrap().flatten() {
            let path = entry.path();
            if entry.file_type().unwrap().is_dir() {
                pending.push(path);
                continue;
            }
            let relative = path
                .strip_prefix(&root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            let asked = relative.starts_with("src/")
                || relative == "pom.xml"
                || relative == ".jails/apply.lock";
            if !asked && !report.contains(&relative) {
                unnamed.push(relative);
            }
        }
    }
    assert!(
        unnamed.is_empty(),
        "these files were written and not named: {unnamed:?}\n{report}"
    );
}

/// A single-module project is a project, not a reactor with one module.
#[test]
fn about_speaks_of_a_project_when_there_is_one_module() {
    let parent = temp_dir("about-single-module");
    let created = jails_cmd(&parent, None)
        .args(["new", "demo", "--offline", "--no-git"])
        .output()
        .unwrap();
    assert!(created.status.success());
    let root = parent.join("demo");
    let about = jails_cmd(&root, None).arg("about").output().unwrap();
    assert!(about.status.success());
    let printed = String::from_utf8_lossy(&about.stdout).to_string();
    let lines = printed.lines().count();
    assert!(
        lines <= 5,
        "a single-module `about` is five lines:\n{printed}"
    );
    assert!(printed.starts_with("Project: demo"), "{printed}");
    for absent in ["Reactor:", "Modules", "(none)"] {
        assert!(
            !printed.contains(absent),
            "{absent} is Maven's word:\n{printed}"
        );
    }
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

/// A file writer must not rediscover the project from the process CWD, which
/// is not the project being written to when `new-cli` creates a directory the
/// CWD is not inside. The visible property: a `new-cli` project's own base
/// package gets the null-marked `package-info.java` every other package gets.
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
    fs::create_dir_all(common::generated(&outer, "src/main/java/com/outer")).unwrap();
    fs::write(
        outer.join("pom.xml"),
        "<project><properties>\
         <maven.compiler.release>27</maven.compiler.release>\
         </properties><dependencies></dependencies></project>",
    )
    .unwrap();
    fs::write(
        common::generated(&outer, "src/main/java/com/outer/Outer.java"),
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

/// `--gradle` writes a Boot 2 Gradle project jails can also read.
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
    // reports success over zero tests.
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
        common::generated(&root, "src/main/java/com/intercom/spring/Application.java").is_file(),
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

/// jails picks the Gradle distribution and the Java release, so it must not
/// pick a pair it has seen fail: `--boot 2.7.18` pins Gradle 8.5 (Boot 2 does
/// not run on Gradle 9.x) while `--java` defaults to the current release, and
/// Gradle 8.5 dies on JDK 26 in its own build script before reading one line
/// of the project.
///
/// Only a *measured* pairing refuses: `gradle::MEASURED` holds the runs, and
/// anything absent from it is `Unknown` and allowed, because refusing on a
/// guess would block a reader who pinned `--gradle-version` themselves.
#[test]
fn a_gradle_project_is_not_created_with_a_jdk_its_gradle_cannot_run() {
    let root = temp_dir("gradle-jdk-pairing");

    let refused = jails_cmd(&root, None)
        .args([
            "new", "legacy", "--gradle", "--boot", "2.7.18", "--java", "26",
        ])
        .output()
        .unwrap();
    assert!(
        !refused.status.success(),
        "Gradle 8.5 with JDK 26 was accepted"
    );
    let stderr = String::from_utf8_lossy(&refused.stderr);
    // The message names both numbers jails chose and the error the reader
    // would otherwise have met, which names neither.
    assert!(
        stderr.contains("Gradle 8.5 does not run on JDK 26"),
        "{stderr}"
    );
    assert!(
        stderr.contains("Unsupported class file major version 70"),
        "{stderr}"
    );
    assert!(stderr.contains("--java 21"), "{stderr}");
    assert!(!root.join("legacy").exists(), "a directory was published");

    // The way out the refusal names is a pairing that was actually run.
    let accepted = jails_cmd(&root, None)
        .args([
            "new", "legacy", "--gradle", "--boot", "2.7.18", "--java", "21",
        ])
        .output()
        .unwrap();
    assert!(
        accepted.status.success(),
        "{}",
        String::from_utf8_lossy(&accepted.stderr)
    );

    // And the pairing jails defaults to for Boot 4 is measured green, so the
    // check must not fire on the ordinary path.
    let modern = jails_cmd(&root, None)
        .args(["new", "current", "--gradle", "--java", "26"])
        .output()
        .unwrap();
    assert!(
        modern.status.success(),
        "{}",
        String::from_utf8_lossy(&modern.stderr)
    );
}
