//! What the user asked for, as a value that survives a crash.
//!
//! ## Why a command needs a fingerprint at all
//!
//! When a reconciliation stops half-way with a stored conflict, the next run
//! has to answer one question: *is this the same command?* Re-parsing the
//! project to find out is not available — the project is exactly what is in an
//! uncertain state, and marker-bearing files may be mid-merge. So the command
//! is projected to a canonical syntax value at the CLI edge, before any
//! project-derived default is consulted, and hashed.
//!
//! ## What the projection deliberately excludes
//!
//! plan.md §R3.1: presentation and debug flags, and `--abort-conflict`. Those
//! change how a run *reports*, not what it does, and including them would make
//! `--debug` look like a different command from the one that stalled. Raw
//! argv, secrets and display text never enter it either — which is a rule
//! about the future as much as the present: a secret-bearing option cannot
//! join this projection without an explicit redacted representation and a
//! protocol-version decision.
//!
//! ## Sets sort, sequences do not
//!
//! `jails add db kafka` and `jails add kafka db` are the same request, so
//! capability positions sort. Field and index order is semantic — a record's
//! components have an order, and a composite index on `(a, b)` is not the one
//! on `(b, a)` — so those positions are preserved exactly. Getting this
//! backwards in either direction produces a wrong answer rather than an error:
//! sorting an ordered position silently accepts a different command as the
//! same one.

use crate::Result;
use crate::entity::{
    CapabilityId, CapabilityInstance, CapabilitySpec, EntityId, EntitySpec, ExternalPathId,
    OneShotId, OneShotSpec, ToolFeature,
};
use crate::identity::{JavaType, ObjectId, ProjectPath};
use jails_support::codec::{self, Codec, Decoder, Encoder, ordered};
use std::collections::{BTreeMap, BTreeSet};

/// The canonical projection of one command line.
///
/// Built from canonical command and option names *after* alias resolution, so
/// two spellings the CLI promises are equivalent produce identical bytes.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CanonicalRequestSyntaxV1 {
    /// The command and subcommand components, without leading dashes.
    pub command_path: Vec<String>,
    /// Validated UTF-8 lexical values. Set-semantic positions arrive sorted;
    /// ordered positions arrive exactly as written.
    pub positionals: Vec<String>,
    /// Only explicitly supplied semantic options, keyed without leading
    /// dashes. Repeated values keep their order unless the option is a set.
    pub options: BTreeMap<String, Vec<String>>,
    /// Only explicitly supplied semantic flags.
    pub flags: BTreeSet<String>,
}

/// The hash of that projection.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct RequestSyntaxFingerprint(ObjectId);

impl RequestSyntaxFingerprint {
    pub fn object(&self) -> ObjectId {
        self.0
    }

    pub fn to_hex(&self) -> String {
        self.0.to_hex()
    }
}

mod lifecycle;
pub use lifecycle::*;
impl Codec for RequestSyntaxFingerprint {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.0.encode(encoder)?;
        Ok(())
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        ObjectId::decode(decoder).map(Self)
    }
}

impl CanonicalRequestSyntaxV1 {
    /// `SHA256("JAILS-REQUEST-SYNTAX-1" || encode(self))`, exactly.
    pub fn fingerprint(&self) -> Result<RequestSyntaxFingerprint> {
        let mut encoder = Encoder::new();
        self.encode(&mut encoder)?;
        Ok(RequestSyntaxFingerprint(ObjectId::from_bytes(
            codec::domain_hash("JAILS-REQUEST-SYNTAX-1", &encoder.finish()?),
        )))
    }

    /// Flags and options this projection must never carry.
    ///
    /// They change how a run reports rather than what it does, so including
    /// one would make `--debug` look like a different command from the one
    /// that stalled — and a stored conflict would refuse to recognise its own
    /// rerun.
    pub const EXCLUDED: &'static [&'static str] = &[
        "debug",
        "output",
        "json",
        "quiet",
        "verbose",
        "abort-conflict",
    ];

    /// Whether a flag or option name belongs in the projection at all.
    pub fn is_semantic(name: &str) -> bool {
        !Self::EXCLUDED.contains(&name)
    }
}
impl Codec for CanonicalRequestSyntaxV1 {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.count(self.command_path.len())?;
        for part in &self.command_path {
            reject_dashes(part)?;
            encoder.string(part)?;
        }
        // A sequence, not a set: order is preserved and duplicates are legal.
        encoder.count(self.positionals.len())?;
        for value in &self.positionals {
            encoder.string(value)?;
        }
        encoder.count(self.options.len())?;
        let mut previous: Option<&String> = None;
        for (key, values) in &self.options {
            ordered(previous, key)?;
            previous = Some(key);
            reject_dashes(key)?;
            encoder.string(key)?;
            encoder.count(values.len())?;
            for value in values {
                encoder.string(value)?;
            }
        }
        encoder.count(self.flags.len())?;
        let mut previous: Option<&String> = None;
        for flag in &self.flags {
            ordered(previous, flag)?;
            previous = Some(flag);
            reject_dashes(flag)?;
            encoder.string(flag)?;
        }
        Ok(())
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        let mut command_path = Vec::new();
        for _ in 0..decoder.count()? {
            let part = decoder.string()?;
            reject_dashes(&part)?;
            command_path.push(part);
        }
        let mut positionals = Vec::new();
        for _ in 0..decoder.count()? {
            positionals.push(decoder.string()?);
        }
        let mut options = BTreeMap::new();
        let mut previous: Option<String> = None;
        for _ in 0..decoder.count()? {
            let key = decoder.string()?;
            ordered(previous.as_ref(), &key)?;
            previous = Some(key.clone());
            reject_dashes(&key)?;
            let mut values = Vec::new();
            for _ in 0..decoder.count()? {
                values.push(decoder.string()?);
            }
            options.insert(key, values);
        }
        let mut flags = BTreeSet::new();
        let mut previous: Option<String> = None;
        for _ in 0..decoder.count()? {
            let flag = decoder.string()?;
            ordered(previous.as_ref(), &flag)?;
            previous = Some(flag.clone());
            reject_dashes(&flag)?;
            flags.insert(flag);
        }
        Ok(Self {
            command_path,
            positionals,
            options,
            flags,
        })
    }
}

