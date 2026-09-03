//! Read-only editor protocol projections derived from ordinary project facts.

use crate::cli::{EditorCommand, EditorDiagnosticScopeArg, EditorSymbolKindArg, Output};
use crate::{Cli, inspect, pom, project};
use clap::CommandFactory;
use jails_support::Result;
use jails_support::domain_hash;
use jails_support::identity::ObjectId;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub(crate) fn run(command: EditorCommand, invocation: crate::Invocation) -> Result<()> {
    if invocation.output != Output::Json {
        return Err(
            "editor protocol commands require `--output json`.\n       fix: pass `--output json`; editor adapters must not parse human output."
                .into(),
        );
    }
    match command {
        EditorCommand::Handshake { path } => handshake(path.as_deref()),
        EditorCommand::Complete {
            arg_index,
            byte_offset,
            path,
            argv,
        } => complete(arg_index, byte_offset, &argv, path.as_deref()),
        EditorCommand::Symbols { kind, query, path } => {
            symbols(kind, query.as_deref(), path.as_deref())
        }
        EditorCommand::Diagnostics { scope, file, path } => {
            diagnostics(scope, file.as_deref(), path.as_deref())
        }
    }
}

fn handshake(start: Option<&Path>) -> Result<()> {
    let project = project_containing(start)?;
    let digest = root_digest(&project)?;
    let root = project.root();
    let mut builds = Vec::new();
    if root.join("pom.xml").is_file() {
        builds.push("maven");
    }
    if root.join("build.gradle").is_file() || root.join("build.gradle.kts").is_file() {
        builds.push("gradle");
    }
    let release = std::fs::read_to_string(root.join("pom.xml"))
        .ok()
        .and_then(|text| pom::release_level(&text))
        .unwrap_or(26);
    // The project's own inputs come from the one answer to "where is the
    // source", so an editor and the scanners inside the tool cannot be told
    // two different trees. `target/generated-sources` is not one of them: it
    // is a build *output* an annotation processor writes, which an editor
    // indexes and no jails scanner reads.
    let roots = inspect::roots::input_roots(root)
        .into_iter()
        .map(|input| (input.label(), input.relative))
        .chain([("generated", "target/generated-sources")])
        .filter(|(_, path)| root.join(path).is_dir())
        .map(|(kind, path)| format!("{{\"path\":{},\"kind\":{}}}", js(path), js(kind)))
        .collect::<Vec<_>>()
        .join(",");
    println!(
        "{{\"schema\":\"jails.editor-handshake.v1\",\"editor_protocol\":1,\"cli_version\":{},\"command_result_schema\":\"jails.command-result.v2\",\"event_schema\":\"jails.event.v1\",\"project\":{{\"identity\":{},\"root_digest\":{},\"build_systems\":[{}],\"java_release\":{},\"new_project_default_java_release\":26,\"source_roots\":[{}]}},\"capabilities\":[\"completion-v1\",\"symbols-v1\",\"diagnostics-v1\",\"prepared-plans-v1\",\"test-watch-events-v1\",\"testd-v2\"]}}",
        js(env!("CARGO_PKG_VERSION")),
        js(&digest.to_string()),
        js(&digest.to_string()),
        builds.into_iter().map(js).collect::<Vec<_>>().join(","),
        release,
        roots
    );
    Ok(())
}

