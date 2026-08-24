//! What jails can be asked to own, as identity separate from content.
//!
//! ## The distinction the whole phase turns on
//!
//! plan.md's opening invariant: *"One desired entity has one typed identity,
//! regardless of which command or manifest declared it."* Today identity and
//! content are computed together into a string key, and the two registries
//! that resulted keyed *differently* — which `CLAUDE.md` records as not merely
//! a nearby cause of the §9.7 bug but as the bug itself.
//!
//! So identity is a value here and content is somewhere else. An intent whose
//! `fields` line changed is the **same** entity with new content, which is
//! exactly the input the regenerate-and-merge repair needs.
//!
//! ## Why `package` is not an `Option`
//!
//! `IntentId.package` is the *resolved* package: the convention already
//! applied, `--package ''` already meaning the base package. An `Option` here
//! would put "the user did not say" and "the user said flat" in one slot,
//! which is the ambiguity that made `Option<&str>` versus `""` drift. The
//! syntax DTO keeps `Option<String>` because omission is a user-input fact;
//! it just never reaches this layer.

use crate::Result;
use crate::declaration::{FieldSpec, IntentSpec};
use crate::identity::{JavaType, Name, ObjectId, Package, ProjectPath};
use jails_spec::spec::kind::{ArtifactKind, Capability};
use jails_support::codec::{self, Decoder, Encoder};

mod declared;

pub use declared::{DeclaredId, DeclaredSpec};

/// The internal name for `ArtifactKind`. Clap's spelling stays at the CLI edge.
pub type Recipe = ArtifactKind;

/// One persistent generated intent: `(recipe, name, resolved package)`.
///
/// `Ord` is written rather than derived, and the reason is subtle enough to be
/// worth stating: a derived `Ord` on `Recipe` orders by *declaration position*,
/// so moving a variant in the enum would silently change the canonical sort
/// order and therefore the recorded bytes of every ledger that holds one.
/// Ordering on the label — which is also the recorded spelling — makes the
/// ordering a property of the value rather than of the source file.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct IntentId {
    pub recipe: Recipe,
    pub name: Name,
    /// Convention resolved; never optional here.
    pub package: Package,
}

impl IntentId {
    /// The canonical sort key: the recorded spelling, not the discriminant.
    fn order_key(&self) -> (&'static str, &str, &str) {
        (
            recipe_label(self.recipe),
            self.name.as_str(),
            self.package.as_str(),
        )
    }

    pub fn new(recipe: Recipe, name: Name, package: Package) -> Self {
        Self {
            recipe,
            name,
            package,
        }
    }

    pub fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.string(recipe_label(self.recipe))?;
        self.name.encode(encoder)?;
        self.package.encode(encoder)
    }

    pub fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        let recipe = recipe_from_label(&decoder.string()?)?;
        Ok(Self {
            recipe,
            name: Name::decode(decoder)?,
            package: Package::decode(decoder)?,
        })
    }
}

impl Ord for IntentId {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.order_key().cmp(&other.order_key())
    }
}

impl PartialOrd for IntentId {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// How a capability is identified: most are one per project, some are not.
///
/// The classes come from §R1.1's table and are enforced at construction, so a
/// CLI parameter is never silently ignored: `--name` on a singleton is a
/// refusal, not a no-op that leaves the user believing they named something.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub enum CapabilityInstance {
    Singleton,
    Named { name: Name, package: Package },
}

/// A capability declaration's identity.
///
/// Ordered on the label for the same reason [`IntentId`] is.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct CapabilityId {
    pub kind: Capability,
    pub instance: CapabilityInstance,
}

impl Ord for CapabilityId {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (self.kind.label(), &self.instance).cmp(&(other.kind.label(), &other.instance))
    }
}

