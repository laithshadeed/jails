//! The marked block: how jails edits a file it did not write.
//!
//! `compose.yaml`, `application.properties`, `spring.factories` and
//! `jails.toml` are all files the reader owns, and jails' rule for every one of
//! them is the same — **an edit must be surgical and leave every other byte
//! alone**. What makes that reversible is a pair of comment markers:
//!
//! ```text
//! # jails:db
//! spring.datasource.url=...
//! # /jails:db
//! ```
//!
//! `remove db` then deletes exactly what `add db` wrote, `add kafka` and
//! `add db` stack without either knowing about the other, and
//! `unowned_properties` can diff the block against what jails would write and
//! *name the lines it did not*, before deleting them. A real project had about
//! twenty hand-tuned Kafka properties inside jails' own markers.
//!
//! ## Why this is a module
//!
//! That format had **five owners** — `compose.rs`, `add.rs`,
//! `add/database.rs`, `add/test_wiring.rs` and `doctor.rs` each built and
//! parsed it with their own `format!`. Same shape as `process.rs` before it was
//! extracted, and with the same consequence waiting: a change to the format,
//! or to the rule about the trailing newline, has to be made in five places and
//! will be made in four. `plan.md` §11 asks for exactly this collection, and
//! calls it a prerequisite for §6.2 option F — a data-only kind cannot declare
//! "and this property block" until the block is a value rather than a string
//! literal in whoever writes it.
//!
//! ## Not a general codemod
//!
//! No AST, no rewriting rules. This owns one format and the four things you can
//! do to it: ask whether it is there, put one in, take one out, and read what
//! is inside. Everything structural about *what goes in* the block stays with
//! the capability that knows.

/// One marked region in a file the reader owns.
///
/// `indent` exists because `compose.yaml` nests services two spaces in, and a
/// marker at column zero inside a mapping is a YAML parse error rather than a
/// comment in the wrong place.
#[derive(Debug, Clone, Copy)]
pub struct Marked<'a> {
    pub marker: &'a str,
    pub indent: &'a str,
}

impl<'a> Marked<'a> {
    /// A block at column zero — properties, `spring.factories`.
    pub fn new(marker: &'a str) -> Self {
        Self { marker, indent: "" }
    }

    /// A block nested inside a YAML mapping.
    pub fn indented(marker: &'a str, indent: &'a str) -> Self {
        Self { marker, indent }
    }

    pub fn open(&self) -> String {
        format!("{}# jails:{}", self.indent, self.marker)
    }

    pub fn close(&self) -> String {
        format!("{}# /jails:{}", self.indent, self.marker)
    }

    /// Whether this file already carries the block.
    ///
    /// Matched on the opening marker alone: a file with an opener and no closer
    /// is damaged, and reporting it as absent would have the next `add` write a
    /// second copy rather than say something is wrong.
    pub fn present_in(&self, text: &str) -> bool {
        exact_line(text, &self.open(), 0).is_some()
    }

    /// The block, rendered around `body`, ending in a newline.
    ///
    /// Every line of `body` is indented, not just the markers: in YAML the
    /// markers and the content are at the same level, and in a properties file
    /// the indent is empty so this costs nothing.
    pub fn render(&self, body: &str) -> String {
        let mut out = String::with_capacity(body.len() + 64);
        out.push_str(&self.open());
        out.push('\n');
        for line in body.lines() {
            if line.trim().is_empty() {
                out.push('\n');
            } else {
                out.push_str(self.indent);
                out.push_str(line);
                out.push('\n');
            }
        }
        out.push_str(&self.close());
        out.push('\n');
        out
    }

    /// What is inside the block, or `None` when it is not there.
    ///
    /// The indent is stripped, so a caller comparing against what it would have
    /// written is comparing like with like.
    pub fn body_in(&self, text: &str) -> Option<String> {
        let (start, end) = self.bounds(text)?;
        let inner = &text[start..end];
        let mut lines = inner.lines();
        lines.next()?; // the opening marker
        let mut body = String::new();
        for line in lines {
            if line.trim() == self.close().trim() {
                break;
            }
            body.push_str(line.strip_prefix(self.indent).unwrap_or(line));
            body.push('\n');
        }
        Some(body)
    }

