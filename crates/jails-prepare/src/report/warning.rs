//! **What the reader should know that is not a failure**, derived from the
//! bytes a change is about to write.
//!
//! Split out of `report.rs` under `abstract.md` rung 11. The parent's secret
//! is *how one prepared change is described*; this one's is *what is worth
//! saying about its contents* -- a different question, and the only part of
//! the report that reads the file bodies at all.
//!
//! Every rule here works on the emitted bytes rather than on the generator
//! that produced them, which is the property that makes it worth having: a
//! kind added tomorrow is covered without knowing this module exists.

use crate::prepare::{FileOp, PreparedChange};
use jails_protocol::identity::ProjectPath;

/// Something the reader should know that is not a failure.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum WarningCode {
    UnmanagedRetained,
    PostCommitDeferred,
    EnvironmentConstrained,
    /// A generated test is `@Disabled`, so it proves nothing and the suite
    /// still reports green over it.
    ///
    /// modern.md §13.8. A generator that cannot write a meaningful assertion
    /// -- a strategy implementation is `return Optional.empty()` with a TODO,
    /// and asserting an accessor returns what was passed in only tests that
    /// javac generated the accessor -- writes an honest `@Disabled` naming
    /// what to prove. That is the right file to write and the wrong thing to
    /// say nothing about: one real project shipped five of its nine tests
    /// disabled, including both controller tests, and reported green. jails'
    /// own `CLAUDE.md` already names this failure mode for skipped tier-3
    /// tests; a generated `@Disabled` is the same thing one level down.
    TestDisabled,
    /// A column reads like a closed set and is stored as free text.
    ///
    /// modern.md §11.3. `direction:String!` produced an unconstrained column,
    /// an unconstrained record and a `"sample"` fixture -- while jails already
    /// had `g enum`, and its own example manifest models that very field as
    /// one. Nothing pointed at it.
    ///
    /// **A warning and never a refusal.** jails cannot know that a `String`
    /// has a closed set; only the reader can. What it can do is notice the
    /// shape and name the command, which is the difference between a tool
    /// with an opinion and a tool that guesses.
    FreeTextClosedSet,
}

impl WarningCode {
    pub fn label(self) -> &'static str {
        match self {
            Self::UnmanagedRetained => "unmanaged-retained",
            Self::PostCommitDeferred => "post-commit-deferred",
            Self::EnvironmentConstrained => "environment-constrained",
            Self::TestDisabled => "test-disabled",
            Self::FreeTextClosedSet => "free-text-closed-set",
        }
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Warning {
    pub code: WarningCode,
    pub paths: Vec<ProjectPath>,
    pub message: String,
}

/// Everything the reader should know about a prepared change, in one place.
///
/// Both the preview and the receipt read this, so a warning cannot appear on
/// `--pretend` and vanish on the real run. plan.md P5.3.
pub fn warnings(change: &PreparedChange) -> Vec<Warning> {
    let mut found = disabled_tests(change);
    found.extend(free_text_closed_sets(change));
    found.sort();
    found
}

fn disabled_tests(change: &PreparedChange) -> Vec<Warning> {
    let mut paths: Vec<ProjectPath> = Vec::new();
    for operation in &change.operations {
        let (path, after) = match operation {
            FileOp::Create { path, after, .. } | FileOp::Replace { path, after, .. } => {
                (path, after)
            }
            _ => continue,
        };
        if !path.as_str().ends_with(".java") {
            continue;
        }
        let Some(bytes) = change.objects.get(&after.id) else {
            continue;
        };
        if bytes.windows(9).any(|window| window == b"@Disabled") {
            paths.push(path.clone());
        }
    }
    if paths.is_empty() {
        return Vec::new();
    }
    paths.sort();
    let message = format!(
        "{} generated test file(s) are @Disabled and prove nothing yet -- the suite \
         reports green over them. Each names what to assert once the class it covers is \
         written.",
        paths.len()
    );
    vec![Warning {
        code: WarningCode::TestDisabled,
        paths,
        message,
    }]
}

/// Column names that almost always denote a closed set.
///
/// **Deliberately short, and matched on the whole trailing word.** A longer
/// list would warn about ordinary text; matching a substring would warn about
/// `statuses_note`. Each of these named a free-text column in a real
/// generated project that the example manifest models as an enum.
const CLOSED_SET_NAMES: &[&str] = &[
    "status",
    "state",
    "kind",
    "direction",
    "mode",
    "stage",
    "phase",
    "category",
    "priority",
    "severity",
    "visibility",
];

/// Free-text columns that read like a closed set.
///
/// Read off the *bytes of the generated migration*, for the same reason
/// [`disabled_tests`] reads the bytes of the generated test: it is the one
/// projection every command goes through, so a generator added tomorrow gets
/// this without knowing it exists.
fn free_text_closed_sets(change: &PreparedChange) -> Vec<Warning> {
    let mut found: Vec<(ProjectPath, String)> = Vec::new();
    for operation in &change.operations {
        let (path, after) = match operation {
            FileOp::Create { path, after, .. } | FileOp::Replace { path, after, .. } => {
                (path, after)
            }
            _ => continue,
        };
        if !path.as_str().ends_with(".sql") {
            continue;
        }
        let Some(bytes) = change.objects.get(&after.id) else {
            continue;
        };
        let Ok(sql) = std::str::from_utf8(bytes) else {
            continue;
        };
        for column in free_text_columns(sql) {
            // Already closed: `check (col in (…))` is exactly the thing this
            // warning asks for, so having it is the end of the matter.
            if sql.contains(&format!("check ({column} in (")) {
                continue;
            }
            found.push((path.clone(), column));
        }
    }
    if found.is_empty() {
        return Vec::new();
    }
    found.sort();
    found.dedup();
    let columns = found
        .iter()
        .map(|(_, column)| column.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let mut paths: Vec<ProjectPath> = found.into_iter().map(|(path, _)| path).collect();
    paths.dedup();
    vec![Warning {
        code: WarningCode::FreeTextClosedSet,
        paths,
        message: format!(
            "{columns} is stored as free text and reads like a closed set, so the column \
             accepts any string.\n       fix: `jails g enum <Name> VALUE OTHER` and declare the \
             field as `<name>:<Name>` -- the schema then carries the set, and the generated \
             tests get real values instead of \"sample\"."
        ),
    }]
}

/// `  direction  text not null,` -> `direction`.
///
/// Matched on the shape `create table` and `add column` both produce, and only
/// on `text`: a column jails gave a narrower type is already as closed as the
/// type allows.
fn free_text_columns(sql: &str) -> Vec<String> {
    let mut found = Vec::new();
    for line in sql.lines() {
        let trimmed = line.trim_start();
        let Some((name, rest)) = trimmed.split_once(char::is_whitespace) else {
            continue;
        };
        let name = name.trim_start_matches("add column ");
        if !rest.trim_start().starts_with("text") {
            continue;
        }
        let last_word = name.rsplit('_').next().unwrap_or(name);
        if CLOSED_SET_NAMES.contains(&last_word) {
            found.push(name.to_string());
        }
    }
    found
}
