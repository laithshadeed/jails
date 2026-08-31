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

/// `transactionId` -> `transaction_id`. Runs of capitals stay together
/// (`customerURL` -> `customer_url`) so an acronym does not explode into one
/// underscore per letter.
///
/// **Here rather than beside the SQL names it produces**, because two layers
/// need it and only one of them may depend on the other: `jails_spec` reads a
/// record off disk and has to say which column each component is,
/// `jails_protocol::identity::SqlName` validates the result, and `SqlName`
/// sits above `jails_spec`. A second copy at the lower layer is the shape of
/// every drift bug in this repository -- and this function is the one step
/// that decides whether two spellings of a field are one field, so a copy
/// that disagreed by a character would split them.
pub fn snake_case(value: &str) -> String {
    let chars: Vec<char> = value.chars().collect();
    let mut out = String::with_capacity(value.len() + 4);
    for (index, &character) in chars.iter().enumerate() {
        if character.is_uppercase() {
            let starts_run = index > 0 && !chars[index - 1].is_uppercase();
            let ends_run = index > 0
                && chars[index - 1].is_uppercase()
                && chars.get(index + 1).is_some_and(|next| next.is_lowercase());
            if starts_run || ends_run {
                out.push('_');
            }
            out.extend(character.to_lowercase());
        } else {
            out.push(character);
        }
    }
    out
}

/// A reader-typed migration description, normalised or refused.
///
/// `Add Note ArchivedAt`, `add-note-archived-at` and `addNoteArchivedAt` all
/// become `add_note_archived_at`; anything holding a character that has no
/// place in a filename -- a slash, a quote, a dot -- is refused rather than
/// stripped, because the result becomes `V<n>__<this>.sql` and a description
/// silently shortened to something else names a migration nobody typed.
///
/// **Distinct from [`snake_case`], which normalises an identifier that is
/// already one.** This one validates: its input is a phrase a person typed at
/// a prompt, so runs of separators collapse, leading and trailing ones go, and
/// an empty result is an error naming what to type instead.
///
/// It is here rather than beside the migrations it names because it is a
/// naming rule and this is where naming lives -- and because its one caller is
/// the canonical `jails g migration`, which must not reach into a crate the
/// cutover deletes.
pub fn sql_name(value: &str) -> crate::Result<String> {
    let mut out = String::new();
    let mut previous_was_lower_or_digit = false;
    for ch in value.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            if ch.is_ascii_uppercase() && previous_was_lower_or_digit && !out.ends_with('_') {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
            previous_was_lower_or_digit = ch.is_ascii_lowercase() || ch.is_ascii_digit();
        } else if matches!(ch, '-' | '_' | ' ') {
            if !out.is_empty() && !out.ends_with('_') {
                out.push('_');
            }
            previous_was_lower_or_digit = false;
        } else {
            return Err(format!("'{value}' is not a usable SQL migration name").into());
        }
    }
    while out.ends_with('_') {
        out.pop();
    }
    if out.is_empty() {
        Err(crate::Failure::Told(
            "a migration needs a description, e.g. `jails g migration create_rewards`".to_string(),
        ))
    } else {
        Ok(out)
    }
}

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

/// Replace a logical-name component inside Java identifiers, outside literals.
/// This is for generator-owned projections only: `Task`, `TaskController`,
/// `JdbcTaskRepository`, and method references such as `toTask` are all
/// derived from the same entity name and move together.
pub fn replace_owned_identifier_component(source: &str, old: &str, new: &str) -> (String, usize) {
    let mut out = String::with_capacity(source.len());
    let mut count = 0;
    let mut i = 0;
    while i < source.len() {
        if let Some(end) = literal_end(source, i) {
            out.push_str(&source[i..end]);
            i = end;
            continue;
        }
        let ch = source[i..].chars().next().unwrap_or('\0');
        if ch.is_alphabetic() || ch == '_' || ch == '$' {
            let start = i;
            i += ch.len_utf8();
            while i < source.len() {
                let next = source[i..].chars().next().unwrap_or('\0');
                if !(next.is_alphanumeric() || next == '_' || next == '$') {
                    break;
                }
                i += next.len_utf8();
            }
            let (token, hits) = replace_bounded_component(&source[start..i], old, new);
            out.push_str(&token);
            count += hits;
            continue;
        }
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
        i += source[i..].chars().next().map_or(1, char::len_utf8);
    }
    count
}

