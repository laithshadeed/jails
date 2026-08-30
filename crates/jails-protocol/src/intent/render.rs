//! How a file's content is described before anything renders it.
//!
//! ## Why `Render` is not a rendered string
//!
//! Keeping the template id and its bindings means the plan can be compared,
//! explained and re-rendered after a template changes, and it means planning
//! does no template resolution — plan.md §R2.1 makes template discovery part
//! of the snapshot, so a planner that rendered eagerly would be reading the
//! filesystem at plan time.
//!
//! ## Why a binding is a typed value
//!
//! A package or a Java type that reaches a template has been validated once,
//! at its constructor. Passing it as a `String` would put a second, weaker
//! check at every call site — or, more likely, none.

use crate::Result;
use crate::conflict::FileMode;
use crate::identity::{JavaType, Name, Package, ProjectPath, TemplateId, TemplateKey};
use crate::provenance::RendererStamp;
use crate::resource::ResourceKey;
use jails_support::codec::{Codec, Decoder, Encoder, MAX_CODEC_DEPTH, ordered};
use std::collections::BTreeMap;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Files
// ---------------------------------------------------------------------------

/// A file's content, either as bytes or as the render that produces them.
///
/// `Arc<[u8]>` rather than `Vec<u8>` because one body is shared by the plan,
/// the report and (in R4) the journal, and copying it three times is the kind
/// of cost that only shows up on a large scaffold.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DesiredBody {
    Bytes(Arc<[u8]>),
    Render {
        template: TemplateId,
        bindings: TemplateBindings,
    },
}

impl Codec for DesiredBody {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        match self {
            Self::Bytes(bytes) => {
                encoder.tag(0);
                encoder.object(bytes, jails_support::codec::DEFAULT_MAX_OBJECT_BYTES)
            }
            Self::Render { template, bindings } => {
                encoder.tag(1);
                template.encode(encoder)?;
                bindings.encode(encoder)
            }
        }
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(match decoder.tag()? {
            0 => Self::Bytes(
                decoder
                    .object(jails_support::codec::DEFAULT_MAX_OBJECT_BYTES)?
                    .into(),
            ),
            1 => Self::Render {
                template: TemplateId::decode(decoder)?,
                bindings: TemplateBindings::decode(decoder)?,
            },
            other => Err(format!("unknown desired body tag {other}"))?,
        })
    }
}

/// What a template is given. Typed values, not strings, so a package or a Java
/// type that reaches a template has already been validated once.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TemplateBindings(BTreeMap<TemplateKey, TemplateValue>);

impl TemplateBindings {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn bind(&mut self, key: TemplateKey, value: TemplateValue) -> Result<()> {
        if let Some(existing) = self.0.get(&key) {
            if existing == &value {
                return Ok(());
            }
            return Err(format!("template key `{key}` already bound to another value").into());
        }
        self.0.insert(key, value);
        Ok(())
    }

    pub fn get(&self, key: &TemplateKey) -> Option<&TemplateValue> {
        self.0.get(key)
    }

    pub fn keys(&self) -> impl Iterator<Item = &TemplateKey> {
        self.0.keys()
    }
}
impl Codec for TemplateBindings {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.count(self.0.len())?;
        let mut previous: Option<&TemplateKey> = None;
        for (key, value) in &self.0 {
            ordered(previous, key)?;
            previous = Some(key);
            key.encode(encoder)?;
            value.encode(encoder, 0)?;
        }
        Ok(())
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        let count = decoder.count()?;
        let mut bindings = BTreeMap::new();
        let mut previous: Option<TemplateKey> = None;
        for _ in 0..count {
            let key = TemplateKey::decode(decoder)?;
            ordered(previous.as_ref(), &key)?;
            previous = Some(key.clone());
            bindings.insert(key, TemplateValue::decode(decoder, 0)?);
        }
        Ok(Self(bindings))
    }
}

/// One bound value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TemplateValue {
    Text(String),
    Name(Name),
    Package(Package),
    JavaType(JavaType),
    Boolean(bool),
    Ordered(Vec<TemplateValue>),
}

impl TemplateValue {
    fn tag(&self) -> u8 {
        match self {
            Self::Text(_) => 0,
            Self::Name(_) => 1,
            Self::Package(_) => 2,
            Self::JavaType(_) => 3,
            Self::Boolean(_) => 4,
            Self::Ordered(_) => 5,
        }
    }

