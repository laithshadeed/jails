use super::token::{Span, Token};
use crate::{Diagnostic, Diagnostics};

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

    pub fn declaration_text(&self, declaration: &DeclarationCst) -> &str {
        &self.source[declaration.span.start..declaration.span.end]
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
