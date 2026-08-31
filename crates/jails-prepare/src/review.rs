//! Runtime-only review material for a prepared transition.
//!
//! The durable [`crate::prepare::PreparedChange`] deliberately
//! carries content addresses rather than preimage bytes. A terminal diff needs
//! the bytes too, but they must not change the operation identity, journal, or
//! receipt. This module is that boundary: preparation derives one review from
//! the already-captured snapshot and the already-prepared postimages, and the
//! CLI may choose to render it.

use crate::Result;
use crate::prepare::{FileOp, PreparedChange};
use jails_protocol::edit::SemanticEdit;
use jails_protocol::identity::ProjectPath;
use jails_protocol::snapshot::{Captured, ProjectSnapshot};
use std::collections::BTreeSet;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ReviewSelection {
    pub diff: bool,
    pub ast: bool,
}

impl ReviewSelection {
    pub fn any(self) -> bool {
        self.diff || self.ast
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReviewFileKind {
    Create,
    Replace,
    Delete,
}

impl ReviewFileKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Replace => "replace",
            Self::Delete => "delete",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Reconciliation {
    Direct,
    ThreeWay,
}

impl Reconciliation {
    pub fn label(self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::ThreeWay => "three-way",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileReview {
    pub path: ProjectPath,
    pub kind: ReviewFileKind,
    pub reconciliation: Reconciliation,
    pub before: Option<Arc<[u8]>>,
    pub after: Option<Arc<[u8]>>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PreparedReview {
    pub files: Vec<FileReview>,
    pub edits: Vec<SemanticEdit>,
}

#[derive(Default)]
pub(crate) struct ReviewSeed {
    pub merged: BTreeSet<ProjectPath>,
    pub edits: Vec<SemanticEdit>,
}

impl PreparedReview {
    pub(crate) fn of(
        base: &ProjectSnapshot,
        change: &PreparedChange,
        seed: ReviewSeed,
    ) -> Result<Self> {
        let mut files = Vec::new();
        for operation in &change.operations {
            let (path, kind, before, after) = match operation {
                FileOp::Create { path, after, .. } => (
                    path,
                    ReviewFileKind::Create,
                    None,
                    Some(object(change, path, after.id)?),
                ),
                FileOp::Replace { path, after, .. } => (
                    path,
                    ReviewFileKind::Replace,
                    current(base, path)?,
                    Some(object(change, path, after.id)?),
                ),
                FileOp::Delete { path, .. } => {
                    (path, ReviewFileKind::Delete, current(base, path)?, None)
                }
            };
            files.push(FileReview {
                path: path.clone(),
                kind,
                reconciliation: if seed.merged.contains(path) {
                    Reconciliation::ThreeWay
                } else {
                    Reconciliation::Direct
                },
                before,
                after,
            });
        }
        Ok(Self {
            files,
            edits: seed.edits,
        })
    }

    /// Rebuild byte-review material for a decoded portable plan.
    ///
    /// The prepared transaction already authenticates every expected
    /// preimage and carried postimage. Reading the current preimage here is
    /// presentation-only; the executor independently rechecks it under the
    /// project lock before writing.
    pub fn for_portable_plan(
        root: impl AsRef<std::path::Path>,
        change: &PreparedChange,
    ) -> Result<Self> {
        let root = root.as_ref();
        let mut files = Vec::new();
        for operation in &change.operations {
            let (path, kind, before, after) = match operation {
                FileOp::Create { path, after, .. } => (
                    path,
                    ReviewFileKind::Create,
                    None,
                    Some(object(change, path, after.id)?),
                ),
                FileOp::Replace { path, after, .. } => (
                    path,
                    ReviewFileKind::Replace,
                    Some(Arc::from(std::fs::read(root.join(path.as_str())).map_err(
                        |error| format!("failed to read `{path}` for plan review: {error}"),
                    )?)),
                    Some(object(change, path, after.id)?),
                ),
                FileOp::Delete { path, .. } => (
                    path,
                    ReviewFileKind::Delete,
                    Some(Arc::from(std::fs::read(root.join(path.as_str())).map_err(
                        |error| format!("failed to read `{path}` for plan review: {error}"),
                    )?)),
                    None,
                ),
            };
            files.push(FileReview {
                path: path.clone(),
                kind,
                reconciliation: Reconciliation::Direct,
                before,
                after,
            });
        }
        Ok(Self {
            files,
            edits: Vec::new(),
        })
    }
}

fn current(base: &ProjectSnapshot, path: &ProjectPath) -> Result<Option<Arc<[u8]>>> {
    Ok(match base.read(path)? {
        Captured::Present(file) => Some(file.bytes.clone()),
        Captured::Absent => None,
    })
}

fn object(
    change: &PreparedChange,
    path: &ProjectPath,
    id: jails_protocol::identity::ObjectId,
) -> Result<Arc<[u8]>> {
    change.objects.get(&id).cloned().ok_or_else(|| {
        format!(
            "prepared postimage `{id}` for `{path}` is absent from its object bundle.\n       \
             fix: report this as a jails preparation bug."
        )
        .into()
    })
}

/// Render the optional review sections after the canonical human envelope.
pub fn render_human(review: &PreparedReview, selection: ReviewSelection) -> String {
    let mut out = String::new();
    if selection.diff {
        for file in &review.files {
            out.push('\n');
            out.push_str(&render_patch(file));
        }
    }
    if selection.ast {
        out.push_str("\nsemantic edits:\n");
        for file in &review.files {
            let kind = match (file.kind, file.reconciliation) {
                (ReviewFileKind::Create, _) => "CreateFile",
                (ReviewFileKind::Replace, Reconciliation::Direct) => "ReplaceFile",
                (ReviewFileKind::Replace, Reconciliation::ThreeWay) => "MergeFile",
                (ReviewFileKind::Delete, _) => "DeleteFile",
            };
            out.push_str(&format!("  {kind} {{ path: {} }}\n", file.path));
        }
        for edit in &review.edits {
            out.push_str(&format!("  {}\n", semantic_kind(edit)));
        }
    }
    out
}

pub(crate) fn render_json_fields(
    review: &PreparedReview,
    selection: ReviewSelection,
) -> Vec<(&'static str, String)> {
    let mut fields = Vec::new();
    if selection.diff {
        let rows = review
            .files
            .iter()
            .map(|file| {
                format!(
                    "    {{\"kind\": {}, \"path\": {}, \"reconciliation\": {}, \"patch\": {}}}",
                    jails_support::json::string(file.kind.label()),
                    jails_support::json::string(&file.path.to_string()),
                    jails_support::json::string(file.reconciliation.label()),
                    jails_support::json::string(&render_patch(file)),
                )
            })
            .collect::<Vec<_>>()
            .join(",\n");
        fields.push(("diffs", json_array(&rows)));
    }
    if selection.ast {
        let file_rows = review.files.iter().map(|file| {
            let kind = match (file.kind, file.reconciliation) {
                (ReviewFileKind::Create, _) => "CreateFile",
                (ReviewFileKind::Replace, Reconciliation::Direct) => "ReplaceFile",
                (ReviewFileKind::Replace, Reconciliation::ThreeWay) => "MergeFile",
                (ReviewFileKind::Delete, _) => "DeleteFile",
            };
            format!(
                "    {{\"kind\": {}, \"path\": {}}}",
                jails_support::json::string(kind),
                jails_support::json::string(&file.path.to_string()),
            )
        });
        let edit_rows = review.edits.iter().map(|edit| {
            format!(
                "    {{\"kind\": {}}}",
                jails_support::json::string(semantic_kind(edit))
            )
        });
        let rows = file_rows.chain(edit_rows).collect::<Vec<_>>().join(",\n");
        fields.push(("ast", json_array(&rows)));
    }
    fields
}

fn json_array(rows: &str) -> String {
    if rows.is_empty() {
        "[]".to_string()
    } else {
        format!("[\n{rows}\n  ]")
    }
}

pub(crate) fn semantic_kind(edit: &SemanticEdit) -> &'static str {
    match edit {
        SemanticEdit::MavenDependency { .. } => "MavenDependency",
        SemanticEdit::BuildPlugin { .. } => "BuildPlugin",
        SemanticEdit::ComposeService { .. } => "ComposeService",
        SemanticEdit::Property { .. } => "Property",
        SemanticEdit::MarkedBlock { .. } => "MarkedBlock",
        SemanticEdit::CommandRegistration { .. } => "CommandRegistration",
        SemanticEdit::HumanConfigCapability { .. } => "HumanConfigCapability",
        SemanticEdit::SpringTestImport { .. } => "SpringTestImport",
        SemanticEdit::HumanConfigLayout { .. } => "HumanConfigLayout",
        SemanticEdit::Retire { .. } => "Retire",
        SemanticEdit::MavenMainClass { .. } => "MavenMainClass",
    }
}

pub(crate) fn render_patch(file: &FileReview) -> String {
    let old_name = match file.kind {
        ReviewFileKind::Create => "/dev/null".to_string(),
        _ => format!("a/{}", file.path),
    };
    let new_name = match file.kind {
        ReviewFileKind::Delete => "/dev/null".to_string(),
        _ => format!("b/{}", file.path),
    };
    let mut out = format!(
        "diff --jails {} {}\n--- {old_name}\n+++ {new_name}\n",
        file.kind.label(),
        file.path
    );
    let Some(before) = text_lines(file.before.as_deref()) else {
        out.push_str("Binary or oversized preimage differs\n");
        return out;
    };
    let Some(after) = text_lines(file.after.as_deref()) else {
        out.push_str("Binary or oversized postimage differs\n");
        return out;
    };
    let cells = before.len().saturating_mul(after.len());
    if cells > 2_000_000 {
        out.push_str(&format!(
            "@@ summary @@\n-{} lines\n+{} lines\n",
            before.len(),
            after.len()
        ));
        return out;
    }
    out.push_str(&format!(
        "@@ -{},{} +{},{} @@{}\n",
        if before.is_empty() { 0 } else { 1 },
        before.len(),
        if after.is_empty() { 0 } else { 1 },
        after.len(),
        if file.reconciliation == Reconciliation::ThreeWay {
            " three-way"
        } else {
            ""
        }
    ));
    for line in line_diff(&before, &after) {
        out.push(line.prefix);
        out.push_str(line.text);
        if !line.text.ends_with('\n') {
            out.push_str("\n\\ No newline at end of file\n");
        }
    }
    out
}

struct DiffLine<'a> {
    prefix: char,
    text: &'a str,
}

fn line_diff<'a>(before: &'a [String], after: &'a [String]) -> Vec<DiffLine<'a>> {
    let width = after.len() + 1;
    let mut lcs = vec![0usize; (before.len() + 1) * width];
    for old in (0..before.len()).rev() {
        for new in (0..after.len()).rev() {
            lcs[old * width + new] = if before[old] == after[new] {
                1 + lcs[(old + 1) * width + new + 1]
            } else {
                lcs[(old + 1) * width + new].max(lcs[old * width + new + 1])
            };
        }
    }
    let (mut old, mut new) = (0, 0);
    let mut lines = Vec::new();
    while old < before.len() || new < after.len() {
        if old < before.len() && new < after.len() && before[old] == after[new] {
            lines.push(DiffLine {
                prefix: ' ',
                text: &before[old],
            });
            old += 1;
            new += 1;
        } else if new < after.len()
            && (old == before.len() || lcs[old * width + new + 1] > lcs[(old + 1) * width + new])
        {
            lines.push(DiffLine {
                prefix: '+',
                text: &after[new],
            });
            new += 1;
        } else {
            lines.push(DiffLine {
                prefix: '-',
                text: &before[old],
            });
            old += 1;
        }
    }
    lines
}

fn text_lines(bytes: Option<&[u8]>) -> Option<Vec<String>> {
    let bytes = bytes.unwrap_or_default();
    if bytes.len() > 2 * 1024 * 1024 || bytes.contains(&0) {
        return None;
    }
    let text = std::str::from_utf8(bytes).ok()?;
    Some(text.split_inclusive('\n').map(redact).collect())
}

fn redact(line: &str) -> String {
    let delimiter = line.find('=').or_else(|| line.find(':'));
    let Some(delimiter) = delimiter else {
        return line.to_string();
    };
    let key = line[..delimiter].to_ascii_lowercase();
    let sensitive = [
        "password",
        "passwd",
        "secret",
        "credential",
        "private_key",
        "private-key",
        "api_key",
        "api-key",
        "access_token",
        "access-token",
        "datasource.url",
        "database_url",
    ]
    .iter()
    .any(|needle| key.contains(needle));
    if !sensitive {
        return line.to_string();
    }
    let newline = if line.ends_with('\n') { "\n" } else { "" };
    format!("{}<redacted>{newline}", &line[..=delimiter])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path() -> ProjectPath {
        ProjectPath::parse("src/main/resources/application.properties").unwrap()
    }

    #[test]
    fn unified_diff_shows_both_sides_and_redacts_secrets() {
        let file = FileReview {
            path: path(),
            kind: ReviewFileKind::Replace,
            reconciliation: Reconciliation::ThreeWay,
            before: Some(Arc::from(
                b"name=demo\npassword=hunter2\nreader=true\n".as_slice(),
            )),
            after: Some(Arc::from(
                b"name=demo\npassword=better\nreader=true\ngenerated=true\n".as_slice(),
            )),
        };
        let patch = render_patch(&file);
        assert!(patch.contains("@@ -1,3 +1,4 @@ three-way"), "{patch}");
        assert!(patch.contains(" password=<redacted>"), "{patch}");
        assert!(patch.contains("+generated=true"), "{patch}");
        assert!(!patch.contains("hunter2"), "{patch}");
        assert!(!patch.contains("better"), "{patch}");
    }

    #[test]
    fn json_fields_are_absent_unless_requested() {
        let review = PreparedReview::default();
        assert!(render_json_fields(&review, ReviewSelection::default()).is_empty());
        let fields = render_json_fields(
            &review,
            ReviewSelection {
                diff: true,
                ast: true,
            },
        );
        assert_eq!(fields, vec![("diffs", "[]".into()), ("ast", "[]".into())]);
    }
}
