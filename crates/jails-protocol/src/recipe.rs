//! What each recipe accepts, and what each capability needs first.
//!
//! plan.md §R1.2: *"Recipe metadata is the single table of allowed
//! inputs/outputs … The manifest parser does not repeat it."* Repeating it is
//! the failure this prevents: the manifest parser, the generator and `destroy`
//! each knowing separately which kinds take an `on` reference is three tables
//! that drift, and `CLAUDE.md` already records what a private copy of a shared
//! list does — `inspect.rs` kept its own layer names and silently reported a
//! renamed layer as "Other".
//!
//! ## The prerequisite graph is validation, never declaration
//!
//! A capability's prerequisites must *already* be declared by some real owner.
//! The planner never auto-injects ownership to make the graph pass, and a
//! dependency, plugin or compose service merely *found* in the live project
//! does not satisfy the requirement either. Both shortcuts would produce a
//! project that works until the day somebody removes the thing nobody recorded
//! as wanted.
//!
//! There are exactly three edges at this baseline, and a test says so — not to
//! pin trivia, but because §R1.2 asks specifically that no hidden POM or file
//! probe quietly adds a fourth.

use jails_spec::spec::kind::{ArtifactKind, Capability};

/// How an artifact behaves over its lifetime.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LifecycleClass {
    /// Owned, updatable, and removable when its last owner is absent.
    PersistentIntent,
    /// `field`: recorded as a receipt and reapplied on every later render of
    /// its target. Not a desired entity and not referenceable.
    OneShotField,
    /// `migration`: append-only. The database has already run it.
    OneShotMigration,
    /// `cases`: an input, not an owned entity. Re-running replaces its output.
    OneShotCases,
}

/// Whether a recipe takes a reference, and whether it must.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RefArity {
    Required,
    Optional,
    /// Supplying it is an error rather than being ignored — a parameter
    /// silently dropped is one the user believes they gave.
    Forbidden,
}

/// What a recipe's positional arguments are.
///
/// plan.md §R1.1's amendment: the list means three different things, and the
/// recipe is the only thing that says which. `jails g enum Status ACTIVE
/// CLOSED` names constants, not `name:type` components -- reading them as
/// fields refuses a command that works, and storing them as fields would claim
/// record components that do not exist.
///
/// The shape is chosen *before* a token is read, so a mis-typed constant is
/// refused as a bad constant rather than reinterpreted as a field.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArgumentShape {
    /// `name:type[!?]@marker` components.
    Fields,
    /// Bare names: enum constants, sealed permits, strategy implementations,
    /// and the components of an existing record `g search` indexes.
    Names,
    /// `childField=parentField`, which only `association` takes.
    Mappings,
}

/// One recipe's contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RecipeMetadata {
    pub class: LifecycleClass,
    pub on: RefArity,
    pub yields: RefArity,
    pub arguments: ArgumentShape,
}

