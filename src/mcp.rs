//! `jails mcp`: the same commands, over the protocol an agent speaks.
//!
//! **An agent is a reader.** `AGENTS.md` is written for one, `jails commands
//! --json` is already a machine-readable catalogue and `--output json` is
//! already the wire; what was missing is the envelope agents actually
//! connect to. This is a Model Context Protocol server over stdio: one
//! JSON-RPC message per line in, one per line out.
//!
//! **It runs nothing inside jails.** Every tool is a subcommand of this
//! binary, spelled by the same `clap::Command` tree that parses the terminal,
//! and calling one re-executes the binary with that argv. There is no
//! registry to add to, no hook to implement and nothing an outside process
//! can make jails load -- which is what keeps it on the right side of the
//! scope bar's "no plugin system".
//!
//! **The tools are the twenty words of day one, plus the two that answer
//! questions about the rest.** `jails commands` and `jails explain` are how
//! an agent finds everything hidden from the first screen, and an agent that
//! needs one of those has the CLI. Deriving the list rather than writing it
//! is the same rule the completer and the editor protocol follow: a second
//! list is how a catalogue starts lying.

use clap::CommandFactory;
use jails_support::Result;
use serde_json::{Value, json};
use std::io::{BufRead, Write};

/// The protocol revision this server implements.
///
/// Sent back verbatim in `initialize`, because a client that asked for a
/// different one has to be told which it is talking to rather than left to
/// assume.
const PROTOCOL_VERSION: &str = "2025-06-18";

pub(crate) fn run() -> Result<()> {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    serve(stdin.lock(), &mut stdout)
}

/// The loop, over any reader and writer, so a test can be the client.
fn serve(input: impl BufRead, output: &mut impl Write) -> Result<()> {
    for line in input.lines() {
        let line = line.map_err(|error| format!("could not read a request: {error}"))?;
        if line.trim().is_empty() {
            continue;
        }
        let Some(response) = respond(&line) else {
            continue;
        };
        writeln!(output, "{response}")
            .and_then(|()| output.flush())
            .map_err(|error| format!("could not write a response: {error}"))?;
    }
    Ok(())
}

/// One request in, one response out -- or `None` for a notification, which
/// by JSON-RPC's rule is answered with silence rather than an empty result.
fn respond(line: &str) -> Option<String> {
    let request: Value = match serde_json::from_str(line) {
        Ok(request) => request,
        // No id to answer with, so this is the one error that has to be
        // reported against a null id.
        Err(error) => {
            return Some(error_response(&Value::Null, -32700, &format!("{error}")));
        }
    };
    let method = request.get("method").and_then(Value::as_str).unwrap_or("");
    // No id is a notification, and JSON-RPC answers those with silence.
    let id = request.get("id").cloned()?;
    Some(match method {
        "initialize" => result_response(&id, &initialize()),
        "ping" => result_response(&id, &json!({})),
        "tools/list" => result_response(&id, &json!({ "tools": tools() })),
        "tools/call" => match call(request.get("params")) {
            Ok(result) => result_response(&id, &result),
            Err(message) => error_response(&id, -32602, &message),
        },
        other => error_response(&id, -32601, &format!("unknown method `{other}`")),
    })
}

fn initialize() -> Value {
    json!({
        "protocolVersion": PROTOCOL_VERSION,
        "capabilities": { "tools": { "listChanged": false } },
        "serverInfo": { "name": "jails", "version": env!("CARGO_PKG_VERSION") },
        "instructions": "Every tool is a jails subcommand. `commands` lists everything \
                         the binary accepts, including what the first screen hides; \
                         `explain` says what a generator kind or a capability is for. \
                         Pass `output: \"json\"` to any tool that reports.",
    })
}

/// The twenty words of day one, plus the two that describe the rest.
///
/// Visible-only, deliberately: `jails commands` is the catalogue of
/// everything, and it is one of the tools -- so an agent reaches the hidden
/// commands by asking rather than by scrolling a list of ninety.
fn tools() -> Vec<Value> {
    let root = crate::Cli::command();
    root.get_subcommands()
        .filter(|sub| sub.get_name() != "help")
        // `commands` is hidden from the first screen and is the one hidden
        // command an agent needs, because it is how the other seventy are
        // found. Everything else hidden stays hidden here for the reason it
        // is hidden there: the twenty words are the surface.
        .filter(|sub| !sub.is_hide_set() || sub.get_name() == "commands")
        .map(tool)
        .collect()
}