impl PartialOrd for CapabilityId {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Which identity rules a capability follows. Data, not a switch: an
/// exhaustive test walks `Capability::value_variants()` so a new capability
/// cannot be added without classifying it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CapabilityClass {
    /// `--name` and `--package` both accepted, both part of identity.
    MultiInstanceNamed,
    /// Identity is the kind; `--package` is mutable placement, `--name` refuses.
    SingletonPlaced,
    /// Identity is the kind; both parameters refuse, the outputs being global.
    SingletonConventional,
}

/// §R1.1's classification table, verbatim.
pub fn capability_class(kind: Capability) -> CapabilityClass {
    use Capability::*;
    use CapabilityClass::*;
    match kind {
        Csv | Sqlite | Json | Http => MultiInstanceNamed,
        Api | Actuator | Cache | Security | Cors | Sse | Mail | Redis | Observability => {
            SingletonPlaced
        }
        Db | Kafka | Testkit | Fake | Format | Coverage | Loadtest | Ci | Docker | K8s
        | Toxiproxy => SingletonConventional,
    }
}

impl CapabilityId {
    /// Build an identity from what the CLI was given, refusing a parameter
    /// this capability class has no meaning for.
    pub fn resolve(
        kind: Capability,
        name: Option<&Name>,
        package: Option<&Package>,
    ) -> Result<Self> {
        match capability_class(kind) {
            CapabilityClass::MultiInstanceNamed => Ok(Self {
                kind,
                instance: CapabilityInstance::Named {
                    name: name.cloned().unwrap_or(default_instance_name(kind)),
                    package: package.cloned().unwrap_or_default(),
                },
            }),
            CapabilityClass::SingletonPlaced => {
                if name.is_some() {
                    return Err(format!(
                        "`{}` is one per project, so `--name` has no meaning for it.\n       \
                         fix: drop `--name`; `--package` does move where it is placed.",
                        kind.label()
                    ));
                }
                Ok(Self {
                    kind,
                    instance: CapabilityInstance::Singleton,
                })
            }
            CapabilityClass::SingletonConventional => {
                if let Some(rejected) = name.map(|_| "--name").or(package.map(|_| "--package")) {
                    return Err(format!(
                        "`{}` writes project-global or conventional output, so `{rejected}` has \
                         no meaning for it.\n       fix: drop `{rejected}`.",
                        kind.label()
                    ));
                }
                Ok(Self {
                    kind,
                    instance: CapabilityInstance::Singleton,
                })
            }
        }
    }

    pub fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.string(self.kind.label())?;
        match &self.instance {
            CapabilityInstance::Singleton => encoder.tag(0),
            CapabilityInstance::Named { name, package } => {
                encoder.tag(1);
                name.encode(encoder)?;
                package.encode(encoder)?;
            }
        }
        Ok(())
    }

    pub fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        let kind = capability_from_label(&decoder.string()?)?;
        let instance = match decoder.tag()? {
            0 => CapabilityInstance::Singleton,
            1 => CapabilityInstance::Named {
                name: Name::decode(decoder)?,
                package: Package::decode(decoder)?,
            },
            other => return Err(format!("unknown capability instance tag {other}")),
        };
        Ok(Self { kind, instance })
    }
}

/// The name a multi-instance capability takes when `--name` is omitted.
fn default_instance_name(kind: Capability) -> Name {
    let label = kind.label();
    let mut chars = label.chars();
    let capitalised = chars
        .next()
        .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
        .unwrap_or_default();
    Name::parse(&capitalised).expect("every capability label is a valid Java identifier")
}

/// A tool-level feature a project can own, distinct from a capability because
/// nothing about it reaches the generated application.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub enum ToolFeature {
    FastTest,
}

impl ToolFeature {
    /// The canonical CLI spelling, which is also the wire form: §R1.4 encodes
    /// a feature as its lowercase name rather than a Rust discriminant, so
    /// reordering the enum cannot change a recorded value.
    pub fn label(self) -> &'static str {
        match self {
            Self::FastTest => "fast-test",
        }
    }

    pub fn parse(text: &str) -> Result<Self> {
        match text {
            "fast-test" => Ok(Self::FastTest),
            other => Err(format!("unknown tool feature `{other}`")),
        }
    }
}

