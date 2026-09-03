//! Where in the file a model path came from.
//!
//! A linker diagnostic names the node it is about -- `$.entities.loan.
//! fields.status.type` -- which says exactly what is wrong and nothing about
//! where to go and fix it. The CST already holds the byte span of every
//! declaration and every member, so the missing half is one table from the
//! path the linker writes to the span the parser saw.
//!
//! **Keyed by the path prefix, resolved longest-first.** A diagnostic is
//! raised about a field's `.type`, its `.id`, its `.column` or its
//! `.required`, and no span exists for any of those: the reader's cursor
//! belongs on the line the field is declared on. Stripping trailing segments
//! until a key matches is what turns four paths into the one span that
//! answers all of them, and it needs no table of which suffixes a linker may
//! append -- which is the table that would go stale.

use super::cst::DocumentCst;
use std::collections::BTreeMap;

/// A one-based position in the model file, as an editor spells one.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Location {
    pub(crate) line: u32,
    pub(crate) column: u32,
}

/// Every model path this document declares, with where it was written.
#[derive(Clone, Debug, Default)]
pub struct SpanIndex {
    rows: BTreeMap<String, Location>,
}

impl SpanIndex {
    pub fn from_cst(cst: &DocumentCst) -> Self {
        let source = cst.source();
        let lines = LineTable::of(source);
        let mut rows = BTreeMap::new();
        for declaration in &cst.declarations {
            if let Some(path) = declaration_path(&declaration.kind, declaration.name.as_deref()) {
                rows.insert(
                    path,
                    lines.locate(first_word(source, declaration.span.start)),
                );
            }
        }
        for member in &cst.members {
            if let Some(path) = member_path(&member.owner, &member.kind, member.name.as_deref()) {
                rows.insert(path, lines.locate(first_word(source, member.span.start)));
            }
        }
        Self { rows }
    }

    /// The location of the nearest declared ancestor of this path, if the
    /// document declared one.
    pub fn locate(&self, path: &str) -> Option<Location> {
        let mut candidate = path;
        loop {
            if let Some(location) = self.rows.get(candidate) {
                return Some(*location);
            }
            candidate = &candidate[..candidate.rfind('.')?];
        }
    }
}

/// The declaration's first written byte.
///
/// A member's span opens at the indent, because that is where the reader's
/// line begins and a syntax-preserving edit has to keep it. A *location*
/// points at what is wrong, so it skips the blanks and lands on the word.
fn first_word(source: &str, start: usize) -> usize {
    source[start..]
        .find(|character: char| !matches!(character, ' ' | '\t'))
        .map_or(start, |at| start + at)
}

/// The path the linker writes for one top-level declaration.
///
/// A kind with no entry is one whose diagnostics are raised against a path
/// this table cannot predict, and predicting one wrongly would send a reader
/// to a line that has nothing to do with the message.
fn declaration_path(kind: &str, name: Option<&str>) -> Option<String> {
    let name = name.map(label);
    Some(match (kind, name) {
        ("app", _) => "$.project".to_string(),
        ("cap", Some(name)) => format!("$.capabilities.{name}"),
        ("dep", Some(name)) => format!("$.dependencies.{name}"),
        ("prop", Some(name)) => format!("$.settings.{name}"),
        ("enum", Some(name)) => format!("$.enums.{name}"),
        ("entity", Some(name)) => format!("$.entities.{name}"),
        ("component", Some(name)) => format!("$.components.{name}"),
        ("eject", Some(name)) => format!("$.ejections.{name}"),
        ("command" | "query" | "transition" | "event", Some(name)) => {
            format!("$.operations.{name}")
        }
        _ => return None,
    })
}

fn member_path(owner: &str, kind: &str, name: Option<&str>) -> Option<String> {
    let owner = label(owner);
    let name = name.map(label);
    Some(match (kind, name) {
        ("field", Some(name)) => format!("$.entities.{owner}.fields.{name}"),
        ("relation", Some(name)) => format!("$.entities.{owner}.relations.{name}"),
        ("index", Some(name)) => format!("$.entities.{owner}.indexes.{name}"),
        ("command" | "query" | "transition" | "event", Some(name)) => {
            format!("$.operations.{name}")
        }
        _ => return None,
    })
}

/// The spelling the linker keys a node by: `createdAt` and `created-at` are
/// both `created_at` there, and a table keyed the way the reader typed it
/// would answer for half the fields in a file.
fn label(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for (index, character) in value.chars().enumerate() {
        if character.is_ascii_uppercase() {
            if index > 0 {
                output.push('_');
            }
            output.push(character.to_ascii_lowercase());
        } else if character == '-' {
            output.push('_');
        } else {
            output.push(character);
        }
    }
    output
}

/// Byte offset to line and column, built once per document.
struct LineTable {
    /// The byte offset each line starts at, in order.
    starts: Vec<usize>,
}

impl LineTable {
    fn of(source: &str) -> Self {
        let mut starts = vec![0];
        starts.extend(
            source
                .bytes()
                .enumerate()
                .filter(|(_, byte)| *byte == b'\n')
                .map(|(at, _)| at + 1),
        );
        Self { starts }
    }

    fn locate(&self, offset: usize) -> Location {
        let line = self.starts.partition_point(|start| *start <= offset);
        let start = self.starts.get(line - 1).copied().unwrap_or_default();
        Location {
            line: u32::try_from(line).unwrap_or(u32::MAX),
            column: u32::try_from(offset - start + 1).unwrap_or(u32::MAX),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_table_counts_from_one() {
        let table = LineTable::of("a\nbb\n\nc");
        assert_eq!(table.locate(0), Location { line: 1, column: 1 });
        assert_eq!(table.locate(2), Location { line: 2, column: 1 });
        assert_eq!(table.locate(3), Location { line: 2, column: 2 });
        assert_eq!(table.locate(6), Location { line: 4, column: 1 });
    }

    /// The property the whole index rests on: a diagnostic is raised about a
    /// path the parser never recorded, and it still resolves.
    #[test]
    fn a_path_resolves_through_its_nearest_declared_ancestor() {
        let mut rows = BTreeMap::new();
        rows.insert(
            "$.entities.loan.fields.status".to_string(),
            Location { line: 9, column: 3 },
        );
        rows.insert(
            "$.entities.loan".to_string(),
            Location { line: 7, column: 1 },
        );
        let index = SpanIndex { rows };
        assert_eq!(
            index.locate("$.entities.loan.fields.status.type"),
            Some(Location { line: 9, column: 3 })
        );
        assert_eq!(
            index.locate("$.entities.loan.indexes.by_status.columns[0]"),
            Some(Location { line: 7, column: 1 })
        );
        assert_eq!(index.locate("$.project.name"), None);
    }
}
