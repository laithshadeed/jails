//! The concrete syntax tree: declaration boundaries over a lossless token run.
//!
//! `source` plus `tokens` reproduces the input byte for byte, and a
//! `DeclarationCst` is a *span* rather than a copy — so a syntax-preserving
//! edit is a splice into the original string, not a re-render of a parsed
//! structure.
//!
//! `MemberCst` exists because entities are edited from the inside: `resource
//! field add` and `g field` change one member of one entity, and naming that
//! member's span is what keeps the rest of the block — its comments, its
//! ordering, its spacing — exactly as the reader left it.

use super::token::{Span, Token};
use crate::{Diagnostic, Diagnostics};

/// Where a top-level declaration of this kind belongs in the document.
///
/// **One table, two questions.** The formatter asks which *group* a
/// declaration is in, so it can put one blank line between groups and none
/// inside one; an insertion asks the finer order, so a new `cap` lands after
/// the last `cap` and before the `dep`s rather than at the bottom of the
/// file. Written twice, the CLI's idea of where a declaration goes and the
/// formatter's idea of where the blank lines go drift apart, and the file the
/// tool writes stops being the file a reader would have written.
///
/// Neither answer ever *moves* a declaration the reader placed: the formatter
/// only inserts blank lines, and the insertion only chooses a position for a
/// line that is not there yet.
pub(super) fn top_level_order(kind: &str) -> (u8, u8) {
    match kind {
        "app" => (0, 0),
        "cap" => (1, 1),
        "dep" => (1, 2),
        "prop" => (1, 3),
        "enum" => (2, 4),
        "entity" => (2, 5),
        "use" => (3, 6),
        "command" | "query" | "transition" | "event" => (4, 7),
        "component" => (5, 8),
        "eject" => (6, 9),
        _ => (7, 10),
    }
}

/// One declaration boundary in a lossless JDL document.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeclarationCst {
    pub kind: String,
    pub name: Option<String>,
    pub span: Span,
}

/// One declaration nested directly inside an entity block.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemberCst {
    pub owner: String,
    pub kind: String,
    pub name: Option<String>,
    pub span: Span,
}

/// Concrete source structure used by syntax-preserving edits.
///
/// `source` plus `tokens` preserves every input byte. Declaration spans make
/// local edits possible without re-rendering unrelated source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DocumentCst {
    source: String,
    pub tokens: Vec<Token>,
    pub declarations: Vec<DeclarationCst>,
    pub members: Vec<MemberCst>,
}

impl DocumentCst {
    #[cfg(test)]
    pub fn declaration_text(&self, declaration: &DeclarationCst) -> &str {
        &self.source[declaration.span.start..declaration.span.end]
    }

    pub(super) fn new(
        source: String,
        tokens: Vec<Token>,
        declarations: Vec<DeclarationCst>,
        members: Vec<MemberCst>,
    ) -> Self {
        Self {
            source,
            tokens,
            declarations,
            members,
        }
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn member_text(&self, member: &MemberCst) -> &str {
        &self.source[member.span.start..member.span.end]
    }

    pub fn reconstruct(&self) -> String {
        self.tokens
            .iter()
            .map(|token| token.text(&self.source))
            .collect()
    }

    /// Replace one declaration while preserving every unrelated source byte.
    /// The returned source is not written anywhere.
    pub fn replace_declaration(
        &self,
        declaration: &DeclarationCst,
        replacement: &str,
    ) -> Result<String, Diagnostics> {
        if !self.declarations.contains(declaration) {
            return Err(edit_problem(
                "the declaration does not belong to this CST",
                "select a declaration returned by this document",
            ));
        }
        self.replace_span(declaration.span, replacement)
    }

    /// Apply one byte-range edit. CLI mutations compose edits from CST-owned
    /// spans instead of rendering the complete semantic model.
    pub fn replace_span(&self, span: Span, replacement: &str) -> Result<String, Diagnostics> {
        if span.start > span.end
            || span.end > self.source.len()
            || !self.source.is_char_boundary(span.start)
            || !self.source.is_char_boundary(span.end)
        {
            return Err(edit_problem(
                "the JDL edit span is outside a UTF-8 source boundary",
                "use a span returned by the parsed CST",
            ));
        }
        let mut edited =
            String::with_capacity(self.source.len() - (span.end - span.start) + replacement.len());
        edited.push_str(&self.source[..span.start]);
        edited.push_str(replacement);
        edited.push_str(&self.source[span.end..]);
        Ok(edited)
    }
}

fn edit_problem(message: &str, fix: &str) -> Diagnostics {
    Diagnostics::from_vec(vec![Diagnostic::new("JDL1001", "$", message, fix)])
}
