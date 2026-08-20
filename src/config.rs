//! `jails.toml` -- the one thing a project gets to say about where generated
//! code lands.
//!
//! jails ships a layer layout (`domain`, `service`, `web`, `app`, `adapters`)
//! and every generator writes into it. That is a fine default and a bad
//! mandate: a project whose spec says `domain`/`application`/`persistence`/
//! `api` has to pass `--package` to *every* call, and one forgotten flag puts
//! a file in a package the project does not otherwise use. `--package` is a
//! per-call override; this is the per-project one.
//!
//! Deliberately not a general config file. It renames layers and nothing
//! else -- no template overrides, no plugin hooks, no per-kind paths. The
//! keys are exactly the constants in [`crate::generate::layout`], so a name
//! that is not one of them is a typo and is reported as such rather than
//! ignored. Silently accepting `adapter = "persistence"` (singular) would
//! put files in `adapters` forever while the file claims otherwise.
//!
//! ## Why this is hand-parsed
//!
//! jails has two dependencies, both clap. The grammar needed here is one
//! table of `key = "value"` pairs, which is about forty lines to read
//! directly and does not justify pulling in a TOML parser plus its error
//! types. The cost is that this understands a *subset* of TOML: `[layout]`,
//! bare keys, double-quoted values, `#` comments. Anything else in the file
//! is ignored, and anything malformed *inside* `[layout]` is an error --
//! quietly skipping a line the user clearly meant is the failure mode this
//! whole module exists to avoid.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::generate::layout;

/// The file, at the project root next to `pom.xml`.
pub(crate) const FILE: &str = "jails.toml";

/// Every layer name a project may rename, and the default it replaces.
///
/// Kept as a list rather than derived from the `layout` module so that adding
/// a constant there without deciding whether it is configurable is a
/// compile-time-visible omission rather than a silent one.
const LAYERS: &[&str] = &[
    layout::DOMAIN,
    layout::APP,
    layout::SERVICE,
    layout::WEB,
    layout::CLI,
    layout::ADAPTERS,
    layout::API,
    layout::TESTKIT,
    layout::CLIENTS,
    layout::JOBS,
    layout::MESSAGING,
];

/// A project's layout overrides: default layer name -> the name to use.
///
/// An absent file is an empty map, not an error -- the overwhelming majority
/// of projects never have one, and `Config::default()` behaving exactly like
/// today's hardcoded layout is what keeps this change from touching them.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Config {
    layout: HashMap<String, String>,
}

impl Config {
    /// Read `jails.toml` from a project root. Absent file -> defaults.
    pub(crate) fn load(root: &Path) -> Result<Self, String> {
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
    pub(crate) fn layer<'a>(&'a self, default: &'a str) -> &'a str {
        self.layout
            .get(default)
            .map(String::as_str)
            .unwrap_or(default)
    }

    fn parse(text: &str) -> Result<Self, String> {
        let mut layout = HashMap::new();
        let mut in_layout = false;

        for (i, raw) in text.lines().enumerate() {
            let line = strip_comment(raw).trim();
            if line.is_empty() {
                continue;
            }
            if let Some(table) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                in_layout = table.trim() == "layout";
                continue;
            }
            if !in_layout {
                continue;
            }

            let lineno = i + 1;
            let (key, value) = line.split_once('=').ok_or_else(|| {
                format!("line {lineno}: expected `key = \"value\"`, found `{line}`")
            })?;
            let key = key.trim();
            let value = unquote(value.trim())
                .ok_or_else(|| format!("line {lineno}: `{key}` must be a double-quoted string"))?;

            if !LAYERS.contains(&key) {
                return Err(format!(
                    "line {lineno}: unknown layer `{key}`. Known layers: {}",
                    LAYERS.join(", ")
                ));
            }
            if !is_package_path(value) {
                return Err(format!(
                    "line {lineno}: `{key} = \"{value}\"` is not a package name"
                ));
            }
            layout.insert(key.to_string(), value.to_string());
        }

        Ok(Self { layout })
    }
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
        assert!(err.contains("adapters"), "the message should list the real names: {err}");
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

    #[test]
    fn a_missing_file_is_not_an_error() {
        let dir = std::env::temp_dir().join(format!("jails-config-absent-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        assert_eq!(Config::load(&dir).unwrap(), Config::default());
        fs::remove_dir_all(&dir).ok();
    }
}
