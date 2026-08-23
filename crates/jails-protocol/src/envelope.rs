//! The schema-2 ledger envelope: a TOML shell around the closed binary codec.
//!
//! Named `envelope` rather than `ledger` because `jails_project::ledger` is
//! the schema-1 store and both exist until R1.5 step 6 retires it. Two modules
//! sharing a name makes every path-based gate ambiguous, which
//! `no_two_crates_share_a_module_name` now refuses.
//!
//! ## Why a binary payload inside TOML at all
//!
//! plan.md §R1.4.1. `.jails/ledger.toml` keeps its path for compatibility, but
//! the contents stop being hand-rolled TOML. A second bespoke recursive TOML
//! serializer would double the wire surface and make canonical byte identity —
//! the property every identity in this protocol rests on — much harder to
//! audit. The payload is opaque machine state; `jails doctor --output json` is
//! the supported decoder, not a text editor.
//!
//! ## The envelope is five lines and nothing else
//!
//! ```toml
//! schema = 2
//! codec = "jails-ledger-payload-1"
//! payload_len = 0
//! payload_sha256 = "e3b0…b855"
//! payload_hex = ""
//! ```
//!
//! In that order, LF-terminated, with no BOM, no CR, no comment, no blank
//! line, no extra whitespace and no extra key. That is a strict *subset* of
//! valid TOML, which is the point: it avoids a general TOML dependency and,
//! more importantly, a permissive parse tree. A parser that accepted a
//! reordered or re-spaced file would let one ledger have many spellings, and a
//! ledger with many spellings cannot be compared byte for byte.
//!
//! ## Order of checks
//!
//! Every limit is applied before the allocation it guards: the source is
//! capped before it is read into memory, the declared length is capped and
//! range-checked before the hex is decoded, and only then are length and
//! digest verified. A ledger arrives from disk after a crash; a declared
//! length is not a promise.

use crate::Result;
use jails_support::codec;

/// 32 MiB of decoded payload.
pub const MAX_LEDGER_PAYLOAD: usize = 32 * 1024 * 1024;
/// Two hex characters per byte, plus the fixed envelope allowance.
pub const MAX_LEDGER_SOURCE: usize = 2 * MAX_LEDGER_PAYLOAD + 512;

/// The codec name this envelope declares. A different one is a refusal, not a
/// best-effort read.
pub const PAYLOAD_CODEC: &str = "jails-ledger-payload-1";
/// The schema this module reads and writes.
pub const SCHEMA: u32 = 2;

/// Render the envelope for one payload.
///
/// Always ends in exactly one LF.
pub fn render(payload: &[u8]) -> Result<String> {
    if payload.len() > MAX_LEDGER_PAYLOAD {
        return Err(format!(
            "ledger payload is {} bytes, over the {MAX_LEDGER_PAYLOAD}-byte limit",
            payload.len()
        ));
    }
    let digest = codec::hex(&codec::sha256(payload));
    let hex = codec::hex_bytes(payload);
    let source = format!(
        "schema = {SCHEMA}\n\
         codec = \"{PAYLOAD_CODEC}\"\n\
         payload_len = {}\n\
         payload_sha256 = \"{digest}\"\n\
         payload_hex = \"{hex}\"\n",
        payload.len()
    );
    // The fixed keys, quotes, digest, decimal length and LF separators have to
    // fit the 512-byte allowance the source cap is built from. Asserted rather
    // than assumed, because the two constants are only correct together.
    debug_assert!(
        source.len() <= 2 * payload.len() + 512,
        "the envelope overhead no longer fits its allowance"
    );
    Ok(source)
}

/// Read an envelope back to its payload bytes.
pub fn parse(source: &str) -> Result<Vec<u8>> {
    if source.len() > MAX_LEDGER_SOURCE {
        return Err(format!(
            "ledger file is {} bytes, over the {MAX_LEDGER_SOURCE}-byte limit",
            source.len()
        ));
    }
    if source.starts_with('\u{feff}') {
        return Err("ledger begins with a byte-order mark".to_string());
    }
    if source.contains('\r') {
        return Err("ledger contains a CR; line endings are LF".to_string());
    }
    if !source.ends_with('\n') {
        return Err("ledger does not end with a newline".to_string());
    }

    let lines: Vec<&str> = source
        .strip_suffix('\n')
        .unwrap_or(source)
        .split('\n')
        .collect();
    if lines.len() != 5 {
        return Err(format!(
            "ledger has {} line(s); schema {SCHEMA} is exactly five, in a fixed order",
            lines.len()
        ));
    }

    let schema = value_of(lines[0], "schema")?;
    if schema != SCHEMA.to_string() {
        return Err(format!(
            "ledger declares schema {schema}.\n       fix: this jails reads schema {SCHEMA}. A \
             newer schema is refused rather than half-read."
        ));
    }
    let declared_codec = quoted(value_of(lines[1], "codec")?)?;
    if declared_codec != PAYLOAD_CODEC {
        return Err(format!(
            "ledger declares codec `{declared_codec}`, and this jails writes `{PAYLOAD_CODEC}`"
        ));
    }

    let declared_len = decimal(value_of(lines[2], "payload_len")?)?;
    if declared_len > MAX_LEDGER_PAYLOAD {
        return Err(format!(
            "ledger declares a {declared_len}-byte payload, over the \
             {MAX_LEDGER_PAYLOAD}-byte limit"
        ));
    }
    let declared_digest = quoted(value_of(lines[3], "payload_sha256")?)?.to_string();
    let hex = quoted(value_of(lines[4], "payload_hex")?)?;

    // The hex length is checked against the declared length *before* decoding,
    // so a hostile pair cannot make the decoder allocate on the larger of them.
    let expected_hex = declared_len
        .checked_mul(2)
        .ok_or("declared payload length overflows")?;
    if hex.len() != expected_hex {
        return Err(format!(
            "ledger declares {declared_len} byte(s) but carries {} hex character(s); \
             {expected_hex} were expected",
            hex.len()
        ));
    }
    let payload = codec::unhex_bytes(hex)?;

    if payload.len() != declared_len {
        return Err(format!(
            "ledger payload decoded to {} byte(s), not the declared {declared_len}",
            payload.len()
        ));
    }
    let actual = codec::hex(&codec::sha256(&payload));
    if actual != declared_digest {
        return Err(format!(
            "ledger payload hashes to {actual}, not the recorded {declared_digest}.\n       \
             fix: the file is corrupt. Restore it from version control; jails will not guess \
             what it recorded."
        ));
    }

    // A file that parses but does not re-render identically has a second
    // spelling, and a ledger with two spellings cannot be compared byte for
    // byte -- which is what every identity here rests on.
    if render(&payload)? != source {
        return Err(
            "ledger is not in canonical form.\n       fix: it parses, but re-rendering it \
             produces different bytes, so two files would mean one ledger. Rewrite it with \
             this jails."
                .to_string(),
        );
    }
    Ok(payload)
}

