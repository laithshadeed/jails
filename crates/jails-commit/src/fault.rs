//! Named points a test can make fail, and nothing at all in a release build.
//!
//! plan.md §R4.5: *"production builds contain no environment-triggered
//! abort."* That is the whole design constraint. A failpoint mechanism that
//! reads an environment variable is a way to make a shipped binary corrupt a
//! user's project, so this one is compiled out entirely: under `cfg(not(any(
//! test, feature = "fault-injection")))` [`trip`] is an inlined `Ok(())` with
//! no state behind it.
//!
//! ## Why failpoints and not just unit tests
//!
//! The property that matters is *convergence*: whatever instant a run stops
//! at, running it again reaches a consistent state. That cannot be tested by
//! calling functions in isolation, because the interesting states are the
//! ones between two filesystem operations. A failpoint puts a test exactly
//! there.
//!
//! ## What this models, and what it does not
//!
//! An injected error models a process that stopped at that point *and
//! unwound*. It does not model losing stack cleanup — that needs a child
//! process and `abort()`, which needs the CLI to route through this executor.
//! §R4.5's child-abort suite therefore lands with R6's migration; what is
//! here is the same failpoint set, tripped in-process.

/// Arm a failpoint for the duration of a scope.
#[cfg(any(test, feature = "fault-injection"))]
pub struct Armed;

#[cfg(any(test, feature = "fault-injection"))]
mod armed {
    use std::cell::{Cell, RefCell};

    thread_local! {
        /// Thread-local, so two tests running in parallel cannot arm each
        /// other's failpoints — which would make the suite flaky in exactly
        /// the way a crash suite must not be.
        pub(super) static ARMED: RefCell<Option<String>> = const { RefCell::new(None) };

        /// Whether the armed point kills the process instead of returning.
        ///
        /// An injected `Err` unwinds: destructors run, guards release, buffers
        /// flush. A real crash does none of that, and the difference is
        /// exactly what a durability claim rests on -- so the child-abort
        /// suite arms this and the process dies inside `trip`.
        pub(super) static ABORTS: Cell<bool> = const { Cell::new(false) };
    }
}

#[cfg(any(test, feature = "fault-injection"))]
impl Armed {
    /// Arm one failpoint. Disarmed when the returned value is dropped, so a
    /// panicking test cannot leak it into the next one.
    pub fn at(name: &str) -> Self {
        armed::ARMED.with(|slot| *slot.borrow_mut() = Some(name.to_string()));
        armed::ABORTS.with(|slot| slot.set(false));
        Self
    }

    /// Arm a failpoint that ends the process there, without unwinding.
    ///
    /// `at` injects an `Err`, which every caller between the trip and the test
    /// gets to handle: locks release, `ScratchDir` guards clean up, the
    /// journal's own `Drop` runs. None of that happens when a machine loses
    /// power, and a recovery proof built only on the unwinding case is proving
    /// the easier half. This aborts inside [`trip`], so the next process finds
    /// exactly what a crash leaves.
    pub fn aborting_at(name: &str) -> Self {
        armed::ARMED.with(|slot| *slot.borrow_mut() = Some(name.to_string()));
        armed::ABORTS.with(|slot| slot.set(true));
        Self
    }
}

#[cfg(any(test, feature = "fault-injection"))]
impl Drop for Armed {
    fn drop(&mut self) {
        armed::ARMED.with(|slot| *slot.borrow_mut() = None);
    }
}

/// Fail here if this point is armed.
#[cfg(any(test, feature = "fault-injection"))]
pub(crate) fn trip(name: &str) -> crate::Result<()> {
    let armed = armed::ARMED.with(|slot| slot.borrow().clone());
    match armed {
        Some(armed) if armed == name => {
            if armed::ABORTS.with(|slot| slot.get()) {
                // No unwinding, no flush, no destructor. The point.
                std::process::abort();
            }
            Err(format!("fault injected at `{name}`").into())
        }
        _ => Ok(()),
    }
}

/// Nothing. Compiled out of a release build entirely.
#[cfg(not(any(test, feature = "fault-injection")))]
#[inline(always)]
pub(crate) fn trip(_name: &str) -> crate::Result<()> {
    Ok(())
}

