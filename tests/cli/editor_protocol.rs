//! Versioned, read-only editor protocol through the real CLI binary.

use super::*;

fn editor_fixture(label: &str) -> PathBuf {
    let root = temp_dir(label);
    write_project_skeleton(&root);
    fs::create_dir_all(common::generated(
        &root,
        "src/main/java/com/example/demo/web",
    ))
    .unwrap();
    fs::create_dir_all(common::generated(&root, "src/test/java/com/example/demo")).unwrap();
    fs::write(
        common::generated(&root, "src/main/java/com/example/demo/web/NoteController.java"),
        "package com.example.demo.web;\n@RestController\n@RequestMapping(\"/notes\")\nfinal class NoteController { @GetMapping public String list() { return \"ok\"; } }\n",
    )
    .unwrap();
    fs::write(
        common::generated(&root, "src/test/java/com/example/demo/NoteTest.java"),
        "package com.example.demo;\nfinal class NoteTest {}\n",
    )
    .unwrap();
    root
}

#[test]
fn handshake_and_symbols_are_versioned_relative_and_read_only() {
    let root = editor_fixture("editor-handshake");
    let before = snapshot_tree(&root);
    let handshake = jails_cmd(&root, None)
        .args(["--output", "json", "editor", "handshake"])
        .output()
        .unwrap();
    assert!(
        handshake.status.success(),
        "{}",
        String::from_utf8_lossy(&handshake.stderr)
    );
    let json = String::from_utf8_lossy(&handshake.stdout);
    assert!(json.contains("jails.editor-handshake.v1"), "{json}");
    assert!(json.contains("jails.command-result.v2"), "{json}");
    assert!(json.contains("\"java_release\":26"), "{json}");
    assert!(
        !json.contains(&root.to_string_lossy().to_string()),
        "absolute root leaked: {json}"
    );

    let symbols = jails_cmd(&root, None)
        .args(["--output", "json", "editor", "symbols", "routes"])
        .output()
        .unwrap();
    assert!(
        symbols.status.success(),
        "{}",
        String::from_utf8_lossy(&symbols.stderr)
    );
    let json = String::from_utf8_lossy(&symbols.stdout);
    assert!(json.contains("jails.editor-symbols.v1"), "{json}");
    assert!(json.contains("route:GET:/notes"), "{json}");
    assert_eq!(snapshot_tree(&root), before);
}

#[test]
fn completion_comes_from_clap_and_diagnostics_preserve_epoch() {
    let root = editor_fixture("editor-completion");
    let completion = jails_cmd(&root, None)
        .args([
            "--output",
            "json",
            "editor",
            "complete",
            "--arg-index",
            "0",
            "--byte-offset",
            "2",
            "--",
            "ed",
        ])
        .output()
        .unwrap();
    assert!(
        completion.status.success(),
        "{}",
        String::from_utf8_lossy(&completion.stderr)
    );
    let json = String::from_utf8_lossy(&completion.stdout);
    assert!(json.contains("jails.editor-completion.v1"), "{json}");
    assert!(json.contains("\"value\":\"editor\""), "{json}");

    let diagnostics = jails_cmd(&root, None)
        .args([
            "--output",
            "json",
            "editor",
            "diagnostics",
            "--scope",
            "buffer",
            "--file",
            "src/main/java/com/example/demo/web/NoteController.java",
        ])
        .output()
        .unwrap();
    assert!(
        diagnostics.status.success(),
        "{}",
        String::from_utf8_lossy(&diagnostics.stderr)
    );
    let json = String::from_utf8_lossy(&diagnostics.stdout);
    assert!(json.contains("jails.editor-diagnostics.v1"), "{json}");
    assert!(json.contains("\"epoch\":"), "{json}");
    assert!(
        json.contains("\"fixes\":[]") || json.contains("\"diagnostics\":[]"),
        "{json}"
    );
}

