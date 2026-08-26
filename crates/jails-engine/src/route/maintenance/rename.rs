//! `jails rename <Old> <New>` — a type and everything that names it, as one
//! transition.

use super::*;

mod cutover;
use cutover::{prepare_cutover, validate_cutover_sql};

/// Rename a Java type across the project, as one transition.
///
/// **Reach for the language server first.** Neovim's `grn` (jdt.ls rename)
/// understands scope, so it will not touch an unrelated `Reward` in another
/// package, and where it works it is strictly better than this command. What
/// this exists for is the case jdt.ls cannot serve: the server is not attached,
/// the project does not currently compile (jdt.ls degrades badly there, and a
/// rename is often exactly how you are trying to fix it), or the rename has to
/// reach a file no buffer has opened.
///
/// It is textual, and two properties are what keep textual honest.
/// `jails_java::identifier` holds both: `Reward` never matches inside
/// `RewardHistory`, so the classic sed disaster cannot happen; and string
/// literals are left alone, because a literal is data and silently rewriting
/// `"Reward not found"` is a change nobody asked for. A literal that genuinely
/// names the class -- a `Class.forName` argument -- is therefore missed, which
/// is the safe direction and is reported rather than hidden.
///
/// The reason this is worth routing is written in V1's own source: it writes
/// every file's new contents, *then* moves the files, with a comment saying
/// that order at least leaves "one consistent state" if a write fails partway.
/// That is a defence against a partial rename, not a prevention of one -- a
/// rename that stops halfway leaves a project that does not compile, and
/// nothing records where it stopped.
///
/// Here every rewrite and every move is one commit. A move is a `Create` at
/// the destination and a `Delete` at the source in the same operation list, so
/// there is no moment where a file exists under both names or neither.
///
/// **Every `.java` file under `src/` is a declared read**, along with every
/// directory it walks. That is not bookkeeping: it is what makes §R4.3 step 2
/// able to refuse a rename planned against a tree somebody has changed since.
/// A file added under `src/` between planning and committing changes a
/// captured listing, and a rename that would silently skip it fails instead.
pub fn rename(run: &Run, old: &str, new: &str, force: bool) -> Result<Outcome> {
    rename_with(run, old, new, force, None)
}