/// Keys arrive already stripped, so a leading dash means a caller skipped the
/// canonicalisation step and `--force` and `force` would hash differently.
fn reject_dashes(value: &str) -> Result<()> {
    if value.starts_with('-') {
        return Err(format!(
            "`{value}` still has a leading dash; the canonical projection stores names without \
             one, or `--force` and `force` would be two different commands"
        )
        .into());
    }
    if value.is_empty() {
        return Err(jails_support::Failure::Told(
            "a command, option or flag name is empty".to_string(),
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// The canonical request
// ---------------------------------------------------------------------------

/// One capability declaration, identity and content together.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CanonicalCapability {
    pub id: CapabilityId,
    pub spec: CapabilitySpec,
}

/// What `generate` was asked to produce.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CanonicalGenerateRequest {
    Entity { id: EntityId, spec: EntitySpec },
    OneShot { id: OneShotId, spec: OneShotSpec },
}

/// What a `destroy` names.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ChangeSubject {
    Entity(EntityId),
    OneShot(OneShotId),
}

/// Every mutation the CLI can express, after aliases and defaults.
///
/// The constructors below enforce plan.md §R3.1's closed admissibility matrix.
/// That matrix exists because *a matching outer Rust shape is not sufficient*:
/// `EntityId::Capability` paired with `EntitySpec::Intent` type-checks and is
/// meaningless, and a `Cases` spec whose source disagreed with its ID would
/// make the derived receipt name a row it does not describe. Checking at
/// construction means no downstream stage has to re-check, and none of them
/// can forget.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CanonicalMutationRequest {
    Add {
        capabilities: Vec<CanonicalCapability>,
        no_start: bool,
    },
    Remove {
        capabilities: Vec<CanonicalCapability>,
        force: bool,
        no_start: bool,
    },
    Sync {
        no_start: bool,
    },
    Generate(CanonicalGenerateRequest),
    Destroy {
        subject: ChangeSubject,
        force: bool,
    },
    AppInit {
        target: ProjectPath,
    },
    AppApply {
        no_start: bool,
    },
    Rename {
        from: JavaType,
        to: JavaType,
        force: bool,
    },
    AdoptLayout,
    FastTest,
    Format {
        scopes: BTreeSet<ProjectPath>,
    },
    RemoveToolFeature {
        feature: ToolFeature,
        force: bool,
    },
    /// `add dependency` / `set`: one resource the reader named, with the
    /// content they asked for.
    ///
    /// One request for both because they are one operation over
    /// [`crate::entity::DeclaredId`], and splitting them by the *kind* of
    /// resource would put the same reconciliation in two places -- which is
    /// how `add` and `remove` came to disagree about the manifest in V1.
    Declare {
        id: crate::entity::DeclaredId,
        spec: crate::entity::DeclaredSpec,
    },
    /// `remove dependency` / `unset`: give that ownership up.
    Undeclare {
        id: crate::entity::DeclaredId,
        force: bool,
    },
    DestroyResource {
        subject: EntityId,
        storage: StorageRetirement,
        force: bool,
    },
    EvolveField(EvolveFieldRequestV1),
    ReviveResource(ReviveResourceRequestV1),
    RepairResource(RepairResourceRequestV1),
    DestroyResourceV2 {
        request: DestroyResourceRequestV2,
        force: bool,
    },
    SqlGenerate {
        queries: BTreeSet<crate::database::QueryId>,
    },
    ContractEmit {
        target: ProjectPath,
        json_schema: bool,
    },
}

impl CanonicalMutationRequest {
    /// `add` / `remove`: a non-empty, sorted, duplicate-free capability list.
    ///
    /// Empty rejects rather than succeeding as a no-op, because `jails add`
    /// with nothing to add is a mistake the user should hear about.
    pub fn capabilities(rows: Vec<CanonicalCapability>) -> Result<Vec<CanonicalCapability>> {
        if rows.is_empty() {
            return Err(jails_support::Failure::Told(
                "no capability named.\n       fix: name at least one.".to_string(),
            ));
        }
        let mut previous: Option<&CapabilityId> = None;
        for row in &rows {
            ordered(previous, &row.id)?;
            previous = Some(&row.id);
            // A singleton's placement lives in its spec; a named instance
            // already carries its package in its identity, so a spec that also
            // carried one would be a second authority for the same fact.
            if matches!(row.id.instance, CapabilityInstance::Named { .. })
                && row.spec.placement.is_some()
            {
                return Err(format!(
                    "`{}` carries its package in its identity, so its spec may not also name \
                     one",
                    row.id.kind.label()
                )
                .into());
            }
        }
        Ok(rows)
    }

    /// `generate <kind>`: a persistent intent, never a capability or tool
    /// feature — those have their own requests.
    pub fn generate_entity(id: EntityId, spec: EntitySpec) -> Result<Self> {
        if !matches!(id, EntityId::Intent(_)) {
            return Err(jails_support::Failure::Told(
                "`generate` produces a persistent intent.\n       fix: a capability is `jails \
                 add`, and the fast-test feature is `jails test --fast`."
                    .to_string(),
            ));
        }
        if !spec.matches(&id) {
            return Err(jails_support::Failure::Told(
                "this generate request pairs an identity and a spec of different kinds".to_string(),
            ));
        }
        Ok(Self::Generate(CanonicalGenerateRequest::Entity {
            id,
            spec,
        }))
    }

