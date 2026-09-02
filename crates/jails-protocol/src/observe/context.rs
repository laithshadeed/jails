//! What a renderer was given, recorded exactly.
//!
//! ## Why this exists at all
//!
//! plan.md §R5 opens with the failure it fixes: today's reconciliation
//! regenerates the *old* side of a three-way merge using today's binary,
//! templates and context — so after a template change, that "old" side is not
//! necessarily bytes jails ever wrote, and the merge is against a file that
//! never existed. Recording the exact context alongside the exact base ends
//! that: reconciliation reads the stored base and never regenerates it.
//!
//! ## Why the context is a closed value and not a map
//!
//! §R5.2: *"there is no JSON/TOML peer and no renderer-supplied opaque map."*
//! An opaque map cannot be validated, so a renderer could record a context
//! that does not describe what it rendered — and the only symptom would be an
//! inexplicable diff years later. Every field here is checked against the
//! subject it claims.
//!
//! ## Why `relevant_inputs` is a hash of declared rows
//!
//! It explains a changed render — "this input moved" — without making every
//! file in the project part of a generated file's provenance. The renderer
//! declares which inputs it consumed *through the snapshot*, so a caller
//! cannot hand it a hash of inputs it never read.

use crate::Result;
use crate::entity::{EntityId, EntitySpec, OneShotId, OneShotSpec};
use crate::identity::{JavaType, ObjectId, Package};
use crate::provenance::RendererId;
use crate::render::TemplateBindings;
use crate::request::CanonicalCapability;
use crate::snapshot::InputPrecondition;
use jails_spec::spec::layout::Layer;
use jails_support::codec::{self, Codec, Decoder, Encoder, ordered};

/// The one context version.
pub(crate) const CONTEXT_SCHEMA: u32 = 1;

/// What a renderer was rendering.
#[derive(Clone, Debug, Eq, PartialEq, jails_codec_derive::Codec)]
pub enum RenderedSubjectContext {
    #[codec(tag = 0)]
    Entity { id: EntityId, spec: EntitySpec },
    #[codec(tag = 1)]
    OneShot { id: OneShotId, spec: OneShotSpec },
}

impl RenderedSubjectContext {
    /// Identity and spec must describe the same thing.
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::Entity { id, spec } if !spec.matches(id) => Err(jails_support::Failure::Told(
                "a renderer context pairs an entity identity and a spec of different kinds"
                    .to_string(),
            )),
            Self::OneShot { id, spec } if !spec.matches(id) => Err(jails_support::Failure::Told(
                "a renderer context pairs a one-shot identity and a spec that disagree".to_string(),
            )),
            _ => Ok(()),
        }
    }
}

/// Which side of a reference this is.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, jails_codec_derive::Codec)]
pub enum ReferenceRole {
    #[codec(tag = 0)]
    On,
    #[codec(tag = 1)]
    Yields,
}

/// One reference the renderer resolved.
///
/// The *resolved* qualified target and an optional managed identity — never a
/// source path. A path recorded here would make the context depend on where
/// the reader keeps their project.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, jails_codec_derive::Codec)]
pub struct ResolvedReferenceContext {
    pub role: ReferenceRole,
    pub target: JavaType,
    pub managed: Option<EntityId>,
}

/// One layer and the package it resolved to.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LayerContext {
    pub layer: Layer,
    pub package: Package,
}

/// Everything a renderer saw.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RendererContextV1 {
    pub renderer: RendererId,
    pub subject: Option<RenderedSubjectContext>,
    pub references: Vec<ResolvedReferenceContext>,
    pub base_package: Package,
    /// Exactly the eleven layers, in declaration order. Not a map: an omitted
    /// layer and a layer resolved to its default would encode the same.
    pub layers: Vec<LayerContext>,
    pub java_release: u32,
    pub capabilities: Vec<CanonicalCapability>,
    pub bindings: TemplateBindings,
}

