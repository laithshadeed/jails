//! How the accepted projection's bytes sit in `.jails/compiler.lock.json`.
//!
//! **A byte as a JSON integer costs four characters.** The lock is the merge
//! base -- one exact copy of every managed file -- and `serde`'s default for
//! `Vec<u8>` is an array of integers, so a 25 kB source tree was recorded as a
//! 446 kB lock: seventeen times the thing it describes, unreadable in a diff,
//! and a new blob in git on every mutation. Generated files are text, so the
//! bytes go in as text.
//!
//! **The type is untouched, and so is the digest rule.** `projection_digest`
//! is a digest of `serde_json::to_vec` of [`crate::RenderedTree`] as `serde`
//! derives it, and the reader recomputes exactly that from the value it
//! decoded. Changing the derive would change the preimage and every lock ever
//! written would stop verifying; changing only what the *file* holds leaves
//! the preimage alone, so a lock written by the previous release verifies
//! unchanged and is rewritten in the new shape by the next mutation.
//!
//! **This is the writing half only.** Reading is
//! [`crate::bytes_field`]: the fields decode from text or from an array
//! directly, so nothing rewrites one into the other on the way in. There used
//! to be an `expand` here that did, and on a hundred-entity project it cost
//! 95 ms of a 122 ms capture -- three million `serde_json::Number`
//! allocations to recover bytes that were already contiguous in the file.
//!
//! The transform is deliberately narrow -- it names the two places bytes live
//! rather than walking for any field called `bytes` -- because a blind walk
//! would reach into a model whose own fields it knows nothing about.

use serde_json::Value;

/// The key the compact form uses, chosen so an older reader fails to find
/// `bytes` rather than reading a string as an array of small integers.
const TEXT: &str = "text";

/// Rewrite a lock's `projection` and `migration_bytes` into the compact form.
///
/// Anything that is not valid UTF-8 stays an array: a `byte[]` field, or a
/// template that ships a binary, is rare and is not worth a second encoding
/// to carry.
pub fn compact(lock: &mut Value) {
    if let Some(files) = lock
        .get_mut("projection")
        .and_then(|projection| projection.get_mut("files"))
        .and_then(Value::as_object_mut)
    {
        for file in files.values_mut() {
            compact_entry(file);
        }
    }
    if let Some(facets) = lock
        .get_mut("projection")
        .and_then(|projection| projection.get_mut("reader_facets"))
        .and_then(Value::as_object_mut)
    {
        for facet in facets.values_mut() {
            compact_entry(facet);
        }
    }
    if let Some(migrations) = lock
        .get_mut("migration_bytes")
        .and_then(Value::as_object_mut)
    {
        for bytes in migrations.values_mut() {
            if let Some(text) = as_text(bytes) {
                *bytes = Value::String(text);
            }
        }
    }
}

fn compact_entry(entry: &mut Value) {
    let Some(object) = entry.as_object_mut() else {
        return;
    };
    let Some(text) = object.get("bytes").and_then(as_text) else {
        return;
    };
    object.remove("bytes");
    object.insert(TEXT.to_string(), Value::String(text));
}

/// The bytes as text, when every one of them is part of valid UTF-8.
fn as_text(bytes: &Value) -> Option<String> {
    let numbers = bytes.as_array()?;
    let mut raw = Vec::with_capacity(numbers.len());
    for number in numbers {
        raw.push(u8::try_from(number.as_u64()?).ok()?);
    }
    String::from_utf8(raw).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What the compact form writes, and what it leaves alone.
    #[test]
    fn compacting_writes_text_and_keeps_what_is_not_utf8() {
        let mut lock = serde_json::json!({
            "projection": {
                "files": {
                    "src/A.java": {"kind": "java-main", "mode": "regular", "bytes": [104, 105]},
                    "src/B.bin": {"kind": "java-main", "mode": "regular", "bytes": [0, 159, 146]},
                },
                "reader_facets": {
                    "compose": {"path": "compose.yaml", "bytes": [97]},
                },
            },
            "migration_bytes": {"src/V1.sql": [98, 99]},
        });
        let before = lock.clone();
        compact(&mut lock);
        assert_eq!(lock["projection"]["files"]["src/A.java"]["text"], "hi");
        assert!(
            lock["projection"]["files"]["src/A.java"]
                .get("bytes")
                .is_none(),
            "the compact form carries one spelling"
        );
        // Not UTF-8, so it stays an array rather than growing a second
        // encoding for the rare case.
        assert!(lock["projection"]["files"]["src/B.bin"]["bytes"].is_array());
        assert_eq!(lock["projection"]["reader_facets"]["compose"]["text"], "a");
        assert_eq!(lock["migration_bytes"]["src/V1.sql"], "bc");

        // There is no inverse here any more: `crate::bytes_field` reads
        // either shape straight into `Vec<u8>`, which is what removed the
        // rewrite this module used to do on the way in.
        let _ = before;
    }

    /// The compact form is smaller by the factor the item is about.
    #[test]
    fn text_is_a_quarter_of_the_size_of_the_array() {
        let body = "public record Note(String title) {}\n".repeat(20);
        let mut lock = serde_json::json!({
            "projection": {"files": {"src/Note.java": {"bytes": body.as_bytes()}}},
        });
        let expanded = serde_json::to_vec(&lock).unwrap().len();
        compact(&mut lock);
        let compacted = serde_json::to_vec(&lock).unwrap().len();
        assert!(
            compacted * 3 < expanded,
            "compact {compacted} against expanded {expanded}"
        );
    }
}
