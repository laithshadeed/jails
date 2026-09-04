//! Workspace capture and exact-plan materialization.
//!
//! The compiler cannot read the filesystem. `jails_project::capture` reads
//! it once; this crate turns semantic desired bytes into one
//! content-addressed `PlanBundle`, verifies it and executes it.

mod execute;
pub mod fault;
mod invert;
mod materialize;
mod reader_facet;
mod reconcile;
mod relocate;
mod verify;

// The reader half lives in `jails-project`: capture, the document adapters
// and the three-way merge produce what this crate materializes and executes.
// Module code says `crate::capture`, `crate::documents` and `crate::merge`.
pub use execute::{
    Execution, PRECONDITION_STALE, advance_lock_and_base_for_formatted_files, execute,
};
pub use invert::invert;
pub(crate) use jails_project::{capture, documents, merge};
pub use materialize::{Restore, digest, materialize};
pub use relocate::{relocate, relocation_targets};
pub use verify::verify_bundle;
