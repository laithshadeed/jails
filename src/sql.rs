//! Mapping a field spec to SQL: the column, its type, and the two JDBC
//! expressions that move a value across the boundary.
//!
//! This is what lets `generate repo` and `generate scaffold` emit a JDBC
//! adapter with no TODOs in it. The information was always there -- `jails g
//! scaffold Reward transactionId:uuid amount:long currency:Currency` states
//! every column and every type -- it just was not being used past the record.
//!
//! The whole point is that one field spec produces the DDL, the insert, the
//! bind and the row mapper *together*, so they cannot disagree. Hand-written,
//! they routinely do: a column called `amount` in the insert and `amount_minor`
//! in the select compiles, passes review, and fails at runtime.
//!
//! What it will not do is guess. A type jails cannot map (a project class that
//! is not an enum, a collection) produces a clearly marked TODO for that one
//! column rather than a plausible-looking wrong mapping, and the generated
//! Javadoc names it.

use std::path::Path;

use crate::generate::{Field, Optionality};

/// How one record component crosses the JDBC boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Column {
    /// The column name: the component name in snake_case.
    pub name: String,
    /// The PostgreSQL type for a `create table`.
    pub sql_type: String,
    /// Whether the DDL should say `not null`.
    pub not_null: bool,
    /// An expression reading this column off a `ResultSet` named `rows`.
    /// `None` when jails cannot map the type.
    pub read: Option<String>,
    /// An expression producing the value to bind, given a variable holding
    /// the record. `None` when jails cannot map the type.
    pub write: Option<String>,
    /// The component's Java type with any `Optional<>` peeled off. Kept so
    /// the caller can work out which imports the generated expressions need
    /// without re-parsing them out of the strings.
    pub java_type: String,
}

impl Column {
    /// True when jails knows how to move this column both ways.
    pub fn mapped(&self) -> bool {
        self.read.is_some() && self.write.is_some()
    }
}

/// Derive a column for every component. `root`/`pkg` are needed to recognise
/// a project enum, which is the one owned type jails can map -- an enum is
/// stored as its name and read back with `valueOf`.
///
/// `receiver` is the name of the variable holding the record in the
/// generated code, and it is baked into every write expression rather than
/// prefixed by the caller. It has to be: `Timestamp.from(x.createdAt())`
/// puts the receiver in the middle, so a caller gluing `receiver + "." +
/// write` together produces `x.Timestamp.from(createdAt())`, which is
/// exactly the kind of not-quite-right code this module exists to stop
/// anyone from writing by hand.
pub(crate) fn columns(fields: &[Field], root: &Path, pkg: &str, receiver: &str) -> Vec<Column> {
    fields
        .iter()
        .map(|field| column(field, root, pkg, receiver))
        .collect()
}

fn column(field: &Field, root: &Path, pkg: &str, receiver: &str) -> Column {
    let name = snake_case(&field.name);
    let accessor = format!("{receiver}.{}()", field.name);
    // `?` means the component is an Optional<T>; the column is nullable and
    // the value has to be unwrapped on the way out and re-wrapped on the way
    // in. Everything else about the mapping is the same, so the inner type
    // drives it.
    let optional = field.optionality == Optionality::Nullable;
    let not_null = !optional;

    if field.collection {
        // A List or Map is not one column. Splitting it into a join table is
        // a schema decision jails has no business making silently.
        return Column {
            name,
            sql_type: "jsonb".into(),
            not_null,
            read: None,
            write: None,
            java_type: field.java_type.clone(),
        };
    }

    let inner = inner_type(&field.java_type);
    if let Some((sql_type, read, write)) = builtin_mapping(&inner, &name, &accessor) {
        return finish(name, sql_type, not_null, optional, read, write, &inner);
    }

    // The one owned type with a knowable representation.
    if field.owned && crate::generate::is_enum_type(root, pkg, &inner) {
        let read = format!("{inner}.valueOf(rows.getString(\"{name}\"))");
        let write = format!("{accessor}.name()");
        return finish(name, "text".into(), not_null, optional, read, write, &inner);
    }

    Column {
        name,
        sql_type: "text".into(),
        not_null,
        read: None,
        write: None,
        java_type: inner,
    }
}

