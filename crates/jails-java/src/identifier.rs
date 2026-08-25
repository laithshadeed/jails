//! A Java type's name, textually: which bytes change when it changes, and
//! which deliberately do not.
//!
//! This is the mechanical half of `jails rename`, at the layer that reads
//! Java rather than at the one that drives a command, because two callers
//! need it: the CLI, and the transaction route that plans a rename as one
//! atomic change. Keeping it beside `rename.rs` would have meant the route
//! reimplementing identifier boundaries and literal skipping, which is how
//! two answers to one question come to disagree.
//!
//! It is textual by design -- see `jails_engine::route::maintenance::rename`'s
//! doc comment for
//! when to prefer a language server instead. What that costs is stated rather
//! than hidden: [`literal_mentions`] counts the occurrences inside string
//! literals that [`replace_identifier`] leaves alone, so the caller can name
//! them.

use std::path::{Path, PathBuf};

/// Replace `old` with `new` wherever it appears as a whole identifier
/// outside a string or character literal. Returns the new text and how many
/// replacements were made.
pub fn replace_identifier(source: &str, old: &str, new: &str) -> (String, usize) {
    let bytes = source.as_bytes();
    let mut out = String::with_capacity(source.len());
    let mut count = 0;
    let mut i = 0;
    while i < bytes.len() {
        // Literals are copied through verbatim: renaming inside one would
        // change data, not code.
        if let Some(end) = literal_end(source, i) {
            out.push_str(&source[i..end]);
            i = end;
            continue;
        }
        if source[i..].starts_with(old) && is_boundary(source, i, i + old.len()) {
            out.push_str(new);
            count += 1;
            i += old.len();
            continue;
        }
        let ch = source[i..].chars().next().unwrap_or('\0');
        out.push(ch);
        i += ch.len_utf8();
    }
    (out, count)
}

/// Where a file ends up. A Java file is named for the type it declares, and
/// jails' own companions extend that name with a fixed suffix -- so
/// `Reward.java`, `RewardTest.java` and `RewardIT.java` all move together,
/// while `RewardHistoryService.java` (a different type that merely starts
/// with the same letters) does not.
pub fn renamed_path(path: &Path, old: &str, new: &str) -> PathBuf {
    const COMPANION_SUFFIXES: [&str; 4] = ["", "Test", "Tests", "IT"];
    let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
        return path.to_path_buf();
    };
    for suffix in COMPANION_SUFFIXES {
        if stem == format!("{old}{suffix}") {
            return path.with_file_name(format!("{new}{suffix}.java"));
        }
    }
    path.to_path_buf()
}

/// How many times `old` appears inside a string literal in one source --
/// the rename's known blind spot, counted so it can be reported.
pub fn literal_mentions(source: &str, old: &str) -> usize {
    let mut count = 0;
    let mut i = 0;
    while i < source.len() {
        if let Some(end) = literal_end(source, i) {
            count += source[i..end].matches(old).count();
            i = end;
            continue;
        }
        i += 1;
    }
    count
}

/// If a literal starts at `at`, the offset just past its closing quote.
/// Comments are deliberately not literals here -- a Javadoc line naming the
/// renamed type should follow the rename.
fn literal_end(source: &str, at: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    if source[at..].starts_with(r#"""""#) {
        let rest = &source[at + 3..];
        return Some(
            rest.find(r#"""""#)
                .map(|o| at + 3 + o + 3)
                .unwrap_or(bytes.len()),
        );
    }
    let quote = match bytes.get(at) {
        Some(b'"') => b'"',
        Some(b'\'') => b'\'',
        _ => return None,
    };
    let mut i = at + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b if b == quote => return Some(i + 1),
            // An unterminated literal is a syntax error; stopping at the
            // newline keeps one bad line from swallowing the whole file.
            b'\n' => return Some(i),
            _ => i += 1,
        }
    }
    Some(bytes.len())
}

/// True when the match at `start..end` is a whole identifier rather than a
/// slice of a longer one.
fn is_boundary(source: &str, start: usize, end: usize) -> bool {
    let before = source[..start].chars().next_back();
    let after = source[end..].chars().next();
    let part_of_identifier = |c: char| c.is_alphanumeric() || c == '_' || c == '$';
    !before.is_some_and(part_of_identifier) && !after.is_some_and(part_of_identifier)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_whole_identifiers_are_replaced() {
        let src = "class Reward { Reward r; RewardHistory h; MyReward m; reward_id x; }";
        let (out, count) = replace_identifier(src, "Reward", "Bonus");
        assert_eq!(count, 2, "{out}");
        assert!(out.contains("class Bonus { Bonus r;"), "{out}");
        assert!(out.contains("RewardHistory"), "{out}");
        assert!(out.contains("MyReward"), "{out}");
    }

    #[test]
    fn string_literals_are_left_alone() {
        let src = r#"class Reward { String s = "Reward not found"; }"#;
        let (out, count) = replace_identifier(src, "Reward", "Bonus");
        assert_eq!(count, 1);
        assert!(out.contains(r#""Reward not found""#), "{out}");
        assert!(out.contains("class Bonus"), "{out}");
    }

    #[test]
    fn text_blocks_are_left_alone() {
        let src = "class Reward { String s = \"\"\"\n  select * from Reward\n  \"\"\"; }";
        let (out, count) = replace_identifier(src, "Reward", "Bonus");
        assert_eq!(count, 1, "{out}");
        assert!(out.contains("select * from Reward"), "{out}");
    }

    #[test]
    fn comments_are_renamed() {
        // A Javadoc that still names the old type is stale documentation,
        // not data.
        let src = "/** A Reward. */\nclass Reward {}";
        let (out, count) = replace_identifier(src, "Reward", "Bonus");
        assert_eq!(count, 2);
        assert!(out.contains("/** A Bonus. */"), "{out}");
    }

    #[test]
    fn an_escaped_quote_does_not_end_a_literal() {
        let src = r#"class A { String s = "a \" Reward"; int Reward; }"#;
        let (out, count) = replace_identifier(src, "Reward", "Bonus");
        assert_eq!(count, 1, "{out}");
        assert!(out.contains(r#""a \" Reward""#), "{out}");
        assert!(out.contains("int Bonus"), "{out}");
    }

    #[test]
    fn companions_move_with_the_type() {
        let dir = Path::new("/p/src/test/java/com/example");
        assert_eq!(
            renamed_path(&dir.join("Reward.java"), "Reward", "Bonus"),
            dir.join("Bonus.java")
        );
        assert_eq!(
            renamed_path(&dir.join("RewardTest.java"), "Reward", "Bonus"),
            dir.join("BonusTest.java")
        );
        assert_eq!(
            renamed_path(&dir.join("RewardIT.java"), "Reward", "Bonus"),
            dir.join("BonusIT.java")
        );
    }

    #[test]
    fn an_unrelated_type_with_a_shared_prefix_stays_put() {
        let dir = Path::new("/p/src/main/java");
        let path = dir.join("RewardHistoryService.java");
        assert_eq!(renamed_path(&path, "Reward", "Bonus"), path);
    }
}