impl RendererContextV1 {
    /// The checks §R5.2 spells out, in one place.
    pub fn validate(&self) -> Result<()> {
        if let Some(subject) = &self.subject {
            subject.validate()?;
        }
        // A `Format` renderer is an aggregate: it renders a file, not an
        // entity, and a subject would be a claim nothing supports.
        if matches!(self.renderer, RendererId::Format(_)) && self.subject.is_some() {
            return Err(jails_support::Failure::Told(
                "a format renderer has no subject; it renders a file, not an entity".to_string(),
            ));
        }
        if !matches!(self.renderer, RendererId::Format(_)) && self.subject.is_none() {
            return Err(format!(
                "renderer {:?} rendered something and recorded no subject",
                self.renderer
            )
            .into());
        }

        if self.layers.len() != Layer::ALL.len() {
            return Err(format!(
                "a renderer context carries {} layers; there are {}",
                self.layers.len(),
                Layer::ALL.len()
            )
            .into());
        }
        for (recorded, expected) in self.layers.iter().zip(Layer::ALL) {
            if recorded.layer != expected {
                return Err(format!(
                    "layer {:?} appears where {expected:?} belongs; the order is part of the \
                     encoding",
                    recorded.layer
                )
                .into());
            }
        }

        let mut previous: Option<&ResolvedReferenceContext> = None;
        for reference in &self.references {
            ordered(previous, reference)?;
            previous = Some(reference);
        }
        let mut previous: Option<&crate::entity::CapabilityId> = None;
        for capability in &self.capabilities {
            ordered(previous, &capability.id)?;
            previous = Some(&capability.id);
        }
        Ok(())
    }

    /// The exact bytes stored as `context_object`.
    pub fn to_object(&self) -> Result<Vec<u8>> {
        let mut encoder = Encoder::new();
        self.encode(&mut encoder)?;
        encoder.finish()
    }
}
impl Codec for RendererContextV1 {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.validate()?;
        encoder.u32(CONTEXT_SCHEMA);
        self.renderer.encode(encoder)?;
        encoder.option(self.subject.as_ref(), |e, subject| subject.encode(e))?;
        encoder.count(self.references.len())?;
        for reference in &self.references {
            reference.encode(encoder)?;
        }
        self.base_package.encode(encoder)?;
        encoder.count(self.layers.len())?;
        for layer in &self.layers {
            encoder.string(layer.layer.package())?;
            layer.package.encode(encoder)?;
        }
        encoder.u32(self.java_release);
        encoder.count(self.capabilities.len())?;
        for capability in &self.capabilities {
            capability.id.encode(encoder)?;
            capability.spec.encode(encoder)?;
        }
        self.bindings.encode(encoder)
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        let schema = decoder.u32()?;
        if schema != CONTEXT_SCHEMA {
            return Err(format!("renderer context schema {schema} is not {CONTEXT_SCHEMA}").into());
        }
        let renderer = RendererId::decode(decoder)?;
        let subject = decoder.option(RenderedSubjectContext::decode)?;
        let count = decoder.count()?;
        let mut references = Vec::new();
        for _ in 0..count {
            references.push(ResolvedReferenceContext::decode(decoder)?);
        }
        let base_package = Package::decode(decoder)?;
        let count = decoder.count()?;
        let mut layers = Vec::new();
        for _ in 0..count {
            let name = decoder.string()?;
            let layer =
                Layer::by_package(&name).ok_or_else(|| format!("unknown layer `{name}`"))?;
            layers.push(LayerContext {
                layer,
                package: Package::decode(decoder)?,
            });
        }
        let java_release = decoder.u32()?;
        let count = decoder.count()?;
        let mut capabilities = Vec::new();
        for _ in 0..count {
            capabilities.push(CanonicalCapability {
                id: crate::entity::CapabilityId::decode(decoder)?,
                spec: crate::entity::CapabilitySpec::decode(decoder)?,
            });
        }
        let context = Self {
            renderer,
            subject,
            references,
            base_package,
            layers,
            java_release,
            capabilities,
            bindings: TemplateBindings::decode(decoder)?,
        };
        context.validate()?;
        Ok(context)
    }
}

