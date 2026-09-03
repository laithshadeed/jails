//! `jails mcp`: the same commands, over the protocol an agent speaks.
//!
//! Driven the way an agent drives it -- JSON-RPC lines into the real
//! binary's stdin, JSON-RPC lines back off its stdout -- because a server
//! tested through its own functions proves the handlers and not the
//! envelope, and the envelope is the whole of what this surface adds.

use super::*;

/// Speak to a real `jails mcp` process and collect what it said back.
fn speak(root: &Path, requests: &[&str]) -> Vec<serde_json::Value> {
    let mut child = jails_cmd(root, None)
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    for request in requests {
        writeln!(stdin, "{request}").unwrap();
    }
    drop(stdin);
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "server exited: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).unwrap_or_else(|error| panic!("{line}: {error}")))
        .collect()
}

/// The handshake, the catalogue, one call that writes, and one refusal --
/// the four things a client does, in the order it does them.
#[test]
fn an_agent_handshakes_lists_tools_generates_and_is_refused_by_name() {
    let root = temp_dir("mcp-journey");
    write_project_skeleton(&root);

    let replies = speak(
        &root,
        &[
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{}}}"#,
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
            r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"generate","arguments":{"kind":"record","name":"Note","fields":["title:string","body:string?"]}}}"#,
            r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"deploy","arguments":{}}}"#,
        ],
    );

    // **The notification is answered with silence**, so four requests carrying
    // an id produce four responses and the notification produces none.
    assert_eq!(replies.len(), 4, "{replies:#?}");

    let handshake = &replies[0];
    assert_eq!(handshake["id"], 1);
    assert_eq!(handshake["result"]["protocolVersion"], "2025-06-18");
    assert!(
        handshake["result"]["capabilities"]["tools"].is_object(),
        "{handshake:#?}"
    );

    let listed = replies[1]["result"]["tools"].as_array().unwrap();
    let names: Vec<&str> = listed
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    // The twenty words of the first screen, plus the one that names the rest.
    for expected in ["new", "generate", "add", "explain", "commands", "doctor"] {
        assert!(names.contains(&expected), "{names:?}");
    }
    // A schema, not a prose description of one: an agent fills the arguments
    // from `inputSchema` or it guesses.
    let generate = listed
        .iter()
        .find(|tool| tool["name"] == "generate")
        .unwrap();
    assert_eq!(generate["inputSchema"]["type"], "object");
    assert!(
        generate["inputSchema"]["properties"]["kind"]["enum"]
            .as_array()
            .unwrap()
            .iter()
            .any(|kind| kind == "record"),
        "{generate:#?}"
    );

    // The call ran the real command in the real project.
    let called = &replies[2];
    assert_eq!(called["result"]["isError"], false, "{called:#?}");
    assert!(
        common::generated(&root, "src/main/java/com/example/demo/domain/Note.java").exists(),
        "the tool call wrote nothing: {called:#?}"
    );

    // **A name the server does not have is a protocol error, and a command
    // that refused is not.** The first is the client asking for something
    // that is not there; the second is jails answering no, with a reason and
    // a fix, which belongs in the tool result where the agent reads it.
    let refused = &replies[3];
    assert!(refused.get("result").is_none(), "{refused:#?}");
    let message = refused["error"]["message"].as_str().unwrap_or_default();
    assert!(
        message.contains("deploy") && message.contains("jails commands"),
        "{message}"
    );
}