/// `key = value`, with the exact single spaces the format fixes.
fn value_of<'a>(line: &'a str, key: &str) -> Result<&'a str> {
    let prefix = format!("{key} = ");
    line.strip_prefix(&prefix)
        .ok_or_else(|| format!("expected a line `{key} = …`, found `{line}`"))
}

fn quoted(value: &str) -> Result<&str> {
    let inner = value
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .ok_or_else(|| format!("expected a quoted value, found `{value}`"))?;
    // No escapes: this subset has no need for them, and supporting them would
    // give one byte string several spellings.
    if inner.contains('\\') || inner.contains('"') {
        return Err(format!(
            "value `{inner}` contains a quote or backslash; this format has no escapes"
        ));
    }
    Ok(inner)
}

/// Unsigned canonical decimal: no sign, no leading zero except `0` itself.
fn decimal(value: &str) -> Result<usize> {
    if value.is_empty() {
        return Err("expected a decimal number, found nothing".to_string());
    }
    if !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(format!("`{value}` is not an unsigned decimal number"));
    }
    if value.len() > 1 && value.starts_with('0') {
        return Err(format!(
            "`{value}` has a leading zero; the canonical form has none"
        ));
    }
    value
        .parse()
        .map_err(|_| format!("`{value}` does not fit this platform's usize"))
}

// ---------------------------------------------------------------------------
// The payload
// ---------------------------------------------------------------------------

use crate::entity::{EntityId, EntitySpec, OneShotId, OwnerId};
use crate::identity::{ObjectId, OperationId, ProjectPath};
use crate::record::{AppliedEntity, AppliedVersion, OneShotReceipt, OutputRecord};
use crate::resource::{ResourceKey, ResourceOwner, ResourceRecord, ResourceValue};
use jails_support::codec::{Decoder, Encoder, ordered};
use std::collections::BTreeSet;

/// Which pre-schema-2 store a row came out of.
///
/// The kind is part of the key, not decoration: two stores could hold rows
/// with identical bytes, and adopting "the row" would then be ambiguous about
/// which one is being resolved.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub enum LegacySourceKind {
    Schema1Ledger,
    Schema1Applied,
    Schema1Model,
    AppStateHeader,
    AppState,
    IntentFiles,
    ModelFiles,
    GlobalFiles,
    VersionFile,
}

impl LegacySourceKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Schema1Ledger => "schema1-ledger",
            Self::Schema1Applied => "schema1-applied",
            Self::Schema1Model => "schema1-model",
            Self::AppStateHeader => "app-state-header",
            Self::AppState => "app-state",
            Self::IntentFiles => "intent-files",
            Self::ModelFiles => "model-files",
            Self::GlobalFiles => "global-files",
            Self::VersionFile => "version-file",
        }
    }

    fn tag(self) -> u8 {
        match self {
            Self::Schema1Ledger => 0,
            Self::Schema1Applied => 1,
            Self::Schema1Model => 2,
            Self::AppStateHeader => 3,
            Self::AppState => 4,
            Self::IntentFiles => 5,
            Self::ModelFiles => 6,
            Self::GlobalFiles => 7,
            Self::VersionFile => 8,
        }
    }

    fn from_tag(tag: u8) -> Result<Self> {
        Ok(match tag {
            0 => Self::Schema1Ledger,
            1 => Self::Schema1Applied,
            2 => Self::Schema1Model,
            3 => Self::AppStateHeader,
            4 => Self::AppState,
            5 => Self::IntentFiles,
            6 => Self::ModelFiles,
            7 => Self::GlobalFiles,
            8 => Self::VersionFile,
            other => Err(format!("unknown legacy source kind tag {other}"))?,
        })
    }
}

/// A stable name for one legacy row, so a human can adopt exactly it.
///
/// plan.md §R1.4: the digest is `SHA256("JAILS-LEGACY-1" || row_bytes)`. It is
/// **derived, never stored** — a recorded key would be a second authority for
/// what the row says, and the failure mode is adopting a row whose content has
/// since changed under a key that still matches.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct LegacyKey {
    pub source_kind: LegacySourceKind,
    pub digest: ObjectId,
}

