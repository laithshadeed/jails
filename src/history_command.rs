//! Read-only projections of authenticated durable receipts.

use crate::{Output, model};
use jails_prepare::operation::OperationSemanticsV1;
use jails_prepare::prepare::{FileOp, PreparedKind};
use jails_prepare::review::{
    FileReview, PreparedReview, Reconciliation, ReviewFileKind, ReviewSelection,
};
use jails_protocol::identity::TransactionId;
use jails_protocol::resource::ResourceOwner;
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
                    "{} generation={} operation={} reason={} files={} risk={} external={} undo={}{}",
                    receipt.transaction,
                    receipt.generation,
                    receipt.prepared.operation_id,
                    receipt_reason(&receipt),
                    receipt.prepared.operations.len(),
                    receipt_risks(&receipt).join(","),
                    external_effect_classification(&receipt),
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
            println!("reason: {}", receipt_reason(&receipt));
            println!("risk: {}", receipt_risks(&receipt).join(","));
            println!(
                "external-effect: {}",
                external_effect_classification(&receipt)
            );
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
                let evidence = operation_evidence(operation);
                println!(
                    "  {:7} {} before={} after={} mode={} owners={}",
                    operation_kind(operation),
                    operation.target(),
                    evidence.before.as_deref().unwrap_or("absent"),
                    evidence.after.as_deref().unwrap_or("absent"),
                    evidence.mode,
                    evidence
                        .contributors
                        .iter()
                        .map(owner_label)
                        .collect::<Vec<_>>()
                        .join(",")
                );
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
                println!(
                    "toolchain-records: {}",
                    receipt.prepared.preparation.tools.len()
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
                    "{{\"snapshot\":{},\"proposed_generation\":{},\"preconditions\":{},\"toolchain_records\":{},\"kind\":{},\"semantics\":{}}}",
                    jails_support::json::string(
                        &receipt.prepared.operation_identity.snapshot.to_hex()
                    ),
                    receipt.prepared.operation_identity.proposed_generation,
                    receipt.prepared.input_preconditions.len(),
                    receipt.prepared.preparation.tools.len(),
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
    if matches!(
        &receipt.prepared.operation_identity.semantics,
        OperationSemanticsV1::Apply(apply)
            if matches!(
                &apply.subject,
                jails_protocol::plan::PlannedSubject::RenameResource(request)
                    if request.strategy == jails_protocol::request::RenameStrategy::Rolling
            )
    ) {
        return (false, Some("contains-rename-campaign".to_string()));
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
            let evidence = operation_evidence(operation);
            let owners = evidence
                .contributors
                .iter()
                .map(|owner| jails_support::json::string(&owner_label(owner)))
                .collect::<Vec<_>>()
                .join(",");
            format!(
                "{{\"path\":{},\"kind\":{},\"before\":{},\"after\":{},\"mode\":{},\"owners\":[{owners}]}}",
                jails_support::json::string(operation.target().as_str()),
                jails_support::json::string(operation_kind(operation)),
                evidence
                    .before
                    .as_deref()
                    .map(jails_support::json::string)
                    .unwrap_or_else(|| "null".to_string()),
                evidence
                    .after
                    .as_deref()
                    .map(jails_support::json::string)
                    .unwrap_or_else(|| "null".to_string()),
                evidence.mode,
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let risks = receipt_risks(receipt)
        .iter()
        .map(|risk| jails_support::json::string(risk))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"transaction_id\":{},\"operation_id\":{},\"generation\":{},\"reason\":{},\"risk\":[{risks}],\"external_effect\":{},\"evidence\":{{\"snapshot\":{},\"preconditions\":{},\"toolchain_records\":{}}},\"files\":[{files}],\"effects\":{},\"undo_eligible\":{},\"undo_reason\":{}}}",
        jails_support::json::string(&receipt.transaction.to_hex()),
        jails_support::json::string(&receipt.prepared.operation_id.to_hex()),
        receipt.generation,
        jails_support::json::string(receipt_reason(receipt)),
        jails_support::json::string(external_effect_classification(receipt)),
        jails_support::json::string(&receipt.prepared.operation_identity.snapshot.to_hex()),
        receipt.prepared.input_preconditions.len(),
        receipt.prepared.preparation.tools.len(),
        receipt.post_commit.len(),
        eligible,
        reason
            .as_deref()
            .map(jails_support::json::string)
            .unwrap_or_else(|| "null".to_string())
    )
}

struct OperationEvidence<'a> {
    before: Option<String>,
    after: Option<String>,
    mode: u32,
    contributors: &'a std::collections::BTreeSet<ResourceOwner>,
}

