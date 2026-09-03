//! `jails lsp`: the model file, in the protocol every editor already speaks.
//!
//! **The same answers as `jails editor`, in an envelope no adapter has to
//! write.** `editor complete`, `editor diagnostics` and `editor symbols` are
//! a versioned read-only protocol, and `jails.nvim` is nine hundred lines of
//! Lua spent turning it into what an editor wanted in the first place. Every
//! editor already has a Language Server Protocol client; what it did not
//! have was a server. This is one, over stdio.
//!
//! **Framing is the whole difference from [`crate::mcp`].** MCP is one
//! JSON-RPC message per line; LSP is `Content-Length: <n>` and a blank line
//! before each. The bodies are the same JSON-RPC, so the two servers share
//! their shape and nothing else -- and a client fed the wrong framing does
//! not error, it hangs, which is why the header is read rather than assumed.
//!
//! **The buffer is the document, not the file on disk.** An editor asks
//! about the text it is holding, unsaved edits included; a server that read
//! `.jails/model.jdl` would complete and diagnose the last save while the
//! reader looks at something else. `didOpen`/`didChange`/`didClose` keep the
//! text and everything else answers out of it.
//!
//! **Everything is best-effort and silent.** Completion runs on a keystroke
//! into a half-typed declaration: the grammar half is static and answers
//! whatever the buffer says, and the half that needs a linked model returns
//! nothing rather than a diagnostic when the model does not parse yet.
//! Diagnostics are the one exception, because saying what is wrong is what
//! they are for.
//!
//! Written against revision 3.17 of the specification, which unlike MCP has
//! no version field for a server to echo: what a server implements is the
//! capability object it answers `initialize` with, and this one is honest
//! about the four things it does.

mod document;
mod language;

use jails_support::Result;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::io::{BufRead, Write};

pub(crate) fn run() -> Result<()> {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    serve(stdin.lock(), &mut stdout)
}

/// Open documents, by URI, and where the project is.
#[derive(Default)]
struct Server {
    documents: BTreeMap<String, String>,
    /// The project the client opened, resolved once at `initialize` and
    /// never re-derived: an editor moves its cursor, not its workspace.
    ///
    /// **A `Project`, not a root**, because the two things this server does
    /// with it -- capturing the accepted tree, spelling a generated file's
    /// URI -- are both about a resolved project, and a bare path passed down
    /// would make every function below re-answer "which project is this".
    project: Option<crate::project::Project>,
    /// Set by `shutdown`, so `exit` after one is a clean stop and `exit`
    /// without one is not, which is the specification's own distinction.
    shutting_down: bool,
}

/// The loop, over any reader and writer, so a test can be the client.
fn serve(mut input: impl BufRead, output: &mut impl Write) -> Result<()> {
    let mut server = Server::default();
    while let Some(body) = read_message(&mut input)? {
        let Ok(request) = serde_json::from_str::<Value>(&body) else {
            // A malformed body has no id to answer against and the
            // specification has no null-id error for a server to send, so
            // the honest response is to keep reading.
            continue;
        };
        let method = request.get("method").and_then(Value::as_str).unwrap_or("");
        if method == "exit" {
            return Ok(());
        }
        let params = request.get("params");
        // A notification carries no id and is answered with silence, but it
        // may still make the server say something of its own: `didOpen` is
        // how diagnostics get published.
        for note in server.notify(method, params) {
            write_message(output, &note)?;
        }
        let Some(id) = request.get("id").cloned() else {
            continue;
        };
        let response = match server.request(method, params) {
            Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
            Err((code, message)) => {
                json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
            }
        };
        write_message(output, &response)?;
    }
    Ok(())
}

