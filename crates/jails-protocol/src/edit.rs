//! Keyed changes *inside* files the reader owns, and what must be true for
//! them to apply.
//!
//! Every edit carries its [`ResourceKey`] because that is what makes it
//! removable: the format owner renders the value, and `remove` finds it again
//! by key rather than by searching the file for text jails once wrote. That is
//! the whole reason `codemod` and `pom` exist, expressed as a value.

use crate::Result;
use crate::entity::{CapabilityId, CapabilitySpec};
use crate::fact::{ProjectFact, ProjectFactKey};
use crate::identity::JavaType;
use crate::resource::{ComposeServiceSpec, DependencySpec, PluginSpec, ResourceKey};
use jails_spec::spec::layout::Layer;
use jails_support::codec::{Decoder, Encoder, ordered};
use std::collections::{BTreeMap, BTreeSet};

// ---------------------------------------------------------------------------
// Semantic edits
// ---------------------------------------------------------------------------

/// A keyed change inside a file the reader owns.
///
/// Every variant carries its [`ResourceKey`] because that is what makes the
/// edit *removable*: the format owner renders the value, and `remove` finds it
/// again by key rather than by searching the file for text jails once wrote.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SemanticEdit {
    MavenDependency {
        key: ResourceKey,
        value: DependencySpec,
    },
    MavenPlugin {
        key: ResourceKey,
        value: PluginSpec,
    },
    ComposeService {
        key: ResourceKey,
        value: ComposeServiceSpec,
    },
    Property {
        key: ResourceKey,
        value: String,
    },
    MarkedBlock {
        key: ResourceKey,
        body: String,
    },
    CommandRegistration {
        key: ResourceKey,
        command: JavaType,
    },
    HumanConfigCapability {
        key: ResourceKey,
        spec: CapabilitySpec,
    },
    /// The one edit with no resource key: a layout line names a layer, and the
    /// layer *is* the key. `directory` is one validated relative component.
    HumanConfigLayout {
        layer: Layer,
        directory: String,
    },
}

impl SemanticEdit {
    fn tag(&self) -> u8 {
        match self {
            Self::MavenDependency { .. } => 0,
            Self::MavenPlugin { .. } => 1,
            Self::ComposeService { .. } => 2,
            Self::Property { .. } => 3,
            Self::MarkedBlock { .. } => 4,
            Self::CommandRegistration { .. } => 5,
            Self::HumanConfigCapability { .. } => 6,
            Self::HumanConfigLayout { .. } => 7,
        }
    }

    /// The resource this edit claims, if it claims one.
    pub fn key(&self) -> Option<&ResourceKey> {
        match self {
            Self::MavenDependency { key, .. }
            | Self::MavenPlugin { key, .. }
            | Self::ComposeService { key, .. }
            | Self::Property { key, .. }
            | Self::MarkedBlock { key, .. }
            | Self::CommandRegistration { key, .. }
            | Self::HumanConfigCapability { key, .. } => Some(key),
            Self::HumanConfigLayout { .. } => None,
        }
    }

    /// Refuses an edit filed under a key of another kind — the same check
    /// [`crate::resource::ResourceValue::agrees_with`] makes, applied where the
    /// value is an edit rather than a record.
    pub fn validate(&self) -> Result<()> {
        let expected = match self {
            Self::MavenDependency { .. } => 1,
            Self::MavenPlugin { .. } => 2,
            Self::ComposeService { .. } => 3,
            Self::Property { .. } => 4,
            Self::MarkedBlock { .. } => 5,
            Self::CommandRegistration { .. } => 6,
            Self::HumanConfigCapability { .. } => 7,
            Self::HumanConfigLayout { directory, .. } => {
                return validate_layout_directory(directory);
            }
        };
        let Some(key) = self.key() else {
            return Ok(());
        };
        if key.tag() != expected {
            return Err(format!(
                "semantic edit of kind {expected} filed under a resource key of kind {}",
                key.tag()
            ));
        }
        Ok(())
    }

