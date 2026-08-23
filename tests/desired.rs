//! Which capabilities can already be stated as desired state, and which cannot.
//!
//! plan.md §R6.1 step 2 puts capability `add`/`remove`/`sync` on V2 while
//! default dispatch stays on V1, and the honest way to run a migration like
//! that is to measure it. `jails_prepare::desire::contribution` translates a
//! planned `model::Change` into owned resources; this walks *every* capability
//! through it against a real fixture project and pins the answer per
//! capability.
//!
//! The table below is therefore a progress board, not documentation. A
//! capability that starts translating without its row changing fails the test,
//! and so does one that stops — because a translation that silently regresses
//! is indistinguishable from one that was never written, and this is the only
//! place that would notice.

mod common;

use clap::ValueEnum;
use jails_prepare::desire;
use jails_project::model::Project;
use jails_protocol::entity::{CapabilityId, CapabilityInstance, EntityId};
use jails_protocol::resource::ResourceOwner;
use jails_spec::spec::kind::Capability;

/// Why a capability is not yet expressible as desired state.
///
/// Both reasons are contributions the closed protocol has no value for yet.
/// They are listed by name so that adding the missing `SemanticEdit` variant
/// shows up here as a row that has to move, rather than as a silent change in
/// behaviour.
#[derive(Debug, PartialEq, Eq)]
enum Verdict {
    /// Fully stated as owned resources today.
    Translates,
    /// Contributes a Spring test import (plan.md §R6.3, `add::test_wiring`).
    NeedsTestWiring,
    /// The capability refuses to plan against this fixture because the fixture
    /// does not meet its precondition — no HTTP routes to load-test, no
    /// actuator to probe. Nothing to do with the translation.
    PreconditionUnmet,
}

const BOARD: &[(&str, Verdict)] = &[
    ("actuator", Verdict::Translates),
    ("api", Verdict::Translates),
    ("cache", Verdict::Translates),
    ("ci", Verdict::Translates),
    ("cors", Verdict::Translates),
    ("coverage", Verdict::Translates),
    ("csv", Verdict::Translates),
    ("db", Verdict::NeedsTestWiring),
    ("docker", Verdict::Translates),
    ("fake", Verdict::Translates),
    ("format", Verdict::Translates),
    ("http", Verdict::Translates),
    ("json", Verdict::Translates),
    // Needs actuator before it can plan probes and burn-rate alerts.
    ("k8s", Verdict::PreconditionUnmet),
    ("kafka", Verdict::Translates),
    // Needs at least one HTTP route in the project to load-test.
    ("loadtest", Verdict::PreconditionUnmet),
    ("mail", Verdict::Translates),
    ("observability", Verdict::Translates),
    ("redis", Verdict::Translates),
    ("security", Verdict::Translates),
    ("sqlite", Verdict::Translates),
    ("sse", Verdict::Translates),
    ("testkit", Verdict::Translates),
    ("toxiproxy", Verdict::Translates),
];

fn reads() -> jails_project::capture::ReadDeclaration {
    jails_project::capture::capability_reads().unwrap()
}

fn owner(capability: Capability) -> ResourceOwner {
    ResourceOwner::Entity(EntityId::Capability(CapabilityId {
        kind: capability,
        instance: CapabilityInstance::Singleton,
    }))
}

fn verdict(capability: Capability, project: &Project) -> Verdict {
    let change = match jails_generate::add::plan_for(capability, project) {
        Ok(change) => change,
        Err(_) => {
            return Verdict::PreconditionUnmet;
        }
    };
    match desire::contribution(&owner(capability), &change, project) {
        Ok(_) => Verdict::Translates,
        Err(message) if message.contains("Spring test import") => Verdict::NeedsTestWiring,
        Err(message) => panic!(
            "{} refused for an unlisted reason: {message}",
            capability.label()
        ),
    }
}

#[test]
fn every_capability_states_where_it_is_in_the_v2_migration() {
    let root = common::temp_dir("desired-capabilities");
    std::fs::create_dir_all(&root).unwrap();
    common::write_spring_fixture(&root);
    let project = Project::load(&root).unwrap();

    let mut unlisted = Vec::new();
    let mut moved = Vec::new();
    let mut seen = Vec::new();
    for &capability in Capability::value_variants() {
        let label = capability.label();
        seen.push(label);
        let actual = verdict(capability, &project);
        match BOARD.iter().find(|(name, _)| *name == label) {
            None => unlisted.push(format!("{label} = {actual:?}")),
            Some((_, expected)) if *expected != actual => {
                moved.push(format!(
                    "{label}: board says {expected:?}, actually {actual:?}"
                ));
            }
            Some(_) => {}
        }
    }
    let stale: Vec<_> = BOARD
        .iter()
        .map(|(name, _)| *name)
        .filter(|name| !seen.contains(name))
        .collect();

    assert!(
        unlisted.is_empty(),
        "capabilities with no row on the migration board: {unlisted:#?}"
    );
    assert!(
        stale.is_empty(),
        "board rows for capabilities that no longer exist: {stale:?}"
    );
    assert!(
        moved.is_empty(),
        "the migration moved and the board did not:\n{}",
        moved.join("\n")
    );
}

