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
    // Two recipes cross a reference, from the two ends: a query *reads*
    // across it, and a use case *resolves* it on the way in -- the caller
    // sends the parent's email and the row needs its id.
    if recipe.via.is_some() && !matches!(recipe.kind, ArtifactKind::Query | ArtifactKind::Usecase) {
        use clap::ValueEnum;
        return Err(format!(
            "`--via` applies to a query or a use case -- the recipes that cross a reference \
             between two resources.\n       fix: drop it from `jails g {} {}`.",
            recipe
                .kind
                .to_possible_value()
                .expect("every kind has a clap value")
                .get_name(),
            recipe.name
        )
        .into());
    }
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
    // One recipe has a precondition to insist on or not: the compare-and-swap
    // update. Nothing else reads `If-Match`, so a flag here would describe a
    // header the generated code never looks at.
    if recipe.if_match.is_some() && recipe.kind != ArtifactKind::Transition {
        use clap::ValueEnum;
        return Err(format!(
            "`--if-match` applies to a transition -- the one recipe with a version to check \
             against.\n       fix: drop it from `jails g {} {}`.",
            recipe
                .kind
                .to_possible_value()
                .expect("every kind has a clap value")
                .get_name(),
            recipe.name
        )
        .into());
    }
    // Two recipes write a row: one creates it and one updates it, and both
    // have a component the endpoint decides rather than the caller. Everywhere
    // else there is no row to pin a component of, so a `--set` would be the M7
    // shape -- a flag accepted, ignored, and never mentioned again.
    if !recipe.pins.is_empty()
        && !matches!(
            recipe.kind,
            ArtifactKind::Usecase | ArtifactKind::Transition
        )
    {
        use clap::ValueEnum;
        return Err(format!(
            "`--set` applies to a use case or a transition -- the recipes that write a \
             row.\n       fix: drop it from `jails g {} {}`.",
            recipe
                .kind
                .to_possible_value()
                .expect("every kind has a clap value")
                .get_name(),
            recipe.name
        )
        .into());
    }
    // Four recipes take a request body jails renders as one bound parameter --
    // a `query`'s criteria record is one, which is why it is here. `handler`
    // writes a whole CRUD surface rather than one route, and `webhook` reads
    // the raw bytes *before* the signature is checked, since binding them
    // first is the bug that kind exists to avoid.
    if recipe.consumes.is_some()
        && !matches!(
            recipe.kind,
            ArtifactKind::Controller
                | ArtifactKind::Usecase
                | ArtifactKind::Transition
                | ArtifactKind::Query
        )
    {
        use clap::ValueEnum;
        return Err(format!(
            "`--consumes` applies to a controller, a use case, a query or a transition -- the \
             recipes that bind one request body.\n       fix: drop it from `jails g {} {}`.",
            recipe
                .kind
                .to_possible_value()
                .expect("every kind has a clap value")
                .get_name(),
            recipe.name
        )
        .into());
    }
    // Five recipes answer or call HTTP on a route a caller might have to
    // match. A scaffold is one of them: it serves a *collection*, so the
    // named route is the collection's and the item routes hang off it --
    // `--path /admin_api/users` is `GET /admin_api/users/{id}` too. Refusing
    // it was the honest answer while nothing carried it and the useless one
    // once a frontend's URLs were the contract: `g scaffold User` served
    // `/users` and the admin page called `/admin_api/users`, so the project
    // could not be made consistent without hand-editing the one controller
    // jails had just written.
    // `handler` writes a whole CRUD surface rather than one route and
    // `webhook` answers a signed POST by definition, so neither is one path.
    if recipe.path.is_some()
        && !matches!(
            recipe.kind,
            ArtifactKind::Controller
                | ArtifactKind::Scaffold
                | ArtifactKind::Usecase
                | ArtifactKind::Query
                | ArtifactKind::Transition
                | ArtifactKind::Client
        )
    {
        use clap::ValueEnum;
        return Err(format!(
            "`--path` applies to a controller, a scaffold, a use case, a query or a \
             transition.\n       fix: drop it from `jails g {} {}`.",
            recipe
                .kind
                .to_possible_value()
                .expect("every kind has a clap value")
                .get_name(),
            recipe.name
        )
        .into());
    }
    // Two recipes render a verb: `controller` answers on one, `client` calls
    // one. Everywhere else the verb is *derived* and a flag contradicting the
    // derivation is the M7 shape again -- `g query X --method post` emitted
    // `@GetMapping` and said nothing, because a query is a GET exactly when
    // every filter comes from `--path` and a POST otherwise, which is a fact
    // about the request rather than a preference.
    // `transition` takes one too: its update is idempotent, so PUT and PATCH
    // are both correct spellings of "set these fields on this row" and a
    // frontend calling one will not accept the other. The first version of
    // this refusal claimed a transition *derives* its verb from the request,
    // which was simply untrue -- it was a hardcoded PUT.
    if recipe.method.is_some()
        && !matches!(
            recipe.kind,
            ArtifactKind::Controller | ArtifactKind::Client | ArtifactKind::Transition
        )
    {
        use clap::ValueEnum;
        let derived = matches!(recipe.kind, ArtifactKind::Query | ArtifactKind::Usecase);
        return Err(format!(
            "`--method` applies to a controller or a client -- the recipes that name a verb \
             rather than derive one.\n       fix: drop it from `jails g {kind} {name}`.{note}",
            kind = recipe
                .kind
                .to_possible_value()
                .expect("every kind has a clap value")
                .get_name(),
            name = recipe.name,
            note = if derived {
                "\n       note: this recipe's verb follows its request -- GET when every filter \
                 comes from\n             `--path`, POST when it carries a body."
            } else {
                ""
            }
        )
        .into());
    }
    takes_only_a_name(
        recipe,
        ArtifactKind::Socket,
        "a socket carries whatever the endpoint sends, and jails has no way to know what",
    )?;
    takes_only_a_name(
        recipe,
        ArtifactKind::Presence,
        "a scope and a member are runtime values the caller picks, not generation-time ones",
    )?;
    takes_only_a_name(
        recipe,
        ArtifactKind::Auth,
        "the subject and scopes are runtime values the caller supplies, not generation-time ones",
    )?;
    takes_only_a_name(
        recipe,
        ArtifactKind::Webhook,
        "the payload is whatever the sender posts, and binding it before the signature is checked \
         is the bug this kind exists to avoid",
    )?;
    takes_only_a_name(
        recipe,
        ArtifactKind::Seed,
        "a seed row is read from the record on disk, so naming its components again could only \
         disagree with them",
    )?;
    takes_only_a_name(
        recipe,
        ArtifactKind::Fetcher,
        "limits and policy are external configuration",
    )?;
    takes_only_a_name(
        recipe,
        ArtifactKind::Idempotency,
        "the scope, key and request bytes are runtime values the caller supplies",
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

/// A kind whose whole request is its name, so a field or a reference is a
/// misunderstanding rather than an extra.
fn takes_only_a_name(recipe: &Recipe<'_>, kind: ArtifactKind, why: &str) -> Result<()> {
    let asked = !recipe.fields.is_empty()
        || recipe.strategy_on.is_some()
        || recipe.strategy_yields.is_some();
    if recipe.kind != kind || !asked {
        return Ok(());
    }
    use clap::ValueEnum;
    let label = kind
        .to_possible_value()
        .expect("every kind has a clap value");
    Err(format!(
        "`{}` takes only a name: {why}.\n       fix: run `jails g {0} {}`.",
        label.get_name(),
        recipe.name
    )
    .into())
}