impl LegacyKey {
    pub fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.tag(self.source_kind.tag());
        self.digest.encode(encoder);
        Ok(())
    }

    pub fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(Self {
            source_kind: LegacySourceKind::from_tag(decoder.tag()?)?,
            digest: ObjectId::decode(decoder)?,
        })
    }

    /// The short spelling `jails doctor` prints and `jails adopt` accepts.
    pub fn to_label(self) -> String {
        format!("{}:{}", self.source_kind.label(), self.digest.to_hex())
    }

    pub fn parse_label(text: &str) -> Result<Self> {
        let (kind, digest) = text
            .split_once(':')
            .ok_or_else(|| format!("`{text}` is not a legacy key: expected <source>:<digest>"))?;
        let source_kind = [
            LegacySourceKind::Schema1Ledger,
            LegacySourceKind::Schema1Applied,
            LegacySourceKind::Schema1Model,
            LegacySourceKind::AppStateHeader,
            LegacySourceKind::AppState,
            LegacySourceKind::IntentFiles,
            LegacySourceKind::ModelFiles,
            LegacySourceKind::GlobalFiles,
            LegacySourceKind::VersionFile,
        ]
        .into_iter()
        .find(|candidate| candidate.label() == kind)
        .ok_or_else(|| format!("unknown legacy source `{kind}`"))?;
        Ok(Self {
            source_kind,
            digest: ObjectId::parse_hex(digest)?,
        })
    }
}

impl std::fmt::Display for LegacyKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_label())
    }
}

/// A row carried forward from a pre-schema-2 store, origin unresolved.
///
/// plan.md §R1.4 keeps these deliberately separate from `applied`. A schema-1
/// row records paths and, sometimes, a spec — but never *who wanted it*, and
/// inventing an owner would turn machine state into human desire, which is the
/// one thing the desired/observed boundary exists to prevent. So a legacy row
/// stays a legacy row until a user-requested adoption resolves it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegacyEntry {
    pub recipe: String,
    pub name: String,
    pub package: String,
    pub fields: Vec<String>,
    pub indexes: Vec<String>,
    pub timestamps: bool,
    pub on: String,
    pub yields: String,
    /// Whether anyone ever recorded *what* this was built from.
    pub spec_presence: SpecPresence,
    pub paths: Vec<String>,
}

/// Whether a legacy row's origin is known. Not inferable from content: a row
/// whose fields happen to match today's manifest is still a row of unknown
/// origin.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpecPresence {
    Present,
    Absent,
    UnknownLegacy,
}

impl SpecPresence {
    fn tag(self) -> u8 {
        match self {
            Self::Present => 0,
            Self::Absent => 1,
            Self::UnknownLegacy => 2,
        }
    }

    fn from_tag(tag: u8) -> Result<Self> {
        match tag {
            0 => Ok(Self::Present),
            1 => Ok(Self::Absent),
            2 => Ok(Self::UnknownLegacy),
            other => Err(format!("unknown spec presence tag {other}")),
        }
    }
}

impl LegacyEntry {
    pub fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.string(&self.recipe)?;
        encoder.string(&self.name)?;
        encoder.string(&self.package)?;
        encode_strings(encoder, &self.fields)?;
        encode_strings(encoder, &self.indexes)?;
        encoder.bool(self.timestamps);
        encoder.string(&self.on)?;
        encoder.string(&self.yields)?;
        encoder.tag(self.spec_presence.tag());
        encode_strings(encoder, &self.paths)
    }

    pub fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(Self {
            recipe: decoder.string()?,
            name: decoder.string()?,
            package: decoder.string()?,
            fields: decode_strings(decoder)?,
            indexes: decode_strings(decoder)?,
            timestamps: decoder.bool()?,
            on: decoder.string()?,
            yields: decoder.string()?,
            spec_presence: SpecPresence::from_tag(decoder.tag()?)?,
            paths: decode_strings(decoder)?,
        })
    }

    /// This row's stable adoption key, derived from its canonical bytes.
    pub fn legacy_key(&self, source_kind: LegacySourceKind) -> Result<LegacyKey> {
        let mut encoder = Encoder::new();
        self.encode(&mut encoder)?;
        let bytes = encoder.finish()?;
        Ok(LegacyKey {
            source_kind,
            digest: ObjectId::from_bytes(jails_support::codec::domain_hash(
                "JAILS-LEGACY-1",
                &bytes,
            )),
        })
    }

    /// This row's identity, for ordering. Legacy rows have no typed identity
    /// by construction, so the canonical order is on the recorded strings.
    fn key(&self) -> (&str, &str, &str) {
        (&self.recipe, &self.name, &self.package)
    }
}

fn encode_strings(encoder: &mut Encoder, values: &[String]) -> Result<()> {
    encoder.count(values.len())?;
    for value in values {
        encoder.string(value)?;
    }
    Ok(())
}

fn decode_strings(decoder: &mut Decoder<'_>) -> Result<Vec<String>> {
    let count = decoder.count()?;
    let mut out = Vec::new();
    for _ in 0..count {
        out.push(decoder.string()?);
    }
    Ok(out)
}

/// The schema-2 ledger payload.
///
/// Deliberately **not** a description of what is wanted. Everything here
/// records what was applied; the desired state comes only from human sources
/// and the current request.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LedgerV2 {
    pub written_by: String,
    /// Monotonic, incremented once per commit.
    pub generation: u64,
    pub last_operation: Option<OperationId>,
    pub applied: Vec<AppliedEntity>,
    /// One row per applied one-shot, by id.
    pub one_shots: Vec<OneShotReceipt>,
    /// One canonical row per `ResourceKey`: what is installed, and who wants
    /// it. A resource with two rows would let the written order decide which
    /// of them a removal consults.
    pub resources: Vec<ResourceRecord>,
    /// One canonical row per path jails has written.
    pub outputs: Vec<OutputRecord>,
    pub legacy: Vec<LegacyEntry>,
    /// A reconciliation that stopped with conflicts still in the tree.
    ///
    /// Its presence is what makes the ordinary bootstrap parsers unsafe: a
    /// committed conflict may deliberately have left markers in the POM, the
    /// human config, the manifest or a source file.
    pub pending_conflict: Option<PendingMarker>,
}

