//! Who wants what, and what it means when one of them stops.
//!
//! ## The mistake this exists to prevent
//!
//! An entity can be wanted by more than one declaration at once: the app
//! manifest and a direct `jails add` may both want `db`. Today there is one
//! recorded row and no owner on it, so "the manifest no longer mentions this"
//! and "nobody wants this" are the same observation — and acting on the second
//! when only the first is true deletes something a different declaration still
//! asks for.
//!
//! So an entity carries a **set** of owners, and every rule below follows from
//! that: removing one owner removes one claim, semantic absence happens only
//! when the last owner goes, and a scope that does not mention an entity has
//! said nothing about the owners outside it.
//!
//! ## Scope is the whole mechanism
//!
//! plan.md §R1.3: *"This scoping is the mechanism that preserves unrelated
//! applied state without lying that machine state is human desire."* The app
//! manifest is authoritative over its own claims — absence there really does
//! mean relinquished — while a direct `destroy` touches exactly the identity
//! named and may not remove another direct row merely because it was not on
//! the command line.
//!
//! ## The ledger contributes no declaration
//!
//! What is observed is *what was applied*, never *what is wanted*. This module
//! reads observed owners forward and never converts a recorded row into a
//! desire. That boundary is why a stale ledger cannot resurrect an entity the
//! human sources have stopped asking for.

use crate::Result;
use crate::entity::{EntityId, EntitySpec, OwnerId};
use std::collections::{BTreeMap, BTreeSet};

/// What the ledger says was applied.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObservedEntity {
    pub spec: EntitySpec,
    pub owners: BTreeSet<OwnerId>,
}

/// What a declaration currently asks for.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DesiredEntity {
    pub id: EntityId,
    pub spec: EntitySpec,
    pub owners: BTreeSet<OwnerId>,
}

/// How far the current request's authority reaches.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReconcileScope {
    /// Complete presence and absence for the `AppManifest` owner.
    AppManifest,
    /// The projected complete capability list in `jails.toml`.
    DirectConfig,
    /// Exactly one direct `generate`/`destroy` request.
    DirectEntity(EntityId),
}

impl ReconcileScope {
    /// The owner this scope speaks for. A scope may only ever add or remove
    /// its own claim.
    pub fn owner(&self) -> OwnerId {
        match self {
            Self::AppManifest => OwnerId::AppManifest,
            Self::DirectConfig => OwnerId::DirectConfig,
            Self::DirectEntity(_) => OwnerId::DirectCli,
        }
    }

    /// Whether this scope is entitled to speak about an entity at all.
    ///
    /// `DirectEntity` is the narrow one and the reason the distinction
    /// matters: `jails destroy record Note` says nothing about `record Memo`,
    /// so silence there must not be read as absence.
    pub fn covers(&self, id: &EntityId, observed: &ObservedEntity) -> bool {
        match self {
            Self::AppManifest => true,
            // The projected config list is complete for capabilities and says
            // nothing about intents.
            Self::DirectConfig => {
                matches!(id, EntityId::Capability(_))
                    || observed.owners.contains(&OwnerId::DirectConfig)
            }
            Self::DirectEntity(target) => target == id,
        }
    }
}

/// One scope's complete declaration set: what is wanted, and how far the
/// claim reaches.
///
/// The scope travels *with* the entities rather than beside them because the
/// two are only meaningful together — a map of entities with no scope cannot
/// say whether an absent entity means "remove it" or "not my business", and
/// that is exactly the question reconciliation asks of every row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DesiredState {
    pub scope: ReconcileScope,
    pub entities: BTreeMap<EntityId, DesiredEntity>,
}

impl DesiredState {
    /// Refuses a row filed under an identity other than its own. Two
    /// authorities for one entity's identity is how a plan comes to name a
    /// file it never rendered.
    pub fn new(scope: ReconcileScope, entities: BTreeMap<EntityId, DesiredEntity>) -> Result<Self> {
        for (id, entity) in &entities {
            if &entity.id != id {
                return Err(format!(
                    "desired entity {:?} is filed under the identity {id:?}",
                    entity.id
                ));
            }
            if !entity.spec.matches(id) {
                return Err(format!(
                    "desired entity {id:?} pairs an identity and a spec of different kinds"
                ));
            }
            if entity.owners.is_empty() {
                return Err(format!(
                    "desired entity {id:?} has no owner; an unowned declaration is an absence"
                ));
            }
        }
        Ok(Self { scope, entities })
    }