fn complete(
    argument_index: u32,
    byte_offset: u32,
    argv: &[String],
    start: Option<&Path>,
) -> Result<()> {
    let index = usize::try_from(argument_index).map_err(|_| "argument index overflows usize")?;
    let token = argv.get(index).map(String::as_str).unwrap_or("");
    let end = usize::try_from(byte_offset).map_err(|_| "byte offset overflows usize")?;
    if end > token.len() || !token.is_char_boundary(end) {
        return Err(
            "editor completion byte offset is outside the selected UTF-8 argument.\n       fix: send a byte offset on a UTF-8 code-point boundary."
                .into(),
        );
    }
    let prefix = &token[..end];
    let mut command = Cli::command();
    for prior in argv.iter().take(index) {
        let next = {
            command
                .get_subcommands()
                .find(|candidate| {
                    candidate.get_name() == prior
                        || candidate.get_all_aliases().any(|alias| alias == prior)
                })
                .cloned()
        };
        if let Some(next) = next {
            command = next;
        }
    }
    let mut candidates = BTreeSet::new();
    if !prefix.starts_with('-') {
        for child in command.get_subcommands() {
            if child.get_name().starts_with(prefix) {
                candidates.insert((0, child.get_name().to_string(), about(child)));
            }
            for alias in child.get_all_aliases() {
                if alias.starts_with(prefix) {
                    candidates.insert((0, alias.to_string(), about(child)));
                }
            }
        }
    }
    for arg in command.get_arguments() {
        if let Some(long) = arg.get_long() {
            let value = format!("--{long}");
            if value.starts_with(prefix) {
                candidates.insert((1, value, arg.get_help().map(|h| h.to_string())));
            }
        }
    }
    let mut rows = candidates
        .into_iter()
        .map(|(kind, value, description)| {
            candidate_json(
                &value,
                if kind == 0 { "command" } else { "option" },
                description.as_deref(),
            )
        })
        .collect::<Vec<_>>();
    // **The model's answers come first.** A reader who has typed three
    // letters of a component name is not choosing between that and a
    // subcommand, and a candidate list is read from the top.
    if let Ok(project) = project_containing(start) {
        let model = crate::editor_complete::candidates(&command, argv, index, prefix, &project);
        let mut offered = model
            .into_iter()
            .map(|candidate| {
                candidate_json(
                    &candidate.value,
                    candidate.kind,
                    candidate.description.as_deref(),
                )
            })
            .collect::<Vec<_>>();
        offered.append(&mut rows);
        rows = offered;
    }
    let rows = rows.join(",");
    let start = token[..end]
        .rfind(char::is_whitespace)
        .map_or(0, |at| at + 1);
    println!(
        "{{\"schema\":\"jails.editor-completion.v1\",\"input\":{{\"argument_index\":{argument_index},\"byte_offset\":{byte_offset}}},\"replace\":{{\"argument_index\":{argument_index},\"start_byte\":{start},\"end_byte\":{end}}},\"candidates\":[{rows}]}}"
    );
    Ok(())
}

fn candidate_json(value: &str, kind: &str, description: Option<&str>) -> String {
    format!(
        "{{\"value\":{},\"display\":{},\"kind\":{},\"description\":{}}}",
        js(value),
        js(value),
        js(kind),
        description.map_or_else(|| "null".into(), js)
    )
}

fn about(command: &clap::Command) -> Option<String> {
    command.get_about().map(|value| value.to_string())
}

#[derive(Eq, Ord, PartialEq, PartialOrd)]
struct Symbol {
    label: String,
    id: String,
    detail: Option<String>,
    path: Option<String>,
    line: usize,
}

fn symbols(kind: EditorSymbolKindArg, query: Option<&str>, start: Option<&Path>) -> Result<()> {
    let project = project_containing(start)?;
    let digest = root_digest(&project)?;
    let mut found = match kind {
        EditorSymbolKindArg::Routes => inspect::collect_routes(project.root())
            .into_iter()
            .map(|route| Symbol {
                label: format!("{} {}", route.verb, route.path),
                id: format!("route:{}:{}:{}", route.verb, route.path, route.handler),
                detail: Some(route.handler),
                path: Some(route.source),
                line: route.line.saturating_sub(1),
            })
            .collect(),
        EditorSymbolKindArg::Beans => inspect::collect_beans(project.root())
            .0
            .into_iter()
            .map(|bean| Symbol {
                label: bean.type_name.clone(),
                id: format!("bean:{}:{}", bean.type_name, bean.stereotype),
                detail: Some(format!("@{}", bean.stereotype)),
                path: Some(
                    bean.source
                        .split(" (")
                        .next()
                        .unwrap_or(&bean.source)
                        .to_string(),
                ),
                line: bean.line.saturating_sub(1),
            })
            .collect(),
        EditorSymbolKindArg::Tests => source_symbols(&project, true),
        EditorSymbolKindArg::Types => source_symbols(&project, false),
    };
    if let Some(query) = query {
        found.retain(|symbol| {
            subsequence(
                query,
                &format!("{} {:?} {}", symbol.label, symbol.detail, symbol.id),
            )
        });
    }
    found.sort_by(|left, right| {
        left.label
            .to_ascii_lowercase()
            .cmp(&right.label.to_ascii_lowercase())
            .then_with(|| left.id.cmp(&right.id))
    });
    let rows = found
        .into_iter()
        .map(symbol_json)
        .collect::<Vec<_>>()
        .join(",");
    println!(
        "{{\"schema\":\"jails.editor-symbols.v1\",\"root_digest\":{},\"epoch\":{},\"kind\":{},\"symbols\":[{}]}}",
        js(&digest.to_string()),
        epoch(digest),
        js(&format!("{kind:?}").to_ascii_lowercase()),
        rows
    );
    Ok(())
}