/// Enough of a stored conflict to decide how to bootstrap and whether a rerun
/// is the same command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingMarker {
    pub operation: OperationId,
    pub generation: u64,
    /// The request that stalled, so a resume can prove it is the same one
    /// without parsing marker-bearing project files to find out.
    pub request_syntax: crate::request::RequestSyntaxFingerprint,
    /// Presentation only, and excluded from every identity: a reworded message
    /// must not make a stored conflict unrecognisable.
    pub resume_display: String,
}

impl PendingMarker {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.operation.encode(encoder);
        if self.generation == 0 {
            return Err("a pending conflict records generation zero".to_string());
        }
        encoder.u64(self.generation);
        self.request_syntax.encode(encoder);
        encoder.string(&self.resume_display)
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        let operation = OperationId::decode(decoder)?;
        let generation = decoder.u64()?;
        if generation == 0 {
            return Err("a pending conflict records generation zero".to_string());
        }
        Ok(Self {
            operation,
            generation,
            request_syntax: crate::request::RequestSyntaxFingerprint::decode(decoder)?,
            resume_display: decoder.string()?,
        })
    }
}

impl LedgerV2 {
    pub fn encode(&self) -> Result<Vec<u8>> {
        let mut encoder = Encoder::new();
        encoder.string(&self.written_by)?;
        if self.generation == 0 {
            return Err(
                "generation is zero; it is incremented once per commit and starts at one"
                    .to_string(),
            );
        }
        encoder.u64(self.generation);
        encoder.option(self.last_operation.as_ref(), |e, id| {
            id.encode(e);
            Ok(())
        })?;
        encoder.count(self.applied.len())?;
        let mut previous: Option<&EntityId> = None;
        for entity in &self.applied {
            ordered(previous, &entity.id)?;
            previous = Some(&entity.id);
            entity.encode(&mut encoder)?;
        }
        encoder.count(self.one_shots.len())?;
        let mut previous: Option<&OneShotId> = None;
        for receipt in &self.one_shots {
            ordered(previous, &receipt.id)?;
            previous = Some(&receipt.id);
            receipt.encode(&mut encoder)?;
        }
        encoder.count(self.resources.len())?;
        let mut previous: Option<&ResourceKey> = None;
        for resource in &self.resources {
            ordered(previous, &resource.key)?;
            previous = Some(&resource.key);
            resource.value.agrees_with(&resource.key)?;
            resource.key.encode(&mut encoder)?;
            encoder.count(resource.owners.len())?;
            let mut owner_before: Option<&ResourceOwner> = None;
            for owner in &resource.owners {
                ordered(owner_before, owner)?;
                owner_before = Some(owner);
                owner.encode(&mut encoder)?;
            }
            resource.value.encode(&mut encoder)?;
        }
        encoder.count(self.outputs.len())?;
        let mut previous: Option<&ProjectPath> = None;
        for output in &self.outputs {
            ordered(previous, &output.path)?;
            previous = Some(&output.path);
            output.encode(&mut encoder)?;
        }
        encoder.count(self.legacy.len())?;
        let mut previous: Option<(&str, &str, &str)> = None;
        for entry in &self.legacy {
            ordered(previous.as_ref(), &entry.key())?;
            previous = Some(entry.key());
            entry.encode(&mut encoder)?;
        }
        encoder.option(self.pending_conflict.as_ref(), |e, marker| marker.encode(e))?;
        encoder.finish()
    }

    pub fn decode(payload: &[u8]) -> Result<Self> {
        let mut decoder = Decoder::new(payload)?;
        let written_by = decoder.string()?;
        let generation = decoder.u64()?;
        if generation == 0 {
            return Err("ledger generation is zero".to_string());
        }
        let last_operation = decoder.option(OperationId::decode)?;
        let count = decoder.count()?;
        let mut applied: Vec<AppliedEntity> = Vec::new();
        for _ in 0..count {
            let entity = AppliedEntity::decode(&mut decoder)?;
            ordered(applied.last().map(|last| &last.id), &entity.id)?;
            applied.push(entity);
        }
        let count = decoder.count()?;
        let mut one_shots: Vec<OneShotReceipt> = Vec::new();
        for _ in 0..count {
            let receipt = OneShotReceipt::decode(&mut decoder)?;
            ordered(one_shots.last().map(|last| &last.id), &receipt.id)?;
            one_shots.push(receipt);
        }
        let count = decoder.count()?;
        let mut resources: Vec<ResourceRecord> = Vec::new();
        for _ in 0..count {
            let key = ResourceKey::decode(&mut decoder)?;
            ordered(resources.last().map(|last| &last.key), &key)?;
            let owner_count = decoder.count()?;
            let mut owners = BTreeSet::new();
            let mut owner_before: Option<ResourceOwner> = None;
            for _ in 0..owner_count {
                let owner = ResourceOwner::decode(&mut decoder)?;
                ordered(owner_before.as_ref(), &owner)?;
                owner_before = Some(owner.clone());
                owners.insert(owner);
            }
            let value = ResourceValue::decode(&mut decoder)?;
            value.agrees_with(&key)?;
            resources.push(ResourceRecord { key, owners, value });
        }
        let count = decoder.count()?;
        let mut outputs: Vec<OutputRecord> = Vec::new();
        for _ in 0..count {
            let output = OutputRecord::decode(&mut decoder)?;
            ordered(outputs.last().map(|last| &last.path), &output.path)?;
            outputs.push(output);
        }
        let count = decoder.count()?;
        let mut legacy: Vec<LegacyEntry> = Vec::new();
        for _ in 0..count {
            let entry = LegacyEntry::decode(&mut decoder)?;
            if let Some(last) = legacy.last() {
                ordered(Some(&last.key()), &entry.key())?;
            }
            legacy.push(entry);
        }
        let pending_conflict = decoder.option(PendingMarker::decode)?;
        decoder.finish()?;
        Ok(Self {
            written_by,
            generation,
            last_operation,
            applied,
            one_shots,
            resources,
            outputs,
            legacy,
            pending_conflict,
        })
    }

