use crate::diagnostic::{Check, Status};
use crate::model::Project;
use jails_protocol::entity::EntityId;
use jails_protocol::resource::ResourceOwner;
use jails_state::compat::MachineState;

/// Compare every recorded managed output with the live project tree.
///
/// The ledger's `current` image is the exact file state accepted by the last
/// commit, including reader edits preserved by a merge. Comparing with that
/// image detects changes made afterwards without mistaking an earlier,
/// deliberately preserved edit for drift.
pub(crate) fn managed_output_checks(project: &Project) -> Vec<Check> {
    let MachineState::Current(store) = jails_state::compat::read(project.root()) else {
        return Vec::new();
    };
    // An interrupted transaction explains every difference below, and the two
    // explanations are not interchangeable. A write that stopped part-way --
    // an unwritable directory, a full disk, an IDE lock -- leaves jails' own
    // newer bytes on disk with the ledger still at the older state, which
    // reads exactly like a developer editing five generated files at once.
    // Reporting it that way sent people to `resource repair --strategy
    // roll-forward`, which adopts the half-applied state as the recorded
    // truth: `doctor` goes green over a project whose every insert names a
    // column no migration created.
    //
    // The transaction is still on disk and the next mutating command finishes
    // it under the lock, so what this needs to say is "run it again", once,
    // instead of five accusations.
    if let Some(pending) = interrupted(project) {
        return vec![pending];
    }
    let mut checks = Vec::new();
    for output in &store.outputs {
        let Some((title, fix)) = owner_action(&output.contributors) else {
            continue;
        };
        let path = project.root().join(output.path.as_str());
        match std::fs::read(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => checks.push(
                Check::new(
                    Status::Fail,
                    title,
                    format!("recorded output `{}` is missing", output.path),
                )
                .fix(fix),
            ),
            Err(error) => checks.push(
                Check::new(
                    Status::Fail,
                    title,
                    format!("recorded output `{}` cannot be read: {error}", output.path),
                )
                .fix(fix),
            ),
            Ok(bytes)
                if jails_protocol::identity::ObjectId::from_bytes(
                    jails_support::codec::sha256(&bytes),
                ) != output.current.sha256 =>
            {
                checks.push(
                    Check::new(
                        Status::Warn,
                        title,
                        format!(
                            "recorded output `{}` changed since the last jails commit",
                            output.path
                        ),
                    )
                    .fix(fix),
                );
            }
            Ok(_) => {}
        }
    }
    checks
}

/// The one check to make when a transaction did not finish.
///
/// Read-only: it reads the journal, and never recovers anything. Recovery
/// belongs to the commands that take the project lock.
pub(crate) fn interrupted(project: &Project) -> Option<Check> {
    let pending = jails_commit::store::Store::at(project.root()).unfinished_transactions();
    let journal = pending.first()?;
    Some(
        Check::new(
            Status::Fail,
            "transaction",
            format!(
                "transaction {} started and did not finish, so some of jails' own output is \
                 newer than what jails has recorded",
                journal.transaction
            ),
        )
        .fix(
            "run the same command again -- it finishes the interrupted transaction before doing \
             anything new. Do not run `jails resource repair`: it would adopt the half-applied \
             state as the recorded truth",
        ),
    )
}

/// Every migration a resource lifecycle has sealed, against the file on disk.
///
/// A different question from the one above, and the one that had no asker. A
/// migration written by `jails resource field` is not recorded as *managed
/// output* -- it carries no renderer stamp -- so deleting
/// `V002__rename_borower_to_borrower.sql` left `doctor` reporting `25 checks,
/// all clear` while deleting the neighbouring create migration, written by
/// `g scaffold`, was correctly caught. The command's own migration was outside
/// the check that existed.
///
/// The seal is a better authority than the output row anyway: it is the exact
/// content digest of published append-only schema history, which is the thing
/// that must not move.
///
/// The two failures need different advice, and conflating them is what closed
/// a loop once. A **missing** file is restorable from its recorded object. An
/// **edited** one is a deliberate correction the reader made, and
/// `resource repair` would silently overwrite it -- so this says what changed
/// and leaves the choice with the person who made it.
pub(crate) fn migration_seal_checks(project: &Project) -> Vec<Check> {
    let MachineState::Current(store) = jails_state::compat::read(project.root()) else {
        return Vec::new();
    };
    let mut checks = Vec::new();
    for lifecycle in &store.lifecycles {
        let EntityId::Intent(id) = &lifecycle.entity else {
            continue;
        };
        let title = format!("migrations {}", id.name);
        for seal in &lifecycle.migrations {
            let path = project.root().join(seal.path.as_str());
            match std::fs::read(&path) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => checks.push(
                    Check::new(
                        Status::Fail,
                        title.clone(),
                        format!("sealed migration `{}` is missing", seal.path),
                    )
                    .fix(format!(
                        "jails resource repair {} --strategy roll-forward",
                        id.name
                    )),
                ),
                Err(error) => checks.push(
                    Check::new(
                        Status::Fail,
                        title.clone(),
                        format!("sealed migration `{}` cannot be read: {error}", seal.path),
                    )
                    .fix("make the file readable and run `jails doctor` again"),
                ),
                Ok(bytes)
                    if jails_protocol::identity::ObjectId::from_bytes(
                        jails_support::codec::sha256(&bytes),
                    ) != seal.content_digest =>
                {
                    checks.push(
                        Check::new(
                            Status::Fail,
                            title.clone(),
                            format!(
                                "sealed migration `{}` differs from the bytes jails published",
                                seal.path
                            ),
                        )
                        .fix(concat!(
                            "published schema history is append-only, so restore its exact bytes ",
                            "and append a later migration for the change you want. `jails ",
                            "resource repair --strategy roll-forward` restores them and will ",
                            "discard the edit"
                        )),
                    );
                }
                Ok(_) => {}
            }
        }
    }
    checks
}

fn owner_action(owners: &std::collections::BTreeSet<ResourceOwner>) -> Option<(String, String)> {
    for owner in owners {
        match owner {
            ResourceOwner::Entity(EntityId::Intent(id)) => {
                return Some((
                    format!("managed {}", id.name),
                    format!("jails resource repair {} --strategy roll-forward", id.name),
                ));
            }
            ResourceOwner::Entity(EntityId::Capability(id)) => {
                return Some((
                    format!("capability {}", id.kind.label()),
                    "jails sync".to_string(),
                ));
            }
            _ => {}
        }
    }
    None
}
