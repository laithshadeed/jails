//! Making a `PreparedChange` durable without pretending several filesystem
//! names change at once.
//!
//! plan.md §R4: *"Default crash recovery rolls a fully persisted, validated
//! journal **forward**."* Preimages exist for a guarded explicit abort and
//! for audit — not as the crash policy. That choice is what keeps the
//! executor small: there is one direction to finish in, and a recovered
//! journal either has everything it needs to finish or was never valid.

pub(crate) mod activate;
pub mod execute;
pub mod fault;
pub mod gc;
pub mod journal;
pub mod outcome;
pub mod recover;
pub mod runtime;
pub mod store;

pub(crate) use jails_support::Result;