    /// Render the complete file: payload plus envelope.
    pub fn render(&self) -> Result<String> {
        render(&self.encode()?)
    }

    /// Read a complete file.
    pub fn parse_file(source: &str) -> Result<Self> {
        Self::decode(&parse(source)?)
    }

    /// The model registry, **derived** rather than stored.
    ///
    /// Schema 1 kept `[[model]]` rows beside `[[applied]]` ones — the same
    /// fact in two places, under two different keys, which `CLAUDE.md` records
    /// as the shape of the §9.7 bug. There is one row now and the view is
    /// computed, so the two cannot disagree.
    pub fn models(&self) -> Vec<(EntityId, Vec<String>)> {
        let mut out: Vec<(EntityId, Vec<String>)> = self
            .applied
            .iter()
            .filter_map(|entity| match &entity.version.spec {
                EntitySpec::Intent(intent) if !intent.fields.is_empty() => Some((
                    entity.id.clone(),
                    intent
                        .fields
                        .iter()
                        .map(|field| field.canonical())
                        .collect(),
                )),
                _ => None,
            })
            .collect();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }
}

// ---------------------------------------------------------------------------
// Schema-1 migration
// ---------------------------------------------------------------------------

/// One schema-1 `[[applied]]` row, as plain data.
///
/// Deliberately not the schema-1 `Applied` type: that lives in `jails-project`,
/// which is *above* this crate. The caller adapts, and the migration stays a
/// pure function of values — which is also what makes it testable without a
/// filesystem.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Schema1Row {
    pub recipe: String,
    pub name: String,
    pub package: String,
    pub fields: Vec<String>,
    pub indexes: Vec<String>,
    pub timestamps: bool,
    pub on: String,
    pub yields: String,
    /// `Some(true)` for an app row, `Some(false)` for a direct one, `None` for
    /// a row written before the key existed.
    pub has_spec: Option<bool>,
    pub paths: Vec<String>,
}

/// Fold a schema-1 ledger into schema 2, **in memory**.
///
/// Two rules, and both are refusals to guess:
///
/// - A row whose origin is unknown stays unknown. It becomes a `LegacyEntry`
///   with `UnknownLegacy`, never an `AppliedEntity` with an invented owner —
///   inventing one would turn machine state into human desire, which is the
///   boundary the whole phase is built on.
/// - A row is only promoted when its origin *and* its identity are both
///   representable. A name that predates the protocol's validation is carried
///   forward as legacy rather than refused, because refusing would strand
///   `destroy` on the projects with the most history to lose.
///
/// `generation` starts at one: zero would make "never committed" and
/// "committed once" the same recorded value.
pub fn migrate_schema1(written_by: &str, rows: &[Schema1Row]) -> Result<LedgerV2> {
    use crate::entity::{IntentId, Recipe};
    use crate::identity::{Name, Package};
    use clap::ValueEnum;

    let mut applied: Vec<AppliedEntity> = Vec::new();
    let mut legacy: Vec<LegacyEntry> = Vec::new();

    for row in rows {
        let typed = Recipe::from_str(&row.recipe, false)
            .ok()
            .zip(Name::parse(&row.name).ok())
            .zip(Package::parse(&row.package).ok())
            .map(|((recipe, name), package)| IntentId::new(recipe, name, package));

        let owner = match row.has_spec {
            Some(true) => Some(OwnerId::AppManifest),
            Some(false) => Some(OwnerId::DirectCli),
            None => None,
        };

        if let (Some(id), Some(owner)) = (typed, owner) {
            // A capitalised field type resolves against the row's own
            // package, which is where a type it names would have been
            // generated.
            let base = id.package.clone();
            let spec = crate::declaration::IntentSpec::parse(
                &row.fields,
                &row.indexes,
                row.timestamps,
                &base,
            );
            if let Ok(spec) = spec {
                applied.push(AppliedEntity {
                    id: EntityId::Intent(id),
                    owners: BTreeSet::from([owner]),
                    version: AppliedVersion {
                        spec: EntitySpec::Intent(spec),
                        operation: OperationId::from_bytes([0; 32]),
                    },
                });
                continue;
            }
            // A spec this binary cannot parse is not evidence that the row
            // is wrong -- it is evidence that the row predates a rule.
            // Carried forward as legacy rather than dropped.
        }
        legacy.push(LegacyEntry {
            recipe: row.recipe.clone(),
            name: row.name.clone(),
            package: row.package.clone(),
            fields: row.fields.clone(),
            indexes: row.indexes.clone(),
            timestamps: row.timestamps,
            on: row.on.clone(),
            yields: row.yields.clone(),
            spec_presence: match row.has_spec {
                Some(true) => SpecPresence::Present,
                Some(false) => SpecPresence::Absent,
                None => SpecPresence::UnknownLegacy,
            },
            paths: row.paths.clone(),
        });
    }

    applied.sort_by(|a, b| a.id.cmp(&b.id));
    legacy.sort_by(|a, b| a.key().cmp(&b.key()));

    Ok(LedgerV2 {
        written_by: written_by.to_string(),
        generation: 1,
        last_operation: None,
        applied,
        // A schema-1 store recorded none of these three. Translating them into
        // empty tables is the honest reading: nothing was claimed, nothing was
        // stamped, and the first schema-2 command to touch a resource records
        // it then. Inventing rows here would give `destroy` a list of files to
        // delete that nothing had actually written.
        one_shots: Vec::new(),
        resources: Vec::new(),
        outputs: Vec::new(),
        legacy,
        pending_conflict: None,
    })
}

#[cfg(test)]
mod tests {

