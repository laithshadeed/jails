//! Completeness and real-toolchain gates for checked-in example manifests.
//!
//! The large production-style examples have their shared service gate in
//! `app.rs`. This module owns the remaining examples and makes the checked-in
//! cost policy exhaustive, so adding a manifest without choosing a tier fails
//! an offline test.

use super::*;
use std::collections::BTreeSet;
use std::process::Command;

const POLICY: &str = include_str!("../../examples/proof-policy.tsv");
const MINICOM_MANIFEST: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/examples/minicom/.jails/app.toml"
);
const MINICOM_SPRING_MANIFEST: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/examples/minicom-spring/.jails/app.toml"
);

#[derive(Debug)]
struct ProofPolicy<'a> {
    manifest: &'a str,
    build_tool: &'a str,
    highest_tier: &'a str,
    cadence: &'a str,
    gate: &'a str,
    prerequisites: &'a str,
}

fn proof_policy() -> Vec<ProofPolicy<'static>> {
    let mut lines = POLICY.lines().filter(|line| !line.starts_with('#'));
    assert_eq!(
        lines.next(),
        Some("manifest\tbuild_tool\thighest_tier\tcadence\tgate\tprerequisites")
    );
    lines
        .map(|line| {
            let columns = line.split('\t').collect::<Vec<_>>();
            assert_eq!(columns.len(), 6, "policy row must have six columns: {line}");
            ProofPolicy {
                manifest: columns[0],
                build_tool: columns[1],
                highest_tier: columns[2],
                cadence: columns[3],
                gate: columns[4],
                prerequisites: columns[5],
            }
        })
        .collect()
}

fn checked_in_manifests() -> BTreeSet<String> {
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples");
    fs::read_dir(&examples)
        .unwrap()
        .flatten()
        .filter_map(|entry| {
            let manifest = entry.path().join(".jails/app.toml");
            manifest.is_file().then(|| {
                manifest
                    .strip_prefix(env!("CARGO_MANIFEST_DIR"))
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            })
        })
        .collect()
}

#[test]
fn example_manifest_policy_covers_every_checked_in_manifest() {
    let policy = proof_policy();
    let policy_paths = policy
        .iter()
        .map(|row| row.manifest.to_owned())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        policy_paths.len(),
        policy.len(),
        "duplicate manifest policy row"
    );
    assert_eq!(policy_paths, checked_in_manifests());

    let gates = [
        "app_manifests_pass_the_full_generated_verification_gate",
        "ledger_cli_manifest_builds_without_spring",
        "unheld_gradle_example_manifest_builds_on_its_pinned_toolchain",
        "unheld_maven_example_manifest_passes_real_verification",
    ]
    .into_iter()
    .collect::<BTreeSet<_>>();
    for row in policy {
        assert!(matches!(row.build_tool, "maven" | "gradle"), "{row:?}");
        assert!(matches!(row.highest_tier, "1" | "2"), "{row:?}");
        assert_eq!(row.cadence, "default-if-available", "{row:?}");
        assert!(gates.contains(row.gate), "unknown automated gate: {row:?}");
        assert!(!row.prerequisites.is_empty(), "{row:?}");
    }
}

