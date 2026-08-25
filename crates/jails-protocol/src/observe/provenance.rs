//! Where a generated file came from: which renderer, which template, which
//! inputs.
//!
//! ## What this buys
//!
//! plan.md §R5.2 requires every managed output to carry a **non-optional** base
//! and renderer. Today a generated file records only that jails wrote it, so
//! when it changes there is nothing to say *why* — a new jails version, a
//! different template, an edited input, or a genuine change of declaration all
//! look identical. `RendererStamp` makes the four distinguishable.
//!
//! ## Two rules that are easy to get wrong
//!
//! - **`template` is `None` for a pure format owner** and `Some` only when
//!   template bytes actually contributed. A stamp that always carried one
//!   would claim a POM splice came from a template it never read.
//! - **Never store an absolute home or template path.** `source_object` proves
//!   the bytes even when a user override later disappears, which an absolute
//!   path on somebody else's machine cannot.
//!
//! `relevant_inputs` hashes only the snapshot inputs a renderer *declared*.
//! Hashing everything would make every project file part of every output's
//! provenance, so any unrelated edit would appear to explain the change — the
//! opposite of the point.

use crate::Result;
use crate::entity::{Recipe, ToolFeature};
use crate::identity::{ObjectId, ProjectPath, TemplateId};
use jails_spec::spec::kind::Capability;
use jails_support::codec::{self, Codec, Decoder, Encoder};

/// Which renderer produced an output.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RendererId {
    Recipe(Recipe),
    Capability(Capability),
    /// A file-format owner rather than a code generator: the thing that knows
    /// how to splice a POM or a compose document without disturbing the rest.
    Format(FormatOwner),
    OneShot(OneShotKind),
    ToolFeature(ToolFeature),
}

/// The formats jails edits in place, each owned by exactly one splicer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FormatOwner {
    Pom,
    Compose,
    Properties,
    HumanConfig,
    MarkedSource,
    CommandRegistration,
    WholeFile,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OneShotKind {
    Field,
    Migration,
    Cases,
}

/// Where a template's bytes came from, as identity rather than location.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TemplateOrigin {
    BuiltIn {
        name: TemplateId,
    },
    ProjectOverride {
        path: ProjectPath,
    },
    /// A machine-level override. Only its logical name is recorded — an
    /// absolute home path means nothing on another machine, and the bytes are
    /// proved by `source_object` regardless.
    UserOverride {
        logical_name: TemplateId,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TemplateStamp {
    pub origin: TemplateOrigin,
    pub source_object: ObjectId,
}

/// Everything needed to explain one rendered output.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RendererStamp {
    pub renderer: RendererId,
    pub renderer_schema: u32,
    pub jails_version: String,
    /// `None` for a pure format owner; `Some` only when template bytes
    /// actually contributed.
    pub template: Option<TemplateStamp>,
    pub context_schema: u32,
    pub context_object: ObjectId,
    /// The canonical hash of only the snapshot inputs this renderer declared.
    pub relevant_inputs: ObjectId,
    /// Full fingerprints of tools that produced the desired base. Git merge is
    /// transaction preparation rather than rendering and never appears here.
    pub tools: Vec<ObjectId>,
}

impl FormatOwner {
    fn tag(self) -> u8 {
        match self {
            Self::Pom => 0,
            Self::Compose => 1,
            Self::Properties => 2,
            Self::HumanConfig => 3,
            Self::MarkedSource => 4,
            Self::CommandRegistration => 5,
            Self::WholeFile => 6,
        }
    }

    fn from_tag(tag: u8) -> Result<Self> {
        Ok(match tag {
            0 => Self::Pom,
            1 => Self::Compose,
            2 => Self::Properties,
            3 => Self::HumanConfig,
            4 => Self::MarkedSource,
            5 => Self::CommandRegistration,
            6 => Self::WholeFile,
            other => return Err(format!("unknown format owner tag {other}").into()),
        })
    }
}

impl OneShotKind {
    fn tag(self) -> u8 {
        match self {
            Self::Field => 0,
            Self::Migration => 1,
            Self::Cases => 2,
        }
    }

    fn from_tag(tag: u8) -> Result<Self> {
        Ok(match tag {
            0 => Self::Field,
            1 => Self::Migration,
            2 => Self::Cases,
            other => return Err(format!("unknown one-shot kind tag {other}").into()),
        })
    }
}

