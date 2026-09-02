//! What a jails project is made of, below anything that generates.
//!
//! Three questions, none of which is "what Java should I write":
//!
//! - **Where is the project, and where inside it does a class go** ([`paths`]).
//! - **What is the conventional package for each layer** ([`layout`]).
//!
//! They sit below the generators because every layer beneath them asks at
//! least one of the three -- `model`, `config`, `compose`, `project` and
//! `inspect` all do. Answered from the generator layer, those modules reach
//! upward, and a cycle is a boundary nothing can enforce.

pub mod constant;
pub mod coordinate;
pub mod manifest;
pub mod paths;
pub mod policy;
pub mod suffix;

pub use paths::*;
