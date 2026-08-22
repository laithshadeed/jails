//! Apply a small, declarative application manifest through Jails' existing
//! capability and generator engines.
//!
//! This is deliberately domain-blind. A crawler and a support inbox are two
//! different lists of the same generic intents; neither gets a command,
//! branch, enum, or template in Jails core.

use crate::Result;
use crate::add::Capability;
use crate::generate::{self, ArtifactKind};
use clap::{Subcommand, ValueEnum};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

const DEFAULT_MANIFEST: &str = ".jails/app.toml";
const STATE_FILE: &str = ".jails/app-state-v1";

#[derive(Subcommand)]
pub(crate) enum AppCommand {
    /// Create a documented starter manifest for this project
    Init {
        /// Manifest path; defaults to .jails/app.toml in the project
        #[arg(long)]
        manifest: Option<PathBuf>,
    },
    /// Show the generic capability and generation intents without writing
    Plan {
        /// Manifest path; defaults to .jails/app.toml in the project
        #[arg(long)]
        manifest: Option<PathBuf>,
    },
    /// Apply every unapplied intent, recording progress after each one
    Apply {
        /// Manifest path; defaults to .jails/app.toml in the project
        #[arg(long)]
        manifest: Option<PathBuf>,
        /// Write Compose services but do not start them
        #[arg(long)]
        no_start: bool,
    },
}

#[derive(Debug, Default)]
struct Manifest {
    schema: u32,
    capabilities: Vec<Capability>,
}

#[derive(Debug, Default)]
struct GenerateIntent {
    kind: Option<ArtifactKind>,
    name: Option<String>,
    fields: Vec<String>,
    timestamps: bool,
    indexes: Vec<String>,
    package: Option<String>,
    strategy_on: Option<String>,
    strategy_yields: Option<String>,
}

impl GenerateIntent {
    fn finish(self, number: usize) -> Result<ResolvedIntent> {
        let kind = self
            .kind
            .ok_or_else(|| format!("[[generate]] #{number} is missing `kind`"))?;
        let name = self
            .name
            .ok_or_else(|| format!("[[generate]] #{number} is missing `name`"))?;
        if name.is_empty() {
            return Err(format!("[[generate]] #{number} has an empty `name`"));
        }
        for value in self
            .fields
            .iter()
            .chain(self.indexes.iter())
            .chain(self.package.iter())
            .chain(self.strategy_on.iter())
            .chain(self.strategy_yields.iter())
        {
            if value.contains(['\n', '\r', '|']) {
                return Err(format!(
                    "[[generate]] #{number} contains a newline or `|`, which is not allowed"
                ));
            }
        }
        Ok(ResolvedIntent {
            kind,
            name,
            fields: self.fields,
            timestamps: self.timestamps,
            indexes: self.indexes,
            package: self.package,
            strategy_on: self.strategy_on,
            strategy_yields: self.strategy_yields,
        })
    }
}

#[derive(Debug)]
struct ResolvedIntent {
    kind: ArtifactKind,
    name: String,
    fields: Vec<String>,
    timestamps: bool,
    indexes: Vec<String>,
    package: Option<String>,
    strategy_on: Option<String>,
    strategy_yields: Option<String>,
}

impl ResolvedIntent {
    fn key(&self) -> String {
        format!(
            "{}|{}|{}|{}|{}|{}|{}|{}",
            self.kind
                .to_possible_value()
                .expect("every ArtifactKind has a clap value")
                .get_name(),
            self.name,
            self.package.as_deref().unwrap_or(""),
            self.fields.join(","),
            self.timestamps,
            self.indexes.join(","),
            self.strategy_on.as_deref().unwrap_or(""),
            self.strategy_yields.as_deref().unwrap_or("")
        )
    }

    fn label(&self) -> String {
        format!(
            "generate {} {}{}",
            self.kind
                .to_possible_value()
                .expect("every ArtifactKind has a clap value")
                .get_name(),
            self.name,
            if self.fields.is_empty() {
                String::new()
            } else {
                format!(" {}", self.fields.join(" "))
            }
        )
    }

    fn apply(&self, pretend: bool) -> Result<()> {
        generate::generate_with_timestamps(
            self.kind,
            &self.name,
            &self.fields,
            self.timestamps,
            self.package.as_deref(),
            &self.indexes,
            self.strategy_on.as_deref(),
            self.strategy_yields.as_deref(),
            pretend,
        )
    }
}

