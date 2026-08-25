//! Reading a project's machine state without changing it.
//!
//! ## Why a facade, and why read-only
//!
//! One reader, used by every command, that answers "does this project have a
//! ledger" without touching it. A `doctor` run, an `app plan`, a `--pretend`
//! all read machine state, and none of them may leave a project different for
//! having been inspected. That rule is why this is a separate module rather
//! than an inline `read_to_string`: the two readers that *did* clean up while
//! looking were second writers, and nothing said so.
//!
//! ## Why classification is a value rather than a `bool`
//!
//! "Does this project have a ledger" has three answers a caller must treat
//! differently: nothing yet, a store this binary can read, and machine state
//! it cannot. Collapsing the last two is the fail-open bug: an unreadable
//! store treated as an empty one silently offers to regenerate a project's
//! whole contents.
//!
//! There used to be a fourth -- a schema-1 store, translated in memory and
//! migrated by the first schema-2 commit. It is gone. jails is not released,
//! so no project in the world holds a schema-1 ledger this binary did not
//! write, and carrying a second format forward cost a parser, a translator, an
//! adoption route, and a `doctor` warning on every entity of every freshly
//! generated project.

use jails_protocol::envelope::LedgerV2;
use jails_support::Result;
use std::path::Path;

/// What a project's machine state is, right now, unmodified.
#[derive(Clone, Debug)]
pub enum MachineState {
    /// No machine state at all. The ordinary state of a project jails has
    /// never touched.
    Absent,
    /// The store this binary writes, read successfully.
    ///
    /// Boxed: a decoded ledger is far larger than either other answer, and
    /// unboxed it would size every `MachineState` -- including the `Absent`
    /// that most reads produce.
    Current(Box<LedgerV2>),
    /// Present and unreadable. Deliberately distinct from `Absent`: treating
    /// an unreadable store as an empty one would silently offer to regenerate
    /// a project's whole contents.
    Unreadable(String),
}

impl MachineState {
    /// The store to plan against, or the reason there is none.
    pub fn ledger(&self) -> Result<&LedgerV2> {
        match self {
            Self::Current(ledger) => Ok(ledger),
            Self::Absent => Err(jails_support::Failure::Told(
                "this project has no jails state yet.\n       fix: nothing recorded means \
                 nothing to reconcile against."
                    .to_string(),
            )),
            Self::Unreadable(why) => Err(jails_support::Failure::Told(why.clone())),
        }
    }

    /// A sentence for a report.
    pub fn describe(&self) -> String {
        match self {
            Self::Absent => "no jails state".to_string(),
            Self::Current(_) => "current".to_string(),
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
    let path = root.join(".jails/ledger.toml");
    let source = match std::fs::read_to_string(&path) {
        Ok(source) => source,
        // No store. `.jails` existing is not evidence of one: a project that
        // has only ever been *prepared* has an objects directory and a lock
        // and nothing to plan against.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return MachineState::Absent;
        }
        // Fail closed. An unreadable store is not an empty one, and the
        // difference is a project's whole contents.
        Err(error) => return MachineState::Unreadable(error.to_string()),
    };
    match LedgerV2::parse_file(&source) {
        Ok(ledger) => MachineState::Current(Box::new(ledger)),
        // One format, so one message. A store this binary cannot read is a
        // store a *different* jails wrote, and the honest instruction is to
        // say which file and let the reader decide -- not to guess at an older
        // schema and translate what it thinks it found.
        Err(why) => MachineState::Unreadable(format!(
            "{} cannot be read by this jails: {why}\n       fix: it was written by a different \
             version. Upgrade to, or use, the jails version that wrote it; this version will not \
             treat unknown state as empty.",
            path.display()
        )),
    }
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

    /// Treating an unreadable store as an empty one would silently offer to
    /// regenerate a project's contents.
    #[test]
    fn a_store_that_cannot_be_parsed_is_unreadable_never_absent() {
        let scratch = project();
        apply::put(scratch.path().join(".jails/ledger.toml"), "not a ledger\n").unwrap();
        let state = read(scratch.path());
        assert!(matches!(state, MachineState::Unreadable(_)), "{state:?}");
        assert!(state.ledger().is_err());
        scratch.close().unwrap();
    }

    #[test]
    fn a_newer_store_schema_fails_closed_with_an_upgrade_instruction() {
        let scratch = project();
        let source = jails_protocol::envelope::render(b"payload")
            .unwrap()
            .replace("schema = 2", "schema = 3");
        apply::put(scratch.path().join(".jails/ledger.toml"), source).unwrap();
        let state = read(scratch.path());
        let MachineState::Unreadable(why) = state else {
            panic!("newer state was not unreadable")
        };
        assert!(why.contains("schema 3"), "{why}");
        assert!(why.contains("Upgrade"), "{why}");
        assert!(
            why.contains("will not treat unknown state as empty"),
            "{why}"
        );
        scratch.close().unwrap();
    }

    /// A `.jails` holding only preparation state is not a store. Reporting one
    /// would make every freshly prepared project look like it had a history.
    #[test]
    fn preparation_state_without_a_ledger_is_still_absent() {
        let scratch = project();
        apply::put(scratch.path().join(".jails/lock"), "").unwrap();
        apply::put(scratch.path().join(".jails/objects/.keep"), "").unwrap();
        assert!(matches!(read(scratch.path()), MachineState::Absent));
        scratch.close().unwrap();
    }
}
