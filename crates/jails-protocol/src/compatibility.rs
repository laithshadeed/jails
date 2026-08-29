//! Compatibility identifiers for every user-visible or durable root format.
//!
//! Keep these in the protocol crate even when the reader lives elsewhere. A
//! version literal beside one parser is easy to change without noticing the
//! writer, the daemon on the other side of a socket, or the compatibility
//! inventory. These names are the shared contract.

/// `.jails/app.toml`'s top-level schema number.
pub const APP_MANIFEST_SCHEMA: u32 = 1;

/// The resident test daemon's line protocol.
pub const TESTD_PROTOCOL: &str = "jails.testd.v1";
/// Numeric component used where a filesystem-safe, compact daemon tag is
/// required (notably Unix socket names).
pub const TESTD_PROTOCOL_VERSION: u32 = 1;
/// Canonical framed protocol used by the unified test coordinator.
pub const TESTD_V2_PROTOCOL: &str = "jails.testd.v2";

/// Reserved for the SQL contract format described by the roadmap. No reader
/// or writer ships yet; reserving the identifier prevents a future format
/// from accidentally giving a different meaning to v1.
pub const SQL_CONTRACT_SCHEMA: &str = "jails.sql-contract.v1";

/// `.jails/ledger.toml`'s closed envelope schema.
pub const DURABLE_ENVELOPE_SCHEMA: u32 = 2;
/// Payload codecs this jails can no longer read, named so the refusal can say
/// *which* older format it found rather than "not mine".
///
/// The first two were readable until the recorded column binding went on the
/// wire (plan.md P3.2): a field name is a `(java, column)` pair now, and a
/// payload carrying only the Java half has no second value to promote. The
/// third stopped when a recorded intent gained its join (`--via`, plan.md
/// P8.1) and the fourth when it gained its order and row ceiling
/// (`--order-by`/`--limit`, plan.md P8.2) and the fifth when it gained its
/// conflict key (`--on-conflict`, plan.md P8.3) and the sixth when it gained a
/// named route (`--path`, plan.md P8.7) and the seventh when it gained the
/// format its endpoint reads (`--consumes`, plan.md P10.2) and the eighth when
/// it gained the row selector and the components pinned to a constant
/// (`--select`/`--set`, plan.md P10.7) and the ninth when it gained a
/// transition's `If-Match` policy (`--if-match`, plan.md P10.7) and the tenth
/// when it gained the request-parameter names a bound record answers to
/// (`--bind`, plan.md P10.7) -- appended fields, so an older
/// payload simply runs out of bytes where the newer one expects some. There is no translation, deliberately -- `CLAUDE.md`'s rule
/// for the store is that a ledger this binary did not write was written by a
/// different jails, and naming the file beats guessing at an older schema.
pub const DURABLE_PAYLOAD_CODEC_SUPERSEDED: &[&str] = &[
    concat!("jails-", "led", "ger-payload-1"),
    concat!("jails-", "led", "ger-payload-2"),
    concat!("jails-", "led", "ger-payload-3"),
    concat!("jails-", "led", "ger-payload-4"),
    concat!("jails-", "led", "ger-payload-5"),
    concat!("jails-", "led", "ger-payload-6"),
    concat!("jails-", "led", "ger-payload-7"),
    concat!("jails-", "led", "ger-payload-8"),
    concat!("jails-", "led", "ger-payload-9"),
    concat!("jails-", "led", "ger-payload-10"),
];
/// Binary codec named by newly written ledger envelopes.
pub const DURABLE_PAYLOAD_CODEC: &str = concat!("jails-", "led", "ger-payload-11");

