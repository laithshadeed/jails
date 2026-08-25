//! What a planner is allowed to know about a project, as values.
//!
//! plan.md §R2.1 gives the snapshot a keyed fact map beside its scalar fields.
//! The rule that makes it worth having is the `sources` half: every parser
//! input records **present-with-a-digest or absent**, so deleting a POM cannot
//! leave a stale dependency fact behind. A map that only held what was found
//! could not tell "no such dependency" from "nobody looked".
//!
//! ## Why key and fact are two enums that must pair
//!
//! A `ProjectFactKey` is what a precondition names; a `ProjectFact` is what
//! was observed. Storing them separately means a key can be *asked about*
//! without a value existing, which is exactly what `RequiresFact` needs. The
//! cost is that they can disagree, so every constructor and decoder calls
//! [`ProjectFact::agrees_with`] — §R2.1: *"Every `ProjectFactKey` may pair only
//! with the same-named `ProjectFact` variant."*
//!
//! ## Why the Java type grammar is closed
//!
//! `JavaTypeExpression` is not an arbitrary source fragment. It is the set of
//! type forms jails' reader accepts, which means a form it has never seen
//! *fails* rather than round-tripping as unparsed text that a later renderer
//! would emit into a file. §R2.1: *"Expand its variants and parser tests
//! together when a real project needs another type form."*

use crate::Result;
use crate::coordinate::{DependencySpec, MavenCoordinate, PluginSpec};
use crate::entity::{CapabilityId, CapabilitySpec};
use crate::identity::{JavaType, MarkerId, Name, ObjectId, ProjectPath, PropertyKey, ServiceName};
use crate::resource::ComposeServiceSpec;
use jails_support::codec::{Codec, Decoder, Encoder, MAX_CODEC_DEPTH, ordered};
use std::collections::BTreeMap;

// ---------------------------------------------------------------------------
// Sources
// ---------------------------------------------------------------------------

/// One parser input. A fact's authority is the source it was read from, and
/// naming the source is what lets a deleted file invalidate its facts.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub enum FactKind {
    Pom,
    HumanConfig,
    Compose,
    Properties(ProjectPath),
    JavaSource(ProjectPath),
}

impl FactKind {
    fn tag(&self) -> u8 {
        match self {
            Self::Pom => 0,
            Self::HumanConfig => 1,
            Self::Compose => 2,
            Self::Properties(_) => 3,
            Self::JavaSource(_) => 4,
        }
    }
}
impl Codec for FactKind {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.tag(self.tag());
        match self {
            Self::Pom | Self::HumanConfig | Self::Compose => Ok(()),
            Self::Properties(path) | Self::JavaSource(path) => path.encode(encoder),
        }
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(match decoder.tag()? {
            0 => Self::Pom,
            1 => Self::HumanConfig,
            2 => Self::Compose,
            3 => Self::Properties(ProjectPath::decode(decoder)?),
            4 => Self::JavaSource(ProjectPath::decode(decoder)?),
            other => Err(format!("unknown fact source tag {other}"))?,
        })
    }
}

/// Whether a parser input existed, and what it hashed to.
///
/// `Absent` is a recorded observation, not a missing entry: it is the
/// difference between "this project has no compose file" and "nobody looked".
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FactSourceState {
    Absent,
    Present { sha256: ObjectId, len: u64 },
}

impl Codec for FactSourceState {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        match self {
            Self::Absent => encoder.tag(0),
            Self::Present { sha256, len } => {
                encoder.tag(1);
                sha256.encode(encoder)?;
                encoder.u64(*len);
            }
        }
        Ok(())
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(match decoder.tag()? {
            0 => Self::Absent,
            1 => Self::Present {
                sha256: ObjectId::decode(decoder)?,
                len: decoder.u64()?,
            },
            other => Err(format!("unknown fact source state tag {other}"))?,
        })
    }
}

// ---------------------------------------------------------------------------
// Keys and facts
// ---------------------------------------------------------------------------

/// What a precondition or a delta names.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub enum ProjectFactKey {
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
    HumanConfigCapability(CapabilityId),
    JavaType(JavaType),
}

