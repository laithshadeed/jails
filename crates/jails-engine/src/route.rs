//! The V2 route, assembled end to end and not yet reachable from dispatch.
//!
//! plan.md §R6.1 is explicit that migration is "incremental in code and tests
//! but **atomic at the production dispatch point**": once one command writes
//! schema 2, an unswitched schema-1 writer cannot safely read or update it, so
//! there is exactly one commit where every command changes at once. Step 1
//! therefore says to land the executor "dark", and steps 2 to 6 to build each
//! command's route while default dispatch stays on V1.
//!
//! This module is where those routes are assembled. Nothing in `main.rs` calls
//! it; the tests do. That is the point — the whole path can be exercised,
//! measured against V1 and crash-tested long before anything depends on it.
//!
//! ## What one route is
//!
//! Seven steps, and each one is a value the next takes:
//!
//! 1. resolve the project, and let the recipe plan what it intends;
//! 2. state that plan as desired resources owned by somebody;
//! 3. capture the project once, and open a projection over the capture;
//! 4. declare the complete desired state for the scope this request speaks for;
//! 5. prepare — render, diff, and turn all of it into exact operations;
//! 6. take the lock;
//! 7. commit, journal-first, ledger-last.
//!
//! The interesting property is that steps 1 to 5 touch nothing. A failure
//! anywhere in them leaves a project that has not been opened for writing.

use jails_support::codec::Codec;
use std::collections::{BTreeMap, BTreeSet};

use clap::ValueEnum;
use jails_commit::execute::{self, LockedProject, ProjectHandle};
use jails_commit::outcome::{CommitError, CommitResult};
use jails_generate::generate::Recipe;
use jails_prepare::command::{CommandEnvelope, EffectRetryReport, ProjectCommitDisposition};
use jails_prepare::desire;
use jails_prepare::pipeline::{self, ObservedStore, PreparationContext};
use jails_prepare::recovery::RecoveryOutcome;
use jails_prepare::report::{Report, ReportedOp};
use jails_project::capability::Declaration;
use jails_project::capture::{self, ReadDeclaration};
use jails_project::model::{Change, Project};
use jails_protocol::bootstrap::Bootstrap;
use jails_protocol::change::{DesiredChange, MaintenanceAttribution};
use jails_protocol::context::RenderedSubjectContext;
use jails_protocol::declaration::{FieldSpec, IntentArguments, IntentSpec};
use jails_protocol::edit::SemanticEdit;
use jails_protocol::entity::{
    CapabilityId, CapabilitySpec, EntityId, EntitySpec, IntentId, OneShotId, OneShotSpec, OwnerId,
    SourceInputId,
};
use jails_protocol::identity::{JavaType, Name, ObjectId, Package, ProjectPath};
use jails_protocol::ownership::{DesiredEntity, DesiredState, ObservedEntity, ReconcileScope};
use jails_protocol::pending::{DesiredInputGuard, DesiredInputId, FrozenDesiredInput};
use jails_protocol::plan::{
    DesiredAppliedEntity, DesiredChangeSet, DesiredOneShotReceipt, LedgerIntent, PlannedSubject,
};
use jails_protocol::provenance::{OneShotKind, RendererId};
use jails_protocol::render::{DesiredBody, DesiredFile, ManagedPath};
use jails_protocol::request::{
    CanonicalCapability, CanonicalGenerateRequest, CanonicalMutationRequest,
    CanonicalRequestSyntaxV1,
};
use jails_protocol::resource::{
    DesiredResource, OneShotLifecycle, OneShotState, ResourceKey, ResourceOwner, ResourceValue,
};
use jails_protocol::snapshot::{MachineRootPresence, TemplateStore};
use jails_protocol::transition::{CommitPlan, EffectResumeReason, EffectRetryPlan, ReceiptGuard};
use jails_spec::spec::kind::{ArtifactKind, Capability};
use jails_support::Result;

mod app;
mod artifact;
mod capability;
mod commit;
mod contract;
mod declare;
mod feature;
mod field;
mod history;
mod lifecycle;
mod maintenance;
mod oneshot;
mod portable;
mod provenance;
mod query;
mod request;
mod session;
mod support;

pub use app::{Intent, app_apply};
pub use artifact::{RequestedStorageRetirement, destroy, generate, recipe, recipe_with_field_data};
pub use capability::{install, remove, sync};
pub use commit::finish_interrupted;
pub use contract::contract_emit;
pub use declare::{add_dependency, set_property, undeclare};
pub use feature::{install_fast_test, remove_fast_test};
pub use field::{
    add_field, add_field_with_data, change_field_type, drop_field, field, field_with_data,
    rename_field, set_field_nullability, set_field_nullability_with_data,
};
pub use history::undo_files;
pub use lifecycle::{repair, revive};
pub use maintenance::{
    RenameResourceInvocation, adopt_layout, app_init, format, rename, rename_resource,
    rename_storage,
};
pub use oneshot::{cases, migration};
pub use portable::apply_plan;
pub use query::sql_generate;
pub(crate) use session::PreparedOutcome;
pub use session::{Outcome, Run};

// The two halves of what used to be this file. `pending.md` §8.1: assembling a
// request and driving a commit are different subjects, and everything below the
// module list was one or the other. Re-exported into this module's namespace so
// the eleven route submodules keep saying `super::commit(..)` and
// `super::Asked` -- the seam is where the code lives, not a new vocabulary.
use commit::{commit, commit_set, commit_subject, observed, retry_existing};
use request::{
    Asked, Declared, Request, asked_capabilities, declaration, declared, declared_capabilities,
    identity, record_capability, recorded_migrations, refuse_reserved_variable, retiring,
};

