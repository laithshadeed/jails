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
//! `[[capability]]` is the third table and the same half as the second: a
//! capability the caller gave a `--name` or a `--package` cannot be written as
//! one string, because `add csv --name Order` and `add csv --name Invoice` are
//! two capabilities and `["csv", "csv"]` says nothing about which. plan.md
//! §R1.1 fixes the shape; [`crate::capability::Declaration`] is the value, and
//! which of the two shapes a declaration lands in is its own decision rather
//! than the caller's.
//!
//! Still deliberately not a general config file -- no template overrides, no
//! plugin hooks, no per-kind paths. All three tables are **closed sets**: the
//! layout keys are exactly the constants in [`crate::spec::layout`], the
//! capability names are derived from the `Capability` enum rather than
//! restated, and a `[[capability]]` table's keys are exactly `kind`, `name`
//! and `package`. A name that is not one of them is a typo and is reported as
//! such rather than ignored -- silently accepting `adapter = "persistence"`
//! (singular) would put files in `adapters` forever while the file claims
//! otherwise, silently accepting `postgress` would leave a capability that
//! looks declared and never syncs, and silently accepting `nmae = "Order"`
//! would leave one that syncs under the wrong name.
//!
//! ## Why this is hand-parsed
//!
//! jails has two dependencies, both clap. The grammar needed here is one
//! table of `key = "value"` pairs, which is about forty lines to read
//! directly and does not justify pulling in a TOML parser plus its error
//! types. The cost is that this understands a *subset* of TOML: `[layout]`,
//! `[project]` and repeated `[[capability]]` tables, bare keys, double-quoted
//! values, single-line string arrays, `#` comments. Anything else in the file is ignored, and anything
//! malformed *inside* a table it knows is an error -- quietly skipping a line
//! the user clearly meant is the failure mode this whole module exists to
//! avoid.
//!
//! Writing back is a targeted splice, not a round trip: `record_capability`
//! rewrites the one `capabilities = [...]` line and leaves every other byte
//! alone, for the same reason `pom.rs` does. This is a file people edit.

use jails_support::Result;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::capability::Declaration;
use crate::spec::kind::Capability;
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
pub(crate) const LAYERS_IN_ORDER: &[(&str, &str)] = &[
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
/// The repeated table a parameterised capability uses, per plan.md §R1.1.
/// `[project] capabilities` keeps the conventional singleton and default
/// instances; anything carrying a `--name` or a `--package` needs somewhere to
/// put it, and a string array has nowhere.
const CAPABILITY_TABLE: &str = "capability";
/// Its closed key set. Unknown key is an error for the same reason an unknown
/// layer is: a `nmae = "Order"` that parsed to nothing would leave a project
/// whose manifest claims a capability jails never installed.
const CAPABILITY_KEYS: [&str; 3] = ["kind", "name", "package"];

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
    ///
    /// Derived from `declarations` at parse time rather than collected beside
    /// it: two named instances of one capability are two declarations and one
    /// label, and a second traversal is how those two counts come to disagree.
    capabilities: Vec<String>,
    /// Every capability this file declares, with the parameters it declared
    /// them with, in file order.
    declarations: Vec<Declaration>,
    /// Just the labels in `[project] capabilities`, which is the only list the
    /// array splice may rewrite. Rendering the derived list back into the
    /// array would copy every `[[capability]]` table into it as a *default*
    /// instance -- a capability the reader never asked for, beside the one
    /// they did.
    array: Vec<String>,
}

impl Config {
    /// Read `jails.toml` from a project root. Absent file -> defaults.
    pub fn load(root: &Path) -> Result<Self> {
        let path = root.join(FILE);
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(format!("failed to read {}: {e}", path.display()).into()),
        };
        Ok(Self::parse(&text).map_err(|e| format!("{FILE}: {e}"))?)
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

