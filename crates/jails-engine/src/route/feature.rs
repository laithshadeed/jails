//! `test --fast` and `remove fast-test`: the one tool feature that exists.
//!
//! §R6.2 is emphatic about the shape here: *"add or retain the `DirectCli`
//! owner of persistent `ToolFeature::FastTest`; it is not a maintenance side
//! channel."* V1 makes it exactly that side channel -- `run::ensure_console_
//! launcher` splices `junit-platform-console` into the reader's POM as a side
//! effect of running tests, records nothing, and leaves no way to take it back
//! out. A dependency that appeared because of how somebody ran their tests,
//! that nothing can name and nothing can remove, is the failure the ownership
//! model exists to prevent.
//!
//! So it is an entity with an owner, and `remove fast-test` is an ordinary
//! reconciliation that gives that ownership up.

use super::*;
use jails_protocol::entity::ToolFeature;

/// Put JUnit's console launcher on the test classpath, as an owned entity.
///
/// The version is not a caller's choice and not a constant: it **must equal
/// the project's own JUnit version**, and a mismatch does not fail to
/// resolve -- it dies at run time with a `NoSuchMethodError` wrapped in "the
/// versions of JUnit jars on the classpath are not properly aligned". A pom
/// that manages the version (a Spring Boot parent, or an imported `junit-bom`)
/// must therefore get **no** version at all: a redundant one pins the launcher
/// while the BOM moves the engine, which is the same misalignment by another
/// route.
pub fn install_fast_test(run: &Run) -> Result<Outcome> {
    let project = run.project();
    let id = EntityId::ToolFeature(ToolFeature::FastTest);
    let owner = ResourceOwner::Entity(id.clone());
    let (version, spec) = console_version(project)?;
    let mut change = jails_project::model::Change::default();
    change.deps.push(jails_project::pom::Dependency {
        group_id: "org.junit.platform",
        artifact_id: "junit-platform-console",
        version,
        scope: Some("test"),
        optional: false,
    });

    let desired = desire::contribution(&owner, &change, project)?;
    let entity = DesiredEntity {
        id: id.clone(),
        spec: EntitySpec::ToolFeature(spec),
        // `DirectCli`, not `DirectConfig`: nobody wrote this in `jails.toml`,
        // and a later `sync` against that list must not take it away.
        owners: BTreeSet::from([OwnerId::DirectCli]),
    };
    let reads = declaration(project, &change, &desired)?;
    let request = Request {
        // This entity alone. `add`'s scope speaks for the whole capability
        // list; here nothing else is being decided, and declaring a wider
        // scope would relinquish everything outside it.
        scope: ReconcileScope::DirectEntity(id),
        declared: BTreeMap::from([(entity.id.clone(), entity)]),
        changes: vec![desired],
    };
    commit(
        run,
        request,
        &reads,
        // The subcommand is `test`, and `--fast` is what makes it a mutation
        // -- so the flag is part of the request rather than presentation.
        &Asked::new(
            CanonicalMutationRequest::FastTest,
            &["test"],
            Vec::new(),
            BTreeMap::new(),
            BTreeSet::from(["fast".to_string()]),
        ),
    )
}

/// Give that ownership up again.
///
/// The exact counterpart, through the same scope: what `--fast` claimed is
/// what this relinquishes, and the POM edit is undone by the format's own
/// unsplice rather than by a second hand-written removal.
pub fn remove_fast_test(run: &Run) -> Result<Outcome> {
    let project = run.project();
    let id = EntityId::ToolFeature(ToolFeature::FastTest);
    let owner = ResourceOwner::Entity(id.clone());
    let store = observed(project)?;
    let reads = retiring(&store, &owner)?;
    let request = Request {
        scope: ReconcileScope::DirectEntity(id),
        // Declared present and empty: this owner wants nothing, which is what
        // makes every resource it holds a relinquishment rather than silence.
        declared: BTreeMap::new(),
        changes: Vec::new(),
    };
    commit(
        run,
        request,
        &reads,
        &Asked::plain(
            CanonicalMutationRequest::RemoveToolFeature {
                feature: ToolFeature::FastTest,
                force: false,
            },
            &["remove"],
            &["fast-test"],
        ),
    )
}

/// The console version this project needs, as both the POM spelling and the
/// recorded spec.
fn console_version(
    project: &Project,
) -> Result<(
    Option<&'static str>,
    jails_protocol::entity::ToolFeatureSpec,
)> {
    match jails_project::junit::console_version(&jails_project::pom::read(project.root())?) {
        jails_project::junit::ConsoleVersion::Managed => Ok((
            None,
            jails_protocol::entity::ToolFeatureSpec {
                console_version: jails_protocol::coordinate::MavenVersion::Managed,
            },
        )),
        jails_project::junit::ConsoleVersion::Pinned(version) => {
            let managed = jails_protocol::identity::ManagedVersion::parse(&version)?;
            Ok((
                // Leaked deliberately: `pom::Dependency` is a compile-time
                // constant everywhere else, this one string is derived once
                // per process, and it has to outlive the splice. Threading a
                // lifetime through forty const declarations to avoid one
                // CLI-lifetime allocation is the worse trade.
                Some(&*version.leak()),
                jails_protocol::entity::ToolFeatureSpec {
                    console_version: jails_protocol::coordinate::MavenVersion::Pinned(managed),
                },
            ))
        }
        jails_project::junit::ConsoleVersion::Unknown => Err(jails_support::Failure::Told(
            "this project declares no JUnit version, so jails cannot align the console \
             launcher with it.\n       A mismatched launcher resolves fine and then dies with \
             NoSuchMethodError.\n       fix: declare org.junit.jupiter:junit-jupiter (or import \
             junit-bom), then retry --fast."
                .to_string(),
        )),
    }
}
