//! `jails.toml` -- what a project says about itself: where generated code
//! lands, and what the project is made of.
//!
//! jails ships a layer layout (`domain`, `service`, `web`, `app`, `adapters`)
//! and every generator writes into it. That is a fine default and a bad
//! mandate: a project whose spec says `domain`/`application`/`persistence`/
//! `api` has to pass `--package` to *every* call, and one forgotten flag puts
//! a file in a package the project does not otherwise use. `--package` is a
//! per-call override; this is the per-project one.
//!
//! `[project] capabilities` is the other half, and the reason `jails sync`
//! can exist: `add` records every capability it applies and `remove` takes it
//! back out, so the file is a true description of the project rather than one
//! somebody has to remember to update. A manifest that is merely *aspirational*
//! would be worse than none, because `sync` acts on it.
//!
//! Still deliberately not a general config file -- no template overrides, no
//! plugin hooks, no per-kind paths. Both tables are **closed sets**: the layout
//! keys are exactly the constants in [`crate::spec::layout`], and the
//! capability names are derived from the `Capability` enum rather than
//! restated. A name that is not one of them is a typo and is reported as such
//! rather than ignored -- silently accepting `adapter = "persistence"`
//! (singular) would put files in `adapters` forever while the file claims
//! otherwise, and silently accepting `postgress` would leave a capability
//! that looks declared and never syncs.
//!
//! ## Why this is hand-parsed
//!
//! jails has two dependencies, both clap. The grammar needed here is one
//! table of `key = "value"` pairs, which is about forty lines to read
//! directly and does not justify pulling in a TOML parser plus its error
//! types. The cost is that this understands a *subset* of TOML: `[layout]`
//! and `[project]`, bare keys, double-quoted values, single-line string
//! arrays, `#` comments. Anything else in the file is ignored, and anything
//! malformed *inside* a table it knows is an error -- quietly skipping a line
//! the user clearly meant is the failure mode this whole module exists to
//! avoid.
//!
//! Writing back is a targeted splice, not a round trip: `record_capability`
//! rewrites the one `capabilities = [...]` line and leaves every other byte
//! alone, for the same reason `pom.rs` does. This is a file people edit.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::spec::layout;

/// The file, at the project root next to `pom.xml`.
pub const FILE: &str = "jails.toml";

/// Every layer name a project may rename, and the default it replaces.
///
/// Kept as a list rather than derived from the `layout` module so that adding
/// a constant there without deciding whether it is configurable is a
/// compile-time-visible omission rather than a silent one.
/// Every layer, in the order a reader wants them -- domain first, adapters
/// last -- with the heading `stats` prints for it.
///
/// **One list, not two.** `inspect.rs` used to keep its own copy of this and
/// its own labels, which meant `jails stats` reported against jails' default
/// package names: a project with `adapters = "persistence"` had its adapters
/// counted as "Other", and the two layers this list has that the copy did not
/// (`cli`, `messaging`) were never counted at all. A second list of the same
/// thing is how that happens, so the validation list below is derived from
/// this one rather than written out again.
pub const LAYERS_IN_ORDER: &[(&str, &str)] = &[
    (layout::DOMAIN, "Domain"),
    (layout::APP, "Ports"),
    (layout::SERVICE, "Services"),
    (layout::WEB, "Web"),
    (layout::API, "API"),
    (layout::MESSAGING, "Messaging"),
    (layout::CLI, "CLI"),
    (layout::CLIENTS, "Clients"),
    (layout::JOBS, "Jobs"),
    (layout::ADAPTERS, "Adapters"),
    (layout::TESTKIT, "Testkit"),
];

#[cfg(test)]
mod layer_list_tests {
    use super::LAYERS_IN_ORDER;
    use jails_spec::spec::layout::Layer;

    /// One list, in one order. `LAYERS_IN_ORDER` adds the report heading;
    /// everything else about a layer comes from `Layer`, and a layer added to
    /// one and not the other is the drift this file's own doc comment is
    /// about.
    #[test]
    fn the_report_list_covers_every_layer_in_declaration_order() {
        let listed: Vec<&str> = LAYERS_IN_ORDER.iter().map(|(name, _)| *name).collect();
        let declared: Vec<&str> = Layer::ALL.iter().map(|layer| layer.package()).collect();
        assert_eq!(listed, declared);
    }
}

fn is_layer(key: &str) -> bool {
    LAYERS_IN_ORDER.iter().any(|(name, _)| *name == key)
}