impl ProjectFactKey {
    pub fn tag(&self) -> u8 {
        match self {
            Self::MavenDependency(_) => 0,
            Self::MavenPlugin(_) => 1,
            Self::ComposeService(_) => 2,
            Self::Property { .. } => 3,
            Self::MarkedBlock { .. } => 4,
            Self::CommandRegistration { .. } => 5,
            Self::HumanConfigCapability(_) => 6,
            Self::JavaType(_) => 7,
        }
    }
}
impl Codec for ProjectFactKey {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.tag(self.tag());
        match self {
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
            Self::JavaType(java_type) => java_type.encode(encoder),
        }
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(match decoder.tag()? {
            0 => Self::MavenDependency(MavenCoordinate::decode(decoder)?),
            1 => Self::MavenPlugin(MavenCoordinate::decode(decoder)?),
            2 => Self::ComposeService(ServiceName::decode(decoder)?),
            3 => Self::Property {
                path: ProjectPath::decode(decoder)?,
                key: PropertyKey::decode(decoder)?,
            },
            4 => Self::MarkedBlock {
                path: ProjectPath::decode(decoder)?,
                marker: MarkerId::decode(decoder)?,
            },
            5 => Self::CommandRegistration {
                dispatcher: JavaType::decode(decoder)?,
                command: JavaType::decode(decoder)?,
            },
            6 => Self::HumanConfigCapability(CapabilityId::decode(decoder)?),
            7 => Self::JavaType(JavaType::decode(decoder)?),
            other => Err(format!("unknown project fact key tag {other}"))?,
        })
    }
}

/// What was observed for a key.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectFact {
    MavenDependency(DependencySpec),
    MavenPlugin(PluginSpec),
    ComposeService(ComposeServiceSpec),
    Property(String),
    /// Only the body's digest: a marked block's content belongs to whoever
    /// wrote it, and a fact map is not the place to carry an arbitrary body.
    MarkedBlock {
        body_sha256: ObjectId,
    },
    CommandRegistration,
    HumanConfigCapability(CapabilitySpec),
    JavaType(JavaTypeFact),
}

impl ProjectFact {
    pub fn tag(&self) -> u8 {
        match self {
            Self::MavenDependency(_) => 0,
            Self::MavenPlugin(_) => 1,
            Self::ComposeService(_) => 2,
            Self::Property(_) => 3,
            Self::MarkedBlock { .. } => 4,
            Self::CommandRegistration => 5,
            Self::HumanConfigCapability(_) => 6,
            Self::JavaType(_) => 7,
        }
    }

    /// §R2.1: a key may pair only with the same-named fact variant, and the
    /// coordinate carried by both must agree.
    pub fn agrees_with(&self, key: &ProjectFactKey) -> Result<()> {
        if self.tag() != key.tag() {
            return Err(format!(
                "project fact kind {} does not match key kind {}",
                self.tag(),
                key.tag()
            ));
        }
        match (key, self) {
            (ProjectFactKey::MavenDependency(coordinate), Self::MavenDependency(spec))
                if spec.coordinate != *coordinate =>
            {
                Err(format!(
                    "dependency fact {} recorded under key {coordinate}",
                    spec.coordinate
                ))
            }
            (ProjectFactKey::MavenPlugin(coordinate), Self::MavenPlugin(spec))
                if spec.coordinate != *coordinate =>
            {
                Err(format!(
                    "plugin fact {} recorded under key {coordinate}",
                    spec.coordinate
                ))
            }
            (ProjectFactKey::ComposeService(name), Self::ComposeService(spec))
                if spec.name != *name =>
            {
                Err(format!(
                    "compose service fact {} recorded under key {name}",
                    spec.name
                ))
            }
            _ => Ok(()),
        }
    }
}
impl Codec for ProjectFact {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.tag(self.tag());
        match self {
            Self::MavenDependency(spec) => spec.encode(encoder),
            Self::MavenPlugin(spec) => spec.encode(encoder),
            Self::ComposeService(spec) => spec.encode(encoder),
            Self::Property(value) => encoder.string(value),
            Self::MarkedBlock { body_sha256 } => {
                body_sha256.encode(encoder)?;
                Ok(())
            }
            Self::CommandRegistration => Ok(()),
            Self::HumanConfigCapability(spec) => spec.encode(encoder),
            Self::JavaType(fact) => fact.encode(encoder),
        }
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(match decoder.tag()? {
            0 => Self::MavenDependency(DependencySpec::decode(decoder)?),
            1 => Self::MavenPlugin(PluginSpec::decode(decoder)?),
            2 => Self::ComposeService(ComposeServiceSpec::decode(decoder)?),
            3 => Self::Property(decoder.string()?),
            4 => Self::MarkedBlock {
                body_sha256: ObjectId::decode(decoder)?,
            },
            5 => Self::CommandRegistration,
            6 => Self::HumanConfigCapability(CapabilitySpec::decode(decoder)?),
            7 => Self::JavaType(JavaTypeFact::decode(decoder)?),
            other => Err(format!("unknown project fact tag {other}"))?,
        })
    }
}