    /// The single entity a `DirectEntity` scope speaks for, if this is one.
    pub fn direct_subject(&self) -> Option<&EntityId> {
        match &self.scope {
            ReconcileScope::DirectEntity(id) => Some(id),
            _ => None,
        }
    }
}

/// The result of comparing one scope's declarations against what was applied.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Reconciled {
    /// Every entity that still has at least one owner.
    pub entities: BTreeMap<EntityId, ObservedEntity>,
    /// Entities whose last owner disappeared, in canonical order.
    pub removed: Vec<EntityId>,
}

/// Compare a scope's declarations against what was applied.
///
/// `references` are the retained typed edges, `(from, to)`. A last-owner
/// removal is refused while anything still points at the entity — otherwise
/// the project stops compiling on the next build, at a file nobody edited.
pub fn reconcile(
    scope: &ReconcileScope,
    declared: &BTreeMap<EntityId, DesiredEntity>,
    observed: &BTreeMap<EntityId, ObservedEntity>,
    references: &[(EntityId, EntityId)],
) -> Result<Reconciled> {
    let owner = scope.owner();
    let mut entities: BTreeMap<EntityId, ObservedEntity> = BTreeMap::new();
    let mut removed = Vec::new();

    // Every identity either side knows about.
    let mut ids: Vec<&EntityId> = observed.keys().chain(declared.keys()).collect();
    ids.sort();
    ids.dedup();

    for id in ids {
        let was = observed.get(id);
        let now = declared.get(id);
        let in_scope =
            was.is_none_or(|entity| scope.covers(id, entity)) && (now.is_some() || was.is_some());

        // Owners outside this scope are carried forward untouched. Omission
        // here is silence, never removal.
        let mut owners: BTreeSet<OwnerId> =
            was.map(|entity| entity.owners.clone()).unwrap_or_default();

        if in_scope {
            // The active scope's claim is a *replacement*: drop the prior one,
            // then re-add it if it is still declared.
            owners.remove(&owner);
        }
        if let Some(entity) = now {
            owners.extend(entity.owners.iter().copied());
        }

        if owners.is_empty() {
            removed.push(id.clone());
            continue;
        }

        // Every surviving owner has to agree on one spec.
        let spec = match (now, was) {
            (Some(declaration), Some(applied)) => {
                let outside: BTreeSet<OwnerId> = applied
                    .owners
                    .iter()
                    .copied()
                    .filter(|existing| *existing != owner)
                    .collect();
                if !outside.is_empty() && applied.spec != declaration.spec {
                    return Err(disagreement(
                        id,
                        owner,
                        &outside,
                        &applied.spec,
                        &declaration.spec,
                    ));
                }
                declaration.spec.clone()
            }
            (Some(declaration), None) => declaration.spec.clone(),
            (None, Some(applied)) => applied.spec.clone(),
            (None, None) => unreachable!("an id came from one of the two maps"),
        };
        entities.insert(id.clone(), ObservedEntity { spec, owners });
    }

    // A removal that would break a retained reference refuses, naming both
    // ends. The alternative is a project that stops compiling on the next
    // build at a file nobody edited.
    for gone in &removed {
        let mut dependants: Vec<&EntityId> = references
            .iter()
            .filter(|(_, to)| to == gone)
            .map(|(from, _)| from)
            .filter(|from| entities.contains_key(*from))
            .collect();
        dependants.sort();
        dependants.dedup();
        if !dependants.is_empty() {
            return Err(format!(
                "removing {} would leave {} pointing at nothing.\n       fix: remove the \
                 dependant first, or keep a declaration that owns {}.",
                describe(gone),
                dependants
                    .iter()
                    .map(|id| describe(id))
                    .collect::<Vec<_>>()
                    .join(", "),
                describe(gone)
            ));
        }
    }

    Ok(Reconciled { entities, removed })
}

