//! **What survives a crash.** The file formats, as values.
//!
//! An envelope is bytes on a disk; a record is a row in one; a pending marker
//! and a conflict are what a stopped reconciliation leaves behind. This is the
//! one group whose members have a *file* behind them, which is why
//! `pending.md` §7.4 names it as the only one of the four with a case for
//! becoming a crate of its own -- it belongs with `jails-state`, and the rest
//! of this crate is values that never meet a disk.
//!
//! Most of `pending` and `conflict` is reached by nothing yet; see their
//! headers, and `pending.md` §11.

pub mod conflict;
pub mod envelope;
pub mod pending;
pub mod record;
