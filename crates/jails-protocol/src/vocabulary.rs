//! **Validating newtypes and closed sets.** What a value is allowed to be.
//!
//! Every type here has one constructor, and it is the only thing that accepts a
//! string. That is plan.md §R1.1's property and the reason the crate exists: a
//! decoder with its own idea of a valid path is a second validator, and two
//! validators drift -- which is how a value rejected at the CLI arrives through
//! a recovered journal instead.
//!
//! Nothing in this group touches a disk or knows what a plan is.

pub mod application;
pub mod coordinate;
pub mod database;
pub mod declaration;
pub mod entity;
pub mod feature;
pub mod identity;
pub mod recipe;
pub mod resource;