/// Every fact a planner may consult, with its sources' presence.
///
/// A value is stored **with the input it was parsed from**. That is what lets
/// a changed or deleted file invalidate exactly its own facts: without it,
/// deleting a POM would leave its dependency facts in place and every later
/// decision would be made against a file that no longer exists.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ProjectFacts {
    sources: BTreeMap<FactKind, FactSourceState>,
    values: BTreeMap<ProjectFactKey, (FactKind, ProjectFact)>,
}

impl ProjectFacts {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn observe(&mut self, kind: FactKind, state: FactSourceState) {
        self.sources.insert(kind, state);
    }

    /// Refuses a mismatched pair and a duplicate key. §R2.1: *"duplicate keys
    /// reject"* — a second value for one key is two answers to one question.
    pub fn record(
        &mut self,
        source: FactKind,
        key: ProjectFactKey,
        fact: ProjectFact,
    ) -> Result<()> {
        fact.agrees_with(&key)?;
        if let Some((existing_source, existing)) = self.values.get(&key) {
            if existing == &fact && existing_source == &source {
                return Ok(());
            }
            return Err(format!(
                "project fact key {key:?} already holds a different value"
            ));
        }
        self.values.insert(key, (source, fact));
        Ok(())
    }

    /// Forget exactly one fact, for a delta that removes it.
    pub fn invalidate_key(&mut self, key: &ProjectFactKey) {
        self.values.remove(key);
    }

    /// Forget everything one input said. Called when that input changed or
    /// went away, before it is reparsed.
    pub fn invalidate(&mut self, source: &FactKind) {
        self.values.retain(|_, (from, _)| from != source);
    }

    pub fn get(&self, key: &ProjectFactKey) -> Option<&ProjectFact> {
        self.values.get(key).map(|(_, fact)| fact)
    }

    /// Which input a fact came from.
    pub fn source_of(&self, key: &ProjectFactKey) -> Option<&FactKind> {
        self.values.get(key).map(|(source, _)| source)
    }

    pub fn source(&self, kind: &FactKind) -> Option<FactSourceState> {
        self.sources.get(kind).copied()
    }

    pub fn values(&self) -> impl Iterator<Item = (&ProjectFactKey, &ProjectFact)> {
        self.values.iter().map(|(key, (_, fact))| (key, fact))
    }
}
impl Codec for ProjectFacts {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.count(self.sources.len())?;
        let mut previous: Option<&FactKind> = None;
        for (kind, state) in &self.sources {
            ordered(previous, kind)?;
            previous = Some(kind);
            kind.encode(encoder)?;
            state.encode(encoder)?;
        }
        encoder.count(self.values.len())?;
        let mut previous: Option<&ProjectFactKey> = None;
        for (key, (source, fact)) in &self.values {
            ordered(previous, key)?;
            previous = Some(key);
            fact.agrees_with(key)?;
            key.encode(encoder)?;
            source.encode(encoder)?;
            fact.encode(encoder)?;
        }
        Ok(())
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        let count = decoder.count()?;
        let mut sources = BTreeMap::new();
        let mut previous: Option<FactKind> = None;
        for _ in 0..count {
            let kind = FactKind::decode(decoder)?;
            ordered(previous.as_ref(), &kind)?;
            previous = Some(kind.clone());
            sources.insert(kind, FactSourceState::decode(decoder)?);
        }
        let count = decoder.count()?;
        let mut values = BTreeMap::new();
        let mut previous: Option<ProjectFactKey> = None;
        for _ in 0..count {
            let key = ProjectFactKey::decode(decoder)?;
            ordered(previous.as_ref(), &key)?;
            previous = Some(key.clone());
            let source = FactKind::decode(decoder)?;
            let fact = ProjectFact::decode(decoder)?;
            fact.agrees_with(&key)?;
            values.insert(key, (source, fact));
        }
        Ok(Self { sources, values })
    }
}