/// The single table. Every arm is explicit so a new `ArtifactKind` fails to
/// compile until it is classified.
pub fn metadata(recipe: ArtifactKind) -> RecipeMetadata {
    use ArtifactKind::*;
    use LifecycleClass::*;
    use RefArity::*;

    let (class, on, yields) = match recipe {
        // The one-shots, named by §R1.1's classification table.
        Field => (OneShotField, Forbidden, Forbidden),
        Migration => (OneShotMigration, Forbidden, Forbidden),
        Cases => (OneShotCases, Forbidden, Forbidden),

        // Persistent intents that take references. These arities are what
        // `generate/recipes.rs` enforces today, read off its arms.
        Strategy => (PersistentIntent, Required, Optional),
        Usecase => (PersistentIntent, Required, Optional),
        Query => (PersistentIntent, Required, Forbidden),
        Transition => (PersistentIntent, Required, Forbidden),
        HttpWorkflow => (PersistentIntent, Required, Forbidden),
        HttpSink => (PersistentIntent, Required, Required),
        Association => (PersistentIntent, Required, Required),
        DurableJob => (PersistentIntent, Required, Required),

        // Persistent intents that take none.
        //
        // `Forbidden` here is the §R1.1 target — *"never silently ignore a CLI
        // parameter"* — and **not** a description of today. Verified: `jails g
        // record Note --on Bogus` currently succeeds and ignores the flag, so
        // a user who typed it believes they said something they did not. This
        // table is what step 7 plans against and R6 activates; the imperative
        // path is unchanged, because R1 is shadow-only.
        Scaffold | Controller | Service | Class | Interface | Record | Factory | Value | Enum
        | Sealed | Repo | Handler | Command | Cli | Client | Fetcher | Job | Idempotency | Auth
        | Webhook | Search | Dto | Event | Test | IntegrationTest => {
            (PersistentIntent, Forbidden, Forbidden)
        }
    };
    // The positional shape, as its own closed match, because it is a
    // different question from the reference arity and answering both in one
    // arm list would make the table unreadable at exactly the point a new
    // kind is added.
    let arguments = match recipe {
        Enum | Sealed | Strategy | Search => ArgumentShape::Names,
        Association => ArgumentShape::Mappings,
        Scaffold | Controller | Service | Class | Interface | Record | Factory | Value | Repo
        | Handler | Command | Cli | Client | Fetcher | Job | Idempotency | Auth | Webhook | Dto
        | Event | Test | IntegrationTest | Usecase | Query | Transition | HttpWorkflow
        | HttpSink | DurableJob | Field | Migration | Cases => ArgumentShape::Fields,
    };
    RecipeMetadata {
        class,
        on,
        yields,
        arguments,
    }
}

/// What this recipe's positional arguments are.
pub fn argument_shape(recipe: ArtifactKind) -> ArgumentShape {
    metadata(recipe).arguments
}

/// Whether a recipe is a persistent, ownable, removable entity.
pub fn is_persistent(recipe: ArtifactKind) -> bool {
    metadata(recipe).class == LifecycleClass::PersistentIntent
}

/// The capabilities a capability requires to already be declared.
///
/// Three edges, all from `k8s`. A Helm deployment needs an image to deploy
/// (`docker`), probes to point at (`actuator`) and metrics for its burn-rate
/// alerts (`observability`) — each a thing the generated chart references by
/// name, so a chart without it is one that deploys and never becomes ready.
pub fn prerequisites(capability: Capability) -> &'static [Capability] {
    match capability {
        Capability::K8s => &[
            Capability::Docker,
            Capability::Actuator,
            Capability::Observability,
        ],
        _ => &[],
    }
}

/// Every prerequisite, transitively, in canonical label order.
///
/// Terminates on a cycle rather than recursing forever: the table is meant to
/// be acyclic and `the_prerequisite_graph_is_acyclic` proves it, but a
/// stack overflow is a poor way to learn that somebody broke it.
pub fn transitive_prerequisites(capability: Capability) -> Vec<Capability> {
    let mut out: Vec<Capability> = Vec::new();
    let mut queue = vec![capability];
    let mut seen = vec![capability];
    while let Some(next) = queue.pop() {
        for required in prerequisites(next) {
            if seen.contains(required) {
                continue;
            }
            seen.push(*required);
            out.push(*required);
            queue.push(*required);
        }
    }
    out.sort_by_key(|capability| capability.label());
    out
}

/// Which declared prerequisites are missing from a set that has real owners.
///
/// The caller supplies the effective desired union — every capability some
/// retained scope actually declares. A capability merely *present in the live
/// project* is not in that set, on purpose: satisfying a prerequisite from an
/// artefact nobody recorded as wanted is how a project works until the day
/// somebody removes it.
pub fn missing_prerequisites(wanted: Capability, declared: &[Capability]) -> Vec<Capability> {
    let mut missing: Vec<Capability> = transitive_prerequisites(wanted)
        .into_iter()
        .filter(|required| !declared.contains(required))
        .collect();
    missing.sort_by_key(|capability| capability.label());
    missing
}