/// Anything that can be owned and reconciled.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub enum EntityId {
    Capability(CapabilityId),
    Intent(IntentId),
    ToolFeature(ToolFeature),
    Declared(DeclaredId),
}

impl EntityId {
    pub fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        match self {
            Self::Capability(id) => {
                encoder.tag(0);
                id.encode(encoder)
            }
            Self::Intent(id) => {
                encoder.tag(1);
                id.encode(encoder)
            }
            Self::ToolFeature(ToolFeature::FastTest) => {
                encoder.tag(2);
                encoder.tag(0);
                Ok(())
            }
            Self::Declared(id) => {
                encoder.tag(3);
                id.encode(encoder)
            }
        }
    }

    pub fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        match decoder.tag()? {
            0 => CapabilityId::decode(decoder).map(Self::Capability),
            1 => IntentId::decode(decoder).map(Self::Intent),
            2 => match decoder.tag()? {
                0 => Ok(Self::ToolFeature(ToolFeature::FastTest)),
                other => Err(format!("unknown tool feature tag {other}")),
            },
            3 => DeclaredId::decode(decoder).map(Self::Declared),
            other => Err(format!("unknown entity tag {other}")),
        }
    }
}

/// Who declared an entity. An entity may have several owners at once, and
/// removing one owner is not removing the entity.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub enum OwnerId {
    /// The one selected app declaration source.
    AppManifest,
    /// A capability declared in `jails.toml`.
    DirectConfig,
    /// Direct `generate`/`destroy` ownership.
    DirectCli,
}

impl OwnerId {
    pub fn tag(self) -> u8 {
        match self {
            Self::AppManifest => 0,
            Self::DirectConfig => 1,
            Self::DirectCli => 2,
        }
    }

    pub fn from_tag(tag: u8) -> Result<Self> {
        match tag {
            0 => Ok(Self::AppManifest),
            1 => Ok(Self::DirectConfig),
            2 => Ok(Self::DirectCli),
            other => Err(format!("unknown owner tag {other}")),
        }
    }
}

// ---------------------------------------------------------------------------
// What an entity was declared to be
// ---------------------------------------------------------------------------

/// A capability's mutable content. Only placement, and only for the singleton
/// class that has one — named identity already carries its package.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CapabilitySpec {
    pub placement: Option<Package>,
}

impl CapabilitySpec {
    pub fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.option(self.placement.as_ref(), |e, package| package.encode(e))
    }

    pub fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(Self {
            placement: decoder.option(Package::decode)?,
        })
    }
}

/// A tool feature's content: the console version it installs.
///
/// `MavenVersion` rather than `ManagedVersion`, and the difference is the
/// whole point. The console launcher's version **must equal the project's own
/// JUnit version**, and a pom that manages that version -- a Spring Boot
/// parent, or an imported `junit-bom` -- must be given **no** version at all:
/// a redundant one pins the launcher while the BOM moves the engine, which is
/// the misalignment that dies at run time with `NoSuchMethodError`. A spec
/// that could only say "pinned X" could not describe the commonest project
/// there is, and recording an invented number would be a claim about bytes
/// jails did not write.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolFeatureSpec {
    pub console_version: crate::coordinate::MavenVersion,
}

impl ToolFeatureSpec {
    pub fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.console_version.encode(encoder)
    }

    pub fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(Self {
            console_version: crate::coordinate::MavenVersion::decode(decoder)?,
        })
    }
}

/// The content half of an entity. Its discriminant must match its identity's —
/// enforced by [`EntitySpec::matches`] wherever the two are paired.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EntitySpec {
    Capability(CapabilitySpec),
    Intent(IntentSpec),
    ToolFeature(ToolFeatureSpec),
    Declared(DeclaredSpec),
}

