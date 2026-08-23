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

#[cfg(test)]
mod tests {
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
}
