//! Commands that answer a question about a project. **Read-only by contract.**
//!
//! [`doctor`] says what is wrong and never fixes it, so it stays safe to run
//! mid-debug. [`why`] turns a log into a cause. [`explain`] says why an
//! artifact is shaped the way it is. [`source`] says where a type lives.
//! [`commands`] walks the same `clap::Command` that parses the arguments, so
//! there is no second list of what jails can do.
//!
//! **The contract is structural now, not a promise.** These five lived one
//! `use` away from `run::mvn` in a crate that also drove Maven, started
//! containers and ran a JVM; `pending.md` §7.6 is the entry about it. This
//! crate cannot depend on `jails-drive` -- `jails-drive` depends on *it*,
//! because `jails run` scans its own output through [`why`] -- so a reporting
//! command that started something would not compile.
//!
//! Severing it took one deletion: `run::find_on_path` was a one-line alias for
//! `process::on_path`, and it was `doctor`'s only reason to name `run` at all.

pub mod commands;
pub mod doctor;
pub mod explain;
pub mod lifecycle_status;
pub mod source;
pub mod why;

// The lower crates, re-exported so every module in this one keeps saying
// `crate::…` wherever it ships.
pub(crate) use jails_generate::{add, generate};
pub(crate) use jails_java::{java, template};
pub(crate) use jails_project::{compose, inspect, maven, model, pom};
pub(crate) use jails_spec::build;
pub(crate) use jails_support::{apply, json, process};