/// Wrap the mapping for an `Optional` component. The column stays the same;
/// only the two expressions change.
fn finish(
    name: String,
    sql_type: String,
    not_null: bool,
    optional: bool,
    read: String,
    write: String,
    inner: &str,
) -> Column {
    if !optional {
        return Column {
            name,
            sql_type,
            not_null,
            read: Some(read),
            write: Some(write),
            java_type: inner.to_string(),
        };
    }
    // A null column must come back as Optional.empty(), not as an Optional
    // wrapping null -- and `getLong` on a null column returns 0, not null,
    // so a primitive read has to be guarded by wasNull() rather than by a
    // null check on its result.
    let read = if is_primitive_read(inner) {
        format!("rows.getObject(\"{name}\") == null ? Optional.empty() : Optional.of({read})")
    } else {
        format!("Optional.ofNullable({read})")
    };
    let write = format!("{write}.orElse(null)");
    Column {
        name,
        sql_type,
        not_null: false,
        read: Some(read),
        write: Some(write),
        java_type: inner.to_string(),
    }
}

/// Types whose JDBC getter returns a primitive and so cannot report null.
fn is_primitive_read(inner: &str) -> bool {
    matches!(
        inner,
        "int" | "Integer" | "long" | "Long" | "double" | "Double" | "boolean" | "Boolean"
    )
}

/// `Optional<Instant>` -> `Instant`; anything else is returned unchanged.
fn inner_type(java_type: &str) -> String {
    java_type
        .strip_prefix("Optional<")
        .and_then(|rest| rest.strip_suffix('>'))
        .unwrap_or(java_type)
        .to_string()
}

/// (sql type, read expression, write expression) for a type from jails' own
/// table. The read expressions deliberately prefer `getObject(name, T.class)`
/// where the driver supports it -- it round-trips UUID and java.time without
/// the string conversions that make hand-written mappers wrong.
fn builtin_mapping(inner: &str, column: &str, accessor: &str) -> Option<(String, String, String)> {
    let (sql, read, write) = match inner {
        "String" => ("text", format!("rows.getString(\"{column}\")"), accessor.to_string()),
        "Integer" | "int" => ("integer", format!("rows.getInt(\"{column}\")"), accessor.to_string()),
        "Long" | "long" => ("bigint", format!("rows.getLong(\"{column}\")"), accessor.to_string()),
        "Boolean" | "boolean" => (
            "boolean",
            format!("rows.getBoolean(\"{column}\")"),
            accessor.to_string(),
        ),
        "Double" | "double" => (
            "double precision",
            format!("rows.getDouble(\"{column}\")"),
            accessor.to_string(),
        ),
        // numeric without a precision: money in a float is a bug, and jails
        // has no way to know the scale this column wants.
        "BigDecimal" => (
            "numeric",
            format!("rows.getBigDecimal(\"{column}\")"),
            accessor.to_string(),
        ),
        "UUID" => (
            "uuid",
            format!("rows.getObject(\"{column}\", UUID.class)"),
            accessor.to_string(),
        ),
        "LocalDate" => (
            "date",
            format!("rows.getObject(\"{column}\", LocalDate.class)"),
            accessor.to_string(),
        ),
        "LocalDateTime" => (
            "timestamp",
            format!("rows.getObject(\"{column}\", LocalDateTime.class)"),
            accessor.to_string(),
        ),
        // `timestamptz` and not `timestamp`: an Instant is a point on the
        // timeline, and storing one in a zone-less column silently reinterprets
        // it as local time on the way back.
        "Instant" => (
            "timestamptz",
            format!("rows.getObject(\"{column}\", OffsetDateTime.class).toInstant()"),
            format!("Timestamp.from({accessor})"),
        ),
        "URI" => (
            "text",
            format!("URI.create(rows.getString(\"{column}\"))"),
            format!("{accessor}.toString()"),
        ),
        _ => return None,
    };
    Some((sql.to_string(), read, write))
}

/// The imports the generated read/write expressions need, sorted and
/// de-duplicated. Derived from the component types rather than scraped out
/// of the expression strings, so adding a mapping cannot forget its import.
pub(crate) fn imports(columns: &[Column]) -> Vec<&'static str> {
    let mut found: Vec<&'static str> = Vec::new();
    for column in columns {
        if !column.mapped() {
            continue;
        }
        let needed: &[&'static str] = match column.java_type.as_str() {
            "UUID" => &["java.util.UUID"],
            "LocalDate" => &["java.time.LocalDate"],
            "LocalDateTime" => &["java.time.LocalDateTime"],
            "BigDecimal" => &["java.math.BigDecimal"],
            // An Instant is read through OffsetDateTime and written through
            // java.sql.Timestamp, so the mapping needs three imports, not one.
            "Instant" => &["java.time.Instant", "java.time.OffsetDateTime", "java.sql.Timestamp"],
            "URI" => &["java.net.URI"],
            _ => &[],
        };
        for import in needed {
            if !found.contains(import) {
                found.push(import);
            }
        }
    }
    found.sort_unstable();
    found
}

