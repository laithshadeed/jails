//! Read-only CLI projections for schema authorities and reconciliation plans.

use crate::cli::{
    IntrospectCommand, IntrospectFormatArg, RunServicesArg, SchemaAuthorityArg, SchemaCommand,
};
use jails_project::model::Project;
use jails_protocol::database::{
    MigrationRisk, PlannedSchemaOp, SchemaObject, SchemaObjectId, SchemaOp, SchemaProvenance,
    SchemaSnapshot,
};
use jails_support::Result;
use std::collections::BTreeSet;
use std::path::Path;

pub(crate) fn resource_status(
    selector: &str,
    datasource: Option<&str>,
    invocation: crate::Invocation,
) -> Result<()> {
    // A canonical project has no ledger, and the legacy report reads one: it
    // answered `state: ambiguous` about an entity the model describes
    // completely. `model_status` reads the same four authorities where they
    // actually live.
    if crate::model_command::owns() {
        if let Some(datasource) = datasource {
            let _ = datasource;
            return Err(jails_support::Failure::Told(
                "`resource status --datasource` is not yet available for a canonical project.\n       fix: run `jails resource status <SELECTOR>` for the declared, generated and migration authorities"
                    .to_string(),
            ));
        }
        return crate::model_status::run(selector, invocation);
    }
    let Some(datasource) = datasource else {
        return jails_report::lifecycle_status::status(selector, None, invocation.output.is_json());
    };
    let project = Project::discover()?;
    let history = jails_drive::live_sql::observe_flyway(
        &project,
        datasource,
        jails_drive::live_sql::LiveServices::Existing,
        invocation.debug,
    )?;
    let catalog = jails_drive::live_sql::observe(
        &project,
        datasource,
        jails_drive::live_sql::LiveServices::Existing,
        "public",
        invocation.debug,
    )?;
    let report =
        jails_report::lifecycle_status::inspect_live(&project, selector, &history, &catalog);
    if invocation.output.is_json() {
        println!("{}", jails_report::lifecycle_status::render_json(&report));
    } else {
        print!("{}", jails_report::lifecycle_status::render_human(&report));
    }
    Ok(())
}

pub(crate) fn introspect(command: IntrospectCommand, invocation: crate::Invocation) -> Result<()> {
    match command {
        IntrospectCommand::Db {
            datasource,
            schema,
            table,
            format,
            services,
        } => {
            let project = Project::discover()?;
            let snapshot = filtered(
                jails_drive::live_sql::observe(
                    &project,
                    &datasource,
                    live_services(services),
                    &schema,
                    invocation.debug,
                )?,
                table.as_deref(),
            )?;
            let format = if invocation.output.is_json() {
                IntrospectFormatArg::Json
            } else {
                format
            };
            print_snapshot(&snapshot, format, None);
            Ok(())
        }
    }
}

pub(crate) fn pull(
    datasource: &str,
    schema: &str,
    table: Option<&str>,
    into_slice: Option<&str>,
    services: RunServicesArg,
    invocation: crate::Invocation,
) -> Result<()> {
    let project = Project::discover()?;
    let snapshot = filtered(
        jails_drive::live_sql::observe(
            &project,
            datasource,
            live_services(services),
            schema,
            invocation.debug,
        )?,
        table,
    )?;
    print_snapshot(
        &snapshot,
        if invocation.output.is_json() {
            IntrospectFormatArg::Json
        } else {
            IntrospectFormatArg::Manifest
        },
        into_slice,
    );
    Ok(())
}

pub(crate) fn schema(command: SchemaCommand, invocation: crate::Invocation) -> Result<()> {
    match command {
        SchemaCommand::Diff {
            from,
            to,
            datasource,
            schema,
            services,
            manifest,
        } => {
            let project = Project::discover()?;
            let from_snapshot = authority(
                &project,
                from,
                datasource.as_deref(),
                &schema,
                services,
                manifest.as_deref(),
                invocation.debug,
            )?;
            let to_snapshot = authority(
                &project,
                to,
                datasource.as_deref(),
                &schema,
                services,
                manifest.as_deref(),
                invocation.debug,
            )?;
            let operations = jails_project::schema::diff(&from_snapshot, &to_snapshot)?;
            let row_evidence = if (from == SchemaAuthorityArg::Live
                || to == SchemaAuthorityArg::Live)
                && datasource.is_some()
            {
                collect_row_evidence(
                    &project,
                    datasource.as_deref().expect("checked above"),
                    services,
                    &operations,
                    invocation.debug,
                )
            } else {
                (Vec::new(), None)
            };
            if invocation.output.is_json() {
                print_diff_json(
                    from,
                    to,
                    &operations,
                    &row_evidence.0,
                    row_evidence.1.as_deref(),
                );
            } else {
                print_diff_human(
                    from,
                    to,
                    &operations,
                    &row_evidence.0,
                    row_evidence.1.as_deref(),
                );
            }
            Ok(())
        }
    }
}