impl Codec for RendererId {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        match self {
            Self::Recipe(recipe) => {
                encoder.tag(0);
                encoder.string(crate::entity::recipe_label(*recipe))?;
            }
            Self::Capability(capability) => {
                encoder.tag(1);
                encoder.string(capability.label())?;
            }
            Self::Format(owner) => {
                encoder.tag(2);
                encoder.tag(owner.tag());
            }
            Self::OneShot(kind) => {
                encoder.tag(3);
                encoder.tag(kind.tag());
            }
            Self::ToolFeature(ToolFeature::FastTest) => {
                encoder.tag(4);
                encoder.tag(0);
            }
        }
        Ok(())
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(match decoder.tag()? {
            0 => Self::Recipe(crate::entity::recipe_from_label(&decoder.string()?)?),
            1 => Self::Capability(crate::entity::capability_from_label(&decoder.string()?)?),
            2 => Self::Format(FormatOwner::from_tag(decoder.tag()?)?),
            3 => Self::OneShot(OneShotKind::from_tag(decoder.tag()?)?),
            4 => match decoder.tag()? {
                0 => Self::ToolFeature(ToolFeature::FastTest),
                other => return Err(format!("unknown tool feature tag {other}").into()),
            },
            other => return Err(format!("unknown renderer tag {other}").into()),
        })
    }
}

impl Codec for TemplateOrigin {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        match self {
            Self::BuiltIn { name } => {
                encoder.tag(0);
                name.encode(encoder)
            }
            Self::ProjectOverride { path } => {
                encoder.tag(1);
                path.encode(encoder)
            }
            Self::UserOverride { logical_name } => {
                encoder.tag(2);
                logical_name.encode(encoder)
            }
        }
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(match decoder.tag()? {
            0 => Self::BuiltIn {
                name: TemplateId::decode(decoder)?,
            },
            1 => Self::ProjectOverride {
                path: ProjectPath::decode(decoder)?,
            },
            2 => Self::UserOverride {
                logical_name: TemplateId::decode(decoder)?,
            },
            other => return Err(format!("unknown template origin tag {other}").into()),
        })
    }
}

impl Codec for TemplateStamp {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.origin.encode(encoder)?;
        self.source_object.encode(encoder)?;
        Ok(())
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(Self {
            origin: TemplateOrigin::decode(decoder)?,
            source_object: ObjectId::decode(decoder)?,
        })
    }
}

