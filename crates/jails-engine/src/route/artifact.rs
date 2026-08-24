//! `generate` and `destroy`: the routes whose subject is one artifact.
//!
//! `ReconcileScope::DirectEntity` is the narrow scope, and it is the whole
//! difference from the capability routes: `jails destroy record Note` says
//! nothing about `record Memo`, so silence about another identity must not be
//! read as absence.

use super::*;

/// Generate one persistent artifact through the transaction protocol.
///
/// The direct counterpart of `generate_in_project`, and the subject is one
/// entity rather than the capability list: `ReconcileScope::DirectEntity` is
/// "exactly one direct `generate`/`destroy` request", so this route may add or
/// remove its own claim and says nothing about anybody else's.
#[allow(clippy::too_many_arguments)]
pub fn generate(
    project: &Project,
    kind: ArtifactKind,
    name: &str,
    fields: &[String],
    package: Option<&str>,
    indexes: &[String],
    on: Option<&str>,
    yields: Option<&str>,
) -> Result<CommitResult> {
    let change = with_test_support(
        project,
        jails_generate::generate::plan_recipe(
            project, kind, name, fields, package, indexes, on, yields,
        )?,
    );
    let id = intent(project, kind, name, package, fields, indexes, on, yields)?;
    let owner = ResourceOwner::Entity(EntityId::Intent(id.clone()));
    let mut desired = desire::contribution(&owner, &change, project)?;
    let entity = DesiredEntity {
        id: EntityId::Intent(id.clone()),
        spec: EntitySpec::Intent(spec(project, kind, fields, indexes, on, yields)?),
        owners: BTreeSet::from([OwnerId::DirectCli]),
    };
    provenance::stamp_files(
        &mut desired,
        project,
        RendererId::Recipe(kind),
        Some(RenderedSubjectContext::Entity {
            id: entity.id.clone(),
            spec: entity.spec.clone(),
        }),
    )?;
    let reads = declaration(project, &change, &desired)?;
    // The request carries the resolved identity and spec, not the argument
    // strings: `g record Note title:string!` and the same call with the
    // package spelled out are one request, and a resume must recognise them
    // as one. The *syntax* half keeps the spelling separately.
    let asked = Asked::new(
        CanonicalMutationRequest::Generate(CanonicalGenerateRequest::Entity {
            id: entity.id.clone(),
            spec: entity.spec.clone(),
        }),
        &["generate"],
        std::iter::once(label(kind).to_string())
            .chain(std::iter::once(name.to_string()))
            .chain(fields.iter().cloned())
            .collect(),
        match package {
            Some(package) => BTreeMap::from([("package".to_string(), vec![package.to_string()])]),
            None => BTreeMap::new(),
        },
        BTreeSet::new(),
    );
    let request = Request {
        scope: ReconcileScope::DirectEntity(EntityId::Intent(id)),
        declared: BTreeMap::from([(entity.id.clone(), entity)]),
        changes: vec![desired],
    };
    commit(project, request, &reads, &asked)
}

/// Take one persistent artifact back out.
///
/// The counterpart of [`generate`], and the same shape as [`remove`] is to
/// [`install`]: nothing is described, the request simply stops declaring the
/// entity, and reconciliation works out what that means. §R6.2 asks
/// `destroy` to "forward-plan remaining resources from recorded exact state"
/// rather than rebuild a path list, which is what this is -- the store says
/// which resources this owner holds, and a resource nobody claims any more is
/// retired.
///
/// `ReconcileScope::DirectEntity` is the narrow scope for exactly this
/// reason: `jails destroy record Note` says nothing about `record Memo`, so
/// declaring nothing here retires one entity rather than every intent in the
/// project.
pub fn destroy(
    project: &Project,
    kind: ArtifactKind,
    name: &str,
    package: Option<&str>,
) -> Result<CommitResult> {
    let id = intent(project, kind, name, package, &[], &[], None, None)?;
    let entity = EntityId::Intent(id.clone());
    let store = observed(project)?;
    if !store
        .ledger
        .as_ref()
        .is_some_and(|ledger| ledger.applied.iter().any(|row| row.id == entity))
    {
        // Naming the command that *would* have recorded it is the whole
        // difference between this and a bare "nothing to destroy" printed
        // over files that are right there. CLAUDE.md keeps that rule for the
        // V1 path; the reason is the same here.
        return Err(format!(
            "no `{} {}` is recorded in this project.\n       fix: `jails g {} {}` is what \
             records one. A destroy that guessed at paths would delete files jails never wrote.",
            label(kind),
            id.name,
            label(kind),
            id.name,
        ));
    }
    let owner = ResourceOwner::Entity(entity.clone());
    let request = Request {
        scope: ReconcileScope::DirectEntity(entity.clone()),
        declared: BTreeMap::new(),
        changes: Vec::new(),
    };
    commit(
        project,
        request,
        &retiring(&store, &owner)?,
        &Asked::plain(
            CanonicalMutationRequest::destroy_entity(entity, false)?,
            &["destroy"],
            &[&label(kind), name],
        ),
    )
}