    /// `generate field|migration|cases`: discriminants equal and every
    /// repeated identity field agreeing.
    pub fn generate_one_shot(id: OneShotId, spec: OneShotSpec) -> Result<Self> {
        if !spec.matches(&id) {
            return Err(jails_support::Failure::Told(
                "this one-shot request pairs an identity and a spec that disagree.\n       fix: \
                 their kinds and their repeated target, path or source must be the same value."
                    .to_string(),
            ));
        }
        Ok(Self::Generate(CanonicalGenerateRequest::OneShot {
            id,
            spec,
        }))
    }

    /// `destroy <kind> <name>`: a persistent intent only.
    pub fn destroy_entity(id: EntityId, force: bool) -> Result<Self> {
        if !matches!(id, EntityId::Intent(_)) {
            return Err(jails_support::Failure::Told(
                "`destroy` removes a persistent intent.\n       fix: a capability is `jails \
                 remove`, and the fast-test feature is `jails remove fast-test`."
                    .to_string(),
            ));
        }
        Ok(Self::Destroy {
            subject: ChangeSubject::Entity(id),
            force,
        })
    }

    pub fn destroy_resource(id: EntityId, storage: StorageRetirement, force: bool) -> Result<Self> {
        if !matches!(id, EntityId::Intent(_)) {
            return Err(jails_support::Failure::Told(
                "resource retirement removes a persistent intent.\n       fix: use `jails \
                 remove` for a capability or tool feature."
                    .to_string(),
            ));
        }
        Ok(Self::DestroyResource {
            subject: id,
            storage,
            force,
        })
    }

    /// `destroy cases`: the only one-shot with a destroy route.
    ///
    /// A field or migration has none by design — a field cannot be un-added
    /// from a record whose other overlays depend on it, and a migration is
    /// append-only because the database has already run it.
    pub fn destroy_one_shot(id: OneShotId, force: bool) -> Result<Self> {
        match id {
            OneShotId::Cases { .. } => Ok(Self::Destroy {
                subject: ChangeSubject::OneShot(id),
                force,
            }),
            OneShotId::Field { .. } => Err(jails_support::Failure::Told(
                "a field has no destroy route.\n       fix: a later render reapplies every \
                 active overlay, so removing one in isolation would leave the others \
                 inconsistent."
                    .to_string(),
            )),
            OneShotId::Migration { .. } => Err(jails_support::Failure::Told(
                "a migration is append-only.\n       fix: the database has already run it; \
                 write a forward migration instead."
                    .to_string(),
            )),
        }
    }

    /// `remove fast-test`: exactly the one feature that exists.
    ///
    /// A future feature needs a protocol and CLI addition rather than falling
    /// through here, which is why this takes the value and still checks it.
    pub fn remove_tool_feature(feature: ToolFeature, force: bool) -> Result<Self> {
        match feature {
            ToolFeature::FastTest => Ok(Self::RemoveToolFeature { feature, force }),
        }
    }

    /// `add dependency` / `set`: identity and content must describe the same
    /// kind of resource.
    ///
    /// The same rule `EntitySpec::matches` carries, applied at the request
    /// boundary so a journal cannot replay a claim that names an artifact and
    /// carries a property value. It type-checks; it writes nothing.
    pub fn declare(
        id: crate::entity::DeclaredId,
        spec: crate::entity::DeclaredSpec,
    ) -> Result<Self> {
        let entity = crate::entity::EntityId::Declared(id.clone());
        if !crate::entity::EntitySpec::Declared(spec.clone()).matches(&entity) {
            return Err(jails_support::Failure::Told(
                "the resource being declared and the content declared for it are different \
                 things"
                    .to_string(),
            ));
        }
        Ok(Self::Declare { id, spec })
    }

    /// Which variant this is, for the fixed request tags.
    pub fn tag(&self) -> u8 {
        match self {
            Self::Add { .. } => 0,
            Self::Remove { .. } => 1,
            Self::Sync { .. } => 2,
            Self::Generate(_) => 3,
            Self::Destroy { .. } => 4,
            Self::AppInit { .. } => 5,
            Self::AppApply { .. } => 6,
            Self::Rename { .. } => 7,
            Self::AdoptLayout => 8,
            Self::FastTest => 10,
            Self::Format { .. } => 11,
            Self::RemoveToolFeature { .. } => 12,
            Self::Declare { .. } => 13,
            Self::Undeclare { .. } => 14,
            Self::DestroyResource { .. } => 15,
            Self::EvolveField(_) => 16,
            Self::ReviveResource(_) => 17,
            Self::RepairResource(_) => 18,
            Self::DestroyResourceV2 { .. } => 19,
            Self::SqlGenerate { .. } => 20,
            Self::ContractEmit { .. } => 21,
        }
    }
}

