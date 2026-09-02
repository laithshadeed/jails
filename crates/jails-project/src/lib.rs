//! One resolved project, and everything jails records about it.
//!
//! [`model::Project`] is the parameter object the generators take: it caches
//! the pom once, applies the project's `[layout]` renames, and answers where a
//! class goes -- so a renderer never re-derives a fact from a `&Path`.
//!
//! The files jails writes *about* a project live here too, and they divide by
//! who owns them. [`config`] and [`compose`] are the reader's, edited by
//! byte-preserving splice.
//!
//! [`maven`] is how to invoke this project's Maven, deliberately separate from
//! `jails_spec::build`, which recognises a build file and never runs one, and
//! from [`pom`], which is the workspace crate's one reader of what a POM says.
//!
//! [`inspect`] reads the project's source to report routes and beans. It is
//! here rather than with the commands because `add`'s HTTP capability derives
//! its client from the same route list, and a command layer above the
//! generators could not be reached from there.

/// The marked-block splice, from its own dependency-free crate so that every
/// tree can reach one implementation: a format with several owners is several
/// answers to what `remove db` deletes. Module code says `crate::codemod`.
pub use jails_codemod as codemod;
pub mod capability;
pub mod compose;
pub mod config;
pub mod feature;
pub mod gradle;
pub mod inspect;
pub mod maven;
pub mod model;
pub mod modernize;
pub mod project;
pub mod properties;
pub mod synonyms;

// The lower crates, re-exported so every module in this one keeps saying
// `crate::…` wherever it ships. Only this block knows which crate a module
// actually lives in, which is what makes moving one a one-line change.
/// The one reader of `pom.xml`, from the crate that also writes it.
/// Module code says `crate::pom`.
pub use jails_workspace::pom;

pub use jails_java::{java, template};
/// The eleven layers, from the crate that owns every closed vocabulary.
/// Module code says `crate::layout`.
pub use jails_model::layout;
pub use jails_spec::{build, release, spec};
pub(crate) use jails_support::{json, process};