impl EntitySpec {
    /// Whether this content belongs to that identity.
    ///
    /// plan.md §R3.1 requires this same discriminant equality *"wherever
    /// ID/spec values pair"* — desired entity, applied entity, renderer
    /// context, pending candidate. A matching outer Rust shape is not
    /// sufficient: `EntityId::Capability` beside `EntitySpec::Intent` type-
    /// checks and means nothing.
    /// The intent arm carries one extra check, from §R1.1's argument-shape
    /// amendment: the positional list a spec holds has to be the shape the
    /// identity's recipe takes. `enum Status` paired with record components,
    /// or `record Note` paired with bare names, type-checks and describes an
    /// artifact that cannot be rendered.
    pub fn matches(&self, id: &EntityId) -> bool {
        match (id, self) {
            (EntityId::Intent(intent), Self::Intent(spec)) => {
                spec.arguments.shape() == crate::recipe::argument_shape(intent.recipe)
            }
            // The one arm whose check goes a level deeper, because a declared
            // resource's identity and its content both name the kind. See
            // `DeclaredSpec::matches`.
            (EntityId::Declared(id), Self::Declared(spec)) => spec.matches(id),
            (EntityId::Capability(_), Self::Capability(_))
            | (EntityId::ToolFeature(_), Self::ToolFeature(_)) => true,
            _ => false,
        }
    }

    pub fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        match self {
            Self::Capability(spec) => {
                encoder.tag(0);
                spec.encode(encoder)
            }
            Self::Intent(spec) => {
                encoder.tag(1);
                spec.encode(encoder)
            }
            Self::ToolFeature(spec) => {
                encoder.tag(2);
                spec.encode(encoder)
            }
            Self::Declared(spec) => {
                encoder.tag(3);
                spec.encode(encoder)
            }
        }
    }

    pub fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(match decoder.tag()? {
            0 => Self::Capability(CapabilitySpec::decode(decoder)?),
            1 => Self::Intent(IntentSpec::decode(decoder)?),
            2 => Self::ToolFeature(ToolFeatureSpec::decode(decoder)?),
            3 => Self::Declared(DeclaredSpec::decode(decoder)?),
            other => return Err(format!("unknown entity spec tag {other}")),
        })
    }
}

// ---------------------------------------------------------------------------
// One-shots
// ---------------------------------------------------------------------------

/// What a one-shot field evolution is applied to.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub enum TypeTargetId {
    Managed(IntentId),
    /// A stable fully qualified name. The bytes live in the spec and read set,
    /// never here: a type jails did not generate can move file without
    /// becoming a different type.
    Existing(JavaType),
}

impl TypeTargetId {
    pub fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        match self {
            Self::Managed(id) => {
                encoder.tag(0);
                id.encode(encoder)
            }
            Self::Existing(ty) => {
                encoder.tag(1);
                ty.encode(encoder)
            }
        }
    }

    pub fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(match decoder.tag()? {
            0 => Self::Managed(IntentId::decode(decoder)?),
            1 => Self::Existing(JavaType::decode(decoder)?),
            other => return Err(format!("unknown type target tag {other}")),
        })
    }
}

/// The stable identity of a file supplied to a one-shot import.
///
/// An external file is identified by the hash of its *canonical absolute
/// path*, never by the spelling the user typed. A symlink and its target are
/// one identity; a moved file is a new one even with identical bytes. The
/// absolute string itself stays in runtime bindings and never reaches the
/// ledger, because it means nothing on another machine.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub enum SourceInputId {
    Project(ProjectPath),
    External { path_id: ExternalPathId },
}

/// `SHA256("JAILS-EXTERNAL-PATH-1" || encode(canonical_utf8_path))`.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct ExternalPathId(ObjectId);

impl ExternalPathId {
    /// The one constructor. Every user of an external path identity calls
    /// this; rehashing a display string independently would give the same file
    /// two identities.
    pub fn of_canonical_path(canonical_utf8_path: &str) -> Result<Self> {
        if !canonical_utf8_path.starts_with('/') {
            return Err(format!(
                "`{canonical_utf8_path}` is not a canonical absolute path.\n       fix: resolve \
                 the path before taking its identity, so a symlink and its target agree."
            ));
        }
        let mut encoder = Encoder::new();
        encoder.string(canonical_utf8_path)?;
        Ok(Self(ObjectId::from_bytes(codec::domain_hash(
            "JAILS-EXTERNAL-PATH-1",
            &encoder.finish()?,
        ))))
    }

