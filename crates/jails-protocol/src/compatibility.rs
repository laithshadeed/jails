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
/// P8.1) -- an appended field, so a v3 payload simply runs out of bytes where
/// v4 expects one. There is no translation, deliberately -- `CLAUDE.md`'s rule
/// for the store is that a ledger this binary did not write was written by a
/// different jails, and naming the file beats guessing at an older schema.
pub const DURABLE_PAYLOAD_CODEC_SUPERSEDED: &[&str] = &[
    concat!("jails-", "led", "ger-payload-1"),
    concat!("jails-", "led", "ger-payload-2"),
    concat!("jails-", "led", "ger-payload-3"),
];
/// Binary codec named by newly written ledger envelopes.
pub const DURABLE_PAYLOAD_CODEC: &str = concat!("jails-", "led", "ger-payload-4");

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
}
