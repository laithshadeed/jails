//! What a planned recipe change *means*, as desired state.
//!
//! plan.md §R6.1 step 2 asks for capability `add`/`remove`/`sync` on V2 while
//! default dispatch stays on V1, and §R6.2's first row says how: *"recipe
//! metadata → desired capability/resource change → one prepared commit"*. This
//! module is that arrow. It takes the value a recipe already produces — the
//! dependencies, plugins, files, compose services and properties it intends —
//! and states them as owned resources instead of as a list of writes.
//!
//! ## Why this is a translation and not a rewrite of the recipes
//!
//! The recipes are the part of jails that knows Spring Boot 4 moved
//! `@AutoConfigureMockMvc`, that Jackson 3 is `tools.jackson`, that Commons CSV
//! renamed `build()` to `get()`. None of that changes when the mutation
//! architecture does, and rewriting it in the same pass would put a hundred
//! version facts and a new transaction protocol in one diff. So the recipes
//! keep producing `model::Change` and this module says what it means.
//!
//! ## What it deliberately refuses
//!
//! One contribution has no home in the closed protocol yet: a Spring test
//! import. It is *refused by name* rather than dropped. A translator that
//! silently loses a contribution produces a desired state that is missing
//! something no test asked about — and the first symptom is a project that
//! compiles and does not start, which is exactly the failure the whole
//! ownership model exists to stop. §R6.3's row for `add::test_wiring` is where
//! the missing variant is designed; until it exists, this says so.
//!
//! ## Ownership, not authorship
//!
//! Every resource here is claimed by the entity the caller names, and a claim
//! is a set. Two capabilities wanting the same dependency is ordinary and both
//! own it; `remove` takes one claim away and the resource survives while the
//! other claim stands. That is the whole reason a dependency is a resource
//! rather than a line somebody spliced.

use std::collections::BTreeSet;

use jails_project::compose::Service as ComposeService;
use jails_project::model::{Change, Project};
use jails_project::pom::Dependency;
use jails_protocol::change::DesiredChange;
use jails_protocol::coordinate::{
    CanonicalPluginXml, DependencySpec, MavenCoordinate, MavenScope, MavenVersion, PluginSpec,
};
use jails_protocol::edit::SemanticEdit;
use jails_protocol::identity::{
    JavaType, ManagedVersion, MarkerId, ProjectPath, PropertyKey, ServiceName, VolumeName,
};
use jails_protocol::render::{DesiredBody, DesiredFile};
use jails_protocol::resource::{
    CanonicalYamlMapping, ComposeServiceSpec, DesiredResource, PropertySetting, ResourceKey,
    ResourceOwner, ResourceValue,
};

use crate::Result;

/// The files a recipe's contributions land in.
///
/// Four paths, stated once. Each is a *format owner's* file — a file jails
/// edits surgically rather than owns whole — which is why none of them is
/// a `WholeFile` resource and all four are addressed by the key of the thing
/// inside them.
pub const POM: &str = "pom.xml";
pub const COMPOSE: &str = "compose.yaml";
pub const APPLICATION_PROPERTIES: &str = "src/main/resources/application.properties";

