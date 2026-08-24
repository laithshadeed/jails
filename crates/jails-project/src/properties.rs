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

/// Set a key, preserving every other byte.
///
/// Rewrites the last line stating the key, because that is the line in force.
/// Rewriting the first would leave a later line still deciding the value while
/// the edit looks applied.
pub fn set(text: &str, key: &str, value: &str) -> String {
    introduce(text, key, value, &[])
}

/// Set a key, and write prose above it *the first time it appears*.
///
/// The comment is only ever written when the key is introduced. A capability's
/// explanation is written for a human reading a file jails does not own, and
/// somebody who edits or deletes it means it: rewriting it on every reconcile
/// would be jails arguing with the reader about their own file. So an existing
/// key keeps whatever is above it, and only its value is brought into line.
pub fn introduce(text: &str, key: &str, value: &str, comment: &[String]) -> String {
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let last = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| entry(line).map(|(name, _)| name == key).unwrap_or(false))
        .map(|(index, _)| index)
        .next_back();
    match last {
        Some(index) => lines[index] = format!("{key}={value}"),
        None => {
            for line in comment {
                lines.push(format!("# {line}"));
            }
            lines.push(format!("{key}={value}"));
        }
    }
    let mut out = lines.join("\n");
    // A properties file ends with a newline; an appended key on the last line
    // of a file that did not would otherwise silently join the previous one.
    if !out.is_empty() {
        out.push('\n');
    }
    out
}

/// Remove every line stating a key, and the comment jails wrote above it.
///
/// Every occurrence of the key, because leaving an earlier one behind would
/// make the key still set after a removal.
///
/// **The comment goes only when it is jails' own, byte for byte.** A capability
/// writes prose above the setting it introduces, and prose describing a setting
/// that is no longer there is worse than no prose -- it says the file
/// configures something it does not. But a reader may have rewritten those
/// lines, or written their own above jails' key, and deleting someone's note
/// because it happened to sit in the right place is not a trade worth making.
/// So the recorded comment is matched exactly, and anything else is left.
///
/// This is also what makes "the last claim out takes the file" work: without
/// it, retiring every key leaves a file of orphaned comments, and `remove`
/// stops being the inverse of `add`.
pub fn remove(text: &str, key: &str, comment: &[String]) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let mut drop = vec![false; lines.len()];
    for (index, line) in lines.iter().enumerate() {
        if !entry(line).map(|(name, _)| name == key).unwrap_or(false) {
            continue;
        }
        drop[index] = true;
        // Walk up through exactly as many lines as jails wrote, last first.
        // A mismatch stops the walk rather than skipping the line: the block
        // is contiguous, so a changed line means the rest is not jails' any
        // more either.
        let mut above = index;
        for wanted in comment.iter().rev() {
            if above == 0 {
                break;
            }
            if lines[above - 1].trim_end() != format!("# {wanted}") {
                break;
            }
            above -= 1;
            drop[above] = true;
        }
    }
    let mut out = lines
        .iter()
        .enumerate()
        .filter(|(index, _)| !drop[*index])
        .map(|(_, line)| *line)
        .collect::<Vec<_>>()
        .join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    out
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
    #[test]
    fn a_comment_is_written_when_the_key_is_introduced() {
        let out = introduce("", "a.b", "one", &["why it is one".to_string()]);
        assert_eq!(out, "# why it is one\na.b=one\n");
    }

    #[test]
    fn an_existing_key_keeps_the_prose_above_it() {
        let text = "# the reader wrote this\na.b=one\n";
        let out = introduce(
            text,
            "a.b",
            "two",
            &["jails would have written this".to_string()],
        );
        assert_eq!(
            out, "# the reader wrote this\na.b=two\n",
            "only the value moves; prose in a file jails does not own is not rewritten"
        );
    }

    use super::*;

    const SAMPLE: &str = "# datasource\nspring.datasource.url=jdbc:one\n\nserver.port=8080\n";

    #[test]
    fn parsing_keeps_only_real_entries() {
        let parsed = parse(SAMPLE);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed["server.port"], "8080");
    }

    /// Last-wins is the language's rule, so the last line is the one in force
    /// and therefore the one an edit must rewrite.
    #[test]
    fn setting_rewrites_the_line_that_is_actually_in_force() {
        let text = "a=1\nb=2\na=3\n";
        assert_eq!(get(text, "a").as_deref(), Some("3"));
        assert_eq!(set(text, "a", "9"), "a=1\nb=2\na=9\n");
    }

    #[test]
    fn setting_an_absent_key_appends_and_leaves_every_other_byte_alone() {
        assert_eq!(
            set(SAMPLE, "server.address", "0.0.0.0"),
            format!("{SAMPLE}server.address=0.0.0.0\n")
        );
    }

    #[test]
    fn a_file_with_no_trailing_newline_does_not_join_two_keys() {
        assert_eq!(set("a=1", "b", "2"), "a=1\nb=2\n");
    }

    /// Leaving an earlier occurrence behind would leave the key set after a
    /// removal that reported success.
    #[test]
    fn removing_takes_every_occurrence() {
        assert_eq!(remove("a=1\nb=2\na=3\n", "a", &[]), "b=2\n");

        // The comment jails wrote goes with the key it describes; a comment
        // that is not jails' own stays where the reader put it.
        assert_eq!(
            remove("# why\na=1\nb=2\n", "a", &["why".to_string()]),
            "b=2\n"
        );
        assert_eq!(
            remove("# theirs\na=1\nb=2\n", "a", &["why".to_string()]),
            "# theirs\nb=2\n"
        );
    }

    #[test]
    fn a_line_jails_does_not_write_is_left_exactly_as_it_is() {
        let text = "a=1\nmulti=one \\\n  two\n";
        assert_eq!(get(text, "multi"), None);
        assert_eq!(set(text, "a", "2"), "a=2\nmulti=one \\\n  two\n");
    }

    #[test]
    fn comments_survive_a_set() {
        assert!(set(SAMPLE, "server.port", "9090").starts_with("# datasource\n"));
    }
}
