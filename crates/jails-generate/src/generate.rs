//! The write path every emitted file goes through, and the vocabulary below it.
//!
//! **This file was `abstract.md` §3.2's named anti-pattern verbatim** — parse,
//! dispatch, write, side effects in one place — and the first three are gone.
//! `jails-compiler` emits all thirty-nine advertised kinds and every mutating
//! command seeds a model before it runs, so nothing dispatched to a per-kind
//! generator any more; the submodules that held them, and the `spring` and
//! `add` trees below them, are deleted.
//!
//! What is left is the half that was never legacy. The rules live in the write
//! path rather than in each caller for one reason: a rule twenty templates have
//! to remember is a rule that decays the first time somebody adds the
//! twenty-first. Import normalisation, `package-info.java` planning,
//! `ensure_failsafe` and `ensure_assertj` are all keyed off the emitted bytes,
//! so a new emitter cannot forget them.
//!
//! `ArtifactKind` is a `clap::ValueEnum` and must stay one — it is the only
//! shape a static completion list can be generated from.
//!
//! Everything below the generators once reached *up* into this file for
//! `Field`, `layout` and `find_project_root`, which is what made `src/` a
//! twelve-module cycle. `jails-spec` is those symbols at their own layer, and
//! the re-exports below are what kept the move a one-line change instead of a
//! sweep.

use crate::model::Project;

// The vocabulary below the generator layer. Re-exported so `generate::Field`
// and `generate::main_dir` still resolve for every caller inside this layer;
// what moved is where they are *defined*, and therefore which way the
// dependency points. See `crate::spec`.
pub use crate::spec::kind::ArtifactKind;
pub use crate::spec::layout;
pub use crate::spec::{field::*, paths::*};
// The one parser, which lives with `FieldSpec` a layer up rather than with the
// `Field` it produces -- `pending.md` §6.3.
pub use jails_protocol::declaration::parse_fields;
// The name a recipe records under, which is an *identity* rule and so belongs
// with the vocabulary rather than with the generators that read it --
// `pending.md` §6.4.
pub use jails_protocol::recipe::{kind_suffix, recorded_name, strip_redundant_suffix};

mod cli;
pub use cli::*;

mod write;
pub use write::*;

/// `Order` -> `order`, for a Java identifier derived from a type name.
///
/// It lived in `generate/web.rs` beside its first caller and outlived it:
/// `jails-report`'s schema lineage still names a column's owning field this
/// way. Kept here rather than there for the rule `CLAUDE.md` states about
/// crates and applies to modules too — vocabulary a surviving caller needs
/// must not live in something that dies.
pub fn lower_first(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) => first.to_lowercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// Every constant of a project enum, in declaration order.
///
/// Read off the file rather than remembered, for the same reason the sample
/// is: jails holds no type model, and a made-up constant produces SQL that
/// looks right and rejects a value the Java enum accepts.
///
/// `None` means "not an enum jails can see", which is different from "an enum
/// with no constants" -- the caller must not emit a `check (... in ())` for
/// either, and only the first is a case where jails simply does not know.
pub(crate) fn enum_constants(project: &Project, pkg: &str, type_name: &str) -> Option<Vec<String>> {
    let source = project.source_of(pkg, type_name)?;
    let source = source.as_str();
    let text = crate::java::blanked(source);
    let body = text.find(&format!("enum {type_name}"))?;
    let open = text[body..].find('{')? + body + 1;
    // Constants come first in an enum body and end at the first `;` or `}`.
    let end = text[open..]
        .find([';', '}'])
        .map(|o| open + o)
        .unwrap_or(text.len());
    let constants: Vec<String> = source
        .get(open..end)?
        .split(',')
        // A constant with a wire value is `OPEN("open")`, and the name is the
        // half before the parenthesis. Reading the whole token would put
        // `OPEN("open")` in a `check (... in (...))`, which fails at
        // `flyway migrate` on whichever machine runs it first.
        .map(|token| token.trim().split('(').next().unwrap_or("").trim())
        .filter(|token| {
            !token.is_empty()
                && token
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_uppercase() || c == '_')
        })
        .map(|token| {
            // `GBP("British Pound")` -- the constant is the name, not the
            // whole declaration.
            token
                .split(['(', ' ', '{'])
                .next()
                .unwrap_or(token)
                .to_string()
        })
        .collect();
    (!constants.is_empty()).then_some(constants)
}