/// `SHA256("JAILS-RELEVANT-INPUT-1" || encode(rows))`.
///
/// An empty set hashes the canonical zero-count vector, not an empty byte
/// string — otherwise "declared nothing" and "was never asked" would share a
/// hash.
pub fn relevant_inputs(rows: &[InputPrecondition]) -> Result<ObjectId> {
    let mut encoder = Encoder::new();
    encoder.count(rows.len())?;
    for row in rows {
        row.encode(&mut encoder)?;
    }
    Ok(ObjectId::from_bytes(codec::domain_hash(
        "JAILS-RELEVANT-INPUT-1",
        &encoder.finish()?,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::declaration::IntentSpec;
    use crate::entity::{IntentId, Recipe};
    use crate::identity::Name;
    use crate::provenance::FormatOwner;

    fn package(text: &str) -> Package {
        Package::parse(text).unwrap()
    }

    fn layers() -> Vec<LayerContext> {
        Layer::ALL
            .into_iter()
            .map(|layer| LayerContext {
                layer,
                package: package(&format!("com.example.demo.{}", layer.package())),
            })
            .collect()
    }

    fn record_context() -> RendererContextV1 {
        RendererContextV1 {
            renderer: RendererId::Recipe(Recipe::Record),
            subject: Some(RenderedSubjectContext::Entity {
                id: EntityId::Intent(IntentId::new(
                    Recipe::Record,
                    Name::parse("Note").unwrap(),
                    package("com.example.demo.domain"),
                )),
                spec: EntitySpec::Intent(IntentSpec::default()),
            }),
            references: Vec::new(),
            base_package: package("com.example.demo"),
            layers: layers(),
            java_release: 25,
            capabilities: Vec::new(),
            bindings: TemplateBindings::new(),
        }
    }

    fn round_trip(context: &RendererContextV1) -> RendererContextV1 {
        let bytes = context.to_object().unwrap();
        let mut decoder = Decoder::new(&bytes).unwrap();
        let back = RendererContextV1::decode(&mut decoder).unwrap();
        decoder.finish().unwrap();
        back
    }

    #[test]
    fn a_recipe_context_round_trips() {
        let context = record_context();
        assert_eq!(round_trip(&context), context);
    }

    #[test]
    fn a_format_context_round_trips_with_no_subject() {
        let context = RendererContextV1 {
            renderer: RendererId::Format(FormatOwner::Pom),
            subject: None,
            ..record_context()
        };
        assert_eq!(round_trip(&context), context);
    }

    /// A format renderer renders a file, not an entity; a subject there would
    /// be a claim nothing supports.
    #[test]
    fn a_format_renderer_with_a_subject_is_refused() {
        let context = RendererContextV1 {
            renderer: RendererId::Format(FormatOwner::Pom),
            ..record_context()
        };
        assert!(context.validate().unwrap_err().contains("has no subject"));
    }

    #[test]
    fn a_recipe_renderer_with_no_subject_is_refused() {
        let context = RendererContextV1 {
            subject: None,
            ..record_context()
        };
        assert!(
            context
                .validate()
                .unwrap_err()
                .contains("recorded no subject")
        );
    }

    /// Not a map: an omitted layer and a layer resolved to its default would
    /// encode the same, and a renderer that saw ten layers is not the one
    /// that saw eleven.
    #[test]
    fn every_layer_must_be_present_in_its_declared_order() {
        let mut short = record_context();
        short.layers.pop();
        assert!(short.validate().unwrap_err().contains("there are 11"));

        let mut shuffled = record_context();
        shuffled.layers.swap(0, 1);
        assert!(
            shuffled
                .validate()
                .unwrap_err()
                .contains("the order is part of the encoding")
        );
    }

    /// "Declared nothing" and "was never asked" must not share a hash.
    #[test]
    fn an_empty_relevant_input_set_hashes_the_zero_count_vector() {
        let empty = relevant_inputs(&[]).unwrap();
        assert_ne!(empty.to_hex(), ObjectId::from_bytes([0; 32]).to_hex());
        assert_eq!(empty, relevant_inputs(&[]).unwrap());
    }

    /// A changed input changes the hash, which is what lets a stamp explain a
    /// changed render.
    #[test]
    fn a_changed_declared_input_changes_the_hash() {
        let one = relevant_inputs(&[InputPrecondition::Absent {
            path: crate::identity::ProjectPath::parse("compose.yaml").unwrap(),
        }])
        .unwrap();
        let other = relevant_inputs(&[InputPrecondition::Absent {
            path: crate::identity::ProjectPath::parse("pom.xml").unwrap(),
        }])
        .unwrap();
        assert_ne!(one, other);
    }

    #[test]
    fn an_unknown_context_schema_is_refused() {
        let mut encoder = Encoder::new();
        encoder.u32(2);
        let bytes = encoder.finish().unwrap();
        let mut decoder = Decoder::new(&bytes).unwrap();
        assert!(RendererContextV1::decode(&mut decoder).is_err());
    }

    /// Identity and spec must describe the same thing, or the context
    /// explains a render of something else.
    #[test]
    fn a_subject_whose_identity_and_spec_disagree_is_refused() {
        let context = RendererContextV1 {
            subject: Some(RenderedSubjectContext::Entity {
                id: EntityId::Intent(IntentId::new(
                    Recipe::Record,
                    Name::parse("Note").unwrap(),
                    package("com.example.demo.domain"),
                )),
                spec: EntitySpec::Capability(crate::entity::CapabilitySpec::default()),
            }),
            ..record_context()
        };
        assert!(context.validate().unwrap_err().contains("different kinds"));
    }
}
