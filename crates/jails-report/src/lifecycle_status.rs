//! `jails resource status`: one read-only view over every recorded authority.

use crate::find_project_root;
use jails_project::model::Project;
use jails_protocol::database::{FlywayHistoryV1, SchemaObject, SchemaObjectKind, SchemaSnapshot};
use jails_protocol::entity::{EntityId, IntentId};
use jails_protocol::lifecycle::{ResourceLifecycleV1, ResourceState};
use jails_protocol::request::CanonicalRequestSyntaxV1;
use jails_protocol::resource_status::{
    AuthorityStatus, ResourceConsistency, ResourceFindingV1, ResourceStatusV1,
};
use jails_state::compat::MachineState;
use jails_support::Result;
use std::collections::{BTreeMap, BTreeSet};

pub fn status(selector: &str, datasource: Option<&str>, json: bool) -> Result<()> {
    let root = find_project_root()?;
    let project = Project::load(&root)?;
    let report = inspect(&project, selector, datasource);
    match json {
        true => println!("{}", render_json(&report)),
        false => print!("{}", render_human(&report)),
    }
    Ok(())
}

pub fn inspect(project: &Project, selector: &str, datasource: Option<&str>) -> ResourceStatusV1 {
    let root = project.root();
    let live = datasource.map(|_| AuthorityStatus::Unknown);
    let state = jails_state::compat::read(root);
    let MachineState::Current(store) = state else {
        let message = match state {
            MachineState::Absent => "this project has no recorded resource state".to_string(),
            MachineState::Unreadable(why) => why,
            MachineState::Current(_) => unreachable!(),
        };
        return ResourceStatusV1 {
            entity: None,
            state: ResourceConsistency::Ambiguous,
            declaration: AuthorityStatus::Unknown,
            generated: AuthorityStatus::Unknown,
            migration_history: AuthorityStatus::Unknown,
            live,
            table: None,
            findings: vec![finding("resource-state-unavailable", message)],
            next_requests: Vec::new(),
        };
    };

    let matches = store
        .lifecycles
        .iter()
        .filter(|lifecycle| selector_matches(lifecycle, selector))
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        if matches.is_empty()
            && let Some(entity) = legacy_entity(&store.applied, selector)
        {
            return ResourceStatusV1 {
                entity: Some(entity),
                state: ResourceConsistency::Ambiguous,
                declaration: AuthorityStatus::Present,
                generated: AuthorityStatus::Unknown,
                migration_history: AuthorityStatus::Unknown,
                live,
                table: None,
                findings: vec![finding(
                    "lifecycle-not-recorded",
                    "the entity predates lifecycle adoption; its storage binding is not yet durable",
                )],
                next_requests: Vec::new(),
            };
        }
        return ResourceStatusV1 {
            entity: None,
            state: ResourceConsistency::Ambiguous,
            declaration: AuthorityStatus::Unknown,
            generated: AuthorityStatus::Unknown,
            migration_history: AuthorityStatus::Unknown,
            live,
            table: None,
            findings: vec![finding(
                "resource-selector-ambiguous",
                match matches.len() {
                    0 => format!("no lifecycle identity matches `{selector}`"),
                    count => format!("`{selector}` matches {count} lifecycle identities"),
                },
            )],
            next_requests: Vec::new(),
        };
    }

    let lifecycle = matches[0];
    let declaration = match store.applied.iter().any(|row| row.id == lifecycle.entity) {
        true => AuthorityStatus::Present,
        false => AuthorityStatus::Absent,
    };
    let source = root.join(format!(
        "src/main/java/{}.java",
        lifecycle.expected_path.qualified().replace('.', "/")
    ));
    let generated = match source.is_file() {
        true => AuthorityStatus::Present,
        false => AuthorityStatus::Absent,
    };

    let mut findings = Vec::new();
    let mut migration_history = AuthorityStatus::Present;
    let mut consistency = match lifecycle.state {
        ResourceState::Active => ResourceConsistency::Consistent,
        ResourceState::RetiredPreservingStorage { .. } => {
            ResourceConsistency::RetiredStoragePresent
        }
        ResourceState::RetiredDropPlanned { .. } => ResourceConsistency::DropPending,
        ResourceState::RenamePending { .. } => ResourceConsistency::RenamePending,
    };
    for seal in &lifecycle.migrations {
        match std::fs::read(root.join(seal.path.as_str())) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                migration_history = AuthorityStatus::Absent;
                consistency = ResourceConsistency::MigrationMissingAfterSeal;
                findings.push(finding(
                    "migration-missing-after-seal",
                    format!("sealed migration `{}` is missing", seal.path),
                ));
            }
            Err(error) => {
                migration_history = AuthorityStatus::Unknown;
                consistency = ResourceConsistency::Ambiguous;
                findings.push(finding(
                    "migration-unreadable",
                    format!("sealed migration `{}` cannot be read: {error}", seal.path),
                ));
            }
            Ok(bytes)
                if jails_support::identity::ObjectId::from_bytes(jails_support::codec::sha256(
                    &bytes,
                )) != seal.content_digest =>
            {
                migration_history = AuthorityStatus::Diverged;
                consistency = ResourceConsistency::MigrationEditedAfterSeal;
                findings.push(finding(
                    "migration-edited-after-seal",
                    format!("sealed migration `{}` has different bytes", seal.path),
                ));
            }
            Ok(_) => {}
        }
    }
    if matches!(lifecycle.state, ResourceState::Active)
        && generated == AuthorityStatus::Absent
        && consistency == ResourceConsistency::Consistent
    {
        consistency = ResourceConsistency::SourceDiverged;
        findings.push(finding(
            "generated-source-missing",
            format!(
                "expected generated source `{}` is missing",
                source.display()
            ),
        ));
    }
    if datasource.is_some() {
        findings.push(finding(
            "live-evidence-unavailable",
            "live catalog observation is not available in the offline lifecycle slice",
        ));
    }

    let table = lifecycle
        .table
        .as_ref()
        .map(|binding| binding.table.clone());
    let mut next_requests = Vec::new();
    if matches!(
        lifecycle.state,
        ResourceState::RetiredPreservingStorage { .. }
    ) && let Some(table) = &table
    {
        next_requests.push(syntax(
            &["resource", "revive"],
            selector,
            Some(("table", table.as_str())),
        ));
    }
    if let ResourceState::RenamePending { campaign, .. } = &lifecycle.state {
        findings.push(finding(
            "rename-campaign-awaiting-retirement",
            format!(
                "rolling rename campaign {} is waiting for the old application version to retire",
                campaign.to_hex()
            ),
        ));
        next_requests.push(CanonicalRequestSyntaxV1 {
            command_path: vec!["rename".to_string(), "storage".to_string()],
            positionals: vec![selector.to_string()],
            options: BTreeMap::from([("complete".to_string(), vec![campaign.to_hex()])]),
            flags: BTreeSet::from(["old-version-retired".to_string()]),
        });
    }
    if matches!(
        consistency,
        ResourceConsistency::MigrationEditedAfterSeal
            | ResourceConsistency::MigrationMissingAfterSeal
    ) {
        next_requests.push(syntax(
            &["resource", "repair"],
            selector,
            Some(("strategy", "roll-forward")),
        ));
    }
    ResourceStatusV1 {
        entity: Some(lifecycle.entity.clone()),
        state: consistency,
        declaration,
        generated,
        migration_history,
        live,
        table,
        findings,
        next_requests,
    }
}