fn layer_names() -> Vec<&'static str> {
    LAYERS_IN_ORDER.iter().map(|(name, _)| *name).collect()
}

/// The table naming where each layer lives.
const LAYOUT_TABLE: &str = "layout";
/// The table naming the capabilities the project is meant to have.
const PROJECT_TABLE: &str = "project";
/// The one key in it.
const CAPABILITIES_KEY: &str = "capabilities";

/// A project's layout overrides: default layer name -> the name to use.
///
/// An absent file is an empty map, not an error -- the overwhelming majority
/// of projects never have one, and `Config::default()` behaving exactly like
/// today's hardcoded layout is what keeps this change from touching them.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Config {
    layout: HashMap<String, String>,
    /// Capability labels, in the order the file lists them. Validated against
    /// the real `Capability` set at parse time, so a typo is an error naming
    /// the real ones rather than a capability that silently never syncs.
    capabilities: Vec<String>,
}

impl Config {
    /// Read `jails.toml` from a project root. Absent file -> defaults.
    pub fn load(root: &Path) -> Result<Self, String> {
        let path = root.join(FILE);
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(format!("failed to read {}: {e}", path.display())),
        };
        Self::parse(&text).map_err(|e| format!("{FILE}: {e}"))
    }

    /// The package a layer's code belongs in, after any override.
    ///
    /// This is the only reader. `generate`'s `place` closure calls it for
    /// every artifact, so a layer that is renamed is renamed everywhere --
    /// including in `destroy`, which resolves its paths through the same
    /// closure and would otherwise strand files.
    pub fn layer<'a>(&'a self, default: &'a str) -> &'a str {
        self.layout
            .get(default)
            .map(String::as_str)
            .unwrap_or(default)
    }

    /// Every layer as `(package, heading)`, in display order, with this
    /// project's renames already applied.
    ///
    /// The one place anything reporting *per layer* should get its list, so a
    /// renamed layer is renamed there too rather than falling into a
    /// catch-all bucket.
    pub fn layers(&self) -> Vec<(String, &'static str)> {
        LAYERS_IN_ORDER
            .iter()
            .map(|(name, label)| (self.layer(name).to_string(), *label))
            .collect()
    }

    /// Canonical layer key and its configured package, in the same stable
    /// order the CLI and editor integrations consume.
    pub fn layout_entries(&self) -> Vec<(&'static str, String)> {
        LAYERS_IN_ORDER
            .iter()
            .map(|(name, _)| (*name, self.layer(name).to_string()))
            .collect()
    }

    /// The capabilities this project declares, in file order.
    pub fn capabilities(&self) -> &[String] {
        &self.capabilities
    }

    fn parse(text: &str) -> Result<Self, String> {
        let mut layout = HashMap::new();
        let mut capabilities = Vec::new();
        let mut table = String::new();

        for (i, raw) in text.lines().enumerate() {
            let line = strip_comment(raw).trim();
            if line.is_empty() {
                continue;
            }
            if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                table = name.trim().to_string();
                continue;
            }

            let lineno = i + 1;

            if table == PROJECT_TABLE {
                let (key, value) = line.split_once('=').ok_or_else(|| {
                    format!("line {lineno}: expected `key = value`, found `{line}`")
                })?;
                let key = key.trim();
                if key != CAPABILITIES_KEY {
                    return Err(format!(
                        "line {lineno}: unknown key `{key}` in [{PROJECT_TABLE}]. \
                         The only key is `{CAPABILITIES_KEY}`."
                    ));
                }
                for label in parse_string_array(value.trim()).ok_or_else(|| {
                    format!(
                        "line {lineno}: `{CAPABILITIES_KEY}` must be a list of \
                             double-quoted names, e.g. \
                             `{CAPABILITIES_KEY} = [\"db\", \"json\"]`"
                    )
                })? {
                    if !is_known_capability(&label) {
                        return Err(format!(
                            "line {lineno}: unknown capability `{label}`. Known: {}",
                            known_capabilities().join(", ")
                        ));
                    }
                    if !capabilities.contains(&label) {
                        capabilities.push(label);
                    }
                }
                continue;
            }

            if table != LAYOUT_TABLE {
                continue;
            }
            let (key, value) = line.split_once('=').ok_or_else(|| {
                format!("line {lineno}: expected `key = \"value\"`, found `{line}`")
            })?;
            let key = key.trim();
            let value = unquote(value.trim())
                .ok_or_else(|| format!("line {lineno}: `{key}` must be a double-quoted string"))?;

            if !is_layer(key) {
                return Err(format!(
                    "line {lineno}: unknown layer `{key}`. Known layers: {}",
                    layer_names().join(", ")
                ));
            }
            if !is_package_path(value) {
                return Err(format!(
                    "line {lineno}: `{key} = \"{value}\"` is not a package name"
                ));
            }
            layout.insert(key.to_string(), value.to_string());
        }

        Ok(Self {
            layout,
            capabilities,
        })
    }
}

