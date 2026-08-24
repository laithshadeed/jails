//! `add dependency` and `set`: the two commands where the reader names the
//! resource and jails contributes only the edit and the record.
//!
//! missing.md §3 and §5 are the same gap seen twice. jails splices
//! dependencies constantly and owns every property a capability writes, and
//! every one of those paths is reachable **only from inside a generator**. A
//! project needing one artifact jails has never heard of, or one setting no
//! capability owns, had to hand-edit `pom.xml` or `application.properties` --
//! which is exactly the file `pom.rs` and the property resource exist to edit
//! surgically, and a hand edit is invisible to `remove`, to `sync`, and to the
//! collision check that stops two owners claiming one key.
//!
//! What this is *not* is a capability. A capability knows what a library is
//! for: `add db` installs Flyway and Testcontainers and a compose service
//! because it knows what a database is. Here jails knows nothing, and says so
//! -- there is no wiring, no test, and no `jails.toml` entry, because there is
//! nothing to sync a declaration against beyond the declaration itself.

use super::*;
use jails_protocol::coordinate::{DependencySpec, MavenCoordinate, MavenScope, MavenVersion};
use jails_protocol::entity::{DeclaredId, DeclaredSpec};

/// Splice one artifact into the build file, as an owned entity.
///
/// `version` is `None` for "let the pom manage it", which is the right answer
/// under a Spring Boot parent or an imported BOM and the *fatal* one without:
/// Maven refuses to read a pom whose dependency has no version and nothing
/// manages it, and every goal fails including `validate`. jails cannot tell
/// the two apart for an artifact it has never heard of -- only the reader
/// knows whether their BOM covers it -- so this asks rather than guesses, and
/// the refusal names the two spellings.
pub fn add_dependency(
    run: &Run,
    coordinate: MavenCoordinate,
    version: Option<String>,
    scope: MavenScope,
) -> Result<Outcome> {
    let project = run.project();
    let version = match version {
        Some(text) => MavenVersion::Pinned(jails_protocol::identity::ManagedVersion::parse(&text)?),
        None => MavenVersion::Managed,
    };
    let id = DeclaredId::Dependency(coordinate.clone());
    let owner = ResourceOwner::Entity(EntityId::Declared(id.clone()));
    let spec = DependencySpec {
        coordinate: coordinate.clone(),
        version: version.clone(),
        scope,
        optional: false,
    };

    let mut change = Change::default();
    change.deps.push(jails_project::pom::Dependency {
        // Leaked for the reason `feature.rs` leaks the console version:
        // `pom::Dependency` is a compile-time constant at every other call
        // site, these three strings are derived once per process, and they
        // have to outlive the splice. A lifetime threaded through forty const
        // declarations to save one CLI-lifetime allocation is the worse trade.
        group_id: coordinate.group_id.as_str().to_string().leak(),
        artifact_id: coordinate.artifact_id.as_str().to_string().leak(),
        version: match &version {
            MavenVersion::Managed => None,
            MavenVersion::Pinned(pinned) => Some(&*pinned.to_string().leak()),
        },
        scope: match scope {
            MavenScope::Compile => None,
            MavenScope::Runtime => Some("runtime"),
            MavenScope::Test => Some("test"),
        },
        optional: false,
    });

    let desired = desire::contribution(&owner, &change, project)?;
    let entity = DesiredEntity {
        id: EntityId::Declared(id.clone()),
        spec: EntitySpec::Declared(DeclaredSpec::Dependency(spec)),
        // `DirectCli`: nobody wrote this in `jails.toml`, and a later `sync`
        // against that list must not take it away.
        owners: BTreeSet::from([OwnerId::DirectCli]),
    };
    let reads = declaration(project, &change, &desired)?;
    let request = CanonicalMutationRequest::declare(
        id.clone(),
        DeclaredSpec::Dependency(DependencySpec {
            coordinate: coordinate.clone(),
            version,
            scope,
            optional: false,
        }),
    )?;
    let asked = Asked::new(
        request,
        &["add", "dependency"],
        vec![format!(
            "{}:{}",
            coordinate.group_id.as_str(),
            coordinate.artifact_id.as_str()
        )],
        scope_option(scope),
        BTreeSet::new(),
    );
    commit(
        run,
        Request {
            scope: ReconcileScope::DirectEntity(EntityId::Declared(id)),
            declared: BTreeMap::from([(entity.id.clone(), entity)]),
            changes: vec![desired],
        },
        &reads,
        &asked,
    )
}

