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
        root.join("src/main/java/com/example/demo/DemoApplication.java"),
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
    let source = root.join("src/main/java/com/example/demo");
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
        root.join("src/main/java/com/example/demo/DemoApplication.java"),
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
/// research.md §0.2. The theme these messages belong to is *oracles that
/// disagree*: a `fix:` line names a command, the reader runs it, and it
/// refuses. `bugs.md` B41 was a whole chain of it -- `doctor` named `resource
/// repair`, `repair` named `revive`, and `revive` answered with an internal
/// planning term over an entity that was fully present on disk. The cheapest
/// control is the one that catches the commonest form: a command, a kind, a
/// capability or a flag that simply does not exist, because it was renamed
/// somewhere else and the prose was not.
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
        // The two closed vocabularies a message names most often, and the two
        // that have been renamed under prose that stayed put.
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

/// A word jails does not have, answered with the thing it has instead.
///
/// `bugs.md` B55: `jails add websocket` answered with clap's bare list of 25
/// capabilities and pointed at nothing, while `jails g socket <Name>` is the
/// whole slice. A reader who knows the word and not the spelling was one
/// command away and got a wall of alternatives.
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
    for alias in ["postgres", "dbconsole"] {
        let refused = jails_cmd(&root, None)
            .args(["add", alias])
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
