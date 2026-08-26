//! The coordinated verbs: `rename resource` and `rename storage`.
//!
//! A different secret from [`super`], which renames a Java identifier across
//! the tree and knows nothing about tables. These two carry one *resource
//! identity* across generated Java, the ledger, the migration lineage and the
//! physical table -- so the storage strategy, the campaign id and the
//! two-phase cutover live here, and the textual planner they both drive stays
//! where it is.

use super::*;

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
    // A bare name is a selector. The slice qualifier exists to *disambiguate*,
    // and demanding it unconditionally made this command unreachable from any
    // imperative project: `jails g scaffold Member` records no slice, so every
    // spelling of `Member` was refused and the coordinated rename -- the one
    // path that carries the storage -- could not be run at all.
    let (slice, current) = match selector.split_once('.') {
        Some((slice, current)) => (Some(slice), current),
        None => (None, selector),
    };
    if current.is_empty() || current.contains('.') || slice.is_some_and(str::is_empty) {
        return Err(format!(
            "`{selector}` is not a resource selector.\n       fix: use the resource's name, or `<slice>.<current-name>` when two slices hold the same one"
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
    if candidates.len() > 1
        && let Some(slice) = slice
    {
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
                "`{selector}` matches {count} managed resources.\n       fix: qualify it as `<slice>.{current}` with the slice that uniquely identifies the resource"
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
            // The same shape the caller used: a qualified selector stays
            // qualified, a bare one stays bare, so the line can be pasted.
            let next = match slice {
                Some(slice) => format!("{slice}.{new}"),
                None => new.to_string(),
            };
            println!(
                "next: jails rename storage {next} --complete {} --old-version-retired",
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

pub(super) fn complete_storage_set(
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
