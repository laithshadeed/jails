//! Reading and translating jails' own machine state.
//!
//! One coherent thing: **what is under `.jails/`, and what a directory holds.**
//! Not what a Java project is -- that is `jails-project`, one layer up -- and
//! not how a transaction is committed, which is `jails-commit`, three above.
//!
//! It exists because `jails-commit` had five references reaching *up* into
//! `jails-project` (`compat::*` and `capture::list_directory`), and committing
//! a transaction is lower-level than knowing what Maven is. `jails-commit`'s
//! own header says its whole point is that the executor is small because there
//! is one direction to finish in; it could not be small while it also had to
//! know how `.jails/` is laid out. `pending.md` §7.3.

pub mod compat;
pub mod listing;

pub(crate) use jails_support::Result;
