//! A unified diff, for showing a reader what a plan changes before it runs.
//!
//! **Not a merge, and deliberately not git's.** `git merge-file` is the
//! three-way merge because reconciling two edits is a problem with real
//! subtlety and a distribution-dependent answer; *displaying* a change is
//! neither. Shelling out per file would make a preview cost one subprocess
//! per changed file and make its output depend on which git the machine has
//! -- the same trap `JAILS_GIT_DIFF_ALGORITHM` exists to pin for merges.
//!
//! The algorithm is the longest common subsequence over lines, which is what
//! `diff -u` shows for the small, mostly-append changes a regenerated file
//! produces. It is quadratic in the number of *differing* lines only: equal
//! prefixes and suffixes are trimmed first, so a thousand-line file with one
//! new import costs a scan.

/// A unified diff of `before` against `after`, or `None` when they agree.
///
/// `context` is the number of unchanged lines shown around each hunk; three
/// is what `diff -u` defaults to. A missing side is rendered as `/dev/null`,
/// so a created file reads the way a reader expects.
pub fn diff(
    path: &str,
    before: Option<&str>,
    after: Option<&str>,
    context: usize,
) -> Option<String> {
    if before == after {
        return None;
    }
    let left = before.map(lines).unwrap_or_default();
    let right = after.map(lines).unwrap_or_default();
    let hunks = hunks(&left, &right, context);
    if hunks.is_empty() {
        return None;
    }
    let mut out = String::new();
    out.push_str(&match before {
        Some(_) => format!("--- a/{path}\n"),
        None => "--- /dev/null\n".to_string(),
    });
    out.push_str(&match after {
        Some(_) => format!("+++ b/{path}\n"),
        None => "+++ /dev/null\n".to_string(),
    });
    for hunk in hunks {
        out.push_str(&hunk);
    }
    Some(out)
}

fn lines(text: &str) -> Vec<&str> {
    match text.is_empty() {
        true => Vec::new(),
        false => text.split('\n').collect(),
    }
}

/// One `@@` block per run of changes, with `context` unchanged lines around.
fn hunks(left: &[&str], right: &[&str], context: usize) -> Vec<String> {
    let edits = edits(left, right);
    let mut hunks = Vec::new();
    let mut index = 0;
    while index < edits.len() {
        if matches!(edits[index], Edit::Equal(..)) {
            index += 1;
            continue;
        }
        // The window: `context` equal lines before the first change, and the
        // run continues while no more than `2 * context` equal lines separate
        // one change from the next -- otherwise two hunks read better than one
        // carrying a page of unchanged text between them.
        let start = index.saturating_sub(context);
        let mut end = index;
        let mut run = 0;
        for (offset, edit) in edits.iter().enumerate().skip(index) {
            match edit {
                Edit::Equal(..) => {
                    run += 1;
                    if run > context * 2 {
                        break;
                    }
                }
                _ => {
                    run = 0;
                    end = offset;
                }
            }
        }
        let end = (end + context + 1).min(edits.len());
        let (mut old_start, mut new_start) = (0, 0);
        for edit in &edits[..start] {
            match edit {
                Edit::Equal(..) => {
                    old_start += 1;
                    new_start += 1;
                }
                Edit::Remove(..) => old_start += 1,
                Edit::Insert(..) => new_start += 1,
            }
        }
        let (mut old_len, mut new_len) = (0, 0);
        let mut body = String::new();
        for edit in &edits[start..end] {
            match edit {
                Edit::Equal(line) => {
                    old_len += 1;
                    new_len += 1;
                    body.push_str(&format!(" {line}\n"));
                }
                Edit::Remove(line) => {
                    old_len += 1;
                    body.push_str(&format!("-{line}\n"));
                }
                Edit::Insert(line) => {
                    new_len += 1;
                    body.push_str(&format!("+{line}\n"));
                }
            }
        }
        hunks.push(format!(
            "@@ -{},{old_len} +{},{new_len} @@\n{body}",
            old_start + 1,
            new_start + 1
        ));
        index = end;
    }
    hunks
}