pub(crate) fn run(command: AppCommand, debug: bool, pretend: bool) -> Result<()> {
    let root = generate::find_project_root()?;
    match command {
        AppCommand::Init { manifest } => init(&root, manifest.as_deref(), pretend),
        AppCommand::Plan { manifest } => plan(&root, manifest.as_deref()),
        AppCommand::Apply { manifest, no_start } if pretend => {
            let _ = no_start;
            plan(&root, manifest.as_deref())
        }
        AppCommand::Apply { manifest, no_start } => {
            apply(&root, manifest.as_deref(), no_start, debug)
        }
    }
}

fn init(root: &Path, requested: Option<&Path>, pretend: bool) -> Result<()> {
    let path = match requested {
        Some(path) if path.is_absolute() => path.to_path_buf(),
        Some(path) => root.join(path),
        None => root.join(DEFAULT_MANIFEST),
    };
    if path.exists() {
        return Err(format!(
            "application manifest already exists: {}.\n       fix: edit it, or pass --manifest with a new path.",
            path.display()
        ));
    }
    if pretend {
        println!("would create application manifest {}", path.display());
        println!();
        println!("--pretend: nothing was written.");
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
    }
    fs::write(
        &path,
        "# Generic application intent. Add capabilities, then one [[generate]] table per slice.\n\
         schema = 1\n\
         capabilities = []\n\n\
         # [[generate]]\n\
         # kind = \"scaffold\"\n\
         # name = \"Note\"\n\
         # fields = [\"id:uuid@pk\", \"title:string!\"]\n\
         # timestamps = true\n",
    )
    .map_err(|error| format!("failed to write {}: {error}", path.display()))?;
    println!("created application manifest {}", path.display());
    Ok(())
}

fn plan(root: &Path, requested: Option<&Path>) -> Result<()> {
    let path = manifest_path(root, requested)?;
    let (manifest, intents) = read_manifest(&path)?;
    crate::add::preflight(&manifest.capabilities, None, None)?;
    let state = read_state(root)?;

    println!("application plan: {}", path.display());
    println!("schema: {}", manifest.schema);
    for capability in &manifest.capabilities {
        println!("  ensure capability  {}", capability.label());
    }
    for intent in &intents {
        let status = if state.contains(&intent.key()) {
            "applied"
        } else {
            "pending"
        };
        println!("  {status:7}  {}", intent.label());
    }
    println!();
    println!("plan only -- nothing was written");
    Ok(())
}

fn apply(root: &Path, requested: Option<&Path>, no_start: bool, debug: bool) -> Result<()> {
    let path = manifest_path(root, requested)?;
    let (manifest, intents) = read_manifest(&path)?;
    crate::add::preflight(&manifest.capabilities, None, None)?;

    println!("applying application manifest {}", path.display());
    for &capability in &manifest.capabilities {
        // Formatting only has useful work after generation. Installing it
        // here used to run Spotless once over the starter project and then a
        // second time during reconciliation over the generated sources. The
        // latter is the actual invariant; defer installation so one Maven
        // lifecycle formats the complete final tree.
        if matches!(capability, Capability::Format) {
            continue;
        }
        crate::add::add(capability, None, false, None, debug, no_start)?;
    }

    let mut state = read_state(root)?;
    for intent in intents {
        let key = intent.key();
        if state.contains(&key) {
            println!("  applied  {}", intent.label());
            continue;
        }
        intent.apply(false)?;
        state.insert(key);
        write_state(root, &state)?;
    }

    // A generator can create a new integration point for an already-applied
    // capability. The database capability is the concrete first case: it
    // wires every existing @SpringBootTest to Testcontainers, then a later
    // generator creates more @SpringBootTest classes. A second idempotent
    // reconciliation makes capability invariants describe the final tree,
    // not only the tree that happened to exist at installation time. Format
    // is deliberately installed here for the first time (see above).
    if !manifest.capabilities.is_empty() {
        println!("reconciling capabilities against generated artifacts");
    }
    for capability in manifest.capabilities {
        crate::add::add(capability, None, false, None, debug, true)?;
    }

    println!("application manifest applied");
    Ok(())
}