/// The delta form: a plain key-to-fact map, with no source. A delta is what
/// a recipe *declares*; where it came from is the recipe, not an input file.
pub(crate) fn encode_fact_map(
    encoder: &mut Encoder,
    values: &BTreeMap<ProjectFactKey, ProjectFact>,
) -> Result<()> {
    encoder.count(values.len())?;
    let mut previous: Option<&ProjectFactKey> = None;
    for (key, fact) in values {
        ordered(previous, key)?;
        previous = Some(key);
        fact.agrees_with(key)?;
        key.encode(encoder)?;
        fact.encode(encoder)?;
    }
    Ok(())
}

pub(crate) fn decode_fact_map(
    decoder: &mut Decoder<'_>,
) -> Result<BTreeMap<ProjectFactKey, ProjectFact>> {
    let count = decoder.count()?;
    let mut values = BTreeMap::new();
    let mut previous: Option<ProjectFactKey> = None;
    for _ in 0..count {
        let key = ProjectFactKey::decode(decoder)?;
        ordered(previous.as_ref(), &key)?;
        previous = Some(key.clone());
        let fact = ProjectFact::decode(decoder)?;
        fact.agrees_with(&key)?;
        values.insert(key, fact);
    }
    Ok(values)
}

// ---------------------------------------------------------------------------
// The Java type grammar
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JavaTypeKind {
    Class,
    Record,
    Interface,
    Enum,
}

impl JavaTypeKind {
    fn tag(self) -> u8 {
        match self {
            Self::Class => 0,
            Self::Record => 1,
            Self::Interface => 2,
            Self::Enum => 3,
        }
    }

    fn from_tag(tag: u8) -> Result<Self> {
        match tag {
            0 => Ok(Self::Class),
            1 => Ok(Self::Record),
            2 => Ok(Self::Interface),
            3 => Ok(Self::Enum),
            other => Err(format!("unknown Java type kind tag {other}")),
        }
    }
}

/// What jails' reader learned about one declared type.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JavaTypeFact {
    pub source: ProjectPath,
    pub kind: JavaTypeKind,
    pub supertypes: Vec<JavaType>,
    pub constructor: Vec<JavaParameterFact>,
    pub enum_constants: Vec<Name>,
}

impl Codec for JavaTypeFact {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.source.encode(encoder)?;
        encoder.tag(self.kind.tag());
        encoder.count(self.supertypes.len())?;
        for supertype in &self.supertypes {
            supertype.encode(encoder)?;
        }
        encoder.count(self.constructor.len())?;
        for parameter in &self.constructor {
            parameter.encode(encoder)?;
        }
        encoder.count(self.enum_constants.len())?;
        for constant in &self.enum_constants {
            constant.encode(encoder)?;
        }
        Ok(())
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        let source = ProjectPath::decode(decoder)?;
        let kind = JavaTypeKind::from_tag(decoder.tag()?)?;
        let supertypes = decoder.seq::<JavaType>()?;
        let constructor = decoder.seq::<JavaParameterFact>()?;
        let enum_constants = decoder.seq::<Name>()?;
        Ok(Self {
            source,
            kind,
            supertypes,
            constructor,
            enum_constants,
        })
    }
}

/// One constructor parameter. Order is preserved: a record's components are
/// positional, so reordering them changes what the constructor means.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JavaParameterFact {
    pub name: Name,
    pub type_expression: JavaTypeExpression,
}

