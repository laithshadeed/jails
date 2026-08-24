//! Resources: the parts of a project that **more than one owner may claim**.
//!
//! A generated `.java` file has one owner and dies with it. A Maven
//! dependency does not: `add db` and `g repo` can both want
//! `spring-boot-starter-jdbc`, and removing one of them must not remove the
//! line. plan.md §R1.1 is explicit that a record therefore holds *enough value
//! to re-render the shared file after one owner leaves* — a hash would let
//! jails detect the loss and not repair it.
//!
//! ## Why the key and the value are separate types that must agree
//!
//! The key is what two owners collide on; the value is what gets written. They
//! carry the coordinate twice, so [`ResourceValue::agrees_with`] is called by
//! every constructor and every decoder — a record whose key says
//! `org.postgresql:postgresql` and whose value installs something else is not
//! a decode error anywhere else in the format, it is a silent substitution.
//!
//! ## Why the plugin block and the compose mapping stay opaque text
//!
//! §R1.1: *"children remain opaque because Maven plugin configuration is
//! intentionally open-ended. This is safer and simpler than a partial plugin
//! AST."* The same holds for a compose service body. What is validated is the
//! *envelope* — one element, no surrounding document, canonical line endings —
//! because that is what a splice depends on, and a partial parse that silently
//! dropped an unrecognised child would corrupt a file the reader owns.

use crate::Result;
use crate::coordinate::{DependencySpec, MavenCoordinate, PluginSpec};
use crate::entity::{CapabilitySpec, EntityId, OneShotId};
use crate::identity::{JavaType, MarkerId, ProjectPath, PropertyKey, ServiceName, VolumeName};
use jails_support::codec::{Decoder, Encoder, ordered};
use std::collections::BTreeSet;

// ---------------------------------------------------------------------------
// Compose
// ---------------------------------------------------------------------------

/// Exactly the mapping beneath one compose service: no `services:`, no
/// service-name key, no markers and no second top-level section.
///
/// The format owner supplies indentation and markers (`codemod::Marked`), so
/// what is stored here is indentation-relative and reusable — which is what
/// lets `add db` and `add kafka` stack in one file.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct CanonicalYamlMapping(String);

impl CanonicalYamlMapping {
    pub fn parse(text: &str) -> Result<Self> {
        if text.contains('\r') {
            return Err("compose mapping contains CR; canonical YAML is LF-only".to_string());
        }
        if text.contains("# jails:") || text.contains("# /jails:") {
            return Err(
                "compose mapping contains a jails marker; markers belong to the format owner"
                    .to_string(),
            );
        }
        if text.trim().is_empty() {
            return Err("compose mapping is empty".to_string());
        }
        if text.contains('\t') {
            return Err("compose mapping contains a tab; YAML indentation is spaces".to_string());
        }
        // The body's own keys sit at column zero: this value is stored
        // relative to the service, and the format owner indents it. So a
        // leading-space first line means someone stored the *indented* text,
        // which would be indented a second time on splice.
        let first = text
            .lines()
            .find(|line| !line.trim().is_empty())
            .unwrap_or_default();
        if first.starts_with(' ') {
            return Err(
                "compose mapping is indented; store it relative to the service".to_string(),
            );
        }
        if first.trim_start().starts_with("services:") {
            return Err("compose mapping includes the `services:` key".to_string());
        }
        Ok(Self(text.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.string(&self.0)
    }

    pub fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Self::parse(&decoder.string()?)
    }
}

/// One managed compose service.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct ComposeServiceSpec {
    pub name: ServiceName,
    pub marker: MarkerId,
    pub mapping: CanonicalYamlMapping,
    pub volumes: BTreeSet<VolumeName>,
}