// ---------------------------------------------------------------------------
// Reference resolution
// ---------------------------------------------------------------------------

/// What a reference points at once resolved.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RefTarget {
    /// Another entity jails owns. Only these participate in ordering.
    Managed(crate::entity::IntentId),
    /// A type the project already has. An already-satisfied leaf: there is
    /// nothing to order it against, because nothing generates it.
    Existing(crate::identity::JavaType),
}

/// Why a reference could not be resolved, as data rather than a formatted
/// string, so the caller can sort and present a whole batch deterministically.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RefError {
    /// Nothing matched. Names what was looked for.
    Missing { spelling: String },
    /// Several matched. Candidates arrive sorted.
    Ambiguous {
        spelling: String,
        candidates: Vec<String>,
    },
}

impl std::fmt::Display for RefError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missing { spelling } => write!(
                f,
                "`{spelling}` names nothing this project declares or contains.\n       fix: \
                 generate it first, or spell it fully qualified."
            ),
            Self::Ambiguous {
                spelling,
                candidates,
            } => write!(
                f,
                "`{spelling}` is ambiguous; {} match it.\n       fix: spell it fully \
                 qualified — one of: {}.",
                candidates.len(),
                candidates.join(", ")
            ),
        }
    }
}

/// Resolve one reference against the managed and existing candidates.
///
/// §R1.2 fixes the search order: compatible managed intents in the *referring*
/// entity's package first, then the conventional package for that kind, then
/// compatible existing source types. A fully-qualified spelling restricts the
/// search to that name but still validates the kind.
///
/// The subtle rule is the collapse: a managed and an existing candidate with
/// the same fully-qualified output are **one** `Managed` target, not an
/// ambiguity. They are the same type — jails generated the file the existing
/// candidate was read from — and reporting a conflict between a thing and
/// itself would make the first regeneration of any referenced type an error.
pub fn resolve_reference(
    spelling: &str,
    managed: &[(crate::entity::IntentId, String)],
    existing: &[String],
) -> std::result::Result<RefTarget, RefError> {
    let qualified = spelling.contains('.');
    let matches_spelling = |candidate: &str| {
        if qualified {
            candidate == spelling
        } else {
            candidate.rsplit('.').next() == Some(spelling)
        }
    };

    let managed_hits: Vec<&(crate::entity::IntentId, String)> = managed
        .iter()
        .filter(|(_, output)| matches_spelling(output))
        .collect();
    let existing_hits: Vec<&String> = existing
        .iter()
        .filter(|output| matches_spelling(output))
        .collect();

    // A managed candidate absorbs the existing file it wrote.
    let existing_hits: Vec<&String> = existing_hits
        .into_iter()
        .filter(|output| !managed_hits.iter().any(|(_, managed)| managed == *output))
        .collect();

    match (managed_hits.as_slice(), existing_hits.as_slice()) {
        ([(id, _)], []) => Ok(RefTarget::Managed(id.clone())),
        ([], [output]) => crate::identity::JavaType::parse(output)
            .map(RefTarget::Existing)
            .map_err(|_| RefError::Missing {
                spelling: spelling.to_string(),
            }),
        ([], []) => Err(RefError::Missing {
            spelling: spelling.to_string(),
        }),
        _ => {
            let mut candidates: Vec<String> = managed_hits
                .iter()
                .map(|(_, output)| output.clone())
                .chain(existing_hits.iter().map(|output| (*output).clone()))
                .collect();
            candidates.sort();
            candidates.dedup();
            Err(RefError::Ambiguous {
                spelling: spelling.to_string(),
                candidates,
            })
        }
    }
}

/// A managed reference edge, for ordering and cycle detection.
pub type Edge = (crate::entity::IntentId, crate::entity::IntentId);