fn manifest_path(root: &Path, requested: Option<&Path>) -> Result<PathBuf> {
    let path = match requested {
        Some(path) if path.is_absolute() => path.to_path_buf(),
        Some(path) => std::env::current_dir()
            .map_err(|e| format!("failed to get cwd: {e}"))?
            .join(path),
        None => root.join(DEFAULT_MANIFEST),
    };
    if !path.is_file() {
        return Err(format!(
            "application manifest not found: {}\n\nfix: create {DEFAULT_MANIFEST}, or pass `--manifest <path>`.",
            path.display()
        ));
    }
    Ok(path)
}

fn read_manifest(path: &Path) -> Result<(Manifest, Vec<ResolvedIntent>)> {
    let text =
        fs::read_to_string(path).map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    parse_manifest(&text).map_err(|e| format!("{}: {e}", path.display()))
}

fn parse_manifest(text: &str) -> Result<(Manifest, Vec<ResolvedIntent>)> {
    let mut manifest = Manifest::default();
    let mut current: Option<GenerateIntent> = None;
    let mut resolved = Vec::new();

    for (offset, raw) in text.lines().enumerate() {
        let line_number = offset + 1;
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        if line == "[[generate]]" {
            if let Some(intent) = current.take() {
                resolved.push(intent.finish(resolved.len() + 1)?);
            }
            current = Some(GenerateIntent::default());
            continue;
        }
        if line.starts_with('[') {
            return Err(format!(
                "line {line_number}: unknown table `{line}`; only `[[generate]]` is supported"
            ));
        }
        let (key, raw_value) = line
            .split_once('=')
            .ok_or_else(|| format!("line {line_number}: expected `key = value`, found `{line}`"))?;
        let key = key.trim();
        let value = raw_value.trim();

        if let Some(intent) = current.as_mut() {
            match key {
                "kind" => {
                    let value = string(value, line_number, key)?;
                    intent.kind = Some(ArtifactKind::from_str(value, false).map_err(|_| {
                        format!(
                            "line {line_number}: unknown generator kind `{value}`; known: {}",
                            ArtifactKind::value_variants()
                                .iter()
                                .filter_map(|kind| kind.to_possible_value())
                                .map(|value| value.get_name().to_string())
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    })?);
                }
                "name" => intent.name = Some(string(value, line_number, key)?.to_string()),
                "fields" => intent.fields = string_array(value, line_number, key)?,
                "timestamps" => {
                    intent.timestamps = match value {
                        "true" => true,
                        "false" => false,
                        _ => {
                            return Err(format!(
                                "line {line_number}: `timestamps` must be true or false"
                            ));
                        }
                    }
                }
                "indexes" => intent.indexes = string_array(value, line_number, key)?,
                "package" => intent.package = Some(string(value, line_number, key)?.to_string()),
                "strategy_on" => {
                    intent.strategy_on = Some(string(value, line_number, key)?.to_string())
                }
                "strategy_yields" => {
                    intent.strategy_yields = Some(string(value, line_number, key)?.to_string())
                }
                _ => {
                    return Err(format!(
                        "line {line_number}: unknown [[generate]] key `{key}`"
                    ));
                }
            }
            continue;
        }

        match key {
            "schema" => {
                manifest.schema = value
                    .parse::<u32>()
                    .map_err(|_| format!("line {line_number}: `schema` must be an integer"))?;
            }
            "capabilities" => {
                for label in string_array(value, line_number, key)? {
                    let capability = Capability::from_str(&label, false).map_err(|_| {
                        format!(
                            "line {line_number}: unknown capability `{label}`; known: {}",
                            Capability::value_variants()
                                .iter()
                                .map(|capability| capability.label())
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    })?;
                    if !manifest.capabilities.contains(&capability) {
                        manifest.capabilities.push(capability);
                    }
                }
            }
            _ => return Err(format!("line {line_number}: unknown top-level key `{key}`")),
        }
    }

    if let Some(intent) = current {
        resolved.push(intent.finish(resolved.len() + 1)?);
    }
    if manifest.schema != 1 {
        return Err(format!(
            "unsupported schema {}; this Jails release supports schema 1",
            manifest.schema
        ));
    }
    let mut keys = HashSet::new();
    for intent in &resolved {
        if !keys.insert(intent.key()) {
            return Err(format!("duplicate intent: {}", intent.label()));
        }
    }
    Ok((manifest, resolved))
}

fn strip_comment(line: &str) -> &str {
    let mut quoted = false;
    let mut escaped = false;
    for (index, ch) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' if quoted => escaped = true,
            '"' => quoted = !quoted,
            '#' if !quoted => return &line[..index],
            _ => {}
        }
    }
    line
}

