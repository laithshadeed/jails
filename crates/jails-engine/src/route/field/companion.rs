use super::*;

pub(super) struct CompanionUpdates {
    pub(super) changes: Vec<DesiredChange>,
    pub(super) entities: BTreeMap<EntityId, DesiredEntity>,
    pub(super) reads: ReadDeclaration,
}

/// Re-desire every field-bearing intent that projects the same logical type.
///
/// `record X` followed by `scaffold X` intentionally leaves two owners: the
/// first is what lets destroying the scaffold preserve the original record.
/// A field evolution must therefore advance both specs and both projections
/// atomically, or the live scaffold is stale immediately and destroying it
/// later restores the pre-evolution record.
pub(super) fn companion_updates(
    project: &Project,
    store: &ObservedStore,
    primary: &IntentId,
    fields: &[FieldSpec],
    package: Option<&str>,
    primary_change: &DesiredChange,
    primary_reads: &ReadDeclaration,
) -> Result<CompanionUpdates> {
    let mut changes = Vec::new();
    let mut entities = BTreeMap::new();
    let mut reads = ReadDeclaration::new();
    for row in store.entities() {
        let (EntityId::Intent(id), EntitySpec::Intent(spec)) = (&row.id, &row.version.spec) else {
            continue;
        };
        if id == primary
            || id.name != primary.name
            || id.package != primary.package
            || !matches!(
                spec.arguments,
                jails_protocol::declaration::IntentArguments::Fields(_)
            )
        {
            continue;
        }
        let after = IntentSpec {
            arguments: jails_protocol::declaration::IntentArguments::Fields(fields.to_vec()),
            ..spec.clone()
        };
        let canonical_fields = fields.iter().map(FieldSpec::canonical).collect::<Vec<_>>();
        let indexes = after
            .indexes
            .iter()
            .map(|index| index.canonical())
            .collect::<Vec<_>>();
        let on = after.on.as_ref().map(JavaType::qualified);
        let yields = after.yields.as_ref().map(JavaType::qualified);
        let mut change = with_test_support(
            project,
            jails_generate::generate::plan_recipe(
                project,
                &jails_generate::generate::Recipe {
                    kind: id.recipe,
                    name: id.name.as_str(),
                    fields: &canonical_fields,
                    indexes: &indexes,
                    strategy_on: on.as_deref(),
                    strategy_yields: yields.as_deref(),
                    method: after.method,
                },
                package,
            )?,
        );
        change.files.retain(|artifact| {
            !artifact
                .path
                .strip_prefix(project.root())
                .is_ok_and(|path| {
                    path.to_string_lossy()
                        .replace('\\', "/")
                        .starts_with("src/main/resources/db/migration/")
                })
        });
        let owner = ResourceOwner::Entity(EntityId::Intent(id.clone()));
        let mut desired = desire::contribution(&owner, &change, project)?;
        provenance::stamp_files(
            &mut desired,
            project,
            RendererId::Recipe(id.recipe),
            Some(RenderedSubjectContext::Entity {
                id: EntityId::Intent(id.clone()),
                spec: EntitySpec::Intent(after.clone()),
            }),
        )?;
        reads = reads.merge(declaration(project, &change, &desired)?);
        entities.insert(
            EntityId::Intent(id.clone()),
            DesiredEntity {
                id: EntityId::Intent(id.clone()),
                spec: EntitySpec::Intent(after),
                owners: BTreeSet::from([OwnerId::DirectCli]),
            },
        );
        changes.push(desired);
    }

    let dependent = dependent_updates(
        project,
        store,
        primary,
        primary_change,
        primary_reads,
        &changes,
    )?;
    changes.extend(dependent.changes);
    entities.extend(dependent.entities);
    reads = reads.merge(dependent.reads);

    Ok(CompanionUpdates {
        changes,
        entities,
        reads,
    })
}

