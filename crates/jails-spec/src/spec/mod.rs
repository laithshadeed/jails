//! What a jails project is made of, below anything that generates.
//!
//! One question, and it is not "what Java should I write": **where is the
//! project, and where inside it does a class go** ([`paths`]). The rest of
//! this module is the small tables that answer it -- a Maven
//! [`coordinate`], a generated [`constant`], the [`suffix`] a kind's
//! principal type carries, and the typed evolution [`policy`] a rename asks
//! for.
//!
//! It sits below the generators because every layer beneath them asks --
//! `model`, `config`, `compose`, `project` and `inspect` all do. Answered
//! from the generator layer, those modules reach upward, and a cycle is a
//! boundary nothing can enforce.

pub mod constant;
pub mod coordinate;
pub mod paths;
pub mod policy;
pub mod suffix;

pub use paths::*;