impl Codec for CanonicalMutationRequest {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.tag(self.tag());
        match self {
            Self::Add {
                capabilities,
                no_start,
            } => {
                encode_capabilities(encoder, capabilities)?;
                encoder.bool(*no_start);
            }
            Self::Remove {
                capabilities,
                force,
                no_start,
            } => {
                encode_capabilities(encoder, capabilities)?;
                encoder.bool(*force);
                encoder.bool(*no_start);
            }
            Self::Sync { no_start } | Self::AppApply { no_start } => encoder.bool(*no_start),
            Self::Generate(request) => request.encode(encoder)?,
            Self::Destroy { subject, force } => {
                subject.encode(encoder)?;
                encoder.bool(*force);
            }
            Self::AppInit { target } => target.encode(encoder)?,
            Self::Rename { from, to, force } => {
                from.encode(encoder)?;
                to.encode(encoder)?;
                encoder.bool(*force);
            }
            Self::AdoptLayout | Self::FastTest => {}
            Self::Format { scopes } => {
                encoder.set(scopes)?;
            }
            Self::RemoveToolFeature { feature, force } => {
                encoder.string(feature.label())?;
                encoder.bool(*force);
            }
            Self::Declare { id, spec } => {
                id.encode(encoder)?;
                spec.encode(encoder)?;
            }
            Self::Undeclare { id, force } => {
                id.encode(encoder)?;
                encoder.bool(*force);
            }
            Self::DestroyResource {
                subject,
                storage,
                force,
            } => {
                subject.encode(encoder)?;
                storage.encode(encoder)?;
                encoder.bool(*force);
            }
            Self::EvolveField(request) => request.encode(encoder)?,
            Self::ReviveResource(request) => request.encode(encoder)?,
            Self::RepairResource(request) => request.encode(encoder)?,
            Self::DestroyResourceV2 { request, force } => {
                request.encode(encoder)?;
                encoder.bool(*force);
            }
            Self::SqlGenerate { queries } => encoder.set(queries)?,
            Self::ContractEmit {
                target,
                json_schema,
            } => {
                target.encode(encoder)?;
                encoder.bool(*json_schema);
            }
        }
        Ok(())
    }

    /// Every route back in goes through the same constructor the CLI uses, so
    /// a request recovered from a journal is checked exactly as a typed one.
    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(match decoder.tag()? {
            0 => Self::Add {
                capabilities: Self::capabilities(decode_capabilities(decoder)?)?,
                no_start: decoder.bool()?,
            },
            1 => Self::Remove {
                capabilities: Self::capabilities(decode_capabilities(decoder)?)?,
                force: decoder.bool()?,
                no_start: decoder.bool()?,
            },
            2 => Self::Sync {
                no_start: decoder.bool()?,
            },
            3 => match CanonicalGenerateRequest::decode(decoder)? {
                CanonicalGenerateRequest::Entity { id, spec } => Self::generate_entity(id, spec)?,
                CanonicalGenerateRequest::OneShot { id, spec } => {
                    Self::generate_one_shot(id, spec)?
                }
            },
            4 => match ChangeSubject::decode(decoder)? {
                ChangeSubject::Entity(id) => Self::destroy_entity(id, decoder.bool()?)?,
                ChangeSubject::OneShot(id) => Self::destroy_one_shot(id, decoder.bool()?)?,
            },
            5 => Self::AppInit {
                target: ProjectPath::decode(decoder)?,
            },
            6 => Self::AppApply {
                no_start: decoder.bool()?,
            },
            7 => Self::Rename {
                from: JavaType::decode(decoder)?,
                to: JavaType::decode(decoder)?,
                force: decoder.bool()?,
            },
            8 => Self::AdoptLayout,
            10 => Self::FastTest,
            11 => {
                let scopes: BTreeSet<ProjectPath> = decoder.set()?;
                Self::Format { scopes }
            }
            12 => {
                let feature = ToolFeature::parse(&decoder.string()?)?;
                Self::remove_tool_feature(feature, decoder.bool()?)?
            }
            13 => Self::declare(
                crate::entity::DeclaredId::decode(decoder)?,
                crate::entity::DeclaredSpec::decode(decoder)?,
            )?,
            14 => Self::Undeclare {
                id: crate::entity::DeclaredId::decode(decoder)?,
                force: decoder.bool()?,
            },
            15 => Self::destroy_resource(
                EntityId::decode(decoder)?,
                StorageRetirement::decode(decoder)?,
                decoder.bool()?,
            )?,
            16 => Self::EvolveField(EvolveFieldRequestV1::decode(decoder)?),
            17 => Self::ReviveResource(ReviveResourceRequestV1::decode(decoder)?),
            18 => Self::RepairResource(RepairResourceRequestV1::decode(decoder)?),
            19 => Self::DestroyResourceV2 {
                request: DestroyResourceRequestV2::decode(decoder)?,
                force: decoder.bool()?,
            },
            20 => Self::SqlGenerate {
                queries: decoder.set()?,
            },
            21 => Self::ContractEmit {
                target: ProjectPath::decode(decoder)?,
                json_schema: decoder.bool()?,
            },
            other => Err(format!("unknown mutation request tag {other}"))?,
        })
    }
}

fn encode_capabilities(encoder: &mut Encoder, rows: &[CanonicalCapability]) -> Result<()> {
    encoder.count(rows.len())?;
    for row in rows {
        row.id.encode(encoder)?;
        row.spec.encode(encoder)?;
    }
    Ok(())
}

fn decode_capabilities(decoder: &mut Decoder<'_>) -> Result<Vec<CanonicalCapability>> {
    let count = decoder.count()?;
    let mut rows = Vec::new();
    for _ in 0..count {
        rows.push(CanonicalCapability {
            id: CapabilityId::decode(decoder)?,
            spec: CapabilitySpec::decode(decoder)?,
        });
    }
    Ok(rows)
}

impl Codec for CanonicalGenerateRequest {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        match self {
            Self::Entity { id, spec } => {
                encoder.tag(0);
                id.encode(encoder)?;
                spec.encode(encoder)
            }
            Self::OneShot { id, spec } => {
                encoder.tag(1);
                id.encode(encoder)?;
                spec.encode(encoder)
            }
        }
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(match decoder.tag()? {
            0 => Self::Entity {
                id: EntityId::decode(decoder)?,
                spec: EntitySpec::decode(decoder)?,
            },
            1 => Self::OneShot {
                id: OneShotId::decode(decoder)?,
                spec: OneShotSpec::decode(decoder)?,
            },
            other => Err(format!("unknown generate request tag {other}"))?,
        })
    }
}

impl Codec for ChangeSubject {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
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

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(match decoder.tag()? {
            0 => Self::Entity(EntityId::decode(decoder)?),
            1 => Self::OneShot(OneShotId::decode(decoder)?),
            other => Err(format!("unknown change subject tag {other}"))?,
        })
    }
}

