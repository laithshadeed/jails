//! A route path a caller names, rather than one jails derives.
//!
//! Derived paths are a virtue greenfield -- one shape, and every generated
//! surface agrees about it. They are unusable when the URLs are a fixed
//! external contract, which is what `missing.md` M8 measured: the ported
//! originals answer `/customer_api/ping`, `/admin_api/issues`,
//! `/api/conversations/`, and none of those is derivable from any name jails
//! would accept for the class.
//!
//! The derivability argument does not block it. `destroy` finds files by what
//! the ledger recorded rather than by recomputing paths, so a recorded path is
//! no harder to undo than a recorded `--package`.
//!
//! **Validated, not passed through.** A route is text jails writes into an
//! annotation, so the closed set here is what stops a value that reads as a
//! path from being something else: no whitespace (a mapping with a space in it
//! is a route nothing can reach), no `..` (a traversal jails would be
//! spelling for the reader), and only the characters Spring's own path
//! grammar uses -- segments, path variables in braces, and the wildcards.

use crate::Result;
use crate::codec::{Codec, Decoder, Encoder};

/// The longest path jails will record. Long enough for any real contract and
/// short enough that a pasted document is refused rather than written into a
/// Java file.
const MAX: usize = 200;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct RoutePath(String);

impl RoutePath {
    pub fn parse(value: &str) -> Result<Self> {
        if !value.starts_with('/') {
            return Err(format!(
                "route path `{value}` does not start with `/`.\n       fix: write it as the \
                 caller sends it, for example `/customer_api/ping`."
            )
            .into());
        }
        if value.len() > MAX {
            return Err(format!(
                "route path is {} characters, over the {MAX}-character limit.\n       fix: a \
                 route is a path, not a document -- pass the one the caller sends.",
                value.len()
            )
            .into());
        }
        if value.contains("..") {
            return Err(format!(
                "route path `{value}` contains `..`.\n       fix: write the path the route \
                 answers, not one relative to something else."
            )
            .into());
        }
        if let Some(bad) = value.chars().find(|c| !is_route_char(*c)) {
            return Err(format!(
                "route path `{value}` contains `{bad}`.\n       fix: paths are made of \
                 `/`-separated segments of letters, digits, `_`, `-` and `.`, plus `{{name}}` \
                 variables and `*` wildcards."
            )
            .into());
        }
        Ok(Self(value.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn is_route_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '/' | '_' | '-' | '.' | '{' | '}' | '*' | ':')
}

impl std::fmt::Display for RoutePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl Codec for RoutePath {
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
    fn a_contract_path_is_kept_exactly_as_the_caller_sends_it() {
        for path in [
            "/customer_api/ping",
            "/api/conversations/",
            "/admin_api/messages",
            "/api/messages/{id}/read",
        ] {
            assert_eq!(RoutePath::parse(path).unwrap().as_str(), path);
        }
    }

    /// Every refusal names the thing that is wrong, because a route is text
    /// jails writes into an annotation and a passthrough would be jails
    /// spelling whatever it was handed.
    #[test]
    fn a_path_that_is_not_one_is_refused_by_name() {
        for (path, expected) in [
            ("customer_api/ping", "does not start with"),
            ("/customer api/ping", "contains ` `"),
            ("/api/../secret", "contains `..`"),
            ("/api/<script>", "contains `<`"),
        ] {
            let error = RoutePath::parse(path).unwrap_err();
            assert!(error.contains(expected), "{path}: {error}");
        }
    }
}