impl ComposeServiceSpec {
    pub fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.name.encode(encoder)?;
        self.marker.encode(encoder)?;
        self.mapping.encode(encoder)?;
        encoder.count(self.volumes.len())?;
        let mut previous: Option<&VolumeName> = None;
        for volume in &self.volumes {
            ordered(previous, volume)?;
            previous = Some(volume);
            volume.encode(encoder)?;
        }
        Ok(())
    }

    pub fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        let name = ServiceName::decode(decoder)?;
        let marker = MarkerId::decode(decoder)?;
        let mapping = CanonicalYamlMapping::decode(decoder)?;
        let count = decoder.count()?;
        let mut volumes = BTreeSet::new();
        let mut previous: Option<VolumeName> = None;
        for _ in 0..count {
            let volume = VolumeName::decode(decoder)?;
            ordered(previous.as_ref(), &volume)?;
            previous = Some(volume.clone());
            volumes.insert(volume);
        }
        Ok(Self {
            name,
            marker,
            mapping,
            volumes,
        })
    }
}

// ---------------------------------------------------------------------------
// Keys, values and owners
// ---------------------------------------------------------------------------

/// What two owners collide on.
///
/// Declaration order is tag order (§R1.4's table), and the derived ordering is
/// therefore the wire ordering — which is what lets a set of keys be encoded
/// canonically without a second sort rule.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub enum ResourceKey {
    WholeFile(ProjectPath),
    MavenDependency(MavenCoordinate),
    MavenPlugin(MavenCoordinate),
    ComposeService(ServiceName),
    Property {
        path: ProjectPath,
        key: PropertyKey,
    },
    MarkedBlock {
        path: ProjectPath,
        marker: MarkerId,
    },
    CommandRegistration {
        dispatcher: JavaType,
        command: JavaType,
    },
    HumanConfigCapability(crate::entity::CapabilityId),
    /// One `@TestConfiguration` imported into one `@SpringBootTest`.
    ///
    /// Keyed by the file *and* the class, because the same capability imports
    /// the same config into every such test in the project and each of those
    /// is an independent claim: a test added later gets its own row, and a
    /// second capability importing a different config into the same file does
    /// not collide with this one.
    SpringTestImport {
        path: ProjectPath,
        class: JavaType,
    },
    /// The `<mainClass>` a build file declares.
    ///
    /// Keyed by the build file, because a project has exactly one packaged
    /// entry point per POM and two claims on it are a collision the reader
    /// has to see -- not a last-writer-wins that decides in silence which of
    /// two `main` methods the jar starts.
    MavenMainClass(ProjectPath),
}

impl ResourceKey {
    pub fn tag(&self) -> u8 {
        match self {
            Self::WholeFile(_) => 0,
            Self::MavenDependency(_) => 1,
            Self::MavenPlugin(_) => 2,
            Self::ComposeService(_) => 3,
            Self::Property { .. } => 4,
            Self::MarkedBlock { .. } => 5,
            Self::CommandRegistration { .. } => 6,
            Self::HumanConfigCapability(_) => 7,
            Self::SpringTestImport { .. } => 8,
            Self::MavenMainClass(_) => 9,
        }
    }

    pub fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.tag(self.tag());
        match self {
            Self::WholeFile(path) => path.encode(encoder),
            Self::MavenDependency(coordinate) | Self::MavenPlugin(coordinate) => {
                coordinate.encode(encoder)
            }
            Self::ComposeService(name) => name.encode(encoder),
            Self::Property { path, key } => {
                path.encode(encoder)?;
                key.encode(encoder)
            }
            Self::MarkedBlock { path, marker } => {
                path.encode(encoder)?;
                marker.encode(encoder)
            }
            Self::CommandRegistration {
                dispatcher,
                command,
            } => {
                dispatcher.encode(encoder)?;
                command.encode(encoder)
            }
            Self::HumanConfigCapability(id) => id.encode(encoder),
            Self::SpringTestImport { path, class } => {
                path.encode(encoder)?;
                class.encode(encoder)
            }
            Self::MavenMainClass(path) => path.encode(encoder),
        }
    }

    pub fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(match decoder.tag()? {
            0 => Self::WholeFile(ProjectPath::decode(decoder)?),
            1 => Self::MavenDependency(MavenCoordinate::decode(decoder)?),
            2 => Self::MavenPlugin(MavenCoordinate::decode(decoder)?),
            3 => Self::ComposeService(ServiceName::decode(decoder)?),
            4 => Self::Property {
                path: ProjectPath::decode(decoder)?,
                key: PropertyKey::decode(decoder)?,
            },
            5 => Self::MarkedBlock {
                path: ProjectPath::decode(decoder)?,
                marker: MarkerId::decode(decoder)?,
            },
            6 => Self::CommandRegistration {
                dispatcher: JavaType::decode(decoder)?,
                command: JavaType::decode(decoder)?,
            },
            7 => Self::HumanConfigCapability(crate::entity::CapabilityId::decode(decoder)?),
            8 => Self::SpringTestImport {
                path: ProjectPath::decode(decoder)?,
                class: JavaType::decode(decoder)?,
            },
            9 => Self::MavenMainClass(ProjectPath::decode(decoder)?),
            other => Err(format!("unknown resource key tag {other}"))?,
        })
    }
}

