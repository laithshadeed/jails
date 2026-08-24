//! Which directory name means which layer, and what to do when the answer is
//! not one.
//!
//! `plan.md` §12's classification, at the layer that owns `jails.toml` and
//! `LAYERS_IN_ORDER` rather than at the binary, because two callers need it:
//! `jails adopt` and the transaction route that records the same `[layout]`
//! table as one commit. What stays in the command is the printing.
//!
//! ## Three rules, and each is the point
//!
//! **A directory matching nothing is reported, not guessed.** A wrong
//! `[layout]` entry is worse than a missing one: jails would confidently write
//! into the wrong package and `destroy` would look in a third place. So the
//! synonym table is closed, and what it does not recognise is handed back for
//! the reader to decide about.
//!
//! **It never touches `[project] capabilities`.** That list is what `jails
//! sync` applies, and inferring it from a directory listing would have `sync`
//! install things nobody asked for into a project jails did not create -- the
//! single most destructive thing this could do. Nothing here can: the only
//! thing it produces is `(layer, directory)` pairs.
//!
//! **One layer, one directory.** If two directories both look like the web
//! layer, neither is written and both are reported: a `[layout]` table can
//! only say one thing, and picking the first alphabetically would be a coin
//! toss the reader never saw.

use crate::spec::layout;
use std::collections::BTreeMap;

/// Directory name -> the layer it means. **Closed**, for the reason above.
///
/// Every entry earns its place by being a name a real Java project uses, not by
/// being a plausible synonym: `persistence`, `infrastructure` and `dao` are
/// what repositories live in; `controllers`, `rest`, `http` and `api` are what
/// serves them; `usecase`/`usecases`/`application` is the port layer under
/// Clean Architecture's own naming. A name nobody uses costs nothing to leave
/// out and something real to get wrong.
const SYNONYMS: &[(&str, &str)] = &[
    (layout::DOMAIN, layout::DOMAIN),
    ("model", layout::DOMAIN),
    ("models", layout::DOMAIN),
    ("entity", layout::DOMAIN),
    ("entities", layout::DOMAIN),
    (layout::APP, layout::APP),
    ("application", layout::APP),
    ("usecase", layout::APP),
    ("usecases", layout::APP),
    ("port", layout::APP),
    ("ports", layout::APP),
    (layout::SERVICE, layout::SERVICE),
    ("services", layout::SERVICE),
    (layout::WEB, layout::WEB),
    ("controller", layout::WEB),
    ("controllers", layout::WEB),
    ("rest", layout::WEB),
    ("resource", layout::WEB),
    ("resources", layout::WEB),
    (layout::API, layout::API),
    ("dto", layout::API),
    ("dtos", layout::API),
    ("contract", layout::API),
    ("contracts", layout::API),
    (layout::MESSAGING, layout::MESSAGING),
    ("event", layout::MESSAGING),
    ("events", layout::MESSAGING),
    ("kafka", layout::MESSAGING),
    (layout::CLI, layout::CLI),
    ("command", layout::CLI),
    ("commands", layout::CLI),
    (layout::CLIENTS, layout::CLIENTS),
    ("client", layout::CLIENTS),
    ("gateway", layout::CLIENTS),
    ("gateways", layout::CLIENTS),
    (layout::JOBS, layout::JOBS),
    ("job", layout::JOBS),
    ("scheduler", layout::JOBS),
    ("schedulers", layout::JOBS),
    (layout::ADAPTERS, layout::ADAPTERS),
    ("adapter", layout::ADAPTERS),
    ("persistence", layout::ADAPTERS),
    ("repository", layout::ADAPTERS),
    ("repositories", layout::ADAPTERS),
    ("dao", layout::ADAPTERS),
    ("infrastructure", layout::ADAPTERS),
    ("infra", layout::ADAPTERS),
    (layout::TESTKIT, layout::TESTKIT),
    ("testsupport", layout::TESTKIT),
    ("fixtures", layout::TESTKIT),
];