    pub fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.validate()?;
        encoder.tag(self.tag());
        match self {
            Self::MavenDependency { key, value } => {
                key.encode(encoder)?;
                value.encode(encoder)
            }
            Self::MavenPlugin { key, value } => {
                key.encode(encoder)?;
                value.encode(encoder)
            }
            Self::ComposeService { key, value } => {
                key.encode(encoder)?;
                value.encode(encoder)
            }
            Self::Property { key, value } => {
                key.encode(encoder)?;
                encoder.string(value)
            }
            Self::MarkedBlock { key, body } => {
                key.encode(encoder)?;
                encoder.string(body)
            }
            Self::CommandRegistration { key, command } => {
                key.encode(encoder)?;
                command.encode(encoder)
            }
            Self::HumanConfigCapability { key, spec } => {
                key.encode(encoder)?;
                spec.encode(encoder)
            }
            Self::HumanConfigLayout { layer, directory } => {
                encoder.string(layer.package())?;
                encoder.string(directory)
            }
        }
    }

    pub fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        let edit = match decoder.tag()? {
            0 => Self::MavenDependency {
                key: ResourceKey::decode(decoder)?,
                value: DependencySpec::decode(decoder)?,
            },
            1 => Self::MavenPlugin {
                key: ResourceKey::decode(decoder)?,
                value: PluginSpec::decode(decoder)?,
            },
            2 => Self::ComposeService {
                key: ResourceKey::decode(decoder)?,
                value: ComposeServiceSpec::decode(decoder)?,
            },
            3 => Self::Property {
                key: ResourceKey::decode(decoder)?,
                value: decoder.string()?,
            },
            4 => Self::MarkedBlock {
                key: ResourceKey::decode(decoder)?,
                body: decoder.string()?,
            },
            5 => Self::CommandRegistration {
                key: ResourceKey::decode(decoder)?,
                command: JavaType::decode(decoder)?,
            },
            6 => Self::HumanConfigCapability {
                key: ResourceKey::decode(decoder)?,
                spec: CapabilitySpec::decode(decoder)?,
            },
            7 => Self::HumanConfigLayout {
                layer: Layer::by_package(&decoder.string()?)
                    .ok_or_else(|| "unknown layout layer".to_string())?,
                directory: decoder.string()?,
            },
            other => Err(format!("unknown semantic edit tag {other}"))?,
        };
        edit.validate()?;
        Ok(edit)
    }
}

/// A layout directory is one relative component, checked here because it lands
/// in a file a human reads and a path traversal there would be jails writing
/// outside the project on the reader's behalf.
fn validate_layout_directory(directory: &str) -> Result<()> {
    if directory.is_empty() {
        return Err("layout directory is empty".to_string());
    }
    for part in directory.split('.') {
        if part.is_empty() || part == "." || part == ".." || part.contains('/') {
            return Err(format!("`{directory}` is not a relative package component"));
        }
    }
    Ok(())
}

/// What must already be true for this change to be applicable.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SemanticPrecondition {
    RequiresCapability(CapabilityId),
    RequiresFact(ProjectFactKey),
    ResourceOwned(ResourceKey),
    ResourceUnclaimed(ResourceKey),
}

impl SemanticPrecondition {
    fn tag(&self) -> u8 {
        match self {
            Self::RequiresCapability(_) => 0,
            Self::RequiresFact(_) => 1,
            Self::ResourceOwned(_) => 2,
            Self::ResourceUnclaimed(_) => 3,
        }
    }

    pub fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.tag(self.tag());
        match self {
            Self::RequiresCapability(id) => id.encode(encoder),
            Self::RequiresFact(key) => key.encode(encoder),
            Self::ResourceOwned(key) | Self::ResourceUnclaimed(key) => key.encode(encoder),
        }
    }

    pub fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(match decoder.tag()? {
            0 => Self::RequiresCapability(CapabilityId::decode(decoder)?),
            1 => Self::RequiresFact(ProjectFactKey::decode(decoder)?),
            2 => Self::ResourceOwned(ResourceKey::decode(decoder)?),
            3 => Self::ResourceUnclaimed(ResourceKey::decode(decoder)?),
            other => Err(format!("unknown semantic precondition tag {other}"))?,
        })
    }
}

