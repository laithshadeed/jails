//! **What a planner may know.** Facts read from a project, and nothing else.
//!
//! A snapshot, the facts derived from it, the order a project must be read in,
//! the context a plan rests on, and who rendered what. Every value here is an
//! *observation*: it says what was there, never what should be.
//!
//! The split from [`crate::intent`] is the one that matters. A planner reads
//! these and writes those, and a type that appeared in both would be a fact
//! that could be asserted -- which is the shape of a plan that justifies itself.

pub mod bootstrap;
pub mod context;
pub mod fact;
pub mod provenance;
pub mod snapshot;