    /// The three tables R1.4 adds are canonical sets, and a decoder that
    /// accepted them out of order would accept two spellings of one store.
    #[test]
    fn the_recorded_tables_round_trip_and_refuse_a_second_spelling() {
        use crate::coordinate::{DependencySpec, MavenCoordinate};
        use crate::resource::{ResourceKey, ResourceRecord, ResourceValue};

        let coordinate =
            |artifact: &str| MavenCoordinate::parse("org.springframework.boot", artifact).unwrap();
        let row = |artifact: &str| ResourceRecord {
            key: ResourceKey::MavenDependency(coordinate(artifact)),
            owners: BTreeSet::from([ResourceOwner::Entity(EntityId::ToolFeature(
                crate::entity::ToolFeature::FastTest,
            ))]),
            value: ResourceValue::MavenDependency(DependencySpec::managed(coordinate(artifact))),
        };
        let mut ledger = LedgerV2 {
            written_by: "0.1.0".to_string(),
            generation: 2,
            last_operation: None,
            applied: Vec::new(),
            one_shots: Vec::new(),
            resources: vec![
                row("spring-boot-starter-actuator"),
                row("spring-boot-starter-web"),
            ],
            outputs: Vec::new(),
            legacy: Vec::new(),
            pending_conflict: None,
        };
        let bytes = ledger.encode().unwrap();
        assert_eq!(LedgerV2::decode(&bytes).unwrap(), ledger);

        ledger.resources.reverse();
        let error = ledger.encode().unwrap_err();
        assert!(error.contains("order"), "{error}");
    }
    use super::*;

    /// The exact bytes, so a second implementation can reproduce them.
    #[test]
    fn the_envelope_is_five_lf_terminated_lines_in_a_fixed_order() {
        let source = render(&[0xde, 0xad, 0xbe, 0xef]).unwrap();
        assert_eq!(
            source,
            "schema = 2\n\
             codec = \"jails-ledger-payload-1\"\n\
             payload_len = 4\n\
             payload_sha256 = \
             \"5f78c33274e43fa9de5659265c1d917e25c03722dcb0b8d27db8d5feaa813953\"\n\
             payload_hex = \"deadbeef\"\n"
        );
        assert_eq!(parse(&source).unwrap(), vec![0xde, 0xad, 0xbe, 0xef]);
    }

    /// §R1.4.1's own example. It demonstrates the envelope only — an empty
    /// payload is not a valid `LedgerPayloadV1` — but the envelope itself has
    /// to render exactly this.
    #[test]
    fn the_empty_payload_envelope_matches_the_rfc_example() {
        assert_eq!(
            render(&[]).unwrap(),
            "schema = 2\n\
             codec = \"jails-ledger-payload-1\"\n\
             payload_len = 0\n\
             payload_sha256 = \
             \"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855\"\n\
             payload_hex = \"\"\n"
        );
    }

    #[test]
    fn a_payload_round_trips_byte_for_byte() {
        for payload in [
            vec![],
            vec![0u8],
            vec![0xff; 3],
            (0..=255u8).collect::<Vec<_>>(),
        ] {
            let source = render(&payload).unwrap();
            assert_eq!(parse(&source).unwrap(), payload);
        }
    }

    /// A newer schema is refused rather than half-read — the same rule the
    /// schema-1 closed parser already had for an unknown top-level key.
    #[test]
    fn a_newer_schema_refuses() {
        let source = render(b"x").unwrap().replace("schema = 2", "schema = 3");
        let error = parse(&source).unwrap_err();
        assert!(error.contains("declares schema 3"), "{error}");
        assert!(error.contains("refused rather than half-read"), "{error}");
    }

    #[test]
    fn a_different_codec_refuses() {
        let source = render(b"x")
            .unwrap()
            .replace(PAYLOAD_CODEC, "some-other-codec-1");
        assert!(parse(&source).unwrap_err().contains("declares codec"));
    }

    /// Corruption is named, with something the reader can act on.
    #[test]
    fn a_digest_that_does_not_match_the_payload_refuses() {
        let source = render(b"payload").unwrap().replace(
            "payload_hex = \"7061796c6f6164\"",
            "payload_hex = \"7061796c6f6165\"",
        );
        let error = parse(&source).unwrap_err();
        assert!(error.contains("hashes to"), "{error}");
        assert!(error.contains("will not guess"), "{error}");
    }

    #[test]
    fn a_declared_length_that_disagrees_with_the_hex_refuses() {
        let source = render(b"abc")
            .unwrap()
            .replace("payload_len = 3", "payload_len = 4");
        let error = parse(&source).unwrap_err();
        assert!(error.contains("hex character(s)"), "{error}");
    }

    /// One byte string, one spelling. Uppercase and odd-length hex both reject
    /// so a payload cannot be written two ways.
    #[test]
    fn hex_is_lowercase_and_even_length() {
        let upper = render(b"\xab").unwrap().replace("\"ab\"", "\"AB\"");
        assert!(parse(&upper).is_err());

        let odd = render(b"\xab").unwrap().replace("\"ab\"", "\"abc\"");
        assert!(parse(&odd).is_err());
    }

    #[test]
    fn a_leading_zero_in_the_length_is_not_canonical() {
        let source = render(b"abc")
            .unwrap()
            .replace("payload_len = 3", "payload_len = 03");
        assert!(parse(&source).unwrap_err().contains("leading zero"));
    }