/// One planned recipe change, as the desire it expresses.
///
/// `owner` is who is asking. Everything below is charged to it, and nothing
/// else in the project is claimed by this call. `project` is here for one
/// reason: a recipe plans in absolute paths and a resource is named by a
/// project-relative one, and the conversion is a check — a contribution
/// pointing outside the project is refused rather than silently reinterpreted.
pub fn contribution(
    owner: &ResourceOwner,
    change: &Change,
    project: &Project,
) -> Result<DesiredChange> {
    let mut desired = DesiredChange::owned_by(owner.clone());
    if let Some(import) = &change.spring_test_import {
        state_test_import(&mut desired, owner, import, project)?;
    }
    for dependency in &change.deps {
        let (key, value) = dependency_resource(dependency)?;
        claim(&mut desired, owner, key.clone(), value.clone())?;
        let ResourceValue::MavenDependency(spec) = value else {
            unreachable!("dependency_resource returns a dependency value");
        };
        desired
            .edits
            .push(SemanticEdit::MavenDependency { key, value: spec });
    }
    for (artifact_id, block) in &change.plugins {
        let (key, value) = plugin_resource(artifact_id, block)?;
        claim(&mut desired, owner, key.clone(), value.clone())?;
        let ResourceValue::MavenPlugin(spec) = value else {
            unreachable!("plugin_resource returns a plugin value");
        };
        desired
            .edits
            .push(SemanticEdit::MavenPlugin { key, value: spec });
    }
    for service in &change.compose {
        let (key, value) = compose_resource(service)?;
        claim(&mut desired, owner, key.clone(), value.clone())?;
        let ResourceValue::ComposeService(spec) = value else {
            unreachable!("compose_resource returns a compose value");
        };
        desired
            .edits
            .push(SemanticEdit::ComposeService { key, value: spec });
    }
    // A dispatch line in a dispatcher this change does not own: `g command`
    // writes the command class and registers it in the CLI that runs it.
    for registration in &change.registrations {
        let key = ResourceKey::CommandRegistration {
            dispatcher: registration.dispatcher.clone(),
            command: registration.command.clone(),
        };
        claim(
            &mut desired,
            owner,
            key.clone(),
            ResourceValue::CommandRegistration {
                command: registration.command.clone(),
            },
        )?;
        desired.edits.push(SemanticEdit::CommandRegistration {
            key,
            command: registration.command.clone(),
        });
    }
    // A block in a file this change does not own whole: one durable job's
    // limits in the app-wide test property source, beside another job's block
    // and whatever the reader put between them. Keyed by path and marker, so
    // removal takes exactly this one out.
    for block in &change.marked {
        let key = ResourceKey::MarkedBlock {
            path: ProjectPath::parse(&block.path)?,
            marker: jails_protocol::identity::MarkerId::parse(&block.marker)?,
        };
        let value = ResourceValue::MarkedBlock(block.rendered());
        claim(&mut desired, owner, key.clone(), value)?;
        desired.edits.push(SemanticEdit::MarkedBlock {
            key,
            body: block.rendered(),
        });
    }
    // A capability's property block is prose *and* settings, in the order it
    // wrote them, and a comment documents the line beneath it. Carrying the
    // pending lines forward is what lets a per-key resource own the
    // explanation the marked block used to hold.
    let mut prose: Vec<String> = Vec::new();
    for line in &change.properties {
        if line.trim().is_empty() {
            continue;
        }
        if let Some(comment) = line.trim().strip_prefix('#') {
            prose.push(comment.trim().to_string());
            continue;
        }
        let (key, value) = property_resource(line, std::mem::take(&mut prose))?;
        claim(&mut desired, owner, key.clone(), value.clone())?;
        let ResourceValue::Property(setting) = value else {
            unreachable!("property_resource returns a property value");
        };
        desired.edits.push(SemanticEdit::Property {
            key,
            value: setting,
        });
    }
    if let Some(orphan) = prose.first() {
        return Err(format!(
            "the comment `{orphan}` is the last thing this capability's properties say, so it \
             documents nothing.\n       \
             fix: a property comment introduces the line beneath it. A trailing one would be \
             written above whichever property happened to be added next."
        ));
    }
    for artifact in &change.files {
        let path = project_path(&artifact.path, project)?;
        let key = ResourceKey::WholeFile(path.clone());
        claim(&mut desired, owner, key.clone(), ResourceValue::WholeFile)?;
        desired.files.push(DesiredFile {
            path,
            body: DesiredBody::Bytes(artifact.contents.as_bytes().into()),
            mode: None,
            resource: Some(key),
            renderer: None,
        });
    }
    // Superseded artifacts are claimed, never written. See
    // `DesiredChange::adopted` for why asserting their absence here would be
    // the wrong verb for a command called `add`.
    for dependency in &change.legacy_deps {
        let (key, _) = dependency_resource(dependency)?;
        if desired.resources.iter().any(|held| held.key == key) {
            return Err(format!(
                "this capability both installs and supersedes {key:?}"
            ));
        }
        if !desired.adopted.contains(&key) {
            desired.adopted.push(key);
        }
    }
    Ok(desired)
}

