//! Completeness and real-toolchain gates for checked-in example manifests.
//!
//! The large production-style examples retain their shared service gate in
//! `app.rs`. This module owns the two previously unheld examples and makes the
//! checked-in cost policy exhaustive, so adding a manifest without choosing a
//! tier fails an offline test.

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
/// **The original property was weaker, and it is worth keeping the history.**
/// `jails new --app` publishes by rename, so an error thrown out of the
/// manifest apply discarded the whole scratch tree. A compose service that
/// would not start -- an ordinary state on a machine where something else
/// already holds `:5432` -- printed `ledger create`, exited 1, and left no
/// directory: no `jails:` line, no project, and no way to tell which of the
/// two had happened. The fix made the effect report *against a project that
/// exists*, so this test asserted an exit of 1 beside a complete tree.
///
/// A canonical project has no post-commit effects at all -- the model is
/// compiled and its exact plan executed, and nothing external is started, which
/// is the same reason `sync` refuses `--no-start` by name. So the failure this
/// was written for cannot occur, and the guarantee is now the stronger one:
/// the engine being unusable changes nothing about what is generated.
#[test]
fn a_failed_post_commit_effect_reports_against_a_project_that_exists() {
    let parent = temp_dir("new-app-effect-failure");
    fs::create_dir_all(&parent).unwrap();
    // A compose engine that refuses everything, so the effect fails for a
    // reason that has nothing to do with what was generated.
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
            .join(".jails/generated/main/java/com/example/effectapp/domain/Deal.java")
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
    // The first revisit may upgrade durable bookkeeping (for example sealing
    // newly published migrations). The executable project is the example's
    // output contract; `.jails` is the engine's versioned state, and is tested
    // by its own protocol/ledger gates.
    let generated_tree = || {
        snapshot_tree(root)
            .into_iter()
            .filter(|(path, _)| {
                !path
                    .strip_prefix(root)
                    .is_ok_and(|relative| relative.starts_with(".jails"))
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
    assert_eq!(
        generated_tree(),
        before,
        "second apply changed generated output"
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
            // **These are the canonical compiler's numbers, and they are
            // lower than the legacy engine's 21 and 57.** The difference is
            // not lost checks but a different shape: canonical emits an
            // `HttpPort` for an entity where legacy emitted a controller and
            // its test, and it has no per-operation use-case unit test because
            // an operation *is* its port plus a JDBC adapter, and the adapter
            // is proved against a real database below rather than against a
            // mock here.
            //
            // What is pinned here is one controller test per routed operation
            // -- `SendMessage`, `MarkAsRead`, `Conversation`, `UnreadForEmail`
            // and `EnsureUser` -- each issuing a real request through the
            // dispatcher with a body built from the model's own JSON samples.
            // Canonical emitted the adapters and nothing that proved them
            // until this pin was written.
            //
            // 15 -> 17 reports, 33 -> 35 tests: `MessageControllerTest` and
            // `UserControllerTest`, from the `http` facet finally serving the
            // resource it declares. It emitted a port nothing implemented, so
            // the sentence above about canonical emitting an `HttpPort` "where
            // legacy emitted a controller and its test" no longer describes a
            // difference in shape -- it described a missing endpoint.
            //
            // 17 -> 18 reports, 35 -> 42 tests: `ArchitectureTest`, whose
            // seven ArchUnit rules the compiler now emits for any model that
            // serves a resource. It is the one generated test that checks the
            // *reader's* code as well as jails' own, which is why it counts
            // for seven where a controller test counts for one.
            //
            // 18 -> 20 reports, 42 -> 46 tests: `MessageDtoTest` and
            // `UserDtoTest`, two cases each. A scaffold declares the request
            // boundary now, so every resource ships the request and response
            // records and the test that holds them to what a caller supplies.
            // 46 -> 48: the error model gained the two outcomes a transition
            // can have when its `If-Match` does not match -- 412 for a version
            // that moved on, 404 for a row that is not there. Both reached the
            // client as a 500 before, which is what alerting pages on.
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
            // **Failsafe ran nothing at all before this.** The canonical
            // path never added the plugin, so every `*IT` it emitted -- the
            // presence adapter's included -- sat in a project that could not
            // run it while `mvn verify` reported success. That is the exact
            // failure `CLAUDE.md` records for the legacy engine, reintroduced.
            //
            // The four here are one per JDBC query adapter plus the presence
            // adapter's three cases. They store a row through the entity's own
            // repository and read it back through the operation, against the
            // real PostgreSQL `add db` wires -- which is the only place a
            // quoted join alias, a foreign key, a `timestamptz` bind and
            // `cast(:x as text) is null` mean anything. Writing them found
            // three defects nothing else could: a `--via` join that reached no
            // emitter, an `Instant` the driver cannot infer a type for, and a
            // `generated always as identity` key that made `save` impossible.
            //
            // 4 -> 7 reports, 6 -> 9 tests: the write half. `SendMessage` and
            // `EnsureUser` are commands and `MarkAsRead` is a transition, and
            // their `insert ... returning` and `update ... returning` were
            // asserted by nothing -- so only the repository adapter's
            // parameters went through `bound_value`, and an enum reached
            // PostgreSQL raw. `Can't infer the SQL type to use for an instance
            // of MessageDirection`, from a statement that compiled.
            //
            // 7 -> 10 reports, 9 -> 13 tests: three declared relations gained
            // an `AssociationIT` each. A foreign key is the one thing in a
            // generated project no unit test can observe, and the catalogue
            // half of that proof is what catches a mapping written backwards.
            reports: 10,
            tests: 13,
            failures: 0,
            errors: 0,
            skipped: 0,
        }
    );
}

/// The pinned Gradle 8.5 / JDK 21 pair, **located rather than inherited**.
///
/// This test used to require the whole gate to be re-entered under
/// `mise x java@21 gradle@8.5`, because it read `gradle` and `JAVA_HOME`
/// straight off the ambient environment. That cost a second `cargo test`
/// invocation running exactly one test, serialised after everything else --
/// 29.15s measured, against a cargo overhead of 0.1s, so essentially all of
/// it was the Gradle build waiting its turn. Located here instead, the test
/// runs inside the ordinary suite and those 29s overlap a 299s test phase
/// rather than following it.
///
/// The ambient pair is still honoured first, so running this test by hand
/// under `mise x` behaves exactly as it did. `mise where` is the fallback,
/// and mise is not an extra dependency: it is what invokes the gate.
fn pinned_gradle_toolchain() -> Option<(PathBuf, Option<PathBuf>)> {
    if pinned_gradle_reports_eight_five(Command::new("gradle")) {
        return Some((PathBuf::from("gradle"), None));
    }
    // `mise which --tool=` resolves the executable itself. `mise where` was
    // the obvious call and is the wrong one: it answers with the *install
    // root*, and the layout under it is not uniform -- Gradle 8.5 unpacks to
    // `<root>/gradle-8.5/bin/gradle`, so a joined `bin/gradle` does not exist
    // and the probe silently reported the toolchain missing.
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
