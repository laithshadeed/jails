//! Which managed files an ejection would move into reader source.
//!
//! Asked of the model rather than of a tree on disk, because ejection has to
//! answer it *before* anything is written -- a destination collision must
//! refuse before any model, lock, migration or generated-tree write. The
//! managed ABI is deliberately not in the answer: records and ports stay
//! jails', and only an adapter implementation is transferable.

use super::*;

/// Which emitted files an ejection boundary owns.
///
/// **`spring_boot` is a required argument because the emitters branch on it.**
/// This re-emits the tree to find the boundary's files, so passing `None` when
/// the project does have Boot makes every `BootCondition::Spring` pack emit
/// nothing *here* while emitting normally everywhere else: `jails model eject
/// cap_kafka` then refuses "emits no ejectable Java implementation" with
/// `KafkaConfig.java` plainly on disk, while a `BootCondition::Any` pack like
/// `cap http` ejects fine -- so the failure reads as a property of the
/// capability rather than of this function. The caller observes the version
/// the same way `capture` does.
pub fn implementation_paths(
    model: &jails_model::AppModel,
    ejection_id: &str,
    spring_boot: Option<&str>,
    maven_wrapper: bool,
) -> Result<Vec<ProjectPath>, CompileError> {
    let root = ProjectPath::parse(MANAGED_ROOT).map_err(CompileError::new)?;
    let mut generated = RenderedTree::new(root);
    let compose_path = ProjectPath::parse("compose.yaml").map_err(CompileError::new)?;
    emit::emit(
        model,
        &mut generated,
        &emit::Observed {
            spring_boot,
            // Which files an ejection moves is a question about paths, and a
            // reader's template cannot move one -- the placeholder set is the
            // contract and a path is not in it.
            templates: &jails_contracts::TemplateOverrides::default(),
            compose_path: &compose_path,
            maven_wrapper,
            // This renders only to work out which files an ejection would
            // move, and both adapters are ejectable whichever one is the bean.
            jdbc: model
                .capabilities
                .values()
                .any(|capability| capability.kind == "db"),
            // A `package-info.java` is not ejectable, so its presence cannot
            // change which files an ejection moves.
            jspecify: false,
        },
    )?;
    Ok(generated
        .files
        .into_iter()
        .filter(|(_, file)| {
            file.provenance.ejectable && file.provenance.ejection_target() == ejection_id
        })
        .filter_map(|(path, file)| {
            let destination = match file.kind {
                FileKind::JavaMain => path
                    .as_str()
                    .strip_prefix(".jails/generated/main/java/")
                    .map(|suffix| format!("src/main/java/{suffix}")),
                FileKind::JavaTest => path
                    .as_str()
                    .strip_prefix(".jails/generated/test/java/")
                    .map(|suffix| format!("src/test/java/{suffix}")),
                FileKind::Resource => path
                    .as_str()
                    .strip_prefix(".jails/generated/main/resources/")
                    .map(|suffix| format!("src/main/resources/{suffix}"))
                    .or_else(|| {
                        path.as_str()
                            .strip_prefix(".jails/generated/test/resources/")
                            .map(|suffix| format!("src/test/resources/{suffix}"))
                    }),
                FileKind::HttpCollection => None,
            }?;
            ProjectPath::parse(destination).ok()
        })
        .collect())
}

/// Whether this component kind has an emitter behind it.
///
/// `audit.md` A1.2. Fifteen of the twenty-three closed kinds linked, planned,
/// applied and reported success while producing no file and no diagnostic.
/// `component client Audit` was accepted, `model check` said "model valid",
/// `sync` said "3 operations, 4 files written", and nothing in the tree
/// mentioned it. A silent no-op on a declaration the author wrote is worse
/// than a refusal, because there is nothing to notice.
///
/// The match is exhaustive on purpose: `jdl-sol.md` §20.2 asks for a test that
/// fails "when a registered role has no emitter", and the strongest version of
/// that test is a compile error. Adding a kind stops the build here until
/// somebody decides which arm it belongs in.
pub(crate) const fn component_kind_is_emitted(kind: jails_model::ComponentKind) -> bool {
    use jails_model::ComponentKind as Kind;
    match kind {
        Kind::Class
        | Kind::Interface
        | Kind::Service
        | Kind::Controller
        | Kind::Sealed
        | Kind::Strategy
        | Kind::Test
        | Kind::IntegrationTest => true,
        // `cases` emits no Java, but it is not silent: its reader-owned
        // brief is captured as an exact plan input, so changing the file
        // after review refuses the apply. A backend need not write a file.
        Kind::Cases => true,
        Kind::Auth
        | Kind::Cli
        | Kind::Client
        | Kind::Command
        | Kind::Handler
        | Kind::Fetcher
        | Kind::Idempotency
        | Kind::Job
        | Kind::Presence
        | Kind::Socket
        | Kind::HttpSink
        | Kind::HttpWorkflow
        | Kind::DurableJob
        | Kind::Webhook => true,
    }
}