impl Server {
    /// A request, which is answered.
    fn request(
        &mut self,
        method: &str,
        params: Option<&Value>,
    ) -> std::result::Result<Value, (i32, String)> {
        match method {
            "initialize" => {
                // A client that opened a directory with no build file gets a
                // server that completes the language and diagnoses the model
                // and cannot jump to a generated file, which is the honest
                // subset rather than a refused handshake.
                self.project = params
                    .and_then(root_of)
                    .and_then(|root| crate::project::Project::load(&root).ok());
                Ok(self.initialize())
            }
            "shutdown" => {
                self.shutting_down = true;
                Ok(Value::Null)
            }
            "textDocument/completion" => Ok(self.completion(params)),
            "textDocument/hover" => Ok(self.hover(params)),
            "textDocument/definition" => Ok(self.definition(params)),
            other => Err((
                -32601,
                format!(
                    "`{other}` is not a method this server implements\n       fix: \
                     `initialize` announced exactly what it does -- completion, hover, \
                     definition and diagnostics; ask `jails editor --help` for the \
                     read-only protocol that answers the rest"
                ),
            )),
        }
    }

    /// A notification, which is not -- though it may produce one of ours.
    fn notify(&mut self, method: &str, params: Option<&Value>) -> Vec<Value> {
        match method {
            "textDocument/didOpen" => {
                let Some((uri, text)) = opened(params) else {
                    return Vec::new();
                };
                self.documents.insert(uri.clone(), text);
                self.publish(&uri)
            }
            "textDocument/didChange" => {
                let Some((uri, text)) = changed(params) else {
                    return Vec::new();
                };
                self.documents.insert(uri.clone(), text);
                self.publish(&uri)
            }
            "textDocument/didClose" => {
                if let Some(uri) = document_uri(params) {
                    self.documents.remove(&uri);
                    // **A closed document's diagnostics are cleared.** The
                    // specification leaves them owned by the server until it
                    // says otherwise, so a file closed with errors keeps
                    // them in the editor's list forever.
                    return vec![publish(&uri, Vec::new())];
                }
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    /// What this server can do, and the characters that should make a client
    /// ask.
    ///
    /// `@` is the item's own bar and the reason for the list; `:` is the
    /// type position, and a space is what separates a keyword from the name
    /// after it. `textDocumentSync: 1` is full text on every change, which
    /// is the right trade for a file of a few hundred lines and removes the
    /// whole class of bug where an incremental patch is applied wrongly.
    fn initialize(&self) -> Value {
        json!({
            "capabilities": {
                "textDocumentSync": 1,
                "completionProvider": { "triggerCharacters": ["@", ":", " "] },
                "hoverProvider": true,
                "definitionProvider": true,
            },
            "serverInfo": {
                "name": "jails",
                "version": env!("CARGO_PKG_VERSION"),
            },
        })
    }

    fn publish(&self, uri: &str) -> Vec<Value> {
        let Some(text) = self.documents.get(uri) else {
            return Vec::new();
        };
        vec![publish(uri, language::diagnostics(text))]
    }

    fn completion(&self, params: Option<&Value>) -> Value {
        let Some((text, line, column)) = self.position(params) else {
            return json!({ "isIncomplete": false, "items": [] });
        };
        let items = language::completion(text, line, column);
        json!({ "isIncomplete": false, "items": items })
    }

    fn hover(&self, params: Option<&Value>) -> Value {
        let Some((text, line, column)) = self.position(params) else {
            return Value::Null;
        };
        match language::hover(text, line, column) {
            Some(markdown) => json!({
                "contents": { "kind": "markdown", "value": markdown },
            }),
            None => Value::Null,
        }
    }

    fn definition(&self, params: Option<&Value>) -> Value {
        let (Some((text, line, column)), Some(project)) =
            (self.position(params), self.project.as_ref())
        else {
            return Value::Null;
        };
        json!(language::definition(text, line, column, project))
    }

    /// The document and the zero-based position a request is about.
    fn position(&self, params: Option<&Value>) -> Option<(&str, usize, usize)> {
        let params = params?;
        let uri = params.get("textDocument")?.get("uri")?.as_str()?;
        let position = params.get("position")?;
        let line = usize::try_from(position.get("line")?.as_u64()?).ok()?;
        let column = usize::try_from(position.get("character")?.as_u64()?).ok()?;
        Some((self.documents.get(uri)?.as_str(), line, column))
    }
}

fn publish(uri: &str, diagnostics: Vec<Value>) -> Value {
    json!({
        "jsonrpc": "2.0",
        "method": "textDocument/publishDiagnostics",
        "params": { "uri": uri, "diagnostics": diagnostics },
    })
}

fn document_uri(params: Option<&Value>) -> Option<String> {
    Some(
        params?
            .get("textDocument")?
            .get("uri")?
            .as_str()?
            .to_string(),
    )
}

fn opened(params: Option<&Value>) -> Option<(String, String)> {
    let item = params?.get("textDocument")?;
    Some((
        item.get("uri")?.as_str()?.to_string(),
        item.get("text")?.as_str()?.to_string(),
    ))
}

/// The full text of the last change, which with `textDocumentSync: 1` is the
/// whole document.
fn changed(params: Option<&Value>) -> Option<(String, String)> {
    let uri = document_uri(params)?;
    let text = params?
        .get("contentChanges")?
        .as_array()?
        .last()?
        .get("text")?
        .as_str()?
        .to_string();
    Some((uri, text))
}

/// Where the workspace is, from whichever of the three spellings the client
/// sent. `rootUri` is deprecated and still what several clients send.
fn root_of(params: &Value) -> Option<std::path::PathBuf> {
    let from_folders = params
        .get("workspaceFolders")
        .and_then(Value::as_array)
        .and_then(|folders| folders.first())
        .and_then(|folder| folder.get("uri"))
        .and_then(Value::as_str)
        .and_then(document::path_of);
    from_folders
        .or_else(|| {
            params
                .get("rootUri")
                .and_then(Value::as_str)
                .and_then(document::path_of)
        })
        .or_else(|| {
            params
                .get("rootPath")
                .and_then(Value::as_str)
                .map(std::path::PathBuf::from)
        })
}

/// One `Content-Length`-framed message, or `None` at end of input.
///
/// **The header is read, not assumed.** A client may send `Content-Type`
/// beside the length and the specification allows any order, so the loop
/// reads headers until the blank line rather than expecting one line.
fn read_message(input: &mut impl BufRead) -> Result<Option<String>> {
    let mut length: Option<usize> = None;
    loop {
        let mut header = String::new();
        let read = input
            .read_line(&mut header)
            .map_err(|error| format!("could not read a header: {error}"))?;
        if read == 0 {
            return Ok(None);
        }
        let header = header.trim_end_matches(['\r', '\n']);
        if header.is_empty() {
            break;
        }
        if let Some(value) = header
            .strip_prefix("Content-Length:")
            .or_else(|| header.strip_prefix("content-length:"))
        {
            length = value.trim().parse().ok();
        }
    }
    let Some(length) = length else {
        // A body of unknown length cannot be skipped past, so the stream is
        // no longer one this server can follow.
        return Err(
            "a message arrived with no `Content-Length` header.\n       fix: frame \
                    every message with one; jails speaks LSP over stdio here, and \
                    `jails mcp` is the server that reads one JSON object per line"
                .into(),
        );
    };
    let mut body = vec![0_u8; length];
    input
        .read_exact(&mut body)
        .map_err(|error| format!("could not read a {length}-byte message: {error}"))?;
    String::from_utf8(body)
        .map(Some)
        .map_err(|error| format!("a message was not UTF-8: {error}").into())
}

fn write_message(output: &mut impl Write, message: &Value) -> Result<()> {
    let body = message.to_string();
    write!(output, "Content-Length: {}\r\n\r\n{body}", body.len())
        .and_then(|()| output.flush())
        .map_err(|error| format!("could not write a response: {error}"))?;
    Ok(())
}