/// Where this invocation's app manifest came from.
///
/// §R1.1: *"Neither becomes an `OwnerId`, and switching manifest paths cannot
/// leave a hidden second app owner."* There is one app-manifest namespace; the
/// source is an input fact, recorded in the fingerprint rather than in the
/// ownership model.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ManifestSourceId {
    Project(ProjectPath),
    External { path_id: ExternalPathId },
}

impl Codec for ManifestSourceId {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        match self {
            Self::Project(path) => {
                encoder.tag(0);
                path.encode(encoder)
            }
            Self::External { path_id } => {
                encoder.tag(1);
                path_id.encode(encoder)?;
                Ok(())
            }
        }
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(match decoder.tag()? {
            0 => Self::Project(ProjectPath::decode(decoder)?),
            1 => Self::External {
                path_id: ExternalPathId::decode(decoder)?,
            },
            other => Err(format!("unknown manifest source tag {other}"))?,
        })
    }
}

/// Everything that decides whether two runs are *the same invocation*.
///
/// The four fields are the four independent ways two runs can differ: what was
/// typed, what that resolved to, which manifest was selected, and what that
/// manifest and the other declaration inputs contained. plan.md §R5.4 makes
/// structural equality of all four the test a conflict resume applies — so a
/// field left out here is a way for a resume to mistake one request for
/// another, which is the whole failure this value exists to prevent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvocationFingerprint {
    pub request_syntax: RequestSyntaxFingerprint,
    pub request: CanonicalMutationRequest,
    pub manifest_source: Option<ManifestSourceId>,
    pub desired_input_sha256: ObjectId,
}