enum Edit<'a> {
    Equal(&'a str),
    Remove(&'a str),
    Insert(&'a str),
}

/// The edit script, with equal prefixes and suffixes trimmed before the
/// quadratic part runs.
fn edits<'a>(left: &[&'a str], right: &[&'a str]) -> Vec<Edit<'a>> {
    let head = left
        .iter()
        .zip(right)
        .take_while(|(a, b)| a == b)
        .count()
        .min(left.len().min(right.len()));
    let tail = left[head..]
        .iter()
        .rev()
        .zip(right[head..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let mut script = left[..head]
        .iter()
        .map(|line| Edit::Equal(line))
        .collect::<Vec<_>>();
    script.extend(middle(
        &left[head..left.len() - tail],
        &right[head..right.len() - tail],
    ));
    script.extend(
        left[left.len() - tail..]
            .iter()
            .map(|line| Edit::Equal(line)),
    );
    script
}

/// The most table `middle` may allocate, in cells.
///
/// **Every guard around this function is on bytes and its cost is quadratic in
/// lines.** Stripping the common head and tail first makes the usual case
/// cheap and does nothing for the case that matters: two files that genuinely
/// differ throughout. Thirty thousand differing lines a side is 900 million
/// cells, and at eight bytes -- plus a `Vec` header per row -- that is over
/// **seven gigabytes** in one command.
///
/// Measured rather than reasoned about: a single
/// `jails resource field add ... --diff` was **6.8 GB resident** and by itself
/// the entire memory profile of the test suite, where every other module
/// peaked under 106 MB. It is what an OOM killer took a developer's 30 GB
/// desktop down over, and the kill left two containers running for four hours
/// after it.
///
/// Four million cells is ~32 MB of table, which covers every file jails
/// generates and every reader file it merges into.
const MAX_TABLE_CELLS: usize = 4_000_000;

/// Both sides in full, when they are too far apart to look for common lines.
///
/// What a diff degrades to anyway once the table is mostly misses, and honest
/// about having stopped looking rather than truncating one side.
fn replaced_wholesale<'a>(left: &[&'a str], right: &[&'a str]) -> Vec<Edit<'a>> {
    left.iter()
        .map(|line| Edit::Remove(line))
        .chain(right.iter().map(|line| Edit::Insert(line)))
        .collect()
}

/// Longest common subsequence over the differing middle.
fn middle<'a>(left: &[&'a str], right: &[&'a str]) -> Vec<Edit<'a>> {
    if left.len().saturating_mul(right.len()) > MAX_TABLE_CELLS {
        return replaced_wholesale(left, right);
    }
    let mut table = vec![vec![0usize; right.len() + 1]; left.len() + 1];
    for (row, l) in left.iter().enumerate().rev() {
        for (column, r) in right.iter().enumerate().rev() {
            table[row][column] = match l == r {
                true => table[row + 1][column + 1] + 1,
                false => table[row + 1][column].max(table[row][column + 1]),
            };
        }
    }
    let (mut row, mut column) = (0, 0);
    let mut script = Vec::new();
    while row < left.len() && column < right.len() {
        if left[row] == right[column] {
            script.push(Edit::Equal(left[row]));
            row += 1;
            column += 1;
        } else if table[row + 1][column] >= table[row][column + 1] {
            script.push(Edit::Remove(left[row]));
            row += 1;
        } else {
            script.push(Edit::Insert(right[column]));
            column += 1;
        }
    }
    script.extend(left[row..].iter().map(|line| Edit::Remove(line)));
    script.extend(right[column..].iter().map(|line| Edit::Insert(line)));
    script
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bound that keeps a diff from taking the machine down with it.
    ///
    /// Thirty thousand differing lines a side sits inside every byte-based
    /// guard upstream and was over 7 GB of table, which is how one
    /// `resource field add --diff` came to be the whole test suite's memory
    /// profile. The assertion is on the shape rather than on resident bytes,
    /// because a test that measures its own RSS measures the allocator:
    /// above the budget every line of both sides is reported and none is lost.
    #[test]
    fn a_middle_too_large_to_align_reports_both_sides_whole_instead_of_allocating() {
        let left: Vec<String> = (0..30_000).map(|n| format!("left {n}")).collect();
        let right: Vec<String> = (0..30_000).map(|n| format!("right {n}")).collect();
        let left: Vec<&str> = left.iter().map(String::as_str).collect();
        let right: Vec<&str> = right.iter().map(String::as_str).collect();
        assert!(left.len() * right.len() > MAX_TABLE_CELLS);

        let script = middle(&left, &right);

        assert_eq!(script.len(), left.len() + right.len());
        assert_eq!(
            script
                .iter()
                .filter(|e| matches!(e, Edit::Remove(_)))
                .count(),
            left.len()
        );
        assert_eq!(
            script
                .iter()
                .filter(|e| matches!(e, Edit::Insert(_)))
                .count(),
            right.len()
        );
    }

    /// Below the budget the real alignment still runs, so the guard cannot be
    /// "fixed" by making every diff a wholesale replacement.
    #[test]
    fn a_middle_within_the_budget_still_aligns_the_lines_the_two_sides_share() {
        let script = middle(&["keep", "drop"], &["keep", "add"]);

        assert!(
            script.iter().any(|e| matches!(e, Edit::Equal("keep"))),
            "a shared line was not aligned"
        );
    }

    #[test]
    fn identical_text_has_no_diff() {
        assert!(diff("a.java", Some("one\ntwo\n"), Some("one\ntwo\n"), 3).is_none());
    }

    #[test]
    fn a_created_file_reads_from_dev_null() {
        let shown = diff("a.java", None, Some("one\n"), 3).unwrap();
        assert!(
            shown.starts_with("--- /dev/null\n+++ b/a.java\n"),
            "{shown}"
        );
        assert!(shown.contains("+one"), "{shown}");
    }

    #[test]
    fn one_inserted_line_shows_as_one_hunk_with_context() {
        let before = "a\nb\nc\nd\ne\nf\ng\nh\n";
        let after = "a\nb\nc\nd\nNEW\ne\nf\ng\nh\n";
        let shown = diff("x.java", Some(before), Some(after), 3).unwrap();
        assert_eq!(shown.matches("@@ -").count(), 1, "{shown}");
        assert!(shown.contains("+NEW\n"), "{shown}");
        // Context only, so the first and last lines stay out of the hunk.
        assert!(!shown.contains(" a\n"), "{shown}");
    }

    #[test]
    fn distant_changes_are_separate_hunks() {
        let before = (0..40)
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        let mut lines = before.split('\n').map(str::to_string).collect::<Vec<_>>();
        lines[2] = "changed".to_string();
        lines[30] = "also".to_string();
        let after = lines.join("\n");
        let shown = diff("x.java", Some(&before), Some(&after), 3).unwrap();
        assert_eq!(shown.matches("@@ -").count(), 2, "{shown}");
    }
}
