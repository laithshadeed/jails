//! Named points a test can make the executor die at, and nothing in a release
//! build.
//!
//! ## The property
//!
//! This executor has no journal and no rollback, on purpose: it trades
//! rollback away for convergence. A crashed command may leave a temporarily
//! mixed but individually valid tree, and the next identical generation
//! repairs it deterministically. So what a crash test asserts is that
//! sentence: re-running *the same plan* after a death at any instant reaches
//! the exact desired state, and a second run changes nothing. The point set
//! is this executor's publication sequence.
//!
//! ## Compiled out
//!
//! Under `cfg(not(any(test, feature = "fault-injection")))` `trip` is an
//! inlined `Ok(())` with no state behind it. A failpoint mechanism that reads
//! an environment variable in a shipped binary is a way to make somebody's
//! project stop halfway.

/// Arm a failpoint for the duration of a scope.
#[cfg(any(test, feature = "fault-injection"))]
pub struct Armed;

#[cfg(any(test, feature = "fault-injection"))]
mod armed {
    use std::cell::{Cell, RefCell};

    thread_local! {
        /// Thread-local, so two tests running in parallel cannot arm each
        /// other's failpoints.
        pub(super) static ARMED: RefCell<Option<String>> = const { RefCell::new(None) };
        /// Whether the armed point kills the process instead of returning.
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
    /// gets to handle: the flock releases, guards clean up, buffers flush.
    /// None of that happens when a machine loses power, and a convergence
    /// proof built only on the unwinding case is proving the easier half.
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
pub(crate) fn trip(name: &str) -> Result<(), String> {
    let armed = armed::ARMED.with(|slot| slot.borrow().clone());
    match armed {
        Some(armed) if armed == name => {
            if armed::ABORTS.with(|slot| slot.get()) {
                // No unwinding, no flush, no destructor. The point.
                std::process::abort();
            }
            Err(format!("fault injected at `{name}`"))
        }
        _ => Ok(()),
    }
}

/// Nothing. Compiled out of a release build entirely.
#[cfg(not(any(test, feature = "fault-injection")))]
#[inline(always)]
pub(crate) fn trip(_name: &str) -> Result<(), String> {
    Ok(())
}

/// One declaration, two products, against two silent failures.
///
/// A point advertised and tripped nowhere arms a fault that can never fire, so
/// an enumeration reports a pass over a path nothing exercised. Here its
/// constant has no user and `-D dead-code` says so.
///
/// A point tripped and advertised nowhere is unreachable by any enumeration.
/// Here it cannot be written: [`trip`] takes a constant, and every constant is
/// in [`POINTS`] by construction.
macro_rules! failpoints {
    ($($name:ident = $wire:literal,)+) => {
        /// Every instant this executor can be interrupted at, so a crash test
        /// can enumerate them and a reader can see the sequence without
        /// reading `execute.rs`.
        #[cfg(any(test, feature = "fault-injection"))]
        pub const POINTS: &[&str] = &[$($wire,)+];

        /// The name of each point, and the only thing [`trip`] accepts.
        pub(crate) mod point {
            $(pub(crate) const $name: &str = $wire;)+
        }
    };
}

failpoints! {
    // The lock is held and nothing is written. A crash here must leave the
    // project exactly as it was, including a lock file whose owner is gone.
    AFTER_LOCK = "after-lock",
    // Preconditions verified. Still nothing written, but the plan has been
    // judged against the tree -- so a re-run has to judge it again rather
    // than trust that it once passed.
    AFTER_PRECONDITIONS = "after-preconditions",
    // Around publishing the managed tree, which is the operation that writes
    // the most files and the one a partial run is most visible in.
    BEFORE_TREE = "before-tree",
    AFTER_TREE = "after-tree",
    // Around one file's atomic write. `before` is the interesting one: the
    // temporary exists and the rename has not happened.
    BEFORE_FILE = "before-file",
    AFTER_FILE = "after-file",
    // Around one deletion -- both the reader-file removal and the prune of a
    // managed file the new tree no longer has. Deletion is the operation a
    // re-run cannot repeat: the second attempt finds it already absent, so
    // convergence here is a claim about the *observed* state rather than
    // about the write having happened once.
    BEFORE_REMOVE = "before-remove",
    AFTER_REMOVE = "after-remove",
    // Everything is written and nothing has checked it. A crash here leaves a
    // tree that is complete and unverified, which is the state a reader would
    // most easily mistake for a finished run.
    BEFORE_VERIFY = "before-verify",
}