impl Codec for InvocationFingerprint {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.request_syntax.encode(encoder)?;
        self.request.encode(encoder)?;
        encoder.option(self.manifest_source.as_ref(), |e, source| source.encode(e))?;
        self.desired_input_sha256.encode(encoder)?;
        Ok(())
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(Self {
            request_syntax: RequestSyntaxFingerprint::decode(decoder)?,
            request: CanonicalMutationRequest::decode(decoder)?,
            manifest_source: decoder.option(ManifestSourceId::decode)?,
            desired_input_sha256: ObjectId::decode(decoder)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn syntax() -> CanonicalRequestSyntaxV1 {
        CanonicalRequestSyntaxV1 {
            command_path: vec!["add".to_string()],
            positionals: vec!["db".to_string(), "kafka".to_string()],
            options: BTreeMap::from([("package".to_string(), vec!["com.example".to_string()])]),
            flags: BTreeSet::from(["no-start".to_string()]),
        }
    }

    #[test]
    fn a_projection_round_trips_and_its_fingerprint_is_stable() {
        let one = syntax();
        let mut encoder = Encoder::new();
        one.encode(&mut encoder).unwrap();
        let bytes = encoder.finish().unwrap();

        let mut decoder = Decoder::new(&bytes).unwrap();
        assert_eq!(CanonicalRequestSyntaxV1::decode(&mut decoder).unwrap(), one);
        decoder.finish().unwrap();

        assert_eq!(one.fingerprint().unwrap(), syntax().fingerprint().unwrap());
    }

    /// `jails add db kafka` and `jails add kafka db` are the same request. The
    /// *caller* sorts a set-semantic position; this test pins that once sorted
    /// they are indistinguishable, which is the property the sort exists for.
    #[test]
    fn a_set_semantic_position_is_the_same_request_in_either_order() {
        let mut written_one_way = syntax();
        written_one_way.positionals = vec!["kafka".to_string(), "db".to_string()];
        written_one_way.positionals.sort();
        assert_eq!(
            written_one_way.fingerprint().unwrap(),
            syntax().fingerprint().unwrap()
        );
    }

    /// Field and index order is semantic, so an ordered position must *not* be
    /// collapsed. Sorting one would silently accept a different command as the
    /// same one — a wrong answer rather than an error.
    #[test]
    fn an_ordered_position_is_not_collapsed() {
        let mut one = syntax();
        one.positionals = vec!["a:string".to_string(), "b:int".to_string()];
        let mut other = syntax();
        other.positionals = vec!["b:int".to_string(), "a:string".to_string()];
        assert_ne!(one.fingerprint().unwrap(), other.fingerprint().unwrap());

        // A repeated value is legal in a sequence, and it changes the request.
        let mut repeated = syntax();
        repeated.positionals = vec!["a:string".to_string(), "a:string".to_string()];
        assert_ne!(repeated.fingerprint().unwrap(), one.fingerprint().unwrap());
    }

    /// `--debug` must not make a rerun look like a different command, or a
    /// stored conflict would refuse to recognise its own resume.
    #[test]
    fn presentation_and_debug_flags_are_not_semantic() {
        for excluded in [
            "debug",
            "output",
            "json",
            "quiet",
            "verbose",
            "abort-conflict",
        ] {
            assert!(
                !CanonicalRequestSyntaxV1::is_semantic(excluded),
                "{excluded} must be excluded"
            );
        }
        for semantic in ["force", "no-start", "package", "name", "manifest"] {
            assert!(
                CanonicalRequestSyntaxV1::is_semantic(semantic),
                "{semantic} must be kept"
            );
        }
    }

    /// An omitted project-derived default stays distinguishable from an
    /// explicit value: the option is simply absent.
    #[test]
    fn an_omitted_option_is_not_the_same_as_an_explicit_one() {
        let mut explicit = syntax();
        explicit
            .options
            .insert("name".to_string(), vec!["Note".to_string()]);
        let omitted = syntax();
        assert_ne!(
            explicit.fingerprint().unwrap(),
            omitted.fingerprint().unwrap()
        );

        // And an explicitly empty value differs from both.
        let mut empty = syntax();
        empty
            .options
            .insert("name".to_string(), vec![String::new()]);
        assert_ne!(
            empty.fingerprint().unwrap(),
            explicit.fingerprint().unwrap()
        );
        assert_ne!(empty.fingerprint().unwrap(), omitted.fingerprint().unwrap());
    }

    /// Names are stored canonically. `--force` and `force` hashing differently
    /// would make an alias look like a different command.
    #[test]
    fn a_name_that_kept_its_dashes_is_refused() {
        let mut leading = syntax();
        leading.flags = BTreeSet::from(["--no-start".to_string()]);
        let error = leading.fingerprint().unwrap_err();
        assert!(error.contains("leading dash"), "{error}");

        let mut empty = syntax();
        empty.command_path = vec![String::new()];
        assert!(empty.fingerprint().is_err());
    }

    #[test]
    fn changing_any_part_changes_the_fingerprint() {
        let base = syntax().fingerprint().unwrap();
        let variants = [
            CanonicalRequestSyntaxV1 {
                command_path: vec!["remove".to_string()],
                ..syntax()
            },
            CanonicalRequestSyntaxV1 {
                positionals: vec!["db".to_string()],
                ..syntax()
            },
            CanonicalRequestSyntaxV1 {
                options: BTreeMap::new(),
                ..syntax()
            },
            CanonicalRequestSyntaxV1 {
                flags: BTreeSet::new(),
                ..syntax()
            },
        ];
        for variant in variants {
            assert_ne!(variant.fingerprint().unwrap(), base);
        }
    }

    /// The domain prefix is fixed by the RFC, so a second implementation has
    /// to reproduce this exact digest for this exact projection.
    #[test]
    fn the_fingerprint_is_the_specified_domain_hash() {
        let one = syntax();
        let mut encoder = Encoder::new();
        one.encode(&mut encoder).unwrap();
        let encoded = encoder.finish().unwrap();
        assert_eq!(
            one.fingerprint().unwrap().object().as_bytes(),
            &codec::domain_hash("JAILS-REQUEST-SYNTAX-1", &encoded)
        );
    }

    // -----------------------------------------------------------------------
    // The closed admissibility matrix
    // -----------------------------------------------------------------------

    use crate::database::{QueryId, QueryName, SliceName};
    use crate::declaration::{FieldSpec, FieldType, IntentSpec};
    use crate::entity::{CasesReceiptId, Recipe, SourceInputId, TypeTargetId};
    use crate::identity::{Name, Package};

    fn intent_id() -> EntityId {
        EntityId::Intent(crate::entity::IntentId::new(
            Recipe::Record,
            Name::parse("Note").unwrap(),
            Package::base(),
        ))
    }

    fn capability_id(kind: jails_spec::spec::kind::Capability) -> CapabilityId {
        CapabilityId::resolve(kind, None, None).unwrap()
    }

    /// A matching outer Rust shape is not sufficient: `EntityId::Capability`
    /// beside `EntitySpec::Intent` type-checks and means nothing.
    #[test]
    fn an_identity_and_spec_of_different_kinds_reject() {
        assert!(
            CanonicalMutationRequest::generate_entity(
                intent_id(),
                EntitySpec::Intent(IntentSpec::default())
            )
            .is_ok()
        );

        let mismatched = CanonicalMutationRequest::generate_entity(
            intent_id(),
            EntitySpec::Capability(CapabilitySpec::default()),
        )
        .unwrap_err();
        assert!(mismatched.contains("different kinds"), "{mismatched}");
    }

    /// `generate` produces a persistent intent. A capability is `jails add`,
    /// and each says so rather than failing obscurely later.
    #[test]
    fn generate_refuses_a_capability_or_tool_feature() {
        for id in [
            EntityId::Capability(capability_id(jails_spec::spec::kind::Capability::Db)),
            EntityId::ToolFeature(ToolFeature::FastTest),
        ] {
            let error = CanonicalMutationRequest::generate_entity(
                id,
                EntitySpec::Intent(IntentSpec::default()),
            )
            .unwrap_err();
            assert!(error.contains("persistent intent"), "{error}");
            assert!(error.contains("fix:"), "{error}");
        }
    }

    #[test]
    fn destroy_refuses_a_capability_or_tool_feature() {
        assert!(CanonicalMutationRequest::destroy_entity(intent_id(), false).is_ok());
        for id in [
            EntityId::Capability(capability_id(jails_spec::spec::kind::Capability::Db)),
            EntityId::ToolFeature(ToolFeature::FastTest),
        ] {
            let error = CanonicalMutationRequest::destroy_entity(id, false).unwrap_err();
            assert!(error.contains("jails remove"), "{error}");
        }
    }

    #[test]
    fn storage_retirement_has_a_new_request_tag_and_round_trips() {
        let request = CanonicalMutationRequest::destroy_resource(
            intent_id(),
            StorageRetirement::Preserve {
                expected_table: SqlName::parse("tasks").unwrap(),
            },
            true,
        )
        .unwrap();
        assert_eq!(request.tag(), 15);

        let mut encoder = Encoder::new();
        request.encode(&mut encoder).unwrap();
        let bytes = encoder.finish().unwrap();
        let mut decoder = Decoder::new(&bytes).unwrap();
        assert_eq!(
            CanonicalMutationRequest::decode(&mut decoder).unwrap(),
            request
        );
        decoder.finish().unwrap();
    }

    fn assert_request_round_trip(request: CanonicalMutationRequest) {
        let mut encoder = Encoder::new();
        request.encode(&mut encoder).unwrap();
        let bytes = encoder.finish().unwrap();
        let mut decoder = Decoder::new(&bytes).unwrap();
        assert_eq!(
            CanonicalMutationRequest::decode(&mut decoder).unwrap(),
            request
        );
        decoder.finish().unwrap();
    }

    #[test]
    fn lifecycle_v1_requests_round_trip_without_reusing_old_tags() {
        let expected_path = || JavaType::parse("com.example.domain.Note").unwrap();
        let expected_table = || SqlName::parse("notes").unwrap();
        let evolve = EvolveFieldRequestV1 {
            entity: intent_id(),
            expected_path: expected_path(),
            expected_table: expected_table(),
            action: FieldEvolution::ChangeType {
                field: Name::parse("priority").unwrap(),
                to: FieldType::parse("long", &Package::base()).unwrap(),
                strategy: TypeChangeStrategy::Safe,
            },
            data: DataEvolution::ReaderOwnedSql(
                ProjectPath::parse("db/conversions/note_priority.sql").unwrap(),
            ),
        };
        let revive = ReviveResourceRequestV1 {
            entity: intent_id(),
            expected_table: expected_table(),
        };
        let repair = RepairResourceRequestV1 {
            entity: intent_id(),
            expected_path: expected_path(),
            strategy: RepairStrategy::RollForward,
            datasource: Some(DatasourceRef::parse("primary").unwrap()),
        };
        let destroy = DestroyResourceRequestV2 {
            entity: intent_id(),
            expected_path: expected_path(),
            storage: StorageRetirement::Drop {
                confirmed_table: expected_table(),
            },
            migration_effect: Some(DatasourceRef::parse("primary").unwrap()),
        };

        for request in [
            CanonicalMutationRequest::EvolveField(evolve),
            CanonicalMutationRequest::ReviveResource(revive),
            CanonicalMutationRequest::RepairResource(repair),
            CanonicalMutationRequest::DestroyResourceV2 {
                request: destroy,
                force: true,
            },
            CanonicalMutationRequest::SqlGenerate {
                queries: BTreeSet::from([QueryId::new(
                    SliceName::parse("Billing").unwrap(),
                    QueryName::parse("FindPayableOrders").unwrap(),
                )]),
            },
        ] {
            assert_request_round_trip(request);
        }
    }

    /// Cases is the only one-shot with a destroy route, and the other two say
    /// why rather than reporting a bare refusal.
    #[test]
    fn only_a_cases_one_shot_can_be_destroyed() {
        let cases = OneShotId::Cases {
            source: SourceInputId::Project(ProjectPath::parse("docs/cases.md").unwrap()),
        };
        assert!(CanonicalMutationRequest::destroy_one_shot(cases, false).is_ok());

        let field = OneShotId::Field {
            target: TypeTargetId::Existing(JavaType::parse("com.example.Note").unwrap()),
            field: Name::parse("title").unwrap(),
        };
        let error = CanonicalMutationRequest::destroy_one_shot(field, false).unwrap_err();
        assert!(error.contains("reapplies every"), "{error}");

        let migration = OneShotId::Migration {
            path: ProjectPath::parse("src/main/resources/db/migration/V1__x.sql").unwrap(),
        };
        let error = CanonicalMutationRequest::destroy_one_shot(migration, false).unwrap_err();
        assert!(error.contains("append-only"), "{error}");
    }

    /// A spec whose repeated identity field disagrees with its ID would make
    /// the derived receipt name a row it does not describe.
    #[test]
    fn a_one_shot_spec_must_agree_with_its_identity_field_by_field() {
        let source = SourceInputId::Project(ProjectPath::parse("docs/cases.md").unwrap());
        let other = SourceInputId::Project(ProjectPath::parse("docs/elsewhere.md").unwrap());
        let output = ProjectPath::parse("src/test/java/CasesTest.java").unwrap();
        let digest = ObjectId::from_bytes(codec::sha256(b"body"));

        assert!(
            CanonicalMutationRequest::generate_one_shot(
                OneShotId::Cases {
                    source: source.clone()
                },
                OneShotSpec::Cases {
                    source: source.clone(),
                    source_sha256: digest,
                    output: output.clone(),
                },
            )
            .is_ok()
        );

        let error = CanonicalMutationRequest::generate_one_shot(
            OneShotId::Cases {
                source: source.clone(),
            },
            OneShotSpec::Cases {
                source: other,
                source_sha256: digest,
                output,
            },
        )
        .unwrap_err();
        assert!(error.contains("disagree"), "{error}");
    }

    #[test]
    fn a_one_shot_of_the_wrong_kind_rejects() {
        let error = CanonicalMutationRequest::generate_one_shot(
            OneShotId::Migration {
                path: ProjectPath::parse("db/V1__x.sql").unwrap(),
            },
            OneShotSpec::Cases {
                source: SourceInputId::Project(ProjectPath::parse("docs/cases.md").unwrap()),
                source_sha256: ObjectId::from_bytes(codec::sha256(b"x")),
                output: ProjectPath::parse("out.java").unwrap(),
            },
        )
        .unwrap_err();
        assert!(error.contains("disagree"), "{error}");
    }

    /// `jails add` with nothing to add is a mistake the user should hear
    /// about, not a silent success.
    #[test]
    fn a_capability_list_must_be_nonempty_sorted_and_unique() {
        let db = CanonicalCapability {
            id: capability_id(jails_spec::spec::kind::Capability::Db),
            spec: CapabilitySpec::default(),
        };
        let kafka = CanonicalCapability {
            id: capability_id(jails_spec::spec::kind::Capability::Kafka),
            spec: CapabilitySpec::default(),
        };

        assert!(CanonicalMutationRequest::capabilities(vec![]).is_err());
        assert!(
            CanonicalMutationRequest::capabilities(vec![db.clone(), kafka.clone()]).is_ok(),
            "db sorts before kafka"
        );
        assert!(CanonicalMutationRequest::capabilities(vec![kafka, db.clone()]).is_err());
        assert!(CanonicalMutationRequest::capabilities(vec![db.clone(), db]).is_err());
    }

    /// A named instance already carries its package in its identity, so a spec
    /// that also named one would be a second authority for the same fact.
    #[test]
    fn a_named_capability_may_not_repeat_its_package_in_its_spec() {
        let id = CapabilityId::resolve(
            jails_spec::spec::kind::Capability::Csv,
            Some(&Name::parse("Dataset").unwrap()),
            Some(&Package::parse("io.example").unwrap()),
        )
        .unwrap();
        let error = CanonicalMutationRequest::capabilities(vec![CanonicalCapability {
            id,
            spec: CapabilitySpec {
                placement: Some(Package::parse("io.example").unwrap()),
            },
        }])
        .unwrap_err();
        assert!(error.contains("may not also name one"), "{error}");
    }

    /// The receipt id excludes content, output and operation, so it survives a
    /// same-source refresh — which is what makes `--receipt` work after the
    /// source file is gone.
    #[test]
    fn a_cases_receipt_id_is_stable_across_a_refresh() {
        let source = SourceInputId::Project(ProjectPath::parse("docs/cases.md").unwrap());
        let id = OneShotId::Cases {
            source: source.clone(),
        };
        let first = CasesReceiptId::of(&id).unwrap();
        let again = CasesReceiptId::of(&OneShotId::Cases { source }).unwrap();
        assert_eq!(first, again);

        let elsewhere = CasesReceiptId::of(&OneShotId::Cases {
            source: SourceInputId::Project(ProjectPath::parse("docs/other.md").unwrap()),
        })
        .unwrap();
        assert_ne!(first, elsewhere);

        // 64 lowercase hex, parsed back byte-identically.
        let text = first.to_hex();
        assert_eq!(text.len(), 64);
        assert_eq!(CasesReceiptId::parse_hex(&text).unwrap(), first);
        assert!(CasesReceiptId::parse_hex(&text.to_uppercase()).is_err());

        // Only defined for a cases one-shot.
        assert!(
            CasesReceiptId::of(&OneShotId::Migration {
                path: ProjectPath::parse("db/V1__x.sql").unwrap()
            })
            .is_err()
        );
    }

    /// A symlink and its target are one identity; a moved file is a new one
    /// even with identical bytes.
    #[test]
    fn an_external_path_identity_is_of_the_canonical_path() {
        let one = crate::entity::ExternalPathId::of_canonical_path("/srv/briefs/a.md").unwrap();
        let same = crate::entity::ExternalPathId::of_canonical_path("/srv/briefs/a.md").unwrap();
        let moved = crate::entity::ExternalPathId::of_canonical_path("/srv/other/a.md").unwrap();
        assert_eq!(one, same);
        assert_ne!(one, moved);

        // A relative path has not been canonicalised, and taking its identity
        // would give the same file different ids from different directories.
        let error = crate::entity::ExternalPathId::of_canonical_path("briefs/a.md").unwrap_err();
        assert!(error.contains("canonical absolute path"), "{error}");
    }

    /// A future tool feature needs a protocol and CLI addition rather than
    /// falling through.
    #[test]
    fn removing_a_tool_feature_names_exactly_the_one_that_exists() {
        assert!(
            CanonicalMutationRequest::remove_tool_feature(ToolFeature::FastTest, false).is_ok()
        );
    }

    /// The request tags are fixed by the RFC and may never be reused.
    #[test]
    fn request_tags_match_the_specified_numbers() {
        let path = || ProjectPath::parse("x.txt").unwrap();
        let cases: [(CanonicalMutationRequest, u8); 12] = [
            (CanonicalMutationRequest::Sync { no_start: false }, 2),
            (CanonicalMutationRequest::AppInit { target: path() }, 5),
            (CanonicalMutationRequest::AppApply { no_start: false }, 6),
            (CanonicalMutationRequest::AdoptLayout, 8),
            (CanonicalMutationRequest::FastTest, 10),
            (
                CanonicalMutationRequest::Format {
                    scopes: BTreeSet::new(),
                },
                11,
            ),
            (
                CanonicalMutationRequest::RemoveToolFeature {
                    feature: ToolFeature::FastTest,
                    force: false,
                },
                12,
            ),
            (
                CanonicalMutationRequest::Rename {
                    from: JavaType::parse("A").unwrap(),
                    to: JavaType::parse("B").unwrap(),
                    force: false,
                },
                7,
            ),
            (
                CanonicalMutationRequest::EvolveField(EvolveFieldRequestV1 {
                    entity: intent_id(),
                    expected_path: JavaType::parse("Note").unwrap(),
                    expected_table: SqlName::parse("notes").unwrap(),
                    action: FieldEvolution::Add(
                        FieldSpec::parse("title:string", &Package::base()).unwrap(),
                    ),
                    data: DataEvolution::None,
                }),
                16,
            ),
            (
                CanonicalMutationRequest::ReviveResource(ReviveResourceRequestV1 {
                    entity: intent_id(),
                    expected_table: SqlName::parse("notes").unwrap(),
                }),
                17,
            ),
            (
                CanonicalMutationRequest::RepairResource(RepairResourceRequestV1 {
                    entity: intent_id(),
                    expected_path: JavaType::parse("Note").unwrap(),
                    strategy: RepairStrategy::RollForward,
                    datasource: None,
                }),
                18,
            ),
            (
                CanonicalMutationRequest::DestroyResourceV2 {
                    request: DestroyResourceRequestV2 {
                        entity: intent_id(),
                        expected_path: JavaType::parse("Note").unwrap(),
                        storage: StorageRetirement::Preserve {
                            expected_table: SqlName::parse("notes").unwrap(),
                        },
                        migration_effect: None,
                    },
                    force: false,
                },
                19,
            ),
        ];
        for (request, tag) in cases {
            assert_eq!(request.tag(), tag, "{request:?}");
        }
    }
}