    /// Every capability this file declares, with its parameters, in file
    /// order. The identity-bearing view; [`Self::capabilities`] is the label
    /// view of the same list.
    pub fn declarations(&self) -> &[Declaration] {
        &self.declarations
    }

    pub fn parse(text: &str) -> Result<Self> {
        let mut layout = HashMap::new();
        let mut array: Vec<String> = Vec::new();
        let mut declarations: Vec<(Declaration, usize)> = Vec::new();
        let mut table = String::new();
        let mut pending: Option<PendingCapability> = None;

        for (i, raw) in text.lines().enumerate() {
            let line = strip_comment(raw).trim();
            if line.is_empty() {
                continue;
            }
            let lineno = i + 1;

            // `[[capability]]` before `[capability]`: stripping one bracket
            // from each end of a repeated table header leaves `[capability]`,
            // which would be read as an ordinary table and its keys silently
            // ignored.
            if let Some(name) = line.strip_prefix("[[").and_then(|l| l.strip_suffix("]]")) {
                finish_capability(&mut pending, &mut declarations)?;
                let name = name.trim();
                if name != CAPABILITY_TABLE {
                    return Err(format!(
                        "line {lineno}: unknown repeated table `[[{name}]]`. The only one is \
                         `[[{CAPABILITY_TABLE}]]`."
                    )
                    .into());
                }
                table = CAPABILITY_TABLE.to_string();
                pending = Some(PendingCapability::at(lineno));
                continue;
            }
            if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                finish_capability(&mut pending, &mut declarations)?;
                table = name.trim().to_string();
                continue;
            }

            if let Some(entry) = pending.as_mut() {
                entry.key(line, lineno)?;
                continue;
            }

            if table == PROJECT_TABLE {
                let (key, value) = line.split_once('=').ok_or_else(|| {
                    format!("line {lineno}: expected `key = value`, found `{line}`")
                })?;
                let key = key.trim();
                if key != CAPABILITIES_KEY {
                    return Err(format!(
                        "line {lineno}: unknown key `{key}` in [{PROJECT_TABLE}]. \
                         The only key is `{CAPABILITIES_KEY}`."
                    )
                    .into());
                }
                for label in parse_string_array(value.trim()).ok_or_else(|| {
                    format!(
                        "line {lineno}: `{CAPABILITIES_KEY}` must be a list of \
                             double-quoted names, e.g. \
                             `{CAPABILITIES_KEY} = [\"db\", \"json\"]`"
                    )
                })? {
                    declarations.push((
                        Declaration::plain(capability_named(&label, lineno)?),
                        lineno,
                    ));
                    array.push(label);
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
                )
                .into());
            }
            if !is_package_path(value) {
                return Err(
                    format!("line {lineno}: `{key} = \"{value}\"` is not a package name").into(),
                );
            }
            layout.insert(key.to_string(), value.to_string());
        }
        finish_capability(&mut pending, &mut declarations)?;

        let mut capabilities: Vec<String> = Vec::new();
        for (at, (declaration, lineno)) in declarations.iter().enumerate() {
            declaration
                .validate()
                .map_err(|e| format!("line {lineno}: {e}"))?;
            // A repeat is an error, never a silent dedup. Both spellings reach
            // the same identity, so one of them describes a capability the
            // reader believes is declared twice -- and `sync` acts on this
            // file. Compared by position, not by line: an array lists several
            // on one line.
            if let Some((_, first)) = declarations[..at]
                .iter()
                .find(|(other, _)| other == declaration)
            {
                return Err(format!(
                    "line {lineno}: `{}` is already declared on line {first}.",
                    declaration.display()
                )
                .into());
            }
            let label = declaration.kind.label().to_string();
            if !capabilities.contains(&label) {
                capabilities.push(label);
            }
        }

        Ok(Self {
            layout,
            capabilities,
            declarations: declarations.into_iter().map(|(d, _)| d).collect(),
            array,
        })
    }
}

