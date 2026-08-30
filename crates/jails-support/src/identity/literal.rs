//! A value the caller pins a generated component to, rather than one the
//! request carries.
//!
//! `POST /admin_api/messages` must write `sender_type = ADMIN` and
//! `POST /customer_api/messages` must write `CUSTOMER`. With the component in
//! the request both endpoints take it from the caller, so either one can forge
//! the other's messages -- and no amount of validation on the request fixes
//! that, because a well-formed request is exactly what the forgery looks like.
//!
//! **A closed alphabet, not a Java expression.** This is the same rule that
//! keeps `@check(...)` out of the field spec: a passthrough would be text
//! jails writes into a constructor argument without being able to say what it
//! means, and the failure would arrive as a compile error in a file the reader
//! did not write. What is accepted here is a *literal* -- an enum constant, a
//! boolean, a number, a short piece of text -- and the generator resolves it
//! against the declared type of the component it pins, refusing by name when
//! the two cannot be reconciled.
//!
//! The alphabet is `research.md` §4.1's `shell-safe-literal`, and it is
//! deliberately shell-safe rather than merely Java-safe: every documented
//! example has to survive being typed unquoted into Bash, Zsh, Fish and
//! PowerShell, so no braces, brackets, pipes, glob characters, quotes or
//! spaces.

use crate::Result;
use crate::codec::{Codec, Decoder, Encoder};

/// The longest literal jails will record. Long enough for any constant, any
/// number and a short label; short enough that a pasted document is refused
/// rather than written into a Java file.
const MAX: usize = 64;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct LiteralValue(String);

impl LiteralValue {
    pub fn parse(value: &str) -> Result<Self> {
        if value.is_empty() {
            return Err(
                "a pinned value is empty.\n       fix: write the constant the component \
                        must hold, for example `--set senderType=ADMIN`."
                    .to_string()
                    .into(),
            );
        }
        if value.len() > MAX {
            return Err(format!(
                "pinned value is {} characters, over the {MAX}-character limit.\n       fix: a \
                 pinned value is a literal, not a document -- a longer default belongs in the \
                 code that reads it.",
                value.len()
            )
            .into());
        }
        if let Some(bad) = value.chars().find(|c| !is_literal_char(*c)) {
            return Err(format!(
                "pinned value `{value}` contains `{bad}`.\n       fix: a pinned value is made \
                 of letters, digits, `_`, `.`, `:`, `+` and `-`. Anything an expression could \
                 hide in would be text jails writes into your code without being able to say \
                 what it means."
            )
            .into());
        }
        Ok(Self(value.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn is_literal_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | ':' | '+' | '-')
}

impl std::fmt::Display for LiteralValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl Codec for LiteralValue {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.string(&self.0)
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Self::parse(&decoder.string()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_literals_a_real_contract_pins_are_kept_exactly() {
        for value in [
            "ADMIN",
            "CUSTOMER",
            "true",
            "false",
            "0",
            "-1",
            "1.5",
            "2024-01-01T00:00:00Z",
            "en-GB",
        ] {
            assert_eq!(LiteralValue::parse(value).unwrap().as_str(), value);
        }
    }

    /// Every refusal names the character, because the whole point of the
    /// closed alphabet is that an expression cannot be smuggled through as a
    /// value.
    #[test]
    fn anything_an_expression_could_hide_in_is_refused_by_name() {
        for (value, expected) in [
            ("", "is empty"),
            ("new Date()", "contains ` `"),
            ("Status.of(x)", "contains `(`"),
            ("a\"b", "contains `\"`"),
            ("$(whoami)", "contains `$`"),
            ("a;b", "contains `;`"),
        ] {
            let error = LiteralValue::parse(value).unwrap_err();
            assert!(error.contains(expected), "{value}: {error}");
        }
    }

    #[test]
    fn a_pasted_document_is_refused_rather_than_written_into_java() {
        let error = LiteralValue::parse(&"a".repeat(MAX + 1)).unwrap_err();
        assert!(error.contains("over the"), "{error}");
    }
}