    pub fn object(&self) -> ObjectId {
        self.0
    }

    pub fn encode(&self, encoder: &mut Encoder) {
        self.0.encode(encoder);
    }

    pub fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(Self(ObjectId::decode(decoder)?))
    }
}

impl SourceInputId {
    pub fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        match self {
            Self::Project(path) => {
                encoder.tag(0);
                path.encode(encoder)
            }
            Self::External { path_id } => {
                encoder.tag(1);
                path_id.0.encode(encoder);
                Ok(())
            }
        }
    }

    pub fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(match decoder.tag()? {
            0 => Self::Project(ProjectPath::decode(decoder)?),
            1 => Self::External {
                path_id: ExternalPathId(ObjectId::decode(decoder)?),
            },
            other => return Err(format!("unknown source input tag {other}")),
        })
    }
}

/// A one-shot operation's identity: stable across a re-run of the same thing.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub enum OneShotId {
    Field { target: TypeTargetId, field: Name },
    Migration { path: ProjectPath },
    Cases { source: SourceInputId },
}

impl OneShotId {
    fn tag(&self) -> u8 {
        match self {
            Self::Field { .. } => 0,
            Self::Migration { .. } => 1,
            Self::Cases { .. } => 2,
        }
    }

    pub fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.tag(self.tag());
        match self {
            Self::Field { target, field } => {
                target.encode(encoder)?;
                field.encode(encoder)
            }
            Self::Migration { path } => path.encode(encoder),
            Self::Cases { source } => source.encode(encoder),
        }
    }

    pub fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(match decoder.tag()? {
            0 => Self::Field {
                target: TypeTargetId::decode(decoder)?,
                field: Name::decode(decoder)?,
            },
            1 => Self::Migration {
                path: ProjectPath::decode(decoder)?,
            },
            2 => Self::Cases {
                source: SourceInputId::decode(decoder)?,
            },
            other => return Err(format!("unknown one-shot id tag {other}")),
        })
    }
}

/// `SHA256("JAILS-CASES-RECEIPT-1" || encode(OneShotId::Cases { source }))`.
///
/// Deliberately excludes source content, output path, receipt operation and
/// every mutable field, so it stays stable across a same-source refresh. That
/// is what makes `destroy cases --receipt <id>` work after the source file has
/// been deleted or moved.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct CasesReceiptId(ObjectId);

impl CasesReceiptId {
    pub fn of(id: &OneShotId) -> Result<Self> {
        if !matches!(id, OneShotId::Cases { .. }) {
            return Err("a cases receipt id is only defined for a cases one-shot".to_string());
        }
        let mut encoder = Encoder::new();
        id.encode(&mut encoder)?;
        Ok(Self(ObjectId::from_bytes(codec::domain_hash(
            "JAILS-CASES-RECEIPT-1",
            &encoder.finish()?,
        ))))
    }

    /// Exactly 64 lowercase hex characters, and rendering what was parsed is
    /// byte-identical.
    pub fn parse_hex(text: &str) -> Result<Self> {
        ObjectId::parse_hex(text).map(Self)
    }

    pub fn to_hex(&self) -> String {
        self.0.to_hex()
    }
}

/// The content half of a one-shot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OneShotSpec {
    Field {
        target: TypeTargetId,
        field: FieldSpec,
    },
    Migration {
        description: String,
        allocated_version: u64,
        path: ProjectPath,
        body: ObjectId,
    },
    Cases {
        source: SourceInputId,
        source_sha256: ObjectId,
        output: ProjectPath,
    },
}

impl OneShotSpec {
    fn tag(&self) -> u8 {
        match self {
            Self::Field { .. } => 0,
            Self::Migration { .. } => 1,
            Self::Cases { .. } => 2,
        }
    }