/// A property's value, and the prose that introduces it.
///
/// A capability's properties are not only settings: `add db` writes
/// `spring.docker.compose.enabled=false` with a line saying jails starts
/// compose itself, and `add cors` writes one saying never to pair `*` with
/// credentials. That prose is written for the reader of a file jails does not
/// own, and a per-key property resource that carried only the value would
/// delete it — which is the opposite of what a marked block was for.
///
/// The comment is stated without its `#`: one value, one spelling. The format
/// owner renders the marker, and renders it *only when the key is introduced*,
/// because prose somebody may have edited is not jails' to rewrite on every
/// reconcile.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PropertySetting {
    pub value: String,
    pub comment: Vec<String>,
}

impl PropertySetting {
    pub fn new(value: impl Into<String>, comment: Vec<String>) -> Result<Self> {
        for line in &comment {
            if line.contains('\n') || line.contains('\r') {
                return Err(
                    "a property comment line contains a newline; one line is one line".to_string(),
                );
            }
            if line.trim_start().starts_with('#') {
                return Err(format!(
                    "the property comment `{line}` carries its own `#`; the marker belongs to the \
                     format owner, so one comment has one spelling"
                ));
            }
        }
        Ok(Self {
            value: value.into(),
            comment,
        })
    }

    /// A setting with nothing to explain.
    pub fn plain(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            comment: Vec::new(),
        }
    }

    pub fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.string(&self.value)?;
        encoder.count(self.comment.len())?;
        for line in &self.comment {
            encoder.string(line)?;
        }
        Ok(())
    }

    pub fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        let value = decoder.string()?;
        let count = decoder.count()?;
        let mut comment = Vec::new();
        for _ in 0..count {
            comment.push(decoder.string()?);
        }
        Self::new(value, comment)
    }
}

/// What gets written for a resource. Same tags as [`ResourceKey`], and the two
/// must agree — see [`ResourceValue::agrees_with`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ResourceValue {
    /// A whole file's content is the file, not the record: the record only
    /// says the path is claimed.
    WholeFile,
    MavenDependency(DependencySpec),
    MavenPlugin(PluginSpec),
    ComposeService(ComposeServiceSpec),
    Property(PropertySetting),
    MarkedBlock(String),
    CommandRegistration {
        command: JavaType,
    },
    HumanConfigCapability(CapabilitySpec),
    SpringTestImport {
        class: JavaType,
        /// The `import` statement the annotation needs when the config lives
        /// in another package, already rendered. Empty when it does not.
        statement: String,
    },
    /// The entry point this claim installs, and the one it displaced.
    ///
    /// `previous` is what makes the claim reversible. There is no way to
    /// derive an entry point's predecessor from a POM that no longer names it,
    /// and a retirement that guessed -- deleting the element, or writing back
    /// whatever `App` a project happens to have -- would leave the jar
    /// starting a class nobody chose.
    MavenMainClass {
        class: JavaType,
        previous: JavaType,
    },
}