/// What V1 installs and what V2 desires have to be the same project.
///
/// plan.md §R6.2 gates the capability switch on "golden parity for every
/// capability", and this is the shape that check has to take. It is
/// deliberately *not* a byte comparison of the whole file: V1 writes a
/// capability's properties inside a `# jails:<label>` marked block and V2
/// writes each key as a resource somebody owns, which is the point of the
/// change -- removing one capability's key can no longer take another's prose
/// with it. So what is compared is what the project ends up meaning: which
/// dependencies the POM declares, and what value each property has in force.
#[test]
fn what_v2_desires_is_what_v1_installs() {
    let mut compared = 0;
    for &capability in Capability::value_variants() {
        let label = capability.label();
        if !matches!(
            BOARD.iter().find(|(name, _)| *name == label),
            Some((_, Verdict::Translates))
        ) {
            continue;
        }

        let v1 = common::temp_dir(&format!("parity-v1-{label}"));
        std::fs::create_dir_all(&v1).unwrap();
        common::write_spring_fixture(&v1);
        let installed = Project::load(&v1).unwrap();
        if jails_generate::add::add_in(&installed, capability, None, false, None, false, true)
            .is_err()
        {
            // A capability that cannot install against the bare fixture has
            // nothing to compare; the board already records why.
            continue;
        }

        let v2 = common::temp_dir(&format!("parity-v2-{label}"));
        std::fs::create_dir_all(&v2).unwrap();
        common::write_spring_fixture(&v2);
        let planned = Project::load(&v2).unwrap();
        let change = jails_generate::add::plan_for(capability, &planned).unwrap();
        let desired = desire::contribution(&owner(capability), &change, &planned).unwrap();
        // Every file the capability writes is declared too: a projection can
        // only overlay a path its snapshot captured.
        let mut declaration = reads();
        for artifact in &change.files {
            let relative = artifact.path.strip_prefix(&v2).unwrap();
            declaration = declaration.file(
                jails_protocol::identity::ProjectPath::parse(relative.to_str().unwrap()).unwrap(),
            );
        }
        let (_snapshot, mut projection) =
            jails_project::capture::projected(&planned, &declaration).unwrap();
        projection.advance(&desired).unwrap();

        let after = |path: &str| -> String {
            let key = jails_protocol::identity::ProjectPath::parse(path).unwrap();
            match projection.entry(&key) {
                Some(jails_project::projection::ProjectedEntry::File(file)) => {
                    String::from_utf8_lossy(&file.bytes).into_owned()
                }
                _ => std::fs::read_to_string(v2.join(path)).unwrap_or_default(),
            }
        };

        let v1_pom = std::fs::read_to_string(v1.join("pom.xml")).unwrap();
        let v2_pom = after("pom.xml");
        for dependency in &change.deps {
            let coordinate = format!("<artifactId>{}</artifactId>", dependency.artifact_id);
            assert!(
                v1_pom.contains(&coordinate),
                "{label}: V1 did not install {}",
                dependency.artifact_id
            );
            assert!(
                v2_pom.contains(&coordinate),
                "{label}: V2 desires {} and the projected POM does not declare it",
                dependency.artifact_id
            );
        }

        let v1_properties =
            std::fs::read_to_string(v1.join(desire::APPLICATION_PROPERTIES)).unwrap_or_default();
        let v2_properties = after(desire::APPLICATION_PROPERTIES);
        for line in &change.properties {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let (key, value) = (key.trim(), value.trim());
            assert_eq!(
                jails_project::properties::get(&v1_properties, key).as_deref(),
                Some(value),
                "{label}: V1 did not set {key}"
            );
            assert_eq!(
                jails_project::properties::get(&v2_properties, key).as_deref(),
                Some(value),
                "{label}: {key} is not in force in the projected properties"
            );
        }
        for artifact in &change.files {
            let relative = artifact.path.strip_prefix(&v2).unwrap();
            let v1_bytes = std::fs::read_to_string(v1.join(relative)).unwrap_or_else(|error| {
                panic!("{label}: V1 did not write {}: {error}", relative.display())
            });
            assert_eq!(
                after(relative.to_str().unwrap()),
                v1_bytes,
                "{label}: {} differs between the two paths",
                relative.display()
            );
        }
        compared += 1;
    }
    assert!(
        compared >= 10,
        "only {compared} capabilities were actually compared; the check is not covering the surface"
    );
    println!("capabilities compared V1 against V2: {compared}");
}

/// The board is a measurement, so it has to say how far along it is.
#[test]
fn the_board_reports_how_much_of_the_capability_surface_translates() {
    let translating = BOARD
        .iter()
        .filter(|(_, verdict)| *verdict == Verdict::Translates)
        .count();
    println!(
        "capabilities stateable as desired resources: {translating}/{}",
        BOARD.len()
    );
    assert!(
        translating * 2 > BOARD.len(),
        "most of the capability surface should translate before the dispatch flips"
    );
}
