//! The values every closed jails format is built from.
//!
//! plan.md §R1.1: *"`Recipe`, `Name`, `Package`, `FieldSpec`, `IndexSpec`,
//! `CapabilityId` and `ProjectPath` are types, not string aliases … Their
//! constructors are the only place that accepts strings, and every wire
//! decoder calls the same constructors."*
//!
//! That last clause is the load-bearing one. A decoder with its own idea of a
//! valid path is a second validator, and two validators drift — which is how a
//! value rejected at the CLI arrives through a recovered journal instead.
//! There is one constructor per type and the codec calls it.
//!
//! ## Four groups, not twenty-three modules
//!
//! Every module here has a genuinely distinct secret and says so. What a reader
//! arriving at this file did not get was any *shape*: a flat list of
//! twenty-three names, alphabetical by accident. `pending.md` §7.4 groups them
//! by the question they answer, and the grouping is a claim rather than
//! filing — a type that belonged in two of these would be a type doing two
//! jobs.
//!
//! - [`vocabulary`] — what a value is allowed to be. Validating newtypes,
//!   closed sets, one constructor each.
//! - [`observe`] — what a planner may know. Observations, never assertions.
//! - [`intent`] — what is being asked for. Nothing here has met a disk.
//! - [`durable`] — what survives a crash. The one group with files behind it.
//!
//! They are **submodules, not crates**: mechanical, compiler-checked, and free
//! to undo. Every module is re-exported at the root below, so
//! `jails_protocol::identity::Name` still resolves and the grouping cost no
//! call site anything. Promote a group to a crate only where the split would
//! enforce an edge that matters; on the evidence exactly one would, and
//! [`durable`]'s own header says which.

pub mod compatibility;
mod durable;
mod intent;
mod observe;
mod vocabulary;

// Flat at the root, grouped in the source. The groups above are for a reader;
// `jails_protocol::identity::Name` is what four hundred call sites say, and
// renaming those would have made a filing decision look like an API change.
pub use durable::{conflict, envelope, lifecycle, pending, record};
pub use intent::{change, edit, effect, ownership, plan, render, request, transition};
pub use observe::{bootstrap, context, fact, provenance, resource_status, snapshot};
pub use vocabulary::{
    application, coordinate, database, declaration, editor, entity, feature, identity, recipe,
    resource,
};

pub(crate) use jails_support::Result;