/// Add independently observed live Flyway and catalog authority to the
/// offline lifecycle report. This function is pure over its typed evidence;
/// the caller owns datasource resolution and I/O.
pub fn inspect_live(
    project: &Project,
    selector: &str,
    history: &FlywayHistoryV1,
    catalog: &SchemaSnapshot,
) -> ResourceStatusV1 {
    let mut report = inspect(project, selector, None);
    report.live = Some(AuthorityStatus::Present);
    if history.applied.iter().any(|row| !row.success) {
        report.live = Some(AuthorityStatus::Diverged);
        report.state = ResourceConsistency::Ambiguous;
        report.findings.push(finding(
            "live-migration-failed",
            "Flyway history contains an unsuccessful migration",
        ));
        return report;
    }

    let Some(entity) = &report.entity else {
        return report;
    };
    let MachineState::Current(store) = jails_state::compat::read(project.root()) else {
        return report;
    };
    let Some(lifecycle) = store
        .lifecycles
        .iter()
        .find(|lifecycle| &lifecycle.entity == entity)
    else {
        return report;
    };
    let table_present = report.table.as_ref().is_some_and(|table| {
        catalog.catalog.objects.iter().any(|(id, object)| {
            id.kind == SchemaObjectKind::Table
                && id.name == *table
                && matches!(object, SchemaObject::Table)
        })
    });

    match &lifecycle.state {
        ResourceState::RetiredDropPlanned { migration, .. } => {
            let script = migration.as_str().rsplit('/').next().unwrap_or_default();
            let applied = history
                .applied
                .iter()
                .any(|row| row.success && row.script == script);
            match (applied, table_present) {
                (true, false) => report.state = ResourceConsistency::DropObservedApplied,
                (true, true) => {
                    report.live = Some(AuthorityStatus::Diverged);
                    report.state = ResourceConsistency::Ambiguous;
                    report.findings.push(finding(
                        "applied-drop-left-table",
                        format!(
                            "Flyway records `{script}` as applied but table `{}` is still present",
                            report
                                .table
                                .as_ref()
                                .map_or("unknown", |table| table.as_str())
                        ),
                    ));
                }
                (false, _) => report.state = ResourceConsistency::DropPending,
            }
        }
        ResourceState::Active | ResourceState::RetiredPreservingStorage { .. }
            if report.table.is_some() && !table_present =>
        {
            report.live = Some(AuthorityStatus::Absent);
            report.state = ResourceConsistency::RuntimeSchemaBehind;
            report.findings.push(finding(
                "live-table-missing",
                format!(
                    "table `{}` is absent from the observed catalog",
                    report
                        .table
                        .as_ref()
                        .map_or("unknown", |table| table.as_str())
                ),
            ));
        }
        _ => {}
    }
    report
}

