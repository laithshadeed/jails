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
//! `remove db` then deletes exactly what `add db` wrote, and `add kafka` and
//! `add db` stack without either knowing about the other. A caller that wants
//! to know what the reader added inside a block can diff its body against what
//! jails would write and name the lines it did not.
//!
//! ## Why this is its own crate
//!
//! The format has one owner. A change to it, or to the rule about the
//! trailing newline, is made here and nowhere else; the architecture gate
//! fails on a `# jails:` literal outside this crate.
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
    /// How this file spells "the rest of the line is a comment".
    ///
    /// `#` for properties, YAML and `spring.factories`; `--` for SQL. It is a
    /// field rather than a constant because a marker in the wrong comment
    /// syntax is not a cosmetic problem: `# jails:table-users` at the top of a
    /// `schema.sql` is a syntax error, so the block jails writes would stop
    /// the application starting.
    pub comment: &'a str,
}

impl Marked<'_> {
    /// What an opening marker begins with, in a `#`-commented file.
    ///
    /// For the callers that ask whether *any* marker is present rather than
    /// looking one up -- a compose mapping that carries one is refused,
    /// because markers belong to the format owner. Spelling the prefix at the
    /// call site is a second place to edit if the format changes and a
    /// validation that silently stops refusing if it is not.
    pub const OPEN_PREFIX: &'static str = "# jails:";

    /// The closing counterpart of [`Self::OPEN_PREFIX`].
    pub const CLOSE_PREFIX: &'static str = "# /jails:";
}

impl<'a> Marked<'a> {
    /// A block at column zero — properties, `spring.factories`.
    pub fn new(marker: &'a str) -> Self {
        Self {
            marker,
            indent: "",
            comment: "#",
        }
    }

    /// A block nested inside a YAML mapping.
    pub fn indented(marker: &'a str, indent: &'a str) -> Self {
        Self {
            comment: "#",
            ..Self::new(marker)
        }
        .with_indent(indent)
    }

    fn with_indent(mut self, indent: &'a str) -> Self {
        self.indent = indent;
        self
    }

    pub fn open(&self) -> String {
        format!("{}{} jails:{}", self.indent, self.comment, self.marker)
    }

    pub fn close(&self) -> String {
        format!("{}{} /jails:{}", self.indent, self.comment, self.marker)
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
}

/// Byte bounds for one whole line whose contents equal `wanted` exactly.
///
/// Marker names are user-derived in a few generators, so a substring match
/// lets `durable-job-email` mistake `durable-job-email-sender` for its own
/// block and cut the longer marker in half on destroy. Line equality keeps
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
mod tests {}

#[cfg(test)]
mod prefix_tests {
    use super::*;

    /// The constants and the renderer must not be able to disagree.
    ///
    /// They are written out rather than derived, because a `const` cannot call
    /// `format!` -- so the only thing keeping them true is this.
    #[test]
    fn the_prefixes_are_what_a_block_is_actually_rendered_with() {
        let marked = Marked::new("example");
        assert_eq!(marked.open(), format!("{}example", Marked::OPEN_PREFIX));
        assert_eq!(marked.close(), format!("{}example", Marked::CLOSE_PREFIX));
    }
}
