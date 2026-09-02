//! Where a jails project is, and what builds it.
//!
//! Two questions, and neither of them is "what Java should I write":
//!
//! - Which build tool owns this directory, and where is its root ([`build`],
//!   [`spec::paths`]).
//! - Which Java and Spring Boot release a generated project is pinned to
//!   ([`release`]).
//!
//! These live here rather than in the generator layer because everything
//! below the generators needs them -- `model`, `config`, `compose`,
//! `project` and `inspect` all ask at least one. Answered from the generator
//! layer, those modules reach upward, and twelve of them become one strongly
//! connected component with no boundary anything can enforce.
//!
//! **No closed vocabulary lives here.** `Layer`, `CapabilityKind`,
//! `ArtifactKind` and their kin are `jails-model`'s, which is why this crate
//! no longer depends on clap; [`spec::suffix`] matches on the generator kind
//! it is given rather than owning the list.
//!
//! [`build`] recognises a build file and never reads one. That is the line it
//! does not cross, and it is why invoking Maven lives in `jails-project`
//! instead.

pub mod build;
pub mod release;
pub mod spec;