fn rename_with(
    run: &Run,
    old: &str,
    new: &str,
    force: bool,
    resource_request: Option<(String, jails_protocol::request::RenameResourceRequestV1)>,
) -> Result<Outcome> {
    let project = run.project();
    // Refused by name before anything is read. `JavaType::parse` accepts a
    // qualified name by splitting at the last dot, so without this a
    // `com.example.Reward` would be quietly read as the simple name `Reward`
    // in package `com.example` -- and then matched textually against every
    // source, which is not what the caller asked for at all.
    validate(old, new)?;
    let from = jails_protocol::identity::JavaType::parse(old)?;
    let to = jails_protocol::identity::JavaType::parse(new)?;
    let store = observed(project)?;
    let cutover = prepare_cutover(project, &store, resource_request.as_ref())?;

    // The walk that finds the sources is not itself a read the snapshot can
    // guard -- something has to look first. What it finds is then declared,
    // directories included, so the recheck covers both the contents of every
    // file considered and the membership of every directory walked.
    let mut reads = capture::capability_reads()?;
    if let Some(cutover) = &cutover {
        reads = reads
            .directory(ProjectPath::parse("src/main/resources/db/migration")?)
            .file(cutover.migration.clone());
        for source in &cutover.sql_sources {
            reads = reads.file(source.clone());
        }
        for directory in walked_directories(&cutover.sql_sources) {
            reads = reads.directory(directory);
        }
    }
    let mut sources = Vec::new();
    for absolute in jails_java::java::source_files(&project.root().join("src")) {
        let relative = super::relative_path(project, &absolute)?;
        // The destination is declared as well, and its absence is the fact
        // the plan depends on. A file appearing at a destination between
        // planning and committing must fail the precondition rather than be
        // silently replaced -- which is only possible if the emptiness was
        // recorded in the first place. Every destination is derivable from
        // the source path alone, so this costs no second walk.
        reads = reads.file(rename_destination(
            &store,
            &relative,
            old,
            new,
            resource_request
                .as_ref()
                .map(|(_, request)| &request.entity),
        )?);
        reads = reads.file(relative.clone());
        sources.push(relative);
    }
    for directory in walked_directories(&sources) {
        reads = reads.directory(directory);
    }
    let (snapshot, _) = capture::projected(project, &reads)?;
    if let Some(cutover) = &cutover {
        validate_cutover_sql(&snapshot, cutover)?;
    }

    // Which entities this rename is an identity transition *for*. An entity
    // is named by its `IntentId`, and the name is half of that -- so
    // `g record Reward` followed by `rename Reward Bonus` leaves a project
    // whose `Bonus.java` is owned by an entity called `Reward`, and
    // `destroy record Bonus` finds nothing while `destroy record Reward`
    // strands the file it claims to delete. plan.md §R2.5 permits exactly
    // this: a maintenance change may propose rows owned by real entities when
    // its `LedgerIntent` describes the exact identity transition.
    let mut renamed_entities = BTreeMap::new();
    for applied in store.ledger.iter().flat_map(|ledger| ledger.applied.iter()) {
        let EntityId::Intent(id) = &applied.id else {
            continue;
        };
        if id.name.as_str() != old
            || resource_request
                .as_ref()
                .is_some_and(|(_, request)| request.entity != applied.id)
        {
            continue;
        }
        let mut renamed = id.clone();
        renamed.name = jails_protocol::identity::Name::parse(new)?;
        renamed_entities.insert(
            applied.id.clone(),
            (
                EntityId::Intent(renamed),
                applied.version.spec.clone(),
                applied.owners.clone(),
            ),
        );
    }

    let mut change = DesiredChange::maintenance(MaintenanceAttribution::Rename);
    let mut moved = 0usize;
    let mut occurrences = 0usize;
    let mut in_literals = 0usize;
    let mut manual_java = BTreeSet::new();
    for source in &sources {
        let jails_protocol::snapshot::Captured::Present(file) = snapshot.read(source)? else {
            continue;
        };
        let Ok(text) = std::str::from_utf8(&file.bytes) else {
            continue;
        };
        let (mut updated, hits) = match resource_request {
            Some(_) => jails_java::identifier::replace_owned_identifier_component(text, old, new),
            None => jails_java::identifier::replace_identifier(text, old, new),
        };
        let literal_hits = jails_java::identifier::literal_mentions(text, old);
        if let Some((_, request)) = &resource_request {
            let owned = owned_by(&store, source, &request.entity);
            let table_hits = cutover.as_ref().map_or(0, |cutover| {
                jails_java::identifier::bounded_mentions(text, cutover.current.as_str())
            });
            if !owned && (hits > 0 || literal_hits > 0 || table_hits > 0) {
                manual_java.insert(source.clone());
                continue;
            }
            if owned && let Some(cutover) = &cutover {
                (updated, _) = jails_java::identifier::replace_literal_sql_identifier(
                    &updated,
                    cutover.current.as_str(),
                    cutover.target.as_str(),
                );
            }
        }
        let destination = rename_destination(
            &store,
            source,
            old,
            new,
            resource_request
                .as_ref()
                .map(|(_, request)| &request.entity),
        )?;
        if hits == 0 && &destination == source {
            continue;
        }
        occurrences += hits;
        in_literals += literal_hits;
        if &destination != source {
            // Refused from the capture, so a destination that appears between
            // planning and committing fails the precondition rather than
            // being quietly overwritten.
            if let jails_protocol::snapshot::Captured::Present(_) = snapshot.read(&destination)? {
                return Err(
                    format!("{destination} already exists -- rename or delete it first").into(),
                );
            }
            moved += 1;
            change.absences.push(jails_protocol::render::ManagedPath {
                path: source.clone(),
                resource: ResourceKey::WholeFile(source.clone()),
                // Never the caller's `--force`, which means "do not ask me".
                // This one overrides the *preimage guard*, and a rename that
                // skipped it could delete a source somebody changed while the
                // plan was being made -- while recreating stale bytes under
                // the new name. Two different meanings for one word is how
                // that gets shipped.
                force: false,
            });
        }
        // A file the store already owns keeps its owner, at its new key.
        // The maintenance tag is never the contributor -- what carries
        // forward is the entity that owned it, under the renamed identity.
        let claim = owner_of(&store, source, &destination, &renamed_entities);
        if let Some((key, owners)) = &claim {
            change.resources.push(DesiredResource::new(
                key.clone(),
                owners.clone(),
                ResourceValue::WholeFile,
            )?);
        }
        change.files.push(DesiredFile {
            path: destination,
            body: DesiredBody::Bytes(updated.into_bytes().into()),
            mode: None,
            resource: claim.map(|(key, _)| key),
            renderer: None,
        });
    }

    if let Some((_, request)) = &resource_request {
        let (renamed, spec, _) = renamed_entities
            .get(&request.entity)
            .ok_or("resource rename did not resolve its output provenance.\n       fix: prepare the rename again from the adopted entity")?;
        let EntityId::Intent(id) = renamed else {
            return Err("resource rename provenance requires an intent identity.\n       fix: reconcile the resource declaration before retrying".into());
        };
        provenance::stamp_files(
            &mut change,
            project,
            RendererId::Recipe(id.recipe),
            Some(RenderedSubjectContext::Entity {
                id: renamed.clone(),
                spec: spec.clone(),
            }),
        )?;
    }

    if let Some(cutover) = &cutover {
        let (_, request) = resource_request
            .as_ref()
            .expect("a storage cutover always has a resource request");
        let renamed = renamed_entities
            .get(&request.entity)
            .map(|(entity, _, _)| entity.clone())
            .ok_or("resource cutover did not resolve the renamed durable identity.\n       fix: prepare the rename again from the adopted entity")?;
        let key = ResourceKey::WholeFile(cutover.migration.clone());
        let owner = ResourceOwner::Entity(renamed);
        change.resources.push(DesiredResource::new(
            key.clone(),
            BTreeSet::from([owner]),
            ResourceValue::WholeFile,
        )?);
        change.files.push(DesiredFile {
            path: cutover.migration.clone(),
            body: DesiredBody::Bytes(cutover.artifact.contents.as_bytes().into()),
            mode: None,
            resource: Some(key),
            renderer: None,
        });
        println!(
            "physical-table-cutover: {} -> {} ({})",
            cutover.current.as_str(),
            cutover.target.as_str(),
            cutover.migration
        );
    }

    if !manual_java.is_empty() {
        let paths = manual_java
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n         ");
        return Err(format!(
            "manual-edit-required: reader-owned Java references the renamed resource:\n         {paths}\n       fix: update those references through a Java-aware rename, then rerun this exact resource rename"
        )
        .into());
    }

    if change.files.is_empty() {
        return Err(format!(
            "no .java file under src/ mentions `{old}` -- check the spelling, or the type may \
             live outside this module"
        )
        .into());
    }
    // The rename's known blind spot, said out loud. An unmentioned exception
    // is indistinguishable from a bug.
    if in_literals > 0 {
        println!(
            "{in_literals} mention(s) inside string literals were left alone. Check them by hand:"
        );
        println!("  grep -rn '\"[^\"]*{old}' src/");
    }
    println!(
        "{} file(s), {occurrences} occurrence(s), {moved} file rename(s).",
        change.files.len()
    );

    let subject = match &resource_request {
        Some((_, request)) => PlannedSubject::RenameResource(Box::new(request.clone())),
        None => PlannedSubject::Rename {
            from: from.clone(),
            to: to.clone(),
            force,
        },
    };
    let set = DesiredChangeSet {
        ledger_intent: LedgerIntent {
            generation_before: store.generation(),
            // The renamed identities arrive and the old ones leave, in the
            // same intent. Removing the old is what drops its rows -- a
            // resource whose last owner has gone has lost its last owner --
            // so every row the renamed entity keeps has to be re-declared
            // under the new owner, not only the ones whose path moved.
            entities_after: renamed_entities
                .values()
                .map(
                    |(id, spec, owners)| jails_protocol::plan::DesiredAppliedEntity {
                        id: id.clone(),
                        spec: spec.clone(),
                        owners: owners.clone(),
                    },
                )
                .collect(),
            one_shots_after: Vec::new(),
            resources_after: change.resources.clone(),
            entities_removed: renamed_entities.keys().cloned().collect(),
        },
        ordered: vec![change],
        subject,
    };
    set.validate()?;
    let asked = match resource_request {
        Some((selector, request)) => {
            let mut options = BTreeMap::from([(
                "strategy".to_string(),
                vec![rename_strategy_name(request.strategy).to_string()],
            )]);
            if let Some(table) = &request.target_table {
                options.insert("table".to_string(), vec![table.as_str().to_string()]);
            }
            options.insert(
                "api".to_string(),
                vec![external_policy_name(request.api).to_string()],
            );
            if let Some(route) = &request.target_route {
                options.insert("route".to_string(), vec![route.as_str().to_string()]);
            }
            Asked::new(
                CanonicalMutationRequest::RenameResource(request),
                &["rename", "resource"],
                vec![selector, new.to_string()],
                options,
                match force {
                    true => BTreeSet::from(["force".to_string()]),
                    false => BTreeSet::new(),
                },
            )
        }
        None => Asked::new(
            CanonicalMutationRequest::Rename {
                from: from.clone(),
                to: to.clone(),
                force,
            },
            &["rename"],
            vec![old.to_string(), new.to_string()],
            BTreeMap::new(),
            match force {
                true => BTreeSet::from(["force".to_string()]),
                false => BTreeSet::new(),
            },
        ),
    };
    commit_set(run, set, &reads, &asked)
}

