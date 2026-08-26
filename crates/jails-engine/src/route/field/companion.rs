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
) -> Result<CompanionUpdates> {
    refuse_stale_strategy_companions(store, primary)?;
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
    Ok(CompanionUpdates {
        changes,
        entities,
        reads,
    })
}

fn refuse_stale_strategy_companions(store: &ObservedStore, primary: &IntentId) -> Result<()> {
    let mut dependents = store
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
            .then(|| format!("{} {}", label(id.recipe), id.name))
        })
        .collect::<Vec<_>>();
    dependents.sort();
    dependents.dedup();
    if dependents.is_empty() {
        return Ok(());
    }
    Err(format!(
        "evolving fields on `{}` would leave generated companions stale: {}\n       fix: keep the current field list, or regenerate those companions after the resource shape is stable.",
        primary.name,
        dependents.join(", ")
    )
    .into())
}
