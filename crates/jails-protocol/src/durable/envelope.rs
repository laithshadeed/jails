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
use crate::compatibility::{
    DURABLE_ENVELOPE_SCHEMA as SCHEMA, DURABLE_PAYLOAD_CODEC as PAYLOAD_CODEC,
    DURABLE_PAYLOAD_CODEC_SUPERSEDED as SUPERSEDED_CODECS,
};
use jails_support::codec;

/// 32 MiB of decoded payload.
pub(crate) const MAX_LEDGER_PAYLOAD: usize = 32 * 1024 * 1024;
/// Two hex characters per byte, plus the fixed envelope allowance.
pub(crate) const MAX_LEDGER_SOURCE: usize = 2 * MAX_LEDGER_PAYLOAD + 512;

/// Render the envelope for one payload.
///
/// Always ends in exactly one LF.
pub fn render(payload: &[u8]) -> Result<String> {
    if payload.len() > MAX_LEDGER_PAYLOAD {
        return Err(format!(
            "ledger payload is {} bytes, over the {MAX_LEDGER_PAYLOAD}-byte limit",
            payload.len()
        )
        .into());
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
        )
        .into());
    }
    if source.starts_with('\u{feff}') {
        return Err(jails_support::Failure::Told(
            "ledger begins with a byte-order mark".to_string(),
        ));
    }
    if source.contains('\r') {
        return Err(jails_support::Failure::Told(
            "ledger contains a CR; line endings are LF".to_string(),
        ));
    }
    if !source.ends_with('\n') {
        return Err(jails_support::Failure::Told(
            "ledger does not end with a newline".to_string(),
        ));
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
        )
        .into());
    }

    let schema = value_of(lines[0], "schema")?;
    if schema != SCHEMA.to_string() {
        return Err(format!(
            "ledger declares schema {schema}.\n       fix: this jails reads schema {SCHEMA}. A \
             newer schema is refused rather than half-read; upgrade jails to a version that \
             supports it."
        )
        .into());
    }
    let declared_codec = quoted(value_of(lines[1], "codec")?)?;
    if declared_codec != PAYLOAD_CODEC {
        // A codec jails once wrote is named as such. "not mine" and "mine,
        // one format ago" are different facts, and only the second one tells
        // the reader that the file in front of them is a jails ledger.
        let superseded = SUPERSEDED_CODECS.contains(&declared_codec);
        let note = if superseded {
            " That codec was written by an older jails and there is no translation to this one."
        } else {
            ""
        };
        return Err(format!(
            "ledger declares codec `{declared_codec}`, and this jails reads \
             `{PAYLOAD_CODEC}`.{note}\n       \
             fix: upgrade jails to a version that supports that codec; this version will not \
             guess."
        )
        .into());
    }

    let declared_len = decimal(value_of(lines[2], "payload_len")?)?;
    if declared_len > MAX_LEDGER_PAYLOAD {
        return Err(format!(
            "ledger declares a {declared_len}-byte payload, over the \
             {MAX_LEDGER_PAYLOAD}-byte limit"
        )
        .into());
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
        )
        .into());
    }
    let payload = codec::unhex_bytes(hex)?;

    if payload.len() != declared_len {
        return Err(format!(
            "ledger payload decoded to {} byte(s), not the declared {declared_len}",
            payload.len()
        )
        .into());
    }
    let actual = codec::hex(&codec::sha256(&payload));
    if actual != declared_digest {
        return Err(format!(
            "ledger payload hashes to {actual}, not the recorded {declared_digest}.\n       \
             fix: the file is corrupt. Restore it from version control; jails will not guess \
             what it recorded."
        )
        .into());
    }

    // A file that parses but does not re-render identically has a second
    // spelling, and a ledger with two spellings cannot be compared byte for
    // byte -- which is what every identity here rests on.
    if render(&payload)? != source {
        return Err(jails_support::Failure::Told(
            "ledger is not in canonical form.\n       fix: it parses, but re-rendering it \
             produces different bytes, so two files would mean one ledger. Rewrite it with \
             this jails."
                .to_string(),
        ));
    }
    Ok(payload)
}

/// `key = value`, with the exact single spaces the format fixes.
fn value_of<'a>(line: &'a str, key: &str) -> Result<&'a str> {
    let prefix = format!("{key} = ");
    Ok(line
        .strip_prefix(&prefix)
        .ok_or_else(|| format!("expected a line `{key} = …`, found `{line}`"))?)
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
        )
        .into());
    }
    Ok(inner)
}

/// Unsigned canonical decimal: no sign, no leading zero except `0` itself.
fn decimal(value: &str) -> Result<usize> {
    if value.is_empty() {
        return Err(jails_support::Failure::Told(
            "expected a decimal number, found nothing".to_string(),
        ));
    }
    if !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(format!("`{value}` is not an unsigned decimal number").into());
    }
    if value.len() > 1 && value.starts_with('0') {
        return Err(format!("`{value}` has a leading zero; the canonical form has none").into());
    }
    value
        .parse()
        .map_err(|_| format!("`{value}` does not fit this platform's usize").into())
}

// ---------------------------------------------------------------------------
// The payload
// ---------------------------------------------------------------------------