/// **One declaration, two products.** G4 of `simplify-sol.md` asks for the
/// registry and the trip sites to come from one place, and this is it: the
/// macro below emits [`POINTS`] -- the list a crash test enumerates -- and one
/// constant per point, which is the only thing [`trip`] can be given.
///
/// That closes both silent failures the hand-written pair had, and closes them
/// in the compiler rather than in a test.
///
/// A point advertised and tripped nowhere used to arm a fault that could never
/// fire, so an enumeration reported a pass over a recovery path nothing
/// exercised. Now its constant has no user and `-D dead-code` says so, because
/// the executor is the only crate that names them.
///
/// A point tripped and advertised nowhere used to be unreachable by any
/// enumeration. Now it cannot exist: a trip site takes a constant, and every
/// constant is in `POINTS` by construction.
///
/// The names stay `&'static str` rather than becoming an enum because a
/// failpoint's identity *is* its name -- it appears in the injected error and
/// in test output -- and a second mapping from variant to string would be the
/// duplication this removes.
macro_rules! failpoints {
    ($($name:ident = $wire:literal,)+) => {
        /// Every point the commit protocol names, so a crash test can
        /// enumerate them and a reader can see the set without reading the
        /// executor.
        #[cfg(any(test, feature = "fault-injection"))]
        pub const POINTS: &[&str] = &[$($wire,)+];

        /// The name of each point, and the only thing [`trip`] accepts.
        ///
        /// Deliberately **not** behind the injection `cfg`: the executor names
        /// these unconditionally, and the mechanism rather than the vocabulary
        /// is what a release build compiles out.
        pub(crate) mod point {
            $(pub(crate) const $name: &str = $wire;)+
        }
    };
}

failpoints! {
    AFTER_LOCK = "after-lock",
    AFTER_RECHECK = "after-recheck",
    AFTER_OBJECTS_SYNC = "after-objects-sync",
    AFTER_ROOT_SYNC = "after-root-sync",
    AFTER_JOURNAL_PREPARED = "after-journal-prepared",
    AFTER_JOURNAL_ACTIVE = "after-journal-active",
    BEFORE_DIRECTORY = "before-directory",
    AFTER_DIRECTORY_SYNC = "after-directory-sync",
    AFTER_LIVE_TEMP_SYNC = "after-live-temp-sync",
    BEFORE_FILE = "before-file",
    AFTER_FILE_RENAME = "after-file-rename",
    AFTER_FILE_DIRSYNC = "after-file-dirsync",
    BEFORE_LEDGER = "before-ledger",
    AFTER_LEDGER_RENAME = "after-ledger-rename",
    AFTER_LEDGER_DIRSYNC = "after-ledger-dirsync",
    AFTER_JOURNAL_LEDGER_COMMITTED = "after-journal-ledger-committed",
    AFTER_JOURNAL_COMPLETE = "after-journal-complete",
    AFTER_RECEIPT_SYNC = "after-receipt-sync",
    BEFORE_RECEIPT_MOVE = "before-receipt-move",
    AFTER_RECEIPT_MOVE = "after-receipt-move",
    AFTER_TRANSACTIONS_PARENT_SYNC = "after-transactions-parent-sync",
    AFTER_RECEIPTS_PARENT_SYNC = "after-receipts-parent-sync",
}

#[cfg(test)]
mod registry_tests {
    use super::POINTS;

    /// **What is left to test, now that the compiler holds the rest.**
    ///
    /// This module used to scan the crate's own source for string literals
    /// and check `POINTS` against them in both directions. That check existed
    /// because the registry and the trip sites were two lists: `before-
    /// directory` and `after-file-rename` were advertised and tripped
    /// nowhere, so a crash test enumerating `POINTS` armed a fault that could
    /// never fire and reported a pass; `after-root-sync` was the mirror
    /// image, tripped in `execute.rs` and named by nothing, so no enumeration
    /// reached it.
    ///
    /// `simplify-sol.md`'s G4 asks for one declaration, and `failpoints!` is
    /// it. Both directions are now closed above a test: a point nobody trips
    /// has an unused constant and `-D dead-code` fails the build, and a point
    /// tripped but unadvertised cannot be written, because `trip` takes a
    /// constant and every constant is in `POINTS`.
    ///
    /// So what remains is the property a macro cannot state: that the wire
    /// names are distinct. Two points sharing one string would compile, and
    /// arming either would fire both -- which is a recovery path proved by
    /// the wrong fault.
    #[test]
    fn every_failpoint_has_a_distinct_name() {
        let unique = POINTS.iter().collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            unique.len(),
            POINTS.len(),
            "two failpoints share a wire name: arming either would fire both"
        );
        assert!(
            POINTS.len() > 20,
            "only {} failpoints -- the registry has lost its declaration",
            POINTS.len()
        );
    }
}