/// Claim the `@Import` this capability needs on every `@SpringBootTest` there
/// is.
///
/// §R6.3's `add::test_wiring` row: *"keyed semantic contributions with
/// explicit owners"*. One claim per target file rather than one for the whole
/// project, because each is independent — a test written after this ran is not
/// covered by a claim about a file it is not in, which is exactly what makes
/// the *next* `add`/`sync` notice it.
///
/// The targets are read off the live tree, and that read is a precondition:
/// every path here is declared by the caller and rechecked under the lock, so
/// a test that appeared between planning and commit makes this refuse rather
/// than silently miss it.
fn state_test_import(
    desired: &mut DesiredChange,
    owner: &ResourceOwner,
    import: &jails_project::model::SpringTestImport,
    project: &Project,
) -> Result<()> {
    let class = JavaType::parse(&import.fqcn())?;
    for path in spring_boot_tests(project) {
        // The `import` statement is only needed when the test is in a
        // different package from the config, and `import_of` returns an empty
        // string when they match -- which is what keeps a flat project
        // compiling.
        let source =
            std::fs::read_to_string(project.root().join(path.as_str())).unwrap_or_default();
        let tests_package =
            jails_java::java::package_of(&source).unwrap_or_else(|| import.pkg.clone());
        let statement = if tests_package == import.pkg {
            String::new()
        } else {
            format!("import {};\n", import.fqcn())
        };
        let key = ResourceKey::SpringTestImport {
            path,
            class: class.clone(),
        };
        claim(
            desired,
            owner,
            key.clone(),
            ResourceValue::SpringTestImport {
                class: class.clone(),
                statement: statement.clone(),
            },
        )?;
        desired.edits.push(SemanticEdit::SpringTestImport {
            key,
            class: class.clone(),
            statement,
        });
    }
    Ok(())
}

/// Every `@SpringBootTest` under `src/test/java`, in a stable order.
///
/// Sorted because the order decides the order of the edits, and two runs of
/// one request that produced different transactions would make the receipt
/// depend on how the filesystem happened to enumerate a directory. Through
/// the shared reader, which matches the annotation on the top-level type --
/// the class `add db` is adding shows a `@SpringBootTest` in its own Javadoc,
/// and a byte scan reads that example as a declaration.
fn spring_boot_tests(project: &Project) -> Vec<ProjectPath> {
    jails_java::java::types_annotated_with(&project.root().join("src/test/java"), "SpringBootTest")
        .into_iter()
        .filter_map(|found| {
            let relative = found.path.strip_prefix(project.root()).ok()?;
            ProjectPath::parse(relative.to_str()?).ok()
        })
        .collect()
}

/// Record one claim, refusing a second different value for one key.
///
/// Two owners of one resource is the design; two *values* for one key is a
/// planning bug, and it has to be caught here rather than at the splice, where
/// the loser would simply be whichever ran last.
fn claim(
    desired: &mut DesiredChange,
    owner: &ResourceOwner,
    key: ResourceKey,
    value: ResourceValue,
) -> Result<()> {
    if let Some(existing) = desired.resources.iter().find(|held| held.key == key) {
        if existing.value != value {
            return Err(format!(
                "one change states two different values for {key:?}"
            ));
        }
        return Ok(());
    }
    let owners = BTreeSet::from([owner.clone()]);
    desired
        .resources
        .push(DesiredResource::new(key, owners, value)?);
    Ok(())
}

