//! The lossless lexer: every byte of the document ends up in a token.
//!
//! Trivia is a token kind rather than something skipped. That is what lets
//! `cst.rs` reconstruct the source exactly and lets an edit replace one
//! declaration's span while every comment and blank line around it stays
//! byte-identical.
//!
//! `Span` is a byte range into the original input, so a diagnostic can point at
//! the source the reader wrote rather than at a re-rendered approximation of
//! it. Every token carries one; nothing here reformats on the way through.

use crate::{Diagnostic, Diagnostics};

/// A UTF-8 byte range in the original JDL document.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}

/// Lossless lexical categories. Trivia is retained instead of discarded.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TokenKind {
    Word,
    Integer,
    String,
    Symbol,
    Whitespace,
    Comment,
    Newline,
    TriviaNewline,
    Eof,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl Token {
    pub fn text<'a>(&self, source: &'a str) -> &'a str {
        &source[self.span.start..self.span.end]
    }

    pub const fn is_trivia(&self) -> bool {
        matches!(
            self.kind,
            TokenKind::Whitespace | TokenKind::Comment | TokenKind::TriviaNewline
        )
    }
}

pub(super) fn lex(source: &str) -> Result<Vec<Token>, Diagnostics> {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut offset = 0;
    let mut parens = 0_u32;
    let mut brackets = 0_u32;
    let mut line_has_syntax = false;

    while offset < bytes.len() {
        let start = offset;
        match bytes[offset] {
            b' ' | b'\t' => {
                offset += 1;
                while offset < bytes.len() && matches!(bytes[offset], b' ' | b'\t') {
                    offset += 1;
                }
                push(&mut tokens, TokenKind::Whitespace, start, offset);
            }
            b'\r' if bytes.get(offset + 1) == Some(&b'\n') => {
                offset += 2;
                newline(
                    &mut tokens,
                    start,
                    offset,
                    &mut line_has_syntax,
                    parens,
                    brackets,
                );
            }
            b'\n' => {
                offset += 1;
                newline(
                    &mut tokens,
                    start,
                    offset,
                    &mut line_has_syntax,
                    parens,
                    brackets,
                );
            }
            b'/' if bytes.get(offset + 1) == Some(&b'/') => {
                offset += 2;
                while offset < bytes.len() && !matches!(bytes[offset], b'\r' | b'\n') {
                    offset += 1;
                }
                push(&mut tokens, TokenKind::Comment, start, offset);
            }
            b'"' => {
                offset += 1;
                let mut escaped = false;
                while offset < bytes.len() {
                    let byte = bytes[offset];
                    if matches!(byte, b'\r' | b'\n') {
                        return Err(problem(
                            source,
                            start,
                            "JDL0003",
                            "a string literal crosses a physical line",
                            "close the quoted string before the newline",
                        ));
                    }
                    offset += 1;
                    if escaped {
                        escaped = false;
                    } else if byte == b'\\' {
                        escaped = true;
                    } else if byte == b'"' {
                        break;
                    }
                }
                if bytes.get(offset.saturating_sub(1)) != Some(&b'"') {
                    return Err(problem(
                        source,
                        start,
                        "JDL0003",
                        "a string literal is not closed",
                        "close the string with `\"`",
                    ));
                }
                let encoded = &source[start..offset];
                serde_json::from_str::<String>(encoded).map_err(|error| {
                    problem(
                        source,
                        start,
                        "JDL0004",
                        format!("invalid JSON string escape: {error}"),
                        "use JSON escapes such as `\\\"`, `\\\\`, `\\n`, or `\\u1234`",
                    )
                })?;
                push(&mut tokens, TokenKind::String, start, offset);
                line_has_syntax = true;
            }
            b'-' if bytes.get(offset + 1) == Some(&b'>') => {
                offset += 2;
                push(&mut tokens, TokenKind::Symbol, start, offset);
                line_has_syntax = true;
            }
            byte if is_symbol(byte) => {
                offset += 1;
                match byte {
                    b'(' => parens += 1,
                    b')' => parens = parens.saturating_sub(1),
                    b'[' => brackets += 1,
                    b']' => brackets = brackets.saturating_sub(1),
                    _ => {}
                }
                push(&mut tokens, TokenKind::Symbol, start, offset);
                line_has_syntax = true;
            }
            byte if is_word_byte(byte) || (byte == b'-' && is_digit(bytes.get(offset + 1))) => {
                offset += 1;
                while offset < bytes.len() && is_word_byte(bytes[offset]) {
                    offset += 1;
                }
                let text = &source[start..offset];
                let kind = if is_integer(text) {
                    TokenKind::Integer
                } else {
                    TokenKind::Word
                };
                push(&mut tokens, kind, start, offset);
                line_has_syntax = true;
            }
            _ => {
                let character = source[offset..]
                    .chars()
                    .next()
                    .expect("offset is in source");
                // **`#` is the one wrong guess worth answering.** A reader
                // coming from `application.properties`, YAML or a shell
                // writes a `#` comment, and "use a token from the JDL v1
                // grammar" tells them the character is wrong without telling
                // them the thing they were trying to do is spelled `//`.
                // Every other unexpected character is a typo, where naming
                // the grammar is the whole of the answer.
                let fix = match character {
                    '#' => "comments start with `//`",
                    _ => "remove it or use a token from the JDL v1 grammar",
                };
                return Err(problem(
                    source,
                    offset,
                    "JDL0002",
                    format!("unexpected character `{character}`"),
                    fix,
                ));
            }
        }
    }

    if line_has_syntax {
        push(&mut tokens, TokenKind::Newline, offset, offset);
    }
    push(&mut tokens, TokenKind::Eof, offset, offset);
    Ok(tokens)
}