impl ResourceValue {
    pub fn tag(&self) -> u8 {
        match self {
            Self::WholeFile => 0,
            Self::MavenDependency(_) => 1,
            Self::MavenPlugin(_) => 2,
            Self::ComposeService(_) => 3,
            Self::Property(_) => 4,
            Self::MarkedBlock(_) => 5,
            Self::CommandRegistration { .. } => 6,
            Self::HumanConfigCapability(_) => 7,
            Self::SpringTestImport { .. } => 8,
            Self::MavenMainClass { .. } => 9,
        }
    }

    /// The coordinate is recorded twice — once as the thing owners collide on,
    /// once as the thing that gets written. This is the only place that checks
    /// they say the same thing, and every constructor and decoder calls it.
    pub fn agrees_with(&self, key: &ResourceKey) -> Result<()> {
        if self.tag() != key.tag() {
            return Err(format!(
                "resource value kind {} does not match key kind {}",
                self.tag(),
                key.tag()
            ));
        }
        match (key, self) {
            (ResourceKey::MavenDependency(coordinate), Self::MavenDependency(spec))
                if spec.coordinate != *coordinate =>
            {
                Err(format!(
                    "dependency {} recorded under key {coordinate}",
                    spec.coordinate
                ))
            }
            (ResourceKey::MavenPlugin(coordinate), Self::MavenPlugin(spec))
                if spec.coordinate != *coordinate =>
            {
                Err(format!(
                    "plugin {} recorded under key {coordinate}",
                    spec.coordinate
                ))
            }
            (ResourceKey::ComposeService(name), Self::ComposeService(spec))
                if spec.name != *name =>
            {
                Err(format!(
                    "compose service {} recorded under key {name}",
                    spec.name
                ))
            }
            (
                ResourceKey::CommandRegistration { command: keyed, .. },
                Self::CommandRegistration { command },
            ) if command != keyed => Err(format!(
                "command registration {command} recorded under key {keyed}"
            )),
            (
                ResourceKey::SpringTestImport { class: keyed, .. },
                Self::SpringTestImport { class, .. },
            ) if class != keyed => Err(format!("test import {class} recorded under key {keyed}")),
            _ => Ok(()),
        }
    }

    pub fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.tag(self.tag());
        match self {
            Self::WholeFile => Ok(()),
            Self::MavenDependency(spec) => spec.encode(encoder),
            Self::MavenPlugin(spec) => spec.encode(encoder),
            Self::ComposeService(spec) => spec.encode(encoder),
            Self::Property(setting) => setting.encode(encoder),
            Self::MarkedBlock(value) => encoder.string(value),
            Self::CommandRegistration { command } => command.encode(encoder),
            Self::HumanConfigCapability(spec) => spec.encode(encoder),
            Self::SpringTestImport { class, statement } => {
                class.encode(encoder)?;
                encoder.string(statement)
            }
            Self::MavenMainClass { class, previous } => {
                class.encode(encoder)?;
                previous.encode(encoder)
            }
        }
    }

    pub fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(match decoder.tag()? {
            0 => Self::WholeFile,
            1 => Self::MavenDependency(DependencySpec::decode(decoder)?),
            2 => Self::MavenPlugin(PluginSpec::decode(decoder)?),
            3 => Self::ComposeService(ComposeServiceSpec::decode(decoder)?),
            4 => Self::Property(PropertySetting::decode(decoder)?),
            5 => Self::MarkedBlock(decoder.string()?),
            6 => Self::CommandRegistration {
                command: JavaType::decode(decoder)?,
            },
            7 => Self::HumanConfigCapability(CapabilitySpec::decode(decoder)?),
            8 => Self::SpringTestImport {
                class: JavaType::decode(decoder)?,
                statement: decoder.string()?,
            },
            9 => Self::MavenMainClass {
                class: JavaType::decode(decoder)?,
                previous: JavaType::decode(decoder)?,
            },
            other => Err(format!("unknown resource value tag {other}"))?,
        })
    }
}