fn tool(command: &clap::Command) -> Value {
    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();
    for argument in command.get_arguments() {
        if argument.is_hide_set() {
            continue;
        }
        let Some(name) = argument.get_long().map(str::to_string).or_else(|| {
            argument
                .is_positional()
                .then(|| argument.get_id().to_string())
        }) else {
            continue;
        };
        let takes_value = argument
            .get_num_args()
            .is_none_or(|count| count.takes_values());
        let mut schema = json!({
            "type": if takes_value { "string" } else { "boolean" },
        });
        if let Some(help) = argument.get_help() {
            schema["description"] = json!(help.to_string());
        }
        // A closed value set reaches the schema as an enum, which is the
        // whole reason a `ValueEnum` is one: the agent is told what the
        // words are rather than guessing and being refused.
        let values = argument
            .get_possible_values()
            .iter()
            .map(|value| json!(value.get_name()))
            .collect::<Vec<_>>();
        if !values.is_empty() {
            schema["enum"] = Value::Array(values);
        }
        if argument.is_required_set() {
            required.push(json!(name));
        }
        properties.insert(name, schema);
    }
    // Every command takes the same global reporting flag, and an agent that
    // cannot ask for JSON has to parse prose.
    properties.insert(
        "output".to_string(),
        json!({ "type": "string", "enum": ["human", "json"],
                "description": "How this command reports" }),
    );
    json!({
        "name": command.get_name(),
        "description": command.get_about().map(|about| about.to_string()).unwrap_or_default(),
        "inputSchema": {
            "type": "object",
            "properties": Value::Object(properties),
            "required": required,
        },
    })
}

/// Run one tool: this binary, with the argv the arguments spell.
fn call(params: Option<&Value>) -> std::result::Result<Value, String> {
    let params = params.ok_or_else(|| "tools/call needs params".to_string())?;
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| "tools/call needs a tool name".to_string())?;
    let known = tools()
        .iter()
        .filter_map(|tool| tool.get("name").and_then(Value::as_str).map(str::to_string))
        .any(|tool| tool == name);
    if !known {
        return Err(format!(
            "`{name}` is not one of this server's tools\n       fix: call `jails commands` \
             for every subcommand, kind, capability and flag jails accepts; this server \
             exposes the ones the first screen names"
        ));
    }
    let empty = serde_json::Map::new();
    let arguments = params
        .get("arguments")
        .and_then(Value::as_object)
        .unwrap_or(&empty);
    let mut argv = vec![name.to_string()];
    // **Spelled in the tree's order, not the object's.** A JSON object has
    // no order worth relying on -- serde sorts these -- and a positional's
    // meaning is its position: `generate record Note title:string` and
    // `generate title:string record Note` are not the same request, and only
    // the first is one clap accepts. So the flags go on first, and then the
    // positionals in the order the command declares them.
    let root = crate::Cli::command();
    let command = root
        .get_subcommands()
        .find(|sub| sub.get_name() == name)
        .ok_or_else(|| format!("`{name}` is not a subcommand"))?;
    for argument in command.get_arguments() {
        let Some(long) = argument.get_long() else {
            continue;
        };
        if let Some(value) = arguments.get(long) {
            push_option(&mut argv, long, value);
        }
    }
    let mut positionals = command.get_positionals().collect::<Vec<_>>();
    positionals.sort_by_key(|argument| argument.get_index().unwrap_or(usize::MAX));
    for argument in positionals {
        if let Some(value) = arguments.get(argument.get_id().as_str()) {
            push_value(&mut argv, value);
        }
    }
    if let Some(output) = arguments.get("output").and_then(Value::as_str) {
        argv.insert(0, output.to_string());
        argv.insert(0, "--output".to_string());
    }
    let executable = std::env::current_exe()
        .map_err(|error| format!("could not find this binary to run a tool: {error}"))?;
    let output = std::process::Command::new(executable)
        .args(&argv)
        .output()
        .map_err(|error| format!("could not run `jails {}`: {error}", argv.join(" ")))?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(json!({
        "content": [{ "type": "text", "text": text }],
        // **A refused command is a tool error, not a protocol one.** The
        // agent asked a well-formed question and jails answered no, with a
        // reason and a fix in the text above; a JSON-RPC error would hide
        // that behind a transport failure.
        "isError": !output.status.success(),
    }))
}

fn push_option(argv: &mut Vec<String>, long: &str, value: &Value) {
    match value {
        Value::Bool(true) => argv.push(format!("--{long}")),
        Value::Bool(false) | Value::Null => {}
        Value::Array(items) => {
            for item in items {
                push_option(argv, long, item);
            }
        }
        other => {
            argv.push(format!("--{long}"));
            argv.push(text_of(other));
        }
    }
}

fn push_value(argv: &mut Vec<String>, value: &Value) {
    match value {
        Value::Null => {}
        Value::Array(items) => {
            for item in items {
                push_value(argv, item);
            }
        }
        other => argv.push(text_of(other)),
    }
}

/// A JSON scalar as the argument a shell would have carried: a string
/// without its quotes, anything else as it is written.
fn text_of(value: &Value) -> String {
    value
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| value.to_string())
}

fn result_response(id: &Value, result: &Value) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "result": result }).to_string()
}

fn error_response(id: &Value, code: i32, message: &str) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } }).to_string()
}