/// Two owners want different things from one entity, so neither is applied.
///
/// Silently taking the newer claim would let a direct `add` rewrite what the
/// manifest asked for, and the next `app apply` would change it straight back.
fn disagreement(
    id: &EntityId,
    owner: OwnerId,
    outside: &BTreeSet<OwnerId>,
    applied: &EntitySpec,
    declared: &EntitySpec,
) -> String {
    let others: Vec<&str> = outside.iter().map(|owner| owner_label(*owner)).collect();
    format!(
        "{} is wanted differently by {} and {}.\n       fix: make the declarations agree — \
         applying one would be undone by the other's next run.\n       {} wants: {}\n       \
         {} wants: {}",
        describe(id),
        owner_label(owner),
        others.join(", "),
        owner_label(owner),
        summarise(declared),
        others.join(", "),
        summarise(applied),
    )
}

fn owner_label(owner: OwnerId) -> &'static str {
    match owner {
        OwnerId::AppManifest => "the app manifest",
        OwnerId::DirectConfig => "jails.toml",
        OwnerId::DirectCli => "a direct command",
    }
}

fn describe(id: &EntityId) -> String {
    match id {
        EntityId::Intent(intent) => format!(
            "`{} {}`",
            crate::entity::recipe_label(intent.recipe),
            intent.name
        ),
        EntityId::Capability(capability) => format!("`{}`", capability.kind.label()),
        EntityId::ToolFeature(_) => "`fast-test`".to_string(),
    }
}

