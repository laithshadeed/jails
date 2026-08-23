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
//! Two contributions have no home in the closed protocol yet: a Spring test
//! import, and the legacy dependencies a capability supersedes. Both are
//! *refused by name* rather than dropped. A translator that silently loses a
//! contribution produces a desired state that is missing something no test
//! asked about — and the first symptom is a project that compiles and does not
//! start, which is exactly the failure the whole ownership model exists to
//! stop. §R6.3's row for `add::test_wiring` is where the missing variant is
//! designed; until it exists, this says so.
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
use jails_protocol::edit::SemanticEdit;
use jails_protocol::identity::{
    ManagedVersion, MarkerId, ProjectPath, PropertyKey, ServiceName, VolumeName,
};
use jails_protocol::render::{DesiredBody, DesiredFile};
use jails_protocol::resource::{
    CanonicalPluginXml, CanonicalYamlMapping, ComposeServiceSpec, DependencySpec, DesiredResource,
    MavenCoordinate, MavenScope, MavenVersion, PluginSpec, ResourceKey, ResourceOwner,
    ResourceValue,
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
    refuse_untranslated(change)?;
    let mut desired = DesiredChange::owned_by(owner.clone());
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
    for line in &change.properties {
        let (key, value) = property_resource(line)?;
        claim(&mut desired, owner, key.clone(), value.clone())?;
        let ResourceValue::Property(text) = value else {
            unreachable!("property_resource returns a property value");
        };
        desired
            .edits
            .push(SemanticEdit::Property { key, value: text });
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
        });
    }
    Ok(desired)
}

/// A contribution this translation cannot yet state, named rather than lost.
fn refuse_untranslated(change: &Change) -> Result<()> {
    if let Some(import) = &change.spring_test_import {
        return Err(format!(
            "this capability contributes the Spring test import {}, and the protocol has no \
             semantic edit for it yet (plan.md §R6.3, the `add::test_wiring` row).\n       \
             fix: keep this capability on the V1 path until that edit exists. Dropping the \
             import would leave every `@SpringBootTest` in the project without a DataSource, \
             which fails at run time and not at plan time.",
            import.fqcn()
        ));
    }
    if let Some(dependency) = change.legacy_deps.first() {
        return Err(format!(
            "this capability supersedes {}:{}, and a superseded dependency is an absence this \
             translation does not yet express.\n       \
             fix: keep this capability on the V1 path. Claiming the new dependency without \
             retiring the old one leaves both on the classpath, which is the two-Jackson-majors \
             failure jails' own doctor reports.",
            dependency.group_id, dependency.artifact_id
        ));
    }
    Ok(())
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

fn property_resource(line: &str) -> Result<(ResourceKey, ResourceValue)> {
    if line.trim_start().starts_with('#') {
        return Err(format!(
            "this capability's properties include the comment `{}`, and a per-key property \
             resource has nowhere to carry it.\n       \
             fix: keep this capability on the V1 path until a comment is part of what a property \
             owner states. Dropping it would delete prose written for the reader of a file jails \
             does not own, which is the opposite of what marked blocks are for.",
            line.trim()
        ));
    }
    let (key, value) = line.split_once('=').ok_or_else(|| {
        format!("`{line}` is not a `key=value` property line, so it names nothing to own")
    })?;
    Ok((
        ResourceKey::Property {
            path: ProjectPath::parse(APPLICATION_PROPERTIES)?,
            key: PropertyKey::parse(key.trim())?,
        },
        ResourceValue::Property(value.to_string()),
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
    fn a_property_line_with_no_value_names_nothing_to_own() {
        let change = Change {
            properties: vec!["spring.docker.compose.enabled".to_string()],
            ..Change::default()
        };
        let message = contribution(&owner(), &change, &project().1).unwrap_err();
        assert!(message.contains("key=value"), "{message}");
    }

    #[test]
    fn a_spring_test_import_is_refused_by_name_rather_than_dropped() {
        let change = Change {
            spring_test_import: Some(jails_project::model::SpringTestImport {
                pkg: "com.example".to_string(),
                class: "TestcontainersConfig",
            }),
            ..Change::default()
        };
        let message = contribution(&owner(), &change, &project().1).unwrap_err();
        assert!(
            message.contains("com.example.TestcontainersConfig"),
            "{message}"
        );
        assert!(message.contains("fix:"), "{message}");
    }

    #[test]
    fn a_superseded_dependency_is_refused_by_name_rather_than_dropped() {
        let change = Change {
            legacy_deps: vec![dependency()],
            ..Change::default()
        };
        let message = contribution(&owner(), &change, &project().1).unwrap_err();
        assert!(message.contains("org.postgresql:postgresql"), "{message}");
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