/// The model's own words, offered where the reader is typing them.
///
/// **A closed set the tool already holds is one the reader should not have
/// to remember.** Which entities exist, which components one has, which
/// types the language spells and which markers a field takes are all answers
/// the binary computes for every other command; before this they stopped at
/// the clap tree, so a completer could offer `--on` and then nothing after
/// it. Every position here is a different source, which is why they are one
/// test: the shape a reader relies on is that the model answers wherever it
/// has an answer.
#[test]
fn completion_offers_what_the_model_declares_and_writes_nothing() {
    let root = editor_fixture("editor-completion-model");
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(
        root.join(".jails/model.jdl"),
        "jdl 1\n\napp Demo @id(project_demo) {\n  pkg com.example.demo\n  java 26\n  \
         platform spring\n  build maven\n  storage none\n}\n\n\
         entity Loan @id(ent_loan) {\n  use repo\n\n  id: uuid @id(fld_loan_id) @pk\n  \
         status: string @id(fld_loan_status)\n  amount: int @id(fld_loan_amount)\n}\n",
    )
    .unwrap();
    let before = snapshot_tree(&root);

    for (argv, index, expected, kind) in [
        // The item's own measurement: a component of the entity, from three
        // letters of it, with no `--on` typed yet.
        (
            vec!["g", "query", "X", "st"],
            3,
            "\"value\":\"status:\"",
            "field",
        ),
        // The entity behind `--on`, which is the flag every other one hangs
        // off.
        (
            vec!["g", "query", "X", "--on", "Lo"],
            4,
            "\"value\":\"Loan\"",
            "entity",
        ),
        // The language's own type table, after the colon.
        (
            vec!["g", "record", "X", "total:de"],
            3,
            "\"value\":\"total:decimal\"",
            "type",
        ),
        // A marker at the end of a whole field, which is where one is typed.
        (
            vec!["g", "scaffold", "X", "status:string@uni"],
            3,
            "\"value\":\"status:string@unique\"",
            "marker",
        ),
    ] {
        let token = argv[index];
        let output = jails_cmd(&root, None)
            .args(["--output", "json", "editor", "complete", "--arg-index"])
            .arg(index.to_string())
            .arg("--byte-offset")
            .arg(token.len().to_string())
            .arg("--")
            .args(&argv)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "`{}` refused: {}",
            argv.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
        let json = String::from_utf8_lossy(&output.stdout);
        assert!(json.contains(expected), "completing `{token}`: {json}");
        assert!(json.contains(&format!("\"kind\":\"{kind}\"")), "{json}");
    }

    // The positions the model has nothing to say about still answer, and
    // answer from clap: the kind is a closed set the parser owns.
    let output = jails_cmd(&root, None)
        .args([
            "--output",
            "json",
            "editor",
            "complete",
            "--arg-index",
            "0",
            "--byte-offset",
            "2",
            "--",
            "mo",
        ])
        .output()
        .unwrap();
    let json = String::from_utf8_lossy(&output.stdout);
    assert!(json.contains("\"value\":\"model\""), "{json}");
    assert!(!json.contains("\"kind\":\"field\""), "{json}");

    assert_eq!(
        snapshot_tree(&root),
        before,
        "completion wrote to the project"
    );
}

/// **A completer that a shell cannot run is a protocol, not a completer.**
///
/// The generated bash script is `clap_complete`'s, which knows the closed
/// sets that are the same in every project and nothing about this one. The
/// appended hook is what asks the binary, and the only way to know it works
/// is to let bash run it: the script is sourced, driven at the cursor a
/// reader would be at, and read back out of `COMPREPLY`.
#[test]
fn the_generated_bash_completion_asks_the_binary_what_this_project_declares() {
    let root = editor_fixture("editor-completion-bash");
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(
        root.join(".jails/model.jdl"),
        "jdl 1\n\napp Demo @id(project_demo) {\n  pkg com.example.demo\n  java 26\n  \
         platform spring\n  build maven\n  storage none\n}\n\n\
         entity Loan @id(ent_loan) {\n  use repo\n\n  id: uuid @id(fld_loan_id) @pk\n  \
         status: string @id(fld_loan_status)\n}\n",
    )
    .unwrap();
    let script = jails_cmd(&root, None)
        .args(["completion", "bash"])
        .output()
        .unwrap();
    assert!(script.status.success());
    let script = String::from_utf8_lossy(&script.stdout).to_string();
    let script_path = root.join("jails-completion.bash");
    fs::write(&script_path, &script).unwrap();

    let syntax = std::process::Command::new("bash")
        .arg("-n")
        .arg(&script_path)
        .output()
        .unwrap();
    assert!(
        syntax.status.success(),
        "the generated script is not valid bash: {}",
        String::from_utf8_lossy(&syntax.stderr)
    );

    let driver = root.join("drive.bash");
    fs::write(
        &driver,
        format!(
            "source {}\n\
             COMP_WORDS=(jails g query Recent st)\n\
             COMP_CWORD=4\n\
             COMP_LINE=\"jails g query Recent st\"\n\
             COMP_POINT=${{#COMP_LINE}}\n\
             _jails_with_the_model jails st Recent\n\
             printf '%s\\n' \"${{COMPREPLY[@]}}\"\n",
            script_path.display()
        ),
    )
    .unwrap();
    let mut shell = std::process::Command::new("bash");
    shell.arg(&driver).current_dir(&root);
    // The hook calls `${words[0]}`, which is what a reader has on PATH.
    let path = format!(
        "{}:{}",
        std::path::Path::new(common::bin())
            .parent()
            .expect("the test binary has a directory")
            .display(),
        std::env::var("PATH").unwrap_or_default()
    );
    shell.env("PATH", path);
    let completed = shell.output().unwrap();
    assert!(
        completed.status.success(),
        "the completer failed: {}",
        String::from_utf8_lossy(&completed.stderr)
    );
    let reply = String::from_utf8_lossy(&completed.stdout);
    assert!(
        reply.lines().any(|line| line == "status:"),
        "bash completed `st` to {reply:?}"
    );
}