    /// The file without this block, or `None` when there was none to remove.
    ///
    /// Takes the trailing newline with it, so removing a block does not leave a
    /// blank line where it was -- which would accumulate one per add/remove
    /// cycle in a file people read.
    pub fn strip_from(&self, text: &str) -> Option<String> {
        let (start, end) = self.bounds(text)?;
        let mut out = String::with_capacity(text.len());
        out.push_str(&text[..start]);
        out.push_str(&text[end..]);
        Some(out)
    }

    /// Byte range covering the whole block including its trailing newline.
    fn bounds(&self, text: &str) -> Option<(usize, usize)> {
        let open = self.open();
        let close = self.close();
        let (start, after_open) = exact_line(text, &open, 0)?;
        let (_, end) = exact_line(text, &close, after_open)?;
        Some((start, end))
    }
}

/// Byte bounds for one whole line whose contents equal `wanted` exactly.
///
/// Marker names are user-derived in a few generators. Substring matching made
/// `durable-job-email` mistake `durable-job-email-sender` for its own block and
/// then cut the longer marker in half during destroy. Line equality keeps
/// prefix-related blocks independent while retaining the trailing newline.
fn exact_line(text: &str, wanted: &str, from: usize) -> Option<(usize, usize)> {
    let mut start = from;
    for line in text.get(from..)?.split_inclusive('\n') {
        let contents = line.strip_suffix('\n').unwrap_or(line);
        let contents = contents.strip_suffix('\r').unwrap_or(contents);
        let end = start + line.len();
        if contents == wanted {
            return Some((start, end));
        }
        start = end;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_block_round_trips_through_render_and_read_back() {
        let block = Marked::new("db");
        let rendered = block.render("a=1\nb=2\n");
        assert_eq!(rendered, "# jails:db\na=1\nb=2\n# /jails:db\n");

        let file = format!("before=yes\n{rendered}after=yes\n");
        assert!(block.present_in(&file));
        assert_eq!(block.body_in(&file).as_deref(), Some("a=1\nb=2\n"));
    }

    /// The reason `indent` exists: a marker at column zero inside a YAML
    /// mapping is a parse error, not a misplaced comment.
    #[test]
    fn an_indented_block_indents_its_body_as_well_as_its_markers() {
        let block = Marked::indented("db", "  ");
        assert_eq!(
            block.render("postgres:\n  image: postgres:17\n"),
            "  # jails:db\n  postgres:\n    image: postgres:17\n  # /jails:db\n"
        );
        // And reading it back gives what was put in, not the indented form.
        let file = format!("services:\n{}", block.render("postgres:\n"));
        assert_eq!(block.body_in(&file).as_deref(), Some("postgres:\n"));
    }

    /// One `add`/`remove` cycle must leave the file as it was found, or a
    /// blank line accumulates per cycle in a file people read.
    #[test]
    fn stripping_a_block_leaves_the_rest_byte_for_byte() {
        let before = "untouched=yes\nalso.untouched=yes\n";
        let block = Marked::new("db");
        let with = format!(
            "untouched=yes\n{}also.untouched=yes\n",
            block.render("a=1\n")
        );

        assert_eq!(block.strip_from(&with).as_deref(), Some(before));
    }

    #[test]
    fn prefix_related_markers_are_distinct_blocks() {
        let short = Marked::new("durable-job-email");
        let long = Marked::new("durable-job-email-sender");
        let only_long = long.render("jobs.email-sender.initial-delay=PT1H\n");

        assert!(!short.present_in(&only_long));
        assert!(short.body_in(&only_long).is_none());
        assert!(short.strip_from(&only_long).is_none());
        assert_eq!(long.strip_from(&only_long).as_deref(), Some(""));
    }

    #[test]
    fn two_blocks_stack_and_removing_one_leaves_the_other() {
        let db = Marked::new("db");
        let kafka = Marked::new("kafka");
        let file = format!("{}{}", db.render("a=1\n"), kafka.render("b=2\n"));

        let without_db = db.strip_from(&file).unwrap();
        assert!(!db.present_in(&without_db));
        assert!(kafka.present_in(&without_db));
        assert_eq!(kafka.body_in(&without_db).as_deref(), Some("b=2\n"));
    }

    /// Absent is `None`, not an empty edit: a caller that cannot tell "removed"
    /// from "there was nothing" writes the file back for no reason.
    #[test]
    fn a_block_that_is_not_there_reports_nothing_rather_than_a_no_op_edit() {
        let block = Marked::new("redis");
        assert!(!block.present_in("a=1\n"));
        assert_eq!(block.strip_from("a=1\n"), None);
        assert_eq!(block.body_in("a=1\n"), None);
    }
}
