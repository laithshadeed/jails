//! One resolved project, and everything jails records about it.
//!
//! [`model::Project`] is the parameter object the generators take: it caches
//! the pom once, applies the project's `[layout]` renames, and answers where a
//! class goes -- so a renderer never re-derives a fact from a `&Path`.
//!
//! The files jails writes *about* a project live here too, and they divide by
//! who owns them. [`config`] and [`compose`] are the reader's, edited by
//! byte-preserving splice. [`ledger`] and [`generated_files`] are jails' own
//! bookkeeping and are never hand-edited.
//!
//! [`maven`] is how to invoke this project's Maven, deliberately separate from
//! `jails_spec::build`, which recognises a build file and never runs one.
//!
//! [`inspect`] reads the project's source to report routes and beans. It is
//! here rather than with the commands because `add`'s HTTP capability derives
//! its client from the same route list, and a command layer above the
//! generators could not be reached from there.

pub mod application_manifest;
pub mod capability;
pub mod capture;
pub mod codemod;
pub mod compose;
pub mod config;
pub mod generated_files;
pub mod gradle;
pub mod inspect;
pub mod junit;
pub mod maven;
pub mod model;
pub mod modernize;
pub mod pom;
pub mod project;
pub mod projection;
pub mod properties;
pub mod query_compiler;
pub mod query_workspace;
pub mod schema;
pub mod synonyms;

// The lower crates, re-exported so every module in this one keeps saying
// `crate::…` wherever it ships. Only this block knows which crate a module
// actually lives in, which is what makes moving one a one-line change.
pub use jails_java::{java, template};
pub use jails_spec::{build, spec};
pub(crate) use jails_support::{json, process};

// `.jails/` reading lives one layer down now (`pending.md` §7.3). Re-exported
// so `crate::compat` keeps meaning what it always did.
pub use jails_state::compat;
