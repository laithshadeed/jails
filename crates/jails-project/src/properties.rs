//! `.properties`, as a format with an owner.
//!
//! Java properties files are last-wins: two lines with one key leave the
//! second in force, and every reader agrees on that. So `set` rewrites the
//! *last* occurrence rather than the first, and appends only when the key is
//! absent — anything else changes the effective value while leaving a line
//! that looks like it still decides.
//!
//! Comments, blank lines and line order are preserved byte for byte. This is a
//! file people edit (`add db` writes into one a human owns), so the same rule
//! `pom.rs` follows applies: a surgical edit, and every other byte left alone.
//!
//! Escapes are deliberately not interpreted. jails writes and reads plain
//! `key=value` lines; a file using `\` continuations or `:` separators is read
//! conservatively — an unrecognised line is left exactly as it is rather than
//! rewritten into a form jails prefers.

use std::collections::BTreeMap;

/// Every `key=value` this file states, last-wins.
pub fn parse(text: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for line in text.lines() {
        if let Some((key, value)) = entry(line) {
            out.insert(key.to_string(), value.to_string());
        }
    }
    out
}

/// The value in force for one key.
pub fn get(text: &str, key: &str) -> Option<String> {
    text.lines()
        .filter_map(entry)
        .rfind(|(name, _)| *name == key)
        .map(|(_, value)| value.to_string())
}

/// One `key=value` line, or `None` for a comment, a blank line, or a form
/// jails does not write and will not rewrite.
fn entry(line: &str) -> Option<(&str, &str)> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('!') {
        return None;
    }
    if trimmed.ends_with('\\') {
        // A continuation: the value spans lines, and rewriting one half of it
        // would corrupt the other.
        return None;
    }
    let (key, value) = trimmed.split_once('=')?;
    let key = key.trim();
    if key.is_empty() {
        return None;
    }
    Some((key, value.trim()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "# datasource\nspring.datasource.url=jdbc:one\n\nserver.port=8080\n";

    #[test]
    fn parsing_keeps_only_real_entries() {
        let parsed = parse(SAMPLE);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed["server.port"], "8080");
    }
}