/// One `[[capability]]` table part-way through being read.
///
/// Kept separate from [`Declaration`] because a table is only a declaration
/// once its `kind` has arrived, and the line number of the header is what a
/// missing-`kind` error has to name.
struct PendingCapability {
    lineno: usize,
    kind: Option<String>,
    name: Option<String>,
    package: Option<String>,
}

impl PendingCapability {
    fn at(lineno: usize) -> Self {
        Self {
            lineno,
            kind: None,
            name: None,
            package: None,
        }
    }

    fn key(&mut self, line: &str, lineno: usize) -> Result<()> {
        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| format!("line {lineno}: expected `key = \"value\"`, found `{line}`"))?;
        let key = key.trim();
        let value = unquote(value.trim())
            .ok_or_else(|| format!("line {lineno}: `{key}` must be a double-quoted string"))?;
        let slot = match key {
            "kind" => &mut self.kind,
            "name" => &mut self.name,
            "package" => &mut self.package,
            _ => {
                return Err(format!(
                    "line {lineno}: unknown key `{key}` in [[{CAPABILITY_TABLE}]]. Known: {}",
                    CAPABILITY_KEYS.join(", ")
                )
                .into());
            }
        };
        if slot.is_some() {
            return Err(format!(
                "line {lineno}: `{key}` is set twice in one [[{CAPABILITY_TABLE}]] table."
            )
            .into());
        }
        *slot = Some(value.to_string());
        Ok(())
    }
}

/// Close the table that was open, if one was.
fn finish_capability(
    pending: &mut Option<PendingCapability>,
    into: &mut Vec<(Declaration, usize)>,
) -> Result<()> {
    let Some(entry) = pending.take() else {
        return Ok(());
    };
    let lineno = entry.lineno;
    let kind = entry.kind.ok_or_else(|| {
        format!("line {lineno}: [[{CAPABILITY_TABLE}]] has no `kind`. Every table needs one.")
    })?;
    into.push((
        Declaration {
            kind: capability_named(&kind, lineno)?,
            name: entry.name,
            package: entry.package,
        },
        lineno,
    ));
    Ok(())
}

/// Resolve a label to the capability it names, or say which ones exist.
fn capability_named(label: &str, lineno: usize) -> Result<Capability> {
    use clap::ValueEnum;
    Ok(Capability::value_variants()
        .iter()
        .copied()
        .find(|candidate| candidate.label() == label)
        .ok_or_else(|| {
            format!(
                "line {lineno}: unknown capability `{label}`. Known: {}",
                known_capabilities().join(", ")
            )
        })?)
}

/// The same edit as text, for a caller holding the bytes rather than a root.
///
/// `None` means the change was already true. This is the splice; the two
/// root-taking functions above are the file half of it, so a projection and a
/// write cannot disagree about what the file becomes.
pub(crate) fn edited_capabilities(
    text: &str,
    change: impl FnOnce(&mut Vec<String>) -> bool,
) -> Result<Option<String>> {
    let mut labels = Config::parse(text)
        .map_err(|e| format!("{FILE}: {e}"))?
        .array;
    if !change(&mut labels) {
        return Ok(None);
    }
    let rendered = format!("{CAPABILITIES_KEY} = {}", render_string_array(&labels));
    Ok(Some(match replace_capabilities_line(text, &rendered) {
        Some(updated) => updated,
        None => append_project_table(text, &rendered),
    }))
}

/// Declare one capability in this text, or `None` when it already is.
///
/// Which of the file's two shapes it lands in is the declaration's to decide,
/// not the caller's: bare goes in `[project] capabilities`, parameterised gets
/// a `[[capability]]` table. That is why this takes a [`Declaration`] rather
/// than a label -- a label cannot say which.
pub(crate) fn with_capability(text: &str, declaration: &Declaration) -> Result<Option<String>> {
    declaration.validate().map_err(|e| format!("{FILE}: {e}"))?;
    let config = Config::parse(text).map_err(|e| format!("{FILE}: {e}"))?;
    if config.declarations.contains(declaration) {
        return Ok(None);
    }
    if declaration.is_plain() {
        let label = declaration.kind.label();
        return edited_capabilities(text, |labels| {
            if labels.iter().any(|l| l == label) {
                return false;
            }
            labels.push(label.to_string());
            true
        });
    }
    Ok(Some(insert_capability_table(text, declaration)?))
}

