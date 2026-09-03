//! Stable-ID index diffing and forward PostgreSQL statements.

use crate::Diagnostic;
use jails_model::{Entity, FieldId, StableId};
use std::collections::{BTreeMap, BTreeSet};

/// The fields this table can be *looked up by*: the leading column of every
/// index the DDL creates for it.
///
/// PostgreSQL uses a multi-column B-tree for a predicate that names its
/// leading column and cannot use it at all for one that names only a later
/// column, so what a table is cheaply searchable by is the set of first
/// columns rather than the union of all of them.
///
/// The five shapes are the five [`crate::emit_sql`] renders -- a column
/// `primary key`, a column `unique`, an `@index` field, a composite key
/// constraint and a declared index -- and
/// `every_index_the_ddl_creates_reports_its_leading_field` reads the emitted
/// statements back to prove this answer and that one agree. A sixth shape
/// added there fails that test rather than silently making a query look
/// served.
///
/// The search projection's GIN index is deliberately absent: it indexes a
/// generated `tsvector` column, which no field predicate names.
pub(crate) fn leading_fields(entity: &Entity) -> BTreeSet<FieldId> {
    let mut fields = entity
        .fields
        .iter()
        .filter(|field| field.primary_key || field.unique || field.indexed)
        .map(|field| field.id.clone())
        .collect::<BTreeSet<_>>();
    fields.extend(
        entity
            .constraints
            .values()
            .filter_map(|constraint| constraint.fields.first().cloned()),
    );
    fields.extend(
        entity
            .indexes
            .values()
            .filter_map(|index| index.columns.first())
            .map(|column| column.field.clone()),
    );
    fields
}

pub(crate) fn derive_changes(
    old: &Entity,
    current: &Entity,
    removals: &BTreeMap<String, String>,
    statements: &mut Vec<String>,
    semantic_ids: &mut BTreeSet<String>,
    descriptions: &mut Vec<String>,
) -> Result<(), Diagnostic> {
    for old_index in old.indexes.values() {
        let Some(current_index) = current.indexes.get(&old_index.id) else {
            let Some(confirmed_name) = removals.get(old_index.id.as_str()) else {
                return Err(Diagnostic::new(
                    "compile-index-removed-without-policy",
                    format!("$.entities.{}.indexes.{}", old.label, old_index.label),
                    format!(
                        "accepted index `{}` was removed without a drop policy",
                        old_index.sql_name
                    ),
                    format!(
                        "use `resource index remove {} {} --confirm-index {}`",
                        old.names.java_type, old_index.label, old_index.sql_name
                    ),
                ));
            };
            if confirmed_name != &old_index.sql_name {
                return Err(Diagnostic::new(
                    "compile-index-confirmation-mismatch",
                    format!("$.entities.{}.indexes.{}", old.label, old_index.label),
                    format!(
                        "confirmed index `{confirmed_name}` is not accepted index `{}`",
                        old_index.sql_name
                    ),
                    format!("pass `--confirm-index {}` exactly", old_index.sql_name),
                ));
            }
            statements.push(format!("drop index {};", old_index.sql_name));
            semantic_ids.extend([
                old.id.as_str().to_string(),
                old_index.id.as_str().to_string(),
            ]);
            descriptions.push(format!("drop_{}", old_index.sql_name));
            continue;
        };
        if old_index != current_index {
            return Err(Diagnostic::new(
                "compile-index-changed-without-policy",
                format!("$.entities.{}.indexes.{}", old.label, old_index.label),
                format!(
                    "accepted index `{}` changed without an evolution policy",
                    old_index.sql_name
                ),
                "add a replacement index before retiring this one",
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const MODEL: &str = "jdl 1\n\
app Demo {\n pkg com.example.demo\n java 26\n platform plain\n build maven\n storage postgres\n}\n\
entity Task {\n \
 id: uuid @pk\n \
 slug: string @unique\n \
 owner: uuid @index\n \
 region: string\n \
 bucket: string\n \
 created: instant\n \
 index [created desc, region]\n \
 unique [region, bucket]\n\
}\n\
use repo for Task\n";

    fn task() -> jails_model::Entity {
        jails_model::parse_jdl(MODEL)
            .expect("the fixture parses")
            .entities
            .into_values()
            .next()
            .expect("the fixture declares one entity")
    }

    /// The reported set is exactly the leading columns of the emitted DDL.
    ///
    /// This is the check that keeps [`leading_fields`] from becoming a second
    /// answer: it does not compare against a list somebody typed here, it
    /// reads the statements [`crate::emit_sql`] actually renders and takes the
    /// first column of each. A sixth index shape added there and not here
    /// fails this rather than quietly making a query look served.
    #[test]
    fn every_index_the_ddl_creates_reports_its_leading_field() {
        let model = jails_model::parse_jdl(MODEL).expect("the fixture parses");
        let entity = task();
        let statements =
            crate::emit_sql::create_table(&model, &entity).expect("the fixture renders");
        let from_ddl = leading_columns_of(&statements);
        assert!(
            from_ddl.len() >= 5,
            "the fixture must exercise every index shape: {statements:?}"
        );
        let reported = leading_fields(&entity)
            .iter()
            .map(|id| {
                entity
                    .field(id)
                    .expect("a reported field is the entity's")
                    .names
                    .sql_column
                    .clone()
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(reported, from_ddl, "{statements:?}");
    }

    /// A column no index leads with is not reported, however many it appears
    /// in. `bucket` is the second column of the unique tuple, and PostgreSQL
    /// cannot use that index for a predicate naming only `bucket`.
    #[test]
    fn a_trailing_column_of_a_composite_index_is_not_a_lookup_column() {
        let entity = task();
        let reported = leading_fields(&entity);
        let bucket = entity
            .fields
            .iter()
            .find(|field| field.names.sql_column == "bucket")
            .expect("the fixture declares bucket");
        assert!(!reported.contains(&bucket.id));
    }

    /// The first column of every `create index`, `primary key` and `unique`
    /// in a rendered `create table` block. Test-only, and deliberately dumb:
    /// its whole job is to not share code with the thing it is checking.
    fn leading_columns_of(statements: &[String]) -> BTreeSet<String> {
        let mut columns = BTreeSet::new();
        for statement in statements {
            if let Some(rest) = statement.strip_prefix("create index ") {
                columns.insert(first_column(&parenthesised(rest)));
                continue;
            }
            for line in statement.lines().map(str::trim) {
                // Not `)`: the closing paren is what `parenthesised` reads a
                // constraint's column list between.
                let line = line.trim_end_matches([',', ';']).trim();
                if let Some(rest) = line.strip_prefix("constraint ") {
                    if rest.contains(" primary key (") || rest.contains(" unique (") {
                        columns.insert(first_column(&parenthesised(rest)));
                    }
                    continue;
                }
                if line.ends_with(" primary key") || line.ends_with(" unique") {
                    columns.insert(first_column(line));
                }
            }
        }
        columns
    }

    fn parenthesised(text: &str) -> String {
        text.split_once('(')
            .and_then(|(_, rest)| rest.split_once(')'))
            .map(|(inside, _)| inside.to_string())
            .unwrap_or_default()
    }

    fn first_column(text: &str) -> String {
        text.split(',')
            .next()
            .unwrap_or_default()
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_string()
    }
}