impl Codec for JavaParameterFact {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.name.encode(encoder)?;
        self.type_expression.encode(encoder, 0)
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(Self {
            name: Name::decode(decoder)?,
            type_expression: JavaTypeExpression::decode(decoder, 0)?,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JavaPrimitive {
    Boolean,
    Byte,
    Short,
    Int,
    Long,
    Char,
    Float,
    Double,
}

/// Nothing calls either of these -- no fact is built from a primitive yet.
/// `pending.md` §7.2.
impl JavaPrimitive {
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "boolean" => Self::Boolean,
            "byte" => Self::Byte,
            "short" => Self::Short,
            "int" => Self::Int,
            "long" => Self::Long,
            "char" => Self::Char,
            "float" => Self::Float,
            "double" => Self::Double,
            _ => return None,
        })
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Boolean => "boolean",
            Self::Byte => "byte",
            Self::Short => "short",
            Self::Int => "int",
            Self::Long => "long",
            Self::Char => "char",
            Self::Float => "float",
            Self::Double => "double",
        }
    }

    fn tag(self) -> u8 {
        match self {
            Self::Boolean => 0,
            Self::Byte => 1,
            Self::Short => 2,
            Self::Int => 3,
            Self::Long => 4,
            Self::Char => 5,
            Self::Float => 6,
            Self::Double => 7,
        }
    }

    fn from_tag(tag: u8) -> Result<Self> {
        match tag {
            0 => Ok(Self::Boolean),
            1 => Ok(Self::Byte),
            2 => Ok(Self::Short),
            3 => Ok(Self::Int),
            4 => Ok(Self::Long),
            5 => Ok(Self::Char),
            6 => Ok(Self::Float),
            7 => Ok(Self::Double),
            other => Err(format!("unknown Java primitive tag {other}")),
        }
    }
}

/// The closed Java type grammar jails' reader accepts.
///
/// A declared name is always resolved to a qualified [`JavaType`]; a type
/// variable stays a variable. The two must not merge — `T` and
/// `com.example.T` are different things, and a reader that erased the
/// difference would happily import a type variable.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum JavaTypeExpression {
    Primitive(JavaPrimitive),
    Declared {
        qualified_name: JavaType,
        arguments: Vec<JavaTypeArgument>,
    },
    TypeVariable(Name),
    Array(Box<JavaTypeExpression>),
}

impl JavaTypeExpression {
    fn tag(&self) -> u8 {
        match self {
            Self::Primitive(_) => 0,
            Self::Declared { .. } => 1,
            Self::TypeVariable(_) => 2,
            Self::Array(_) => 3,
        }
    }

    pub fn encode(&self, encoder: &mut Encoder, depth: usize) -> Result<()> {
        let depth = deeper(depth)?;
        encoder.tag(self.tag());
        match self {
            Self::Primitive(primitive) => {
                encoder.tag(primitive.tag());
                Ok(())
            }
            Self::Declared {
                qualified_name,
                arguments,
            } => {
                qualified_name.encode(encoder)?;
                encoder.count(arguments.len())?;
                for argument in arguments {
                    argument.encode(encoder, depth)?;
                }
                Ok(())
            }
            Self::TypeVariable(name) => name.encode(encoder),
            Self::Array(inner) => inner.encode(encoder, depth),
        }
    }

    pub fn decode(decoder: &mut Decoder<'_>, depth: usize) -> Result<Self> {
        let depth = deeper(depth)?;
        Ok(match decoder.tag()? {
            0 => Self::Primitive(JavaPrimitive::from_tag(decoder.tag()?)?),
            1 => {
                let qualified_name = JavaType::decode(decoder)?;
                let count = decoder.count()?;
                let mut arguments = Vec::new();
                for _ in 0..count {
                    arguments.push(JavaTypeArgument::decode(decoder, depth)?);
                }
                Self::Declared {
                    qualified_name,
                    arguments,
                }
            }
            2 => Self::TypeVariable(Name::decode(decoder)?),
            3 => Self::Array(Box::new(Self::decode(decoder, depth)?)),
            other => Err(format!("unknown Java type expression tag {other}"))?,
        })
    }
}

/// One generic argument. Wildcards are three distinct values rather than a
/// bound plus a flag, because `? super T` and `? extends T` are not the same
/// bound in the other direction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum JavaTypeArgument {
    Exact(JavaTypeExpression),
    Extends(JavaTypeExpression),
    Super(JavaTypeExpression),
    Unbounded,
}

impl JavaTypeArgument {
    fn tag(&self) -> u8 {
        match self {
            Self::Exact(_) => 0,
            Self::Extends(_) => 1,
            Self::Super(_) => 2,
            Self::Unbounded => 3,
        }
    }

