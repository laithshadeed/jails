//! `SqlName` — a validated unquoted SQL identifier.
//!
//! Deliberately narrow: lowercase snake case, the shape jails' own generated
//! tables have. It is used at destructive lifecycle boundaries, where the
//! reader confirms a drop by typing the table's exact name, so accepting a
//! broader SQL expression would make that confirmation mean less than it
//! looks like it means.
//!
//! One constructor, refusing rather than quoting or normalising.

use crate::Result;

/// A validated unquoted SQL identifier used at destructive lifecycle
/// boundaries. Generated table names are lowercase snake case, so accepting a
/// broader SQL expression here would make exact confirmation meaningless.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct SqlName(String);

impl SqlName {
    pub fn parse(value: &str) -> Result<Self> {
        let mut chars = value.chars();
        let Some(first) = chars.next() else {
            return Err(
                "SQL name is empty.\n       fix: pass the exact generated table name.".into(),
            );
        };
        if !(first.is_ascii_lowercase() || first == '_')
            || !chars.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_')
        {
            return Err(format!(
                "`{value}` is not a lowercase unquoted SQL name.\n       \
                 fix: pass the exact generated table name, for example `tasks`."
            )
            .into());
        }
        Ok(Self(value.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}
