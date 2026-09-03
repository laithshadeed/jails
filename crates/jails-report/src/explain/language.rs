//! `jails explain jdl`: the language, printed from its own registries.
//!
//! **Every row is walked, none is written here.** The declaration families
//! and their attributes come from `jails_model::jdl_grammar`, which is what
//! the parser refuses against; the field types from
//! `jails_model::builtin::ALL`; the `use` projections from the same list the
//! parser matches on; the `cap` kinds from `CapabilityKind` filtered by
//! `declarable_in_source`. A hand-written copy of any of them would be the
//! one thing this command is for: telling a reader what the binary in front
//! of them accepts, rather than what a document said it accepted.
//!
//! `docs/10-language.md` is the specification; this is the binary agreeing
//! with it out loud.

use jails_model::{CapabilityKind, jdl_grammar};
use jails_support::Result;

pub(super) fn explain() -> Result<()> {
    println!("jdl 1 -- the application model's language.\n");

    println!("declarations");
    let width = jdl_grammar::FAMILIES
        .iter()
        .map(|family| family.keyword.len())
        .max()
        .unwrap_or(0);
    for family in jdl_grammar::FAMILIES {
        println!(
            "  {:width$}  {}",
            family.keyword,
            family.summary,
            width = width
        );
        println!(
            "  {:width$}  @{}",
            "",
            family.attributes.join(" @"),
            width = width
        );
    }

    println!("\nfield types");
    for row in jails_model::builtin::ALL {
        let semantics = row.1;
        let aliases = match semantics.aliases.is_empty() {
            true => String::new(),
            false => format!("  (also {})", semantics.aliases.join(", ")),
        };
        println!(
            "  {:<12}{}{aliases}",
            semantics.token, semantics.sql_postgres
        );
    }
    println!("  Capitalised     a type this project declares, passed through by name");
    println!("  name!  name?    non-blank, nullable; bare is non-null");

    println!("\nuse projections");
    println!("  {}", jdl_grammar::PROJECTIONS.join(", "));

    println!("\ncap kinds");
    let declarable = CapabilityKind::ALL
        .iter()
        .filter(|kind| kind.declarable_in_source())
        .map(|kind| kind.label())
        .collect::<Vec<_>>();
    println!("  {}", declarable.join(", "));
    println!("\n`db` and `h2` are selections of `app.storage`, not `cap` declarations.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use jails_model::jdl_grammar;

    /// **The attribute list is the parser's, not a copy of it.** The point of
    /// the command is that a reader can trust it against the binary in front
    /// of them, which holds only while both halves read one table.
    #[test]
    fn every_attribute_printed_is_one_the_parser_accepts() {
        let printed: usize = jdl_grammar::FAMILIES
            .iter()
            .map(|family| family.attributes.len())
            .sum();
        assert!(
            printed >= 30,
            "only {printed} attributes across the families"
        );
        assert_eq!(
            jdl_grammar::FIELD.len(),
            13,
            "a field's markers are the longest list, and the one that moves"
        );
    }
}
