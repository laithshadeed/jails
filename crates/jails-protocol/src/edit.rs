//! Keyed changes *inside* files the reader owns, and what must be true for
//! them to apply.
//!
//! Every edit carries its [`ResourceKey`] because that is what makes it
//! removable: the format owner renders the value, and `remove` finds it again
//! by key rather than by searching the file for text jails once wrote. That is
//! the whole reason `codemod` and `pom` exist, expressed as a value.

use crate::Result;
use crate::coordinate::{DependencySpec, PluginSpec};
use crate::entity::{CapabilityId, CapabilitySpec};
use crate::fact::{ProjectFact, ProjectFactKey};
use crate::identity::JavaType;
use crate::resource::{ComposeServiceSpec, PropertySetting, ResourceKey};
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
        value: PropertySetting,
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
    /// Import one `@TestConfiguration` into one `@SpringBootTest`.
    ///
    /// `add db` cannot skip this and cannot state it as a whole file: the
    /// classes it edits are the reader's, and one of them is the
    /// `contextLoads` test that shipped with the project. Once
    /// `spring-boot-starter-jdbc` is in the POM, auto-configuration demands a
    /// `DataSource` for every `@SpringBootTest`, so a capability that adds
    /// the dependency and walks away breaks a test nobody wrote.
    ///
    /// One edit per target file, keyed by that file, because each is an
    /// independent claim: a test written later gets its own row rather than
    /// being silently covered by a claim made about a file it is not in.
    SpringTestImport {
        key: ResourceKey,
        class: JavaType,
        /// The `import` statement the annotation needs when the config lives
        /// in another package, already rendered. Empty when it does not.
        statement: String,
    },
    /// The one edit with no resource key: a layout line names a layer, and the
    /// layer *is* the key. `directory` is one validated relative component.
    HumanConfigLayout {
        layer: Layer,
        directory: String,
    },
    /// Take a resource back out of the file that holds it.
    ///
    /// The inverse of whichever edit installed it, and one variant rather than
    /// eight because the *key* already says which file and which element: a
    /// dependency is unspliced from the POM, a property line is removed, a
    /// compose service and its marked block come out, a capability leaves the
    /// manifest. Removal carries no value, and that is the point -- what to
    /// take out is decided by identity, not by comparing the bytes that are
    /// there against the bytes jails would have written. A reader who edited
    /// the line still gets it removed, because they asked for the thing that
    /// owns it to go.
    Retire {
        key: ResourceKey,
    },
    /// Point the packaged jar at the class this change generated.
    ///
    /// `generate cli` writes a second dispatcher, and a project with two
    /// `main` methods still starts whichever one the POM names -- so a
    /// manifest that generated a CLI and registered its commands produced a
    /// jar answering only `help`. V1 did this with a `std::fs` write after the
    /// plan, which is exactly the shape this protocol removes: the routes did
    /// not know the POM had moved, so nothing recorded it and nothing could
    /// put it back.
    MavenMainClass {
        key: ResourceKey,
        class: JavaType,
        /// The entry point this displaces, so retiring the claim restores it.
        previous: JavaType,
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
            Self::Retire { .. } => 8,
            Self::SpringTestImport { .. } => 9,
            Self::MavenMainClass { .. } => 10,
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
            | Self::HumanConfigCapability { key, .. }
            | Self::SpringTestImport { key, .. }
            | Self::MavenMainClass { key, .. }
            | Self::Retire { key } => Some(key),
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
            Self::SpringTestImport { .. } => 8,
            Self::MavenMainClass { .. } => 9,
            Self::HumanConfigLayout { directory, .. } => {
                return validate_layout_directory(directory);
            }
            // Any key at all: a retirement is filed under the resource it
            // removes, whatever kind that is.
            Self::Retire { .. } => return Ok(()),
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
                value.encode(encoder)
            }
            Self::MarkedBlock { key, body } => {
                key.encode(encoder)?;
                encoder.string(body)
            }
            Self::Retire { key } => key.encode(encoder),
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
            Self::SpringTestImport {
                key,
                class,
                statement,
            } => {
                key.encode(encoder)?;
                class.encode(encoder)?;
                encoder.string(statement)
            }
            Self::MavenMainClass {
                key,
                class,
                previous,
            } => {
                key.encode(encoder)?;
                class.encode(encoder)?;
                previous.encode(encoder)
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
                value: PropertySetting::decode(decoder)?,
            },
            8 => Self::Retire {
                key: ResourceKey::decode(decoder)?,
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
            9 => Self::SpringTestImport {
                key: ResourceKey::decode(decoder)?,
                class: JavaType::decode(decoder)?,
                statement: decoder.string()?,
            },
            10 => Self::MavenMainClass {
                key: ResourceKey::decode(decoder)?,
                class: JavaType::decode(decoder)?,
                previous: JavaType::decode(decoder)?,
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
    use crate::coordinate::MavenCoordinate;

    #[test]
    fn an_edit_filed_under_the_wrong_kind_of_key_is_refused() {
        let edit = SemanticEdit::Property {
            key: ResourceKey::MavenDependency(
                MavenCoordinate::parse("org.postgresql", "postgresql").unwrap(),
            ),
            value: PropertySetting::plain("x"),
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
