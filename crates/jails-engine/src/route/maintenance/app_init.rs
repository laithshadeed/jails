//! `jails app init` — seed `.jails/app.toml` and hand the file to the reader.

use super::*;

/// Seed an application manifest, through the protocol.
///
/// It is one file with fixed bytes, which is exactly why routing it is worth
/// the paragraph: V1 refuses when the path exists by calling `Path::exists`
/// and then writes, and anything may happen between those two statements.
/// Here the path is a **declared read**, so the refusal is a precondition
/// §R4.3 step 2 rechecks under the lock.
pub fn app_init(run: &Run, manifest: Option<&str>) -> Result<Outcome> {
    let project = run.project();
    let skeleton = format!(
        "\
# Generic application intent. Add capabilities, then one [[generate]] table per slice.
schema = {}
capabilities = []

# [[generate]]
# kind = \"scaffold\"
# name = \"Note\"
# fields = [\"id:uuid@pk\", \"title:string!\"]
# timestamps = true
",
        jails_protocol::compatibility::APP_MANIFEST_SCHEMA
    );
    let target = ProjectPath::parse(manifest.unwrap_or(".jails/app.toml"))?;

    // Seeding is not regeneration, which is why an existing manifest is a
    // refusal rather than a three-way merge. The bytes below are a skeleton
    // nobody keeps; a manifest that exists is a document somebody has been
    // writing, and merging one into the other produces a file neither of them
    // meant.
    let reads = capture::capability_reads()?.file(target.clone());
    let (snapshot, _) = capture::projected(project, &reads)?;
    if let jails_protocol::snapshot::Captured::Present(_) = snapshot.read(&target)? {
        return Err(format!(
            "application manifest already exists: {target}.\n       fix: edit it, or pass \
             --manifest with a new path."
        )
        .into());
    }

    let manifest_target = target.clone();
    let mut change = DesiredChange::maintenance(MaintenanceAttribution::AppInit);
    change.files.push(DesiredFile {
        path: target.clone(),
        body: DesiredBody::Bytes(skeleton.into_bytes().into()),
        mode: None,
        // No resource, and that is the point. A resource is a claim that jails
        // owns these bytes and will decide about them again; this file belongs
        // to the reader the moment it lands.
        resource: None,
        renderer: None,
    });

    let observed = observed(project)?;
    let set = DesiredChangeSet {
        ledger_intent: LedgerIntent {
            // Nothing enters the store. `app init` seeds a file and stops --
            // what the manifest goes on to declare is `app apply`'s business,
            // and recording a row here would be a claim on a document jails
            // does not write again.
            generation_before: observed.generation(),
            entities_after: Vec::new(),
            one_shots_after: Vec::new(),
            resources_after: Vec::new(),
            entities_removed: Vec::new(),
        },
        ordered: vec![change],
        subject: PlannedSubject::AppInit { target },
    };
    set.validate()?;
    commit_set(
        run,
        set,
        &reads,
        &Asked::plain(
            CanonicalMutationRequest::AppInit {
                target: manifest_target,
            },
            &["app", "init"],
            &[],
        ),
    )
}