pub(crate) fn migrate_lint(manifest: Option<&Path>, invocation: crate::Invocation) -> Result<()> {
    let project = Project::discover()?;
    let findings = jails_project::query_workspace::migration_lint(&project, manifest)?;
    if invocation.output.is_json() {
        let rows = findings
            .iter()
            .map(|finding| {
                format!(
                    "{{\"path\":{},\"statement\":{},\"risks\":[{}],\"summary\":{}}}",
                    jails_support::json::string(finding.path.as_str()),
                    finding.statement,
                    finding
                        .risks
                        .iter()
                        .map(|risk| jails_support::json::string(risk_label(*risk)))
                        .collect::<Vec<_>>()
                        .join(","),
                    jails_support::json::string(&finding.summary)
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        println!("{{\"schema\":\"jails.migration-lint.v1\",\"findings\":[{rows}]}}");
    } else if findings.is_empty() {
        println!("migration lint: no destructive or deployment-sensitive statements");
    } else {
        for finding in findings {
            println!(
                "{} statement {}  [{}]\n  {}",
                finding.path,
                finding.statement,
                finding
                    .risks
                    .iter()
                    .map(|risk| risk_label(*risk))
                    .collect::<Vec<_>>()
                    .join(", "),
                finding.summary
            );
        }
    }
    Ok(())
}

fn authority(
    project: &Project,
    authority: SchemaAuthorityArg,
    datasource: Option<&str>,
    schema: &str,
    services: RunServicesArg,
    manifest: Option<&Path>,
    debug: bool,
) -> Result<SchemaSnapshot> {
    match authority {
        SchemaAuthorityArg::Declared => {
            jails_project::query_workspace::declared_schema(project, manifest)
        }
        SchemaAuthorityArg::Migrations => {
            jails_project::query_workspace::migration_schema(project, manifest)
        }
        SchemaAuthorityArg::Live => {
            let datasource = datasource.ok_or_else(|| {
                "live schema authority requires an explicit datasource.\n       fix: pass `--datasource postgres`."
                    .to_string()
            })?;
            jails_drive::live_sql::observe(
                project,
                datasource,
                live_services(services),
                schema,
                debug,
            )
        }
    }
}

fn live_services(services: RunServicesArg) -> jails_drive::live_sql::LiveServices {
    match services {
        RunServicesArg::Existing => jails_drive::live_sql::LiveServices::Existing,
        RunServicesArg::Start => jails_drive::live_sql::LiveServices::Start,
        RunServicesArg::None => jails_drive::live_sql::LiveServices::None,
    }
}

fn filtered(mut snapshot: SchemaSnapshot, table: Option<&str>) -> Result<SchemaSnapshot> {
    let Some(pattern) = table else {
        return Ok(snapshot);
    };
    validate_glob(pattern)?;
    snapshot.catalog.objects.retain(|id, _| {
        matches!(
            id.kind,
            jails_protocol::database::SchemaObjectKind::Schema
                | jails_protocol::database::SchemaObjectKind::Enum
                | jails_protocol::database::SchemaObjectKind::Domain
        ) || id
            .parent
            .as_ref()
            .is_some_and(|parent| glob(pattern, parent.name.as_str()))
            || glob(pattern, id.name.as_str())
    });
    snapshot.catalog = jails_protocol::database::CatalogSnapshot::new(
        snapshot.catalog.dialect,
        snapshot.catalog.objects,
        snapshot.catalog.opaque,
    )?;
    Ok(snapshot)
}

fn validate_glob(pattern: &str) -> Result<()> {
    if pattern.is_empty()
        || !pattern.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_' || byte == b'*'
        })
    {
        return Err(format!(
            "`{pattern}` is not a lowercase table glob.\n       fix: use a pattern such as `orders*`."
        )
        .into());
    }
    Ok(())
}

fn glob(pattern: &str, value: &str) -> bool {
    let parts = pattern.split('*').collect::<Vec<_>>();
    if parts.len() == 1 {
        return pattern == value;
    }
    let mut rest = value;
    for (index, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        let Some(at) = rest.find(part) else {
            return false;
        };
        if index == 0 && !pattern.starts_with('*') && at != 0 {
            return false;
        }
        rest = &rest[at + part.len()..];
    }
    pattern.ends_with('*') || rest.is_empty()
}

fn print_snapshot(
    snapshot: &SchemaSnapshot,
    format: IntrospectFormatArg,
    into_slice: Option<&str>,
) {
    match format {
        IntrospectFormatArg::Human => {
            println!(
                "catalog {} · {} object(s) · digest {} · {}",
                snapshot.catalog.dialect.label(),
                snapshot.catalog.objects.len(),
                snapshot.catalog.digest,
                provenance(&snapshot.provenance)
            );
            for (id, object) in &snapshot.catalog.objects {
                println!("{}  {}", operation_name(id), object_summary(object));
            }
        }
        IntrospectFormatArg::Json => print_snapshot_json(snapshot),
        IntrospectFormatArg::Manifest => {
            println!("schema = \"jails.schema-import.v1\"");
            println!("catalog_digest = \"{}\"", snapshot.catalog.digest);
            if let Some(slice) = into_slice {
                println!("into_slice = {}", jails_support::json::string(slice));
            }
            for (id, object) in &snapshot.catalog.objects {
                println!("\n[[objects]]");
                println!("kind = \"{}\"", kind_label(id.kind));
                println!(
                    "name = {}",
                    jails_support::json::string(&operation_name(id))
                );
                println!(
                    "definition = {}",
                    jails_support::json::string(&object_summary(object))
                );
            }
        }
    }
}

fn print_snapshot_json(snapshot: &SchemaSnapshot) {
    let objects = snapshot
        .catalog
        .objects
        .iter()
        .map(|(id, object)| {
            format!(
                "{{\"kind\":{},\"name\":{},\"definition\":{}}}",
                jails_support::json::string(kind_label(id.kind)),
                jails_support::json::string(&operation_name(id)),
                jails_support::json::string(&object_summary(object))
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    println!(
        "{{\"schema\":\"jails.schema-snapshot.v1\",\"dialect\":{},\"digest\":{},\"provenance\":{},\"objects\":[{}]}}",
        jails_support::json::string(snapshot.catalog.dialect.label()),
        jails_support::json::string(&snapshot.catalog.digest.to_string()),
        jails_support::json::string(&provenance(&snapshot.provenance)),
        objects
    );
}

fn print_diff_human(
    from: SchemaAuthorityArg,
    to: SchemaAuthorityArg,
    operations: &[PlannedSchemaOp],
    row_evidence: &[jails_drive::live_sql::RowEvidence],
    evidence_error: Option<&str>,
) {
    println!(
        "schema diff {from:?} -> {to:?}: {} operation(s)",
        operations.len()
    );
    for operation in operations {
        println!(
            "{}  [{}]",
            operation_summary(&operation.operation),
            operation
                .risks
                .iter()
                .map(|risk| risk_label(*risk))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    for evidence in row_evidence {
        println!(
            "evidence  {}.{} rows={} [live, read-only]",
            evidence.schema, evidence.table, evidence.rows
        );
    }
    if let Some(error) = evidence_error {
        println!("evidence  unavailable [live, read-only]: {error}");
    }
}

fn print_diff_json(
    from: SchemaAuthorityArg,
    to: SchemaAuthorityArg,
    operations: &[PlannedSchemaOp],
    row_evidence: &[jails_drive::live_sql::RowEvidence],
    evidence_error: Option<&str>,
) {
    let rows = operations
        .iter()
        .map(|operation| {
            format!(
                "{{\"operation\":{},\"risks\":[{}]}}",
                jails_support::json::string(&operation_summary(&operation.operation)),
                operation
                    .risks
                    .iter()
                    .map(|risk| jails_support::json::string(risk_label(*risk)))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let evidence = row_evidence
        .iter()
        .map(|row| {
            format!(
                "{{\"schema\":{},\"table\":{},\"rows\":{},\"evidence\":\"live\"}}",
                jails_support::json::string(&row.schema),
                jails_support::json::string(&row.table),
                row.rows
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    println!(
        "{{\"schema\":\"jails.schema-diff.v1\",\"from\":{},\"to\":{},\"operations\":[{}],\"row_evidence\":[{}],\"evidence_error\":{}}}",
        jails_support::json::string(&format!("{from:?}").to_ascii_lowercase()),
        jails_support::json::string(&format!("{to:?}").to_ascii_lowercase()),
        rows,
        evidence,
        evidence_error
            .map(jails_support::json::string)
            .unwrap_or_else(|| "null".into())
    );
}

fn collect_row_evidence(
    project: &Project,
    datasource: &str,
    services: RunServicesArg,
    operations: &[PlannedSchemaOp],
    debug: bool,
) -> (Vec<jails_drive::live_sql::RowEvidence>, Option<String>) {
    let tables = operations
        .iter()
        .filter(|operation| {
            operation.risks.contains(&MigrationRisk::Destructive)
                || operation.risks.contains(&MigrationRisk::DataDependent)
        })
        .filter_map(|operation| affected_table(&operation.operation))
        .collect::<BTreeSet<_>>();
    match jails_drive::live_sql::row_counts(
        project,
        datasource,
        live_services(services),
        &tables,
        debug,
    ) {
        Ok(evidence) => (evidence, None),
        Err(error) => (Vec::new(), Some(error.to_string())),
    }
}

fn affected_table(
    operation: &SchemaOp,
) -> Option<(
    jails_protocol::identity::SqlName,
    jails_protocol::identity::SqlName,
)> {
    let id = match operation {
        SchemaOp::Create { id, .. } | SchemaOp::Alter { id, .. } | SchemaOp::Drop { id, .. } => id,
        SchemaOp::Rename { before, .. } => before,
    };
    if id.kind == jails_protocol::database::SchemaObjectKind::Table {
        return Some((id.namespace.clone(), id.name.clone()));
    }
    id.parent.as_ref().map(|parent| {
        (
            parent
                .namespace
                .clone()
                .unwrap_or_else(|| id.namespace.clone()),
            parent.name.clone(),
        )
    })
}

fn operation_summary(operation: &SchemaOp) -> String {
    match operation {
        SchemaOp::Create { id, .. } => format!("CREATE {}", operation_name(id)),
        SchemaOp::Alter { id, .. } => format!("ALTER {}", operation_name(id)),
        SchemaOp::Rename { before, after } => {
            format!(
                "RENAME {} TO {}",
                operation_name(before),
                operation_name(after)
            )
        }
        SchemaOp::Drop { id, .. } => format!("DROP {}", operation_name(id)),
    }
}

fn operation_name(id: &SchemaObjectId) -> String {
    let parent = id
        .parent
        .as_ref()
        .map(|parent| format!("{}.", parent.name.as_str()))
        .unwrap_or_default();
    format!("{}.{parent}{}", id.namespace.as_str(), id.name.as_str())
}

fn kind_label(kind: jails_protocol::database::SchemaObjectKind) -> &'static str {
    use jails_protocol::database::SchemaObjectKind as K;
    match kind {
        K::Schema => "schema",
        K::Table => "table",
        K::Column => "column",
        K::PrimaryKey => "primary-key",
        K::ForeignKey => "foreign-key",
        K::Unique => "unique",
        K::Index => "index",
        K::Check => "check",
        K::Enum => "enum",
        K::Domain => "domain",
        K::View => "view",
        K::Routine => "routine",
        K::Policy => "policy",
        K::Opaque => "opaque",
    }
}

fn object_summary(object: &SchemaObject) -> String {
    match object {
        SchemaObject::Schema => "schema".into(),
        SchemaObject::Table => "table".into(),
        SchemaObject::Column {
            sql_type,
            nullable,
            ordinal,
            default_expression,
            generated,
            identity,
            comment,
        } => format!(
            "{} {} ordinal={} default={} generated={} identity={} comment={}",
            sql_type.as_str(),
            if *nullable { "nullable" } else { "not-null" },
            ordinal,
            default_expression.as_deref().unwrap_or("-"),
            generated.as_deref().unwrap_or("-"),
            identity.as_deref().unwrap_or("-"),
            comment.as_deref().unwrap_or("-")
        ),
        SchemaObject::PrimaryKey { columns } => columns
            .iter()
            .map(|column| column.as_str())
            .collect::<Vec<_>>()
            .join(","),
        SchemaObject::ForeignKey { definition, .. }
        | SchemaObject::Unique { definition }
        | SchemaObject::Index { definition }
        | SchemaObject::Check { definition }
        | SchemaObject::Domain { definition }
        | SchemaObject::View { definition }
        | SchemaObject::Routine { definition }
        | SchemaObject::Policy { definition }
        | SchemaObject::Opaque { definition } => definition.clone(),
        SchemaObject::Enum { labels } => labels.join(","),
    }
}

fn provenance(provenance: &SchemaProvenance) -> String {
    match provenance {
        SchemaProvenance::Declared => "declared".into(),
        SchemaProvenance::Migrations { files } => format!("migrations:{}", files.len()),
        SchemaProvenance::Live {
            server_major,
            database_fingerprint,
        } => format!("live:postgres-{server_major}:{database_fingerprint}"),
    }
}

fn risk_label(risk: MigrationRisk) -> &'static str {
    match risk {
        MigrationRisk::Additive => "additive",
        MigrationRisk::DataDependent => "data-dependent",
        MigrationRisk::ConstraintLoss => "constraint-loss",
        MigrationRisk::Destructive => "destructive",
        MigrationRisk::DeploymentIncompatible => "deployment-incompatible",
        MigrationRisk::Opaque => "opaque",
    }
}
