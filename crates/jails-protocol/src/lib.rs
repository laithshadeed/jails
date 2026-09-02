//! The plan, transition and effect vocabulary.
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
//! **Half the types §R1.1 names are one crate lower now.** `Name`, `Package`
//! and `ProjectPath` are in [`jails_support::identity`]: they know nothing
//! about a plan, and the crates that outlive the cutover need them without
//! depending on this one, which dies with the legacy engine. The rule above is
//! unchanged and travelled with them — it was never about *which* crate holds
//! a constructor, only that there is one. `jails_protocol::identity::Name`
//! still resolves, through the re-export below.
//!
//! ## Five groups, not a flat list
//!
//! Every module here has a genuinely distinct secret and says so. What a reader
//! arriving at this file did not get was any *shape*: a flat list of names,
//! alphabetical by accident. `pending.md` §7.4 groups them by the question they
//! answer, and the grouping is a claim rather than filing — a type that
//! belonged in two of these would be a type doing two jobs.
//!
//! - `vocabulary` — what a value is allowed to be. Closed sets and the
//!   recipe/entity/resource values, one constructor each.
//! - `observe` — what a planner may know. Observations, never assertions.
//! - `intent` — what is being asked for. Nothing here has met a disk.
//! - `durable` — what survives a crash. The one group with files behind it.
//! - [`compatibility`] — what an older jails wrote, read once and never
//!   written again.
//!
//! They are **submodules, not crates**: mechanical, compiler-checked, and free
//! to undo. Every module is re-exported at the root below, so the grouping cost
//! no call site anything. Promote a group to a crate only where the split would
//! enforce an edge that matters — which is exactly what happened to `identity`,
//! and `durable`'s own header says which group is next.

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