fn newline(
    tokens: &mut Vec<Token>,
    start: usize,
    end: usize,
    line_has_syntax: &mut bool,
    parens: u32,
    brackets: u32,
) {
    let kind = if *line_has_syntax && parens == 0 && brackets == 0 {
        TokenKind::Newline
    } else {
        TokenKind::TriviaNewline
    };
    push(tokens, kind, start, end);
    if parens == 0 && brackets == 0 {
        *line_has_syntax = false;
    }
}

fn push(tokens: &mut Vec<Token>, kind: TokenKind, start: usize, end: usize) {
    tokens.push(Token {
        kind,
        span: Span::new(start, end),
    });
}

fn is_symbol(byte: u8) -> bool {
    matches!(
        byte,
        b'{' | b'}'
            | b'('
            | b')'
            | b'['
            | b']'
            | b','
            | b':'
            | b'?'
            | b'@'
            | b'='
            | b'<'
            | b'>'
            | b'*'
    )
}

fn is_word_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.')
}

fn is_digit(byte: Option<&u8>) -> bool {
    byte.is_some_and(u8::is_ascii_digit)
}

fn is_integer(value: &str) -> bool {
    let digits = value.strip_prefix('-').unwrap_or(value);
    !digits.is_empty()
        && digits.bytes().all(|byte| byte.is_ascii_digit())
        && (digits == "0" || !digits.starts_with('0'))
}

pub(super) fn problem(
    source: &str,
    offset: usize,
    code: &'static str,
    message: impl Into<String>,
    fix: impl Into<String>,
) -> Diagnostics {
    let (line, column) = line_column(source, offset);
    // The path keeps the prose spelling it has always had, and the same two
    // numbers go into the fields an editor reads. One is the sentence, the
    // other is the jump.
    Diagnostics::from_vec(vec![
        Diagnostic::new(
            code,
            format!("line {line}, column {column}, byte {offset}"),
            message,
            fix,
        )
        .at(
            u32::try_from(line).unwrap_or(u32::MAX),
            u32::try_from(column).unwrap_or(u32::MAX),
        ),
    ])
}

fn line_column(source: &str, offset: usize) -> (usize, usize) {
    let prefix = &source[..offset.min(source.len())];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let line_start = prefix.rfind('\n').map_or(0, |position| position + 1);
    let column = prefix[line_start..].chars().count() + 1;
    (line, column)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lexer_is_lossless_for_comments_blank_lines_and_crlf() {
        let source = "// lead\r\n\r\njdl 1\r\napp Demo {} // tail\r\n";
        let tokens = lex(source).unwrap();
        let reconstructed = tokens
            .iter()
            .map(|token| token.text(source))
            .collect::<String>();
        assert_eq!(reconstructed, source);
        assert!(tokens.iter().any(|token| token.kind == TokenKind::Comment));
        assert!(
            tokens
                .iter()
                .any(|token| token.kind == TokenKind::TriviaNewline)
        );
    }

    #[test]
    fn newline_inside_delimiters_is_not_a_statement_boundary() {
        let tokens = lex("jdl 1\nuse search(\n fields: [title]\n)\n").unwrap();
        assert_eq!(
            tokens
                .iter()
                .filter(|token| token.kind == TokenKind::Newline)
                .count(),
            2
        );
    }
}
