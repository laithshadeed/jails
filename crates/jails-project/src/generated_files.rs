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
//! into one *in memory* on every read, and the old files are removed only after
//! a mutating command has durably written the ledger that replaces them.
//! Refusing to read them would strand `destroy` on exactly the projects with
//! the most history to lose; deleting them while merely reading -- which is
//! what this module used to do -- made `app plan` and `--pretend` destructive.

use crate::ledger::{self, Applied, Ledger, Model};
use jails_support::Result;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// The ledger as it currently reads, and the pre-ledger sources that had to be
/// folded into it to get there.
struct Read {
    ledger: Ledger,
    /// Exactly the paths the fold consumed, kept rather than re-derived. Same
    /// rule as the recorded file list: a name rebuilt later is today's answer
    /// for yesterday's layout, and here it would delete the wrong thing.
    legacy: Vec<PathBuf>,
}

impl Read {
    /// Take the old layout out, once the ledger that replaced it is on disk.
    ///
    /// Call order is the whole point: `ledger::save` is atomic, so at every
    /// instant either the old layout or the new ledger is a complete record of
    /// what jails owns -- never neither.
    ///
    /// Best-effort by design. The ledger is authoritative the moment it is
    /// written, and a leftover legacy directory is folded again on the next
    /// read to the same result; failing the command over it would turn a
    /// successful migration into a reported failure.
    fn retire(&self) {
        for path in &self.legacy {
            let _ = if path.is_dir() {
                // A pre-ledger `.jails/` subdirectory jails created and
                // nothing else writes into.
                jails_support::apply::remove_managed_tree(path)
            } else {
                jails_support::apply::remove(path)
            };
        }
    }
}

/// Read the ledger, folding a pre-ledger `.jails/` **in memory only**.
///
/// This used to delete the legacy files as a side effect of reading them, which
/// made `app plan`, `destroy --pretend` and `generate`'s model lookups
/// destructive: a reader asking what jails would do consumed the only copy of
/// what jails had done. Reading is non-mutating for every caller now, and
/// `Read::retire` is what a mutating command calls afterwards.
fn read(root: &Path) -> Result<Read> {
    let mut ledger = ledger::load(root)?;
    let legacy = if ledger.applied.is_empty() && ledger.models.is_empty() {
        fold_legacy(root, &mut ledger)?
    } else {
        Vec::new()
    };
    Ok(Read { ledger, legacy })
}