/// Replace a whole SQL identifier, but only inside Java string literals and
/// text blocks. This is intentionally narrower than general text replacement:
/// coordinated storage cutovers use it only for generator-owned Java files,
/// where SQL is data embedded in a known projection. Code, comments, and
/// longer identifiers remain byte-identical.
pub fn replace_literal_sql_identifier(source: &str, old: &str, new: &str) -> (String, usize) {
    let mut out = String::with_capacity(source.len());
    let mut count = 0;
    let mut i = 0;
    while i < source.len() {
        let Some(end) = literal_end(source, i) else {
            let ch = source[i..].chars().next().unwrap_or('\0');
            out.push(ch);
            i += ch.len_utf8();
            continue;
        };
        let literal = &source[i..end];
        if looks_like_sql(literal) {
            let (replaced, hits) = replace_bounded(literal, old, new);
            out.push_str(&replaced);
            count += hits;
        } else {
            out.push_str(literal);
        }
        i = end;
    }
    (out, count)
}

fn looks_like_sql(literal: &str) -> bool {
    let lower = literal.to_ascii_lowercase();
    [
        "select ",
        "insert ",
        "update ",
        "delete ",
        " from ",
        " join ",
        "alter table ",
    ]
    .iter()
    .any(|keyword| lower.contains(keyword))
}

fn replace_bounded_component(source: &str, old: &str, new: &str) -> (String, usize) {
    let mut out = String::with_capacity(source.len());
    let mut count = 0;
    let mut at = 0;
    while let Some(relative) = source[at..].find(old) {
        let start = at + relative;
        let end = start + old.len();
        let before = source[..start].chars().next_back();
        let after = source[end..].chars().next();
        let starts_component = before.is_none_or(|character| {
            !character.is_alphanumeric()
                || character == '_'
                || (character.is_lowercase() && old.chars().next().is_some_and(char::is_uppercase))
        });
        let ends_component =
            after.is_none_or(|character| !character.is_alphanumeric() || character.is_uppercase());
        out.push_str(&source[at..start]);
        if starts_component && ends_component {
            out.push_str(new);
            count += 1;
        } else {
            out.push_str(old);
        }
        at = end;
    }
    out.push_str(&source[at..]);
    (out, count)
}

/// Count whole identifier mentions without assigning them a meaning. The
/// resource planner uses this to conservatively block reader-owned SQL and
/// embedded SQL before a physical cutover.
pub fn bounded_mentions(source: &str, identifier: &str) -> usize {
    let mut count = 0;
    let mut at = 0;
    while let Some(relative) = source[at..].find(identifier) {
        let start = at + relative;
        let end = start + identifier.len();
        if is_boundary(source, start, end) {
            count += 1;
        }
        at = end;
    }
    count
}

fn replace_bounded(source: &str, old: &str, new: &str) -> (String, usize) {
    let mut out = String::with_capacity(source.len());
    let mut count = 0;
    let mut at = 0;
    while let Some(relative) = source[at..].find(old) {
        let start = at + relative;
        let end = start + old.len();
        out.push_str(&source[at..start]);
        if is_boundary(source, start, end) {
            out.push_str(new);
            count += 1;
        } else {
            out.push_str(old);
        }
        at = end;
    }
    out.push_str(&source[at..]);
    (out, count)
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
    fn unicode_outside_literals_keeps_every_scan_on_character_boundaries() {
        let src = "// café — Reward\nclass Reward {}";
        assert_eq!(literal_mentions(src, "Reward"), 0);
        let (out, count) = replace_identifier(src, "Reward", "Bonus");
        assert_eq!(count, 2, "{out}");
        assert!(out.contains("// café — Bonus"), "{out}");
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

    #[test]
    fn a_cutover_rewrites_only_whole_identifiers_inside_literals() {
        let src = "// tasks stays documentation\nclass Task { String sql = \"select * from tasks where task_status = ?\"; String block = \"\"\"\ninsert into tasks(id) values (?)\n\"\"\"; }";
        let (out, count) = replace_literal_sql_identifier(src, "tasks", "work_items");
        assert_eq!(count, 2, "{out}");
        assert!(out.contains("// tasks stays documentation"), "{out}");
        assert!(out.contains("from work_items where task_status"), "{out}");
        assert!(out.contains("insert into work_items"), "{out}");
    }

    #[test]
    fn owned_projection_components_move_as_java_tokens() {
        let src = "class TaskController { JdbcTaskRepository repository; Task task; String route = \"/tasks\"; }";
        let (out, count) = replace_owned_identifier_component(src, "Task", "WorkItem");
        assert_eq!(count, 3, "{out}");
        assert!(out.contains("class WorkItemController"), "{out}");
        assert!(out.contains("JdbcWorkItemRepository"), "{out}");
        assert!(out.contains("WorkItem task"), "{out}");
        assert!(out.contains("\"/tasks\""), "{out}");
    }
}