    pub fn encode(&self, encoder: &mut Encoder, depth: usize) -> Result<()> {
        encoder.tag(self.tag());
        match self {
            Self::Exact(inner) | Self::Extends(inner) | Self::Super(inner) => {
                inner.encode(encoder, depth)
            }
            Self::Unbounded => Ok(()),
        }
    }

    pub fn decode(decoder: &mut Decoder<'_>, depth: usize) -> Result<Self> {
        Ok(match decoder.tag()? {
            0 => Self::Exact(JavaTypeExpression::decode(decoder, depth)?),
            1 => Self::Extends(JavaTypeExpression::decode(decoder, depth)?),
            2 => Self::Super(JavaTypeExpression::decode(decoder, depth)?),
            3 => Self::Unbounded,
            other => Err(format!("unknown Java type argument tag {other}"))?,
        })
    }
}

/// A recursive value carries a checked counter rather than recursing freely:
/// `List<List<List<…>>>` from a hostile record is a stack overflow, which is
/// an abort rather than an error.
fn deeper(depth: usize) -> Result<usize> {
    if depth >= MAX_CODEC_DEPTH {
        return Err(format!(
            "Java type nested deeper than {MAX_CODEC_DEPTH}; the grammar is closed, not unbounded"
        ));
    }
    Ok(depth + 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coordinate::{CanonicalPluginXml, DEFAULT_PLUGIN_GROUP};
    use jails_support::codec::sha256;

    fn coordinate(group: &str, artifact: &str) -> MavenCoordinate {
        MavenCoordinate::parse(group, artifact).unwrap()
    }

    fn path(text: &str) -> ProjectPath {
        ProjectPath::parse(text).unwrap()
    }

    fn java(text: &str) -> JavaType {
        JavaType::parse(text).unwrap()
    }

    fn round_trip(facts: &ProjectFacts) -> ProjectFacts {
        let mut encoder = Encoder::new();
        facts.encode(&mut encoder).unwrap();
        let bytes = encoder.finish().unwrap();
        let mut decoder = Decoder::new(&bytes).unwrap();
        let back = ProjectFacts::decode(&mut decoder).unwrap();
        decoder.finish().unwrap();
        back
    }

    #[test]
    fn a_populated_fact_map_round_trips() {
        let mut facts = ProjectFacts::new();
        facts.observe(
            FactKind::Pom,
            FactSourceState::Present {
                sha256: ObjectId::from_bytes(sha256(b"pom")),
                len: 3,
            },
        );
        facts.observe(FactKind::Compose, FactSourceState::Absent);
        facts
            .record(
                FactKind::Pom,
                ProjectFactKey::MavenDependency(coordinate("org.postgresql", "postgresql")),
                ProjectFact::MavenDependency(crate::coordinate::DependencySpec::managed(
                    coordinate("org.postgresql", "postgresql"),
                )),
            )
            .unwrap();
        facts
            .record(
                FactKind::Pom,
                ProjectFactKey::MavenPlugin(coordinate(
                    DEFAULT_PLUGIN_GROUP,
                    "maven-failsafe-plugin",
                )),
                ProjectFact::MavenPlugin(
                    PluginSpec::new(
                        coordinate(DEFAULT_PLUGIN_GROUP, "maven-failsafe-plugin"),
                        CanonicalPluginXml::parse(
                            "<plugin>\n  <artifactId>maven-failsafe-plugin</artifactId>\n</plugin>",
                        )
                        .unwrap(),
                    )
                    .unwrap(),
                ),
            )
            .unwrap();
        facts
            .record(
                FactKind::JavaSource(path("src/main/java/com/example/demo/domain/Note.java")),
                ProjectFactKey::JavaType(java("com.example.demo.domain.Note")),
                ProjectFact::JavaType(JavaTypeFact {
                    source: path("src/main/java/com/example/demo/domain/Note.java"),
                    kind: JavaTypeKind::Record,
                    supertypes: vec![],
                    constructor: vec![JavaParameterFact {
                        name: Name::parse("tags").unwrap(),
                        type_expression: JavaTypeExpression::Declared {
                            qualified_name: java("java.util.List"),
                            arguments: vec![JavaTypeArgument::Extends(
                                JavaTypeExpression::Declared {
                                    qualified_name: java("java.lang.CharSequence"),
                                    arguments: vec![],
                                },
                            )],
                        },
                    }],
                    enum_constants: vec![],
                }),
            )
            .unwrap();

        assert_eq!(round_trip(&facts), facts);
    }

    /// An absent source is a recorded observation, not a missing entry: it is
    /// what tells "this project has no compose file" from "nobody looked".
    #[test]
    fn an_absent_source_is_recorded_not_omitted() {
        let mut facts = ProjectFacts::new();
        assert_eq!(facts.source(&FactKind::Compose), None);
        facts.observe(FactKind::Compose, FactSourceState::Absent);
        assert_eq!(
            facts.source(&FactKind::Compose),
            Some(FactSourceState::Absent)
        );
    }

    #[test]
    fn a_fact_of_the_wrong_kind_for_its_key_is_refused() {
        let mut facts = ProjectFacts::new();
        let error = facts
            .record(
                FactKind::Compose,
                ProjectFactKey::ComposeService(ServiceName::parse("db").unwrap()),
                ProjectFact::CommandRegistration,
            )
            .unwrap_err();
        assert!(error.contains("does not match key kind"), "{error}");
    }

    /// Two answers to one question. Recording the same value twice is fine —
    /// two parsers agreeing is not a conflict.
    #[test]
    fn a_second_different_value_for_one_key_is_refused() {
        let mut facts = ProjectFacts::new();
        let key = ProjectFactKey::Property {
            path: path("src/main/resources/application.properties"),
            key: PropertyKey::parse("spring.datasource.url").unwrap(),
        };
        facts
            .record(
                FactKind::Properties(path("src/main/resources/application.properties")),
                key.clone(),
                ProjectFact::Property("jdbc:one".to_string()),
            )
            .unwrap();
        facts
            .record(
                FactKind::Properties(path("src/main/resources/application.properties")),
                key.clone(),
                ProjectFact::Property("jdbc:one".to_string()),
            )
            .expect("an identical re-observation is not a conflict");
        let error = facts
            .record(
                FactKind::Properties(path("src/main/resources/application.properties")),
                key,
                ProjectFact::Property("jdbc:two".to_string()),
            )
            .unwrap_err();
        assert!(error.contains("already holds a different value"), "{error}");
    }

    /// A hostile record must not be able to turn a decode into a stack
    /// overflow, which is an abort rather than an error.
    #[test]
    fn a_type_nested_past_the_ceiling_is_refused() {
        let mut expression = JavaTypeExpression::Primitive(JavaPrimitive::Int);
        for _ in 0..MAX_CODEC_DEPTH + 1 {
            expression = JavaTypeExpression::Array(Box::new(expression));
        }
        let mut encoder = Encoder::new();
        let error = expression.encode(&mut encoder, 0).unwrap_err();
        assert!(error.contains("closed, not unbounded"), "{error}");
    }

    /// Without this a deleted POM would leave its dependency facts in place
    /// and every later decision would be made against a file that is gone.
    #[test]
    fn invalidating_one_input_leaves_every_other_input_untouched() {
        let mut facts = ProjectFacts::new();
        let properties = FactKind::Properties(path("src/main/resources/application.properties"));
        facts
            .record(
                FactKind::Pom,
                ProjectFactKey::MavenDependency(coordinate("org.postgresql", "postgresql")),
                ProjectFact::MavenDependency(crate::coordinate::DependencySpec::managed(
                    coordinate("org.postgresql", "postgresql"),
                )),
            )
            .unwrap();
        facts
            .record(
                properties.clone(),
                ProjectFactKey::Property {
                    path: path("src/main/resources/application.properties"),
                    key: PropertyKey::parse("server.port").unwrap(),
                },
                ProjectFact::Property("8080".to_string()),
            )
            .unwrap();

        facts.invalidate(&properties);

        assert_eq!(facts.values().count(), 1);
        assert_eq!(
            facts
                .source_of(&ProjectFactKey::MavenDependency(coordinate(
                    "org.postgresql",
                    "postgresql"
                )))
                .cloned(),
            Some(FactKind::Pom)
        );
    }

    #[test]
    fn an_unknown_fact_tag_is_refused() {
        let mut encoder = Encoder::new();
        encoder.tag(200);
        let bytes = encoder.finish().unwrap();
        let mut decoder = Decoder::new(&bytes).unwrap();
        assert!(ProjectFact::decode(&mut decoder).is_err());
    }
}