/// Set one property in one file, as an owned entity.
///
/// `where_` picks between the application's own configuration and the test
/// overlay. The overlay is `src/test/resources/config/application.properties`
/// and not the spelling everybody reaches for, and the difference is not
/// cosmetic: `classpath:/config/` outranks `classpath:/` **and is additive**,
/// so one key here overrides one key there. `src/test/resources/
/// application.properties` shadows the main file wholesale and silently
/// unsets everything the tests did not restate.
pub fn set_property(run: &Run, key: String, value: String, in_tests: bool) -> Result<Outcome> {
    let project = run.project();
    let file = if in_tests {
        desire::TEST_CONFIG_PROPERTIES
    } else {
        desire::APPLICATION_PROPERTIES
    };
    let id = DeclaredId::Property {
        path: ProjectPath::parse(file)?,
        key: jails_protocol::identity::PropertyKey::parse(&key)?,
    };
    let owner = ResourceOwner::Entity(EntityId::Declared(id.clone()));
    let setting = jails_protocol::resource::PropertySetting::new(value.clone(), Vec::new())?;

    let mut change = Change::default();
    let line = format!("{key}={value}");
    if in_tests {
        change.test_properties.push(line);
    } else {
        change.properties.push(line);
    }

    let desired = desire::contribution(&owner, &change, project)?;
    let entity = DesiredEntity {
        id: EntityId::Declared(id.clone()),
        spec: EntitySpec::Declared(DeclaredSpec::Property(setting.clone())),
        owners: BTreeSet::from([OwnerId::DirectCli]),
    };
    let reads = declaration(project, &change, &desired)?;
    let request = CanonicalMutationRequest::declare(id.clone(), DeclaredSpec::Property(setting))?;
    let asked = Asked::new(
        request,
        &["set"],
        vec![format!("{key}={value}")],
        BTreeMap::new(),
        if in_tests {
            BTreeSet::from(["tests".to_string()])
        } else {
            BTreeSet::new()
        },
    );
    commit(
        run,
        Request {
            scope: ReconcileScope::DirectEntity(EntityId::Declared(id)),
            declared: BTreeMap::from([(entity.id.clone(), entity)]),
            changes: vec![desired],
        },
        &reads,
        &asked,
    )
}

/// Give one declared resource up again.
///
/// The exact counterpart of whichever command declared it, through the same
/// scope: the edit is undone by the format's own unsplice rather than by a
/// second hand-written removal, which is what keeps `remove` from drifting
/// away from `add`.
pub fn undeclare(run: &Run, id: DeclaredId) -> Result<Outcome> {
    let project = run.project();
    let owner = ResourceOwner::Entity(EntityId::Declared(id.clone()));
    let store = observed(project)?;
    let reads = retiring(&store, &owner)?;
    let (command, positional) = match &id {
        DeclaredId::Dependency(coordinate) => (
            vec!["remove", "dependency"],
            format!(
                "{}:{}",
                coordinate.group_id.as_str(),
                coordinate.artifact_id.as_str()
            ),
        ),
        DeclaredId::Property { key, .. } => (vec!["unset"], key.as_str().to_string()),
    };
    commit(
        run,
        Request {
            scope: ReconcileScope::DirectEntity(EntityId::Declared(id.clone())),
            // Declared present and empty: this owner wants nothing, which is
            // what makes every resource it holds a relinquishment rather than
            // silence.
            declared: BTreeMap::new(),
            changes: Vec::new(),
        },
        &reads,
        &Asked::plain(
            CanonicalMutationRequest::Undeclare { id, force: false },
            &command,
            &[positional.as_str()],
        ),
    )
}

/// `--scope` as the option map an `Asked` records, and only when it was said.
///
/// §R5.4: the syntax record carries what was *explicitly supplied*, so the
/// default must not appear -- a rerun typed without the flag is the same
/// request and has to hash as one.
fn scope_option(scope: MavenScope) -> BTreeMap<String, Vec<String>> {
    match scope {
        MavenScope::Compile => BTreeMap::new(),
        other => BTreeMap::from([("scope".to_string(), vec![other.label().to_string()])]),
    }
}
