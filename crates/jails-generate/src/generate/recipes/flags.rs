//! Flags one recipe takes and the others do not, refused rather than ignored.
//!
//! A kind that silently drops a flag is `missing.md` M7's finding: `g client`
//! accepted `--method`, `--on` and `--returns` and generated the same CRUD
//! shape whatever they said, so the reader got a plausible artifact answering
//! a different question and nothing told them. Refusing costs one line and
//! removes the whole class.
//!
//! Kept out of `artifacts_for` because it is a different question: that
//! function maps a kind to the files it would write, and this decides whether
//! the request is one a kind can answer at all.

use super::*;

/// Refuse a flag that belongs to another recipe, and one pair that cannot be
/// combined.
pub(super) fn refuse_misplaced(recipe: &Recipe<'_>) -> Result<()> {
    only_for(
        recipe,
        ArtifactKind::Query,
        "`--via`",
        "the one recipe that reads a second table",
        recipe.via.is_some(),
    )?;
    only_for(
        recipe,
        ArtifactKind::Query,
        "`--order-by` and `--limit`",
        "the one recipe that returns a list",
        recipe.order_by.is_some() || recipe.limit.is_some(),
    )?;
    only_for(
        recipe,
        ArtifactKind::Usecase,
        "`--on-conflict`",
        "the recipe that creates a row",
        recipe.on_conflict.is_some(),
    )?;
    // The outbox wraps `Storing{X}UseCase` by name, and `--on-conflict`
    // replaces that class with a JdbcClient adapter. Refused rather than
    // silently picking one: an outbox over a get-or-create is a real thing to
    // want, and it is not this.
    if recipe.on_conflict.is_some() && recipe.strategy_yields.is_some() {
        return Err(jails_support::Failure::Told(
            "`--on-conflict` and `--yields` cannot be combined: the transactional outbox \
             delegates to the storing implementation, which a get-or-create replaces.\n       \
             fix: generate the use case with one or the other."
                .to_string(),
        ));
    }
    Ok(())
}

fn only_for(
    recipe: &Recipe<'_>,
    owner: ArtifactKind,
    flags: &str,
    why: &str,
    passed: bool,
) -> Result<()> {
    if !passed || recipe.kind == owner {
        return Ok(());
    }
    use clap::ValueEnum;
    let label = |kind: ArtifactKind| {
        kind.to_possible_value()
            .expect("every kind has a clap value")
            .get_name()
            .to_string()
    };
    Err(format!(
        "{flags} only applies to a {}, {why}.\n       fix: drop it from `jails g {} {}`.",
        label(owner),
        label(recipe.kind),
        recipe.name
    )
    .into())
}