/// Resource-oriented spelling of rename.
///
/// The selector is deliberately parsed before the legacy identifier planner
/// runs, so `Billing.Task` cannot be mistaken for a Java package-qualified
/// textual rename. The storage-specific plan is added by the coordinated
/// planner; until then only the already-complete preserve-table transition is
/// accepted here.
pub struct RenameResourceInvocation<'a> {
    pub selector: &'a str,
    pub new: &'a str,
    pub strategy: jails_protocol::request::RenameStrategy,
    pub target_table: Option<&'a str>,
    pub api: jails_protocol::request::ExternalRenamePolicy,
    pub target_route: Option<&'a str>,
    pub force: bool,
}

pub fn rename_resource(run: &Run, invocation: RenameResourceInvocation<'_>) -> Result<Outcome> {
    let RenameResourceInvocation {
        selector,
        new,
        strategy,
        target_table,
        api,
        target_route,
        force,
    } = invocation;
    let (slice, current) = selector.split_once('.').ok_or_else(|| {
        format!(
            "`{selector}` is not a resource selector.\n       fix: use `<slice>.<current-name>`, for example `Billing.Task`"
        )
    })?;
    if slice.is_empty() || current.is_empty() || current.contains('.') {
        return Err(format!(
            "`{selector}` is not a resource selector.\n       fix: use exactly `<slice>.<current-name>`, for example `Billing.Task`"
        )
        .into());
    }
    validate(current, new)?;
    let target_table = target_table
        .map(jails_protocol::identity::SqlName::parse)
        .transpose()?;
    let route = target_route
        .map(jails_protocol::application::RoutePath::parse)
        .transpose()?;
    let project = run.project();
    let store = observed(project)?;
    let mut candidates = store
        .ledger
        .iter()
        .flat_map(|ledger| ledger.applied.iter())
        .filter_map(|applied| match (&applied.id, &applied.version.spec) {
            (EntityId::Intent(id), EntitySpec::Intent(_)) if id.name.as_str() == current => {
                let path = store
                    .lifecycles()
                    .iter()
                    .find(|lifecycle| lifecycle.entity == applied.id)
                    .map(|lifecycle| lifecycle.expected_path.clone())
                    .unwrap_or_else(|| JavaType::new(id.package.clone(), id.name.clone()));
                Some((applied.id.clone(), path))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    if candidates.len() > 1 {
        let wanted = slice.to_ascii_lowercase();
        candidates.retain(|(_, path)| {
            path.package()
                .as_str()
                .rsplit('.')
                .any(|part| part.eq_ignore_ascii_case(&wanted))
        });
    }
    let [(entity, expected_path)] = candidates.as_slice() else {
        return Err(match candidates.len() {
            0 => format!(
                "no managed resource matches `{selector}`.\n       fix: inspect `jails resource status`, then use its exact slice and current name"
            ),
            count => format!(
                "`{selector}` matches {count} managed resources.\n       fix: use the slice that uniquely identifies the resource"
            ),
        }
        .into());
    };
    let lifecycle = store
        .lifecycles()
        .iter()
        .find(|lifecycle| lifecycle.entity == *entity)
        .ok_or_else(|| {
            format!(
                "`{selector}` has no adopted resource lifecycle.\n       fix: run `jails resource status {current}` to adopt and inspect its storage binding"
            )
        })?;
    let current_table = lifecycle.table.as_ref().ok_or_else(|| {
        format!(
            "`{selector}` has no explicit table binding.\n       fix: adopt its storage binding before a coordinated rename"
        )
    })?;
    if lifecycle.expected_path != *expected_path {
        return Err(format!(
            "`{selector}` is stale: the lifecycle path is `{}`.\n       fix: rerun the rename with the current resource path",
            lifecycle.expected_path.qualified()
        )
        .into());
    }
    let request = jails_protocol::request::RenameResourceRequestV1 {
        entity: entity.clone(),
        expected_path: expected_path.clone(),
        new_name: Name::parse(new)?,
        strategy,
        target_table: target_table.clone(),
        api,
        target_route: route,
    };
    request.validate()?;
    if api == jails_protocol::request::ExternalRenamePolicy::Rename {
        return Err("`--api rename` requires the contract compatibility planner.\n       fix: omit it to preserve routes, JSON names, operation IDs, events, and error codes".into());
    }
    match strategy {
        jails_protocol::request::RenameStrategy::PreserveTable => {
            if target_table.is_some() {
                return Err("`--table` is not used by `preserve-table`.\n       fix: omit `--table`; the current physical binding will be retained".into());
            }
            println!("physical-table-preserved: {}", current_table.table.as_str());
            rename_with(
                run,
                current,
                new,
                force,
                Some((selector.to_string(), request)),
            )
        }
        jails_protocol::request::RenameStrategy::SingleCutover => {
            let conventional_current =
                jails_protocol::identity::SqlName::conventional_table(&Name::parse(current)?);
            let target = match target_table {
                Some(target) => target,
                None if current_table.table == conventional_current => {
                    jails_protocol::identity::SqlName::conventional_table(&Name::parse(new)?)
                }
                None => {
                    return Err(format!(
                        "`{selector}` has explicit table binding `{}`.\n       fix: pass `--table <target-table>` or use `--strategy preserve-table`",
                        current_table.table.as_str()
                    )
                    .into());
                }
            };
            if target == current_table.table {
                return Err(format!(
                    "target table `{}` is already the current binding.\n       fix: choose a distinct target table or use `--strategy preserve-table`",
                    target.as_str()
                )
                .into());
            }
            let mut request = request;
            request.target_table = Some(target);
            rename_with(
                run,
                current,
                new,
                force,
                Some((selector.to_string(), request)),
            )
        }
        jails_protocol::request::RenameStrategy::Rolling => {
            let conventional_current =
                jails_protocol::identity::SqlName::conventional_table(&Name::parse(current)?);
            let target = match target_table {
                Some(target) => target,
                None if current_table.table == conventional_current => {
                    jails_protocol::identity::SqlName::conventional_table(&Name::parse(new)?)
                }
                None => {
                    return Err(format!(
                        "`{selector}` has explicit table binding `{}`.\n       fix: pass `--table <target-table>` or use `--strategy preserve-table`",
                        current_table.table.as_str()
                    )
                    .into());
                }
            };
            if target == current_table.table {
                return Err(format!(
                    "target table `{}` is already the current binding.\n       fix: choose a distinct target table or use `--strategy preserve-table`",
                    target.as_str()
                )
                .into());
            }
            let mut request = request;
            request.target_table = Some(target);
            let campaign = request.campaign_id()?;
            let outcome = rename_with(
                run,
                current,
                new,
                force,
                Some((selector.to_string(), request)),
            )?;
            println!("rename-campaign: {}", campaign.to_hex());
            println!(
                "next: jails rename storage {slice}.{new} --complete {} --old-version-retired",
                campaign.to_hex()
            );
            Ok(outcome)
        }
    }
}

pub fn rename_storage(
    run: &Run,
    selector: &str,
    campaign: &str,
    old_version_retired: bool,
    force: bool,
) -> Result<Outcome> {
    cutover::complete_storage_rename(run, selector, campaign, old_version_retired, force)
}

fn complete_storage_set(
    store: &ObservedStore,
    applied: &jails_protocol::record::AppliedEntity,
    change: DesiredChange,
    request: jails_protocol::request::CompleteStorageRenameRequestV1,
) -> Result<DesiredChangeSet> {
    let set = DesiredChangeSet {
        ledger_intent: LedgerIntent {
            generation_before: store.generation(),
            entities_after: vec![jails_protocol::plan::DesiredAppliedEntity {
                id: applied.id.clone(),
                spec: applied.version.spec.clone(),
                owners: applied.owners.clone(),
            }],
            one_shots_after: Vec::new(),
            resources_after: change.resources.clone(),
            entities_removed: Vec::new(),
        },
        ordered: vec![change],
        subject: PlannedSubject::CompleteStorageRename(Box::new(request)),
    };
    set.validate()?;
    Ok(set)
}

fn rename_strategy_name(strategy: jails_protocol::request::RenameStrategy) -> &'static str {
    match strategy {
        jails_protocol::request::RenameStrategy::PreserveTable => "preserve-table",
        jails_protocol::request::RenameStrategy::SingleCutover => "single-cutover",
        jails_protocol::request::RenameStrategy::Rolling => "rolling",
    }
}

fn external_policy_name(policy: jails_protocol::request::ExternalRenamePolicy) -> &'static str {
    match policy {
        jails_protocol::request::ExternalRenamePolicy::Preserve => "preserve",
        jails_protocol::request::ExternalRenamePolicy::Rename => "rename",
    }
}

/// One simple Java type name, in and out.
fn validate(old: &str, new: &str) -> Result<()> {
    for (label, name) in [("old", old), ("new", new)] {
        if name.is_empty() {
            return Err(format!("the {label} name is empty").into());
        }
        if !name
            .chars()
            .next()
            .is_some_and(|c| c.is_alphabetic() || c == '_')
        {
            return Err(format!(
                "`{name}` is not a Java identifier -- the {label} name must start with a letter"
            )
            .into());
        }
        if !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return Err(format!(
                "`{name}` is not a Java identifier. `jails rename` renames one type, not a \
                 package path -- pass the simple name (`Reward`, not `com.example.Reward`)"
            )
            .into());
        }
    }
    if old == new {
        return Err("the old and new names are the same".into());
    }
    Ok(())
}