/// Take one capability declaration back out.
///
/// The exact inverse: a bare one leaves the array, a parameterised one takes
/// its whole table with it. A declaration the file does not make is `None`
/// rather than an error, because `remove` is allowed to be run twice.
pub(crate) fn without_capability(text: &str, declaration: &Declaration) -> Result<Option<String>> {
    let config = Config::parse(text).map_err(|e| format!("{FILE}: {e}"))?;
    if !config.declarations.contains(declaration) {
        return Ok(None);
    }
    if declaration.is_plain() {
        let label = declaration.kind.label();
        if config.array.iter().any(|l| l == label) {
            return edited_capabilities(text, |labels| {
                let before = labels.len();
                labels.retain(|l| l != label);
                labels.len() != before
            });
        }
    }
    remove_capability_table(text, declaration)
}

/// Every `[[capability]]` table in the text, as `(first line, one past last,
/// what it declares)`.
///
/// The end excludes trailing blank lines, so removing a block does not take
/// the separator before the next table with it.
fn capability_blocks(text: &str) -> Result<Vec<(usize, usize, Declaration)>> {
    let lines: Vec<&str> = text.lines().collect();
    let mut blocks = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let header = strip_comment(lines[i]).trim();
        if header != format!("[[{CAPABILITY_TABLE}]]") {
            i += 1;
            continue;
        }
        let start = i;
        let mut entry = PendingCapability::at(start + 1);
        let mut end = i + 1;
        while end < lines.len() {
            let line = strip_comment(lines[end]).trim();
            if line.starts_with('[') {
                break;
            }
            if !line.is_empty() {
                entry.key(line, end + 1)?;
            }
            end += 1;
        }
        i = end;
        while end > start + 1 && strip_comment(lines[end - 1]).trim().is_empty() {
            end -= 1;
        }
        let mut collected = Vec::new();
        finish_capability(&mut Some(entry), &mut collected)?;
        if let Some((declaration, _)) = collected.pop() {
            blocks.push((start, end, declaration));
        }
    }
    Ok(blocks)
}

/// One table, in the shape jails writes it.
fn render_capability_table(declaration: &Declaration) -> String {
    let mut out = format!(
        "[[{CAPABILITY_TABLE}]]\nkind = \"{}\"\n",
        declaration.kind.label()
    );
    if let Some(name) = &declaration.name {
        out.push_str(&format!("name = \"{name}\"\n"));
    }
    if let Some(package) = &declaration.package {
        out.push_str(&format!("package = \"{package}\"\n"));
    }
    out
}

/// Splice a new table in, ordered by canonical identity among the tables
/// already there.
///
/// Ordering the *insert* rather than rewriting the file is the same rule the
/// rest of this module follows: two projects that declared the same set end up
/// with the same file, and a table somebody wrote by hand keeps its formatting
/// and its place.
fn insert_capability_table(text: &str, declaration: &Declaration) -> Result<String> {
    let blocks = capability_blocks(text)?;
    let rendered = render_capability_table(declaration);
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let at = blocks
        .iter()
        .find(|(_, _, other)| other.sort_key() > declaration.sort_key())
        .map(|(start, _, _)| *start)
        .or_else(|| blocks.last().map(|(_, end, _)| *end));
    let block: Vec<String> = rendered.lines().map(str::to_string).collect();
    match at {
        Some(at) => {
            let mut spliced = block;
            spliced.push(String::new());
            for (offset, line) in spliced.into_iter().enumerate() {
                lines.insert(at + offset, line);
            }
        }
        None => {
            if !lines.is_empty() {
                lines.push(String::new());
            }
            lines.extend(block);
        }
    }
    let mut joined = lines.join("\n");
    joined.push('\n');
    Ok(joined)
}

