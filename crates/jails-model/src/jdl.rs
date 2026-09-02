//! The JDL authoring frontend for [`crate::AppModel`].
//!
//! `.jails/model.jdl` is written in `jdl 1` and nothing else; [`v1`] is the
//! whole language, and [`parse`] is its one entry point. It lowers to the
//! closed [`crate::source`] shape and hands that to the linker, so the
//! frontend never becomes a second model.

pub mod v1;

pub use v1::parse;
