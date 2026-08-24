//! Turning semantic desire into an exact executable transition.
//!
//! plan.md §R3: *"R3 finishes every renderer, splice, merge, formatter and
//! report decision so R4 contains no domain logic."* That is the whole shape
//! of this crate — everything a commit needs to decide is decided here, and
//! the executor's job is reduced to applying a value it is handed.
//!
//! It remains plan-only. Nothing in here creates `.jails/`, migrates state or
//! commits an operation; R4 adds the executor that does.

pub mod command;
pub mod desire;
pub mod merge;
pub mod operation;
pub mod pipeline;
pub mod prepare;
pub mod receipt;
pub mod reconcile;
pub mod recovery;
pub mod report;
pub mod sandbox;
pub mod serialize;
pub mod tool;

pub(crate) use jails_support::Result;
