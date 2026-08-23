//! Reading a project's machine state without changing it.
//!
//! ## Why a facade, and why read-only
//!
//! plan.md §R6.1 step 7 asks for one reader every command uses to parse
//! schema 1, schema 2 and legacy inputs "without mutation", and step 9 makes
//! the switch to schema 2 atomic at the dispatch point. Those two go
//! together: while both writers exist, any *read* that also cleans up is a
//! second writer — and §R6.3 names exactly where that already happened,
//! `generated_files::migrate_legacy` and `app::migrate_app_state`, both of
//! which removed their sources while still building the in-memory state.
//!
//! The rule this enforces is simple and load-bearing: **deleting a legacy
//! source is a `FileOp` committed after the schema-2 ledger is durable**, not
//! a side effect of looking. A `doctor` run, an `app plan`, a `--pretend` —
//! all of them read machine state, and none of them may leave a project
//! different for having been inspected.
//!
//! ## Why classification is a value rather than a `bool`
//!
//! "Does this project have a ledger" has four answers a caller must treat
//! differently: nothing yet, schema 2, a schema-1 store that can be
//! translated, and machine state that cannot be read at all. Collapsing the
//! last two loses the difference between "migrate this" and "stop".

use crate::ledger::{self, Ledger};
use jails_support::Result;
use std::path::{Path, PathBuf};

/// What a project's machine state is, right now, unmodified.
#[derive(Clone, Debug)]
pub enum MachineState {
    /// No machine state at all. The ordinary state of a project jails has
    /// never touched.
    Absent,
    /// The current store, read successfully.
    Current(Ledger),
    /// Pre-schema-2 sources that a first schema-2 commit will translate.
    ///
    /// The translation is *in memory*. `sources` names what a migration will
    /// have to delete, and deleting them is that commit's business.
    Legacy {
        translated: Ledger,
        sources: Vec<PathBuf>,
    },
    /// Present and unreadable. Deliberately distinct from `Absent`: treating
    /// an unreadable store as an empty one is the fail-open bug §3.1 fixed,
    /// and it would silently offer to regenerate a project's whole contents.
    Unreadable(String),
}

impl MachineState {
    /// The store to plan against, or the reason there is none.
    pub fn ledger(&self) -> Result<&Ledger> {
        match self {
            Self::Current(ledger)
            | Self::Legacy {
                translated: ledger, ..
            } => Ok(ledger),
            Self::Absent => Err(
                "this project has no jails state yet.\n       fix: nothing recorded means \
                 nothing to reconcile against."
                    .to_string(),
            ),
            Self::Unreadable(why) => Err(why.clone()),
        }
    }

    /// Whether a first schema-2 commit still has legacy sources to retire.
    pub fn pending_sources(&self) -> &[PathBuf] {
        match self {
            Self::Legacy { sources, .. } => sources,
            _ => &[],
        }
    }

    /// A sentence for a report.
    pub fn describe(&self) -> String {
        match self {
            Self::Absent => "no jails state".to_string(),
            Self::Current(_) => "current".to_string(),
            Self::Legacy { sources, .. } => format!(
                "{} legacy source{} to migrate",
                sources.len(),
                if sources.len() == 1 { "" } else { "s" }
            ),
            Self::Unreadable(why) => format!("unreadable: {why}"),
        }
    }
}

/// Read a project's machine state. Writes nothing, ever.
///
/// The `#[must_use]` is not decoration: a caller that reads state and drops
/// it has usually meant to *check* something, and the check is the value.
#[must_use = "reading machine state is only useful for what it says"]
pub fn read(root: &Path) -> MachineState {
    let machine = root.join(".jails");
    match ledger::load(root) {
        Ok(ledger) if ledger.is_empty() && !machine.exists() => MachineState::Absent,
        Ok(ledger) => {
            let sources = legacy_sources(&machine);
            if sources.is_empty() {
                MachineState::Current(ledger)
            } else {
                MachineState::Legacy {
                    translated: ledger,
                    sources,
                }
            }
        }
        // Fail closed. An unreadable store is not an empty one, and the
        // difference is a project's whole contents.
        Err(why) => MachineState::Unreadable(why),
    }
}

