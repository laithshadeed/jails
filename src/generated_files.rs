//! Provenance for generated paths, over the one ledger.
//!
//! This module used to *be* the storage: `.jails/files`, `.jails/version`,
//! `.jails/intents/<hash>.files` and `.jails/models/<hash>.files` -- four
//! layouts and a hand-rolled FNV to name the files. `src/ledger.rs` is the
//! storage now, and what is left here is the vocabulary the generators already
//! speak -- `record`, `paths`, `forget`, `record_model`, `model_fields` --
//! expressed against it.
//!
//! Keeping the API meant the switch touched no generator, which is the point.
//! `abstract.md` §4.5's complaint was never about these five verbs; it was that
//! two of the four files were intent registries **keyed differently**. One
//! entity keyed on `(recipe, name, package)` removes that, and the callers
//! never had to know either key existed.
//!
//! **Old projects still read.** A `.jails/` that predates the ledger is folded
//! into one on first write and the old files removed. Refusing to read them
//! would strand `destroy` on exactly the projects with the most history to lose.

use crate::Result;
use crate::ledger::{self, Applied, Ledger, Model};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Read the ledger, migrating a pre-ledger `.jails/` if that is what is there.
fn read(root: &Path) -> Result<Ledger> {
    let mut current = ledger::load(root)?;
    if current.applied.is_empty() && current.models.is_empty() {
        migrate_legacy(root, &mut current)?;
    }
    Ok(current)
}

/// Fold `.jails/intents/*`, `.jails/models/*`, `.jails/files` and
/// `.jails/version` into the ledger, then take them out.
///
/// The old per-intent filename was `<kind>-<name>-<fnv>.files`, and the hash
/// was over `kind\0name\0package` -- recoverable only for the first two. The
/// name hint carries those, so recipe and name survive and the package reads
/// back as the conventional one. That is lossy for an intent generated with
/// `--package`, and the consequence is stated rather than hidden: such an
/// intent migrates without its override, and a later `destroy` falls back to
/// recomputing paths -- which is what it did before any of this existed.
fn migrate_legacy(root: &Path, into: &mut Ledger) -> Result<()> {
    let intents = root.join(".jails/intents");
    let models = root.join(".jails/models");
    if !intents.is_dir() && !models.is_dir() {
        return Ok(());
    }

    if let Ok(entries) = fs::read_dir(&intents) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.extension().is_some_and(|ext| ext == "files") {
                continue;
            }
            let Some((recipe, name)) = legacy_identity(&path) else {
                continue;
            };
            let files = read_lines(&path)?;
            into.applied.push(Applied {
                recipe,
                name,
                package: String::new(),
                fields: Vec::new(),
                indexes: Vec::new(),
                on: String::new(),
                yields: String::new(),
                timestamps: false,
                files,
            });
        }
    }
    if let Ok(entries) = fs::read_dir(&models) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.extension().is_some_and(|ext| ext == "files") {
                continue;
            }
            let Some((_, name)) = legacy_identity(&path) else {
                continue;
            };
            into.models.push(Model {
                name,
                package: String::new(),
                fields: read_lines(&path)?,
            });
        }
    }

    for stale in [root.join(".jails/files"), root.join(".jails/version")] {
        let _ = fs::remove_file(stale);
    }
    let _ = fs::remove_dir_all(&intents);
    let _ = fs::remove_dir_all(&models);
    Ok(())
}

/// `record-note-44c464a9777ec2f0.files` -> `("record", "Note")`.
///
/// The hint was lowercased when written, so the name comes back lowercase and
/// is capitalised here -- every generated type name is capitalised, which is
/// what makes that recoverable at all.
fn legacy_identity(path: &Path) -> Option<(String, String)> {
    let stem = path.file_stem()?.to_str()?;
    let without_hash = stem.rsplit_once('-')?.0;
    let (recipe, name) = without_hash.split_once('-')?;
    if recipe.is_empty() || name.is_empty() {
        return None;
    }
    let mut chars = name.chars();
    let capitalised = chars.next()?.to_uppercase().collect::<String>() + chars.as_str();
    Some((recipe.to_string(), capitalised))
}