/// Fold `.jails/intents/*` and `.jails/models/*` into the ledger in memory.
///
/// Returns the paths it consumed, which is what `Read::retire` later removes.
///
/// The old per-intent filename was `<kind>-<name>-<fnv>.files`, and the hash
/// was over `kind\0name\0package` -- recoverable only for the first two. The
/// name hint carries those, so recipe and name survive and the package reads
/// back as the conventional one. That is lossy for an intent generated with
/// `--package`, and the consequence is stated rather than hidden: such an
/// intent migrates without its override, and a later `destroy` falls back to
/// recomputing paths -- which is what it did before any of this existed.
fn fold_legacy(root: &Path, into: &mut Ledger) -> Result<Vec<PathBuf>> {
    let intents = root.join(".jails/intents");
    let models = root.join(".jails/models");
    if !intents.is_dir() && !models.is_dir() {
        return Ok(Vec::new());
    }
    let mut consumed = Vec::new();

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
            consumed.push(path);
            into.applied.push(Applied {
                recipe,
                name,
                package: String::new(),
                // The old layout recorded paths and nothing about origin, so
                // whether a manifest ever owned this is genuinely unknown --
                // and staying unknown is the honest answer.
                spec: ledger::SpecPresence::UnknownLegacy,
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
            let fields = read_lines(&path)?;
            consumed.push(path);
            into.models.push(Model {
                name,
                package: String::new(),
                fields,
            });
        }
    }

    // The two siblings that layout kept but this one has no column for: a flat
    // path list superseded by the per-entity `files`, and a version string the
    // ledger's own `version` replaces. They are part of the layout being
    // retired even though nothing was folded out of them.
    consumed.extend([root.join(".jails/files"), root.join(".jails/version")]);
    consumed.extend([intents, models]);
    Ok(consumed)
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
pub fn record(
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
    let mut state = read(root)?;
    state.ledger.version = env!("CARGO_PKG_VERSION").to_string();
    ledger::entry_mut(
        &mut state.ledger,
        ledger::EntityKey::new(kind, name, package),
    )
    .files = relative.into_iter().collect();
    ledger::save(root, &state.ledger)?;
    state.retire();
    Ok(())
}

pub fn paths(
    root: &Path,
    kind: &str,
    name: &str,
    package: Option<&str>,
) -> Result<Option<Vec<PathBuf>>> {
    let current = read(root)?.ledger;
    let Some(entry) = current
        .applied
        .iter()
        .find(|entry| entry.is(ledger::EntityKey::new(kind, name, package)))
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

pub fn forget(root: &Path, kind: &str, name: &str, package: Option<&str>) -> Result<()> {
    let mut state = read(root)?;
    let before = state.ledger.applied.len();
    state
        .ledger
        .applied
        .retain(|entry| !entry.is(ledger::EntityKey::new(kind, name, package)));
    if state.ledger.applied.len() != before || !state.legacy.is_empty() {
        ledger::save(root, &state.ledger)?;
        state.retire();
    }
    Ok(())
}

pub fn record_model(
    root: &Path,
    name: &str,
    package: Option<&str>,
    fields: &[String],
) -> Result<()> {
    let mut state = read(root)?;
    state.ledger.version = env!("CARGO_PKG_VERSION").to_string();
    let entry = Model {
        name: name.to_string(),
        package: package.unwrap_or_default().to_string(),
        fields: fields.to_vec(),
    };
    match state
        .ledger
        .models
        .iter_mut()
        .find(|model| model.name == entry.name && model.package == entry.package)
    {
        Some(existing) => *existing = entry,
        None => state.ledger.models.push(entry),
    }
    ledger::save(root, &state.ledger)?;
    state.retire();
    Ok(())
}

pub fn model_fields(root: &Path, name: &str, package: Option<&str>) -> Result<Option<Vec<String>>> {
    // Schema 2 first, and the schema-1 reader only for a project that still
    // has one. `ledger::load` refuses a newer schema outright -- correctly, it
    // is a downgrade -- so asking it first turned "which fields does this
    // record declare" into a hard error on every project the current binary
    // has written.
    if let Some(fields) = schema_two_fields(root, name, package)? {
        return Ok(Some(fields));
    }
    let path = root.join(".jails/ledger.toml");
    if path.is_file() && crate::ledger::load(root).is_err() {
        return Ok(None);
    }
    let current = read(root)?.ledger;
    Ok(current
        .models
        .iter()
        .find(|model| model.name == name && model.package == package.unwrap_or_default())
        .map(|model| model.fields.clone()))
}

/// The field spec a schema-2 store records for a generated record or scaffold.
///
/// This is the *declared* spec, not the record read back off disk, which is
/// what keeps `@pk`/`@unique`/`@index` alive across a compose: a Java type
/// cannot say what its column is, and inferring a primary key from a component
/// called `id` would put one in a schema nobody asked for.
fn schema_two_fields(
    root: &Path,
    name: &str,
    package: Option<&str>,
) -> Result<Option<Vec<String>>> {
    let Ok(source) = std::fs::read_to_string(root.join(".jails/ledger.toml")) else {
        return Ok(None);
    };
    let Ok(ledger) = jails_protocol::envelope::LedgerV2::parse_file(&source) else {
        return Ok(None);
    };
    Ok(ledger.applied.iter().find_map(|entity| {
        let jails_protocol::entity::EntityId::Intent(id) = &entity.id else {
            return None;
        };
        let jails_protocol::entity::EntitySpec::Intent(spec) = &entity.version.spec else {
            return None;
        };
        (id.name.as_str() == name && id.package.as_str() == package.unwrap_or_default())
            .then(|| spec.arguments.canonical())
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn scratch() -> PathBuf {
        jails_support::scratch::ScratchDir::in_temp("jails-provenance")
            .unwrap()
            .keep()
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

    /// Reading is not a migration. `app plan`, `destroy --pretend` and every
    /// model lookup go through `read`, and this module used to delete the old
    /// layout from inside it -- so asking jails what it *would* do consumed the
    /// only copy of what it had done.
    #[test]
    fn reading_a_pre_ledger_project_folds_it_without_touching_a_byte() {
        let root = scratch();
        fs::create_dir_all(root.join(".jails/intents")).unwrap();
        let intent = root.join(".jails/intents/record-note-44c464a9777ec2f0.files");
        fs::write(&intent, "src/main/java/com/example/demo/domain/Note.java\n").unwrap();
        let stale = root.join(".jails/version");
        fs::write(&stale, "0.0.1\n").unwrap();
        let before = snapshot(&root.join(".jails"));

        assert_eq!(
            paths(&root, "record", "Note", None).unwrap(),
            Some(vec![
                root.join("src/main/java/com/example/demo/domain/Note.java")
            ]),
            "the legacy layout is still readable"
        );
        assert_eq!(model_fields(&root, "Absent", None).unwrap(), None);

        assert_eq!(
            before,
            snapshot(&root.join(".jails")),
            "reads wrote nothing"
        );
        assert!(!root.join(".jails/ledger.toml").exists());

        // The first mutating command is what retires it -- and only after the
        // ledger that replaces it is on disk.
        record(
            &root,
            "record",
            "Other",
            None,
            &[root.join("src/main/java/O.java")],
        )
        .unwrap();
        assert!(root.join(".jails/ledger.toml").is_file());
        assert!(!intent.exists());
        assert!(!stale.exists());
        assert_eq!(
            paths(&root, "record", "Note", None).unwrap(),
            Some(vec![
                root.join("src/main/java/com/example/demo/domain/Note.java")
            ]),
            "and the folded history is in the ledger, not lost with the files"
        );
    }

    /// Every path under a directory with its bytes, so "wrote nothing" is a
    /// claim about content and not only about which names still exist.
    fn snapshot(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
        let mut out = Vec::new();
        let Ok(entries) = fs::read_dir(dir) else {
            return out;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                out.extend(snapshot(&path));
            } else {
                out.push((path.clone(), fs::read(&path).unwrap()));
            }
        }
        out.sort();
        out
    }
}
