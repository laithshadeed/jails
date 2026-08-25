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
    use std::cell::RefCell;

    thread_local! {
        /// Thread-local, so two tests running in parallel cannot arm each
        /// other's failpoints — which would make the suite flaky in exactly
        /// the way a crash suite must not be.
        pub(super) static ARMED: RefCell<Option<String>> = const { RefCell::new(None) };
    }
}

#[cfg(any(test, feature = "fault-injection"))]
impl Armed {
    /// Arm one failpoint. Disarmed when the returned value is dropped, so a
    /// panicking test cannot leak it into the next one.
    pub fn at(name: &str) -> Self {
        armed::ARMED.with(|slot| *slot.borrow_mut() = Some(name.to_string()));
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
        Some(armed) if armed == name => Err(format!("fault injected at `{name}`").into()),
        _ => Ok(()),
    }
}

/// Nothing. Compiled out of a release build entirely.
#[cfg(not(any(test, feature = "fault-injection")))]
#[inline(always)]
pub(crate) fn trip(_name: &str) -> crate::Result<()> {
    Ok(())
}

/// Every point the commit protocol names, so a test can enumerate them and a
/// reader can see the set without reading the executor.
#[cfg(any(test, feature = "fault-injection"))]
pub const POINTS: &[&str] = &[
    "after-lock",
    "after-recheck",
    "after-objects-sync",
    "after-journal-prepared",
    "after-journal-active",
    "before-directory",
    "after-directory-sync",
    "after-live-temp-sync",
    "before-file",
    "after-file-rename",
    "after-file-dirsync",
    "before-ledger",
    "after-ledger-rename",
    "after-ledger-dirsync",
    "after-journal-ledger-committed",
    "after-journal-complete",
    "after-receipt-sync",
    "before-receipt-move",
    "after-receipt-move",
    "after-transactions-parent-sync",
    "after-receipts-parent-sync",
];