fn dependency_resource(dependency: &Dependency) -> Result<(ResourceKey, ResourceValue)> {
    let coordinate = MavenCoordinate::parse(dependency.group_id, dependency.artifact_id)?;
    let version = match dependency.version {
        // Not a default worth hiding: a versionless `<dependency>` is correct
        // under a managing parent and fatal without one, so which of these two
        // a recipe chose is a fact about the project it planned against.
        None => MavenVersion::Managed,
        Some(text) => MavenVersion::Pinned(ManagedVersion::parse(text)?),
    };
    let spec = DependencySpec {
        coordinate: coordinate.clone(),
        version,
        scope: MavenScope::parse(dependency.scope.unwrap_or_default())?,
        optional: dependency.optional,
    };
    Ok((
        ResourceKey::MavenDependency(coordinate),
        ResourceValue::MavenDependency(spec),
    ))
}

fn plugin_resource(artifact_id: &str, block: &str) -> Result<(ResourceKey, ResourceValue)> {
    let xml = CanonicalPluginXml::parse(block)?;
    // The block is the authority on its own coordinate, and the recipe's
    // artifact id is checked against it rather than trusted: a plugin listed
    // under one name and declaring another is how an `unsplice` removes the
    // wrong element.
    let coordinate = xml.declared_coordinate()?;
    if coordinate.artifact_id.as_str() != artifact_id {
        return Err(format!(
            "the plugin block declares {coordinate} but the recipe files it under {artifact_id}"
        ));
    }
    let spec = PluginSpec::new(coordinate.clone(), xml)?;
    Ok((
        ResourceKey::MavenPlugin(coordinate),
        ResourceValue::MavenPlugin(spec),
    ))
}

fn compose_resource(service: &ComposeService) -> Result<(ResourceKey, ResourceValue)> {
    let name = ServiceName::parse(service.name)?;
    let spec = ComposeServiceSpec {
        name: name.clone(),
        marker: MarkerId::parse(service.marker)?,
        mapping: CanonicalYamlMapping::parse(&dedent(service.body)?)?,
        volumes: match service.volume {
            Some(volume) => BTreeSet::from([VolumeName::parse(volume)?]),
            None => BTreeSet::new(),
        },
    };
    Ok((
        ResourceKey::ComposeService(name),
        ResourceValue::ComposeService(spec),
    ))
}

fn property_resource(line: &str, comment: Vec<String>) -> Result<(ResourceKey, ResourceValue)> {
    let (key, value) = line.split_once('=').ok_or_else(|| {
        format!("`{line}` is not a `key=value` property line, so it names nothing to own")
    })?;
    Ok((
        ResourceKey::Property {
            path: ProjectPath::parse(APPLICATION_PROPERTIES)?,
            key: PropertyKey::parse(key.trim())?,
        },
        ResourceValue::Property(PropertySetting::new(value, comment)?),
    ))
}

fn project_path(path: &std::path::Path, project: &Project) -> Result<ProjectPath> {
    let relative = match path.strip_prefix(project.root()) {
        Ok(relative) => relative,
        // A relative path is already what a resource is named by, and a plan
        // that produced one is not wrong -- but an *absolute* path outside the
        // project is a contribution to somebody else's tree, which no owner in
        // this transaction can claim.
        Err(_) if path.is_relative() => path,
        Err(_) => {
            return Err(format!(
                "{} is outside {}, so no owner in this project can claim it",
                path.display(),
                project.root().display()
            ));
        }
    };
    let text = relative
        .to_str()
        .ok_or_else(|| format!("{} is not valid UTF-8", relative.display()))?;
    ProjectPath::parse(text)
}

