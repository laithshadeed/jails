//! What a jails project *is*, below anything that generates into one.
//!
//! Four questions, none of which is "what Java should I write":
//!
//! - Which build tool owns this directory, and where is its root ([`build`],
//!   [`spec::paths`]).
//! - What is the conventional package for each layer ([`spec::layout`]).
//! - Which Java and Spring Boot release a generated project is pinned to
//!   ([`release`]).
//!
//! These live here rather than in `generate.rs` because everything below the
//! generators needs them -- `model`, `config`, `compose`, `project` and
//! `inspect` all ask at least one of them. Answering from `generate.rs`
//! makes those modules reach upward, and twelve of them become one strongly
//! connected component with no boundary anything can enforce.
//!
//! [`build`] recognises a build file and never reads one. That is the line it
//! does not cross, and it is why invoking Maven lives in `jails-project`
//! instead.

pub mod build;
pub mod release;
pub mod spec;