fn selector_matches(lifecycle: &ResourceLifecycleV1, selector: &str) -> bool {
    lifecycle.expected_path.qualified() == selector
        || matches!(&lifecycle.entity, EntityId::Intent(id) if id.name.as_str().eq_ignore_ascii_case(selector))
}

fn legacy_entity(
    applied: &[jails_protocol::record::AppliedEntity],
    selector: &str,
) -> Option<EntityId> {
    let matches = applied
        .iter()
        .filter_map(|row| match &row.id {
            EntityId::Intent(id) if id.name.as_str().eq_ignore_ascii_case(selector) => {
                Some(row.id.clone())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    (matches.len() == 1).then(|| matches[0].clone())
}

fn finding(code: &str, message: impl Into<String>) -> ResourceFindingV1 {
    ResourceFindingV1 {
        code: code.to_string(),
        message: message.into(),
    }
}

fn syntax(path: &[&str], selector: &str, option: Option<(&str, &str)>) -> CanonicalRequestSyntaxV1 {
    CanonicalRequestSyntaxV1 {
        command_path: path.iter().map(|part| (*part).to_string()).collect(),
        positionals: vec![selector.to_string()],
        options: option
            .map(|(key, value)| BTreeMap::from([(key.to_string(), vec![value.to_string()])]))
            .unwrap_or_default(),
        flags: BTreeSet::new(),
    }
}

fn entity_label(entity: &EntityId) -> String {
    match entity {
        EntityId::Intent(IntentId {
            recipe: _,
            name,
            package,
        }) => format!("{name}@{package}"),
        other => format!("{other:?}"),
    }
}

fn command(request: &CanonicalRequestSyntaxV1) -> String {
    let mut parts = vec!["jails".to_string()];
    parts.extend(request.command_path.iter().cloned());
    parts.extend(request.positionals.iter().cloned());
    for (key, values) in &request.options {
        for value in values {
            parts.push(format!("--{key}"));
            parts.push(value.clone());
        }
    }
    parts.join(" ")
}

pub fn render_human(report: &ResourceStatusV1) -> String {
    let mut out = format!(
        "resource: {}\nstate: {}\n",
        report
            .entity
            .as_ref()
            .map(entity_label)
            .unwrap_or_else(|| "unknown".to_string()),
        report.state.label()
    );
    out.push_str(&format!(
        "declaration: {}\ngenerated: {}\nmigration-history: {}\n",
        report.declaration.label(),
        report.generated.label(),
        report.migration_history.label()
    ));
    if let Some(live) = report.live {
        out.push_str(&format!("live: {}\n", live.label()));
    }
    if let Some(table) = &report.table {
        out.push_str(&format!("table: {}\n", table.as_str()));
    }
    for finding in &report.findings {
        out.push_str(&format!("finding: {}: {}\n", finding.code, finding.message));
    }
    for request in &report.next_requests {
        out.push_str(&format!("next: {}\n", command(request)));
    }
    out
}

pub fn render_json(report: &ResourceStatusV1) -> String {
    let entity = report
        .entity
        .as_ref()
        .map(|entity| crate::json::string(&entity_label(entity)))
        .unwrap_or_else(|| "null".to_string());
    let table = report
        .table
        .as_ref()
        .map(|table| crate::json::string(table.as_str()))
        .unwrap_or_else(|| "null".to_string());
    let live = report
        .live
        .map(|status| crate::json::string(status.label()))
        .unwrap_or_else(|| "null".to_string());
    let findings = report
        .findings
        .iter()
        .map(|finding| {
            format!(
                "{{\"code\":{},\"message\":{}}}",
                crate::json::string(&finding.code),
                crate::json::string(&finding.message)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let next = report
        .next_requests
        .iter()
        .map(|request| crate::json::string(&command(request)))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"schema_version\":1,\"entity\":{entity},\"state\":{},\"declaration\":{},\"generated\":{},\"migration_history\":{},\"live\":{live},\"table\":{table},\"findings\":[{findings}],\"next_requests\":[{next}]}}",
        crate::json::string(report.state.label()),
        crate::json::string(report.declaration.label()),
        crate::json::string(report.generated.label()),
        crate::json::string(report.migration_history.label()),
    )
}