fn read_lines(path: &Path) -> Result<Vec<String>> {
    let source = fs::read_to_string(path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    Ok(source
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(str::to_owned)
        .collect())
}

/// Record the paths one intent wrote, leaving every other column alone.
///
/// `app apply` writes the *spec* on the same row. Replacing the row here would
/// erase it, and the erasure would look exactly like a manifest whose fields
/// line had been emptied -- so this sets `files` and nothing else.
pub(crate) fn record(
    root: &Path,
    kind: &str,
    name: &str,
    package: Option<&str>,
    paths: &[PathBuf],
) -> Result<()> {
    let mut relative = BTreeSet::new();
    for path in paths {
        relative.insert(ledger::relative(root, path)?);
    }
    let mut current = read(root)?;
    current.version = env!("CARGO_PKG_VERSION").to_string();
    ledger::entry_mut(&mut current, kind, name, package).files = relative.into_iter().collect();
    ledger::save(root, &current)
}

pub(crate) fn paths(
    root: &Path,
    kind: &str,
    name: &str,
    package: Option<&str>,
) -> Result<Option<Vec<PathBuf>>> {
    let current = read(root)?;
    let Some(entry) = current
        .applied
        .iter()
        .find(|entry| entry.is(kind, name, package))
    else {
        return Ok(None);
    };
    entry
        .files
        .iter()
        .map(|relative| ledger::absolute(root, relative))
        .collect::<Result<Vec<_>>>()
        .map(Some)
}

pub(crate) fn forget(root: &Path, kind: &str, name: &str, package: Option<&str>) -> Result<()> {
    let mut current = read(root)?;
    let before = current.applied.len();
    current
        .applied
        .retain(|entry| !entry.is(kind, name, package));
    if current.applied.len() != before {
        ledger::save(root, &current)?;
    }
    Ok(())
}

pub(crate) fn record_model(
    root: &Path,
    name: &str,
    package: Option<&str>,
    fields: &[String],
) -> Result<()> {
    let mut current = read(root)?;
    current.version = env!("CARGO_PKG_VERSION").to_string();
    let entry = Model {
        name: name.to_string(),
        package: package.unwrap_or_default().to_string(),
        fields: fields.to_vec(),
    };
    match current
        .models
        .iter_mut()
        .find(|model| model.name == entry.name && model.package == entry.package)
    {
        Some(existing) => *existing = entry,
        None => current.models.push(entry),
    }
    ledger::save(root, &current)
}

pub(crate) fn model_fields(
    root: &Path,
    name: &str,
    package: Option<&str>,
) -> Result<Option<Vec<String>>> {
    let current = read(root)?;
    Ok(current
        .models
        .iter()
        .find(|model| model.name == name && model.package == package.unwrap_or_default())
        .map(|model| model.fields.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);

    fn scratch() -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "jails-provenance-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn generated_paths_are_sorted_normalised_and_scoped_per_intent() {
        let root = scratch();
        let first = root.join("src/test/java/A.java");
        let second = root.join("src/main/java/B.java");

        record(&root, "record", "A", None, &[first.clone(), second.clone()]).unwrap();

        let recorded = paths(&root, "record", "A", None).unwrap().unwrap();
        assert_eq!(recorded, vec![second, first], "sorted, not insertion order");
        assert!(paths(&root, "record", "B", None).unwrap().is_none());
    }

    #[test]
    fn recording_the_same_intent_twice_replaces_rather_than_appends() {
        let root = scratch();
        record(
            &root,
            "record",
            "A",
            None,
            &[root.join("src/main/java/A.java")],
        )
        .unwrap();
        record(
            &root,
            "record",
            "A",
            None,
            &[root.join("src/main/java/B.java")],
        )
        .unwrap();

        assert_eq!(
            paths(&root, "record", "A", None).unwrap().unwrap(),
            vec![root.join("src/main/java/B.java")],
            "identity is (recipe, name, package), so this is one entity updated"
        );
    }

    #[test]
    fn the_package_override_is_part_of_identity() {
        let root = scratch();
        record(
            &root,
            "record",
            "A",
            None,
            &[root.join("src/main/java/A.java")],
        )
        .unwrap();
        record(
            &root,
            "record",
            "A",
            Some("other"),
            &[root.join("src/main/java/other/A.java")],
        )
        .unwrap();

        assert!(paths(&root, "record", "A", None).unwrap().is_some());
        assert!(
            paths(&root, "record", "A", Some("other"))
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn forgetting_an_intent_leaves_the_others() {
        let root = scratch();
        record(
            &root,
            "record",
            "A",
            None,
            &[root.join("src/main/java/A.java")],
        )
        .unwrap();
        record(
            &root,
            "record",
            "B",
            None,
            &[root.join("src/main/java/B.java")],
        )
        .unwrap();

        forget(&root, "record", "A", None).unwrap();

        assert!(paths(&root, "record", "A", None).unwrap().is_none());
        assert!(paths(&root, "record", "B", None).unwrap().is_some());
    }

    #[test]
    fn models_round_trip_and_are_scoped_by_package() {
        let root = scratch();
        record_model(&root, "Note", None, &["title:string!".to_string()]).unwrap();

        assert_eq!(
            model_fields(&root, "Note", None).unwrap(),
            Some(vec!["title:string!".to_string()])
        );
        assert_eq!(model_fields(&root, "Note", Some("other")).unwrap(), None);
    }

    /// A project whose `.jails/` predates the ledger must not lose its history.
    #[test]
    fn a_pre_ledger_project_is_migrated_rather_than_ignored() {
        let root = scratch();
        fs::create_dir_all(root.join(".jails/intents")).unwrap();
        fs::create_dir_all(root.join(".jails/models")).unwrap();
        fs::write(
            root.join(".jails/intents/record-note-44c464a9777ec2f0.files"),
            "src/main/java/com/example/demo/domain/Note.java\n",
        )
        .unwrap();
        fs::write(
            root.join(".jails/models/model-note-70ab6d016b346e7e.files"),
            "title:string!\n",
        )
        .unwrap();
        fs::write(root.join(".jails/files"), "src/main/java/x\n").unwrap();
        fs::write(root.join(".jails/version"), "0.0.1\n").unwrap();

        // Any write triggers the fold.
        record_model(&root, "Other", None, &["a:string".to_string()]).unwrap();

        assert_eq!(
            paths(&root, "record", "Note", None).unwrap(),
            Some(vec![
                root.join("src/main/java/com/example/demo/domain/Note.java")
            ]),
            "the recorded path set survived the migration"
        );
        assert_eq!(
            model_fields(&root, "Note", None).unwrap(),
            Some(vec!["title:string!".to_string()])
        );
        assert!(
            !root.join(".jails/intents").exists(),
            "the old layout is gone"
        );
        assert!(!root.join(".jails/files").exists());
        assert!(root.join(".jails/ledger.toml").is_file());
    }
}