/// A kind as the word somebody types, taken from the same `ValueEnum` clap
/// parses -- so a refusal naming `jails g <kind>` names a command that exists.
pub use support::with_test_support;

fn label(kind: ArtifactKind) -> String {
    kind.to_possible_value()
        .expect("every ArtifactKind has a clap value")
        .get_name()
        .to_string()
}

/// Assemble a change set whose durable effect is entirely resource ownership.
///
/// Query projection is the first route with this shape, but keeping the store
/// representation here prevents feature routes from depending on how an
/// otherwise empty durable transition is encoded.
fn resource_change_set(
    generation_before: u64,
    ordered: Vec<DesiredChange>,
    resources_after: Vec<DesiredResource>,
    subject: PlannedSubject,
) -> DesiredChangeSet {
    DesiredChangeSet {
        ledger_intent: LedgerIntent {
            generation_before,
            entities_after: Vec::new(),
            one_shots_after: Vec::new(),
            resources_after,
            entities_removed: Vec::new(),
        },
        ordered,
        subject,
    }
}

/// A planned artifact's path, as the project-relative name a resource has.
///
/// A recipe plans in absolute paths because it writes files; a resource is
/// named by where it sits in the project, so that the same record means the
/// same thing on another machine.
/// The project as the changes planned so far leave it.
///
/// Generators read the project back: a query that constructs `Order` asks the
/// projection for `Order.java`'s components rather than being told them. So a
/// transition that rewrites `Order.java` and then plans something which reads
/// it has to plan the second half against the bytes the first half will
/// write, not against the ones still on disk. Reading disk instead is how
/// `g field` came to leave every companion constructing the old component
/// list -- a project that does not compile, over which every jails oracle
/// reported health, because each file was byte-identical to what jails wrote.
/// A recorded intent's declared order, as the token the CLI takes.
///
/// One spelling in and out: `IndexSpec::canonical` is what `--order-by` parses,
/// so a re-plan hands the generator exactly what the reader typed rather than a
/// second rendering of it.
pub(super) fn ordering_token(spec: &IntentSpec) -> Option<String> {
    (!spec.order_by.is_empty()).then(|| {
        jails_protocol::declaration::IndexSpec {
            columns: spec.order_by.clone(),
        }
        .canonical()
    })
}

pub(super) fn projected_after(
    project: &Project,
    reads: &ReadDeclaration,
    changes: &[DesiredChange],
) -> Result<Project> {
    let (_, mut projection) = capture::projected(project, reads)?;
    for change in changes {
        projection.advance(change)?;
    }
    let mut overlay = BTreeMap::new();
    for (path, entry) in projection.overlay() {
        if let jails_project::projection::ProjectedEntry::File(file) = entry {
            overlay.insert(path.clone(), file.bytes.to_vec());
        }
    }
    Project::projected(project, overlay)
}

fn relative_path(project: &Project, path: &std::path::Path) -> Result<ProjectPath> {
    let relative = path.strip_prefix(project.root()).map_err(|_| {
        format!(
            "{} is outside {}, so this request cannot claim it",
            path.display(),
            project.root().display()
        )
    })?;
    let text = relative
        .to_str()
        .ok_or_else(|| format!("{} is not valid UTF-8", relative.display()))?;
    ProjectPath::parse(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A refusal that names `jails g <kind>` names a command that exists.
    ///
    /// `label` reads clap's own `ValueEnum` rather than a second table, so the
    /// word in a message and the word the parser accepts cannot drift apart.
    #[test]
    fn a_kinds_label_is_the_word_clap_accepts() {
        for kind in ArtifactKind::value_variants() {
            let spelled = label(*kind);
            assert_eq!(
                ArtifactKind::from_str(&spelled, false).unwrap(),
                *kind,
                "`{spelled}` is what a refusal prints, so it has to parse back"
            );
        }
    }

    /// The smallest tree `Project::load` accepts: a pom and one class to read
    /// the base package off.
    fn project(label: &str) -> (jails_support::scratch::ScratchDir, Project) {
        let root = jails_support::scratch::ScratchDir::in_temp(label).unwrap();
        let sources = root.path().join("src/main/java/com/example/demo");
        std::fs::create_dir_all(&sources).unwrap();
        std::fs::write(
            root.path().join("pom.xml"),
            "<project><modelVersion>4.0.0</modelVersion><groupId>com.example</groupId>\
             <artifactId>demo</artifactId><version>0.0.1</version></project>",
        )
        .unwrap();
        std::fs::write(
            sources.join("App.java"),
            "package com.example.demo;\n\npublic class App {}\n",
        )
        .unwrap();
        let project = Project::load(root.path()).unwrap();
        (root, project)
    }

    #[test]
    fn a_path_inside_the_project_is_claimed_by_its_project_relative_name() {
        let (root, project) = project("route-relative");
        let path = root.path().join("src/main/java/com/example/demo/App.java");
        assert_eq!(
            relative_path(&project, &path).unwrap().as_str(),
            "src/main/java/com/example/demo/App.java"
        );
    }

    /// A path outside the project is a refusal, not a `..` walk out of it.
    #[test]
    fn a_path_outside_the_project_is_refused_by_naming_both() {
        let (_root, project) = project("route-outside");
        let message = relative_path(&project, std::path::Path::new("/etc/passwd")).unwrap_err();
        assert!(
            message.contains("/etc/passwd") && message.contains("cannot claim it"),
            "got {message}"
        );
    }
}