    pub fn encode(&self, encoder: &mut Encoder, depth: usize) -> Result<()> {
        let depth = deeper(depth)?;
        encoder.tag(self.tag());
        match self {
            Self::Text(text) => encoder.string(text),
            Self::Name(name) => name.encode(encoder),
            Self::Package(package) => package.encode(encoder),
            Self::JavaType(java_type) => java_type.encode(encoder),
            Self::Boolean(value) => {
                encoder.bool(*value);
                Ok(())
            }
            Self::Ordered(values) => {
                encoder.count(values.len())?;
                for value in values {
                    value.encode(encoder, depth)?;
                }
                Ok(())
            }
        }
    }

    pub fn decode(decoder: &mut Decoder<'_>, depth: usize) -> Result<Self> {
        let depth = deeper(depth)?;
        Ok(match decoder.tag()? {
            0 => Self::Text(decoder.string()?),
            1 => Self::Name(Name::decode(decoder)?),
            2 => Self::Package(Package::decode(decoder)?),
            3 => Self::JavaType(JavaType::decode(decoder)?),
            4 => Self::Boolean(decoder.bool()?),
            5 => {
                let count = decoder.count()?;
                let mut values = Vec::new();
                for _ in 0..count {
                    values.push(Self::decode(decoder, depth)?);
                }
                Self::Ordered(values)
            }
            other => Err(format!("unknown template value tag {other}"))?,
        })
    }
}

fn deeper(depth: usize) -> Result<usize> {
    if depth >= MAX_CODEC_DEPTH {
        return Err(format!("value nested deeper than {MAX_CODEC_DEPTH}").into());
    }
    Ok(depth + 1)
}

/// One file this change wants on disk.
///
/// `mode` is **the only optional mode in the mutation model** (§R2.3). `None`
/// is not "unknown": it means the recipe has no opinion, and preparation
/// resolves it deterministically — a replace keeps the captured live mode, a
/// create uses `0o644`. A recipe that creates an executable must say `0o755`,
/// because a mode derived from the process umask would make the same plan
/// produce different files on two machines.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DesiredFile {
    pub path: ProjectPath,
    pub body: DesiredBody,
    pub mode: Option<FileMode>,
    pub resource: Option<ResourceKey>,
    /// Where these bytes came from, for the `OutputRecord` the commit writes.
    ///
    /// §R5.2 requires every *managed output* to carry a non-optional renderer,
    /// and this is where the answer travels. It is `Option` here and not there
    /// for one reason: a `DesiredFile` is also how a change states bytes for a
    /// file nobody owns, and stamping one of those would claim provenance for
    /// a path that has no output row. A file with a `resource` and no stamp
    /// records no base, which is the pre-R5 behaviour and is what the update
    /// path refuses against.
    pub renderer: Option<DesiredProvenance>,
}

/// A stamp and the context object it names.
///
/// The bytes travel with the stamp rather than being fetched later, because
/// the commit has to store the object *before* the ledger that references it
/// -- §R5.1's rule -- and a stamp whose context object is not in the store is
/// a dangling reference the next GC cycle would collect.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DesiredProvenance {
    pub stamp: RendererStamp,
    /// Exactly `encode(RendererContextV1)`, which is what
    /// `RendererStamp::context_object` hashes.
    pub context: Arc<[u8]>,
}

impl Codec for DesiredProvenance {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.stamp.encode(encoder)?;
        encoder.object(
            &self.context,
            jails_support::codec::DEFAULT_MAX_OBJECT_BYTES,
        )
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(Self {
            stamp: RendererStamp::decode(decoder)?,
            context: Arc::from(
                decoder
                    .object(jails_support::codec::DEFAULT_MAX_OBJECT_BYTES)?
                    .as_slice(),
            ),
        })
    }
}

impl Codec for DesiredFile {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.path.encode(encoder)?;
        self.body.encode(encoder)?;
        encoder.option(self.mode.as_ref(), |e, mode| {
            mode.encode(e)?;
            Ok(())
        })?;
        encoder.option(self.resource.as_ref(), |e, key| key.encode(e))?;
        encoder.option(self.renderer.as_ref(), |e, provenance| provenance.encode(e))
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(Self {
            path: ProjectPath::decode(decoder)?,
            body: DesiredBody::decode(decoder)?,
            mode: decoder.option(FileMode::decode)?,
            resource: decoder.option(ResourceKey::decode)?,
            renderer: decoder.option(DesiredProvenance::decode)?,
        })
    }
}

/// A path that must not exist afterwards.
///
/// `force` is what distinguishes "remove the file jails wrote" from "remove
/// whatever is there" — the second needs a human to have asked for it, which
/// is why it is recorded on the absence rather than decided at execution.
#[derive(Clone, Debug, Eq, PartialEq, jails_codec_derive::Codec)]
pub struct ManagedPath {
    pub path: ProjectPath,
    pub resource: ResourceKey,
    pub force: bool,
}