/// Find a reference cycle, returning the full stable path if there is one.
///
/// Only `Managed` edges participate: an `Existing` target is already there, so
/// it can never be waiting on anything. Self-reference is a cycle of length
/// one and is reported as such rather than being special-cased away.
pub fn find_cycle(edges: &[Edge]) -> Option<Vec<crate::entity::IntentId>> {
    let mut nodes: Vec<crate::entity::IntentId> = Vec::new();
    for (from, to) in edges {
        for node in [from, to] {
            if !nodes.contains(node) {
                nodes.push(node.clone());
            }
        }
    }
    // Sorted so the reported path is the same on every run and machine.
    nodes.sort();

    for start in &nodes {
        let mut path = vec![start.clone()];
        if walk(start, edges, &mut path) {
            return Some(path);
        }
    }
    None
}

fn walk(
    from: &crate::entity::IntentId,
    edges: &[Edge],
    path: &mut Vec<crate::entity::IntentId>,
) -> bool {
    let mut outgoing: Vec<&crate::entity::IntentId> = edges
        .iter()
        .filter(|(source, _)| source == from)
        .map(|(_, target)| target)
        .collect();
    outgoing.sort();
    for next in outgoing {
        if next == &path[0] {
            path.push(next.clone());
            return true;
        }
        if path.contains(next) {
            continue;
        }
        path.push(next.clone());
        if walk(next, edges, path) {
            return true;
        }
        path.pop();
    }
    false
}

