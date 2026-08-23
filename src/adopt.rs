//! `jails adopt`: teach jails where an existing project already keeps things.
//!
//! `plan.md` §12. A project jails did not create keeps its controllers in
//! `controllers`, its repositories in `persistence`, its DTOs in `dto` — and
//! every jails command that reports or writes per layer then gets it wrong.
//! `stats` counted `Web 2` as `Other 4` until a `[layout]` table said
//! otherwise, and that was the whole fix: **this command writes configuration,
//! not new machinery.** Everything downstream already reads
//! `Config::layers()`.
//!
//! ## Three rules, and each is the point
//!
//! **A directory matching nothing is reported, not guessed.** A wrong `[layout]`
//! entry is worse than a missing one: jails would confidently write into the
//! wrong package and `destroy` would look in a third place. So the synonym
//! table is closed, and what it does not recognise is printed for the reader to
//! decide about.
//!
//! **It never writes `[project] capabilities`.** That list is what `jails sync`
//! applies, and inferring it from a directory listing would have `sync` install
//! things nobody asked for into a project jails did not create — the single
//! most destructive thing this command could do.
//!
//! **One layer, one directory.** If two directories both look like the web
//! layer, neither is written and both are reported: a `[layout]` table can only
//! say one thing, and picking the first alphabetically would be a coin toss the
//! reader never saw.

use crate::config;
use crate::generate::{find_project_root, layout};
use jails_support::Result;
use std::collections::BTreeMap;
use std::path::Path;

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
enum Reading {
    /// Recognised, and already spelled the way jails spells it.
    Conventional(&'static str),
    /// Recognised under another name: this is a `[layout]` entry.
    Renamed { layer: &'static str, dir: String },
    /// Not in the table. Reported, never guessed at.
    Unknown(String),
}

pub fn adopt(pretend: bool) -> Result<()> {
    let root = find_project_root()?;
    let project = crate::model::Project::inspect(&root)?;
    if project.base().is_empty() {
        return Err(
            "no Java sources found under src/main/java, so there is no package to read.\n       \
             fix: run this from a project with sources, or `jails new <name>` to create one."
                .to_string(),
        );
    }
    let base_dir = root
        .join("src/main/java")
        .join(project.base().replace('.', "/"));
    report(&root, &read_layout(&base_dir), pretend)
}

/// Classify every immediate subdirectory of the base package.
fn read_layout(base_dir: &Path) -> Vec<Reading> {
    let Ok(entries) = std::fs::read_dir(base_dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
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

fn report(root: &Path, readings: &[Reading], pretend: bool) -> Result<()> {
    // One layer, one directory. Two candidates is a question, not an answer.
    let mut by_layer: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for reading in readings {
        if let Reading::Renamed { layer, dir } = reading {
            by_layer.entry(layer).or_default().push(dir);
        }
    }

    let mut writes: Vec<(&str, &str)> = Vec::new();
    let mut ambiguous: Vec<(&str, &Vec<&str>)> = Vec::new();
    for (layer, dirs) in &by_layer {
        match dirs.as_slice() {
            [only] => writes.push((layer, only)),
            _ => ambiguous.push((layer, dirs)),
        }
    }

    for reading in readings {
        if let Reading::Conventional(layer) = reading {
            println!("  keep    {layer:<10} already jails' own name");
        }
    }
    for (layer, dir) in &writes {
        println!("  layout  {layer:<10} = \"{dir}\"");
    }
    for (layer, dirs) in &ambiguous {
        println!(
            "  ask     {layer:<10} matches {} -- a [layout] table can only name one, \
             so none is written",
            dirs.join(", ")
        );
    }
    let unknown: Vec<&str> = readings
        .iter()
        .filter_map(|reading| match reading {
            Reading::Unknown(name) => Some(name.as_str()),
            _ => None,
        })
        .collect();
    for name in &unknown {
        println!("  ignore  {name:<10} not a layer jails knows -- left alone");
    }

    if writes.is_empty() {
        println!();
        println!("nothing to adopt: no directory under the base package needs a different name.");
        return Ok(());
    }
    if pretend {
        println!();
        println!("--pretend: nothing was written.");
        return Ok(());
    }
    for (layer, dir) in &writes {
        config::record_layout(root, layer, dir)?;
    }
    println!();
    println!(
        "wrote {} [layout] entr{} to {}. `jails stats` reports against them now.",
        writes.len(),
        if writes.len() == 1 { "y" } else { "ies" },
        root.join(config::FILE).display()
    );
    // Said out loud because it is the rule that makes this command safe to run.
    println!("[project] capabilities was not touched: `jails sync` acts on that list.");
    Ok(())
}

/// Every layer name, for the tests to check the table against.
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

    #[test]
    fn a_renamed_directory_becomes_a_layout_entry_and_an_unknown_one_does_not() {
        let root = std::env::temp_dir().join(format!("jails-adopt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for dir in ["controllers", "persistence", "domain", "util"] {
            std::fs::create_dir_all(root.join(dir)).unwrap();
        }
        let readings = read_layout(&root);
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
