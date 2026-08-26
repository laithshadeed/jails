//! Atomic lifecycle and migration-lineage updates for a prepared ledger.

use super::super::ObservedStore;
use super::Recorded;
use crate::{Result, pipeline::ObjectBundle};
use jails_protocol::entity::{EntityId, EntitySpec, OneShotId, TypeTargetId};
use jails_protocol::identity::{JavaType, OperationId, ProjectPath, SqlName};
use jails_protocol::lifecycle::{
    MigrationSealV1, MigrationVersion, ResourceLifecycleV1, ResourceState, TableBinding,
};
use jails_protocol::plan::{LedgerIntent, PlannedSubject};
use jails_protocol::request::StorageRetirement;
use jails_protocol::resource::{ResourceKey, ResourceOwner};
use jails_protocol::snapshot::{Captured, ProjectSnapshot};
use std::collections::BTreeSet;

#[derive(Clone, Copy)]
enum Transition {
    Evolve,
    Preserve,
    Drop,
    Revive,
    Repair,
}

struct Target {
    entity: EntityId,
    expected_path: Option<JavaType>,
    expected_table: Option<SqlName>,
    transition: Transition,
}

pub(in crate::pipeline) struct LifecycleContext<'a> {
    pub(in crate::pipeline) observed: &'a ObservedStore,
    pub(in crate::pipeline) recorded: &'a Recorded,
    pub(in crate::pipeline) base: &'a ProjectSnapshot,
    pub(in crate::pipeline) objects: &'a mut ObjectBundle,
    pub(in crate::pipeline) subject: &'a PlannedSubject,
    pub(in crate::pipeline) intent: &'a LedgerIntent,
    pub(in crate::pipeline) operation: OperationId,
}

impl Target {
    fn from_subject(subject: &PlannedSubject) -> Option<Self> {
        Some(match subject {
            PlannedSubject::EvolveField(request) => Self {
                entity: request.entity.clone(),
                expected_path: Some(request.expected_path.clone()),
                expected_table: Some(request.expected_table.clone()),
                transition: Transition::Evolve,
            },
            PlannedSubject::DestroyResourceV2(request) => {
                let (expected_table, transition) = match &request.storage {
                    StorageRetirement::Preserve { expected_table } => {
                        (expected_table.clone(), Transition::Preserve)
                    }
                    StorageRetirement::Drop { confirmed_table } => {
                        (confirmed_table.clone(), Transition::Drop)
                    }
                };
                Self {
                    entity: request.entity.clone(),
                    expected_path: Some(request.expected_path.clone()),
                    expected_table: Some(expected_table),
                    transition,
                }
            }
            PlannedSubject::ReviveResource(request) => Self {
                entity: request.entity.clone(),
                expected_path: None,
                expected_table: Some(request.expected_table.clone()),
                transition: Transition::Revive,
            },
            PlannedSubject::RepairResource(request) => Self {
                entity: request.entity.clone(),
                expected_path: Some(request.expected_path.clone()),
                expected_table: None,
                transition: Transition::Repair,
            },
            _ => return None,
        })
    }
}