/// Transaction journal root-format marker, including its fixed-width NUL.
pub const JOURNAL_MAGIC: &[u8; 16] = b"JAILS-JOURNAL-1\0";
/// Published receipt root-format marker, including its fixed-width NUL.
pub const RECEIPT_MAGIC: &[u8; 16] = b"JAILS-RECEIPT-1\0";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daemon_numeric_and_text_versions_agree() {
        assert_eq!(
            TESTD_PROTOCOL,
            format!("jails.testd.v{TESTD_PROTOCOL_VERSION}")
        );
    }

    #[test]
    fn root_markers_keep_the_fixed_wire_width() {
        assert_eq!(JOURNAL_MAGIC.len(), 16);
        assert_eq!(RECEIPT_MAGIC.len(), 16);
    }

    #[test]
    fn the_checked_in_inventory_names_every_compatibility_root() {
        let inventory = include_str!("../../../docs/compatibility.tsv");
        for identifier in [
            format!("schema={APP_MANIFEST_SCHEMA}"),
            TESTD_PROTOCOL.to_string(),
            TESTD_V2_PROTOCOL.to_string(),
            SQL_CONTRACT_SCHEMA.to_string(),
            format!("schema={DURABLE_ENVELOPE_SCHEMA}"),
            DURABLE_PAYLOAD_CODEC.to_string(),
            "JAILS-JOURNAL-1\\0".to_string(),
            "JAILS-RECEIPT-1\\0".to_string(),
        ] {
            assert!(
                inventory.contains(&identifier),
                "compatibility inventory is missing {identifier}"
            );
        }
        for superseded in DURABLE_PAYLOAD_CODEC_SUPERSEDED {
            assert!(
                inventory.contains(&format!("{superseded}\tsuperseded")),
                "the inventory must record {superseded} as unreadable"
            );
        }
        assert!(
            inventory.contains("sql_contract\tjails.sql-contract.v1\treserved"),
            "the unimplemented SQL contract must be explicitly reserved"
        );
    }

    /// The daemon's v1 line protocol, against captured bytes of each side.
    ///
    /// These two fixtures sat in `tests/protocol-golden/` referenced by
    /// nothing, which `simplify-sol.md`'s audit called out: their presence
    /// proved nothing, and the wire could have moved underneath them without
    /// a word. They are read here because this module is where the protocol
    /// identifier lives, so changing [`TESTD_PROTOCOL`] now fails against a
    /// real request and a real reply rather than against another constant.
    ///
    /// The shape asserted is the framing, not the payload: a handshake line,
    /// then a verb and its arguments terminated by a blank line, and on the
    /// way back a message ending in `EOT` plus the exit status. A daemon that
    /// changes any of those is not speaking v1, whatever it calls itself.
    #[test]
    fn the_v1_daemon_fixtures_are_the_protocol_this_crate_names() {
        let request = decode_hex(include_str!(
            "../../../tests/protocol-golden/testd-request.hex"
        ));
        let reply = decode_hex(include_str!(
            "../../../tests/protocol-golden/testd-reply.hex"
        ));

        for (side, bytes) in [("request", &request), ("reply", &reply)] {
            let text = String::from_utf8(bytes.to_vec())
                .unwrap_or_else(|_| panic!("the {side} fixture is not UTF-8"));
            assert_eq!(
                text.lines().next(),
                Some(TESTD_PROTOCOL),
                "the {side} fixture opens with a different handshake than \
                 `TESTD_PROTOCOL`"
            );
        }

        let request = String::from_utf8(request).expect("checked above");
        let mut lines = request.lines();
        lines.next();
        assert_eq!(lines.next(), Some("RUN"), "a v1 request names its verb");
        assert!(
            request.ends_with("\n\n"),
            "a v1 request's argument list is terminated by a blank line"
        );

        let reply = String::from_utf8(reply).expect("checked above");
        let (message, status) = reply
            .split_once('\u{4}')
            .expect("a v1 reply ends its message with EOT before the exit status");
        assert!(
            message.ends_with('\n'),
            "a v1 reply's message is line-terminated before EOT"
        );
        assert!(
            status.trim_end().parse::<i32>().is_ok(),
            "a v1 reply's trailer is the exit status, found {status:?}"
        );
    }

    /// The fixtures are stored as hex so a diff shows the bytes, including the
    /// control characters the framing depends on.
    fn decode_hex(text: &str) -> Vec<u8> {
        let digits: Vec<u8> = text
            .bytes()
            .filter(|byte| !byte.is_ascii_whitespace())
            .collect();
        assert!(
            digits.len().is_multiple_of(2),
            "a hex fixture has an odd digit count"
        );
        digits
            .chunks(2)
            .map(|pair| {
                u8::from_str_radix(std::str::from_utf8(pair).expect("hex digits are ASCII"), 16)
                    .expect("a hex fixture holds only hex digits")
            })
            .collect()
    }
}