/// Who claims a resource. An entity or a one-shot — never a human owner, since
/// a human declares an *entity* and the entity claims the resource.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub enum ResourceOwner {
    Entity(EntityId),
    OneShot(OneShotId),
}

impl ResourceOwner {
    pub fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        match self {
            Self::Entity(id) => {
                encoder.tag(0);
                id.encode(encoder)
            }
            Self::OneShot(id) => {
                encoder.tag(1);
                id.encode(encoder)
            }
        }
    }

    pub fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(match decoder.tag()? {
            0 => Self::Entity(EntityId::decode(decoder)?),
            1 => Self::OneShot(OneShotId::decode(decoder)?),
            other => Err(format!("unknown resource owner tag {other}"))?,
        })
    }
}

pub fn encode_owners(encoder: &mut Encoder, owners: &BTreeSet<ResourceOwner>) -> Result<()> {
    encoder.count(owners.len())?;
    let mut previous: Option<&ResourceOwner> = None;
    for owner in owners {
        ordered(previous, owner)?;
        previous = Some(owner);
        owner.encode(encoder)?;
    }
    Ok(())
}

pub fn decode_owners(decoder: &mut Decoder<'_>) -> Result<BTreeSet<ResourceOwner>> {
    let count = decoder.count()?;
    let mut owners = BTreeSet::new();
    let mut previous: Option<ResourceOwner> = None;
    for _ in 0..count {
        let owner = ResourceOwner::decode(decoder)?;
        ordered(previous.as_ref(), &owner)?;
        previous = Some(owner.clone());
        owners.insert(owner);
    }
    Ok(owners)
}

/// A resource as it is to be, with the complete owner set.
///
/// The owner set is complete on purpose: removing one owner from a two-owner
/// resource leaves the resource, and the only way to say that in one value is
/// to record the set rather than a delta.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DesiredResource {
    pub key: ResourceKey,
    pub owners: BTreeSet<ResourceOwner>,
    pub value: ResourceValue,
}

impl DesiredResource {
    pub fn new(
        key: ResourceKey,
        owners: BTreeSet<ResourceOwner>,
        value: ResourceValue,
    ) -> Result<Self> {
        value.agrees_with(&key)?;
        if owners.is_empty() {
            return Err(format!(
                "desired resource {key:?} has no owner; an unowned claim is an absence, not a \
                 resource"
            ));
        }
        Ok(Self { key, owners, value })
    }

    pub fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.key.encode(encoder)?;
        encode_owners(encoder, &self.owners)?;
        self.value.encode(encoder)
    }

    pub fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        let key = ResourceKey::decode(decoder)?;
        let owners = decode_owners(decoder)?;
        let value = ResourceValue::decode(decoder)?;
        Self::new(key, owners, value)
    }
}

/// The same thing as a planner sees it: value plus owners, keyed elsewhere.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectedResource {
    pub value: ResourceValue,
    pub owners: BTreeSet<ResourceOwner>,
}

/// A recorded resource — identical shape to [`DesiredResource`], and separate
/// because one is what someone wants and the other is what the store says.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResourceRecord {
    pub key: ResourceKey,
    pub owners: BTreeSet<ResourceOwner>,
    pub value: ResourceValue,
}

// ---------------------------------------------------------------------------
// One-shot bookkeeping
// ---------------------------------------------------------------------------

/// Whether a one-shot's target still exists.
///
/// A retired one-shot is *kept*: a migration that has been applied to a
/// database cannot be un-applied by deleting its record, and a receipt that
/// vanished when its target did would make the same `g field` run twice.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OneShotState {
    Active,
    RetiredTargetRemoved,
}

impl OneShotState {
    fn tag(self) -> u8 {
        match self {
            Self::Active => 0,
            Self::RetiredTargetRemoved => 1,
        }
    }

    pub fn encode(self, encoder: &mut Encoder) {
        encoder.tag(self.tag());
    }

    pub fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        match decoder.tag()? {
            0 => Ok(Self::Active),
            1 => Ok(Self::RetiredTargetRemoved),
            other => Err(format!("unknown one-shot state tag {other}")),
        }
    }
}