/// Record the lifecycle transition named by the plan after output images have
/// been recorded, so migration seals and the ledger commit describe one state.
pub(in crate::pipeline) fn record_lifecycle(
    store: &mut jails_protocol::envelope::LedgerV2,
    mut context: LifecycleContext<'_>,
) -> Result<()> {
    if let PlannedSubject::RenameResource(request) = context.subject {
        return record_resource_rename(store, &mut context, request);
    }
    let Some(target) = Target::from_subject(context.subject) else {
        return adopt_new_scaffolds(store, &mut context);
    };

    let existing = store
        .lifecycles
        .iter()
        .find(|lifecycle| lifecycle.entity == target.entity)
        .cloned();
    validate_expected_identity(existing.as_ref(), &target)?;

    let mut lifecycle = match existing {
        Some(lifecycle) => lifecycle,
        None => bootstrap_lifecycle(context.observed, context.intent, &target)?,
    };
    let operation = context.operation;
    let published_now = seal_migrations(
        &target.entity,
        &mut lifecycle.migrations,
        store,
        &mut context,
    )?;

    if let Some(table) = target.expected_table.clone() {
        lifecycle.table = Some(TableBinding { table });
    }
    if let Some(path) = target.expected_path.clone() {
        lifecycle.expected_path = path;
    }
    if let Some(spec) = desired_spec(context.intent, &target.entity) {
        lifecycle.last_spec = spec;
    }

    lifecycle.state = match target.transition {
        Transition::Evolve => {
            require_active(&lifecycle.state, "evolve")?;
            ResourceState::Active
        }
        Transition::Preserve => {
            require_active(&lifecycle.state, "destroy")?;
            ResourceState::RetiredPreservingStorage {
                retired_by: operation,
            }
        }
        Transition::Drop => {
            require_active(&lifecycle.state, "destroy")?;
            let mut drop_paths = published_now.into_iter().collect::<Vec<_>>();
            drop_paths.sort();
            let [migration] = drop_paths.as_slice() else {
                return Err(format!(
                    "resource lifecycle expected one new drop migration for {:?}, found {}.\n       \
                     fix: prepare exactly one forward DROP TABLE migration and retry",
                    target.entity,
                    drop_paths.len()
                )
                .into());
            };
            ResourceState::RetiredDropPlanned {
                migration: migration.clone(),
                retired_by: operation,
            }
        }
        Transition::Revive => match lifecycle.state {
            ResourceState::RetiredPreservingStorage { .. } => ResourceState::Active,
            ResourceState::RetiredDropPlanned { .. } => {
                return Err(format!(
                    "resource {:?} has an append-only drop planned and cannot be revived.\n       \
                     fix: create a separately named resource with a new forward migration",
                    target.entity
                )
                .into());
            }
            ResourceState::Active => {
                return Err(format!(
                    "resource {:?} is already active.\n       fix: use `resource status` to inspect \
                     it, or evolve its fields directly",
                    target.entity
                )
                .into());
            }
        },
        Transition::Repair => lifecycle.state,
    };

    match store
        .lifecycles
        .iter_mut()
        .find(|held| held.entity == target.entity)
    {
        Some(held) => *held = lifecycle,
        None => store.lifecycles.push(lifecycle),
    }
    store
        .lifecycles
        .sort_by(|left, right| left.entity.cmp(&right.entity));
    Ok(())
}