/// The claim a moved file carries forward, if the store had one.
///
/// Keyed at the *destination*, owned by the *renamed* entity: the row moves
/// with the file, which is what keeps `destroy` able to find it.
fn owner_of(
    store: &ObservedStore,
    source: &ProjectPath,
    destination: &ProjectPath,
    renamed: &BTreeMap<EntityId, (EntityId, EntitySpec, BTreeSet<OwnerId>)>,
) -> Option<(ResourceKey, BTreeSet<ResourceOwner>)> {
    let row = store
        .ledger
        .iter()
        .flat_map(|ledger| ledger.resources.iter())
        .find(|row| row.key == ResourceKey::WholeFile(source.clone()))?;
    let owners: BTreeSet<ResourceOwner> = row
        .owners
        .iter()
        .map(|owner| match owner {
            ResourceOwner::Entity(id) => match renamed.get(id) {
                Some((renamed, _, _)) => ResourceOwner::Entity(renamed.clone()),
                None => owner.clone(),
            },
            other => other.clone(),
        })
        .collect();
    Some((ResourceKey::WholeFile(destination.clone()), owners))
}

/// Where a source ends up, as a project path.
fn destination_of(source: &ProjectPath, old: &str, new: &str) -> Result<ProjectPath> {
    let renamed =
        jails_java::identifier::renamed_path(std::path::Path::new(&source.to_string()), old, new);
    ProjectPath::parse(
        renamed
            .to_str()
            .ok_or_else(|| format!("`{}` is not valid UTF-8", renamed.display()))?,
    )
}

