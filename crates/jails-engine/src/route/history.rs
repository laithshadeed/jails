//! Receipt-driven forward file restoration.

use super::*;
use jails_prepare::operation::OperationSemanticsV1;
use jails_prepare::prepare::FileOp;
use jails_protocol::conflict::FileImage;
use jails_protocol::plan::UndoFilesPlanV1;
use jails_protocol::request::UndoFilesRequestV1;

pub fn undo_files(run: &Run, transaction: &str, merge: bool) -> Result<Outcome> {
    let id = jails_protocol::identity::TransactionId::parse_hex(transaction)?;
    let project = run.project();
    let durable = jails_commit::store::Store::at(project.root());
    let receipt = durable.read_receipt(&id)?;
    let store = observed(project)?;
    if receipt.generation != store.generation() {
        return Err(format!(
            "stale-undo: transaction `{id}` is generation {}, but the project is generation {}.\n       fix: undo only the newest receipt, or prepare a new forward correction for the older change",
            receipt.generation,
            store.generation()
        )
        .into());
    }
    refuse_non_file_undo(&receipt)?;

    let request = UndoFilesRequestV1 {
        transaction: id,
        merge,
    };
    let mut reads = capture::capability_reads()?;
    for operation in &receipt.prepared.operations {
        reads = reads.file(operation.target().clone());
    }
    let (snapshot, _) = capture::projected(project, &reads)?;
    let mut changed = Vec::new();
    for operation in &receipt.prepared.operations {
        if !matches_after(&snapshot, operation)? {
            changed.push(operation.target().clone());
        }
    }
    if !changed.is_empty() {
        let paths = changed
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n         ");
        let merge_note = if merge {
            " `--merge` cannot safely merge this receipt yet because at least one inverse is a create or delete"
        } else {
            " pass `--merge` only when every changed inverse is a text replacement, or reconcile the files by hand"
        };
        return Err(format!(
            "undo-after-image-changed: these paths no longer match the receipt's after-images:\n         {paths}\n       fix:{merge_note}; no file was written"
        )
        .into());
    }

    let state_before = match receipt.prepared.state_before() {
        FileImage::Absent => None,
        FileImage::Present { object, .. } => Some(durable.read_object(object)?),
    };
    let mut change = DesiredChange::maintenance(MaintenanceAttribution::Undo);
    for operation in &receipt.prepared.operations {
        match operation {
            FileOp::Create { path, .. } => change.absences.push(ManagedPath {
                path: path.clone(),
                resource: ResourceKey::WholeFile(path.clone()),
                force: false,
            }),
            FileOp::Replace { path, before, .. } | FileOp::Delete { path, before, .. } => {
                change.files.push(DesiredFile {
                    path: path.clone(),
                    body: DesiredBody::Bytes(durable.read_object(&before.object)?.into()),
                    mode: Some(before.mode),
                    resource: None,
                    renderer: None,
                });
            }
        }
    }
    let subject = PlannedSubject::UndoFiles(Box::new(UndoFilesPlanV1 {
        request: request.clone(),
        state_before,
    }));
    let set = DesiredChangeSet::maintenance_only(store.generation(), subject, change);
    set.validate()?;
    let asked = Asked::new(
        CanonicalMutationRequest::UndoFiles(request),
        &["undo"],
        vec![transaction.to_string()],
        BTreeMap::new(),
        if merge {
            BTreeSet::from(["merge".to_string()])
        } else {
            BTreeSet::new()
        },
    );
    commit_set(run, set, &reads, &asked)
}

fn matches_after(
    snapshot: &jails_protocol::snapshot::ProjectSnapshot,
    operation: &FileOp,
) -> Result<bool> {
    let captured = snapshot.read(operation.target())?;
    Ok(match operation {
        FileOp::Create { after, mode, .. } | FileOp::Replace { after, mode, .. } => {
            matches!(
                captured,
                jails_protocol::snapshot::Captured::Present(file)
                    if file.sha256 == after.id && file.len == after.len && file.mode == *mode
            )
        }
        FileOp::Delete { .. } => {
            matches!(captured, jails_protocol::snapshot::Captured::Absent)
        }
    })
}

fn refuse_non_file_undo(receipt: &jails_commit::journal::ReceiptV1) -> Result<()> {
    if let Some(path) = receipt
        .prepared
        .operations
        .iter()
        .map(FileOp::target)
        .find(|path| ResourceKey::WholeFile((*path).clone()).is_migration_history())
    {
        return Err(format!(
            "undo-refused: transaction `{}` contains migration `{path}`.\n       fix: create a new forward corrective migration and matching code change",
            receipt.transaction
        )
        .into());
    }
    if !receipt.post_commit.is_empty() || !receipt.prepared.post_commit.is_empty() {
        return Err(format!(
            "undo-refused: transaction `{}` contains an external effect whose observation cannot be disproved.\n       fix: prepare an explicit forward correction for the project files and external system",
            receipt.transaction
        )
        .into());
    }
    if matches!(
        &receipt.prepared.operation_identity.semantics,
        OperationSemanticsV1::Apply(apply)
            if matches!(
                &apply.subject,
                PlannedSubject::RenameResource(request)
                    if request.strategy == jails_protocol::request::RenameStrategy::Rolling
            )
    ) {
        return Err(format!(
            "undo-refused: transaction `{}` started a durable rolling rename campaign.\n       fix: complete the campaign with the reported `jails rename storage` command, then make any correction forward",
            receipt.transaction
        )
        .into());
    }
    if !matches!(
        receipt.prepared.kind,
        jails_prepare::prepare::PreparedKind::Apply
    ) {
        return Err(format!(
            "undo-refused: transaction `{}` is not an ordinary apply receipt.\n       fix: inspect it with `jails show {}` and resolve its recorded lifecycle forward",
            receipt.transaction,
            receipt.transaction
        )
        .into());
    }
    Ok(())
}
