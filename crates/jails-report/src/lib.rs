//! Commands that answer a question about a project. **Read-only by contract.**
//!
//! [`doctor`] says what is wrong and never fixes it, so it stays safe to run
//! mid-debug. [`why`] turns a log into a cause. [`explain`] says why an
//! artifact is shaped the way it is. [`source`] says where a type lives.
//! [`commands`] walks the same `clap::Command` that parses the arguments, so
//! there is no second list of what jails can do.
//!
//! **The contract is structural, not a promise.** This crate cannot depend on
//! `jails-drive` -- `jails-drive` depends on *it*, because `jails run` scans
//! its own output through [`why`] -- so a reporting command that started
//! something would not compile.

pub mod commands;
mod diagnostic;
pub mod doctor;
pub mod explain;
pub mod source;
pub mod why;
pub mod why_subject;

// The lower crates, re-exported so every module in this one keeps saying
// `crate::…` wherever it ships. Each symbol is taken from the crate that owns
// it.
pub(crate) use jails_java::{java, template};
pub(crate) use jails_model::{ArtifactKind, CapabilityKind};
pub(crate) use jails_project::layout;
pub(crate) use jails_project::{compose, inspect, maven, model, pom};
pub(crate) use jails_spec::spec::paths::find_project_root;
pub(crate) use jails_spec::{build, release};
pub(crate) use jails_support::{apply, json, process};
