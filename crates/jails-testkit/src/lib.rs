//! What the tests need and production does not.
//!
//! One item so far, and it is here for a reason that took a while to state.
//! [`CWD_LOCK`] lived at the bottom of `jails-support`, in production, with a
//! doc comment explaining correctly why it could not be `#[cfg(test)]`: a
//! `#[cfg(test)]` item is invisible to *dependent* crates' tests, and the
//! crates that need this lock are not the crate that defines it.
//!
//! That reasoning is sound and the placement was still wrong -- it made a piece
//! of test infrastructure part of the shipped API of the lowest layer, where
//! `pending.md` §7.5's own boundary rule says only things that "would still
//! make sense in a tool that had never heard of Maven" belong. A crate taken as
//! a `[dev-dependency]` says what it is instead of hiding it, and the scope is
//! unchanged: every test binary is its own process, so each links one instance,
//! which is exactly what the lock has to cover.

/// The process-global current directory, as a lock.
///
/// Unit tests within one crate share a test binary and therefore one working
/// directory, so a test that calls `std::env::set_current_dir` must hold this
/// for the duration or it races every other test in the same process.
pub static CWD_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