fn string<'a>(value: &'a str, line: usize, key: &str) -> Result<&'a str> {
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .ok_or_else(|| format!("line {line}: `{key}` must be a double-quoted string"))
}

fn string_array(value: &str, line: usize, key: &str) -> Result<Vec<String>> {
    let inner = value
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .ok_or_else(|| format!("line {line}: `{key}` must be a one-line string array"))?
        .trim();
    if inner.is_empty() {
        return Ok(Vec::new());
    }
    let mut values = Vec::new();
    let bytes = inner.as_bytes();
    let mut at = 0;
    while at < bytes.len() {
        while at < bytes.len() && bytes[at].is_ascii_whitespace() {
            at += 1;
        }
        if at == bytes.len() || bytes[at] != b'"' {
            return Err(format!(
                "line {line}: `{key}` must contain only double-quoted strings"
            ));
        }
        at += 1;
        let start = at;
        let mut escaped = false;
        while at < bytes.len() {
            if escaped {
                escaped = false;
                at += 1;
                continue;
            }
            match bytes[at] {
                b'\\' => escaped = true,
                b'"' => break,
                _ => {}
            }
            at += 1;
        }
        if at == bytes.len() {
            return Err(format!("line {line}: unterminated string in `{key}`"));
        }
        values.push(inner[start..at].to_string());
        at += 1;
        while at < bytes.len() && bytes[at].is_ascii_whitespace() {
            at += 1;
        }
        if at == bytes.len() {
            break;
        }
        if bytes[at] != b',' {
            return Err(format!(
                "line {line}: expected a comma between strings in `{key}`"
            ));
        }
        at += 1;
    }
    Ok(values)
}

fn read_state(root: &Path) -> Result<HashSet<String>> {
    let path = root.join(STATE_FILE);
    match fs::read_to_string(&path) {
        Ok(text) => {
            let mut lines = text.lines();
            if lines.next() != Some("schema=1") {
                return Err(format!(
                    "{} has an unsupported or missing schema header",
                    path.display()
                ));
            }
            Ok(lines
                .filter(|line| !line.trim().is_empty())
                .map(ToString::to_string)
                .collect())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(HashSet::new()),
        Err(error) => Err(format!("failed to read {}: {error}", path.display())),
    }
}

fn write_state(root: &Path, state: &HashSet<String>) -> Result<()> {
    let path = root.join(STATE_FILE);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create {}: {e}", parent.display()))?;
    }
    let mut entries: Vec<&str> = state.iter().map(String::as_str).collect();
    entries.sort_unstable();
    let mut text = String::from("schema=1\n");
    for entry in entries {
        text.push_str(entry);
        text.push('\n');
    }
    fs::write(&path, text).map_err(|e| format!("failed to write {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_domain_blind_application_manifest() {
        let (_, intents) = parse_manifest(
            r#"
                schema = 1
                capabilities = ["db", "api"]

                [[generate]]
                kind = "enum"
                name = "Status"
                fields = ["PENDING", "DONE"]

                [[generate]]
                kind = "scaffold"
                name = "Task"
                fields = ["id:uuid@pk", "status:Status"]
                indexes = ["status, id"]
            "#,
        )
        .unwrap();
        assert_eq!(intents.len(), 2);
        assert_eq!(intents[1].name, "Task");
        assert_eq!(intents[1].indexes, ["status, id"]);
    }

    #[test]
    fn rejects_unknown_keys_instead_of_silently_ignoring_them() {
        let error = parse_manifest(
            r#"
                schema = 1
                capabilities = []
                [[generate]]
                kind = "record"
                name = "Task"
                feilds = ["id:uuid"]
            "#,
        )
        .unwrap_err();
        assert!(error.contains("feilds"), "{error}");
    }

    #[test]
    fn a_capability_only_application_is_valid() {
        let (manifest, intents) = parse_manifest(
            r#"
                schema = 1
                capabilities = ["api", "actuator"]
            "#,
        )
        .unwrap();
        assert_eq!(manifest.capabilities.len(), 2);
        assert!(intents.is_empty());
    }
}