/// How a one-shot relates to the resources it touched.
///
/// `Field` splits them because the two behave differently when the target goes
/// away: a target-coupled resource dies with the record it was added to, while
/// an append-only one (a migration file, a fixture) stays.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OneShotLifecycle {
    Field {
        target_coupled: BTreeSet<ResourceKey>,
        append_only: BTreeSet<ResourceKey>,
    },
    Migration,
    Cases,
}

impl OneShotLifecycle {
    fn tag(&self) -> u8 {
        match self {
            Self::Field { .. } => 0,
            Self::Migration => 1,
            Self::Cases => 2,
        }
    }

    pub fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.tag(self.tag());
        match self {
            Self::Field {
                target_coupled,
                append_only,
            } => {
                encode_keys(encoder, target_coupled)?;
                encode_keys(encoder, append_only)
            }
            Self::Migration | Self::Cases => Ok(()),
        }
    }

    pub fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(match decoder.tag()? {
            0 => Self::Field {
                target_coupled: decode_keys(decoder)?,
                append_only: decode_keys(decoder)?,
            },
            1 => Self::Migration,
            2 => Self::Cases,
            other => Err(format!("unknown one-shot lifecycle tag {other}"))?,
        })
    }
}

pub fn encode_keys(encoder: &mut Encoder, keys: &BTreeSet<ResourceKey>) -> Result<()> {
    encoder.count(keys.len())?;
    let mut previous: Option<&ResourceKey> = None;
    for key in keys {
        ordered(previous, key)?;
        previous = Some(key);
        key.encode(encoder)?;
    }
    Ok(())
}