impl RendererStamp {
    /// `SHA256("JAILS-RELEVANT-INPUT-1" || encode(rows))`.
    ///
    /// Only the inputs a renderer declared. Hashing every project file would
    /// make any unrelated edit appear to explain a changed render, which is
    /// the opposite of what provenance is for.
    pub fn relevant_input_hash(rows: &[(ProjectPath, ObjectId)]) -> Result<ObjectId> {
        let mut encoder = Encoder::new();
        encoder.count(rows.len())?;
        let mut previous: Option<&ProjectPath> = None;
        for (path, object) in rows {
            codec::ordered(previous, path)?;
            previous = Some(path);
            path.encode(&mut encoder)?;
            object.encode(&mut encoder)?;
        }
        Ok(ObjectId::from_bytes(codec::domain_hash(
            "JAILS-RELEVANT-INPUT-1",
            &encoder.finish()?,
        )))
    }
}
impl Codec for RendererStamp {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.renderer.encode(encoder)?;
        encoder.u32(self.renderer_schema);
        encoder.string(&self.jails_version)?;
        encoder.option(self.template.as_ref(), |e, stamp| stamp.encode(e))?;
        encoder.u32(self.context_schema);
        self.context_object.encode(encoder)?;
        self.relevant_inputs.encode(encoder)?;
        encoder.count(self.tools.len())?;
        for tool in &self.tools {
            tool.encode(encoder)?;
        }
        Ok(())
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        let renderer = RendererId::decode(decoder)?;
        let renderer_schema = decoder.u32()?;
        let jails_version = decoder.string()?;
        let template = decoder.option(TemplateStamp::decode)?;
        let context_schema = decoder.u32()?;
        let context_object = ObjectId::decode(decoder)?;
        let relevant_inputs = ObjectId::decode(decoder)?;
        let count = decoder.count()?;
        let mut tools = Vec::new();
        for _ in 0..count {
            tools.push(ObjectId::decode(decoder)?);
        }
        Ok(Self {
            renderer,
            renderer_schema,
            jails_version,
            template,
            context_schema,
            context_object,
            relevant_inputs,
            tools,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jails_support::codec::sha256;

    fn object(seed: &str) -> ObjectId {
        ObjectId::from_bytes(sha256(seed.as_bytes()))
    }

    fn stamp(template: Option<TemplateStamp>) -> RendererStamp {
        RendererStamp {
            renderer: RendererId::Recipe(Recipe::Record),
            renderer_schema: 1,
            jails_version: "0.1.0".to_string(),
            template,
            context_schema: 1,
            context_object: object("context"),
            relevant_inputs: object("inputs"),
            tools: vec![object("spotless")],
        }
    }

    #[test]
    fn every_renderer_id_round_trips() {
        for renderer in [
            RendererId::Recipe(Recipe::Scaffold),
            RendererId::Capability(Capability::Db),
            RendererId::Format(FormatOwner::Pom),
            RendererId::Format(FormatOwner::MarkedSource),
            RendererId::OneShot(OneShotKind::Migration),
            RendererId::ToolFeature(ToolFeature::FastTest),
        ] {
            let mut encoder = Encoder::new();
            renderer.encode(&mut encoder).unwrap();
            let bytes = encoder.finish().unwrap();
            let mut decoder = Decoder::new(&bytes).unwrap();
            assert_eq!(RendererId::decode(&mut decoder).unwrap(), renderer);
            decoder.finish().unwrap();
        }
    }

    /// A stamp that always carried a template would claim a POM splice came
    /// from a template it never read.
    #[test]
    fn a_pure_format_owner_carries_no_template() {
        let pom = RendererStamp {
            renderer: RendererId::Format(FormatOwner::Pom),
            template: None,
            ..stamp(None)
        };
        let mut encoder = Encoder::new();
        pom.encode(&mut encoder).unwrap();
        let bytes = encoder.finish().unwrap();
        let mut decoder = Decoder::new(&bytes).unwrap();
        let back = RendererStamp::decode(&mut decoder).unwrap();
        assert!(back.template.is_none());
        assert_eq!(back, pom);
    }

    /// The bytes are proved by `source_object`, so a user override that later
    /// disappears is still explicable — which an absolute path on somebody
    /// else's machine could never be.
    #[test]
    fn a_template_origin_records_identity_not_a_location() {
        for origin in [
            TemplateOrigin::BuiltIn {
                name: TemplateId::parse("generate/record.java").unwrap(),
            },
            TemplateOrigin::ProjectOverride {
                path: ProjectPath::parse(".jails/templates/generate/record.java").unwrap(),
            },
            TemplateOrigin::UserOverride {
                logical_name: TemplateId::parse("generate/record.java").unwrap(),
            },
        ] {
            let full = stamp(Some(TemplateStamp {
                origin: origin.clone(),
                source_object: object("template-bytes"),
            }));
            let mut encoder = Encoder::new();
            full.encode(&mut encoder).unwrap();
            let bytes = encoder.finish().unwrap();
            let mut decoder = Decoder::new(&bytes).unwrap();
            assert_eq!(RendererStamp::decode(&mut decoder).unwrap(), full);
            decoder.finish().unwrap();
        }

        // An absolute path cannot even be constructed as a project override.
        assert!(ProjectPath::parse("/home/someone/.jails/templates/x.java").is_err());
    }

    /// The four reasons an output can change are distinguishable, which is the
    /// whole point of recording any of this.
    #[test]
    fn a_changed_input_template_or_version_gives_a_different_stamp() {
        let base = stamp(None);
        let cases = [
            RendererStamp {
                jails_version: "0.2.0".to_string(),
                ..base.clone()
            },
            RendererStamp {
                relevant_inputs: object("other-inputs"),
                ..base.clone()
            },
            RendererStamp {
                context_object: object("other-context"),
                ..base.clone()
            },
            RendererStamp {
                template: Some(TemplateStamp {
                    origin: TemplateOrigin::BuiltIn {
                        name: TemplateId::parse("x").unwrap(),
                    },
                    source_object: object("t"),
                }),
                ..base.clone()
            },
        ];
        for changed in cases {
            assert_ne!(changed, base);
        }
    }

    /// Only declared inputs. Hashing everything would make any unrelated edit
    /// appear to explain the change.
    #[test]
    fn the_relevant_input_hash_covers_only_the_rows_it_is_given() {
        let path = |p: &str| ProjectPath::parse(p).unwrap();
        let one = RendererStamp::relevant_input_hash(&[
            (path("pom.xml"), object("a")),
            (path("src/main/java/A.java"), object("b")),
        ])
        .unwrap();
        let same = RendererStamp::relevant_input_hash(&[
            (path("pom.xml"), object("a")),
            (path("src/main/java/A.java"), object("b")),
        ])
        .unwrap();
        assert_eq!(one, same);

        let fewer = RendererStamp::relevant_input_hash(&[(path("pom.xml"), object("a"))]).unwrap();
        assert_ne!(one, fewer);

        // Unsorted rows refuse rather than hashing to a second value for the
        // same set.
        let unsorted = RendererStamp::relevant_input_hash(&[
            (path("src/main/java/A.java"), object("b")),
            (path("pom.xml"), object("a")),
        ]);
        assert!(unsorted.is_err());
    }

    #[test]
    fn an_unknown_tag_rejects() {
        let mut decoder = Decoder::new(&[9]).unwrap();
        assert!(
            RendererId::decode(&mut decoder)
                .unwrap_err()
                .contains("unknown renderer tag")
        );
    }
}
