//! Read-only projections of authenticated durable receipts.

use crate::{Output, model};
use jails_prepare::operation::OperationSemanticsV1;
use jails_prepare::prepare::{FileOp, PreparedKind};
use jails_prepare::review::{
    FileReview, PreparedReview, Reconciliation, ReviewFileKind, ReviewSelection,
};
use jails_protocol::identity::TransactionId;
use jails_support::Result;

pub(crate) fn history(limit: usize, output: Output) -> Result<()> {
    let project = model::Project::discover()?;
    let store = jails_commit::store::Store::at(project.root());
    let receipts = store.read_receipts()?;
    let receipts = receipts.into_iter().take(limit).collect::<Vec<_>>();
    match output {
        Output::Human => {
            if receipts.is_empty() {
                println!("no committed transactions");
                return Ok(());
            }
            for receipt in receipts {
                let (eligible, reason) = undo_eligibility(&receipt);
                println!(
                    "{} generation={} operation={} files={} effects={} undo={}{}",
                    receipt.transaction,
                    receipt.generation,
                    receipt.prepared.operation_id,
                    receipt.prepared.operations.len(),
                    receipt.post_commit.len(),
                    if eligible { "eligible" } else { "refused" },
                    reason
                        .as_deref()
                        .map(|reason| format!(" reason={reason}"))
                        .unwrap_or_default()
                );
            }
        }
        Output::Json => {
            let rows = receipts
                .iter()
                .map(receipt_json)
                .collect::<Vec<_>>()
                .join(",");
            println!("{{\"schema_version\":1,\"receipts\":[{rows}]}}");
        }
    }
    Ok(())
}

pub(crate) fn show(transaction: &str, diff: bool, why: bool, output: Output) -> Result<()> {
    let id = TransactionId::parse_hex(transaction)?;
    let project = model::Project::discover()?;
    let store = jails_commit::store::Store::at(project.root());
    let receipt = store.read_receipt(&id)?;
    let (eligible, reason) = undo_eligibility(&receipt);
    let review = diff.then(|| receipt_review(&store, &receipt)).transpose()?;
    match output {
        Output::Human => {
            println!("transaction: {}", receipt.transaction);
            println!("operation: {}", receipt.prepared.operation_id);
            println!("generation: {}", receipt.generation);
            println!(
                "undo: {}{}",
                if eligible { "eligible" } else { "refused" },
                reason
                    .as_deref()
                    .map(|reason| format!(" ({reason})"))
                    .unwrap_or_default()
            );
            println!("files:");
            for operation in &receipt.prepared.operations {
                println!("  {:7} {}", operation_kind(operation), operation.target());
            }
            println!("effects: {}", receipt.post_commit.len());
            if why {
                println!("snapshot: {}", receipt.prepared.operation_identity.snapshot);
                println!(
                    "proposed-generation: {}",
                    receipt.prepared.operation_identity.proposed_generation
                );
                println!(
                    "preconditions: {}",
                    receipt.prepared.input_preconditions.len()
                );
                println!("kind: {}", prepared_kind(&receipt.prepared.kind));
                println!(
                    "semantics: {}",
                    semantics_kind(&receipt.prepared.operation_identity.semantics)
                );
            }
            if let Some(review) = review {
                print!(
                    "{}",
                    jails_prepare::review::render_human(
                        &review,
                        ReviewSelection {
                            diff: true,
                            ast: false,
                        },
                    )
                );
            }
        }
        Output::Json => {
            let why_json = if why {
                format!(
                    "{{\"snapshot\":{},\"proposed_generation\":{},\"preconditions\":{},\"kind\":{},\"semantics\":{}}}",
                    jails_support::json::string(
                        &receipt.prepared.operation_identity.snapshot.to_hex()
                    ),
                    receipt.prepared.operation_identity.proposed_generation,
                    receipt.prepared.input_preconditions.len(),
                    jails_support::json::string(prepared_kind(&receipt.prepared.kind)),
                    jails_support::json::string(semantics_kind(
                        &receipt.prepared.operation_identity.semantics
                    ))
                )
            } else {
                "null".to_string()
            };
            let diff_json = review
                .as_ref()
                .map(|review| {
                    jails_support::json::string(&jails_prepare::review::render_human(
                        review,
                        ReviewSelection {
                            diff: true,
                            ast: false,
                        },
                    ))
                })
                .unwrap_or_else(|| "null".to_string());
            println!(
                "{{\"schema_version\":1,\"receipt\":{},\"why\":{why_json},\"diff\":{diff_json}}}",
                receipt_json(&receipt)
            );
        }
    }
    Ok(())
}