    /// Whether this content belongs to that identity — discriminants equal
    /// **and** every repeated identity field agreeing.
    ///
    /// The repeated fields are the subtle half. A `Cases` spec carries its own
    /// `source`, and a spec whose source disagreed with its ID would make the
    /// derived `CasesReceiptId` name a row it does not describe.
    pub fn matches(&self, id: &OneShotId) -> bool {
        match (id, self) {
            (
                OneShotId::Field { target, field },
                Self::Field {
                    target: t,
                    field: f,
                },
            ) => target == t && field == &f.name,
            (OneShotId::Migration { path }, Self::Migration { path: p, .. }) => path == p,
            (OneShotId::Cases { source }, Self::Cases { source: s, .. }) => source == s,
            _ => false,
        }
    }

    pub fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.tag(self.tag());
        match self {
            Self::Field { target, field } => {
                target.encode(encoder)?;
                field.encode(encoder)
            }
            Self::Migration {
                description,
                allocated_version,
                path,
                body,
            } => {
                encoder.string(description)?;
                encoder.u64(*allocated_version);
                path.encode(encoder)?;
                body.encode(encoder);
                Ok(())
            }
            Self::Cases {
                source,
                source_sha256,
                output,
            } => {
                source.encode(encoder)?;
                source_sha256.encode(encoder);
                output.encode(encoder)
            }
        }
    }

    pub fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(match decoder.tag()? {
            0 => Self::Field {
                target: TypeTargetId::decode(decoder)?,
                field: FieldSpec::decode(decoder)?,
            },
            1 => Self::Migration {
                description: decoder.string()?,
                allocated_version: decoder.u64()?,
                path: ProjectPath::decode(decoder)?,
                body: ObjectId::decode(decoder)?,
            },
            2 => Self::Cases {
                source: SourceInputId::decode(decoder)?,
                source_sha256: ObjectId::decode(decoder)?,
                output: ProjectPath::decode(decoder)?,
            },
            other => return Err(format!("unknown one-shot spec tag {other}")),
        })
    }
}

// ---------------------------------------------------------------------------
// Label <-> value, through clap's own table
// ---------------------------------------------------------------------------
//
// Deliberately routed through `ValueEnum` rather than a second match: the CLI
// spelling and the recorded spelling must be the same string, or a ledger
// written by one and read by the other disagrees about what a row names.

pub(crate) fn recipe_label(recipe: Recipe) -> &'static str {
    use clap::ValueEnum;
    recipe
        .to_possible_value()
        .expect("every ArtifactKind has a clap value")
        .get_name()
        .to_string()
        .leak()
}

pub(crate) fn recipe_from_label(label: &str) -> Result<Recipe> {
    use clap::ValueEnum;
    Recipe::from_str(label, false).map_err(|_| format!("unknown recipe `{label}`"))
}

pub(crate) fn capability_from_label(label: &str) -> Result<Capability> {
    use clap::ValueEnum;
    Capability::value_variants()
        .iter()
        .find(|candidate| candidate.label() == label)
        .copied()
        .ok_or_else(|| format!("unknown capability `{label}`"))
}