/// **The protocol carries the language's diagnostics, not a summary of
/// them.**
///
/// An adapter that has to run `model check` and scrape its prose breaks the
/// first time a message is reworded. `editor diagnostics` runs the same parse
/// and link that command runs and reports each one in the schema's shape:
/// the code an adapter can branch on, the model path as the subject, and the
/// line and column an editor jumps to. Both halves of the language are here,
/// because they come from different places: a syntax error is the parser's
/// and a `model-*` code is the linker's.
#[test]
fn editor_diagnostics_return_the_model_check_codes_with_a_line() {
    let root = editor_fixture("editor-diagnostics-model");
    fs::create_dir_all(root.join(".jails")).unwrap();
    let model = "jdl 1\n\napp Demo @id(project_demo) {\n  pkg com.example.demo\n  java 26\n  \
                 platform spring\n  build maven\n  storage none\n}\n\n\
                 entity Loan @id(ent_loan) {\n  use repo\n\n  id: uuid @id(fld_loan_id) @pk\n  \
                 status: strin @id(fld_loan_status)\n}\n";
    fs::write(root.join(".jails/model.jdl"), model).unwrap();

    let checked = jails_cmd(&root, None)
        .args(["--output", "json", "model", "check"])
        .output()
        .unwrap();
    let checked: serde_json::Value = serde_json::from_slice(&checked.stdout).unwrap();
    let expected = checked["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .map(|diagnostic| diagnostic["code"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    assert_eq!(expected, ["model-field-type"]);

    let reported = jails_cmd(&root, None)
        .args([
            "--output",
            "json",
            "editor",
            "diagnostics",
            "--scope",
            "project",
        ])
        .output()
        .unwrap();
    assert!(reported.status.success());
    let reported: serde_json::Value = serde_json::from_slice(&reported.stdout).unwrap();
    let rows = reported["diagnostics"].as_array().unwrap();
    assert_eq!(
        rows.iter()
            .map(|row| row["code"].as_str().unwrap().to_string())
            .collect::<Vec<_>>(),
        expected
    );
    assert_eq!(rows[0]["subject"], "$.entities.loan.fields.status.type");
    assert_eq!(rows[0]["primary"]["path"], ".jails/model.jdl");
    // Zero-based, as everything else in this protocol is: the fifteenth line
    // of the file.
    assert_eq!(rows[0]["primary"]["range"]["start"]["line"], 14);
    assert_eq!(rows[0]["primary"]["range"]["start"]["byte_column"], 2);
    assert!(!rows[0]["fixes"].as_array().unwrap().is_empty());

    // A syntax error is the parser's, and it reaches the same surface.
    fs::write(
        root.join(".jails/model.jdl"),
        model.replace("jdl 1", "jdl 9"),
    )
    .unwrap();
    let reported = jails_cmd(&root, None)
        .args([
            "--output",
            "json",
            "editor",
            "diagnostics",
            "--scope",
            "project",
        ])
        .output()
        .unwrap();
    let reported: serde_json::Value = serde_json::from_slice(&reported.stdout).unwrap();
    let rows = reported["diagnostics"].as_array().unwrap();
    assert!(!rows.is_empty(), "a syntax error reported nothing");
    assert!(
        rows[0]["primary"]["range"]["start"]["line"]
            .as_u64()
            .is_some(),
        "{:#?}",
        rows[0]
    );

    // A buffer that is not the model has none of them, and does not pay for
    // a link to find that out.
    let elsewhere = jails_cmd(&root, None)
        .args([
            "--output",
            "json",
            "editor",
            "diagnostics",
            "--scope",
            "buffer",
            "--file",
            "src/main/java/com/example/demo/web/NoteController.java",
        ])
        .output()
        .unwrap();
    let elsewhere: serde_json::Value = serde_json::from_slice(&elsewhere.stdout).unwrap();
    assert!(
        elsewhere["diagnostics"].as_array().unwrap().is_empty(),
        "{elsewhere}"
    );
}

// ---- `jails lsp`: the same answers, in the envelope every editor speaks ----

/// One `Content-Length`-framed message, as a client writes it.
fn framed(message: serde_json::Value) -> Vec<u8> {
    let body = message.to_string();
    format!("Content-Length: {}\r\n\r\n{body}", body.len()).into_bytes()
}

/// Speak to a real `jails lsp` process and split what it said back into
/// messages, so a test reads the same stream a client does -- framing
/// included, because the framing is the whole difference between this server
/// and the one an agent connects to.
fn converse(root: &Path, requests: Vec<serde_json::Value>) -> Vec<serde_json::Value> {
    let mut child = jails_cmd(root, None)
        .arg("lsp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    for request in requests {
        stdin.write_all(&framed(request)).unwrap();
    }
    drop(stdin);
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "server exited: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut rest = output.stdout.as_slice();
    let mut messages = Vec::new();
    while !rest.is_empty() {
        let split = rest
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .expect("a message with no header");
        let header = String::from_utf8_lossy(&rest[..split]).to_string();
        let length: usize = header
            .split(':')
            .nth(1)
            .expect("no Content-Length")
            .trim()
            .parse()
            .unwrap();
        let body = &rest[split + 4..split + 4 + length];
        messages.push(serde_json::from_slice(body).unwrap());
        rest = &rest[split + 4 + length..];
    }
    messages
}

fn uri_of(root: &Path) -> String {
    format!("file://{}", root.join(".jails/model.jdl").display())
}

/// The four things a client does: handshake, open, type, and ask.
#[test]
fn an_editor_opens_the_model_types_an_at_sign_and_is_offered_the_attribute_list() {
    let root = temp_dir("lsp-journey");
    write_project_skeleton(&root);
    // A modelled project, written by the tool rather than by hand, so the
    // buffer under test is one a reader would actually be looking at.
    let generated = jails_cmd(&root, None)
        .args(["g", "record", "Note", "title:string"])
        .output()
        .unwrap();
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );
    let model = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    let uri = uri_of(&root);

    // Put the cursor at the end of a field line and type `@`.
    let lines: Vec<&str> = model.lines().collect();
    let field = lines
        .iter()
        .position(|line| line.trim_start().starts_with("title:"))
        .expect("no field line in the model the tool just wrote");
    let mut edited: Vec<String> = lines.iter().map(|line| (*line).to_string()).collect();
    edited[field].push_str(" @");
    let column = edited[field].chars().count();
    let typed = edited.join("\n");

    let replies = converse(
        &root,
        vec![
            serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize",
                "params":{"rootUri": format!("file://{}", root.display()), "capabilities":{}}}),
            serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
            serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen",
                "params":{"textDocument":{"uri": uri, "languageId":"jdl","version":1,"text": model}}}),
            serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didChange",
                "params":{"textDocument":{"uri": uri,"version":2},
                          "contentChanges":[{"text": typed}]}}),
            serde_json::json!({"jsonrpc":"2.0","id":2,"method":"textDocument/completion",
                "params":{"textDocument":{"uri": uri},
                          "position":{"line": field, "character": column}}}),
            serde_json::json!({"jsonrpc":"2.0","id":3,"method":"textDocument/hover",
                "params":{"textDocument":{"uri": uri},"position":{"line": 2,"character": 1}}}),
            serde_json::json!({"jsonrpc":"2.0","id":4,"method":"shutdown"}),
            serde_json::json!({"jsonrpc":"2.0","method":"exit"}),
        ],
    );

    let answer = |id: i64| {
        replies
            .iter()
            .find(|message| message["id"] == id)
            .unwrap_or_else(|| panic!("no answer to {id}: {replies:#?}"))
    };

    // The capabilities are the contract, and `@` is what makes a client ask.
    let capabilities = &answer(1)["result"]["capabilities"];
    assert_eq!(capabilities["textDocumentSync"], 1);
    assert!(
        capabilities["completionProvider"]["triggerCharacters"]
            .as_array()
            .unwrap()
            .iter()
            .any(|character| character == "@"),
        "{capabilities:#?}"
    );

    // **This is the item's own bar**: `@` on a field offers the field's
    // attributes, and nothing that belongs to another declaration.
    let items = answer(2)["result"]["items"].as_array().unwrap();
    let labels: Vec<&str> = items
        .iter()
        .map(|item| item["label"].as_str().unwrap())
        .collect();
    for expected in ["pk", "unique", "notBlank", "index"] {
        assert!(labels.contains(&expected), "{labels:?}");
    }
    assert!(!labels.contains(&"retired"), "an entity's own: {labels:?}");

    // Hover is the same table `jails explain jdl` prints.
    let hover = answer(3)["result"]["contents"]["value"]
        .as_str()
        .unwrap_or_default();
    assert!(
        hover.contains("app") || hover.contains("project"),
        "{hover}"
    );

    // **A notification is answered with silence, except where it publishes.**
    // Three notifications went in; the two document ones each publish, and
    // `initialized` says nothing.
    let published: Vec<&serde_json::Value> = replies
        .iter()
        .filter(|message| message["method"] == "textDocument/publishDiagnostics")
        .collect();
    assert_eq!(published.len(), 2, "{replies:#?}");
    assert!(
        published[0]["params"]["diagnostics"]
            .as_array()
            .unwrap()
            .is_empty(),
        "the model the tool wrote does not parse: {:#?}",
        published[0]
    );
    // A bare `@` is a syntax error, and every jails diagnostic carries its
    // fix, which is the half an editor would otherwise drop.
    let after = published[1]["params"]["diagnostics"].as_array().unwrap();
    assert_eq!(after.len(), 1, "{after:#?}");
    assert!(
        after[0]["message"].as_str().unwrap().contains("fix:"),
        "{after:#?}"
    );
    assert_eq!(after[0]["source"], "jails");
}