/// A broken container engine cannot touch what `jails new --app` produces.
///
/// The model is compiled and its exact plan executed, and nothing external is
/// started -- the same reason `sync` refuses `--no-start` by name -- so the
/// engine being unusable changes nothing about what is generated.
#[test]
fn a_failed_post_commit_effect_reports_against_a_project_that_exists() {
    let parent = temp_dir("new-app-effect-failure");
    fs::create_dir_all(&parent).unwrap();
    // A compose engine that refuses everything.
    let tools = parent.join("tools");
    fs::create_dir_all(&tools).unwrap();
    let docker = tools.join("docker");
    fs::write(
        &docker,
        "#!/bin/sh\necho 'compose is unavailable' >&2\nexit 1\n",
    )
    .unwrap();
    set_executable(&docker);
    let manifest = parent.join("app.toml");
    fs::write(
        &manifest,
        "schema = 1\ncapabilities = [\"db\"]\n\n[[generate]]\nkind = \"scaffold\"\nname = \"Deal\"\nfields = [\"id:uuid@pk\", \"amount:decimal\"]\n",
    )
    .unwrap();

    let path = format!(
        "{}:{}",
        tools.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let output = jails_cmd(&parent, None)
        .env("PATH", path)
        .args([
            "new",
            "effectapp",
            "--offline",
            "--no-git",
            "--app",
            manifest.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    let rendered = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(0), "{rendered}");
    let project = parent.join("effectapp");
    assert!(project.is_dir(), "the project was discarded:\n{rendered}");
    // Into the managed tree, because the project is canonical from its first
    // command: `jails new` seeds `.jails/model.jdl`, and the manifest replays
    // into it through the same frontends `jails g` uses.
    assert!(
        project
            .join("src/main/java/com/example/effectapp/domain/Deal.java")
            .is_file(),
        "the manifest's own output is missing:\n{rendered}"
    );
    assert!(
        project.join(".jails/model.jdl").is_file(),
        "the project is not canonical:\n{rendered}"
    );
}

fn generated_unheld_maven_example() -> &'static PathBuf {
    static GENERATED: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    GENERATED.get_or_init(|| {
        let parent = temp_dir("example-minicom-maven");
        let output = jails_cmd(&parent, None)
            .args([
                "new",
                "minicom",
                "--offline",
                "--no-devtools",
                "--no-git",
                // The manifest declares Compose services. Proving it
                // *generates* must not depend on a container engine being up
                // on the machine running the suite.
                "--no-start",
                "--app",
                MINICOM_MANIFEST,
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "minicom generation: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        parent.join("minicom")
    })
}

fn generated_unheld_gradle_example() -> &'static PathBuf {
    static GENERATED: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    GENERATED.get_or_init(|| {
        let parent = temp_dir("example-minicom-gradle");
        let output = jails_cmd(&parent, None)
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
                // Not `h2`: the manifest declares it as a capability, and a
                // dependency the reader's own build block also names is a
                // second editable source for the same fact -- which the
                // Gradle adapter refuses rather than adopting.
                "web,data-jdbc",
                "--no-devtools",
                "--no-git",
                // The manifest declares Compose services. Proving it
                // *generates* must not depend on a container engine being up
                // on the machine running the suite.
                "--no-start",
                "--app",
                MINICOM_SPRING_MANIFEST,
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "minicom-spring generation: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        parent.join("spring")
    })
}

fn assert_second_apply_is_a_noop(root: &Path) {
    // The executable project is the example's output contract; `.jails` is
    // versioned bookkeeping with its own gates, and `target/` is the build
    // tool's, written by the verification test that shares this fixture and
    // may be running at the same time.
    let generated_tree = || {
        snapshot_tree(root)
            .into_iter()
            .filter(|(path, _)| {
                !path.strip_prefix(root).is_ok_and(|relative| {
                    relative.starts_with(".jails") || relative.starts_with("target")
                })
            })
            .collect::<Vec<_>>()
    };
    let before = generated_tree();
    let output = jails_cmd(root, None)
        .args(["app", "apply", "--no-start"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "second manifest apply: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let after = generated_tree();
    let mut changed: Vec<String> = Vec::new();
    let before_by_path: std::collections::BTreeMap<_, _> = before.iter().cloned().collect();
    let after_by_path: std::collections::BTreeMap<_, _> = after.iter().cloned().collect();
    for (path, bytes) in &after_by_path {
        match before_by_path.get(path) {
            None => changed.push(format!("created {}", path.display())),
            Some(previous) if previous != bytes => {
                changed.push(format!("rewrote {}", path.display()));
            }
            Some(_) => {}
        }
    }
    for path in before_by_path.keys() {
        if !after_by_path.contains_key(path) {
            changed.push(format!("removed {}", path.display()));
        }
    }
    assert!(
        changed.is_empty(),
        "second apply changed generated output:\n  {}",
        changed.join("\n  ")
    );
}

#[test]
fn unheld_example_manifests_generate_offline_and_reapply_without_writes() {
    assert_second_apply_is_a_noop(generated_unheld_maven_example());
    assert_second_apply_is_a_noop(generated_unheld_gradle_example());
}

#[test]
fn unheld_maven_example_manifest_passes_real_verification() {
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
    if !real_docker_available() {
        skip("a running Docker-compatible container runtime is required");
        return;
    }

    let root = generated_unheld_maven_example();
    let path = real_path_without_mvnd();
    let status = jails_cmd_with_path(root, &path)
        .arg("check")
        .status()
        .unwrap();
    assert!(
        status.success(),
        "the exact minicom manifest failed jails check"
    );
    assert_eq!(
        maven_report_summary(root, "surefire-reports"),
        MavenReportSummary {
            // What is pinned: one controller test per routed operation
            // (`SendMessage`, `MarkAsRead`, `Conversation`, `UnreadForEmail`,
            // `EnsureUser`), each issuing a real request through the
            // dispatcher with a body built from the model's own JSON samples;
            // `MessageControllerTest` and `UserControllerTest` for the `http`
            // facet's resources; `ArchitectureTest`, whose seven ArchUnit
            // rules check the reader's code as well as jails' own; and
            // `MessageDtoTest` and `UserDtoTest`, two cases each, holding the
            // request boundary to what a caller supplies. There is no
            // per-operation use-case unit test: an operation is its port plus
            // a JDBC adapter, and the adapter is proved against a real
            // database below rather than against a mock here.
            reports: 20,
            tests: 48,
            failures: 0,
            errors: 0,
            skipped: 0,
        }
    );
    assert_eq!(
        maven_report_summary(root, "failsafe-reports"),
        MavenReportSummary {
            // Failsafe must run every `*IT` the compiler emits; a project that
            // cannot run them reports success over nothing. The ITs are one
            // per JDBC query adapter, one per write operation (`SendMessage`,
            // `EnsureUser`, `MarkAsRead`), the presence adapter's three cases,
            // and an `AssociationIT` per declared relation. They store a row
            // through the entity's own repository and read it back through the
            // operation, against the real PostgreSQL `add db` wires -- the
            // only place a quoted join alias, a foreign key, a `timestamptz`
            // bind, an enum parameter's SQL type and `cast(:x as text) is
            // null` mean anything.
            reports: 10,
            tests: 13,
            failures: 0,
            errors: 0,
            skipped: 0,
        }
    );
}

/// The pinned Gradle 8.5 / JDK 21 pair, located rather than inherited, so the
/// test runs inside the ordinary suite instead of needing the gate re-entered
/// under `mise x java@21 gradle@8.5`.
///
/// The ambient pair is honoured first, so running this test by hand under
/// `mise x` works. mise is the fallback and not an extra dependency: it is
/// what invokes the gate.
fn pinned_gradle_toolchain() -> Option<(PathBuf, Option<PathBuf>)> {
    if pinned_gradle_reports_eight_five(Command::new("gradle")) {
        return Some((PathBuf::from("gradle"), None));
    }
    // `mise which --tool=` resolves the executable itself. `mise where`
    // answers with the install root, and the layout under it is not uniform
    // -- Gradle 8.5 unpacks to `<root>/gradle-8.5/bin/gradle`, so a joined
    // `bin/gradle` does not exist.
    let located = |tool: &str, exe: &str| {
        let output = Command::new("mise")
            .args(["which", exe, &format!("--tool={tool}")])
            .output()
            .ok()?;
        output
            .status
            .success()
            .then(|| PathBuf::from(String::from_utf8_lossy(&output.stdout).trim().to_owned()))
            .filter(|path| path.is_file())
    };
    // Gradle wants a JDK *home*, and what is resolvable is the `java` binary
    // inside `<home>/bin`, so the home is two levels up from it.
    let java_home = located("java@21", "java")?
        .parent()?
        .parent()?
        .to_path_buf();
    let gradle = located("gradle@8.5", "gradle")?;
    let mut probe = Command::new(&gradle);
    probe.env("JAVA_HOME", &java_home);
    pinned_gradle_reports_eight_five(probe).then_some((gradle, Some(java_home)))
}

/// Whether this command is Gradle 8.5 running on JDK 21, both of which the
/// example proof policy pins and neither of which is the repository default.
fn pinned_gradle_reports_eight_five(mut command: Command) -> bool {
    command.arg("--version").output().is_ok_and(|output| {
        let version = String::from_utf8_lossy(&output.stdout);
        output.status.success()
            && version.contains("Gradle 8.5")
            && version.lines().any(|line| {
                line.strip_prefix("JVM:")
                    .is_some_and(|value| value.trim_start().starts_with("21"))
            })
    })
}

#[test]
fn unheld_gradle_example_manifest_builds_on_its_pinned_toolchain() {
    let Some((gradle, java_home)) = pinned_gradle_toolchain() else {
        skip("Gradle 8.5 running on JDK 21 is required by the example proof policy");
        return;
    };
    let root = generated_unheld_gradle_example();
    let mut build = Command::new(&gradle);
    build.current_dir(root).args(["--no-daemon", "build"]);
    if let Some(java_home) = &java_home {
        build.env("JAVA_HOME", java_home);
    }
    let status = build.status().unwrap();
    assert!(
        status.success(),
        "the exact minicom-spring manifest failed Gradle build"
    );
    let reports = xml_test_report_summary(&root.join("build/test-results/test"));
    assert_eq!(reports.tests, 8, "Gradle must collect every generated test");
    assert_eq!(
        reports.skipped, 3,
        "only the three honest handler stubs are disabled"
    );
    assert_eq!(reports.failures, 0);
    assert_eq!(reports.errors, 0);
}