/// Add a capability to `[project] capabilities`, creating `jails.toml` if the
/// project has none.
///
/// Called by `add` after it succeeds, so the manifest stays true without
/// anyone maintaining it by hand -- a file you have to remember to update is
/// a file that is wrong, and a wrong manifest is worse than none because
/// `sync` would act on it.
///
/// Rewrites only the one line. Everything else in the file -- comments, the
/// `[layout]` table, key order -- is left byte-for-byte alone, for the same
/// reason `pom.rs` splices rather than round-trips: this is a file people
/// edit.
pub fn record_capability(root: &Path, label: &str) -> Result<(), String> {
    edit_capabilities(root, |labels| {
        if labels.iter().any(|l| l == label) {
            return false;
        }
        labels.push(label.to_string());
        true
    })
}

/// Take a capability back out, for `remove`. The exact inverse of
/// `record_capability`: leaving it listed would have the next `sync` put back
/// what you just removed.
pub fn forget_capability(root: &Path, label: &str) -> Result<(), String> {
    edit_capabilities(root, |labels| {
        let before = labels.len();
        labels.retain(|l| l != label);
        labels.len() != before
    })
}

/// Read the declared list, let `change` mutate it, and write the file back if
/// it said something changed.
fn edit_capabilities(
    root: &Path,
    change: impl FnOnce(&mut Vec<String>) -> bool,
) -> Result<(), String> {
    let path = root.join(FILE);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("failed to read {}: {e}", path.display())),
    };

    let mut labels = Config::parse(&text)
        .map_err(|e| format!("{FILE}: {e}"))?
        .capabilities;
    if !change(&mut labels) {
        return Ok(());
    }

    let rendered = format!("{CAPABILITIES_KEY} = {}", render_string_array(&labels));
    let updated = match replace_capabilities_line(&text, &rendered) {
        Some(updated) => updated,
        None => append_project_table(&text, &rendered),
    };
    crate::apply::put(&path, updated)
}

/// Swap the existing `capabilities = [...]` line in place, keeping its
/// indentation. `None` when the file has no `[project]` table yet.
fn replace_capabilities_line(text: &str, rendered: &str) -> Option<String> {
    let mut table = String::new();
    let mut out: Vec<String> = Vec::new();
    let mut replaced = false;
    for raw in text.lines() {
        let line = strip_comment(raw).trim();
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            table = name.trim().to_string();
            out.push(raw.to_string());
            continue;
        }
        let is_target = table == PROJECT_TABLE
            && line
                .split_once('=')
                .is_some_and(|(key, _)| key.trim() == CAPABILITIES_KEY);
        if is_target {
            let indent: String = raw.chars().take_while(|c| c.is_whitespace()).collect();
            out.push(format!("{indent}{rendered}"));
            replaced = true;
        } else {
            out.push(raw.to_string());
        }
    }
    if !replaced {
        return None;
    }
    let mut joined = out.join("\n");
    if text.ends_with('\n') {
        joined.push('\n');
    }
    Some(joined)
}

fn append_project_table(text: &str, rendered: &str) -> String {
    let mut out = String::new();
    if text.trim().is_empty() {
        out.push_str(
            "# What this project is made of. `jails sync` makes the project\n\
             # match this file; `jails add` and `jails remove` keep it true.\n",
        );
    } else {
        out.push_str(text);
        if !text.ends_with('\n') {
            out.push('\n');
        }
        out.push('\n');
    }
    out.push_str(&format!("[{PROJECT_TABLE}]\n{rendered}\n"));
    out
}

