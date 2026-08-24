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
use jails_protocol::envelope::LedgerV2;
use jails_protocol::snapshot::{LegacyDirectoryKind, LegacyFileName, LegacySourcePath};
use jails_support::Result;
use std::path::{Path, PathBuf};

/// What a project's machine state is, right now, unmodified.
#[derive(Clone, Debug)]
pub enum MachineState {
    /// No machine state at all. The ordinary state of a project jails has
    /// never touched.
    Absent,
    /// The store this binary writes, read successfully.
    Current(LedgerV2),
    /// Pre-schema-2 state that a first schema-2 commit will translate.
    ///
    /// The translation is *in memory*. `sources` names the *other* legacy
    /// files a migration will have to delete; the schema-1 ledger itself is
    /// not among them, because the guarded ledger replace consumes it as
    /// `ledger_before -> ledger_after` and deleting it here would drop the very
    /// rows being migrated.
    Legacy {
        translated: LedgerV2,
        sources: Vec<PathBuf>,
    },
    /// Present and unreadable. Deliberately distinct from `Absent`: treating
    /// an unreadable store as an empty one is the fail-open bug §3.1 fixed,
    /// and it would silently offer to regenerate a project's whole contents.
    Unreadable(String),
}

impl MachineState {
    /// The store to plan against, or the reason there is none.
    pub fn ledger(&self) -> Result<&LedgerV2> {
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
                "schema 1, with {} other legacy source{} to migrate",
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
    let source = match std::fs::read_to_string(machine.join("ledger.toml")) {
        Ok(source) => source,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            // No store. `.jails` existing is not evidence of one: a project
            // that has only ever been *prepared* has an objects directory and
            // a lock and nothing to plan against, and reporting that as
            // something to migrate would make every fresh project look like a
            // pre-schema-2 one. Only a legacy source is evidence, and then the
            // ledger really is the missing half.
            let sources = legacy_sources(&machine);
            return match sources.is_empty() {
                true => MachineState::Absent,
                false => MachineState::Legacy {
                    translated: translate(&Ledger::empty()),
                    sources,
                },
            };
        }
        // Fail closed. An unreadable store is not an empty one, and the
        // difference is a project's whole contents.
        Err(error) => return MachineState::Unreadable(error.to_string()),
    };
    // Schema 2 first: it is what this binary writes, and asking the older
    // parser first would answer "use a newer jails" about a store this very
    // binary produced.
    match LedgerV2::parse_file(&source) {
        Ok(ledger) => MachineState::Current(ledger),
        Err(current) => match ledger::parse_source(&source) {
            Ok(schema1) => MachineState::Legacy {
                translated: translate(&schema1),
                sources: legacy_sources(&machine),
            },
            // Neither format. The schema-2 message is the one to show: this
            // binary writes schema 2, and a store it cannot read is more
            // likely a newer one than an older one.
            Err(_) => MachineState::Unreadable(current),
        },
    }
}

/// Where one legacy source lives, given the machine directory.
///
/// The single owner of that mapping. The lister below and the executor that
/// deletes them both go through it, so a source recorded under one spelling
/// cannot be looked for under another -- which would make a migration refuse
/// its own guarded preimage.
pub fn legacy_source_at(machine: &Path, path: &LegacySourcePath) -> PathBuf {
    match path {
        LegacySourcePath::Schema1Ledger => machine.join("ledger.toml"),
        LegacySourcePath::AppState => machine.join("app-state-v1"),
        LegacySourcePath::GlobalFiles => machine.join("files"),
        LegacySourcePath::VersionFile => machine.join("version"),
        LegacySourcePath::IntentFiles { name } => machine.join("intents").join(name.as_str()),
        LegacySourcePath::ModelFiles { name } => machine.join("models").join(name.as_str()),
    }
}

