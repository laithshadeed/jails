//! `jails rename <Old> <New>` -- rename a type and everything that names it.
//!
//! This is the one refactor with no equivalent anywhere else in the
//! toolchain a terminal Java setup has. Neovim's `grn` (jdt.ls rename) does
//! handle a class rename including the file rename, and where it works it is
//! strictly better than this command: it understands scope, so it will not
//! touch an unrelated `Reward` in another package. Reach for it first.
//!
//! What this exists for is the case jdt.ls cannot serve: the language server
//! is not attached, the project does not currently compile (jdt.ls degrades
//! badly there, and a rename is often exactly how you are trying to fix it),
//! or the rename has to reach a file no buffer has opened. It is textual and
//! says so. Two properties keep textual honest:
//!
//! - **Identifier boundaries, not substrings.** `Reward` never matches inside
//!   `RewardHistory`, so the classic sed disaster cannot happen.
//! - **String literals are left alone.** Code and comments are renamed; a
//!   literal is data, and silently rewriting `"Reward not found"` would be a
//!   change nobody asked for. A literal that genuinely names the class (a
//!   `Class.forName` argument) is therefore missed, which is the safe
//!   direction and is reported rather than hidden.
//!
//! Nothing is written until the whole plan has been shown and confirmed.

use std::io::Write as _;
use std::path::{Path, PathBuf};

use crate::generate::find_project_root;
use jails_support::Result;

/// One file the rename touches.
struct Edit {
    path: PathBuf,
    /// Where the file ends up. Equal to `path` when only its contents change.
    destination: PathBuf,
    occurrences: usize,
    updated: String,
}

pub fn rename(old: &str, new: &str, dry_run: bool, force: bool) -> Result<()> {
    validate(old, new)?;
    let root = find_project_root()?;
    let edits = plan(&root, old, new)?;

    if edits.is_empty() {
        return Err(format!(
            "no .java file under src/ mentions `{old}` -- check the spelling, or the type may live outside this module"
        ));
    }

    let renamed: Vec<&Edit> = edits.iter().filter(|e| e.path != e.destination).collect();
    let touched = edits.len();
    let occurrences: usize = edits.iter().map(|e| e.occurrences).sum();

    println!("rename {old} -> {new}");
    println!();
    for edit in &edits {
        if edit.path == edit.destination {
            println!(
                "  edit    {}  ({} occurrence(s))",
                rel(&root, &edit.path),
                edit.occurrences
            );
        } else {
            println!(
                "  rename  {}  ->  {}  ({} occurrence(s))",
                rel(&root, &edit.path),
                rel(&root, &edit.destination),
                edit.occurrences
            );
        }
    }
    println!();
    println!(
        "{touched} file(s), {occurrences} occurrence(s), {} file rename(s).",
        renamed.len()
    );

    // A literal mentioning the old name is the one thing a textual rename
    // deliberately will not touch, so it has to be said out loud -- an
    // unmentioned exception is indistinguishable from a bug.
    let in_literals = literal_mentions(&edits, old);
    if in_literals > 0 {
        println!(
            "{in_literals} mention(s) inside string literals were left alone. Check them by hand:"
        );
        println!("  jails mvn -- -q  # or: grep -rn '\"[^\"]*{old}' src/");
    }

    if dry_run {
        println!();
        println!("--dry-run: nothing was written.");
        return Ok(());
    }

    if !force {
        print!("proceed? [y/N] ");
        std::io::stdout().flush().ok();
        let mut answer = String::new();
        std::io::stdin()
            .read_line(&mut answer)
            .map_err(|e| format!("failed to read confirmation: {e}"))?;
        if !answer.trim().eq_ignore_ascii_case("y") {
            println!("aborted");
            return Ok(());
        }
    }

    // Contents first, then the file moves. The other order would leave a
    // half-applied rename behind if a write failed partway: renamed files
    // whose contents still declare the old type name do not compile, while
    // rewritten contents in unmoved files at least still describe one
    // consistent state.
    for edit in &edits {
        crate::apply::put(&edit.path, &edit.updated)?;
    }
    for edit in &edits {
        if edit.path == edit.destination {
            continue;
        }
        jails_support::apply::move_file(&edit.path, &edit.destination).map_err(|e| {
            format!(
                "failed to move {} to {}: {e}",
                edit.path.display(),
                edit.destination.display()
            )
        })?;
    }

    println!();
    println!("renamed. Next:");
    println!("  jails check     # format check + clean compile + tests");
    Ok(())
}

fn validate(old: &str, new: &str) -> Result<()> {
    for (label, name) in [("old", old), ("new", new)] {
        if name.is_empty() {
            return Err(format!("the {label} name is empty"));
        }
        if !name
            .chars()
            .next()
            .is_some_and(|c| c.is_alphabetic() || c == '_')
        {
            return Err(format!(
                "`{name}` is not a Java identifier -- the {label} name must start with a letter"
            ));
        }
        if !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return Err(format!(
                "`{name}` is not a Java identifier. `jails rename` renames one type, not a package \
                 path -- pass the simple name (`Reward`, not `com.example.Reward`)"
            ));
        }
    }
    if old == new {
        return Err("the old and new names are the same".into());
    }
    Ok(())
}

fn plan(root: &Path, old: &str, new: &str) -> Result<Vec<Edit>> {
    let mut edits = Vec::new();
    for path in crate::java::source_files(&root.join("src")) {
        let Ok(source) = std::fs::read_to_string(&path) else {
            continue;
        };
        let (updated, occurrences) = replace_identifier(&source, old, new);
        let destination = renamed_path(&path, old, new);
        if occurrences == 0 && destination == path {
            continue;
        }
        if destination != path && destination.exists() {
            return Err(format!(
                "{} already exists -- rename or delete it first",
                rel(root, &destination)
            ));
        }
        edits.push(Edit {
            path,
            destination,
            occurrences,
            updated,
        });
    }
    edits.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(edits)
}

/// Where a file ends up. A Java file is named for the type it declares, and
/// jails' own companions extend that name with a fixed suffix -- so
/// `Reward.java`, `RewardTest.java` and `RewardIT.java` all move together,
/// while `RewardHistoryService.java` (a different type that merely starts
/// with the same letters) does not.
fn renamed_path(path: &Path, old: &str, new: &str) -> PathBuf {
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

/// Replace `old` with `new` wherever it appears as a whole identifier
/// outside a string or character literal. Returns the new text and how many
/// replacements were made.
fn replace_identifier(source: &str, old: &str, new: &str) -> (String, usize) {
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

/// How many times the old name appears inside a string literal, across every
/// file the plan touches -- the rename's known blind spot, counted so it can
/// be reported.
fn literal_mentions(edits: &[Edit], old: &str) -> usize {
    let mut count = 0;
    for edit in edits {
        let Ok(source) = std::fs::read_to_string(&edit.path) else {
            continue;
        };
        let bytes = source.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if let Some(end) = literal_end(&source, i) {
                count += source[i..end].matches(old).count();
                i = end;
                continue;
            }
            i += 1;
        }
    }
    count
}

fn rel(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
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

    #[test]
    fn a_package_qualified_name_is_rejected() {
        let err = validate("com.example.Reward", "Bonus").unwrap_err();
        assert!(err.contains("simple name"), "{err}");
        assert!(validate("Reward", "Reward").is_err());
        assert!(validate("Reward", "Bonus").is_ok());
    }
}
