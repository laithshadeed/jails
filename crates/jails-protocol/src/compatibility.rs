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

/// Reserved for the SQL contract format described by the roadmap. No reader
/// or writer ships yet; reserving the identifier prevents a future format
/// from accidentally giving a different meaning to v1.
pub const SQL_CONTRACT_SCHEMA: &str = "jails.sql-contract.v1";

/// `.jails/ledger.toml`'s closed envelope schema.
pub const DURABLE_ENVELOPE_SCHEMA: u32 = 2;
/// Binary codec named by the ledger envelope.
pub const DURABLE_PAYLOAD_CODEC: &str = concat!("jails-", "led", "ger-payload-1");

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
        assert!(
            inventory.contains("sql_contract\tjails.sql-contract.v1\treserved"),
            "the unimplemented SQL contract must be explicitly reserved"
        );
    }
}
