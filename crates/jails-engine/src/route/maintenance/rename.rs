//! `jails rename <Old> <New>` — a type and everything that names it, as one
//! transition.

use super::*;

mod cutover;
mod resource;
mod source;
use cutover::{prepare_cutover, prepare_owned_object_renames, validate_cutover_sql};
pub use resource::{RenameResourceInvocation, rename_resource, rename_storage};
use source::{
    carried_resource_reads, rename_destination, renamed_entities, validate, walked_directories,
};

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
    refuse_storage_backed(run, old, new)?;
    rename_with(run, old, new, force, None)
}

/// Refuse the textual rename for a resource that owns a table.
///
/// The textual rename carries the Java and nothing else. On a storage-backed
/// entity that is not a partial success, it is a divergence: the adapter is
/// rewritten to `select ... from readers`, the schema history still creates
/// `members`, no migration renames anything -- and both oracles report health,
/// because every file is byte-identical to what jails wrote and every
/// migration applies. Flyway then stops the application on the first query.
///
/// `rename resource` is the command that plans both halves, so this names it
/// with the strategy spelled out rather than leaving the reader to discover
/// that a whole second verb exists.
fn refuse_storage_backed(run: &Run, old: &str, new: &str) -> Result<()> {
    let store = observed(run.project())?;
    let Some(lifecycle) = store.lifecycles().iter().find(|lifecycle| {
        lifecycle.expected_path.name().as_str() == old && lifecycle.table.is_some()
    }) else {
        return Ok(());
    };
    let table = lifecycle
        .table
        .as_ref()
        .map(|binding| binding.table.as_str().to_string())
        .unwrap_or_default();
    Err(format!(
        "`{old}` is backed by table `{table}`, and this rename carries only the Java.\n       \
         fix: keep the table with `jails rename resource {old} {new} --strategy preserve-table`, \
         or move it with `--strategy single-cutover --table <new-table>`."
    )
    .into())
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
    let mut cutover = prepare_cutover(project, &store, resource_request.as_ref())?;
    let renamed_entities = renamed_entities(
        &store,
        old,
        new,
        resource_request
            .as_ref()
            .map(|(_, request)| &request.entity),
    )?;

    // The walk that finds the sources is not itself a read the snapshot can
    // guard -- something has to look first. What it finds is then declared,
    // directories included, so the recheck covers both the contents of every
    // file considered and the membership of every directory walked.
    let mut reads = carried_resource_reads(&store, &renamed_entities, capture::capability_reads()?);
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
    if let Some(cutover) = &mut cutover {
        validate_cutover_sql(&snapshot, cutover)?;
        let entity = &resource_request
            .as_ref()
            .expect("a cutover has a coordinated resource request")
            .1
            .entity;
        prepare_owned_object_renames(project, &snapshot, &store, entity, cutover)?;
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
        let renderer = claim
            .as_ref()
            .and_then(|(_, owners)| {
                owners.iter().find_map(|owner| match owner {
                    ResourceOwner::Entity(entity) => renamed_entities
                        .values()
                        .find(|(renamed, _, _)| renamed == entity),
                    _ => None,
                })
            })
            .map(|(entity, spec, _)| {
                let EntityId::Intent(id) = entity else {
                    unreachable!("renamed generated entities are intents")
                };
                provenance::provenance(
                    project,
                    RendererId::Recipe(id.recipe),
                    Some(RenderedSubjectContext::Entity {
                        id: entity.clone(),
                        spec: spec.clone(),
                    }),
                )
            })
            .transpose()?;
        change.files.push(DesiredFile {
            path: destination,
            body: DesiredBody::Bytes(updated.into_bytes().into()),
            mode: None,
            resource: claim.map(|(key, _)| key),
            renderer,
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

    // Identity moves do not relinquish unchanged resources. The generated
    // create migration is the important case: it stays byte-identical, but
    // its ownership must move from the old entity id to the renamed one so a
    // later rolling completion can still prove its constraint/index names
    // are generator-owned.
    carry_renamed_resources(&store, &renamed_entities, &mut change)?;

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
    let owners = row
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

fn owned_by(store: &ObservedStore, source: &ProjectPath, entity: &EntityId) -> bool {
    store
        .ledger
        .iter()
        .flat_map(|ledger| ledger.resources.iter())
        .find(|row| row.key == ResourceKey::WholeFile(source.clone()))
        .is_some_and(|row| row.owners.contains(&ResourceOwner::Entity(entity.clone())))
}

fn carry_renamed_resources(
    store: &ObservedStore,
    renamed: &BTreeMap<EntityId, (EntityId, EntitySpec, BTreeSet<OwnerId>)>,
    change: &mut DesiredChange,
) -> Result<()> {
    let replaced = change
        .resources
        .iter()
        .map(|resource| resource.key.clone())
        .chain(
            change
                .absences
                .iter()
                .map(|absence| absence.resource.clone()),
        )
        .collect::<BTreeSet<_>>();
    for row in store
        .ledger
        .iter()
        .flat_map(|ledger| ledger.resources.iter())
    {
        if replaced.contains(&row.key) {
            continue;
        }
        let mut changed = false;
        let owners = row
            .owners
            .iter()
            .map(|owner| match owner {
                ResourceOwner::Entity(entity) => match renamed.get(entity) {
                    Some((renamed, _, _)) => {
                        changed = true;
                        ResourceOwner::Entity(renamed.clone())
                    }
                    None => owner.clone(),
                },
                _ => owner.clone(),
            })
            .collect::<BTreeSet<_>>();
        if changed {
            change.resources.push(DesiredResource::new(
                row.key.clone(),
                owners,
                row.value.clone(),
            )?);
        }
    }
    Ok(())
}