    /// The strict subset: a reordered, re-spaced, commented or padded file is
    /// still valid TOML and is still refused. A ledger with several spellings
    /// cannot be compared byte for byte.
    #[test]
    fn a_file_that_is_valid_toml_but_not_this_subset_refuses() {
        let canonical = render(b"abc").unwrap();
        for (label, mangled) in [
            ("a comment", format!("# jails\n{canonical}")),
            ("a blank line", canonical.replacen('\n', "\n\n", 1)),
            (
                "extra spacing",
                canonical.replace("schema = 2", "schema  =  2"),
            ),
            (
                "a reordered pair",
                canonical.replace("schema = 2\ncodec", "codec_placeholder\nschema = 2\ncodec"),
            ),
            ("a trailing key", format!("{canonical}extra = 1\n")),
            ("trailing text", format!("{canonical}\n")),
            ("no final newline", canonical.trim_end().to_string()),
            ("a CR", canonical.replace('\n', "\r\n")),
            ("a BOM", format!("\u{feff}{canonical}")),
        ] {
            assert!(parse(&mangled).is_err(), "{label} was accepted");
        }
    }

    /// Every limit precedes the allocation it guards. A declared length is not
    /// a promise: this file arrives from disk, possibly after a crash.
    #[test]
    fn a_hostile_length_is_capped_before_anything_is_allocated() {
        let source = format!(
            "schema = 2\ncodec = \"{PAYLOAD_CODEC}\"\npayload_len = 999999999999\n\
             payload_sha256 = \"{}\"\npayload_hex = \"\"\n",
            "0".repeat(64)
        );
        let error = parse(&source).unwrap_err();
        assert!(error.contains("over the"), "{error}");

        let huge = "x".repeat(MAX_LEDGER_SOURCE + 1);
        assert!(parse(&huge).unwrap_err().contains("over the"));
    }

    #[test]
    fn a_payload_over_the_limit_is_refused_at_render_time_too() {
        let big = vec![0u8; MAX_LEDGER_PAYLOAD + 1];
        assert!(render(&big).unwrap_err().contains("over the"));
    }

    #[test]
    fn an_escape_sequence_has_no_meaning_in_this_subset() {
        let source = render(b"abc")
            .unwrap()
            .replace(PAYLOAD_CODEC, "jails\\u002dledger");
        assert!(parse(&source).is_err());
    }

    // -----------------------------------------------------------------------
    // The payload and the schema-1 migration
    // -----------------------------------------------------------------------

    use crate::declaration::IntentSpec;
    use crate::entity::{IntentId, Recipe};
    use crate::identity::{Name, Package};

    fn intent(name: &str, fields: &[&str]) -> AppliedEntity {
        let owned: Vec<String> = fields.iter().map(|f| f.to_string()).collect();
        AppliedEntity {
            id: EntityId::Intent(IntentId::new(
                Recipe::Record,
                Name::parse(name).unwrap(),
                Package::base(),
            )),
            owners: BTreeSet::from([OwnerId::AppManifest]),
            version: AppliedVersion {
                spec: EntitySpec::Intent(
                    IntentSpec::parse(&owned, &[], false, &Package::base()).unwrap(),
                ),
                operation: OperationId::from_bytes(jails_support::codec::sha256(b"op")),
            },
        }
    }

    #[test]
    fn a_payload_round_trips_through_the_whole_file() {
        let ledger = LedgerV2 {
            written_by: "0.1.0".to_string(),
            generation: 7,
            last_operation: Some(OperationId::from_bytes(jails_support::codec::sha256(b"x"))),
            applied: vec![intent("Alpha", &["a:string"]), intent("Beta", &["b:int"])],
            one_shots: Vec::new(),
            resources: Vec::new(),
            outputs: Vec::new(),
            legacy: vec![],
            pending_conflict: None,
        };
        let source = ledger.render().unwrap();
        assert_eq!(LedgerV2::parse_file(&source).unwrap(), ledger);
        // And the whole file is canonical, so re-rendering is byte-identical.
        assert_eq!(
            LedgerV2::parse_file(&source).unwrap().render().unwrap(),
            source
        );
    }

    /// Zero would make "never committed" and "committed once" the same
    /// recorded value.
    #[test]
    fn a_generation_starts_at_one() {
        let ledger = LedgerV2 {
            written_by: "0.1.0".to_string(),
            generation: 0,
            ..Default::default()
        };
        assert!(ledger.encode().unwrap_err().contains("generation is zero"));
    }

    /// An entity nobody owns is a contradiction: reconciliation would remove
    /// it on sight, so a row for it cannot be written.
    #[test]
    fn an_applied_row_with_no_owner_refuses() {
        let mut orphan = intent("Alpha", &[]);
        orphan.owners.clear();
        let mut encoder = Encoder::new();
        assert!(
            orphan
                .encode(&mut encoder)
                .unwrap_err()
                .contains("no owner")
        );
    }

    /// One value, one encoding — applied rows arrive in canonical order.
    #[test]
    fn applied_rows_must_be_sorted_and_unique() {
        let ledger = LedgerV2 {
            written_by: "0.1.0".to_string(),
            generation: 1,
            last_operation: None,
            applied: vec![intent("Beta", &[]), intent("Alpha", &[])],
            one_shots: Vec::new(),
            resources: Vec::new(),
            outputs: Vec::new(),
            legacy: vec![],
            pending_conflict: None,
        };
        assert!(
            ledger
                .encode()
                .unwrap_err()
                .contains("not canonically ordered")
        );

        let duplicated = LedgerV2 {
            applied: vec![intent("Alpha", &[]), intent("Alpha", &[])],
            ..ledger
        };
        assert!(duplicated.encode().unwrap_err().contains("duplicate key"));
    }