/// The table a type maps to: snake_case, pluralised the naive way. jails
/// pluralises by appending `s` and nothing more -- an irregular plural is a
/// judgment call, and a wrong guess in a migration is expensive to undo.
pub(crate) fn table_name(type_name: &str) -> String {
    let base = snake_case(type_name);
    if base.ends_with('s') {
        base
    } else {
        format!("{base}s")
    }
}

/// `transactionId` -> `transaction_id`. Runs of capitals stay together
/// (`customerURL` -> `customer_url`) so an acronym does not explode into
/// one underscore per letter.
pub(crate) fn snake_case(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    let mut out = String::with_capacity(name.len() + 4);
    for (i, &c) in chars.iter().enumerate() {
        if c.is_uppercase() {
            let starts_run = i > 0 && !chars[i - 1].is_uppercase();
            let ends_run = i > 0
                && chars[i - 1].is_uppercase()
                && chars.get(i + 1).is_some_and(|n| n.is_lowercase());
            if starts_run || ends_run {
                out.push('_');
            }
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// The `create table` for a scaffolded type, as a Flyway migration body.
///
/// The primary key is whichever column is named `id` if there is one, and
/// otherwise nothing -- jails will not invent a surrogate key, because a
/// record whose components are its natural key (the common case for the
/// value types jails generates) does not want one.
pub(crate) fn create_table(type_name: &str, columns: &[Column]) -> String {
    let table = table_name(type_name);
    let width = columns.iter().map(|c| c.name.len()).max().unwrap_or(0).max(4);
    let type_width = columns
        .iter()
        .map(|c| c.sql_type.len())
        .max()
        .unwrap_or(0)
        .max(4);

    let mut body = String::new();
    for column in columns {
        let null = if column.not_null { " not null" } else { "" };
        // Trimmed before the comma: a nullable column would otherwise carry
        // the padding that only exists to line `not null` up.
        let declaration = format!("{:type_width$}{null}", column.sql_type);
        body.push_str(&format!(
            "  {:width$}  {},\n",
            column.name,
            declaration.trim_end()
        ));
    }

    let key = columns.iter().find(|c| c.name == "id");
    let constraint = match key {
        Some(id) => format!("\n  constraint {table}_pk\n    primary key ({})\n", id.name),
        None => String::new(),
    };
    // Trailing comma removed only when no constraint follows it.
    let body = if constraint.is_empty() {
        body.trim_end().trim_end_matches(',').to_string() + "\n"
    } else {
        body
    };

    let unmapped: Vec<&str> = columns
        .iter()
        .filter(|c| !c.mapped())
        .map(|c| c.name.as_str())
        .collect();
    let note = if unmapped.is_empty() {
        String::new()
    } else {
        format!(
            "-- jails could not derive a column type for: {}.\n\
             -- Those are guesses; correct them before this runs anywhere real.\n",
            unmapped.join(", ")
        )
    };

    format!(
        "-- Forward-only migration, generated from the field spec.\n\
         {note}create table {table} (\n{body}{constraint});\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::parse_fields_for_test as parse;
    use std::path::PathBuf;

    fn cols(specs: &[&str]) -> Vec<Column> {
        let fields = parse(&specs.iter().map(|s| s.to_string()).collect::<Vec<_>>()).unwrap();
        columns(&fields, &PathBuf::from("/nonexistent"), "com.example", "value")
    }

    #[test]
    fn snake_case_splits_words_and_keeps_acronyms_together() {
        assert_eq!(snake_case("transactionId"), "transaction_id");
        assert_eq!(snake_case("id"), "id");
        assert_eq!(snake_case("customerURL"), "customer_url");
        assert_eq!(snake_case("URLTarget"), "url_target");
        assert_eq!(snake_case("Reward"), "reward");
    }

    #[test]
    fn table_names_pluralise_without_inventing_irregulars() {
        assert_eq!(table_name("Reward"), "rewards");
        assert_eq!(table_name("WorkItem"), "work_items");
        // Already plural: appending a second `s` would be worse than nothing.
        assert_eq!(table_name("News"), "news");
    }

    #[test]
    fn builtin_types_map_both_ways() {
        let columns = cols(&["transactionId:uuid", "amount:long", "createdAt:instant"]);
        assert!(columns.iter().all(|c| c.mapped()), "{columns:?}");
        assert_eq!(columns[0].name, "transaction_id");
        assert_eq!(columns[0].sql_type, "uuid");
        assert_eq!(columns[1].sql_type, "bigint");
        // An Instant must land in a zone-aware column, or reading it back
        // reinterprets it as local time.
        assert_eq!(columns[2].sql_type, "timestamptz");
    }

    #[test]
    fn an_optional_component_is_nullable_and_unwrapped() {
        let columns = cols(&["note:string?"]);
        assert!(!columns[0].not_null);
        assert!(columns[0].read.as_ref().unwrap().contains("Optional.ofNullable"));
        assert!(columns[0].write.as_ref().unwrap().ends_with(".orElse(null)"));
        assert!(columns[0].write.as_ref().unwrap().starts_with("value.note()"));
    }

    #[test]
    fn an_optional_primitive_is_guarded_by_a_null_column_check() {
        // getLong returns 0 for a null column, so Optional.ofNullable on its
        // result would turn a missing value into a present zero.
        let columns = cols(&["score:long?"]);
        let read = columns[0].read.as_ref().unwrap();
        assert!(read.contains("getObject(\"score\") == null"), "{read}");
        assert!(read.contains("Optional.empty()"), "{read}");
    }

    #[test]
    fn an_unmappable_type_is_reported_rather_than_guessed() {
        // SourceRef is a project type jails knows nothing about, and the
        // fixture root has no file to prove it is an enum.
        let columns = cols(&["ref:SourceRef"]);
        assert!(!columns[0].mapped());
        assert!(columns[0].read.is_none());
    }

    #[test]
    fn a_collection_is_not_one_column() {
        let columns = cols(&["tags:list<string>"]);
        assert!(!columns[0].mapped());
    }

    #[test]
    fn the_receiver_is_baked_into_the_write_expression() {
        // Not prefixed by the caller: Timestamp.from(x.createdAt()) puts the
        // receiver in the middle, and gluing it on the front produces
        // x.Timestamp.from(createdAt()), which does not compile.
        let columns = cols(&["createdAt:instant", "name:string"]);
        assert_eq!(
            columns[0].write.as_deref(),
            Some("Timestamp.from(value.createdAt())")
        );
        assert_eq!(columns[1].write.as_deref(), Some("value.name()"));
    }

    #[test]
    fn imports_cover_every_type_the_expressions_name() {
        let found = imports(&cols(&["id:uuid", "at:instant", "amount:bigdecimal"]));
        assert!(found.contains(&"java.util.UUID"), "{found:?}");
        assert!(found.contains(&"java.math.BigDecimal"), "{found:?}");
        // The Instant mapping names three types, not one.
        assert!(found.contains(&"java.time.OffsetDateTime"), "{found:?}");
        assert!(found.contains(&"java.sql.Timestamp"), "{found:?}");
        // A String needs nothing.
        assert!(imports(&cols(&["name:string"])).is_empty());
    }

    #[test]
    fn create_table_emits_a_primary_key_only_for_an_id_column() {
        let with_id = create_table("Reward", &cols(&["id:string!", "amount:long"]));
        assert!(with_id.contains("primary key (id)"), "{with_id}");
        assert!(with_id.contains("create table rewards ("), "{with_id}");
        assert!(with_id.trim_end().ends_with(");"), "{with_id}");

        // No padding left dangling in front of a nullable column's comma.
        let nullable = create_table("Reward", &cols(&["id:string!", "note:string?"]));
        assert!(nullable.contains("note  text,"), "{nullable}");
        assert!(!nullable.contains(" ,"), "{nullable}");

        let without = create_table("Reward", &cols(&["amount:long"]));
        assert!(!without.contains("primary key"), "{without}");
        // No dangling comma when nothing follows the last column.
        assert!(!without.contains(",\n)"), "{without}");
    }

    #[test]
    fn create_table_flags_the_columns_it_could_not_derive() {
        let ddl = create_table("Thing", &cols(&["id:string!", "ref:SourceRef"]));
        assert!(ddl.contains("could not derive"), "{ddl}");
        assert!(ddl.contains("ref"), "{ddl}");
    }
}
