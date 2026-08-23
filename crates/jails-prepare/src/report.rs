//! What a prepared transaction looks like to a person.
//!
//! plan.md §R3.4 makes reporting a projection of the prepared value rather
//! than a second description of the work. That is the whole point: today
//! `--pretend` and the real run interpret the same buckets independently, so
//! a dry run can disagree with what happens. Here there is one value, and the
//! report is a pure function of it.
//!
//! Sorted by path, because a report whose line order depends on a hash map is
//! a report two identical runs disagree about.

use crate::prepare::{FileOp, OperationTarget, PreparedChange, PreparedKind};

/// One reported line.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Line {
    pub verb: &'static str,
    pub subject: String,
}

impl std::fmt::Display for Line {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "  {:<7} {}", self.verb, self.subject)
    }
}

/// The verb for one operation, chosen from what it does rather than from what
/// the caller thought it was doing.
fn verb(op: &FileOp) -> &'static str {
    match op {
        FileOp::Create { .. } => "create",
        FileOp::Replace { .. } => "update",
        FileOp::Delete { path, .. } => match path {
            // A legacy delete is a migration, not a removal the user asked
            // for, and saying "delete" about their old state would read as
            // data loss rather than as cleanup.
            OperationTarget::LegacyMachine(_) => "retire",
            OperationTarget::Project(_) => "delete",
        },
    }
}

/// Every line this change would produce, in path order.
pub fn lines(change: &PreparedChange) -> Vec<Line> {
    let mut lines: Vec<Line> = change
        .directories
        .iter()
        .map(|directory| Line {
            verb: "mkdir",
            subject: directory.path().to_string(),
        })
        .chain(change.operations.iter().map(|op| Line {
            verb: verb(op),
            subject: op.target().to_string(),
        }))
        .collect();
    lines.sort_by(|a, b| a.subject.cmp(&b.subject).then(a.verb.cmp(b.verb)));
    lines
}

/// The one-line summary, which has to be able to say "nothing".
///
/// A run that found everything already in place is a real outcome, and
/// printing a confident "applied" over it is how a tool teaches people to
/// stop reading its output.
pub fn summary(change: &PreparedChange) -> String {
    match &change.kind {
        PreparedKind::Conflict { paths } => format!(
            "{} file{} could not be merged automatically",
            paths.len(),
            plural(paths.len())
        ),
        PreparedKind::Finalise { .. } => "finishing the frozen conflict".to_string(),
        PreparedKind::Abort { .. } => "putting the frozen conflict back".to_string(),
        PreparedKind::Apply if change.is_no_op() => "nothing to do".to_string(),
        PreparedKind::Apply => {
            let files = change.operations.len();
            let effects = change.post_commit.len();
            let mut summary = format!("{files} file{}", plural(files));
            if effects > 0 {
                summary.push_str(", then reconciling compose services");
            }
            summary
        }
    }
}

fn plural(count: usize) -> &'static str {
    if count == 1 { "" } else { "s" }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prepare::tests::{change_with, create};

    #[test]
    fn lines_are_ordered_by_path_not_by_construction() {
        let change = change_with(vec![
            create("src/main/java/com/example/demo/Zebra.java", b"z"),
            create("src/main/java/com/example/demo/Apple.java", b"a"),
        ]);
        let rendered: Vec<String> = lines(&change)
            .into_iter()
            .map(|line| line.subject)
            .collect();
        assert_eq!(
            rendered,
            vec![
                "src/main/java/com/example/demo/Apple.java",
                "src/main/java/com/example/demo/Zebra.java"
            ]
        );
    }

    /// Printing a confident "applied" over a run that did nothing is how a
    /// tool teaches people to stop reading its output.
    #[test]
    fn a_change_with_nothing_to_do_says_so() {
        assert_eq!(summary(&change_with(Vec::new())), "nothing to do");
    }

    #[test]
    fn one_file_is_not_reported_as_one_files() {
        let change = change_with(vec![create("pom.xml", b"<project/>")]);
        assert_eq!(summary(&change), "1 file");
    }
}
