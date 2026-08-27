//! What shape a read takes, decided before anything is rendered.
//!
//! `query.rs` is the renderers -- the criteria record, the port, the adapter,
//! the controller and their tests. This is the half that answers *what* they
//! render: which columns the select reads, which side of a join each filter
//! lives on, how the rows come back and how many. Split out under
//! `abstract.md` rung 11 when the file passed the largest-module ceiling; the
//! two halves share a subject and not a secret.

use super::*;

/// The two column lists one query reads through: what it selects, and what it
/// filters on. Both are derived from the same field spec in one place, which
/// is what stops a select and a where clause naming different columns.
pub(super) struct Projection {
    pub(super) target_columns: Vec<crate::sql::Column>,
    pub(super) filter_columns: Vec<crate::sql::Column>,
    /// The table qualifier each filter's column takes, parallel to
    /// `filter_columns`. Empty for an unjoined query, where a bare column name
    /// is unambiguous and qualifying it would churn every golden for nothing.
    pub(super) filter_qualifiers: Vec<String>,
    pub(super) join: Option<Join>,
    /// The `order by` clause's columns, already in the order they were
    /// declared. Empty means the adapter's own rule.
    pub(super) ordering: Vec<String>,
    pub(super) limit: u32,
}

/// The row ceiling an equality query has always applied. Stated once, here,
/// because it is now also what a refusal and the Javadoc quote.
pub(super) const DEFAULT_MAX_RESULTS: u32 = 100;

/// What a caller asked this read to look like: the order, the ceiling and the
/// route.
///
/// All three were decisions a generator made silently -- newest first with the
/// key as the tiebreak, 100 rows, and `/queries/<name>` -- and none was
/// sayable from the command line. `missing.md` M5's smaller half and M8. They
/// travel together because they are one question asked three ways: what this
/// read is, rather than what it filters on.
pub(crate) struct Bounds<'a> {
    pub(crate) order_by: Option<&'a str>,
    pub(crate) limit: Option<u32>,
}

/// The declared order, as column names, or empty for the adapter's own rule.
///
/// Accepts the component (`sentAt`) or the column it maps to (`sent_at`):
/// both name exactly one column, and refusing one of two unambiguous spellings
/// would be arbitrary. `asc`/`desc` and nothing else follows a name -- the
/// shape check is `IndexSpec::parse_columns`, and this is the referential half
/// it deliberately leaves out, done where the record is readable.
pub(super) fn declared_ordering(
    name: &str,
    target: &str,
    target_columns: &[crate::sql::Column],
    order_by: Option<&str>,
) -> jails_support::Result<Vec<String>> {
    let Some(token) = order_by else {
        return Ok(Vec::new());
    };
    let declared = jails_protocol::declaration::IndexSpec::parse_columns(token)?;
    let mut ordering = Vec::with_capacity(declared.columns.len());
    for column in &declared.columns {
        let named = column.field.as_str();
        let Some(found) = target_columns
            .iter()
            .find(|candidate| candidate.component == named || candidate.name == named)
        else {
            return Err(format!(
                "query {name} orders by `{named}`, which {target} does not declare.\n       \
                 fix: order by one of: {}.",
                target_columns
                    .iter()
                    .map(|candidate| candidate.component.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
            .into());
        };
        ordering.push(match column.direction {
            jails_protocol::declaration::IndexDirection::Ascending => found.name.clone(),
            jails_protocol::declaration::IndexDirection::Descending => {
                format!("{} desc", found.name)
            }
        });
    }
    Ok(ordering)
}
