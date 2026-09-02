//! Black-box baseline for the test/run loop.
//!
//! Fake build tools make the observed value jails' argv and exit/result
//! contract rather than whichever Maven or Gradle happens to be installed, so
//! a routing regression is distinguishable from a deliberate change.

use super::*;

#[derive(Debug)]
struct Case<'a> {
    build: &'a str,
    command: &'a str,
    outcome: &'a str,
    evidence: &'a str,
}

fn cases() -> Vec<Case<'static>> {
    include_str!("../../docs/black-box-behavior.tsv")
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let mut columns = line.split('\t');
            let case = Case {
                build: columns.next().unwrap(),
                command: columns.next().unwrap(),
                outcome: columns.next().unwrap(),
                evidence: columns.next().unwrap(),
            };
            assert!(
                columns.next().is_none(),
                "behaviour matrix row has extra columns: {line}"
            );
            case
        })
        .collect()
}

fn write_fixture(root: &Path, build: &str) {
    let source = common::generated(root, "src/main/java/com/example/demo/DemoApplication.java");
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::write(
        source,
        "package com.example.demo;\n\npublic class DemoApplication {}\n",
    )
    .unwrap();
    match build {
        "maven" => {
            write_plain_fixture(root);
            let pom = fs::read_to_string(root.join("pom.xml")).unwrap();
            fs::write(
                root.join("pom.xml"),
                pom.replace(
                    "<project>",
                    "<project>\n    <!-- org.springframework.boot: baseline routing marker -->",
                ),
            )
            .unwrap();
        }
        "gradle" => {
            fs::write(root.join("settings.gradle"), "rootProject.name = 'demo'\n").unwrap();
            fs::write(
                root.join("build.gradle"),
                "plugins {\n    id 'java'\n    id 'org.springframework.boot' version '4.0.0'\n}\n",
            )
            .unwrap();
        }
        other => panic!("unknown matrix build '{other}'"),
    }
}

#[test]
fn maven_and_gradle_command_results_match_the_checked_in_baseline() {
    let cases = cases();
    assert_eq!(
        cases.len(),
        12,
        "the matrix must keep both builds and all rows"
    );

    for (number, case) in cases.iter().enumerate() {
        let root = temp_dir(&format!("behaviour-{}-{number}", case.build));
        write_fixture(&root, case.build);
        let tools = temp_dir(&format!("behaviour-tools-{}-{number}", case.build));
        let log = tools.join("log.txt");
        write_fake_maven(&tools, &["mvn", "gradle"], &log);

        let output = jails_cmd(&root, Some(&tools))
            .args(case.command.split_ascii_whitespace())
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        match case.outcome {
            "success" => assert!(
                output.status.success(),
                "{} '{}' unexpectedly refused:\n{stdout}{stderr}",
                case.build,
                case.command
            ),
            "refused" => assert!(
                !output.status.success(),
                "{} '{}' unexpectedly succeeded:\n{stdout}{stderr}",
                case.build,
                case.command
            ),
            other => panic!("unknown matrix outcome '{other}'"),
        }

        let invocation = read_log(&log);
        for expectation in case.evidence.split('|') {
            let (where_, needle) = expectation
                .split_once(':')
                .unwrap_or_else(|| panic!("evidence needs a location: {expectation}"));
            let actual = match where_ {
                "stdout" => stdout.as_ref(),
                "stderr" => stderr.as_ref(),
                "tool" => invocation.as_str(),
                other => panic!("unknown evidence location '{other}'"),
            };
            assert!(
                actual.contains(needle),
                "{} '{}' expected {where_} to contain '{needle}'.\nstdout:\n{stdout}\nstderr:\n{stderr}\ntool log:\n{invocation}",
                case.build,
                case.command
            );
        }
    }
}