/// Add or replace one `[layout]` entry, creating `jails.toml` if there is none.
///
/// The same surgical rule as `record_capability`: this is a file people edit,
/// so everything else in it -- comments, key order, `[project]` -- is left
/// byte-for-byte alone. `jails adopt` is the only caller, and it deliberately
/// cannot reach `[project] capabilities` from here.
pub fn record_layout(root: &Path, layer: &str, directory: &str) -> Result<(), String> {
    let path = root.join(FILE);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("failed to read {}: {e}", path.display())),
    };
    if !is_layer(layer) {
        return Err(format!(
            "`{layer}` is not a layer. Known layers: {}",
            layer_names().join(", ")
        ));
    }

    let rendered = format!("{layer} = \"{directory}\"");
    let updated = match replace_layout_line(&text, layer, &rendered) {
        Some(updated) => updated,
        None => insert_into_layout_table(&text, &rendered),
    };
    crate::apply::put(&path, updated)
}

/// Swap an existing `<layer> = "..."` inside `[layout]`, keeping its position.
fn replace_layout_line(text: &str, layer: &str, rendered: &str) -> Option<String> {
    let mut in_layout = false;
    let mut out = Vec::new();
    let mut replaced = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_layout = trimmed == format!("[{LAYOUT_TABLE}]");
        } else if in_layout && !replaced {
            let key = trimmed.split('=').next().unwrap_or("").trim();
            if key == layer {
                out.push(rendered.to_string());
                replaced = true;
                continue;
            }
        }
        out.push(line.to_string());
    }
    replaced.then(|| {
        let mut joined = out.join("\n");
        joined.push('\n');
        joined
    })
}

/// Append the entry to `[layout]`, adding the table if the file has none.
fn insert_into_layout_table(text: &str, rendered: &str) -> String {
    let header = format!("[{LAYOUT_TABLE}]");
    if let Some(at) = text.lines().position(|line| line.trim() == header) {
        let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
        // After the header and any entries already under it, before the blank
        // line or next table -- so entries stay together.
        let mut insert = at + 1;
        while insert < lines.len() && !lines[insert].trim_start().starts_with('[') {
            insert += 1;
        }
        while insert > at + 1 && lines[insert - 1].trim().is_empty() {
            insert -= 1;
        }
        lines.insert(insert, rendered.to_string());
        let mut joined = lines.join("\n");
        joined.push('\n');
        return joined;
    }
    let mut out = String::new();
    if text.trim().is_empty() {
        out.push_str(
            "# Where this project keeps each layer. Written by `jails adopt`;\n\
             # every jails command that reports or writes per layer reads it.\n",
        );
    } else {
        out.push_str(text);
        if !text.ends_with('\n') {
            out.push('\n');
        }
        out.push('\n');
    }
    out.push_str(&format!("{header}\n{rendered}\n"));
    out
}

fn render_string_array(labels: &[String]) -> String {
    let items: Vec<String> = labels.iter().map(|l| format!("\"{l}\"")).collect();
    format!("[{}]", items.join(", "))
}

/// Every capability label jails knows, derived from the `Capability` enum
/// rather than restated here -- a capability added there without a thought
/// for the manifest is then automatically valid in it, instead of being a
/// name this file rejects for no reason a reader could find.
fn known_capabilities() -> Vec<&'static str> {
    use clap::ValueEnum;
    crate::spec::kind::Capability::value_variants()
        .iter()
        .map(|c| c.label())
        .collect()
}

fn is_known_capability(label: &str) -> bool {
    known_capabilities().contains(&label)
}

/// `["a", "b"]` on one line. Deliberately not multi-line: the grammar this
/// module understands is a subset, and `jails add` writes the file itself, so
/// the one shape jails emits is the one it has to read.
fn parse_string_array(value: &str) -> Option<Vec<String>> {
    let inner = value.strip_prefix('[')?.strip_suffix(']')?.trim();
    if inner.is_empty() {
        return Some(Vec::new());
    }
    inner
        .split(',')
        .map(|item| unquote(item.trim()).map(str::to_string))
        .collect()
}

/// Drop a trailing `#` comment, but not one inside a quoted value --
/// `service = "app#1"` is a bad package name, and it should be reported as
/// one rather than silently truncated to `app`.
fn strip_comment(line: &str) -> &str {
    let mut quoted = false;
    for (i, c) in line.char_indices() {
        match c {
            '"' => quoted = !quoted,
            '#' if !quoted => return &line[..i],
            _ => {}
        }
    }
    line
}

fn unquote(value: &str) -> Option<&str> {
    value
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .filter(|v| !v.contains('"'))
}