fn adopt_new_scaffolds(
    store: &mut jails_protocol::envelope::LedgerV2,
    context: &mut LifecycleContext<'_>,
) -> Result<()> {
    let candidates = context
        .intent
        .entities_after
        .iter()
        .filter_map(|row| match (&row.id, &row.spec) {
            (EntityId::Intent(id), EntitySpec::Intent(_))
                if id.recipe == jails_spec::spec::kind::ArtifactKind::Scaffold =>
            {
                Some((row.id.clone(), id.clone(), row.spec.clone()))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    for (entity, id, spec) in candidates {
        if store
            .lifecycles
            .iter()
            .any(|lifecycle| lifecycle.entity == entity)
        {
            continue;
        }
        let mut lifecycle = ResourceLifecycleV1 {
            entity: entity.clone(),
            expected_path: owned_scaffold_type(context.intent, &entity, &id)
                .unwrap_or_else(|| JavaType::new(id.package.clone(), id.name.clone())),
            last_spec: spec,
            state: ResourceState::Active,
            table: Some(TableBinding {
                table: SqlName::conventional_table(&id.name),
            }),
            migrations: Vec::new(),
        };
        seal_migrations(&entity, &mut lifecycle.migrations, store, context)?;
        store.lifecycles.push(lifecycle);
    }
    store
        .lifecycles
        .sort_by(|left, right| left.entity.cmp(&right.entity));
    Ok(())
}

fn owned_scaffold_type(
    intent: &LedgerIntent,
    entity: &EntityId,
    id: &jails_protocol::entity::IntentId,
) -> Option<JavaType> {
    let suffix = format!("/{}.java", id.name.as_str());
    let path = intent.resources_after.iter().find_map(|resource| {
        let ResourceKey::WholeFile(path) = &resource.key else {
            return None;
        };
        (resource
            .owners
            .contains(&ResourceOwner::Entity(entity.clone()))
            && path.as_str().ends_with(&suffix))
        .then_some(path)
    })?;
    let relative = path.as_str().strip_prefix("src/main/java/")?;
    let (package, _) = relative.rsplit_once('/')?;
    Some(JavaType::new(
        jails_protocol::identity::Package::parse(&package.replace('/', ".")).ok()?,
        id.name.clone(),
    ))
}

fn record_resource_rename(
    store: &mut jails_protocol::envelope::LedgerV2,
    context: &mut LifecycleContext<'_>,
    request: &jails_protocol::request::RenameResourceRequestV1,
) -> Result<()> {
    use jails_protocol::request::RenameStrategy;

    let position = store
        .lifecycles
        .iter()
        .position(|lifecycle| lifecycle.entity == request.entity)
        .ok_or_else(|| {
            format!(
                "rename target {:?} has no adopted resource lifecycle.\n       fix: rerun `jails resource status` and prepare the rename from its current identity",
                request.entity
            )
        })?;
    let mut lifecycle = store.lifecycles[position].clone();
    if lifecycle.expected_path != request.expected_path {
        return Err(format!(
            "rename plan is stale: expected `{}`, found `{}`.\n       fix: prepare the rename again from the current resource path",
            request.expected_path.qualified(),
            lifecycle.expected_path.qualified()
        )
        .into());
    }
    require_active(&lifecycle.state, "rename")?;

    let mut after = context
        .intent
        .entities_after
        .iter()
        .filter(|candidate| !context.intent.entities_removed.contains(&candidate.id));
    let renamed = after.next().ok_or(
        "resource rename did not declare its renamed entity.\n       fix: prepare the coordinated rename again",
    )?;
    if after.next().is_some() {
        return Err("resource rename declared more than one replacement entity.\n       fix: prepare one resolved entity rename at a time".into());
    }
    if store
        .lifecycles
        .iter()
        .enumerate()
        .any(|(index, held)| index != position && held.entity == renamed.id)
    {
        return Err("the renamed identity already has lifecycle state.\n       fix: choose an unused logical name or reconcile the conflicting resource first".into());
    }

    match request.strategy {
        RenameStrategy::PreserveTable => {
            if request.target_table.is_some() {
                return Err("preserve-table may not replace the physical binding.\n       fix: omit the target table and prepare the rename again".into());
            }
        }
        RenameStrategy::SingleCutover => {
            let target = request.target_table.clone().ok_or(
                "single-cutover has no resolved target table.\n       fix: prepare the resource rename again",
            )?;
            let published =
                seal_migrations(&renamed.id, &mut lifecycle.migrations, store, context)?;
            if published.len() != 1 {
                return Err(format!(
                    "single-cutover expected one new forward migration, found {}.\n       fix: prepare exactly one table-rename migration",
                    published.len()
                )
                .into());
            }
            lifecycle.table = Some(TableBinding { table: target });
        }
        RenameStrategy::Rolling => {
            return Err("storage rename state reached the preserve-only lifecycle recorder.\n       fix: prepare the coordinated storage plan again".into());
        }
    }
    let EntityId::Intent(id) = &renamed.id else {
        return Err("direct resource rename produced a non-intent identity.\n       fix: reconcile the application manifest before retrying".into());
    };
    let EntitySpec::Intent(spec) = &renamed.spec else {
        return Err("direct resource rename produced a non-intent specification.\n       fix: reconcile the resource declaration before retrying".into());
    };
    lifecycle.entity = renamed.id.clone();
    lifecycle.expected_path = JavaType::new(id.package.clone(), id.name.clone());
    lifecycle.last_spec = EntitySpec::Intent(spec.clone());
    store.lifecycles[position] = lifecycle;
    store
        .lifecycles
        .sort_by(|left, right| left.entity.cmp(&right.entity));
    Ok(())
}

fn bootstrap_lifecycle(
    observed: &ObservedStore,
    intent: &LedgerIntent,
    target: &Target,
) -> Result<ResourceLifecycleV1> {
    if matches!(target.transition, Transition::Revive) {
        return Err(format!(
            "resource {:?} has no preserved lifecycle to revive.\n       fix: use `resource status` \
             to resolve the stable identity before retrying",
            target.entity
        )
        .into());
    }
    let expected_path = target.expected_path.clone().ok_or_else(|| {
        format!(
            "resource {:?} has no recorded source path.\n       fix: repair or adopt the resource \
             with its exact generated Java type",
            target.entity
        )
    })?;
    let last_spec = desired_spec(intent, &target.entity)
        .or_else(|| observed_spec(observed, &target.entity))
        .ok_or_else(|| {
            format!(
                "resource {:?} has no declared model to retain.\n       fix: restore its declaration \
                 or select the recorded entity before retrying",
                target.entity
            )
        })?;
    Ok(ResourceLifecycleV1 {
        entity: target.entity.clone(),
        expected_path,
        last_spec,
        state: ResourceState::Active,
        table: target
            .expected_table
            .clone()
            .map(|table| TableBinding { table }),
        migrations: Vec::new(),
    })
}

fn validate_expected_identity(
    existing: Option<&ResourceLifecycleV1>,
    target: &Target,
) -> Result<()> {
    let Some(existing) = existing else {
        return Ok(());
    };
    if target
        .expected_path
        .as_ref()
        .is_some_and(|path| path != &existing.expected_path)
    {
        return Err(format!(
            "resource {:?} no longer has the expected source path.\n       fix: rerun the command \
             against the path reported by `resource status`",
            target.entity
        )
        .into());
    }
    if let (Some(expected), Some(recorded)) = (&target.expected_table, &existing.table)
        && expected != &recorded.table
    {
        return Err(format!(
            "resource {:?} no longer has the expected table `{}`.\n       fix: rerun the \
             command with the table reported by `resource status`",
            target.entity,
            expected.as_str()
        )
        .into());
    }
    Ok(())
}

fn require_active(state: &ResourceState, action: &str) -> Result<()> {
    if matches!(state, ResourceState::Active) {
        return Ok(());
    }
    Err(format!(
        "cannot {action} a retired resource.\n       fix: inspect `resource status`; only preserved \
         storage can be revived"
    )
    .into())
}

fn desired_spec(intent: &LedgerIntent, entity: &EntityId) -> Option<EntitySpec> {
    intent
        .entities_after
        .iter()
        .find(|row| &row.id == entity)
        .map(|row| row.spec.clone())
}

fn observed_spec(observed: &ObservedStore, entity: &EntityId) -> Option<EntitySpec> {
    observed
        .ledger
        .as_ref()?
        .applied
        .iter()
        .find(|row| &row.id == entity)
        .map(|row| row.version.spec.clone())
}

fn seal_migrations(
    entity: &EntityId,
    seals: &mut Vec<MigrationSealV1>,
    store: &jails_protocol::envelope::LedgerV2,
    context: &mut LifecycleContext<'_>,
) -> Result<BTreeSet<ProjectPath>> {
    let mut observed_paths = BTreeSet::new();
    let mut paths = BTreeSet::new();
    paths.extend(seals.iter().map(|seal| seal.path.clone()));
    observed_paths.extend(seals.iter().map(|seal| seal.path.clone()));
    if let Some(ledger) = &context.observed.ledger {
        collect_resource_paths(&mut observed_paths, &ledger.resources, entity);
        paths.extend(observed_paths.iter().cloned());
    }
    collect_desired_paths(&mut paths, context.intent, entity);
    for output in &store.outputs {
        if output
            .contributors
            .iter()
            .any(|owner| owner_names(owner, entity))
            && is_migration_path(&output.path)
        {
            paths.insert(output.path.clone());
        }
    }

    for path in &paths {
        let durable_output = store.outputs.iter().find(|row| &row.path == path);
        let content_digest = match context.recorded.outputs.get(path) {
            Some((_, current)) => current.sha256,
            None => match context.base.read(path)? {
                Captured::Present(file) => {
                    context
                        .objects
                        .entry(file.sha256)
                        .or_insert_with(|| file.bytes.clone());
                    file.sha256
                }
                Captured::Absent => {
                    return Err(format!(
                        "migration-missing-after-seal: `{path}` is absent.\n       fix: restore its \
                         exact receipt object before changing the resource"
                    )
                    .into());
                }
            },
        };
        let version = version_of(path)?;
        if let Some(held) = seals
            .iter()
            .find(|seal| seal.version == version || seal.path == *path)
        {
            if held.path != *path || held.content_digest != content_digest {
                return Err(format!(
                    "migration-edited-after-seal: `{path}` differs from its sealed identity or \
                     bytes.\n       fix: restore the exact recorded migration and append a later version"
                )
                .into());
            }
            continue;
        }
        let mut contributors = durable_output
            .into_iter()
            .flat_map(|output| output.contributors.iter())
            .filter_map(entity_contributor)
            .collect::<BTreeSet<_>>();
        contributors.insert(entity.clone());
        seals.push(MigrationSealV1 {
            version,
            path: path.clone(),
            content_digest,
            contributors,
            receipt: context.operation,
        });
    }
    seals.sort_by_key(|seal| seal.version);
    Ok(paths.difference(&observed_paths).cloned().collect())
}

fn collect_resource_paths(
    paths: &mut BTreeSet<ProjectPath>,
    resources: &[jails_protocol::resource::ResourceRecord],
    entity: &EntityId,
) {
    for row in resources {
        if row.owners.iter().any(|owner| owner_names(owner, entity))
            && let Some(path) = migration_path(&row.key)
        {
            paths.insert(path.clone());
        }
    }
}

fn collect_desired_paths(
    paths: &mut BTreeSet<ProjectPath>,
    intent: &LedgerIntent,
    entity: &EntityId,
) {
    for row in &intent.resources_after {
        if row.owners.iter().any(|owner| owner_names(owner, entity))
            && let Some(path) = migration_path(&row.key)
        {
            paths.insert(path.clone());
        }
    }
}

fn migration_path(key: &ResourceKey) -> Option<&ProjectPath> {
    match key {
        ResourceKey::WholeFile(path) if key.is_migration_history() => Some(path),
        _ => None,
    }
}

fn is_migration_path(path: &ProjectPath) -> bool {
    ResourceKey::WholeFile(path.clone()).is_migration_history()
}

fn owner_names(owner: &ResourceOwner, entity: &EntityId) -> bool {
    match owner {
        ResourceOwner::Entity(owner) => owner == entity,
        ResourceOwner::OneShot(OneShotId::Field {
            target: TypeTargetId::Managed(target),
            ..
        }) => matches!(entity, EntityId::Intent(intent) if intent == target),
        _ => false,
    }
}

fn entity_contributor(owner: &ResourceOwner) -> Option<EntityId> {
    match owner {
        ResourceOwner::Entity(entity) => Some(entity.clone()),
        ResourceOwner::OneShot(OneShotId::Field {
            target: TypeTargetId::Managed(target),
            ..
        }) => Some(EntityId::Intent(target.clone())),
        _ => None,
    }
}

fn version_of(path: &ProjectPath) -> Result<MigrationVersion> {
    let file = path.as_str().rsplit('/').next().unwrap_or(path.as_str());
    let stem = file
        .strip_suffix(".sql")
        .ok_or_else(|| format!("`{path}` is not a SQL migration"))?;
    let numbered = stem.strip_prefix('V').unwrap_or(stem);
    let digit_count = numbered.bytes().take_while(u8::is_ascii_digit).count();
    let (digits, suffix) = numbered.split_at(digit_count);
    if digits.is_empty() || !suffix.starts_with('_') {
        return Err(format!(
            "`{path}` has no ordered migration version.\n       fix: use `V001__name.sql` or \
             `001_name.sql` naming"
        )
        .into());
    }
    let value = digits.parse::<u32>().map_err(|_| {
        format!(
            "migration version in `{path}` is too large.\n       fix: choose the next u32 version"
        )
    })?;
    MigrationVersion::new(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flyway_and_plain_ordered_names_yield_the_same_version() {
        let flyway =
            ProjectPath::parse("src/main/resources/db/migration/V014__add_status_to_orders.sql")
                .unwrap();
        let plain =
            ProjectPath::parse("src/main/resources/db/migration/014_add_status.sql").unwrap();
        assert_eq!(version_of(&flyway).unwrap(), version_of(&plain).unwrap());
        assert_eq!(version_of(&flyway).unwrap().get(), 14);
    }

    #[test]
    fn unordered_sql_is_not_silently_sealed() {
        let path = ProjectPath::parse("src/main/resources/db/migration/add_status.sql").unwrap();
        let error = version_of(&path).unwrap_err();
        assert!(error.contains("fix:"), "{error}");
    }
}
