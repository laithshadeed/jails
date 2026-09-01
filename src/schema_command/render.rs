//! How a schema snapshot or a schema diff is printed.
//!
//! Split from the commands above by audience rather than by subject: the
//! functions here decide nothing and read nothing, they only turn a value into
//! the two shapes a reader consumes it in -- a table for a person and JSON for
//! a program -- and the pair has to stay in step.

use super::*;

pub(super) fn print_snapshot(
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

pub(super) fn print_diff_human(
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

pub(super) fn print_diff_json(
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

pub(super) fn collect_row_evidence(
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

pub(super) fn risk_label(risk: MigrationRisk) -> &'static str {
    match risk {
        MigrationRisk::Additive => "additive",
        MigrationRisk::DataDependent => "data-dependent",
        MigrationRisk::ConstraintLoss => "constraint-loss",
        MigrationRisk::Destructive => "destructive",
        MigrationRisk::DeploymentIncompatible => "deployment-incompatible",
        MigrationRisk::Opaque => "opaque",
    }
}
