//! The name a request parameter arrives under, when it is not the component's
//! own.
//!
//! Spring's **data binder** has no naming strategy. Jackson has one and applies
//! it to JSON without help, so a project whose responses are snake_case still
//! binds a *form* field called `userId` unless each component says otherwise --
//! which is why `@BindParam` is generated from the project's wire naming at
//! all.
//!
//! Derivation covers the ordinary case and cannot cover this one: the brief's
//! own customer page reads `message.id` out of the response and posts
//! `message_id` back. The same value has two names on two wires, and neither
//! is derivable from the other. So it is a name the reader types.
//!
//! Narrower than a literal on purpose: a request parameter is a name, so no
//! `:` and no `+`.

use crate::Result;
use crate::codec::{Codec, Decoder, Encoder};

/// Long enough for any real parameter, short enough that a pasted document is
/// refused rather than written into an annotation.
const MAX: usize = 64;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct WireName(String);

impl WireName {
    pub fn parse(value: &str) -> Result<Self> {
        if value.is_empty() {
            return Err(
                "a bound name is empty.\n       fix: write the request parameter the \
                        caller sends, for example `--bind id=message_id`."
                    .to_string()
                    .into(),
            );
        }
        if value.len() > MAX {
            return Err(format!(
                "bound name is {} characters, over the {MAX}-character limit.\n       fix: a \
                 bound name is a request parameter, not a document.",
                value.len()
            )
            .into());
        }
        if let Some(bad) = value.chars().find(|c| !is_wire_char(*c)) {
            return Err(format!(
                "bound name `{value}` contains `{bad}`.\n       fix: a request parameter is \
                 made of letters, digits, `_`, `-` and `.`."
            )
            .into());
        }
        Ok(Self(value.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn is_wire_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.')
}

impl std::fmt::Display for WireName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl Codec for WireName {
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
    fn the_names_a_real_form_sends_are_kept_exactly() {
        for value in ["message_id", "user_id", "email", "sender-type", "a.b"] {
            assert_eq!(WireName::parse(value).unwrap().as_str(), value);
        }
    }

    #[test]
    fn anything_that_is_not_a_parameter_name_is_refused_by_name() {
        for (value, expected) in [
            ("", "is empty"),
            ("message id", "contains ` `"),
            ("a[b]", "contains `[`"),
            ("a:b", "contains `:`"),
        ] {
            let error = WireName::parse(value).unwrap_err();
            assert!(error.contains(expected), "{value}: {error}");
        }
    }
}