fn source_symbols(project: &project::Project, tests: bool) -> Vec<Symbol> {
    let source_root = project.root().join(if tests {
        "src/test/java"
    } else {
        "src/main/java"
    });
    jails_project::java::source_files(&source_root)
        .into_iter()
        .filter_map(|path| {
            let source = std::fs::read_to_string(&path).ok()?;
            let info = jails_project::java::type_info(&source)?;
            let qualified = if info.package.is_empty() {
                info.name.clone()
            } else {
                format!("{}.{}", info.package, info.name)
            };
            Some(Symbol {
                label: info.name,
                id: format!("{}:{qualified}", if tests { "test" } else { "type" }),
                detail: Some(qualified),
                path: path
                    .strip_prefix(project.root())
                    .ok()
                    .map(|path| path.to_string_lossy().into_owned()),
                line: 0,
            })
        })
        .collect()
}

fn symbol_json(symbol: Symbol) -> String {
    let location = symbol.path.map(|path| format!(
        "{{\"path\":{},\"range\":{{\"start\":{{\"line\":{},\"byte_column\":0}},\"end\":{{\"line\":{},\"byte_column\":0}}}}}}",
        js(&path), symbol.line, symbol.line.saturating_add(1)
    )).unwrap_or_else(|| "null".into());
    format!(
        "{{\"id\":{},\"label\":{},\"detail\":{},\"location\":{},\"evidence\":\"parsed\"}}",
        js(&symbol.id),
        js(&symbol.label),
        symbol
            .detail
            .map(|value| js(&value))
            .unwrap_or_else(|| "null".into()),
        location
    )
}

fn diagnostics(
    scope: EditorDiagnosticScopeArg,
    file: Option<&Path>,
    start: Option<&Path>,
) -> Result<()> {
    if scope == EditorDiagnosticScopeArg::Project && file.is_some() {
        return Err("`--file` is invalid for project diagnostics.\n       fix: omit it or select `--scope buffer`.".into());
    }
    let project = project_containing(start)?;
    let digest = root_digest(&project)?;
    let mut rows = Vec::new();
    if let Some(file) = file {
        let path = checked_relative(file)?;
        if let Err(error) = std::fs::read_to_string(project.root().join(path)) {
            rows.push(diagnostic_json(
                "file-unreadable",
                "error",
                &error.to_string(),
                Some(&path.to_string_lossy()),
                0,
            ));
        }
    }
    let scope_json = match (scope, file) {
        (EditorDiagnosticScopeArg::Buffer, Some(file)) => format!(
            "{{\"buffer\":{}}}",
            js(&checked_relative(file)?.to_string_lossy())
        ),
        (EditorDiagnosticScopeArg::Buffer, None) => return Err(
            "buffer diagnostics require `--file`.\n       fix: pass a project-relative file path."
                .into(),
        ),
        (EditorDiagnosticScopeArg::Project, _) => js("project"),
    };
    println!(
        "{{\"schema\":\"jails.editor-diagnostics.v1\",\"root_digest\":{},\"epoch\":{},\"scope\":{},\"diagnostics\":[{}]}}",
        js(&digest.to_string()),
        epoch(digest),
        scope_json,
        rows.join(",")
    );
    Ok(())
}

