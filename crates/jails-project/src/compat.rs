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

/// Translate a schema-1 ledger into a schema-2 draft, in memory.
///
/// plan.md §R2.5, and the conservatism is the whole design. **Every** schema-1
/// row becomes a `LegacyEntry` and none becomes an `AppliedEntity`: the old
/// format did not record who asked for a row, and a row whose fields happen to
/// match today's manifest is still a row of unknown origin. Joining them by
/// coincidence would give a manifest ownership of files somebody generated by
/// hand, and the next `app apply` would relinquish them.
///
/// So the draft has empty applied, one-shot, resource and output tables and
/// generation 0. What that costs is real and is the point: after migrating,
/// `destroy` knows nothing until `jails adopt --legacy-key` resolves a row
/// explicitly. What it buys is that nothing is ever silently misattributed.
///
/// `jails.toml` is deliberately not translated. Its capabilities become
/// `DirectConfig` desired claims during ordinary resolution, and the file is
/// never deleted or rewritten by a migration -- it is the reader's.
pub fn translate(schema1: &Ledger) -> jails_protocol::envelope::LedgerV2 {
    jails_protocol::envelope::LedgerV2 {
        written_by: env!("CARGO_PKG_VERSION").to_string(),
        // Zero, so the first V2 mutation writes generation 1 with the
        // schema-1 file as its guarded before-image. A draft that claimed a
        // generation would be claiming a transition nobody performed.
        generation: 0,
        last_operation: None,
        applied: Vec::new(),
        one_shots: Vec::new(),
        resources: Vec::new(),
        outputs: Vec::new(),
        legacy: {
            let mut rows: Vec<jails_protocol::envelope::LegacyEntry> = schema1
                .applied
                .iter()
                .map(|row| jails_protocol::envelope::LegacyEntry {
                    recipe: row.recipe.clone(),
                    name: row.name.clone(),
                    package: row.package.clone(),
                    fields: row.fields.clone(),
                    indexes: row.indexes.clone(),
                    timestamps: row.timestamps,
                    on: row.on.clone(),
                    yields: row.yields.clone(),
                    spec_presence: match row.spec {
                        crate::ledger::SpecPresence::Present => {
                            jails_protocol::envelope::SpecPresence::Present
                        }
                        crate::ledger::SpecPresence::Absent => {
                            jails_protocol::envelope::SpecPresence::Absent
                        }
                        crate::ledger::SpecPresence::UnknownLegacy => {
                            jails_protocol::envelope::SpecPresence::UnknownLegacy
                        }
                    },
                    paths: row.files.clone(),
                })
                .collect();
            // Byte-identical duplicates collapse; everything else is kept,
            // because two rows that differ are two facts even when they name
            // the same thing.
            rows.sort_by(|a, b| {
                (&a.recipe, &a.name, &a.package).cmp(&(&b.recipe, &b.name, &b.package))
            });
            rows.dedup();
            rows
        },
        pending_conflict: None,
    }
}

/// Every legacy row this project carries, whichever schema its ledger is in.
///
/// The discovery half of §R2.5's adoption: a row can only be claimed by its
/// stable `LegacyKey`, so something has to say what the keys *are*. Reading it
/// here rather than through the transaction store keeps `doctor` read-only by
/// contract and keeps the executor out of a layer that only asks questions.
///
/// Both schemas answer. A project still on schema 1 has its rows translated on
/// the way through, which is the same translation the first V2 commit will
/// perform -- so the keys a reader is shown before migrating are the keys they
/// will adopt with afterwards.
pub fn legacy_rows(project: &crate::model::Project) -> Vec<jails_protocol::envelope::LegacyEntry> {
    let path = project.root().join(".jails/ledger.toml");
    let Ok(source) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    match jails_protocol::envelope::LedgerV2::parse_file(&source) {
        Ok(ledger) => ledger.legacy,
        Err(_) => match ledger::parse_source(&source) {
            Ok(schema1) => translate(&schema1).legacy,
            Err(_) => Vec::new(),
        },
    }
}

/// One unowned schema-1 row, and the command that claims it.
pub struct Adoptable {
    pub what: String,
    pub detail: String,
    pub command: String,
}

/// Every unowned row, ready to report.
///
/// The key derivation and the command skeleton live here rather than at the
/// reporting site because `adopt_legacy` is the thing that accepts them: a
/// second copy of "which key, spelled how" is exactly the drift that makes a
/// printed command not work when somebody pastes it.
pub fn adoptable(project: &crate::model::Project) -> Vec<Adoptable> {
    legacy_rows(project)
        .iter()
        .filter_map(|row| {
            let key = row
                .legacy_key(jails_protocol::envelope::LegacySourceKind::Schema1Applied)
                .ok()?;
            Some(Adoptable {
                what: format!("{} {}", row.recipe, row.name),
                detail: format!(
                    "{} file(s) from a schema-1 ledger, with no recorded owner -- so `destroy` \
                     and `sync` cannot act on them",
                    row.paths.len()
                ),
                command: format!(
                    "jails adopt --legacy-key {} --intent {}:{}",
                    key.to_label(),
                    row.recipe,
                    row.name
                ),
            })
        })
        .collect()
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
