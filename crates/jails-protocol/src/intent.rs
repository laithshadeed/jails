//! **What is being asked for.** A request, and everything it becomes before
//! anything is written.
//!
//! One request becomes a change, a plan, a transition and a set of edits and
//! effects. None of it has touched a disk: these are values a `--pretend` can
//! print and a journal can record, which is what makes the two the same thing.

pub mod change;
pub mod edit;
pub mod effect;
pub mod ownership;
pub mod plan;
pub mod render;
pub mod request;
pub mod testing;
pub mod transition;
