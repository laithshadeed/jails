//! Completeness and real-toolchain gates for checked-in example manifests.
//!
//! The large production-style examples retain their shared service gate in
//! `app.rs`. This module owns the two previously unheld examples and makes the
//! checked-in cost policy exhaustive, so adding a manifest without choosing a
//! tier fails an offline test.

use super::*;
use std::collections::BTreeSet;

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
                "web,data-jdbc,h2",
                "--no-devtools",
                "--no-git",
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
            reports: 17,
            tests: 50,
            failures: 0,
            errors: 0,
            skipped: 0,
        }
    );
    assert_eq!(
        maven_report_summary(root, "failsafe-reports"),
        MavenReportSummary {
            reports: 5,
            tests: 6,
            failures: 0,
            errors: 0,
            skipped: 0,
        }
    );
}

fn pinned_gradle_toolchain_available() -> bool {
    std::process::Command::new("gradle")
        .arg("--version")
        .output()
        .is_ok_and(|output| {
            let version = String::from_utf8_lossy(&output.stdout);
            output.status.success()
                && version.contains("Gradle 8.5")
                && version.lines().any(|line| line.starts_with("JVM: 21"))
        })
}

#[test]
fn unheld_gradle_example_manifest_builds_on_its_pinned_toolchain() {
    if !pinned_gradle_toolchain_available() {
        skip("Gradle 8.5 running on JDK 21 is required by the example proof policy");
        return;
    }
    let root = generated_unheld_gradle_example();
    let status = std::process::Command::new("gradle")
        .current_dir(root)
        .args(["--no-daemon", "build"])
        .status()
        .unwrap();
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
