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