/// Strip the indentation a compose service body was stored with.
///
/// The V1 value is written ready to splice *under* `services:\n  <name>:`, so
/// it carries four leading spaces on every line. The canonical value is stated
/// relative to the service and indented by the format owner, which is what
/// stops one mapping having two spellings. A line indented less than the first
/// is refused rather than guessed at: silently un-nesting somebody's YAML
/// changes what the file means.
fn dedent(body: &str) -> Result<String> {
    let indent = body
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start().len())
        .unwrap_or_default();
    let mut out = String::with_capacity(body.len());
    for line in body.lines() {
        if line.trim().is_empty() {
            out.push('\n');
            continue;
        }
        let stripped = line.strip_prefix(&" ".repeat(indent)).ok_or_else(|| {
            format!("compose mapping line `{line}` is indented less than the mapping it is in")
        })?;
        out.push_str(stripped);
        out.push('\n');
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use jails_protocol::entity::{CapabilityId, CapabilityInstance, EntityId};
    use jails_spec::spec::kind::Capability;

    fn owner() -> ResourceOwner {
        ResourceOwner::Entity(EntityId::Capability(CapabilityId {
            kind: Capability::Db,
            instance: CapabilityInstance::Singleton,
        }))
    }

    /// The smallest thing `Project::load` accepts.
    ///
    /// Every test here is about translation, so what the project *is* barely
    /// matters -- but a `Project` is only ever resolved from a real module
    /// root, and inventing a second way to make one would be a second
    /// authority on what a project is.
    fn project() -> (jails_support::scratch::ScratchDir, Project) {
        let scratch = jails_support::scratch::ScratchDir::in_temp("desire-fixture").unwrap();
        let root = scratch.path().to_path_buf();
        jails_support::apply::ensure_directory(root.join("src/main/java/com/example")).unwrap();
        jails_support::apply::put(
            root.join("pom.xml"),
            "<project><modelVersion>4.0.0</modelVersion><groupId>com.example</groupId>\
             <artifactId>demo</artifactId><version>0.0.1</version></project>\n",
        )
        .unwrap();
        jails_support::apply::put(
            root.join("src/main/java/com/example/App.java"),
            "package com.example;\n\npublic class App {}\n",
        )
        .unwrap();
        let project = Project::load(&root).unwrap();
        (scratch, project)
    }

    fn dependency() -> Dependency {
        Dependency {
            group_id: "org.postgresql",
            artifact_id: "postgresql",
            version: None,
            scope: Some("runtime"),
            optional: false,
        }
    }

    #[test]
    fn a_dependency_becomes_a_resource_the_capability_owns() {
        let change = Change {
            deps: vec![dependency()],
            ..Change::default()
        };
        let desired = contribution(&owner(), &change, &project().1).unwrap();
        assert_eq!(desired.resources.len(), 1);
        assert_eq!(desired.edits.len(), 1);
        assert!(desired.resources[0].owners.contains(&owner()));
        assert!(matches!(
            desired.resources[0].value,
            ResourceValue::MavenDependency(DependencySpec {
                version: MavenVersion::Managed,
                scope: MavenScope::Runtime,
                ..
            })
        ));
    }

    #[test]
    fn one_file_is_both_a_claim_and_a_body() {
        let change = Change {
            files: vec![jails_project::model::Artifact::rendered(
                std::path::PathBuf::from("src/test/java/com/example/TestcontainersConfig.java"),
                "class TestcontainersConfig {}".to_string(),
            )],
            ..Change::default()
        };
        let desired = contribution(&owner(), &change, &project().1).unwrap();
        assert_eq!(desired.files.len(), 1);
        assert_eq!(
            desired.files[0].resource.as_ref(),
            Some(&desired.resources[0].key),
            "the file's body and its claim name the same resource"
        );
    }

    /// A block in a file the change does not own becomes a keyed claim.
    ///
    /// Keyed by path *and* marker, which is what makes removal exact: two
    /// durable jobs write two blocks into one property file, and taking one
    /// out has to leave the other and anything the reader put between them.
    /// Owning the whole file instead would make `destroy` delete both.
    #[test]
    fn a_marked_block_is_claimed_by_its_path_and_its_marker() {
        let change = Change {
            marked: vec![jails_project::model::MarkedBlock {
                path: "src/test/resources/config/application.properties".to_string(),
                marker: "durable-job-item-dispatcher".to_string(),
                settings: vec!["jobs.item-dispatcher.max-attempts=2".to_string()],
            }],
            ..Change::default()
        };
        let desired = contribution(&owner(), &change, &project().1).unwrap();
        let ResourceKey::MarkedBlock { path, marker } = &desired.resources[0].key else {
            panic!(
                "expected a marked block, got {:?}",
                desired.resources[0].key
            );
        };
        assert_eq!(
            path.to_string(),
            "src/test/resources/config/application.properties"
        );
        assert_eq!(marker.as_str(), "durable-job-item-dispatcher");
        // And the file is not claimed whole: nothing here owns it.
        assert!(desired.files.is_empty(), "{:?}", desired.files);
        assert!(matches!(
            desired.edits.first(),
            Some(SemanticEdit::MarkedBlock { .. })
        ));
    }

    #[test]
    fn a_compose_service_is_stored_relative_to_itself() {
        let change = Change {
            compose: vec![jails_project::compose::POSTGRES],
            ..Change::default()
        };
        let desired = contribution(&owner(), &change, &project().1).unwrap();
        let ResourceValue::ComposeService(spec) = &desired.resources[0].value else {
            panic!("expected a compose service");
        };
        assert!(
            spec.mapping.as_str().starts_with("image: postgres"),
            "the stored mapping is not indented, got {:?}",
            spec.mapping.as_str()
        );
        assert!(spec.volumes.iter().any(|v| v.as_str() == "postgres-data"));
    }

    #[test]
    fn a_property_line_is_owned_by_its_key_not_by_a_block() {
        let change = Change {
            properties: vec!["spring.docker.compose.enabled=false".to_string()],
            ..Change::default()
        };
        let desired = contribution(&owner(), &change, &project().1).unwrap();
        let ResourceKey::Property { key, path } = &desired.resources[0].key else {
            panic!("expected a property");
        };
        assert_eq!(key.as_str(), "spring.docker.compose.enabled");
        assert_eq!(path.as_str(), APPLICATION_PROPERTIES);
    }

    #[test]
    fn a_comment_is_carried_onto_the_property_it_introduces() {
        let change = Change {
            properties: vec![
                "# jails starts compose itself.".to_string(),
                "spring.docker.compose.enabled=false".to_string(),
            ],
            ..Change::default()
        };
        let desired = contribution(&owner(), &change, &project().1).unwrap();
        assert_eq!(
            desired.resources.len(),
            1,
            "the comment is not its own resource"
        );
        let ResourceValue::Property(setting) = &desired.resources[0].value else {
            panic!("expected a property");
        };
        assert_eq!(setting.value, "false");
        assert_eq!(
            setting.comment,
            ["jails starts compose itself.".to_string()]
        );
    }

    #[test]
    fn a_trailing_comment_documents_nothing_and_is_refused() {
        let change = Change {
            properties: vec!["a.b=one".to_string(), "# and then?".to_string()],
            ..Change::default()
        };
        let message = contribution(&owner(), &change, &project().1).unwrap_err();
        assert!(message.contains("documents nothing"), "{message}");
    }

    #[test]
    fn a_property_line_with_no_value_names_nothing_to_own() {
        let change = Change {
            properties: vec!["spring.docker.compose.enabled".to_string()],
            ..Change::default()
        };
        let message = contribution(&owner(), &change, &project().1).unwrap_err();
        assert!(message.contains("key=value"), "{message}");
    }

    /// §R6.3's `add::test_wiring` row, which used to be a refusal here.
    ///
    /// One claim per `@SpringBootTest` in the project, keyed by that file.
    /// The class the capability is *adding* is not one of them: its Javadoc
    /// shows how to import it, and a text scan that read the example as a
    /// declaration would have the config import itself.
    #[test]
    fn a_spring_test_import_is_claimed_once_per_test_it_edits() {
        let (scratch, project) = project();
        jails_support::apply::put(
            scratch
                .path()
                .join("src/test/java/com/example/DemoApplicationTests.java"),
            "package com.example;\n\n@SpringBootTest\nclass DemoApplicationTests {}\n",
        )
        .unwrap();
        jails_support::apply::put(
            scratch
                .path()
                .join("src/test/java/com/example/TestcontainersConfig.java"),
            "package com.example;\n\n/** Use it as {@code @SpringBootTest} plus @Import. */\n\
             class TestcontainersConfig {}\n",
        )
        .unwrap();
        let change = Change {
            spring_test_import: Some(jails_project::model::SpringTestImport {
                pkg: "com.example".to_string(),
                class: "TestcontainersConfig",
            }),
            ..Change::default()
        };

        let desired = contribution(&owner(), &change, &project).unwrap();

        let claimed: Vec<String> = desired
            .resources
            .iter()
            .filter_map(|resource| match &resource.key {
                ResourceKey::SpringTestImport { path, .. } => Some(path.to_string()),
                _ => None,
            })
            .collect();
        assert_eq!(
            claimed,
            vec!["src/test/java/com/example/DemoApplicationTests.java".to_string()],
            "the Javadoc example is not a declaration"
        );
        assert_eq!(
            desired
                .edits
                .iter()
                .filter(|edit| matches!(edit, SemanticEdit::SpringTestImport { .. }))
                .count(),
            1
        );
    }

    /// A test in another package needs the import statement as well as the
    /// annotation; one in the same package must not get a self-import, which
    /// is what keeps a flat project compiling.
    #[test]
    fn the_import_statement_is_only_rendered_when_the_packages_differ() {
        let (scratch, project) = project();
        jails_support::apply::put(
            scratch
                .path()
                .join("src/test/java/com/example/web/RoutesTest.java"),
            "package com.example.web;\n\n@SpringBootTest\nclass RoutesTest {}\n",
        )
        .unwrap();
        let change = Change {
            spring_test_import: Some(jails_project::model::SpringTestImport {
                pkg: "com.example".to_string(),
                class: "TestcontainersConfig",
            }),
            ..Change::default()
        };

        let desired = contribution(&owner(), &change, &project).unwrap();

        let statement = desired.edits.iter().find_map(|edit| match edit {
            SemanticEdit::SpringTestImport { statement, .. } => Some(statement.clone()),
            _ => None,
        });
        assert_eq!(
            statement,
            Some("import com.example.TestcontainersConfig;\n".to_string())
        );
    }

    #[test]
    fn a_superseded_dependency_is_claimed_but_never_written() {
        let change = Change {
            legacy_deps: vec![dependency()],
            ..Change::default()
        };
        let desired = contribution(&owner(), &change, &project().1).unwrap();
        assert!(
            desired.resources.is_empty() && desired.edits.is_empty(),
            "nothing installs a superseded artifact"
        );
        assert_eq!(desired.adopted.len(), 1, "but removal has to cover it");
    }

    #[test]
    fn installing_and_superseding_one_artifact_is_refused() {
        let change = Change {
            deps: vec![dependency()],
            legacy_deps: vec![dependency()],
            ..Change::default()
        };
        let message = contribution(&owner(), &change, &project().1).unwrap_err();
        assert!(message.contains("installs and supersedes"), "{message}");
    }

    #[test]
    fn two_values_for_one_key_are_refused() {
        let change = Change {
            properties: vec!["a.b=one".to_string(), "a.b=two".to_string()],
            ..Change::default()
        };
        let message = contribution(&owner(), &change, &project().1).unwrap_err();
        assert!(message.contains("two different values"), "{message}");
    }
}