/// What this change teaches the fact map.
///
/// A delta rather than a new map: a change knows what it added and removed,
/// and nothing else, so a whole map would be a claim about facts it never
/// looked at.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FactDelta {
    pub add: BTreeMap<ProjectFactKey, ProjectFact>,
    pub remove: BTreeSet<ProjectFactKey>,
}

impl FactDelta {
    /// Adding and removing one key in one change is not two intentions, it is
    /// an ambiguity — the projection would depend on which half ran first.
    pub fn validate(&self) -> Result<()> {
        for (key, fact) in &self.add {
            fact.agrees_with(key)?;
            if self.remove.contains(key) {
                return Err(format!(
                    "fact delta both adds and removes {key:?}; the result would depend on order"
                ));
            }
        }
        Ok(())
    }

    pub fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.validate()?;
        crate::fact::encode_fact_map(encoder, &self.add)?;
        encoder.count(self.remove.len())?;
        let mut previous: Option<&ProjectFactKey> = None;
        for key in &self.remove {
            ordered(previous, key)?;
            previous = Some(key);
            key.encode(encoder)?;
        }
        Ok(())
    }

    pub fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        let add = crate::fact::decode_fact_map(decoder)?;
        let count = decoder.count()?;
        let mut remove = BTreeSet::new();
        let mut previous: Option<ProjectFactKey> = None;
        for _ in 0..count {
            let key = ProjectFactKey::decode(decoder)?;
            ordered(previous.as_ref(), &key)?;
            previous = Some(key.clone());
            remove.insert(key);
        }
        let delta = Self { add, remove };
        delta.validate()?;
        Ok(delta)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resource::MavenCoordinate;

    #[test]
    fn an_edit_filed_under_the_wrong_kind_of_key_is_refused() {
        let edit = SemanticEdit::Property {
            key: ResourceKey::MavenDependency(
                MavenCoordinate::parse("org.postgresql", "postgresql").unwrap(),
            ),
            value: "x".to_string(),
        };
        let error = edit.validate().unwrap_err();
        assert!(error.contains("filed under a resource key"), "{error}");
    }

    #[test]
    fn a_layout_edit_takes_one_relative_component() {
        for bad in ["", "../etc", "infra/jdbc", "a..b"] {
            assert!(
                SemanticEdit::HumanConfigLayout {
                    layer: Layer::Adapters,
                    directory: bad.to_string(),
                }
                .validate()
                .is_err(),
                "accepted {bad:?}"
            );
        }
        assert!(
            SemanticEdit::HumanConfigLayout {
                layer: Layer::Adapters,
                directory: "infra.jdbc".to_string(),
            }
            .validate()
            .is_ok(),
            "a nested package is a legal rename"
        );
    }

    /// Adding and removing one key in one change is not two intentions, it is
    /// an ambiguity: the projection would depend on which half ran first.
    #[test]
    fn a_fact_delta_that_both_adds_and_removes_a_key_is_refused() {
        let key = ProjectFactKey::ComposeService(
            crate::identity::ServiceName::parse("postgres").unwrap(),
        );
        let delta = FactDelta {
            add: BTreeMap::from([(
                key.clone(),
                ProjectFact::ComposeService(crate::resource::ComposeServiceSpec {
                    name: crate::identity::ServiceName::parse("postgres").unwrap(),
                    marker: crate::identity::MarkerId::parse("db").unwrap(),
                    mapping: crate::resource::CanonicalYamlMapping::parse("image: postgres:17\n")
                        .unwrap(),
                    volumes: BTreeSet::new(),
                }),
            )]),
            remove: BTreeSet::from([key]),
        };
        let error = delta.validate().unwrap_err();
        assert!(error.contains("depend on order"), "{error}");
    }
}