/// The same closed list, typed and with each source's on-disk path.
///
/// The untyped [`legacy_sources`] answers "is this project pre-schema-2"; this
/// answers "what exactly will the migration delete", which is a different
/// question and needs the identity the record stores rather than a `PathBuf`.
/// A directory is named separately because it is removed after its children
/// and is not a source with an image.
///
/// Takes the machine directory rather than the project root, like its untyped
/// sibling: what these read is `.jails`, and a function that joined the name
/// itself would be a second place that decides where machine state lives.
///
/// **The `.files` children are enumerated, not guessed.** A migration records
/// the sources it *found*, and the executor refuses to delete a legacy target
/// that migration did not find -- so a name invented here would refuse at
/// commit rather than be silently skipped.
pub fn legacy_typed_sources(machine: &Path) -> Vec<(LegacySourcePath, PathBuf)> {
    let mut found: Vec<(LegacySourcePath, PathBuf)> = Vec::new();
    for kind in [
        LegacySourcePath::AppState,
        LegacySourcePath::GlobalFiles,
        LegacySourcePath::VersionFile,
    ] {
        let path = legacy_source_at(machine, &kind);
        if path.is_file() {
            found.push((kind, path));
        }
    }
    for (directory, wrap) in [
        (
            "intents",
            (|name| LegacySourcePath::IntentFiles { name })
                as fn(LegacyFileName) -> LegacySourcePath,
        ),
        ("models", |name| LegacySourcePath::ModelFiles { name }),
    ] {
        let at = machine.join(directory);
        let Ok(entries) = std::fs::read_dir(&at) else {
            continue;
        };
        let mut children: Vec<PathBuf> = entries
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| path.is_file())
            .collect();
        children.sort();
        for child in children {
            let Some(name) = child.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            // A component that is not a legacy `.files` name is left alone
            // rather than deleted: this list is what a migration promises to
            // remove, and promising to remove something it cannot name is how
            // a cleanup takes a file it did not put there.
            let Ok(name) = LegacyFileName::parse(name) else {
                continue;
            };
            found.push((wrap(name), child));
        }
    }
    found.sort_by(|(one, _), (other, _)| one.cmp(other));
    found
}

/// Which legacy directories this project still has.
pub fn legacy_directories(machine: &Path) -> Vec<LegacyDirectoryKind> {
    [
        ("intents", LegacyDirectoryKind::Intents),
        ("models", LegacyDirectoryKind::Models),
    ]
    .into_iter()
    .filter(|(name, _)| machine.join(name).is_dir())
    .map(|(_, kind)| kind)
    .collect()
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
    /// The answer this facade got wrong for as long as nothing asked it: a
    /// store *this binary writes* was reported as "use a newer jails",
    /// because the schema-1 parser was asked first and refuses anything with a
    /// `schema` key.
    #[test]
    fn a_store_this_binary_writes_reads_as_current() {
        let dir = jails_support::scratch::ScratchDir::in_temp("jails-compat-current")
            .unwrap()
            .keep();
        let machine = dir.join(".jails");
        crate::apply::ensure_directory(&machine).unwrap();
        let ledger = jails_protocol::envelope::LedgerV2 {
            generation: 1,
            ..Default::default()
        };
        crate::apply::put(machine.join("ledger.toml"), ledger.render().unwrap()).unwrap();

        match read(&dir) {
            MachineState::Current(read_back) => assert_eq!(read_back.generation, 1),
            other => panic!("expected the current store, got {}", other.describe()),
        }
    }

    /// A machine root with no store in it is not something to migrate.
    ///
    /// `.jails` exists as soon as anything has been *prepared* -- there is an
    /// objects directory and a lock and nothing to plan against -- so treating
    /// its existence as evidence of a store made every fresh project look
    /// pre-schema-2. Only a legacy source is evidence.
    #[test]
    fn a_machine_root_without_a_store_is_absent_not_a_migration() {
        let dir = jails_support::scratch::ScratchDir::in_temp("jails-compat-empty-machine")
            .unwrap()
            .keep();
        let machine = dir.join(".jails");
        crate::apply::ensure_directory(machine.join("objects")).unwrap();
        assert!(matches!(read(&dir), MachineState::Absent));

        // One legacy source *is* evidence, and then the missing ledger is the
        // half that is gone rather than a project with no history.
        crate::apply::put(machine.join("version"), "0.0.1\n").unwrap();
        match read(&dir) {
            MachineState::Legacy { sources, .. } => assert_eq!(sources.len(), 1, "{sources:?}"),
            other => panic!("expected a migration, got {}", other.describe()),
        }
    }

    /// And a schema-1 store is still `Legacy` even when it is the only source
    /// left, because the ledger itself is what a migration replaces.
    #[test]
    fn a_schema_one_store_alone_is_still_a_migration() {
        let dir = jails_support::scratch::ScratchDir::in_temp("jails-compat-schema1")
            .unwrap()
            .keep();
        let machine = dir.join(".jails");
        crate::apply::ensure_directory(&machine).unwrap();
        crate::apply::put(machine.join("ledger.toml"), "version = \"0.1.0\"\n").unwrap();

        match read(&dir) {
            MachineState::Legacy { sources, .. } => assert!(sources.is_empty(), "{sources:?}"),
            other => panic!("expected a migration, got {}", other.describe()),
        }
    }

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