/// A one-line summary of a spec, for a disagreement report.
fn summarise(spec: &EntitySpec) -> String {
    match spec {
        EntitySpec::Intent(intent) => {
            if intent.fields.is_empty() {
                "no fields".to_string()
            } else {
                intent
                    .fields
                    .iter()
                    .map(|field| field.canonical())
                    .collect::<Vec<_>>()
                    .join(" ")
            }
        }
        EntitySpec::Capability(capability) => match &capability.placement {
            Some(package) if !package.is_base() => format!("placement {package}"),
            _ => "the conventional placement".to_string(),
        },
        EntitySpec::ToolFeature(feature) => format!("console {}", feature.console_version),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::declaration::IntentSpec;
    use crate::entity::{CapabilityId, CapabilitySpec, IntentId, Recipe};
    use crate::identity::{Name, Package};
    use jails_spec::spec::kind::Capability;

    fn intent(name: &str) -> EntityId {
        EntityId::Intent(IntentId::new(
            Recipe::Record,
            Name::parse(name).unwrap(),
            Package::base(),
        ))
    }

    fn capability(kind: Capability) -> EntityId {
        EntityId::Capability(CapabilityId::resolve(kind, None, None).unwrap())
    }

    fn spec(fields: &[&str]) -> EntitySpec {
        let owned: Vec<String> = fields.iter().map(|f| f.to_string()).collect();
        EntitySpec::Intent(IntentSpec::parse(&owned, &[], false, &Package::base()).unwrap())
    }

    fn capability_spec() -> EntitySpec {
        EntitySpec::Capability(CapabilitySpec::default())
    }

    fn observed(spec: EntitySpec, owners: &[OwnerId]) -> ObservedEntity {
        ObservedEntity {
            spec,
            owners: owners.iter().copied().collect(),
        }
    }

    fn desired(id: &EntityId, spec: EntitySpec, owner: OwnerId) -> DesiredEntity {
        DesiredEntity {
            id: id.clone(),
            spec,
            owners: BTreeSet::from([owner]),
        }
    }

    /// The rule everything else follows from. Two declarations want `db`; the
    /// manifest stops mentioning it; `db` stays, because `jails.toml` still
    /// wants it.
    #[test]
    fn removing_one_owner_removes_one_claim_and_not_the_entity() {
        let db = capability(Capability::Db);
        let observed_state = BTreeMap::from([(
            db.clone(),
            observed(
                capability_spec(),
                &[OwnerId::AppManifest, OwnerId::DirectConfig],
            ),
        )]);

        let result = reconcile(
            &ReconcileScope::AppManifest,
            &BTreeMap::new(),
            &observed_state,
            &[],
        )
        .unwrap();

        assert!(result.removed.is_empty(), "{:?}", result.removed);
        assert_eq!(
            result.entities[&db].owners,
            BTreeSet::from([OwnerId::DirectConfig]),
            "only the manifest's claim went"
        );
    }

    /// And when the last owner goes, the entity really is absent.
    #[test]
    fn semantic_absence_happens_only_when_the_last_owner_disappears() {
        let db = capability(Capability::Db);
        let observed_state = BTreeMap::from([(
            db.clone(),
            observed(capability_spec(), &[OwnerId::AppManifest]),
        )]);

        let result = reconcile(
            &ReconcileScope::AppManifest,
            &BTreeMap::new(),
            &observed_state,
            &[],
        )
        .unwrap();

        assert_eq!(result.removed, vec![db]);
        assert!(result.entities.is_empty());
    }

    /// `jails destroy record Note` says nothing about `record Memo`. Reading
    /// that silence as absence would delete an entity nobody mentioned.
    #[test]
    fn a_direct_request_touches_exactly_the_identity_it_names() {
        let note = intent("Note");
        let memo = intent("Memo");
        let observed_state = BTreeMap::from([
            (
                note.clone(),
                observed(spec(&["a:string"]), &[OwnerId::DirectCli]),
            ),
            (
                memo.clone(),
                observed(spec(&["b:int"]), &[OwnerId::DirectCli]),
            ),
        ]);

        let result = reconcile(
            &ReconcileScope::DirectEntity(note.clone()),
            &BTreeMap::new(),
            &observed_state,
            &[],
        )
        .unwrap();

        assert_eq!(result.removed, vec![note]);
        assert!(
            result.entities.contains_key(&memo),
            "an entity that was not named is untouched"
        );
    }

    /// The ledger contributes no declaration: an owner outside the scope is
    /// carried forward, never re-derived from what the scope happens to say.
    #[test]
    fn an_owner_outside_the_scope_is_never_removed_by_omission() {
        let note = intent("Note");
        let observed_state = BTreeMap::from([(
            note.clone(),
            observed(
                spec(&["a:string"]),
                &[OwnerId::AppManifest, OwnerId::DirectCli],
            ),
        )]);

        // A direct destroy of a *different* entity.
        let result = reconcile(
            &ReconcileScope::DirectEntity(intent("Other")),
            &BTreeMap::new(),
            &observed_state,
            &[],
        )
        .unwrap();
        assert_eq!(result.entities[&note].owners.len(), 2);
    }

    /// A sole owner may update its own spec freely.
    #[test]
    fn a_sole_owner_updates_normally() {
        let note = intent("Note");
        let observed_state = BTreeMap::from([(
            note.clone(),
            observed(spec(&["a:string"]), &[OwnerId::AppManifest]),
        )]);
        let declared = BTreeMap::from([(
            note.clone(),
            desired(&note, spec(&["a:string", "b:int"]), OwnerId::AppManifest),
        )]);

        let result = reconcile(
            &ReconcileScope::AppManifest,
            &declared,
            &observed_state,
            &[],
        )
        .unwrap();
        assert_eq!(result.entities[&note].spec, spec(&["a:string", "b:int"]));
    }

    /// Two owners may update together when their declarations agree.
    #[test]
    fn two_owners_that_agree_update_together() {
        let note = intent("Note");
        let observed_state = BTreeMap::from([(
            note.clone(),
            observed(
                spec(&["a:string"]),
                &[OwnerId::AppManifest, OwnerId::DirectCli],
            ),
        )]);
        let declared = BTreeMap::from([(
            note.clone(),
            desired(&note, spec(&["a:string"]), OwnerId::AppManifest),
        )]);

        let result = reconcile(
            &ReconcileScope::AppManifest,
            &declared,
            &observed_state,
            &[],
        )
        .unwrap();
        assert_eq!(
            result.entities[&note].owners,
            BTreeSet::from([OwnerId::AppManifest, OwnerId::DirectCli])
        );
    }

    /// Silently taking the newer claim would let a direct command rewrite what
    /// the manifest asked for — and the next `app apply` would change it
    /// straight back, forever.
    #[test]
    fn two_owners_that_disagree_refuse_and_show_both() {
        let note = intent("Note");
        let observed_state = BTreeMap::from([(
            note.clone(),
            observed(
                spec(&["a:string"]),
                &[OwnerId::AppManifest, OwnerId::DirectCli],
            ),
        )]);
        let declared = BTreeMap::from([(
            note.clone(),
            desired(&note, spec(&["b:int"]), OwnerId::AppManifest),
        )]);

        let error = reconcile(
            &ReconcileScope::AppManifest,
            &declared,
            &observed_state,
            &[],
        )
        .unwrap_err();

        assert!(error.contains("wanted differently"), "{error}");
        assert!(error.contains("the app manifest"), "{error}");
        assert!(error.contains("a direct command"), "{error}");
        assert!(error.contains("a:string"), "it shows both specs: {error}");
        assert!(error.contains("b:int"), "it shows both specs: {error}");
        assert!(error.contains("undone by the other"), "{error}");
    }

    /// Otherwise the project stops compiling on the next build, at a file
    /// nobody edited.
    #[test]
    fn a_last_owner_removal_refuses_while_something_still_points_at_it() {
        let note = intent("Note");
        let repo = intent("NoteRepo");
        let observed_state = BTreeMap::from([
            (
                note.clone(),
                observed(spec(&["a:string"]), &[OwnerId::AppManifest]),
            ),
            (repo.clone(), observed(spec(&[]), &[OwnerId::DirectCli])),
        ]);
        let declared =
            BTreeMap::from([(repo.clone(), desired(&repo, spec(&[]), OwnerId::DirectCli))]);

        let error = reconcile(
            &ReconcileScope::AppManifest,
            &declared,
            &observed_state,
            &[(repo.clone(), note.clone())],
        )
        .unwrap_err();

        assert!(error.contains("pointing at nothing"), "{error}");
        assert!(error.contains("record Note"), "{error}");
        assert!(error.contains("record NoteRepo"), "{error}");
    }

    /// A reference from something that is *also* going away is not a reason to
    /// refuse: both leave together.
    #[test]
    fn a_reference_from_another_removed_entity_does_not_block() {
        let note = intent("Note");
        let repo = intent("NoteRepo");
        let observed_state = BTreeMap::from([
            (note.clone(), observed(spec(&[]), &[OwnerId::AppManifest])),
            (repo.clone(), observed(spec(&[]), &[OwnerId::AppManifest])),
        ]);

        let result = reconcile(
            &ReconcileScope::AppManifest,
            &BTreeMap::new(),
            &observed_state,
            &[(repo.clone(), note.clone())],
        )
        .unwrap();
        assert_eq!(result.removed.len(), 2);
    }

    /// A brand-new declaration is simply added.
    #[test]
    fn a_new_declaration_becomes_a_new_entity() {
        let note = intent("Note");
        let declared = BTreeMap::from([(
            note.clone(),
            desired(&note, spec(&["a:string"]), OwnerId::AppManifest),
        )]);
        let result = reconcile(
            &ReconcileScope::AppManifest,
            &declared,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
        assert_eq!(
            result.entities[&note].owners,
            BTreeSet::from([OwnerId::AppManifest])
        );
        assert!(result.removed.is_empty());
    }

    /// `jails.toml` is the complete capability list, and says nothing about
    /// intents.
    #[test]
    fn the_config_scope_speaks_for_capabilities_only() {
        let db = capability(Capability::Db);
        let note = intent("Note");
        let observed_state = BTreeMap::from([
            (
                db.clone(),
                observed(capability_spec(), &[OwnerId::DirectConfig]),
            ),
            (note.clone(), observed(spec(&[]), &[OwnerId::DirectCli])),
        ]);

        let result = reconcile(
            &ReconcileScope::DirectConfig,
            &BTreeMap::new(),
            &observed_state,
            &[],
        )
        .unwrap();

        assert_eq!(result.removed, vec![db], "the capability list is complete");
        assert!(
            result.entities.contains_key(&note),
            "an intent is not the config scope's business"
        );
    }
}
