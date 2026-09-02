//! What the tests need and production does not.
//!
//! [`hold_cwd`]'s lock cannot be `#[cfg(test)]`: a `#[cfg(test)]` item is
//! invisible to *dependent* crates' tests, and the crates that need this lock
//! are not the crate that defines it. It does not belong in a production crate
//! either, because test infrastructure is not part of a shipped API. A crate
//! taken as a `[dev-dependency]` says what it is. Every test binary is its own
//! process, so each links one instance, which is exactly what the lock has to
//! cover.

/// The process-global current directory, as a lock.
///
/// Unit tests within one crate share a test binary and therefore one working
/// directory, so a test that calls `std::env::set_current_dir` must hold this
/// for the duration or it races every other test in the same process.
///
/// Private, because the only correct way to take it is [`hold_cwd`] and a
/// `pub static` invites `.lock().unwrap()`.
static CWD_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Hold the working directory for the rest of this scope.
///
/// A holder that panicked does not make this lock unusable, and that is the
/// point of this function: `.lock().unwrap()` reports a poisoned mutex as a
/// `PoisonError` in the *next* test to ask for it, which is a different test
/// from the one that failed and names neither the panic nor its cause.
///
/// Recovering is sound here rather than merely convenient: this mutex guards
/// no data (it is a `Mutex<()>`), and every holder captures the current
/// directory and sets its own before doing anything, so there is no state a
/// panicking holder can leave behind for the next one to trip over.
pub fn hold_cwd() -> std::sync::MutexGuard<'static, ()> {
    CWD_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
