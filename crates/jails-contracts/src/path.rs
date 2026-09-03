//! `ProjectPath` — the only spelling of "somewhere in this project".
//!
//! One constructor, and it rejects rather than normalises: absolute, empty,
//! trailing-slash, backslash, NUL, `.` and `..` are all errors. Normalising
//! would be the friendlier choice and the wrong one, because these values are
//! *keys* — a plan's preconditions, its operations and its tree manifest all
//! address the same file by this string, and two spellings that normalise to
//! one path would be two keys that address one file. The plan would then
//! contain a contradiction the digest could not see.
//!
//! Refusing `..` here is also what makes `is_within` sound. Nothing below the
//! compiler resolves symlinks or canonicalises against the disk, so a lexical
//! containment check is only an escape check if the path cannot lexically
//! escape in the first place.

use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};

/// A canonical path relative to the captured project root.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct ProjectPath(String);

impl ProjectPath {
    pub fn parse(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        if value.is_empty()
            || value.starts_with('/')
            || value.ends_with('/')
            || value.contains('\\')
            || value.contains('\0')
            || value
                .split('/')
                .any(|component| component.is_empty() || matches!(component, "." | ".."))
        {
            return Err(format!(
                "`{value}` is not a canonical project-relative path"
            ));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn join(&self, child: &str) -> Result<Self, String> {
        Self::parse(format!("{}/{child}", self.0))
    }

    pub fn is_within(&self, root: &Self) -> bool {
        self == root
            || self
                .0
                .strip_prefix(root.as_str())
                .is_some_and(|suffix| suffix.starts_with('/'))
    }
}

impl Display for ProjectPath {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_never_escape_the_captured_project() {
        assert!(ProjectPath::parse("src/main/java").is_ok());
        assert!(ProjectPath::parse("../outside").is_err());
        assert!(ProjectPath::parse("/absolute").is_err());
        assert!(ProjectPath::parse("a//b").is_err());
        assert!(ProjectPath::parse("a\\b").is_err());
    }
}