/// The deprecated manifest spellings, and the rule for them.
///
/// `strategy_on`/`strategy_yields` shipped in a user-facing file format, so
/// they keep parsing through R6. Supplying an alias *and* its canonical key is
/// an error **even when the values match**: two spellings for one fact means a
/// later edit to one of them silently loses, and a format people hand-edit
/// cannot afford that.
pub fn reference_key(
    canonical: &str,
    alias: &str,
    has_canonical: bool,
    has_alias: bool,
) -> std::result::Result<(), String> {
    if has_canonical && has_alias {
        return Err(format!(
            "`{canonical}` and `{alias}` are the same reference under two spellings.\n       \
             fix: keep `{canonical}`. Setting both is an error even when they agree, because a \
             later edit to one of them would silently lose."
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::ValueEnum;

    /// A recipe added without a thought for its lifecycle cannot compile, and
    /// this walks the enum to prove the table is total rather than trusting
    /// that it is.
    #[test]
    fn every_recipe_is_classified() {
        for recipe in ArtifactKind::value_variants() {
            let meta = metadata(*recipe);
            // A one-shot never takes a reference: it is not an entity, so
            // there is nothing for an edge to point at.
            if meta.class != LifecycleClass::PersistentIntent {
                assert_eq!(meta.on, RefArity::Forbidden, "{recipe:?}");
                assert_eq!(meta.yields, RefArity::Forbidden, "{recipe:?}");
            }
        }
    }

    /// §R1.1's classification table names exactly three one-shots.
    #[test]
    fn exactly_three_recipes_are_one_shots() {
        let one_shots: Vec<&ArtifactKind> = ArtifactKind::value_variants()
            .iter()
            .filter(|recipe| !is_persistent(**recipe))
            .collect();
        assert_eq!(one_shots.len(), 3, "{one_shots:?}");
        for expected in [
            ArtifactKind::Field,
            ArtifactKind::Migration,
            ArtifactKind::Cases,
        ] {
            assert!(one_shots.contains(&&expected), "{expected:?}");
        }
    }

    /// `yields` without `on` would be an edge from nothing.
    #[test]
    fn a_recipe_that_yields_also_takes_a_source() {
        for recipe in ArtifactKind::value_variants() {
            let meta = metadata(*recipe);
            if meta.yields != RefArity::Forbidden {
                assert_ne!(
                    meta.on,
                    RefArity::Forbidden,
                    "{recipe:?} yields a reference but takes no source"
                );
            }
        }
    }

    /// Three edges, all from `k8s`. §R1.2 asks specifically that no hidden POM
    /// or file probe quietly adds a fourth.
    #[test]
    fn the_prerequisite_graph_has_exactly_the_three_declared_edges() {
        let mut edges: Vec<(&str, &str)> = Vec::new();
        for capability in Capability::value_variants() {
            for required in prerequisites(*capability) {
                edges.push((capability.label(), required.label()));
            }
        }
        edges.sort_unstable();
        assert_eq!(
            edges,
            vec![
                ("k8s", "actuator"),
                ("k8s", "docker"),
                ("k8s", "observability"),
            ]
        );
    }

    /// Flavour and Java release are project preconditions, not capabilities,
    /// so nothing may sneak them in as prerequisite edges.
    #[test]
    fn the_prerequisite_graph_is_acyclic() {
        for capability in Capability::value_variants() {
            let transitive = transitive_prerequisites(*capability);
            assert!(
                !transitive.contains(capability),
                "{:?} requires itself",
                capability.label()
            );
        }
    }

    /// A capability found in the live project does not satisfy a prerequisite:
    /// only a real declaration does.
    #[test]
    fn a_prerequisite_is_satisfied_only_by_a_declaration() {
        assert_eq!(
            missing_prerequisites(Capability::K8s, &[]),
            vec![
                Capability::Actuator,
                Capability::Docker,
                Capability::Observability
            ],
            "sorted by label, so the diagnostic is deterministic"
        );
        assert_eq!(
            missing_prerequisites(Capability::K8s, &[Capability::Docker, Capability::Actuator]),
            vec![Capability::Observability]
        );
        assert!(
            missing_prerequisites(
                Capability::K8s,
                &[
                    Capability::Docker,
                    Capability::Actuator,
                    Capability::Observability
                ]
            )
            .is_empty()
        );
        // A capability with no prerequisites is never blocked.
        assert!(missing_prerequisites(Capability::Db, &[]).is_empty());
    }

    /// The diagnostic order is canonical so two runs report the same list.
    #[test]
    fn transitive_prerequisites_are_sorted_and_exclude_the_subject() {
        let transitive = transitive_prerequisites(Capability::K8s);
        assert_eq!(
            transitive,
            vec![
                Capability::Actuator,
                Capability::Docker,
                Capability::Observability
            ]
        );
        assert!(!transitive.contains(&Capability::K8s));
    }

    // -----------------------------------------------------------------------
    // Reference resolution
    // -----------------------------------------------------------------------

    use crate::entity::IntentId;
    use crate::identity::{Name, Package};

    fn intent(name: &str, package: &str) -> IntentId {
        IntentId::new(
            ArtifactKind::Record,
            Name::parse(name).unwrap(),
            Package::parse(package).unwrap(),
        )
    }

    fn managed(name: &str, package: &str) -> (IntentId, String) {
        let id = intent(name, package);
        let qualified = if package.is_empty() {
            name.to_string()
        } else {
            format!("{package}.{name}")
        };
        (id, qualified)
    }

    #[test]
    fn an_unqualified_reference_resolves_to_the_one_candidate() {
        let managed_rows = [managed("Note", "com.example.domain")];
        assert_eq!(
            resolve_reference("Note", &managed_rows, &[]).unwrap(),
            RefTarget::Managed(intent("Note", "com.example.domain"))
        );
    }

    /// An existing type is an already-satisfied leaf.
    #[test]
    fn a_reference_to_a_type_the_project_already_has_resolves_to_existing() {
        let target =
            resolve_reference("Money", &[], &["com.example.domain.Money".to_string()]).unwrap();
        match target {
            RefTarget::Existing(ty) => assert_eq!(ty.qualified(), "com.example.domain.Money"),
            other => panic!("{other:?}"),
        }
    }

    /// The collapse rule. A managed intent and the file it wrote are the same
    /// type; reporting a conflict between a thing and itself would make the
    /// first regeneration of any referenced type an error.
    #[test]
    fn a_managed_intent_absorbs_the_existing_file_it_wrote() {
        let managed_rows = [managed("Note", "com.example.domain")];
        let existing = ["com.example.domain.Note".to_string()];
        assert_eq!(
            resolve_reference("Note", &managed_rows, &existing).unwrap(),
            RefTarget::Managed(intent("Note", "com.example.domain")),
            "one target, not an ambiguity"
        );
    }

    /// Any *other* multiple match is ambiguous, with sorted candidates so two
    /// runs report the same list.
    #[test]
    fn two_different_types_with_one_simple_name_are_ambiguous() {
        let managed_rows = [
            managed("Note", "com.example.web"),
            managed("Note", "com.example.domain"),
        ];
        match resolve_reference("Note", &managed_rows, &[]).unwrap_err() {
            RefError::Ambiguous {
                spelling,
                candidates,
            } => {
                assert_eq!(spelling, "Note");
                assert_eq!(
                    candidates,
                    vec!["com.example.domain.Note", "com.example.web.Note"],
                    "sorted"
                );
            }
            other => panic!("{other:?}"),
        }
    }

    /// A fully-qualified spelling restricts the search to that one name.
    #[test]
    fn a_qualified_spelling_disambiguates() {
        let managed_rows = [
            managed("Note", "com.example.web"),
            managed("Note", "com.example.domain"),
        ];
        assert_eq!(
            resolve_reference("com.example.domain.Note", &managed_rows, &[]).unwrap(),
            RefTarget::Managed(intent("Note", "com.example.domain"))
        );
    }

    #[test]
    fn a_reference_to_nothing_says_what_it_looked_for() {
        let error = resolve_reference("Absent", &[], &[]).unwrap_err();
        assert!(matches!(error, RefError::Missing { .. }));
        let rendered = error.to_string();
        assert!(rendered.contains("`Absent`"), "{rendered}");
        assert!(rendered.contains("fix:"), "{rendered}");
    }

    /// Self-reference is a cycle of length one, reported rather than
    /// special-cased away.
    #[test]
    fn a_self_reference_is_a_cycle() {
        let note = intent("Note", "com.example.domain");
        let cycle = find_cycle(&[(note.clone(), note.clone())]).expect("a cycle");
        assert_eq!(cycle, vec![note.clone(), note]);
    }

    /// The full stable path, so two runs blame the same entity first.
    #[test]
    fn a_cycle_reports_its_whole_path_deterministically() {
        let a = intent("A", "p");
        let b = intent("B", "p");
        let c = intent("C", "p");
        let edges = [
            (a.clone(), b.clone()),
            (b.clone(), c.clone()),
            (c.clone(), a.clone()),
        ];
        let first = find_cycle(&edges).expect("a cycle");
        assert_eq!(first, vec![a.clone(), b, c, a]);

        // Declaration order must not change the reported path.
        let shuffled = [edges[2].clone(), edges[0].clone(), edges[1].clone()];
        assert_eq!(find_cycle(&shuffled).expect("a cycle"), first);
    }

    #[test]
    fn an_acyclic_graph_has_no_cycle() {
        let a = intent("A", "p");
        let b = intent("B", "p");
        let c = intent("C", "p");
        assert!(find_cycle(&[(a.clone(), b.clone()), (b, c)]).is_none());
        assert!(find_cycle(&[]).is_none());
        // A diamond is not a cycle.
        let d = intent("D", "p");
        assert!(
            find_cycle(&[
                (a.clone(), intent("B", "p")),
                (a, intent("C", "p")),
                (intent("B", "p"), d.clone()),
                (intent("C", "p"), d),
            ])
            .is_none()
        );
    }

    /// Two spellings for one fact means a later edit to one of them silently
    /// loses, and a format people hand-edit cannot afford that.
    #[test]
    fn an_alias_and_its_canonical_key_together_are_an_error_even_when_equal() {
        assert!(reference_key("on", "strategy_on", true, false).is_ok());
        assert!(reference_key("on", "strategy_on", false, true).is_ok());
        assert!(reference_key("on", "strategy_on", false, false).is_ok());

        let error = reference_key("on", "strategy_on", true, true).unwrap_err();
        assert!(error.contains("two spellings"), "{error}");
        assert!(error.contains("even when they agree"), "{error}");
    }
}