pub fn decode_keys(decoder: &mut Decoder<'_>) -> Result<BTreeSet<ResourceKey>> {
    let count = decoder.count()?;
    let mut keys = BTreeSet::new();
    let mut previous: Option<ResourceKey> = None;
    for _ in 0..count {
        let key = ResourceKey::decode(decoder)?;
        ordered(previous.as_ref(), &key)?;
        previous = Some(key.clone());
        keys.insert(key);
    }
    Ok(keys)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::{IntentId, Recipe};
    use crate::identity::{Name, Package};

    fn coordinate(group: &str, artifact: &str) -> MavenCoordinate {
        MavenCoordinate::parse(group, artifact).unwrap()
    }

    fn owner(name: &str) -> ResourceOwner {
        ResourceOwner::Entity(EntityId::Intent(IntentId::new(
            Recipe::Record,
            Name::parse(name).unwrap(),
            Package::parse("com.example.demo.domain").unwrap(),
        )))
    }

    fn owners(names: &[&str]) -> BTreeSet<ResourceOwner> {
        names.iter().map(|name| owner(name)).collect()
    }

    fn round_trip(resource: &DesiredResource) -> DesiredResource {
        let mut encoder = Encoder::new();
        resource.encode(&mut encoder).unwrap();
        let bytes = encoder.finish().unwrap();
        let mut decoder = Decoder::new(&bytes).unwrap();
        let back = DesiredResource::decode(&mut decoder).unwrap();
        decoder.finish().unwrap();
        back
    }

    #[test]
    fn a_shared_dependency_round_trips_with_both_owners() {
        let key = ResourceKey::MavenDependency(coordinate(
            "org.springframework.boot",
            "spring-boot-starter-jdbc",
        ));
        let value = ResourceValue::MavenDependency(DependencySpec::managed(coordinate(
            "org.springframework.boot",
            "spring-boot-starter-jdbc",
        )));
        let resource = DesiredResource::new(key, owners(&["Note", "Invoice"]), value).unwrap();
        assert_eq!(round_trip(&resource), resource);
        assert_eq!(resource.owners.len(), 2);
    }

    /// The coordinate is recorded twice, so this is the check that stops a
    /// record installing something other than what its key claims.
    #[test]
    fn a_value_recorded_under_the_wrong_key_is_refused() {
        let key = ResourceKey::MavenDependency(coordinate("org.postgresql", "postgresql"));
        let value = ResourceValue::MavenDependency(DependencySpec::managed(coordinate(
            "com.h2database",
            "h2",
        )));
        let error = DesiredResource::new(key, owners(&["Note"]), value).unwrap_err();
        assert!(error.contains("recorded under key"), "{error}");
    }

    #[test]
    fn a_value_of_the_wrong_kind_is_refused() {
        let key = ResourceKey::MavenDependency(coordinate("org.postgresql", "postgresql"));
        let error = ResourceValue::WholeFile.agrees_with(&key).unwrap_err();
        assert!(error.contains("does not match key kind"), "{error}");
    }

    /// An unowned claim is an absence. Recording one would make the next
    /// reconcile see a resource nobody wants and keep it forever.
    #[test]
    fn a_resource_with_no_owner_is_refused() {
        let key = ResourceKey::ComposeService(ServiceName::parse("postgres").unwrap());
        let error = DesiredResource::new(key, BTreeSet::new(), ResourceValue::WholeFile);
        assert!(error.is_err());
    }

    /// The format owner supplies indentation and markers. A mapping that
    /// carried either would be indented or marked twice on splice.
    #[test]
    fn a_compose_mapping_carries_neither_indentation_nor_markers() {
        assert!(CanonicalYamlMapping::parse("  image: postgres:17\n").is_err());
        assert!(CanonicalYamlMapping::parse("# jails:db\nimage: postgres:17\n").is_err());
        assert!(CanonicalYamlMapping::parse("services:\n  db:\n").is_err());
        assert!(CanonicalYamlMapping::parse("image: postgres:17\nports:\n  - 5432:5432\n").is_ok());
    }

    /// Declaration order is tag order, and the derived ordering is therefore
    /// the wire ordering — which is what lets a set of keys encode canonically
    /// with no second sort rule.
    #[test]
    fn key_ordering_follows_tag_order() {
        let keys = [
            ResourceKey::WholeFile(ProjectPath::parse("pom.xml").unwrap()),
            ResourceKey::MavenDependency(coordinate("g", "a")),
            ResourceKey::MavenPlugin(coordinate("g", "a")),
            ResourceKey::ComposeService(ServiceName::parse("db").unwrap()),
        ];
        let mut sorted = keys.to_vec();
        sorted.sort();
        assert_eq!(sorted, keys);
        assert_eq!(
            keys.iter().map(ResourceKey::tag).collect::<Vec<_>>(),
            vec![0, 1, 2, 3]
        );
    }

    /// A set that arrives in another order is a second encoding of one value,
    /// and therefore a second identity for it.
    #[test]
    fn an_unsorted_owner_set_is_refused_on_decode() {
        let mut encoder = Encoder::new();
        encoder.count(2).unwrap();
        owner("Zulu").encode(&mut encoder).unwrap();
        owner("Alpha").encode(&mut encoder).unwrap();
        let bytes = encoder.finish().unwrap();
        let mut decoder = Decoder::new(&bytes).unwrap();
        assert!(decode_owners(&mut decoder).is_err());
    }

    #[test]
    fn one_shot_lifecycle_round_trips() {
        let lifecycle = OneShotLifecycle::Field {
            target_coupled: [ResourceKey::WholeFile(
                ProjectPath::parse("src/main/java/com/example/demo/domain/Note.java").unwrap(),
            )]
            .into_iter()
            .collect(),
            append_only: [ResourceKey::WholeFile(
                ProjectPath::parse("src/main/resources/db/migration/V2__add.sql").unwrap(),
            )]
            .into_iter()
            .collect(),
        };
        let mut encoder = Encoder::new();
        lifecycle.encode(&mut encoder).unwrap();
        let bytes = encoder.finish().unwrap();
        let mut decoder = Decoder::new(&bytes).unwrap();
        assert_eq!(OneShotLifecycle::decode(&mut decoder).unwrap(), lifecycle);
        decoder.finish().unwrap();
    }
}
