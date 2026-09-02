//! Which build language a module is written in.
//!
//! A closed set of two, plus the answer for a directory that has neither.
//! JDL v1 §22 has an unsupported build language abort an upgrade rather than
//! be guessed, which is why [`BuildSystem::Unknown`] is a member here and not
//! an `Option` at each call site: "jails looked and it was neither" is a fact
//! the snapshot records, and a `None` that could equally mean "nobody asked"
//! is not the same fact.
//!
//! `jails_spec::build::Build` is a different question -- what a *directory*
//! looks like from outside, including a build file jails recognises by name
//! and will not read -- and it keeps its own answer.

use serde::{Deserialize, Serialize};

/// The build language a module's `build` axis names.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum BuildSystem {
    Maven,
    Gradle,
    /// Neither, observed. Not "not yet asked": every reader of this value has
    /// already looked.
    Unknown,
}