    /// Schema 1 kept `[[model]]` rows beside `[[applied]]` ones — the same
    /// fact in two places under two keys, which CLAUDE.md records as the shape
    /// of the §9.7 bug. The view is computed now, so the two cannot disagree.
    #[test]
    fn the_model_registry_is_derived_from_the_applied_rows() {
        let ledger = LedgerV2 {
            written_by: "0.1.0".to_string(),
            generation: 1,
            last_operation: None,
            applied: vec![
                intent("Beta", &["b:int"]),
                intent("Alpha", &["a:string!", "id:uuid@pk"]),
                intent("NoFields", &[]),
            ],
            one_shots: Vec::new(),
            resources: Vec::new(),
            outputs: Vec::new(),
            legacy: vec![],
            pending_conflict: None,
        };
        let models = ledger.models();
        assert_eq!(models.len(), 2, "a spec with no fields is not a model");
        assert_eq!(models[0].1, vec!["a:string!", "id:uuid@pk"]);
        assert_eq!(models[1].1, vec!["b:int"]);
        // Sorted by identity, so two runs derive the same view.
        assert!(models[0].0 < models[1].0);
    }

    /// A row whose origin is unknown stays unknown. Inventing an owner would
    /// turn machine state into human desire.
    #[test]
    fn a_row_of_unknown_origin_migrates_to_legacy_not_to_applied() {
        let ledger = migrate_schema1(
            "0.1.0",
            &[Schema1Row {
                recipe: "record".to_string(),
                name: "Note".to_string(),
                fields: vec!["title:string".to_string()],
                has_spec: None,
                paths: vec!["src/main/java/Note.java".to_string()],
                ..Default::default()
            }],
        )
        .unwrap();

        assert!(ledger.applied.is_empty(), "no invented owner");
        assert_eq!(ledger.legacy.len(), 1);
        assert_eq!(ledger.legacy[0].spec_presence, SpecPresence::UnknownLegacy);
        assert_eq!(ledger.legacy[0].paths, vec!["src/main/java/Note.java"]);
    }

    /// A known origin promotes, and the owner follows `has_spec`.
    #[test]
    fn a_row_with_a_known_origin_becomes_an_applied_entity() {
        let ledger = migrate_schema1(
            "0.1.0",
            &[
                Schema1Row {
                    recipe: "record".to_string(),
                    name: "FromManifest".to_string(),
                    fields: vec!["a:string".to_string()],
                    has_spec: Some(true),
                    ..Default::default()
                },
                Schema1Row {
                    recipe: "record".to_string(),
                    name: "FromCli".to_string(),
                    has_spec: Some(false),
                    ..Default::default()
                },
            ],
        )
        .unwrap();

        assert_eq!(ledger.applied.len(), 2);
        assert!(ledger.legacy.is_empty());
        let owners: Vec<&BTreeSet<OwnerId>> =
            ledger.applied.iter().map(|entity| &entity.owners).collect();
        assert!(owners.iter().any(|set| set.contains(&OwnerId::AppManifest)));
        assert!(owners.iter().any(|set| set.contains(&OwnerId::DirectCli)));
        // Sorted by identity: `FromCli` before `FromManifest`.
        assert!(ledger.applied[0].id < ledger.applied[1].id);
    }

    /// Refusing a row this binary cannot fully parse would strand `destroy` on
    /// exactly the projects with the most history to lose.
    #[test]
    fn a_row_this_binary_cannot_represent_is_carried_forward_not_dropped() {
        let ledger = migrate_schema1(
            "0.1.0",
            &[
                Schema1Row {
                    // A recipe no current binary knows.
                    recipe: "widget".to_string(),
                    name: "Thing".to_string(),
                    has_spec: Some(true),
                    paths: vec!["src/main/java/Thing.java".to_string()],
                    ..Default::default()
                },
                Schema1Row {
                    recipe: "record".to_string(),
                    // A name that is a Java keyword: it cannot be an identity.
                    name: "class".to_string(),
                    has_spec: Some(true),
                    ..Default::default()
                },
                Schema1Row {
                    recipe: "record".to_string(),
                    name: "Broken".to_string(),
                    // A field spec no current parser accepts.
                    fields: vec!["not a field".to_string()],
                    has_spec: Some(true),
                    ..Default::default()
                },
            ],
        )
        .unwrap();

        assert!(ledger.applied.is_empty());
        assert_eq!(ledger.legacy.len(), 3, "every row survives");
        let widget = ledger
            .legacy
            .iter()
            .find(|entry| entry.recipe == "widget")
            .expect("the unknown recipe survived");
        assert_eq!(
            widget.paths,
            vec!["src/main/java/Thing.java"],
            "and keeps the paths destroy would need"
        );
    }

    /// Migration is deterministic: the same input gives the same bytes,
    /// whatever order the rows arrived in.
    #[test]
    fn migration_is_deterministic_and_order_independent() {
        let rows = [
            Schema1Row {
                recipe: "record".to_string(),
                name: "Beta".to_string(),
                has_spec: Some(false),
                ..Default::default()
            },
            Schema1Row {
                recipe: "record".to_string(),
                name: "Alpha".to_string(),
                has_spec: Some(false),
                ..Default::default()
            },
        ];
        let one = migrate_schema1("0.1.0", &rows).unwrap().render().unwrap();
        let reversed: Vec<Schema1Row> = rows.iter().rev().cloned().collect();
        let other = migrate_schema1("0.1.0", &reversed)
            .unwrap()
            .render()
            .unwrap();
        assert_eq!(one, other);
    }

    /// A migrated ledger is a valid schema-2 file end to end.
    #[test]
    fn a_migrated_ledger_renders_and_parses() {
        let ledger = migrate_schema1(
            "0.1.0",
            &[Schema1Row {
                recipe: "record".to_string(),
                name: "Note".to_string(),
                fields: vec!["id:uuid@pk".to_string(), "title:string!".to_string()],
                has_spec: Some(true),
                paths: vec!["src/main/java/Note.java".to_string()],
                ..Default::default()
            }],
        )
        .unwrap();
        let source = ledger.render().unwrap();
        assert_eq!(LedgerV2::parse_file(&source).unwrap(), ledger);
        assert_eq!(ledger.generation, 1);
        assert_eq!(ledger.models().len(), 1);
    }
}
