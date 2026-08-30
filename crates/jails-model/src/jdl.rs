//! The single authoring frontend for [`crate::AppModel`].
//!
//! JDL lowers to the closed semantic source and then uses the same linker as
//! the model's own construction. It never becomes a second model.
//!
//! **There is one dialect.** A pre-v1 draft grammar and a `.jails/model.toml`
//! compatibility input lived beside it through the cutover, and each was a
//! second answer to the same question: the draft stated a field's label with a
//! separate `@as` pin, the TOML stated it as a table key, and v1 states it by
//! writing the declaration's name. Three spellings of one fact is how
//! `resource field rename` came to be impossible on a v1 source while passing
//! its test on a TOML one, and how `resource index add` came to render a
//! grammar the parser it fed rejects. Both are deleted; `git log -p --
//! crates/jails-model/src/jdl` is where they and the reasons live.

pub mod v1;

pub use v1::parse;
