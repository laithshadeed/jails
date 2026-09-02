//! Workspace capture and exact-plan materialization.
//!
//! The compiler cannot read the filesystem. This crate captures it once and
//! turns semantic desired bytes into one content-addressed `PlanBundle`.

mod capture;
mod documents;
mod execute;
pub mod fault;
mod materialize;
mod merge;
mod reader_facet;
mod reconcile;
mod verify;

pub use capture::{
    capture, capture_import, capture_planned, observe_build_system, observe_spring_boot,
};
pub use documents::maven_dependency_block;
/// The one reader of `pom.xml`. Every question about a Maven build a
/// command asks outside capture is asked here, so a second scanner cannot
/// answer it differently.
pub use documents::pom;
pub use execute::{Execution, execute};
pub use materialize::{Restore, digest, materialize};
pub use verify::verify_bundle;
