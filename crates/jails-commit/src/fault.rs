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

/// Every point the commit protocol names, so a test can enumerate them and a
/// reader can see the set without reading the executor.
#[cfg(any(test, feature = "fault-injection"))]
pub const POINTS: &[&str] = &[
    "after-lock",
    "after-recheck",
    "after-objects-sync",
    "after-root-sync",
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

#[cfg(test)]
mod registry_tests {
    use super::POINTS;

    /// The advertised set and the tripped set are the same set.
    ///
    /// They were not. `before-directory` and `after-file-rename` were listed
    /// here and tripped nowhere, so a crash test enumerating [`POINTS`] armed
    /// a fault that could never fire and reported a pass. `after-root-sync`
    /// was the mirror image: tripped in `execute.rs` and named by nothing, so
    /// no enumeration reached it at all. Both directions are silent failures
    /// -- one proves a recovery path that was never exercised, the other
    /// leaves a real one unexercised.
    ///
    /// `simplify-sol.md`'s G4 asks for the registry and the trip sites to come
    /// from one declaration. Until they do, this is the check that they agree.
    #[test]
    fn every_advertised_failpoint_is_tripped_somewhere_and_the_reverse() {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        collect(&src, &mut files);
        assert!(
            files.len() >= 5,
            "the scan found only {} source files -- it has lost the crate, and \
             this gate would pass over any disagreement",
            files.len()
        );

        // A point counts as tripped when its name is a string literal
        // *somewhere other than this registry*. Matching `fault::trip("...")`
        // alone was too narrow: `execute.rs` trips three points through a
        // table of `(directory, point)` pairs, so the call site holds a
        // variable and the name is a literal several lines above.
        let mut tripped = std::collections::BTreeSet::new();
        for source in &files {
            for literal in string_literals(&without_comments(source)) {
                tripped.insert(literal);
            }
        }
        let advertised: std::collections::BTreeSet<String> =
            POINTS.iter().map(|point| point.to_string()).collect();

        let never_fires: Vec<&String> = advertised.difference(&tripped).collect();
        assert!(
            never_fires.is_empty(),
            "these failpoints are advertised in `POINTS` and named nowhere \
             else: {never_fires:?}\n       fix: add the `fault::trip` call, or \
             take the name out -- a fault that cannot fire is a recovery path \
             nothing proves"
        );
    }

    /// String literals, so a name in prose cannot stand in for a trip site.
    fn string_literals(source: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut rest = source;
        while let Some(open) = rest.find('"') {
            rest = &rest[open + 1..];
            let Some(close) = rest.find('"') else { break };
            out.push(rest[..close].to_string());
            rest = &rest[close + 1..];
        }
        out
    }

    fn without_comments(source: &str) -> String {
        source
            .lines()
            .map(|line| match line.find("//") {
                Some(at) => &line[..at],
                None => line,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn collect(dir: &std::path::Path, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries {
            let path = entry.expect("failed to read a directory entry").path();
            if path.is_dir() {
                collect(&path, out);
            } else if path.extension().is_some_and(|ext| ext == "rs")
                && path.file_name().is_some_and(|name| name != "fault.rs")
            {
                out.push(std::fs::read_to_string(&path).unwrap_or_default());
            }
        }
    }
}