fn diagnostic_json(
    code: &str,
    severity: &str,
    message: &str,
    path: Option<&str>,
    line: usize,
) -> String {
    let primary = path.map(|path| format!("{{\"path\":{},\"range\":{{\"start\":{{\"line\":{},\"byte_column\":0}},\"end\":{{\"line\":{},\"byte_column\":0}}}}}}", js(path), line, line.saturating_add(1))).unwrap_or_else(|| "null".into());
    format!(
        "{{\"code\":{},\"severity\":{},\"message\":{},\"subject\":null,\"primary\":{},\"related\":[],\"evidence\":[{}],\"fixes\":[]}}",
        js(code),
        js(severity),
        js(message),
        primary,
        "\"parsed\""
    )
}

fn project_containing(start: Option<&Path>) -> Result<project::Project> {
    let start = match start {
        Some(path) => path.to_path_buf(),
        None => std::env::current_dir()
            .map_err(|error| format!("could not read the current directory: {error}"))?,
    };
    let mut current = if start.is_file() {
        start.parent().unwrap_or(&start).to_path_buf()
    } else {
        start
    };
    loop {
        if current.join("pom.xml").is_file()
            || current.join("build.gradle").is_file()
            || current.join("build.gradle.kts").is_file()
        {
            return project::Project::load(&current);
        }
        if !current.pop() {
            return Err("not inside a Maven or Gradle project.\n       fix: pass `--path` beneath a project build file.".into());
        }
    }
}

fn root_digest(project: &project::Project) -> Result<ObjectId> {
    let mut files = BTreeMap::new();
    for name in [
        "pom.xml",
        "build.gradle",
        "build.gradle.kts",
        "settings.gradle",
        "settings.gradle.kts",
        "jails.toml",
        "compose.yaml",
        ".jails/app.toml",
    ] {
        let path = project.root().join(name);
        if let Ok(bytes) = std::fs::read(path) {
            files.insert(name.to_string(), bytes);
        }
    }
    for base in ["src/main", "src/test"] {
        collect_files(project, &project.root().join(base), &mut files)?;
    }
    let mut input = Vec::new();
    for (path, body) in files {
        input.extend_from_slice(path.as_bytes());
        input.push(0);
        input.extend_from_slice(&body);
        input.push(0xff);
    }
    Ok(ObjectId::from_bytes(domain_hash(
        "JAILS-EDITOR-ROOT-1",
        &input,
    )))
}

fn collect_files(
    project: &project::Project,
    dir: &Path,
    output: &mut BTreeMap<String, Vec<u8>>,
) -> Result<()> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Ok(());
    };
    let mut entries = entries
        .collect::<std::io::Result<Vec<_>>>()
        .map_err(|error| format!("could not read {}: {error}", dir.display()))?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            collect_files(project, &path, output)?;
        } else if path.is_file() {
            output.insert(
                path.strip_prefix(project.root())
                    .expect("walk is below root")
                    .to_string_lossy()
                    .into_owned(),
                std::fs::read(&path)
                    .map_err(|error| format!("could not read {}: {error}", path.display()))?,
            );
        }
    }
    Ok(())
}

fn checked_relative(path: &Path) -> Result<&Path> {
    if path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err("editor paths must be project-relative and may not contain `..`.\n       fix: send the slash path reported by the handshake or symbols response.".into());
    }
    Ok(path)
}

fn subsequence(needle: &str, haystack: &str) -> bool {
    let mut chars = needle
        .to_ascii_lowercase()
        .chars()
        .collect::<Vec<_>>()
        .into_iter();
    let mut wanted = chars.next();
    for actual in haystack.to_ascii_lowercase().chars() {
        if wanted == Some(actual) {
            wanted = chars.next();
            if wanted.is_none() {
                return true;
            }
        }
    }
    wanted.is_none()
}

fn epoch(digest: ObjectId) -> u64 {
    u64::from_str_radix(&digest.to_string()[..16], 16).unwrap_or(0)
}

fn js(value: &str) -> String {
    jails_support::json::string(value)
}
