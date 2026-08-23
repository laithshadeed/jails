//! What a jails project *is*, below anything that generates into one.
//!
//! Three questions, none of which is "what Java should I write":
//!
//! - Which build tool owns this directory, and where is its root ([`build`],
//!   [`spec::paths`]).
//! - What is the conventional package for each layer ([`spec::layout`]).
//! - What does a field spec mean ([`spec::field`]).
//!
//! All of it used to live in `generate.rs`, and every layer below the
//! generators reached up into it: `model`, `config`, `compose`, `project` and
//! `inspect` all did. That made twelve modules one strongly connected
//! component, which is a boundary nothing can enforce.
//!
//! [`build`] recognises a build file and never reads one. That is the line it
//! does not cross, and it is why invoking Maven lives in `jails-project`
//! instead.

pub mod build;
pub mod spec;

// The lower crates, re-exported so every module in this one keeps saying
// `crate::…` wherever it ships. Only this block knows which crate a module
// actually lives in, which is what makes moving one a one-line change.
pub(crate) use jails_java::java;