/// What one directory under the base package turned out to be.
#[derive(Debug, PartialEq, Eq)]
pub enum Reading {
    /// Recognised, and already spelled the way jails spells it.
    Conventional(&'static str),
    /// Recognised under another name: this is a `[layout]` entry.
    Renamed { layer: &'static str, dir: String },
    /// Not in the table. Reported, never guessed at.
    Unknown(String),
}

/// Classify directory names under the base package.
///
/// Takes the names rather than reading them, so the caller decides where a
/// listing comes from -- the disk for `jails adopt`, a declared and rechecked
/// capture for the route.
pub fn readings(names: &[String]) -> Vec<Reading> {
    let mut names = names.to_vec();
    names.sort();
    names
        .into_iter()
        .map(|name| {
            match SYNONYMS
                .iter()
                .find(|(synonym, _)| *synonym == name.to_ascii_lowercase())
            {
                Some((_, layer)) if *layer == name => Reading::Conventional(layer),
                Some((_, layer)) => Reading::Renamed { layer, dir: name },
                None => Reading::Unknown(name),
            }
        })
        .collect()
}

/// What a set of readings resolves to: the entries to write, and the two
/// kinds of question that are reported instead.
pub struct Resolved<'a> {
    pub writes: Vec<(&'a str, &'a str)>,
    pub ambiguous: Vec<(&'a str, Vec<&'a str>)>,
    pub unknown: Vec<&'a str>,
}

pub fn resolve(readings: &[Reading]) -> Resolved<'_> {
    // One layer, one directory. Two candidates is a question, not an answer.
    let mut by_layer: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for reading in readings {
        if let Reading::Renamed { layer, dir } = reading {
            by_layer.entry(layer).or_default().push(dir);
        }
    }
    let mut writes = Vec::new();
    let mut ambiguous = Vec::new();
    for (layer, dirs) in by_layer {
        match dirs.as_slice() {
            [only] => writes.push((layer, *only)),
            _ => ambiguous.push((layer, dirs)),
        }
    }
    Resolved {
        writes,
        ambiguous,
        unknown: readings
            .iter()
            .filter_map(|reading| match reading {
                Reading::Unknown(name) => Some(name.as_str()),
                _ => None,
            })
            .collect(),
    }
}

/// Every layer name, from the one owner of the list.
///
/// Derived from `LAYERS_IN_ORDER` rather than from the synonym table, which
/// is what lets `every_layer_is_its_own_synonym` catch a layer the table
/// forgot -- reading it back out of the table would make that test tautological.
#[cfg(test)]
fn known_layers() -> Vec<&'static str> {
    crate::config::LAYERS_IN_ORDER
        .iter()
        .map(|(name, _)| *name)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_synonym_maps_to_a_real_layer() {
        let layers = known_layers();
        for (synonym, layer) in SYNONYMS {
            assert!(
                layers.contains(layer),
                "`{synonym}` maps to `{layer}`, which is not in config::LAYERS_IN_ORDER"
            );
        }
    }

    /// Every layer has to be reachable under its own name, or adopting a
    /// project that already agrees with jails would report it as unknown.
    #[test]
    fn every_layer_is_its_own_synonym() {
        for layer in known_layers() {
            assert!(
                SYNONYMS.iter().any(|(synonym, _)| *synonym == layer),
                "layer `{layer}` is not in the synonym table under its own name"
            );
        }
    }

    #[test]
    fn the_table_has_no_duplicate_keys() {
        let mut seen = std::collections::BTreeSet::new();
        for (synonym, _) in SYNONYMS {
            assert!(seen.insert(*synonym), "`{synonym}` appears twice");
        }
    }

    /// The classification takes names rather than reading a directory, so
    /// this needs no filesystem at all -- which is the point of moving it
    /// down: the route hands it a captured listing.
    #[test]
    fn a_renamed_directory_becomes_a_layout_entry_and_an_unknown_one_does_not() {
        let readings = readings(&[
            "controllers".to_string(),
            "persistence".to_string(),
            "domain".to_string(),
            "util".to_string(),
        ]);
        assert!(readings.contains(&Reading::Renamed {
            layer: layout::WEB,
            dir: "controllers".to_string()
        }));
        assert!(readings.contains(&Reading::Renamed {
            layer: layout::ADAPTERS,
            dir: "persistence".to_string()
        }));
        assert!(readings.contains(&Reading::Conventional(layout::DOMAIN)));
        assert!(readings.contains(&Reading::Unknown("util".to_string())));
    }
}