fn rename_destination(
    store: &ObservedStore,
    source: &ProjectPath,
    old: &str,
    new: &str,
    resource: Option<&EntityId>,
) -> Result<ProjectPath> {
    let Some(entity) = resource else {
        return destination_of(source, old, new);
    };
    let owned = owned_by(store, source, entity);
    if !owned {
        return Ok(source.clone());
    }
    let path = std::path::Path::new(source.as_str());
    let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
        return Ok(source.clone());
    };
    let Some(position) = stem.find(old) else {
        return Ok(source.clone());
    };
    let mut renamed = stem.to_string();
    renamed.replace_range(position..position + old.len(), new);
    let destination = path.with_file_name(format!("{renamed}.java"));
    ProjectPath::parse(
        destination
            .to_str()
            .ok_or_else(|| format!("`{}` is not valid UTF-8", destination.display()))?,
    )
}

fn owned_by(store: &ObservedStore, source: &ProjectPath, entity: &EntityId) -> bool {
    store
        .ledger
        .iter()
        .flat_map(|ledger| ledger.resources.iter())
        .find(|row| row.key == ResourceKey::WholeFile(source.clone()))
        .is_some_and(|row| row.owners.contains(&ResourceOwner::Entity(entity.clone())))
}

/// Every directory the walk passed through, so its membership is guarded too.
///
/// Derived from the files rather than collected during the walk: a directory
/// with no `.java` file in it contributes nothing this rename could have
/// missed, and declaring it would guard a listing no decision depended on.
fn walked_directories(sources: &[ProjectPath]) -> BTreeSet<ProjectPath> {
    let mut out = BTreeSet::new();
    for source in sources {
        let text = source.to_string();
        if let Some((directory, _)) = text.rsplit_once('/')
            && let Ok(path) = ProjectPath::parse(directory)
        {
            out.insert(path);
        }
    }
    out
}