/// `SHA256("JAILS-ENTITY-1" || encode(id))`, for use as a stable map key in
/// formats that cannot hold the structured value.
pub fn entity_digest(id: &EntityId) -> Result<[u8; codec::DIGEST_BYTES]> {
    let mut encoder = Encoder::new();
    id.encode(&mut encoder)?;
    Ok(codec::domain_hash("JAILS-ENTITY-1", &encoder.finish()?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::ValueEnum;

    fn name(text: &str) -> Name {
        Name::parse(text).unwrap()
    }

    fn package(text: &str) -> Package {
        Package::parse(text).unwrap()
    }

    /// The invariant the whole phase turns on: content is not identity.
    ///
    /// This is the distinction `CLAUDE.md` records as *being* the §9.7 bug
    /// rather than a nearby cause — two registries keyed differently, one on
    /// identity and one on identity-plus-arguments, so an edited `fields` line
    /// arrived as a *new* intent against files that already existed.
    #[test]
    fn identity_is_recipe_name_and_package_and_nothing_else() {
        let first = IntentId::new(Recipe::Record, name("Note"), package("com.example.domain"));
        let same = IntentId::new(Recipe::Record, name("Note"), package("com.example.domain"));
        assert_eq!(first, same);

        for different in [
            IntentId::new(Recipe::Value, name("Note"), package("com.example.domain")),
            IntentId::new(Recipe::Record, name("Memo"), package("com.example.domain")),
            IntentId::new(Recipe::Record, name("Note"), package("com.example.web")),
            IntentId::new(Recipe::Record, name("Note"), Package::base()),
        ] {
            assert_ne!(first, different, "{different:?}");
        }
    }

    /// A derived `Ord` would sort by declaration position, so moving a variant
    /// in `ArtifactKind` would silently change every recorded ledger's bytes.
    #[test]
    fn ordering_follows_the_recorded_spelling_not_the_enum_order() {
        let mut ids = [
            IntentId::new(Recipe::Scaffold, name("A"), Package::base()),
            IntentId::new(Recipe::Record, name("A"), Package::base()),
        ];
        ids.sort();
        assert_eq!(
            recipe_label(ids[0].recipe),
            "record",
            "`record` sorts before `scaffold` alphabetically, though `scaffold` \
             is declared first"
        );
        assert!(
            Recipe::value_variants()
                .iter()
                .position(|k| *k == Recipe::Scaffold)
                < Recipe::value_variants()
                    .iter()
                    .position(|k| *k == Recipe::Record),
            "the premise: Scaffold really is declared first"
        );
    }

    /// A capability parameter is never silently ignored.
    #[test]
    fn a_parameter_a_capability_has_no_meaning_for_is_refused() {
        // Multi-instance: both accepted, both part of identity.
        let csv = CapabilityId::resolve(
            Capability::Csv,
            Some(&name("Dataset")),
            Some(&package("io.example.imports")),
        )
        .unwrap();
        assert!(matches!(csv.instance, CapabilityInstance::Named { .. }));
        assert_ne!(
            csv,
            CapabilityId::resolve(Capability::Csv, Some(&name("Other")), None).unwrap()
        );

        // Singleton placed: `--package` moves it, `--name` refuses.
        assert!(CapabilityId::resolve(Capability::Actuator, None, Some(&package("x"))).is_ok());
        let error =
            CapabilityId::resolve(Capability::Actuator, Some(&name("X")), None).unwrap_err();
        assert!(error.contains("one per project"), "{error}");
        assert!(error.contains("fix:"), "{error}");

        // Singleton conventional: both refuse, the output being project-global.
        for (n, p) in [(Some(&name("X")), None), (None, Some(&package("x")))] {
            let error = CapabilityId::resolve(Capability::Db, n, p).unwrap_err();
            assert!(error.contains("project-global or conventional"), "{error}");
        }
        assert!(CapabilityId::resolve(Capability::Db, None, None).is_ok());
    }

    /// Classification is data, and a capability added without a thought for it
    /// fails to compile rather than falling into a default.
    #[test]
    fn every_capability_is_classified() {
        for kind in Capability::value_variants() {
            let class = capability_class(*kind);
            let resolved = match class {
                CapabilityClass::MultiInstanceNamed => {
                    CapabilityId::resolve(*kind, Some(&name("X")), Some(&package("p")))
                }
                CapabilityClass::SingletonPlaced => {
                    CapabilityId::resolve(*kind, None, Some(&package("p")))
                }
                CapabilityClass::SingletonConventional => CapabilityId::resolve(*kind, None, None),
            };
            assert!(resolved.is_ok(), "{:?}: {resolved:?}", kind.label());
        }
    }

    /// A multi-instance capability with no `--name` still has a name, so two
    /// default instances of different kinds are still different entities.
    #[test]
    fn a_default_instance_name_is_derived_not_absent() {
        let csv = CapabilityId::resolve(Capability::Csv, None, None).unwrap();
        match &csv.instance {
            CapabilityInstance::Named { name, package } => {
                assert_eq!(name.as_str(), "Csv");
                assert!(package.is_base());
            }
            other => panic!("expected a named instance, found {other:?}"),
        }
        assert_ne!(
            csv,
            CapabilityId::resolve(Capability::Json, None, None).unwrap()
        );
    }

    #[test]
    fn every_entity_id_round_trips_through_the_codec() {
        let entities = [
            EntityId::Intent(IntentId::new(
                Recipe::Scaffold,
                name("Note"),
                package("com.example.demo.domain"),
            )),
            EntityId::Intent(IntentId::new(Recipe::Record, name("Flat"), Package::base())),
            EntityId::Capability(CapabilityId::resolve(Capability::Db, None, None).unwrap()),
            EntityId::Capability(
                CapabilityId::resolve(Capability::Csv, Some(&name("Dataset")), None).unwrap(),
            ),
            EntityId::ToolFeature(ToolFeature::FastTest),
        ];
        for entity in &entities {
            let mut encoder = Encoder::new();
            entity.encode(&mut encoder).unwrap();
            let bytes = encoder.finish().unwrap();

            let mut decoder = Decoder::new(&bytes).unwrap();
            assert_eq!(&EntityId::decode(&mut decoder).unwrap(), entity);
            decoder.finish().unwrap();
        }
    }

    /// An unknown tag rejects rather than being skipped, so a ledger from a
    /// newer jails is refused instead of half-read.
    #[test]
    fn an_unknown_tag_rejects() {
        let mut decoder = Decoder::new(&[9]).unwrap();
        assert!(
            EntityId::decode(&mut decoder)
                .unwrap_err()
                .contains("unknown entity tag")
        );

        let mut encoder = Encoder::new();
        encoder.string("no-such-recipe").unwrap();
        let bytes = encoder.finish().unwrap();
        let mut decoder = Decoder::new(&bytes).unwrap();
        assert!(
            IntentId::decode(&mut decoder)
                .unwrap_err()
                .contains("unknown recipe")
        );
    }

    /// Two entities that differ only in owner-invisible ways still differ, and
    /// equal entities hash equal — which is what makes the digest usable as a
    /// key in a format that cannot hold the structured value.
    #[test]
    fn the_entity_digest_is_a_function_of_the_identity() {
        let one = EntityId::Intent(IntentId::new(Recipe::Record, name("A"), Package::base()));
        let same = EntityId::Intent(IntentId::new(Recipe::Record, name("A"), Package::base()));
        let other = EntityId::Intent(IntentId::new(Recipe::Record, name("B"), Package::base()));

        assert_eq!(entity_digest(&one).unwrap(), entity_digest(&same).unwrap());
        assert_ne!(entity_digest(&one).unwrap(), entity_digest(&other).unwrap());
    }

    /// The recorded spelling and the CLI spelling are the same string. A
    /// second match table here is how a ledger written by one binary and read
    /// by another disagrees about what a row names.
    #[test]
    fn labels_round_trip_through_claps_own_table() {
        for kind in Recipe::value_variants() {
            let label = recipe_label(*kind);
            assert_eq!(recipe_from_label(label).unwrap(), *kind, "{label}");
        }
        for capability in Capability::value_variants() {
            let label = capability.label();
            assert_eq!(
                capability_from_label(label).unwrap(),
                *capability,
                "{label}"
            );
        }
    }

    #[test]
    fn owner_tags_are_stable_and_exhaustive() {
        for owner in [
            OwnerId::AppManifest,
            OwnerId::DirectConfig,
            OwnerId::DirectCli,
        ] {
            assert_eq!(OwnerId::from_tag(owner.tag()).unwrap(), owner);
        }
        assert!(
            OwnerId::from_tag(3)
                .unwrap_err()
                .contains("unknown owner tag")
        );
    }
}