/// Every pre-schema-2 file still present.
///
/// Named rather than derived from a directory walk: a closed list is what
/// lets a migration say exactly what it will delete, and a walk would sweep
/// up anything a future version puts there.
fn legacy_sources(machine: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for name in ["app-state-v1", "files", "version"] {
        let path = machine.join(name);
        if path.exists() {
            found.push(path);
        }
    }
    for directory in ["intents", "models"] {
        let path = machine.join(directory);
        let Ok(entries) = std::fs::read_dir(&path) else {
            continue;
        };
        let mut children: Vec<PathBuf> = entries
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .collect();
        children.sort();
        found.extend(children);
        found.push(path);
    }
    found.sort();
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use jails_support::apply;
    use jails_support::scratch::ScratchDir;

    fn project() -> ScratchDir {
        ScratchDir::in_temp("jails-compat").unwrap()
    }

    #[test]
    fn a_project_jails_has_never_touched_reads_as_absent() {
        let scratch = project();
        assert!(matches!(read(scratch.path()), MachineState::Absent));
        scratch.close().unwrap();
    }

    /// Treating an unreadable store as an empty one is the fail-open bug §3.1
    /// fixed; it would silently offer to regenerate a project's contents.
    #[test]
    fn an_unreadable_store_is_not_an_empty_one() {
        let scratch = project();
        apply::ensure_directory(scratch.path().join(".jails")).unwrap();
        apply::put(scratch.path().join(".jails/ledger.toml"), "not = a [ledger").unwrap();

        match read(scratch.path()) {
            MachineState::Unreadable(why) => assert!(!why.trim().is_empty()),
            other => panic!("expected unreadable, got {}", other.describe()),
        }
        assert!(read(scratch.path()).ledger().is_err());
        scratch.close().unwrap();
    }

    /// The one property the whole facade exists for.
    #[test]
    fn reading_legacy_state_leaves_every_source_exactly_where_it_was() {
        let scratch = project();
        let machine = scratch.path().join(".jails");
        apply::ensure_directory(machine.join("intents")).unwrap();
        apply::put(machine.join("version"), "0.0.1\n").unwrap();
        apply::put(machine.join("files"), "src/main/java/App.java\n").unwrap();
        apply::put(
            machine.join("intents/record-note.files"),
            "src/main/java/com/example/demo/domain/Note.java\n",
        )
        .unwrap();

        let state = read(scratch.path());
        let sources = state.pending_sources().to_vec();
        assert!(!sources.is_empty(), "{}", state.describe());
        for source in &sources {
            assert!(
                source.exists(),
                "{} was removed by a read",
                source.display()
            );
        }
        // And again: reading twice must report the same thing.
        assert_eq!(read(scratch.path()).pending_sources(), sources);
        scratch.close().unwrap();
    }

    /// A closed list is what lets a migration say exactly what it will
    /// delete; a directory walk would sweep up whatever a future version put
    /// there.
    #[test]
    fn an_unrecognised_file_under_the_machine_root_is_not_a_legacy_source() {
        let scratch = project();
        let machine = scratch.path().join(".jails");
        apply::ensure_directory(&machine).unwrap();
        apply::put(machine.join("version"), "0.0.1\n").unwrap();
        apply::put(machine.join("something-new.toml"), "x = 1\n").unwrap();

        let state = read(scratch.path());
        let named: Vec<String> = state
            .pending_sources()
            .iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert!(named.contains(&"version".to_string()), "{named:?}");
        assert!(
            !named.contains(&"something-new.toml".to_string()),
            "{named:?}"
        );
        scratch.close().unwrap();
    }

    /// "Nothing yet" and "cannot be read" must not collapse into one answer:
    /// one means migrate, the other means stop.
    #[test]
    fn every_state_describes_itself_distinctly() {
        let described = [
            MachineState::Absent.describe(),
            MachineState::Unreadable("the ledger did not parse".to_string()).describe(),
        ];
        assert_ne!(described[0], described[1]);
    }
}
