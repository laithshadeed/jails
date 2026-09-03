//! Reading a byte string that may be stored as text.
//!
//! **The lock writes text and the type decodes bytes.** `Vec<u8>` serialises
//! as an array of integers, which is four characters a byte; the accepted
//! projection is one exact copy of every managed file, so
//! [`crate::lock_bytes::compact`] writes it as a JSON string instead. Reading
//! it back used to mean rewriting the string into an array of
//! `serde_json::Number` and decoding *that*, which on a hundred-entity
//! project cost 95 ms of the 122 ms capture -- three million allocations to
//! recover bytes that were already contiguous in the file.
//!
//! So the field reads either shape directly. Serialization is untouched, and
//! that matters: `projection_digest` is a digest of `serde_json::to_vec` of
//! the derived form, so a lock written by any release still verifies.

use serde::Deserializer;
use serde::de::{Error, SeqAccess, Visitor};
use std::fmt::{Formatter, Result as FmtResult};

/// Bytes from a JSON string or from an array of integers.
pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
where
    D: Deserializer<'de>,
{
    deserializer.deserialize_any(EitherShape)
}

struct EitherShape;

impl<'de> Visitor<'de> for EitherShape {
    type Value = Vec<u8>;

    fn expecting(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        formatter.write_str("a byte string, as text or as an array of integers")
    }

    fn visit_str<E: Error>(self, value: &str) -> Result<Self::Value, E> {
        Ok(value.as_bytes().to_vec())
    }

    fn visit_string<E: Error>(self, value: String) -> Result<Self::Value, E> {
        Ok(value.into_bytes())
    }

    fn visit_bytes<E: Error>(self, value: &[u8]) -> Result<Self::Value, E> {
        Ok(value.to_vec())
    }

    fn visit_byte_buf<E: Error>(self, value: Vec<u8>) -> Result<Self::Value, E> {
        Ok(value)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        let mut out = Vec::with_capacity(seq.size_hint().unwrap_or_default());
        while let Some(byte) = seq.next_element::<u8>()? {
            out.push(byte);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct Holder {
        #[serde(alias = "text", deserialize_with = "super::deserialize")]
        bytes: Vec<u8>,
    }

    /// Both shapes decode to the same bytes, which is what lets one release
    /// read a lock another wrote.
    #[test]
    fn text_and_an_array_of_integers_decode_alike() {
        let from_text: Holder = serde_json::from_str(r#"{"text": "hi"}"#).unwrap();
        let from_array: Holder = serde_json::from_str(r#"{"bytes": [104, 105]}"#).unwrap();
        assert_eq!(from_text.bytes, b"hi");
        assert_eq!(from_array.bytes, from_text.bytes);
    }

    /// A byte outside ASCII survives both ways round.
    #[test]
    fn a_multi_byte_character_keeps_its_bytes() {
        let from_text: Holder = serde_json::from_str("{\"text\": \"\u{00e9}\"}").unwrap();
        assert_eq!(from_text.bytes, "é".as_bytes());
        let from_array: Holder = serde_json::from_str(r#"{"bytes": [195, 169]}"#).unwrap();
        assert_eq!(from_array.bytes, from_text.bytes);
    }
}