/// Go-to-definition on a declaration is every file it generated, read off
/// the ids the lock already records rather than compiled again.
#[test]
fn go_to_definition_on_a_declaration_lands_on_the_files_it_generated() {
    let root = temp_dir("lsp-definition");
    write_project_skeleton(&root);
    let generated = jails_cmd(&root, None)
        .args(["g", "record", "Note", "title:string"])
        .output()
        .unwrap();
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );
    let model = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    let uri = uri_of(&root);
    let line = model
        .lines()
        .position(|line| line.starts_with("component record Note"))
        .unwrap_or_else(|| {
            model
                .lines()
                .position(|line| line.contains("Note") && !line.trim_start().starts_with("title:"))
                .expect("no declaration of Note")
        });
    let column = model.lines().nth(line).unwrap().find("Note").unwrap() + 1;

    let replies = converse(
        &root,
        vec![
            serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize",
                "params":{"rootUri": format!("file://{}", root.display())}}),
            serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen",
                "params":{"textDocument":{"uri": uri, "text": model}}}),
            serde_json::json!({"jsonrpc":"2.0","id":2,"method":"textDocument/definition",
                "params":{"textDocument":{"uri": uri},
                          "position":{"line": line, "character": column}}}),
            serde_json::json!({"jsonrpc":"2.0","method":"exit"}),
        ],
    );
    let locations = replies.iter().find(|message| message["id"] == 2).unwrap()["result"]
        .as_array()
        .unwrap();
    let uris: Vec<&str> = locations
        .iter()
        .map(|location| location["uri"].as_str().unwrap())
        .collect();
    assert!(
        uris.iter().any(|uri| uri.ends_with("Note.java")),
        "{uris:?}"
    );
}
