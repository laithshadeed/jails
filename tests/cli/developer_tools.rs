//! Portable contracts and transparent developer tools through the real CLI.

use super::*;

/// A project with one route.
///
/// The model is what makes it a project rather than a directory: `contract
/// emit` reads the routes off the tree either way, but the assertion that it
/// leaves a lock behind is about a project with `.jails/model.jdl`.
fn web_fixture(label: &str) -> PathBuf {
    let root = temp_dir(label);
    write_project_skeleton(&root);
    assert!(
        jails_cmd(&root, None)
            .args(["g", "record", "Note", "id:uuid@pk", "title:string!"])
            .status()
            .unwrap()
            .success()
    );
    fs::create_dir_all(common::generated(
        &root,
        "src/main/java/com/example/demo/web",
    ))
    .unwrap();
    fs::write(
        common::generated(&root, "src/main/java/com/example/demo/web/NoteController.java"),
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

    fs::remove_file(common::generated(
        &root,
        "src/main/java/com/example/demo/web/NoteController.java",
    ))
    .unwrap();
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
    // The project's durable record is `.jails/compiler.lock.json` and nothing
    // else; a command that quietly created other bookkeeping would put the
    // project on a record nothing reads.
    assert!(root.join(".jails/compiler.lock.json").is_file());
    assert!(!root.join(".jails/receipts").exists());
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
        common::generated(&root, "src/main/java/com/example/demo/DemoApplication.java"),
        "package com.example.demo;\nimport org.springframework.boot.autoconfigure.SpringBootApplication;\n@SpringBootApplication public class DemoApplication {}\n",
    )
    .unwrap();
    fs::create_dir_all(root.join("scripts")).unwrap();
    fs::write(root.join("scripts/check.jsh"), "beans().count();\n").unwrap();
    let fake = temp_dir("spring-runner-bin");
    let log = fake.join("tool.log");
    write_fake_maven(&fake, &["mvn", "java", "jshell"], &log);
    fs::write(
        fake.join("java"),
        format!(
            "#!/bin/sh\necho 'openjdk version \"26\"' >&2\necho \"$0 $*\" >> \"{}\"\nexit 0\n",
            log.display()
        ),
    )
    .unwrap();

    let output = jails_cmd(&root, Some(&fake))
        .env_remove("JAVA_HOME")
        .args([
            "runner",
            "--file",
            "scripts/check.jsh",
            "--profile",
            "test",
            "--web",
            "random",
            "--compile",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let invoked = read_log(&log);
    assert!(invoked.contains("mvn compile"), "{invoked}");
    assert!(invoked.contains("jshell --execution local"), "{invoked}");
    assert!(invoked.contains("--class-path"), "{invoked}");
    assert!(invoked.contains("--startup"), "{invoked}");
    assert!(invoked.contains("script.jsh"), "{invoked}");
}

#[test]
fn runner_treats_a_jshell_snippet_failure_as_a_failed_command() {
    let root = web_fixture("spring-runner-failure");
    fs::write(
        common::generated(&root, "src/main/java/com/example/demo/DemoApplication.java"),
        "package com.example.demo;\nimport org.springframework.boot.autoconfigure.SpringBootApplication;\n@SpringBootApplication public class DemoApplication {}\n",
    )
    .unwrap();
    fs::create_dir_all(root.join("scripts")).unwrap();
    fs::write(root.join("scripts/broken.jsh"), "int broken = ;\n").unwrap();
    let fake = temp_dir("spring-runner-failure-bin");
    let log = fake.join("tool.log");
    write_fake_maven(&fake, &["mvn", "java", "jshell"], &log);
    fs::write(
        fake.join("java"),
        format!(
            "#!/bin/sh\necho 'openjdk version \"26\"' >&2\necho \"$0 $*\" >> \"{}\"\nexit 0\n",
            log.display()
        ),
    )
    .unwrap();
    fs::write(
        fake.join("jshell"),
        format!(
            "#!/bin/sh\necho \"$0 $*\" >> \"{}\"\necho 'Error:' >&2\necho 'rejected snippet' >&2\nexit 0\n",
            log.display()
        ),
    )
    .unwrap();
    let output = jails_cmd(&root, Some(&fake))
        .env_remove("JAVA_HOME")
        .args([
            "runner",
            "--file",
            "scripts/broken.jsh",
            "--profile",
            "test",
            "--compile",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let diagnostics = String::from_utf8_lossy(&output.stderr);
    assert!(
        diagnostics.contains("runner snippet or Spring context cleanup failed"),
        "{diagnostics}"
    );
}

#[test]
fn real_console_and_runner_observe_predestroy_and_reject_session_failures() {
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
    if !std::process::Command::new("jshell")
        .arg("--version")
        .status()
        .is_ok_and(|status| status.success())
    {
        skip("jshell not found on PATH");
        return;
    }

    let root = temp_dir("spring-runner-lifecycle");
    fs::write(
        root.join("pom.xml"),
        format!(
            "<project xmlns=\"http://maven.apache.org/POM/4.0.0\">\n  <modelVersion>4.0.0</modelVersion>\n  <parent><groupId>org.springframework.boot</groupId><artifactId>spring-boot-starter-parent</artifactId><version>4.1.0</version></parent>\n  <groupId>com.example</groupId><artifactId>runner-lifecycle</artifactId><version>0.0.1-SNAPSHOT</version>\n  <properties><java.version>{TARGET_RELEASE}</java.version></properties>\n  <dependencies>\n    <dependency><groupId>org.springframework.boot</groupId><artifactId>spring-boot-starter</artifactId></dependency>\n    <dependency><groupId>org.springframework</groupId><artifactId>spring-tx</artifactId></dependency>\n  </dependencies>\n</project>\n"
        ),
    )
    .unwrap();
    let source = common::generated(&root, "src/main/java/com/example/demo");
    fs::create_dir_all(&source).unwrap();
    fs::write(
        source.join("DemoApplication.java"),
        "package com.example.demo;\nimport org.springframework.boot.autoconfigure.SpringBootApplication;\n@SpringBootApplication public class DemoApplication {}\n",
    )
    .unwrap();
    fs::write(
        source.join("LifecycleProbe.java"),
        "package com.example.demo;\nimport jakarta.annotation.PreDestroy;\nimport java.nio.file.Files;\nimport java.nio.file.Path;\nimport org.springframework.stereotype.Component;\n@Component public class LifecycleProbe { @PreDestroy void close() throws Exception { Files.writeString(Path.of(\"lifecycle.marker\"), \"closed\"); } }\n",
    )
    .unwrap();
    fs::create_dir_all(root.join("scripts")).unwrap();
    fs::write(root.join("scripts/good.jsh"), "beans().count();\n").unwrap();
    fs::write(root.join("scripts/compile-error.jsh"), "int broken = ;\n").unwrap();
    fs::write(
        root.join("scripts/runtime-error.jsh"),
        "throw new IllegalStateException(\"runner probe\");\n",
    )
    .unwrap();
    fs::write(
        root.join("scripts/cleanup-error.jsh"),
        "((org.springframework.beans.factory.support.DefaultListableBeanFactory) ctx.getBeanFactory()).destroySingleton(\"jailsShutdownProbe\");\nJailsShutdownProbe.clean.set(false);\n",
    )
    .unwrap();
    let lifecycle = root.join("lifecycle.marker");
    let path = real_path_without_mvnd();

    let good = jails_cmd_with_path(&root, &path)
        .args([
            "runner",
            "--file",
            "scripts/good.jsh",
            "--profile",
            "test",
            "--compile",
        ])
        .output()
        .unwrap();
    assert!(
        good.status.success(),
        "{}",
        String::from_utf8_lossy(&good.stderr)
    );
    assert_eq!(fs::read_to_string(&lifecycle).unwrap(), "closed");

    fs::remove_file(&lifecycle).unwrap();
    let console = jails_cmd_with_path(&root, &path)
        .args(["console", "--profile", "test"])
        .output()
        .unwrap();
    assert!(
        console.status.success(),
        "{}",
        String::from_utf8_lossy(&console.stderr)
    );
    assert!(
        lifecycle.is_file(),
        "console EOF did not run @PreDestroy\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&console.stdout),
        String::from_utf8_lossy(&console.stderr)
    );
    assert_eq!(fs::read_to_string(&lifecycle).unwrap(), "closed");

    for script in [
        "scripts/compile-error.jsh",
        "scripts/runtime-error.jsh",
        "scripts/cleanup-error.jsh",
    ] {
        let failed = jails_cmd_with_path(&root, &path)
            .args(["runner", "--file", script, "--profile", "test"])
            .output()
            .unwrap();
        assert!(!failed.status.success(), "{script} exited successfully");
        assert!(
            String::from_utf8_lossy(&failed.stderr)
                .contains("runner snippet or Spring context cleanup failed"),
            "{}",
            String::from_utf8_lossy(&failed.stderr)
        );
    }
}

#[test]
fn unsafe_spring_boots_print_preflight_and_require_yes_without_a_terminal() {
    let root = web_fixture("spring-preflight");
    fs::write(
        root.join("pom.xml"),
        "<project><properties><maven.compiler.release>26</maven.compiler.release></properties></project>",
    )
    .unwrap();
    fs::create_dir_all(root.join("src/main/resources")).unwrap();
    fs::write(
        root.join("src/main/resources/application-prod.properties"),
        "spring.datasource.url=jdbc:postgresql://prod.example/app\nspring.datasource.password=never-print-me\n",
    )
    .unwrap();
    fs::write(
        common::generated(&root, "src/main/java/com/example/demo/DemoApplication.java"),
        "package com.example.demo;\nimport org.springframework.boot.autoconfigure.SpringBootApplication;\n@SpringBootApplication public class DemoApplication {}\n",
    )
    .unwrap();
    fs::create_dir_all(root.join("scripts")).unwrap();
    fs::write(root.join("scripts/check.jsh"), "beans().count();\n").unwrap();
    let fake = temp_dir("spring-preflight-bin");
    let log = fake.join("tool.log");
    write_fake_maven(&fake, &["mvn", "java", "jshell"], &log);
    fs::write(
        fake.join("java"),
        format!(
            "#!/bin/sh\necho 'openjdk version \"26\"' >&2\necho \"$0 $*\" >> \"{}\"\nexit 0\n",
            log.display()
        ),
    )
    .unwrap();

    let refused = jails_cmd(&root, Some(&fake))
        .env_remove("JAVA_HOME")
        .args(["console", "--profile", "prod", "--compile"])
        .output()
        .unwrap();
    assert!(!refused.status.success());
    let diagnostics = String::from_utf8_lossy(&refused.stderr);
    for evidence in [
        "main: com.example.demo.DemoApplication",
        "release: 26",
        "profiles: prod",
        "web: none",
        "datasource sources: src/main/resources/application-prod.properties (values redacted)",
        "pass `--yes`",
    ] {
        assert!(
            diagnostics.contains(evidence),
            "missing `{evidence}`:\n{diagnostics}"
        );
    }
    assert!(!diagnostics.contains("never-print-me"), "{diagnostics}");
    assert!(
        !read_log(&log).contains("mvn compile"),
        "refused preflight compiled the application"
    );

    let console = jails_cmd(&root, Some(&fake))
        .env_remove("JAVA_HOME")
        .args(["console", "--profile", "prod", "--compile", "--yes"])
        .output()
        .unwrap();
    assert!(
        console.status.success(),
        "{}",
        String::from_utf8_lossy(&console.stderr)
    );
    assert!(read_log(&log).contains("mvn compile"));

    let runner = jails_cmd(&root, Some(&fake))
        .env_remove("JAVA_HOME")
        .args([
            "runner",
            "--file",
            "scripts/check.jsh",
            "--profile",
            "test",
            "--web",
            "configured",
            "--compile",
            "--yes",
        ])
        .output()
        .unwrap();
    assert!(
        runner.status.success(),
        "{}",
        String::from_utf8_lossy(&runner.stderr)
    );
    let diagnostics = String::from_utf8_lossy(&runner.stderr);
    assert!(diagnostics.contains("profiles: test"), "{diagnostics}");
    assert!(diagnostics.contains("web: configured"), "{diagnostics}");
}

/// Every `jails …` command jails tells a reader to run is one the CLI knows.
///
/// A `fix:` line that names a command the reader runs and that refuses is an
/// oracle disagreeing with itself; the commonest form is a command, a kind, a
/// capability or a flag that does not exist because it was renamed somewhere
/// else and the prose was not.
///
/// The oracle is `jails commands --json`, which is walked out of the same
/// `clap::Command` that parses arguments and the same `ValueEnum`s that
/// validate them -- so this compares the prose against the parser rather than
/// against a second list.
#[test]
fn every_command_a_message_tells_the_reader_to_run_is_one_that_exists() {
    let surface = jails_cmd(&temp_dir("fix-conformance"), None)
        .args(["commands", "--json"])
        .output()
        .unwrap();
    assert!(surface.status.success());
    let surface = String::from_utf8_lossy(&surface.stdout);

    let known = |section: &str| -> std::collections::BTreeSet<String> {
        let mut names = std::collections::BTreeSet::new();
        let Some(start) = surface.find(&format!("\"{section}\": [")) else {
            return names;
        };
        let body = &surface[start..];
        let end = body.find("\n  ]").unwrap_or(body.len());
        for line in body[..end].lines() {
            for key in ["\"name\": \"", "\"aliases\": ["] {
                let Some(at) = line.find(key) else { continue };
                let rest = &line[at + key.len()..];
                if key.ends_with('[') {
                    for alias in rest.split(']').next().unwrap_or("").split(',') {
                        let alias = alias.trim().trim_matches('"');
                        if !alias.is_empty() {
                            names.insert(alias.to_string());
                        }
                    }
                } else if let Some(name) = rest.split('"').next() {
                    names.insert(name.to_string());
                }
            }
        }
        names
    };
    let subcommands = known("subcommands");
    let kinds = known("kinds");
    let capabilities = known("capabilities");
    assert!(subcommands.len() > 40 && kinds.len() > 30 && capabilities.len() > 20);

    let mut unknown: Vec<String> = Vec::new();
    for (path, quoted) in quoted_jails_commands() {
        let tokens: Vec<&str> = quoted.split_whitespace().skip(1).collect();
        let Some(first) = tokens.first() else {
            continue;
        };
        if first.starts_with('-') {
            continue;
        }
        // Longest path first: `remove fast-test` is a command in its own
        // right, and matching only the head would check it as a capability.
        let matched = (1..=tokens.len().min(3))
            .rev()
            .map(|depth| tokens[..depth].join(" "))
            .find(|path| subcommands.contains(path));
        let Some(matched) = matched else {
            unknown.push(format!("{path}: `{quoted}` -- no subcommand `{first}`"));
            continue;
        };
        if matched.contains(' ') {
            continue;
        }
        // The two closed vocabularies a message names most often.
        let vocabulary = match *first {
            "generate" | "g" => Some((&kinds, "kind")),
            "add" | "remove" => Some((&capabilities, "capability")),
            _ => None,
        };
        if let Some((allowed, what)) = vocabulary
            && let Some(second) = tokens.get(1)
            && !second.starts_with('-')
            && !second.starts_with('<')
            && second.chars().all(|c| c.is_ascii_lowercase() || c == '-')
            && !allowed.contains(*second)
        {
            unknown.push(format!("{path}: `{quoted}` -- no {what} `{second}`"));
        }
    }
    assert!(
        unknown.is_empty(),
        "these messages tell the reader to run something the CLI does not have:\n  {}\n\n\
         A `fix:` line that refuses is worse than none: the reader cannot tell which \
         answer to believe.",
        unknown.join("\n  ")
    );
}

/// Every backticked `jails …` in a production message, with the file it is in.
///
/// Read from the *blanked* copy's own raw source: a message is a string
/// literal, so the ordinary production scan -- which blanks literals -- is
/// exactly the wrong lens here. Comments are excluded by requiring the
/// backtick to be inside a literal in the raw text, which is approximated by
/// skipping lines whose first non-space characters are `//`.
fn quoted_jails_commands() -> Vec<(String, String)> {
    fn walk(dir: &std::path::Path, out: &mut Vec<(String, String)>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        let mut paths: Vec<std::path::PathBuf> =
            entries.flatten().map(|entry| entry.path()).collect();
        paths.sort();
        for path in paths {
            if path.is_dir() {
                walk(&path, out);
                continue;
            }
            if path.extension().is_none_or(|extension| extension != "rs") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let name = path.display().to_string();
            // Rejoin `\`-continued string literals first. A message long
            // enough to need one is exactly the kind that names a command,
            // and reading the halves separately finds `jails \` and calls it
            // a subcommand.
            let joined = text
                .split("\\\n")
                .map(|part| part.trim_start_matches([' ', '\t']))
                .collect::<Vec<_>>()
                .join("")
                // And the `concat!` pieces, whose adjacency is the other way
                // a long message is written and the other way a command comes
                // to be read as two.
                .split("\",\n")
                .map(|part| part.trim_start_matches([' ', '\t']))
                .collect::<Vec<_>>()
                .join("\u{1}")
                .replace("\u{1}\"", "");
            for line in joined.lines() {
                if line.trim_start().starts_with("//") {
                    continue;
                }
                for segment in line.split("`jails ").skip(1) {
                    let Some(command) = segment.split('`').next() else {
                        continue;
                    };
                    // A quote inside the backticks means the backtick was
                    // not closing a command: this is a test asserting on the
                    // text of one, not a message telling anybody to run it.
                    if command.contains(['{', '"', '\u{1}']) || command.trim().is_empty() {
                        continue;
                    }
                    out.push((name.clone(), format!("jails {command}")));
                }
            }
        }
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut out = Vec::new();
    walk(&root.join("src"), &mut out);
    walk(&root.join("crates"), &mut out);
    assert!(
        out.len() > 50,
        "the message scanner found only {} commands -- it has lost track of where the \
         code lives",
        out.len()
    );
    out
}

/// A word jails does not have, answered with the thing it has instead:
/// `jails add websocket` points at `jails g socket <Name>` rather than at
/// clap's bare list of capabilities.
///
/// The table is deliberately only for words with a *real* answer. A synonym
/// pointing at nothing would be worse than clap's list, which at least says
/// what does exist -- so the last case here checks that an unknown word still
/// gets it.
#[test]
fn a_capability_jails_does_not_have_names_the_one_it_does() {
    let root = temp_dir("capability-synonyms");
    write_spring_fixture(&root);

    for (word, expected) in [
        ("websocket", "jails g socket Chat"),
        (
            "devtools",
            "jails add dependency org.springframework.boot:spring-boot-devtools",
        ),
        ("flyway", "jails add db"),
    ] {
        let refused = jails_cmd(&root, None).args(["add", word]).output().unwrap();
        assert!(!refused.status.success(), "add {word} was accepted");
        let stderr = String::from_utf8_lossy(&refused.stderr);
        assert!(
            stderr.contains(expected),
            "add {word} did not name `{expected}`:\n{stderr}"
        );
        // jails' own voice, not clap's list.
        assert!(stderr.starts_with("jails:"), "add {word}: {stderr}");
        assert!(
            !stderr.contains("[possible values:"),
            "add {word}: {stderr}"
        );
    }

    // A word clap already accepts must not be in the table: `postgres` is a
    // visible_alias for `db`, and an entry for it would claim a capability
    // that works does not exist.
    //
    // `--no-start`, because `postgres` really is `db`: being a working alias
    // is the point being asserted, so this call *succeeds*, and without the
    // flag it would run `docker compose up` and leak a PostgreSQL and its
    // compose network that nothing takes down.
    for alias in ["postgres", "dbconsole"] {
        let refused = jails_cmd(&root, None)
            .args(["add", alias, "--no-start"])
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&refused.stderr);
        assert!(
            !stderr.contains("there is no"),
            "`{alias}` is accepted elsewhere; the table claims it does not exist:\n{stderr}"
        );
    }

    // A word with no answer keeps clap's, which is the useful reply there.
    let unknown = jails_cmd(&root, None)
        .args(["add", "nonsense"])
        .output()
        .unwrap();
    assert!(!unknown.status.success());
    let stderr = String::from_utf8_lossy(&unknown.stderr);
    assert!(stderr.contains("[possible values:"), "{stderr}");

    // And the interception must not have eaten ordinary clap behaviour.
    let helped = jails_cmd(&root, None).args(["--help"]).output().unwrap();
    assert!(helped.status.success());
    assert!(!String::from_utf8_lossy(&helped.stdout).is_empty());
}

/// Every command path the binary advertises is exercised by some test.
///
/// The catalog is the oracle -- `jails commands --json` walks the same
/// `clap::Command` that parses arguments -- so a command added without a
/// journey fails here rather than shipping untested.
///
/// A journey is an invocation, not a mention. Comments are stripped before
/// the scan: prose naming a command is not coverage, and a gate that counted
/// it would pass on the strength of its own documentation.
///
/// A command that cannot be exercised without infrastructure still needs a
/// journey: its refusal is behaviour too. `EXERCISED_ELSEWHERE` is for paths
/// whose journey cannot live in this workspace at all, and it is empty on
/// purpose.
#[test]
fn every_advertised_command_path_has_a_journey() {
    /// Paths with no journey, each with the reason it cannot have one.
    /// Empty: every advertised command is reachable from a test, including the
    /// ones whose only reachable behaviour is a refusal.
    const EXERCISED_ELSEWHERE: [(&str, &str); 0] = [];

    let surface = jails_cmd(&temp_dir("journey-coverage"), None)
        .args(["commands", "--json"])
        .output()
        .unwrap();
    assert!(surface.status.success());
    let surface = String::from_utf8_lossy(&surface.stdout);

    let mut paths = Vec::new();
    for line in surface.lines() {
        let Some(at) = line.find("\"name\": \"") else {
            continue;
        };
        let rest = &line[at + "\"name\": \"".len()..];
        if let Some(end) = rest.find('"') {
            paths.push(rest[..end].to_string());
        }
    }
    assert!(
        paths.len() > 90,
        "the catalog reported only {} command paths -- it has stopped \
         describing the surface, and this gate would pass over anything",
        paths.len()
    );

    let sources = test_sources_without_comments();
    assert!(
        sources.len() > 200_000,
        "the test scan found only {} bytes -- it has lost the suite",
        sources.len()
    );

    let mut unexercised = Vec::new();
    for path in &paths {
        if EXERCISED_ELSEWHERE.iter().any(|(name, _)| name == path) {
            continue;
        }
        let words: Vec<&str> = path.split(' ').collect();
        let found = match words.as_slice() {
            [one] => sources.contains(&format!("\"{one}\"")),
            [first, second, ..] => {
                sources.contains(&format!("\"{first}\", \"{second}\""))
                    || sources.contains(&format!("\"{path}\""))
            }
            [] => true,
        };
        if !found {
            unexercised.push(path.clone());
        }
    }
    assert!(
        unexercised.is_empty(),
        "these command paths are advertised and no test runs them: \
         {unexercised:?}\n       fix: add a journey -- a refusal counts, and \
         for a command that needs infrastructure the refusal is usually the \
         only behaviour a test can reach"
    );
}

/// Every top-level command the binary accepts has a `jails <name>` entry in
/// `README.md`, so a command cannot be added -- or kept -- without a page
/// saying what it is for. The README is the spec (`CLAUDE.md`), and
/// `jails commands --json` is the oracle, so the two cannot drift apart.
#[test]
fn every_top_level_command_has_a_readme_entry() {
    /// Names clap adds, which are not jails commands.
    const NOT_A_COMMAND: [&str; 1] = ["help"];

    let surface = jails_cmd(&temp_dir("readme-coverage"), None)
        .args(["commands", "--json"])
        .output()
        .unwrap();
    assert!(surface.status.success());
    let surface: serde_json::Value = serde_json::from_slice(&surface.stdout).unwrap();
    let commands = surface["subcommands"].as_array().unwrap();
    assert!(
        commands.len() > 30,
        "the catalog reported only {} top-level commands -- it has stopped \
         describing the surface, and this gate would pass over anything",
        commands.len()
    );

    // The whole file, not the `Commands` section alone: `jails app`,
    // `jails adopt` and `jails modernize` each have a section of their own.
    let section =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/README.md")).unwrap();

    let mut undocumented = Vec::new();
    for command in commands {
        let name = command["name"].as_str().unwrap();
        // The catalog is flat: `app init` sits beside `app`. A page for the
        // parent is the page for its verbs.
        if NOT_A_COMMAND.contains(&name) || name.contains(' ') {
            continue;
        }
        let mut spellings = vec![name.to_string()];
        if let Some(aliases) = command["aliases"].as_array() {
            spellings.extend(
                aliases
                    .iter()
                    .filter_map(|alias| alias.as_str().map(str::to_string)),
            );
        }
        let documented = spellings.iter().any(|spelling| {
            section.contains(&format!("`jails {spelling} "))
                || section.contains(&format!("`jails {spelling}`"))
                || section.contains(&format!("`jails {spelling}|"))
        });
        if !documented {
            undocumented.push(name.to_string());
        }
    }
    assert!(
        undocumented.is_empty(),
        "these commands exist and README.md does not mention them: \
         {undocumented:?}\n       fix: write the entry, or \
         remove the command"
    );
}

/// Every `tests/**/*.rs`, with `//` comments removed.
fn test_sources_without_comments() -> String {
    fn walk(dir: &std::path::Path, out: &mut String) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries {
            let path = entry.expect("failed to read a directory entry").path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                let text = std::fs::read_to_string(&path).unwrap_or_default();
                for line in text.lines() {
                    // Whitespace is collapsed as well as comments dropped:
                    // `rustfmt` breaks a long `args([...])` across lines, so
                    // `"editor", "complete"` and `"editor",\n  "complete"`
                    // are the same invocation and only one of them would match
                    // a literal search.
                    out.push_str(line.split("//").next().unwrap_or("").trim());
                    out.push(' ');
                }
            }
        }
    }
    let mut out = String::new();
    walk(
        &std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests"),
        &mut out,
    );
    out
}

/// The `kafka` family refuses outside a project, by name.
///
/// Eight subcommands that shell into the broker's own CLI tools inside the
/// compose container. None can do its work without a project and a running
/// broker, so the behaviour a test can reach is the refusal, and a refusal
/// is a journey too.
#[test]
fn every_kafka_subcommand_refuses_outside_a_project_rather_than_panicking() {
    let root = temp_dir("kafka-outside-a-project");
    // Each path is written out in full rather than assembled from a prefix
    // and a loop variable. `every_advertised_command_path_has_a_journey` looks
    // for the path as a literal, and cannot see one a loop builds -- so a
    // journey that exists but is invisible to the gate reads exactly like a
    // missing one.
    //
    // `send` takes a required payload; the rest take an optional topic. The
    // argument is supplied so clap's own "missing argument" refusal does not
    // stand in for the one being tested -- it would satisfy the assertion
    // below for the wrong reason.
    for (path, argument) in [
        ("kafka topics", None),
        ("kafka describe", None),
        ("kafka send", Some("{}")),
        ("kafka poison", None),
        ("kafka tail", None),
        ("kafka dlt", None),
        ("kafka lag", None),
        ("kafka reset", None),
    ] {
        let subcommand = path.split(' ').nth(1).expect("a two-word path");
        let mut command = jails_cmd(&root, None);
        command.args(["kafka", subcommand]);
        if let Some(argument) = argument {
            command.arg(argument);
        }
        let output = command.output().unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !output.status.success(),
            "`kafka {subcommand}` succeeded in a directory with no project"
        );
        assert!(
            stderr.contains("no pom.xml"),
            "`kafka {subcommand}` refused without naming the missing build \
             file: {stderr}"
        );
    }
}

/// `architecture baseline` refuses outside a project too.
#[test]
fn architecture_baseline_refuses_outside_a_project() {
    let root = temp_dir("architecture-baseline-outside");
    let output = jails_cmd(&root, None)
        .args(["architecture", "baseline"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("no pom.xml"),
        "the refusal should name the missing build file"
    );
}

/// `jails setup` writes to the machine, so its journey gives it a fake one.
///
/// It is the one command that edits a file outside any project --
/// `~/.testcontainers.properties`, through `apply::put_outside_project`, which
/// is deliberately named so nothing else reaches it by accident. The journey
/// therefore points `HOME` at a scratch directory: a test that ran this
/// against the real one would rewrite the developer's own file, which is
/// exactly the accident the verb's name is about.
#[test]
fn setup_writes_the_reuse_key_into_the_home_it_is_given() {
    let home = temp_dir("setup-fake-home");
    let output = jails_cmd(&home, None)
        .arg("setup")
        .env("HOME", &home)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "setup failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let written = home.join(".testcontainers.properties");
    assert!(
        written.is_file(),
        "setup reported success and wrote no ~/.testcontainers.properties"
    );
    let text = std::fs::read_to_string(&written).unwrap();
    assert!(
        text.contains("testcontainers.reuse.enable=true"),
        "setup wrote a file without the key it exists to set: {text}"
    );
}

/// Which generator kinds a real compiler never sees.
///
/// The map from kind to build fixture is derived rather than declared: which
/// kinds a test covers is a fact about its steps, not a list to keep in step
/// by hand.
///
/// A kind counts as compiled when some test both generates it and gates on a
/// real toolchain. The number matters because the golden suite checks *bytes*,
/// not compilability: jails could emit Java that does not compile for a kind
/// in `NOT_COMPILED` and every existing test would stay green.
///
/// Ratchet, not a threshold: the list may shrink and may not grow.
#[test]
fn no_new_generator_kind_escapes_the_real_toolchain() {
    /// Kinds no real compiler builds. Shrink only.
    ///
    /// Empty: `every_remaining_generator_kind_compiles_in_one_spring_project`
    /// builds the kinds with no fixture of their own in one project and one
    /// `mvn test`, because what needs proving is that each kind's output
    /// compiles, not that it does so alone.
    const NOT_COMPILED: [&str; 0] = [];

    let surface = jails_cmd(&temp_dir("kind-build-coverage"), None)
        .args(["commands", "--json"])
        .output()
        .unwrap();
    assert!(surface.status.success());
    let surface = String::from_utf8_lossy(&surface.stdout);
    let kinds = catalog_section(&surface, "kinds");
    assert!(
        kinds.len() > 30,
        "the catalog reported only {} kinds -- it has stopped describing the \
         surface",
        kinds.len()
    );

    let compiled = kinds_reaching_a_real_toolchain(&kinds);
    let escaping: Vec<&String> = kinds.iter().filter(|k| !compiled.contains(*k)).collect();
    let recorded: std::collections::BTreeSet<&str> = NOT_COMPILED.into_iter().collect();

    let unrecorded: Vec<&&String> = escaping
        .iter()
        .filter(|k| !recorded.contains(k.as_str()))
        .collect();
    assert!(
        unrecorded.is_empty(),
        "these kinds generate Java that no real compiler ever sees, and are \
         not in `NOT_COMPILED`: {unrecorded:?}\n       fix: generate the kind \
         inside a toolbox a real-toolchain test builds, so a change that stops \
         it compiling fails here rather than shipping"
    );
    let fixed: Vec<&&str> = recorded
        .iter()
        .filter(|k| compiled.iter().any(|done| done == *k))
        .collect();
    assert!(
        fixed.is_empty(),
        "these kinds are compiled now and still listed in `NOT_COMPILED`: \
         {fixed:?}\n       fix: take them out -- an improvement nobody records \
         is one the next change silently undoes"
    );
}

/// Names in one `jails commands --json` section.
fn catalog_section(surface: &str, section: &str) -> Vec<String> {
    let mut names = Vec::new();
    let Some(start) = surface.find(&format!("\"{section}\": [")) else {
        return names;
    };
    let body = &surface[start..];
    let end = body.find("\n  ]").unwrap_or(body.len());
    for line in body[..end].lines() {
        let Some(at) = line.find("\"name\": \"") else {
            continue;
        };
        let rest = &line[at + "\"name\": \"".len()..];
        if let Some(stop) = rest.find('"') {
            names.push(rest[..stop].to_string());
        }
    }
    names
}

/// Kinds generated inside a function that gates on a real toolchain.
///
/// Per file and per function, with string literals blanked before the braces
/// are counted: these files are full of Java fixtures, so a `{` inside a
/// string literal is not a block, and counting it makes one function's body
/// span the rest of the file and report every kind as compiled.
///
/// The blanked copy is the same length as the original, so offsets found in
/// one index the other: braces are matched in the blank, content is read from
/// the source.
fn kinds_reaching_a_real_toolchain(kinds: &[String]) -> std::collections::BTreeSet<String> {
    // `real_maven_cmd` is the one that actually runs Maven; without it the
    // toolbox *builders* -- which generate a dozen kinds and then run `mvn
    // test` over the result -- are invisible, and a coverage gate that
    // under-reports sends people to write tests that already exist.
    const REAL: [&str; 8] = [
        "real_mvn_available",
        "real_maven_cmd",
        "real_gradle_cmd",
        "real_java_supports_target_release",
        "verified_spring_toolbox",
        "verified_spring_services_toolbox",
        "verified_plain_toolbox",
        "maven_report_summary",
    ];
    let mut compiled = std::collections::BTreeSet::new();
    for source in test_source_files() {
        let blanked = blank_literals(&source);
        let bytes = blanked.as_bytes();
        for (at, _) in blanked.match_indices("fn ") {
            let Some(open) = blanked[at..].find('{').map(|i| at + i) else {
                continue;
            };
            let mut depth = 0usize;
            let mut close = open;
            for (index, byte) in bytes.iter().enumerate().skip(open) {
                match byte {
                    b'{' => depth += 1,
                    b'}' => {
                        depth -= 1;
                        if depth == 0 {
                            close = index;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            if close <= open {
                continue;
            }
            let body = &source[open..close];
            if !REAL.iter().any(|marker| body.contains(marker)) {
                continue;
            }
            let flat = body.split_whitespace().collect::<Vec<_>>().join(" ");
            for kind in kinds {
                if flat.contains(&format!("\"generate\", \"{kind}\""))
                    || flat.contains(&format!("\"g\", \"{kind}\""))
                {
                    compiled.insert(kind.clone());
                }
            }
        }
    }
    compiled
}

/// Each `tests/**/*.rs` separately, with `//` comments blanked in place.
///
/// Separately, because concatenating them lets one file's unbalanced-looking
/// braces run into the next.
fn test_source_files() -> Vec<String> {
    fn walk(dir: &std::path::Path, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries {
            let path = entry.expect("failed to read a directory entry").path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                let text = std::fs::read_to_string(&path).unwrap_or_default();
                out.push(blank_comments(&text));
            }
        }
    }
    let mut out = Vec::new();
    walk(
        &std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests"),
        &mut out,
    );
    out
}

/// The byte after the string literal starting at `at`, when one starts there.
///
/// Both scanners below need this and neither may guess it: a `//` inside a
/// literal is fixture text, not a comment (`"entity Note { // wording"` is
/// one), and blanking the rest of that line leaves an unterminated literal
/// that swallows every brace after it. A function whose opening brace is
/// inside that swallowed span is not a function any scan can see.
fn literal_end(source: &str, at: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    if source[at..].starts_with('r') && source[at + 1..].starts_with(['#', '"']) {
        let hashes = source[at + 1..].bytes().take_while(|b| *b == b'#').count();
        if source[at + 1 + hashes..].starts_with('"') {
            let close = format!("\"{}", "#".repeat(hashes));
            let mut end = at + 1 + hashes + 1;
            while end < bytes.len() && !bytes[end..].starts_with(close.as_bytes()) {
                end += 1;
            }
            return Some((end + close.len()).min(bytes.len()));
        }
    }
    if bytes[at] == b'"' {
        let mut end = at + 1;
        while end < bytes.len() && bytes[end] != b'"' {
            end += if bytes[end] == b'\\' { 2 } else { 1 };
        }
        return Some((end + 1).min(bytes.len()));
    }
    None
}

/// `//` comments replaced by spaces of the same length; literals left alone.
fn blank_comments(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut i = 0;
    while i < source.len() {
        if let Some(end) = literal_end(source, i) {
            out.push_str(&source[i..end]);
            i = end;
            continue;
        }
        if source[i..].starts_with("//") {
            let end = source[i..].find('\n').map_or(source.len(), |at| i + at);
            blank_span(&source[i..end], &mut out);
            i = end;
            continue;
        }
        let ch = source[i..].chars().next().expect("in bounds");
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// String and char literals replaced by spaces of the same length.
fn blank_literals(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut i = 0;
    while i < source.len() {
        if let Some(end) = literal_end(source, i) {
            blank_span(&source[i..end], &mut out);
            i = end;
            continue;
        }
        let ch = source[i..].chars().next().expect("in bounds");
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// One space per byte, newlines kept, so offsets and line numbers survive.
fn blank_span(span: &str, out: &mut String) {
    for byte in span.bytes() {
        out.push(if byte == b'\n' { '\n' } else { ' ' });
    }
}
