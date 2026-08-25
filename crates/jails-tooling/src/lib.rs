//! Commands that drive a toolchain or report on a project.
//!
//! Everything here either shells out to something -- Maven, Docker Compose,
//! psql, a JVM -- or reads a project and prints what it finds. Nothing here
//! generates code, which is the line against the crate below.
//!
//! [`run`], [`launcher`] and [`testd`] are three ways to run the same tests,
//! fastest last, and [`affected`] is how `testd` decides which ones. [`doctor`]
//! is read-only by contract. [`why`] turns a log into a cause. [`commands`]
//! walks the same `clap::Command` that parses the arguments, so there is no
//! second list of what jails can do.

pub(crate) mod affected;
pub mod bench;
pub mod commands;
pub mod console;
pub mod doctor;
pub mod explain;
pub mod kafka;
pub(crate) mod launcher;
pub mod lint;
pub mod migrate;
pub(crate) mod reports;
pub mod run;
pub mod source;
pub mod testd;
pub mod why;

// The lower crates, re-exported so every module in this one keeps saying
// `crate::…` wherever it ships. Only this block knows which crate a module
// actually lives in, which is what makes moving one a one-line change.
pub(crate) use jails_generate::{add, generate};
pub(crate) use jails_java::{classfile, java, template};
pub(crate) use jails_project::{compose, inspect, junit, maven, model, pom};
pub(crate) use jails_spec::build;
pub(crate) use jails_support::{apply, json, process};