/// Take one whole table out, and the blank line that separated it.
fn remove_capability_table(text: &str, declaration: &Declaration) -> Result<Option<String>> {
    let blocks = capability_blocks(text)?;
    let Some((start, end, _)) = blocks
        .into_iter()
        .find(|(_, _, other)| other == declaration)
    else {
        return Ok(None);
    };
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let mut end = end;
    while end < lines.len() && lines[end].trim().is_empty() {
        end += 1;
    }
    let mut start = start;
    if end >= lines.len() {
        while start > 0 && lines[start - 1].trim().is_empty() {
            start -= 1;
        }
    }
    lines.drain(start..end);
    let mut joined = lines.join("\n");
    if !joined.is_empty() {
        joined.push('\n');
    }
    Ok(Some(joined))
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

/// The same layout edit as text. See [`edited_capabilities`] for why the
/// splice and the write are separate.
pub(crate) fn with_layout(text: &str, layer: &str, directory: &str) -> Result<String> {
    if !is_layer(layer) {
        return Err(format!(
            "`{layer}` is not a layer. Known layers: {}",
            layer_names().join(", ")
        )
        .into());
    }
    let rendered = format!("{layer} = \"{directory}\"");
    Ok(match replace_layout_line(text, layer, &rendered) {
        Some(updated) => updated,
        None => insert_into_layout_table(text, &rendered),
    })
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
mod capability_table_tests {
    use super::*;

    fn declarations(text: &str) -> Vec<Declaration> {
        Config::parse(text).unwrap().declarations().to_vec()
    }

    /// The shape plan.md §R1.1 prints, read back with both parameters.
    #[test]
    fn a_repeated_table_declares_a_parameterised_capability() {
        let text = "[project]\ncapabilities = [\"db\"]\n\n\
                    [[capability]]\nkind = \"csv\"\nname = \"Dataset\"\n\
                    package = \"imports\"\n";
        assert_eq!(
            declarations(text),
            vec![
                Declaration::plain(Capability::Db),
                Declaration {
                    kind: Capability::Csv,
                    name: Some("Dataset".to_string()),
                    package: Some("imports".to_string()),
                },
            ]
        );
        // Both views of one list: two declarations of `csv` would still be one
        // label, and `db` is in both.
        assert_eq!(Config::parse(text).unwrap().capabilities(), ["db", "csv"]);
    }

    /// The failure this module exists to prevent, in the new grammar: a
    /// `[[capability]]` whose keys fell through to the ignored-table branch
    /// would declare a capability jails then never installs.
    #[test]
    fn a_table_is_not_read_as_an_ordinary_one_of_the_same_name() {
        let text = "[[capability]]\nkind = \"json\"\nname = \"Order\"\n";
        assert_eq!(declarations(text).len(), 1);
        assert_eq!(declarations(text)[0].name.as_deref(), Some("Order"));
    }

    #[test]
    fn an_unknown_key_in_a_table_is_reported_not_ignored() {
        let text = "[[capability]]\nkind = \"csv\"\nnmae = \"Order\"\n";
        let error = Config::parse(text).unwrap_err();
        assert!(error.contains("unknown key `nmae`"), "{error}");
        assert!(error.contains("kind, name, package"), "{error}");
    }

    #[test]
    fn a_table_with_no_kind_is_an_error_naming_its_line() {
        let error = Config::parse("[[capability]]\nname = \"Order\"\n").unwrap_err();
        assert!(error.contains("line 1"), "{error}");
        assert!(error.contains("has no `kind`"), "{error}");
    }

    #[test]
    fn a_key_set_twice_in_one_table_is_an_error() {
        let text = "[[capability]]\nkind = \"csv\"\nname = \"A\"\nname = \"B\"\n";
        let error = Config::parse(text).unwrap_err();
        assert!(error.contains("set twice"), "{error}");
    }

    #[test]
    fn an_unknown_repeated_table_is_an_error() {
        let error = Config::parse("[[plugin]]\nkind = \"csv\"\n").unwrap_err();
        assert!(
            error.contains("unknown repeated table `[[plugin]]`"),
            "{error}"
        );
    }

    /// The manifest and the CLI enforce one rule, so a project cannot declare
    /// what `jails add` would refuse.
    #[test]
    fn a_parameter_the_capability_has_no_meaning_for_is_refused() {
        let named = Config::parse("[[capability]]\nkind = \"db\"\nname = \"Main\"\n").unwrap_err();
        assert!(named.contains("--name"), "{named}");
        let placed =
            Config::parse("[[capability]]\nkind = \"ci\"\npackage = \"ops\"\n").unwrap_err();
        assert!(placed.contains("--package"), "{placed}");
        // A singleton that *is* placed keeps accepting one.
        assert!(Config::parse("[[capability]]\nkind = \"actuator\"\npackage = \"ops\"\n").is_ok());
    }

    #[test]
    fn declaring_the_same_capability_twice_is_an_error_not_a_silent_dedup() {
        let error = Config::parse("[project]\ncapabilities = [\"db\", \"db\"]\n").unwrap_err();
        assert!(error.contains("already declared"), "{error}");
        let across = Config::parse(
            "[project]\ncapabilities = [\"csv\"]\n\n[[capability]]\nkind = \"csv\"\n",
        )
        .unwrap_err();
        assert!(across.contains("already declared"), "{across}");
    }

    /// Two named instances are two capabilities, which is the whole reason
    /// this table exists.
    #[test]
    fn two_named_instances_of_one_capability_are_both_declared() {
        let text = "[[capability]]\nkind = \"csv\"\nname = \"Order\"\n\n\
                    [[capability]]\nkind = \"csv\"\nname = \"Invoice\"\n";
        assert_eq!(declarations(text).len(), 2);
        assert_eq!(Config::parse(text).unwrap().capabilities(), ["csv"]);
    }

    #[test]
    fn a_bare_declaration_lands_in_the_array_and_a_parameterised_one_in_a_table() {
        let plain = with_capability("", &Declaration::plain(Capability::Db))
            .unwrap()
            .unwrap();
        assert!(plain.contains("capabilities = [\"db\"]"), "{plain}");
        assert!(!plain.contains("[[capability]]"), "{plain}");

        let named = with_capability(
            &plain,
            &Declaration {
                kind: Capability::Csv,
                name: Some("Order".to_string()),
                package: None,
            },
        )
        .unwrap()
        .unwrap();
        assert!(
            named.contains("[[capability]]\nkind = \"csv\"\nname = \"Order\""),
            "{named}"
        );
        assert!(named.contains("capabilities = [\"db\"]"), "{named}");
        assert_eq!(declarations(&named).len(), 2);
    }

    /// Declaring it again is a no-op, so `add` twice does not grow the file.
    #[test]
    fn a_declaration_already_made_changes_nothing() {
        let declaration = Declaration {
            kind: Capability::Csv,
            name: Some("Order".to_string()),
            package: None,
        };
        let once = with_capability("", &declaration).unwrap().unwrap();
        assert_eq!(with_capability(&once, &declaration).unwrap(), None);
    }

    /// Ordered on insert, never by rewriting what is already there.
    #[test]
    fn a_new_table_is_placed_in_canonical_order() {
        let mut text = String::new();
        for name in ["Order", "Alpha", "Zulu"] {
            text = with_capability(
                &text,
                &Declaration {
                    kind: Capability::Csv,
                    name: Some(name.to_string()),
                    package: None,
                },
            )
            .unwrap()
            .unwrap();
        }
        let names: Vec<Option<String>> = declarations(&text).into_iter().map(|d| d.name).collect();
        assert_eq!(
            names,
            vec![
                Some("Alpha".to_string()),
                Some("Order".to_string()),
                Some("Zulu".to_string())
            ]
        );
    }

    #[test]
    fn removing_one_table_leaves_the_others_and_the_array_alone() {
        let order = Declaration {
            kind: Capability::Csv,
            name: Some("Order".to_string()),
            package: None,
        };
        let invoice = Declaration {
            kind: Capability::Csv,
            name: Some("Invoice".to_string()),
            package: None,
        };
        let text = with_capability("", &Declaration::plain(Capability::Db))
            .unwrap()
            .unwrap();
        let text = with_capability(&text, &order).unwrap().unwrap();
        let text = with_capability(&text, &invoice).unwrap().unwrap();

        let without = without_capability(&text, &order).unwrap().unwrap();
        assert_eq!(
            declarations(&without),
            vec![Declaration::plain(Capability::Db), invoice]
        );
        assert!(without.contains("capabilities = [\"db\"]"), "{without}");
        // Removing what is not declared is not an error, and writes nothing.
        assert_eq!(without_capability(&without, &order).unwrap(), None);
    }

    /// The inverse is exact: add then remove is where the file started.
    #[test]
    fn a_table_added_and_taken_back_out_leaves_the_bytes_it_found() {
        let before = "# hand written\n[project]\ncapabilities = [\"db\"]\n\n\
                      [layout]\nadapters = \"persistence\"\n";
        let declaration = Declaration {
            kind: Capability::Actuator,
            name: None,
            package: Some("ops".to_string()),
        };
        let with = with_capability(before, &declaration).unwrap().unwrap();
        assert!(with.contains("[[capability]]"), "{with}");
        assert_eq!(
            without_capability(&with, &declaration).unwrap().unwrap(),
            before
        );
    }

    /// The array splice must not copy a table's capability into the array as a
    /// default instance the reader never asked for.
    #[test]
    fn writing_the_array_leaves_table_capabilities_out_of_it() {
        let text = "[[capability]]\nkind = \"csv\"\nname = \"Order\"\n";
        let updated = with_capability(text, &Declaration::plain(Capability::Db))
            .unwrap()
            .unwrap();
        assert!(updated.contains("capabilities = [\"db\"]"), "{updated}");
        assert_eq!(declarations(&updated).len(), 2);
    }
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
        assert!(capability_named("db", 1).is_ok());
        assert!(capability_named("postgres", 1).is_err());
    }

    /// The capability edit, reached the way the V2 projection reaches it.
    ///
    /// These tests were written against `record_capability(root, label)` and
    /// `forget_capability(root, label)`, a pair of V1 entry points that read
    /// the file, spliced it and wrote it back. Nothing called them once the
    /// projection started splicing text it already holds, but `pub` kept
    /// `dead_code` from saying so. The splice they wrapped is still the
    /// shipped one, so the tests keep it -- through the text-in/text-out half,
    /// which is what the projection calls.
    fn record_capability(root: &std::path::Path, label: &str) -> Result<()> {
        edit_manifest(root, |labels| {
            if labels.iter().any(|l| l == label) {
                return false;
            }
            labels.push(label.to_string());
            true
        })
    }

    fn forget_capability(root: &std::path::Path, label: &str) -> Result<()> {
        edit_manifest(root, |labels| {
            let before = labels.len();
            labels.retain(|l| l != label);
            labels.len() != before
        })
    }

    fn edit_manifest(
        root: &std::path::Path,
        change: impl FnOnce(&mut Vec<String>) -> bool,
    ) -> Result<()> {
        let path = root.join(FILE);
        let text = fs::read_to_string(&path).unwrap_or_default();
        if let Some(updated) = edited_capabilities(&text, change)? {
            fs::write(&path, updated).map_err(|e| e.to_string())?;
        }
        Ok(())
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
