//! Which options a generator kind accepts, as a table rather than a branch.
//!
//! **The refusal is the point.** A flag a kind cannot honour has to be
//! refused by name -- silently ignoring it produces a project the reader
//! believes they asked for and did not get -- and a table is what keeps every
//! kind's answer in one place, where adding a kind means adding a row rather
//! than remembering nine `if` statements spread through the frontend.

use super::*;

const RECORD_FACETS: &[Facet] = &[Facet::Record];
const ENUM_FACETS: &[Facet] = &[Facet::Enum];
const SCAFFOLD_FACETS: &[Facet] = &[
    Facet::Record,
    Facet::Repository,
    Facet::Service,
    Facet::Http,
];

pub(crate) struct EntityProfile {
    pub(crate) facets: &'static [Facet],
    pub(crate) timestamps: bool,
    /// Whether this profile puts a table behind the entity.
    ///
    /// Only `scaffold` does, which is what makes `--unique` meaningful: a
    /// composite unique is a constraint on columns, and a profile with no
    /// columns has nowhere to put one.
    pub(crate) table: bool,
    /// Whether `--path` pins this profile's collection route.
    ///
    /// Only `scaffold` has one: it is the profile that carries `Facet::Http`,
    /// and a route on a kind that serves nothing would be a flag with nowhere
    /// to land.
    pub(crate) route: bool,
}