/// Re-desire every recorded entity that *constructs* the evolved resource.
///
/// A `query`, `transition` or `usecase` generated `--on Order` writes Java
/// that calls `new Order(...)` with the component list `Order` had at the
/// time. Adding one field to `Order` therefore breaks each of them, and
/// nothing reported it: the operation list named zero companions, `doctor`
/// stayed clear because every file on disk was byte-identical to what jails
/// wrote, and only `javac` found it. Refusing instead is worse in a different
/// way -- one generated query would make "this entity needs one more column",
/// the most common change there is, permanently impossible.
///
/// They are regenerated rather than patched, and against the **projection**:
/// these generators read the target's components back out of `<Name>.java`,
/// so they have to plan against the bytes this same transition is about to
/// write.
fn dependent_updates(
    project: &Project,
    store: &ObservedStore,
    primary: &IntentId,
    primary_change: &DesiredChange,
    primary_reads: &ReadDeclaration,
    same_name: &[DesiredChange],
) -> Result<CompanionUpdates> {
    let dependents: Vec<(IntentId, IntentSpec)> = store
        .entities()
        .iter()
        .filter_map(|row| {
            let (EntityId::Intent(id), EntitySpec::Intent(spec)) = (&row.id, &row.version.spec)
            else {
                return None;
            };
            let targets_primary = spec.on.as_ref().is_some_and(|on| {
                on.name() == &primary.name
                    && (on.package().is_base() || on.package() == &primary.package)
            });
            (targets_primary
                && matches!(
                    id.recipe,
                    ArtifactKind::Query | ArtifactKind::Transition | ArtifactKind::Usecase
                ))
            .then(|| (id.clone(), spec.clone()))
        })
        .collect();

    let mut changes = Vec::new();
    let mut entities = BTreeMap::new();
    let mut reads = ReadDeclaration::new();
    if dependents.is_empty() {
        return Ok(CompanionUpdates {
            changes,
            entities,
            reads,
        });
    }

    let mut planned = vec![primary_change.clone()];
    planned.extend(same_name.iter().cloned());
    let projected = super::super::projected_after(project, primary_reads, &planned)?;

    for (id, spec) in dependents {
        // Its own package, not the primary's: a companion may have been
        // generated with a different `--package`, and planning it under the
        // evolved resource's would write a second copy somewhere else.
        let package = recipe_package(project, &id, None)?;
        let package = package.as_deref();
        let canonical_fields = spec
            .fields()
            .iter()
            .map(FieldSpec::canonical)
            .collect::<Vec<_>>();
        let indexes = spec
            .indexes
            .iter()
            .map(|index| index.canonical())
            .collect::<Vec<_>>();
        let on = spec.on.as_ref().map(JavaType::qualified);
        let yields = spec.yields.as_ref().map(JavaType::qualified);
        let change = with_test_support(
            &projected,
            jails_generate::generate::plan_recipe(
                &projected,
                &jails_generate::generate::Recipe {
                    kind: id.recipe,
                    name: id.name.as_str(),
                    fields: &canonical_fields,
                    indexes: &indexes,
                    strategy_on: on.as_deref(),
                    strategy_yields: yields.as_deref(),
                    method: spec.method,
                },
                package,
            )?,
        );
        let owner = ResourceOwner::Entity(EntityId::Intent(id.clone()));
        let mut desired = desire::contribution(&owner, &change, project)?;
        provenance::stamp_files(
            &mut desired,
            project,
            RendererId::Recipe(id.recipe),
            Some(RenderedSubjectContext::Entity {
                id: EntityId::Intent(id.clone()),
                spec: EntitySpec::Intent(spec.clone()),
            }),
        )?;
        reads = reads.merge(declaration(project, &change, &desired)?);
        entities.insert(
            EntityId::Intent(id.clone()),
            DesiredEntity {
                id: EntityId::Intent(id.clone()),
                spec: EntitySpec::Intent(spec),
                owners: BTreeSet::from([OwnerId::DirectCli]),
            },
        );
        changes.push(desired);
    }

    Ok(CompanionUpdates {
        changes,
        entities,
        reads,
    })
}
