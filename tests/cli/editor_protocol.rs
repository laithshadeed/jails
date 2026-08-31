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
