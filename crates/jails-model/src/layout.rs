//! Where this project keeps each layer.
//!
//! `jails adopt` exists so a project jails did not write keeps its own
//! directory names, and records the renames in `jails.toml`. `CLAUDE.md` states
//! the rule that follows: *anything reporting or writing per layer must go
//! through the project's renames.* The legacy engine reads them through
//! `Config::layers()`; the compiler reads them through this.
//!
//! It is a captured fact, not a declaration. The reader owns `jails.toml`, so
//! the layout arrives through [`crate::ProjectFacts`] like every other external
//! fact and the compiler stays pure: equal snapshot in, equal packages out.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The layers a project can rename, in the order the legacy engine lists them.
///
/// A closed set, because a `jails.toml` saying `adapter = "persistence"` that
/// silently kept writing to `adapters` would be worse than no file at all --
/// which is the rule `jails-project`'s own parser already applies to this same
/// table. `layer_names_match_the_legacy_engine` in the root test suite holds
/// this list against `Layer::ALL`, since the two crates cannot see each other.
pub const RENAMEABLE_LAYERS: [&str; 11] = [
    "domain",
    "app",
    "service",
    "web",
    "api",
    "messaging",
    "cli",
    "clients",
    "jobs",
    "adapters",
    "testkit",
];

/// A project's layer renames, empty when it has none.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct Layout {
    renames: BTreeMap<String, String>,
}

impl Layout {
    /// The `[layout]` table of a `jails.toml`.
    ///
    /// Hand-parsed for the reason the rest of jails hand-parses this file, and
    /// **an unrecognised key is an error**: this is a file people edit, and a
    /// typo that reads as "no rename" produces a tree the reader did not ask
    /// for with nothing to say why.
    pub fn parse(source: &str) -> Result<Self, String> {
        let mut renames = BTreeMap::new();
        let mut in_layout = false;
        for line in source.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(table) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                in_layout = table.trim() == "layout";
                continue;
            }
            if !in_layout {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            let value = value.trim().trim_matches('"');
            if !RENAMEABLE_LAYERS.contains(&key) {
                return Err(format!(
                    "jails.toml [layout] has no layer `{key}`.\n       \
                     fix: use one of {}.",
                    RENAMEABLE_LAYERS.join(", ")
                ));
            }
            renames.insert(key.to_string(), value.to_string());
        }
        Ok(Self { renames })
    }

    /// This project's package segment for `layer`, which is `layer` itself
    /// unless it was renamed.
    ///
    /// Takes the segment rather than a whole package so a nested one --
    /// `adapters.jdbc`, `ports.http` -- renames its head and keeps its tail:
    /// a reader who called their adapters `persistence` means
    /// `persistence.jdbc`, not that the JDBC adapter has moved.
    pub fn segment<'a>(&'a self, layer: &'a str) -> &'a str {
        self.renames.get(layer).map(String::as_str).unwrap_or(layer)
    }

    /// Whether this project renamed nothing, so its packages are the defaults.
    pub fn is_default(&self) -> bool {
        self.renames.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_project_with_no_file_keeps_every_default_name() {
        let layout = Layout::default();
        assert!(layout.is_default());
        for layer in RENAMEABLE_LAYERS {
            assert_eq!(layout.segment(layer), layer);
        }
    }

    #[test]
    fn a_rename_applies_to_its_layer_and_to_nothing_else() {
        let layout = Layout::parse(
            "# a comment\n[layout]\nadapters = \"persistence\"\n\n[project]\nadapters = \"ignored\"\n",
        )
        .unwrap();
        assert_eq!(layout.segment("adapters"), "persistence");
        assert_eq!(layout.segment("web"), "web");
        assert!(!layout.is_default());
    }

    /// The rule `jails-project`'s parser already applies to this table: an
    /// unknown key is an error, because silently meaning "no rename" produces
    /// a tree nobody asked for.
    #[test]
    fn an_unknown_layer_is_refused_by_name() {
        let error = Layout::parse("[layout]\nadapter = \"persistence\"\n").unwrap_err();
        assert!(error.contains("no layer `adapter`"), "{error}");
        assert!(error.contains("fix:"), "{error}");
    }

    /// A `[layout]` table jails wrote is not the only thing in the file, and a
    /// key outside it belongs to somebody else.
    #[test]
    fn keys_outside_the_layout_table_are_not_renames() {
        let layout =
            Layout::parse("[project]\ncapabilities = [\"db\"]\n[layout]\nweb = \"http\"\n")
                .unwrap();
        assert_eq!(layout.segment("web"), "http");
        assert!(layout.segment("capabilities").eq("capabilities"));
    }
}