use crate::entity::{EntityId, EntitySpec, OneShotId};
use crate::identity::{OperationId, ProjectPath};
use crate::lifecycle::ResourceLifecycleV1;
use crate::record::{AppliedEntity, OneShotReceipt, OutputRecord};
use crate::resource::{ResourceKey, ResourceOwner, ResourceRecord, ResourceValue};
use jails_support::codec::{Codec, Decoder, Encoder, ordered};
use std::collections::BTreeSet;

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
    /// Stable entity identity, declared model, retirement state, and sealed
    /// migration lineage. Old payloads decode this append-only registry as
    /// empty and populate it on the first lifecycle-aware mutation.
    pub lifecycles: Vec<ResourceLifecycleV1>,
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

impl Codec for PendingMarker {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.operation.encode(encoder)?;
        if self.generation == 0 {
            return Err(jails_support::Failure::Told(
                "a pending conflict records generation zero".to_string(),
            ));
        }
        encoder.u64(self.generation);
        self.request_syntax.encode(encoder)?;
        encoder.string(&self.resume_display)
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        let operation = OperationId::decode(decoder)?;
        let generation = decoder.u64()?;
        if generation == 0 {
            return Err(jails_support::Failure::Told(
                "a pending conflict records generation zero".to_string(),
            ));
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
            return Err(jails_support::Failure::Told(
                "generation is zero; it is incremented once per commit and starts at one"
                    .to_string(),
            ));
        }
        encoder.u64(self.generation);
        encoder.option(self.last_operation.as_ref(), |e, id| {
            id.encode(e)?;
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
        encoder.option(self.pending_conflict.as_ref(), |e, marker| marker.encode(e))?;
        encoder.count(self.lifecycles.len())?;
        let mut previous: Option<&EntityId> = None;
        for lifecycle in &self.lifecycles {
            ordered(previous, &lifecycle.entity)?;
            previous = Some(&lifecycle.entity);
            lifecycle.encode(&mut encoder)?;
        }
        encoder.finish()
    }

    pub fn decode(payload: &[u8]) -> Result<Self> {
        let mut decoder = Decoder::new(payload)?;
        let written_by = decoder.string()?;
        let generation = decoder.u64()?;
        if generation == 0 {
            return Err(jails_support::Failure::Told(
                "ledger generation is zero".to_string(),
            ));
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
        let pending_conflict = decoder.option(PendingMarker::decode)?;
        let mut lifecycles = Vec::new();
        if !decoder.is_finished() {
            let count = decoder.count()?;
            for _ in 0..count {
                let lifecycle = ResourceLifecycleV1::decode(&mut decoder)?;
                ordered(
                    lifecycles
                        .last()
                        .map(|last: &ResourceLifecycleV1| &last.entity),
                    &lifecycle.entity,
                )?;
                lifecycles.push(lifecycle);
            }
        }
        decoder.finish()?;
        Ok(Self {
            written_by,
            generation,
            last_operation,
            applied,
            one_shots,
            resources,
            outputs,
            lifecycles,
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
                EntitySpec::Intent(intent) if !intent.arguments.is_empty() => {
                    Some((entity.id.clone(), intent.arguments.canonical()))
                }
                _ => None,
            })
            .collect();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }
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
            lifecycles: vec![],
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
             codec = \"jails-ledger-payload-11\"\n\
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
             codec = \"jails-ledger-payload-11\"\n\
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
        let error = parse(&source).unwrap_err();
        assert!(error.contains("declares codec"), "{error}");
        assert!(!error.contains("older jails"), "{error}");
    }

    /// plan.md P3.2. A ledger this jails wrote one format ago is refused as
    /// what it is, not as a stranger.
    #[test]
    fn a_superseded_codec_refuses_by_name() {
        for superseded in SUPERSEDED_CODECS {
            let source = render(b"x").unwrap().replace(PAYLOAD_CODEC, superseded);
            let error = parse(&source).unwrap_err();
            assert!(error.contains(superseded), "{error}");
            assert!(error.contains("older jails"), "{error}");
            assert!(error.contains("no translation"), "{error}");
        }
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
    use crate::entity::OwnerId;
    use crate::entity::{IntentId, Recipe};
    use crate::identity::{Name, Package};
    use crate::record::AppliedVersion;

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
                    IntentSpec::parse(
                        crate::entity::Recipe::Record,
                        &owned,
                        &[],
                        false,
                        &Package::base(),
                    )
                    .unwrap(),
                ),
                operation: OperationId::from_bytes(jails_support::codec::sha256(b"op")),
            },
        }
    }

    #[test]
    fn ledger_file_matches_the_protocol_golden() {
        let ledger = LedgerV2 {
            written_by: "0.1.0".to_string(),
            generation: 7,
            last_operation: Some(OperationId::from_bytes(jails_support::codec::sha256(b"x"))),
            applied: vec![intent("Note", &["title:string!"])],
            one_shots: Vec::new(),
            resources: Vec::new(),
            outputs: Vec::new(),
            lifecycles: vec![],
            pending_conflict: None,
        };
        let actual = ledger.render().unwrap();
        let expected = include_str!("../../../../tests/protocol-golden/ledger-v11.toml");
        assert_eq!(actual, expected);
        assert_eq!(LedgerV2::parse_file(expected).unwrap(), ledger);
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
            lifecycles: vec![],
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
            lifecycles: vec![],
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
            lifecycles: vec![],
            pending_conflict: None,
        };
        let models = ledger.models();
        assert_eq!(models.len(), 2, "a spec with no fields is not a model");
        assert_eq!(models[0].1, vec!["a:string!", "id:uuid@pk"]);
        assert_eq!(models[1].1, vec!["b:int"]);
        // Sorted by identity, so two runs derive the same view.
        assert!(models[0].0 < models[1].0);
    }
}
