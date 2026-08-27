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
        let recorded = Recorded::read(&after);
        let mut change = with_test_support(
            project,
            jails_generate::generate::plan_recipe(
                project,
                &recorded.recipe(id.recipe, id.name.as_str(), &canonical_fields, &indexes),
                package,
            )?,
        );
        drop_migrations(project, &mut change);
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

/// A regenerated companion must not re-emit its migration.
///
/// The schema change belongs to the evolution itself: the companion's own
/// `create table` was applied when it was first generated, and planning it
/// again would either collide with the file on disk or append a second
/// version of a migration that has already run.
fn drop_migrations(project: &Project, change: &mut Change) {
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
}

/// The recipes that read the evolved resource's component list off disk.
///
/// Every one of them calls `Target::read` (or reads the record directly) and
/// renders a constructor call, a row mapper or a column list from whatever
/// `<Name>.java` said at the time — so adding one component makes each of
/// them stale, and stale here means a project that no longer compiles. A
/// recipe whose only use of `--on` is to name a type in a signature is
/// deliberately absent: nothing it wrote stopped being true.
const READS_THE_TARGETS_COMPONENTS: [ArtifactKind; 5] = [
    ArtifactKind::Query,
    ArtifactKind::Transition,
    ArtifactKind::Usecase,
    ArtifactKind::Association,
    ArtifactKind::DurableJob,
];

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
            // `--yields` as well as `--on`: an `association` names its parent
            // there, and a `usecase --yields <Event>` reads the event's
            // components to build the outbox payload. Matching only `--on`
            // left both of those constructing a type that had changed.
            let names_primary = [spec.on.as_ref(), spec.yields.as_ref()]
                .into_iter()
                .flatten()
                .any(|referenced| {
                    referenced.name() == &primary.name
                        && (referenced.package().is_base()
                            || referenced.package() == &primary.package)
                });
            (names_primary && READS_THE_TARGETS_COMPONENTS.contains(&id.recipe))
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
        // Whatever shape this recipe's arguments take. An `association`'s are
        // `child=parent` mappings, not fields, and reading them as fields
        // handed `plan_recipe` an empty list -- which it correctly refused as
        // an association with no mapping, in the middle of an unrelated `g
        // field`.
        let canonical_fields = spec.arguments.canonical();
        let indexes = spec
            .indexes
            .iter()
            .map(|index| index.canonical())
            .collect::<Vec<_>>();
        let recorded = Recorded::read(&spec);
        let mut change = with_test_support(
            &projected,
            jails_generate::generate::plan_recipe(
                &projected,
                &recorded.recipe(id.recipe, id.name.as_str(), &canonical_fields, &indexes),
                package,
            )?,
        );
        drop_migrations(project, &mut change);
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