/// An empty value is legal and means "the base package" -- the same thing
/// `--package ''` means, and the flat layout some projects want.
fn is_package_path(value: &str) -> bool {
    value.is_empty()
        || value.split('.').all(|seg| {
            !seg.is_empty()
                && seg
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
                && !seg.starts_with(|c: char| c.is_ascii_digit())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_layout_leaves_every_layer_at_its_default() {
        let config = Config::default();
        assert_eq!(config.layer(layout::SERVICE), "service");
        assert_eq!(config.layer(layout::ADAPTERS), "adapters");
    }

    #[test]
    fn a_renamed_layer_is_returned_and_the_others_are_not() {
        let config = Config::parse(
            r#"
            [layout]
            service = "application"
            adapters = "persistence"
            web = "api"
            "#,
        )
        .unwrap();
        assert_eq!(config.layer(layout::SERVICE), "application");
        assert_eq!(config.layer(layout::ADAPTERS), "persistence");
        assert_eq!(config.layer(layout::WEB), "api");
        // Untouched layers keep the default rather than becoming empty.
        assert_eq!(config.layer(layout::DOMAIN), "domain");
    }

    #[test]
    fn sections_other_than_layout_are_ignored() {
        let config = Config::parse(
            r#"
            [something-else]
            service = "nope"

            [layout]
            service = "application"
            "#,
        )
        .unwrap();
        assert_eq!(config.layer(layout::SERVICE), "application");
    }

    /// The whole reason the keys are a closed set: a near-miss spelling that
    /// parsed happily would leave files in `adapters` while `jails.toml`
    /// claimed otherwise, and nothing would ever say so.
    #[test]
    fn a_misspelled_layer_is_an_error_not_a_no_op() {
        let err = Config::parse("[layout]\nadapter = \"persistence\"\n").unwrap_err();
        assert!(err.contains("unknown layer `adapter`"), "{err}");
        assert!(
            err.contains("adapters"),
            "the message should list the real names: {err}"
        );
    }

    #[test]
    fn an_empty_value_means_the_base_package() {
        let config = Config::parse("[layout]\nservice = \"\"\n").unwrap();
        assert_eq!(config.layer(layout::SERVICE), "");
    }

    #[test]
    fn a_value_that_is_not_a_package_name_is_rejected() {
        for bad in ["My Service", "com..app", "2fast", "Application"] {
            let text = format!("[layout]\nservice = \"{bad}\"\n");
            assert!(
                Config::parse(&text).is_err(),
                "`{bad}` should not be accepted as a package name"
            );
        }
    }

    #[test]
    fn a_dotted_value_is_a_nested_subpackage() {
        let config = Config::parse("[layout]\nadapters = \"infra.jdbc\"\n").unwrap();
        assert_eq!(config.layer(layout::ADAPTERS), "infra.jdbc");
    }

    #[test]
    fn comments_and_blank_lines_are_skipped() {
        let config = Config::parse(
            "# how this project is laid out\n\n[layout]\nservice = \"application\" # not `service`\n",
        )
        .unwrap();
        assert_eq!(config.layer(layout::SERVICE), "application");
    }

    #[test]
    fn a_hash_inside_a_value_is_not_a_comment() {
        // It is a bad package name, and must be reported as one rather than
        // truncated into a good one.
        assert!(Config::parse("[layout]\nservice = \"app#1\"\n").is_err());
    }

    #[test]
    fn an_unquoted_value_is_an_error() {
        let err = Config::parse("[layout]\nservice = application\n").unwrap_err();
        assert!(err.contains("double-quoted"), "{err}");
    }

    // ---- the manifest: what the project is made of ----

    #[test]
    fn declared_capabilities_are_read_in_file_order() {
        let config =
            Config::parse("[project]\ncapabilities = [\"db\", \"json\", \"kafka\"]\n").unwrap();
        assert_eq!(config.capabilities(), ["db", "json", "kafka"]);
    }

    #[test]
    fn a_project_with_no_manifest_declares_nothing() {
        assert!(Config::default().capabilities().is_empty());
        assert!(
            Config::parse("[layout]\nservice = \"application\"\n")
                .unwrap()
                .capabilities()
                .is_empty()
        );
    }

    /// The same rule as a misspelled layer, and for the same reason: a
    /// capability jails does not know would sit in the file looking applied
    /// and never sync, which is the failure a manifest exists to remove.
    #[test]
    fn an_unknown_capability_is_an_error_naming_the_real_ones() {
        let err = Config::parse("[project]\ncapabilities = [\"postgress\"]\n").unwrap_err();
        assert!(err.contains("unknown capability `postgress`"), "{err}");
        assert!(
            err.contains("db"),
            "the message should list the real ones: {err}"
        );
    }

    #[test]
    fn an_unknown_key_in_project_is_an_error() {
        let err = Config::parse("[project]\nname = \"demo\"\n").unwrap_err();
        assert!(err.contains("unknown key `name`"), "{err}");
    }

    #[test]
    fn a_capability_list_must_be_a_list() {
        let err = Config::parse("[project]\ncapabilities = \"db\"\n").unwrap_err();
        assert!(err.contains("list of"), "{err}");
    }

    /// An alias is not a label. `postgres` is accepted on the command line
    /// because clap maps it to `Db`, but the manifest stores what `add`
    /// wrote, and two spellings of one capability would let a project list
    /// it twice.
    #[test]
    fn the_manifest_stores_labels_not_aliases() {
        assert!(is_known_capability("db"));
        assert!(!is_known_capability("postgres"));
    }

    fn manifest_dir(label: &str) -> std::path::PathBuf {
        jails_support::scratch::ScratchDir::in_temp(&format!("jails-manifest-{label}"))
            .unwrap()
            .keep()
    }

    #[test]
    fn recording_a_capability_creates_the_file_and_is_idempotent() {
        let dir = manifest_dir("record");
        record_capability(&dir, "db").unwrap();
        record_capability(&dir, "json").unwrap();
        // Twice is not two entries.
        record_capability(&dir, "db").unwrap();

        assert_eq!(Config::load(&dir).unwrap().capabilities(), ["db", "json"]);
        let text = fs::read_to_string(dir.join(FILE)).unwrap();
        assert!(text.contains("capabilities = [\"db\", \"json\"]"), "{text}");
        fs::remove_dir_all(&dir).ok();
    }

    /// This is a file people edit. Recording rewrites one line and leaves
    /// every other byte -- comments, the `[layout]` table, key order -- alone,
    /// for the same reason `pom.rs` splices rather than round-trips.
    #[test]
    fn recording_preserves_everything_else_in_the_file() {
        let dir = manifest_dir("preserve");
        let original = "# how this project is laid out\n\
                        [layout]\n\
                        adapters = \"persistence\" # not `adapters`\n\
                        \n\
                        [project]\n\
                        capabilities = [\"db\"]\n";
        fs::write(dir.join(FILE), original).unwrap();

        record_capability(&dir, "kafka").unwrap();

        let text = fs::read_to_string(dir.join(FILE)).unwrap();
        assert!(text.contains("# how this project is laid out"), "{text}");
        assert!(
            text.contains("adapters = \"persistence\" # not `adapters`"),
            "{text}"
        );
        assert!(
            text.contains("capabilities = [\"db\", \"kafka\"]"),
            "{text}"
        );
        // The layout override still parses and still applies.
        let config = Config::load(&dir).unwrap();
        assert_eq!(config.layer(layout::ADAPTERS), "persistence");
        fs::remove_dir_all(&dir).ok();
    }

    /// Left listed, the next `sync` would put back what `remove` just took
    /// out. The two have to be exact inverses.
    #[test]
    fn forgetting_a_capability_is_the_inverse_of_recording_it() {
        let dir = manifest_dir("forget");
        record_capability(&dir, "db").unwrap();
        record_capability(&dir, "kafka").unwrap();
        forget_capability(&dir, "db").unwrap();
        assert_eq!(Config::load(&dir).unwrap().capabilities(), ["kafka"]);

        // Forgetting one that was never there is not an error.
        forget_capability(&dir, "redis").unwrap();
        assert_eq!(Config::load(&dir).unwrap().capabilities(), ["kafka"]);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_missing_file_is_not_an_error() {
        let dir = jails_support::scratch::ScratchDir::in_temp("jails-config-absent")
            .unwrap()
            .keep();
        assert_eq!(Config::load(&dir).unwrap(), Config::default());
        fs::remove_dir_all(&dir).ok();
    }
}
#[test]
fn layout_entries_are_pinned_to_the_canonical_order_and_apply_renames() {
    let config = Config::parse("[layout]\nweb = \"http\"\n").unwrap();
    let entries = config.layout_entries();
    assert_eq!(entries.len(), LAYERS_IN_ORDER.len());
    assert_eq!(
        entries.iter().map(|(name, _)| *name).collect::<Vec<_>>(),
        LAYERS_IN_ORDER
            .iter()
            .map(|(name, _)| *name)
            .collect::<Vec<_>>()
    );
    assert!(entries.contains(&("web", "http".to_string())));
}