pub(crate) fn entity_profile(kind: ArtifactKind) -> Option<&'static EntityProfile> {
    static RECORD: EntityProfile = EntityProfile {
        facets: RECORD_FACETS,
        timestamps: false,
        table: false,
        route: false,
    };
    static ENUM: EntityProfile = EntityProfile {
        facets: ENUM_FACETS,
        timestamps: false,
        table: false,
        route: false,
    };
    static SCAFFOLD: EntityProfile = EntityProfile {
        facets: SCAFFOLD_FACETS,
        timestamps: true,
        table: true,
        route: true,
    };
    match kind {
        ArtifactKind::Record | ArtifactKind::Value => Some(&RECORD),
        ArtifactKind::Enum => Some(&ENUM),
        ArtifactKind::Scaffold => Some(&SCAFFOLD),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OperationProfile {
    Command,
    Query,
    Transition,
    Event,
}

pub(crate) fn operation_profile(kind: ArtifactKind) -> Option<OperationProfile> {
    match kind {
        ArtifactKind::Usecase => Some(OperationProfile::Command),
        ArtifactKind::Query => Some(OperationProfile::Query),
        ArtifactKind::Transition => Some(OperationProfile::Transition),
        ArtifactKind::Event => Some(OperationProfile::Event),
        _ => None,
    }
}

/// Which of the supplied flags this profile has nowhere to put.
///
/// **Named, one at a time, with what it does apply to.** "does not represent
/// one or more supplied flags" is true of every one of these and useful for
/// none: a reader who typed `g record Thing --path /thing` needs to be told
/// that `--path` is for the kinds that answer a route, not to re-read the
/// command looking for which flag was the wrong one.
pub(crate) fn reject_unsupported_options(
    args: &GenerateArgs,
    profile: &EntityProfile,
) -> Result<()> {
    let unsupported: &[(bool, &str, &str)] = &[
        (
            args.timestamps && !profile.timestamps,
            "--timestamps",
            "the kinds that own a table",
        ),
        (
            !args.uniques.is_empty() && !profile.table,
            "--unique",
            "the kinds that own a table",
        ),
        // `--package` is an entity's too: it pins the package the whole slice
        // projects into, which is the same relationship a capability's has to
        // its backend's conventional one.
        (
            args.default_literal.is_some(),
            "--default-literal",
            "`resource field` commands",
        ),
        (
            args.backfill_file.is_some(),
            "--backfill-file",
            "`resource field nullability`",
        ),
        (args.strategy_on.is_some(), "--on", "operations"),
        (args.strategy_yields.is_some(), "--yields", "operations"),
        (args.via.is_some(), "--via", "a query or a use case"),
        (args.order_by.is_some(), "--order-by", "queries"),
        (args.limit.is_some(), "--limit", "queries"),
        (args.on_conflict.is_some(), "--on-conflict", "use cases"),
        (
            args.path.is_some() && !profile.route,
            "--path",
            "the kinds that answer a route",
        ),
        (args.select.is_some(), "--select", "transitions"),
        (!args.set.is_empty(), "--set", "a use case or a transition"),
        (args.if_match.is_some(), "--if-match", "transitions"),
        (!args.bind.is_empty(), "--bind", "a controller"),
        (
            args.method.is_some(),
            "--method",
            "the kinds that answer a route",
        ),
        (
            args.consumes.is_some(),
            "--consumes",
            "the kinds that answer a route",
        ),
    ];
    if let Some((_, flag, applies)) = unsupported.iter().find(|(supplied, _, _)| *supplied) {
        return Err(Failure::Told(format!(
            "`{flag}` applies to {applies}, and `{}` is not one of them.\n       fix: drop `{flag}`, or generate a kind that carries it",
            kind_name(args.kind)
        )));
    }
    Ok(())
}

pub(crate) fn validate_entity_args(args: &GenerateArgs) -> Result<()> {
    let profile = entity_profile(args.kind).ok_or_else(|| {
        Failure::Told(format!(
            "`{}` is not an entity declaration",
            kind_name(args.kind)
        ))
    })?;
    // **Checked before the identity is derived from it.** A stable id is a
    // projection of the name, so `Bad!Name` fails there first -- and reports
    // that `ent_bad!_name` is not a stable id, which is about a value the
    // reader never typed. The linker says the same thing about the Java type
    // and never gets the chance.
    if let Some(bad) = args
        .name
        .chars()
        .find(|character| !character.is_ascii_alphanumeric() && *character != '_')
    {
        return Err(Failure::Told(format!(
            "`{bad}` is not valid in a Java identifier, and `{}` becomes one.\n       fix: name it with letters, digits and `_`",
            args.name
        )));
    }
    reject_unsupported_options(args, profile)
}

pub(crate) fn reject_unsupported_operation_options(
    args: &GenerateArgs,
    profile: OperationProfile,
) -> Result<()> {
    // **Name the flag, and say which kind it belongs to.** "does not
    // represent one or more supplied flags" is true of every one of these and
    // useful for none: the reader has to guess which of the eight they typed
    // is the problem, and the answer is usually that the flag belongs to a
    // sibling kind. One row per flag, so the refusal reads like the sentence
    // somebody would say out loud.
    let kind = kind_name(args.kind);
    let entity_only = "an entity declaration";
    let unsupported: &[(bool, &str, &str)] = &[
        (args.timestamps, "--timestamps", entity_only),
        (args.package.is_some(), "--package", entity_only),
        (args.default_literal.is_some(), "--default", entity_only),
        (args.backfill_file.is_some(), "--backfill", entity_only),
        (!args.indexes.is_empty(), "--index", entity_only),
        (!args.uniques.is_empty(), "--unique", entity_only),
        (
            args.on_conflict.is_some() && profile != OperationProfile::Command,
            "--on-conflict",
            "a command",
        ),
        (
            args.via.is_some()
                && !matches!(profile, OperationProfile::Query | OperationProfile::Command),
            "--via",
            "a query or a command",
        ),
        (
            args.select.is_some() && profile != OperationProfile::Transition,
            "--select",
            "a transition",
        ),
        (
            !args.set.is_empty()
                && !matches!(
                    profile,
                    OperationProfile::Transition | OperationProfile::Command
                ),
            "--set",
            "a transition or a command",
        ),
        (
            args.if_match.is_some() && profile != OperationProfile::Transition,
            "--if-match",
            "a transition",
        ),
        (
            args.consumes.is_some() && profile == OperationProfile::Event,
            "--consumes",
            "an operation with a request boundary",
        ),
        (
            args.order_by.is_some() && profile != OperationProfile::Query,
            "--order-by",
            "a query",
        ),
        (
            args.limit.is_some() && profile != OperationProfile::Query,
            "--limit",
            "a query",
        ),
        (
            args.strategy_yields.is_some()
                && !matches!(
                    profile,
                    OperationProfile::Transition | OperationProfile::Command
                ),
            "--yields",
            "a transition or a command",
        ),
        // **A query's and a command's verb follows its request**, so
        // `--method` there is not a preference jails declines to honour -- it
        // is a claim about the request that contradicts the request.
        (
            args.method.is_some() && profile != OperationProfile::Transition,
            "--method",
            "a transition; every other operation's verb follows its request",
        ),
        (
            args.path.is_some() && profile == OperationProfile::Event,
            "--path",
            "an operation with a route",
        ),
    ];
    if let Some((_, flag, applies_to)) = unsupported.iter().find(|(hit, _, _)| *hit) {
        return Err(Failure::Told(format!(
            "`{flag}` applies to {applies_to}, and `{kind}` is not one.\n       fix: drop `{flag}`, or generate the kind it belongs to"
        )));
    }
    // **An event may stand on its own, and the grammar has always said so.**
    // `parse_operation(None)` accepts a top-level `event`, the linker gives it
    // `on: None`, and the compiler emits its payload record from the declared
    // parameters -- so a domain event that is nobody's row (`PageDiscovered`,
    // carrying its own id and the moment it happened) was refused only by this
    // frontend. Every other operation writes or reads a row and needs one.
    if args.strategy_on.is_none() && profile != OperationProfile::Event {
        return Err(Failure::Told(format!(
            "canonical `{}` needs the entity it operates on.\n       fix: pass `--on <Entity>`",
            kind_name(args.kind)
        )));
    }
    Ok(())
}
