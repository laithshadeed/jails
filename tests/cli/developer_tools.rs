//! Portable contracts and transparent developer tools through the real CLI.

use super::*;

fn web_fixture(label: &str) -> PathBuf {
    let root = temp_dir(label);
    write_project_skeleton(&root);
    fs::create_dir_all(root.join("src/main/java/com/example/demo/web")).unwrap();
    fs::write(
        root.join("src/main/java/com/example/demo/web/NoteController.java"),
        "package com.example.demo.web;\n@RestController\n@RequestMapping(\"/notes\")\nfinal class NoteController { @GetMapping(\"/{id}\") String get() { return \"ok\"; } }\n",
    )
    .unwrap();
    root
}

#[test]
fn contract_emit_and_check_catch_a_removed_operation() {
    let root = web_fixture("contract-check");
    let emitted = jails_cmd(&root, None)
        .args(["contract", "emit", "--format", "openapi"])
        .output()
        .unwrap();
    assert!(
        emitted.status.success(),
        "{}",
        String::from_utf8_lossy(&emitted.stderr)
    );
    let document = String::from_utf8_lossy(&emitted.stdout);
    assert!(document.contains("\"openapi\": \"3.1.0\""), "{document}");
    assert!(document.contains("source-observed"), "{document}");
    let baseline = root.join("baseline.json");
    fs::write(&baseline, &emitted.stdout).unwrap();

    fs::remove_file(root.join("src/main/java/com/example/demo/web/NoteController.java")).unwrap();
    let checked = jails_cmd(&root, None)
        .args(["contract", "check", "--against", baseline.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!checked.status.success());
    let stdout = String::from_utf8_lossy(&checked.stdout);
    assert!(stdout.contains("BREAKING"), "{stdout}");
    assert!(stdout.contains("GET /notes/{id}"), "{stdout}");
}

#[test]
fn contract_out_uses_preview_and_transaction_commit() {
    let root = web_fixture("contract-out");
    let target = root.join(".jails/contracts/openapi.json");
    let preview = jails_cmd(&root, None)
        .args([
            "--pretend",
            "contract",
            "emit",
            "--out",
            ".jails/contracts/openapi.json",
        ])
        .output()
        .unwrap();
    assert!(
        preview.status.success(),
        "{}",
        String::from_utf8_lossy(&preview.stderr)
    );
    assert!(!target.exists(), "contract preview wrote its output");
    assert!(String::from_utf8_lossy(&preview.stdout).contains("openapi.json"));

    let applied = jails_cmd(&root, None)
        .args(["contract", "emit", "--out", ".jails/contracts/openapi.json"])
        .output()
        .unwrap();
    assert!(
        applied.status.success(),
        "{}",
        String::from_utf8_lossy(&applied.stderr)
    );
    let document = fs::read_to_string(target).unwrap();
    assert!(document.contains("\"openapi\": \"3.1.0\""));
    assert!(root.join(".jails/receipts").is_dir());
}

#[test]
fn request_print_is_exact_redacted_and_does_not_launch_curl() {
    let root = web_fixture("request-print");
    let fake = temp_dir("request-print-bin");
    let log = fake.join("curl.log");
    write_fake_maven(&fake, &["curl"], &log);
    let output = jails_cmd(&root, Some(&fake))
        .env("REQUEST_TOKEN", "top-secret")
        .args([
            "request",
            "GET",
            "/notes/{id}",
            "--base-url",
            "http://127.0.0.1:8080",
            "--param",
            "id=a/b",
            "--query",
            "verbose=true",
            "--header-env",
            "Authorization=REQUEST_TOKEN",
            "--print",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("curl --silent --show-error --fail-with-body"),
        "{stdout}"
    );
    assert!(stdout.contains("/notes/a%2Fb"), "{stdout}");
    assert!(stdout.contains("<redacted:headers>"), "{stdout}");
    assert!(!stdout.contains("top-secret"), "{stdout}");
    assert!(read_log(&log).is_empty(), "curl launched in --print mode");
}

#[test]
fn db_console_defaults_to_pgcli_without_starting_compose_or_leaking_password() {
    let root = web_fixture("db-console-pgcli");
    fs::write(
        root.join("compose.yaml"),
        "services:\n  postgres:\n    image: postgres:17\n    environment:\n      POSTGRES_DB: app\n      POSTGRES_USER: app\n      POSTGRES_PASSWORD: db-secret\n    ports:\n      - \"5432:5432\"\n",
    )
    .unwrap();
    let fake = temp_dir("db-console-pgcli-bin");
    let log = fake.join("tool.log");
    write_fake_maven(&fake, &["pgcli", "docker"], &log);
    let output = jails_cmd(&root, Some(&fake))
        .args(["--debug", "db", "console"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let invoked = read_log(&log);
    assert!(invoked.contains("pgcli --warn"), "{invoked}");
    assert!(
        !invoked.contains("compose"),
        "database console started Compose: {invoked}"
    );
    let diagnostics = String::from_utf8_lossy(&output.stderr);
    assert!(!diagnostics.contains("db-secret"), "{diagnostics}");
}

#[test]
fn logs_are_bounded_and_accept_only_declared_services() {
    let root = web_fixture("bounded-logs");
    fs::write(
        root.join("compose.yaml"),
        "services:\n  postgres:\n    image: postgres:17\n",
    )
    .unwrap();
    let fake = temp_dir("bounded-logs-bin");
    let log = fake.join("docker.log");
    write_fake_maven(&fake, &["docker"], &log);
    let output = jails_cmd(&root, Some(&fake))
        .args(["logs", "postgres"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(read_log(&log).contains("compose logs --tail 200 postgres"));

    let refused = jails_cmd(&root, Some(&fake))
        .args(["logs", "redis"])
        .output()
        .unwrap();
    assert!(!refused.status.success());
    assert!(String::from_utf8_lossy(&refused.stderr).contains("not declared"));
}

#[test]
fn runner_boots_one_spring_main_with_private_startup_and_project_script() {
    let root = web_fixture("spring-runner");
    fs::write(
        root.join("src/main/java/com/example/demo/DemoApplication.java"),
        "package com.example.demo;\nimport org.springframework.boot.autoconfigure.SpringBootApplication;\n@SpringBootApplication public class DemoApplication {}\n",
    )
    .unwrap();
    fs::create_dir_all(root.join("scripts")).unwrap();
    fs::write(root.join("scripts/check.jsh"), "beans().count();\n").unwrap();
    let fake = temp_dir("spring-runner-bin");
    let log = fake.join("tool.log");
    write_fake_maven(&fake, &["mvn", "jshell"], &log);

    let output = jails_cmd(&root, Some(&fake))
        .args([
            "runner",
            "--file",
            "scripts/check.jsh",
            "--profile",
            "test",
            "--web",
            "random",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let invoked = read_log(&log);
    assert!(invoked.contains("dependency:build-classpath"), "{invoked}");
    assert!(invoked.contains("jshell --class-path"), "{invoked}");
    assert!(invoked.contains("--startup"), "{invoked}");
    assert!(invoked.contains("script.jsh"), "{invoked}");
}