fn receipt_review(
    store: &jails_commit::store::Store,
    receipt: &jails_commit::journal::ReceiptV1,
) -> Result<PreparedReview> {
    let mut files = Vec::new();
    for operation in &receipt.prepared.operations {
        let (kind, before, after) = match operation {
            FileOp::Create { after, .. } => (
                ReviewFileKind::Create,
                None,
                Some(store.read_object(after)?.into()),
            ),
            FileOp::Replace { before, after, .. } => (
                ReviewFileKind::Replace,
                Some(store.read_object(&before.object)?.into()),
                Some(store.read_object(after)?.into()),
            ),
            FileOp::Delete { before, .. } => (
                ReviewFileKind::Delete,
                Some(store.read_object(&before.object)?.into()),
                None,
            ),
        };
        files.push(FileReview {
            path: operation.target().clone(),
            kind,
            reconciliation: Reconciliation::Direct,
            before,
            after,
        });
    }
    Ok(PreparedReview {
        files,
        edits: Vec::new(),
    })
}

fn undo_eligibility(receipt: &jails_commit::journal::ReceiptV1) -> (bool, Option<String>) {
    if let Some(path) = receipt
        .prepared
        .operations
        .iter()
        .map(FileOp::target)
        .find(|path| {
            jails_protocol::resource::ResourceKey::WholeFile((*path).clone()).is_migration_history()
        })
    {
        return (false, Some(format!("contains-migration:{path}")));
    }
    if !receipt.post_commit.is_empty() || !receipt.prepared.post_commit.is_empty() {
        return (false, Some("contains-external-effect".to_string()));
    }
    if !matches!(receipt.prepared.kind, PreparedKind::Apply) {
        return (false, Some("not-an-ordinary-apply".to_string()));
    }
    (true, None)
}

fn receipt_json(receipt: &jails_commit::journal::ReceiptV1) -> String {
    let (eligible, reason) = undo_eligibility(receipt);
    let files = receipt
        .prepared
        .operations
        .iter()
        .map(|operation| {
            format!(
                "{{\"path\":{},\"kind\":{}}}",
                jails_support::json::string(operation.target().as_str()),
                jails_support::json::string(operation_kind(operation))
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"transaction_id\":{},\"operation_id\":{},\"generation\":{},\"files\":[{files}],\"effects\":{},\"undo_eligible\":{},\"undo_reason\":{}}}",
        jails_support::json::string(&receipt.transaction.to_hex()),
        jails_support::json::string(&receipt.prepared.operation_id.to_hex()),
        receipt.generation,
        receipt.post_commit.len(),
        eligible,
        reason
            .as_deref()
            .map(jails_support::json::string)
            .unwrap_or_else(|| "null".to_string())
    )
}

fn operation_kind(operation: &FileOp) -> &'static str {
    match operation {
        FileOp::Create { .. } => "create",
        FileOp::Replace { .. } => "replace",
        FileOp::Delete { .. } => "delete",
    }
}

fn prepared_kind(kind: &PreparedKind) -> &'static str {
    match kind {
        PreparedKind::Apply => "apply",
        PreparedKind::Conflict { .. } => "conflict",
        PreparedKind::Finalise { .. } => "finalise",
        PreparedKind::Abort { .. } => "abort",
    }
}

fn semantics_kind(semantics: &OperationSemanticsV1) -> &'static str {
    match semantics {
        OperationSemanticsV1::Apply(_) => "apply",
        OperationSemanticsV1::Finalise { .. } => "finalise",
        OperationSemanticsV1::Abort { .. } => "abort",
    }
}