fn operation_evidence(operation: &FileOp) -> OperationEvidence<'_> {
    match operation {
        FileOp::Create {
            after,
            mode,
            contributors,
            ..
        } => OperationEvidence {
            before: None,
            after: Some(after.id.to_hex()),
            mode: mode.bits(),
            contributors,
        },
        FileOp::Replace {
            before,
            after,
            mode,
            contributors,
            ..
        } => OperationEvidence {
            before: Some(before.object.id.to_hex()),
            after: Some(after.id.to_hex()),
            mode: mode.bits(),
            contributors,
        },
        FileOp::Delete {
            before,
            contributors,
            ..
        } => OperationEvidence {
            before: Some(before.object.id.to_hex()),
            after: None,
            mode: before.mode.bits(),
            contributors,
        },
    }
}

fn owner_label(owner: &ResourceOwner) -> String {
    match owner {
        ResourceOwner::Entity(id) => format!("entity:{id:?}"),
        ResourceOwner::OneShot(id) => format!("one-shot:{id:?}"),
        ResourceOwner::SchemaHistory => "schema-history".to_string(),
        ResourceOwner::Query(id) => format!("query:{id:?}"),
        ResourceOwner::ProjectArchitecture => "project-architecture".to_string(),
    }
}

fn receipt_reason(receipt: &jails_commit::journal::ReceiptV1) -> &'static str {
    let OperationSemanticsV1::Apply(apply) = &receipt.prepared.operation_identity.semantics else {
        return semantics_kind(&receipt.prepared.operation_identity.semantics);
    };
    use jails_protocol::plan::PlannedSubject;
    match &apply.subject {
        PlannedSubject::Reconcile(_) => "reconcile",
        PlannedSubject::ApplyOneShot { .. } => "apply-one-shot",
        PlannedSubject::DestroyCases { .. } => "destroy-cases",
        PlannedSubject::AppInit { .. } => "app-init",
        PlannedSubject::Rename { .. } => "rename-type",
        PlannedSubject::RenameResource(_) => "rename-resource",
        PlannedSubject::CompleteStorageRename(_) => "complete-storage-rename",
        PlannedSubject::AdoptLayout => "adopt-layout",
        PlannedSubject::Format { .. } => "format",
        PlannedSubject::EvolveField(_) => "evolve-field",
        PlannedSubject::DestroyResourceV2(_) => "destroy-resource",
        PlannedSubject::ReviveResource(_) => "revive-resource",
        PlannedSubject::RepairResource(_) => "repair-resource",
        PlannedSubject::GenerateQueries { .. } => "generate-queries",
        PlannedSubject::ContractProjection { .. } => "contract-projection",
        PlannedSubject::UndoFiles(_) => "undo-files",
    }
}

fn receipt_risks(receipt: &jails_commit::journal::ReceiptV1) -> Vec<&'static str> {
    let mut risks = std::collections::BTreeSet::new();
    if receipt
        .prepared
        .operations
        .iter()
        .any(|operation| matches!(operation, FileOp::Delete { .. }))
    {
        risks.insert("destructive");
    }
    if receipt.prepared.operations.iter().any(|operation| {
        jails_protocol::resource::ResourceKey::WholeFile(operation.target().clone())
            .is_migration_history()
    }) {
        risks.insert("deployment-incompatible");
    }
    if !receipt.prepared.post_commit.is_empty() {
        risks.insert("external-effect");
    }
    if risks.is_empty() {
        risks.insert("ordinary");
    }
    risks.into_iter().collect()
}

fn external_effect_classification(receipt: &jails_commit::journal::ReceiptV1) -> &'static str {
    if receipt.prepared.post_commit.is_empty() {
        return "none";
    }
    if receipt
        .post_commit
        .iter()
        .all(|effect| matches!(effect.state, jails_protocol::effect::EffectState::Succeeded))
    {
        "resolved"
    } else {
        "unresolved"
    }
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
