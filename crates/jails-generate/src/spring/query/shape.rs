//! What shape a read takes, decided before anything is rendered.
//!
//! `query.rs` is the renderers -- the criteria record, the port, the adapter,
//! the controller and their tests. This is the half that answers *what* they
//! render: which columns the select reads, which side of a join each filter
//! lives on, how the rows come back and how many. Split out under
//! `abstract.md` rung 11 when the file passed the largest-module ceiling; the
//! two halves share a subject and not a secret.

use super::*;
use crate::model::{Layer, Slice};

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

/// The second table a query reads, and the column pair that joins them.
///
/// `--via <Parent>` names the *type*, not the association. An association
/// records its mapping only in the migration it wrote, and re-reading
/// generated SQL to recover a decision is the guessing `build.rs` refuses to
/// do with a build file. The join column is derived from the two records
/// instead: `<parent>Id` when the child has it, otherwise the single component
/// of the parent key's type whose name ends in `Id`. Two candidates is a
/// refusal naming both.
pub(super) struct Join {
    pub(super) parent: String,
    pub(super) parent_table: String,
    /// The child component that references the parent, and its column.
    pub(super) child_component: String,
    pub(super) child_column: String,
    /// The parent's own key component, and its column.
    pub(super) parent_component: String,
    pub(super) parent_column: String,
    pub(super) parent_fields: Vec<crate::generate::Field>,
}

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
    pub(crate) path: Option<&'a str>,
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

/// Work out how the two tables meet, or refuse and say what was looked for.
pub(super) fn resolve_join(
    slice: &Slice,
    name: &str,
    target: &str,
    target_fields: &[crate::generate::Field],
    parent: &str,
) -> jails_support::Result<Join> {
    if parent == target {
        return Err(format!(
            "query {name} joins {target} to itself.\n       fix: drop `--via {parent}`; a query \
             already filters on its own components."
        )
        .into());
    }
    let domain: &str = &slice.owned(Layer::Domain);
    let parent_fields = Target::read(slice, "query", name, parent)?.fields;
    let parent_columns = crate::sql::columns(&parent_fields, slice.project(), domain, "row");
    let parent_key = crate::sql::key_column(&parent_columns).ok_or_else(|| {
        format!(
            "query {name} joins through {parent}, which declares no key to join on.\n       \
             fix: give {parent} one `@pk` component."
        )
    })?;
    let parent_component = parent_key.component.clone();
    let parent_key_type = parent_fields
        .iter()
        .find(|field| field.name == parent_component)
        .map(|field| usecase_normalized_type(&field.java_type))
        .unwrap_or_default();
    // The conventional name first -- `<parent>Id` is what the outbox,
    // `association` and `durable-job` all already read -- then the one
    // component that could be it. Never a choice between two.
    let conventional = format!("{}Id", crate::generate::lower_first(parent));
    let child = target_fields
        .iter()
        .find(|field| field.name == conventional)
        .or_else(|| {
            let candidates = target_fields
                .iter()
                .filter(|field| {
                    field.name.ends_with("Id")
                        && usecase_normalized_type(&field.java_type) == parent_key_type
                })
                .collect::<Vec<_>>();
            match candidates.as_slice() {
                [only] => Some(*only),
                _ => None,
            }
        });
    let Some(child) = child else {
        let candidates = target_fields
            .iter()
            .filter(|field| field.name.ends_with("Id"))
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>();
        return Err(format!(
            "query {name} joins {target} to {parent}, but jails cannot tell which component of \
             {target} references it{}.\n       fix: name it `{conventional}`, the convention \
             every other reference here uses.",
            if candidates.is_empty() {
                String::new()
            } else {
                format!(" -- candidates: {}", candidates.join(", "))
            }
        )
        .into());
    };
    let child_columns = crate::sql::columns(target_fields, slice.project(), domain, "row");
    let child_column = child_columns
        .iter()
        .find(|column| column.component == child.name)
        .map(|column| column.name.clone())
        .ok_or_else(|| {
            format!(
                "query {name} cannot map {target}.{} to a column",
                child.name
            )
        })?;
    Ok(Join {
        parent: parent.to_string(),
        parent_table: crate::sql::table_name(parent),
        child_component: child.name.clone(),
        child_column,
        parent_component,
        parent_column: parent_key.name.clone(),
        parent_fields,
    })
}
