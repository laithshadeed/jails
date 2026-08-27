//! Which column of one resource references another.
//!
//! Shared because two recipes ask it and the answer has to be the same both
//! times. `g query --via` reads the parent alongside the child; `g usecase
//! --via` resolves the child's foreign key from a component of the parent on
//! the way in. A second copy of "which component is the reference" is a second
//! answer, and the one that differs is the one nobody tested.

use super::*;

/// One resolved reference between two resources.
pub(crate) struct Join {
    pub(crate) parent: String,
    pub(crate) parent_table: String,
    /// The child component that references the parent, and its column.
    pub(crate) child_component: String,
    pub(crate) child_column: String,
    /// The parent's own key component, and its column.
    pub(crate) parent_component: String,
    pub(crate) parent_column: String,
    pub(crate) parent_fields: Vec<crate::generate::Field>,
}

/// Work out how the two tables meet, or refuse and say what was looked for.
///
/// The join column is derived from the two records rather than recorded:
/// `<parent>Id` when the child has it, otherwise the single component of the
/// parent key's type whose name ends in `Id`. Two candidates is a refusal
/// naming both, never a choice -- re-reading generated SQL to recover a
/// decision is the guessing `build.rs` refuses to do with a build file.
pub(crate) fn resolve_join(
    slice: &Slice,
    recipe: &str,
    name: &str,
    target: &str,
    target_fields: &[crate::generate::Field],
    parent: &str,
) -> jails_support::Result<Join> {
    if parent == target {
        return Err(format!(
            "{recipe} {name} joins {target} to itself.\n       fix: drop `--via {parent}`; this recipe \
             already reads its own components."
        )
        .into());
    }
    let domain: &str = &slice.owned(Layer::Domain);
    let parent_fields = Target::read(slice, recipe, name, parent)?.fields;
    let parent_columns = crate::sql::columns(&parent_fields, slice.project(), domain, "row");
    let parent_key = crate::sql::key_column(&parent_columns).ok_or_else(|| {
        format!(
            "{recipe} {name} joins through {parent}, which declares no key to join on.\n       \
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
            "{recipe} {name} joins {target} to {parent}, but jails cannot tell which component of \
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
                "{recipe} {name} cannot map {target}.{} to a column",
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
