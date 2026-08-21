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
    /// The table constraints declared on the field spec. Carried through
    /// unchanged -- `create_table` is the only reader.
    pub constraints: crate::generate::Constraints,
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
            constraints: field.constraints,
        };
    }

    let inner = inner_type(&field.java_type);
    if let Some((sql_type, read, mut write)) = builtin_mapping(&inner, &name, &accessor) {
        if optional {
            write = optional_write(&inner, &accessor);
        }
        return finish(
            name,
            sql_type,
            not_null,
            optional,
            read,
            write,
            &inner,
            field.constraints,
        );
    }

    // The one owned type with a knowable representation.
    if field.owned && crate::generate::is_enum_type(root, pkg, &inner) {
        let read = format!("{inner}.valueOf(rows.getString(\"{name}\"))");
        let write = if optional {
            format!("{accessor}.map({inner}::name).orElse(null)")
        } else {
            format!("{accessor}.name()")
        };
        return finish(
            name,
            "text".into(),
            not_null,
            optional,
            read,
            write,
            &inner,
            field.constraints,
        );
    }

    Column {
        name,
        sql_type: "text".into(),
        not_null,
        read: None,
        write: None,
        java_type: inner,
        constraints: field.constraints,
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
    constraints: crate::generate::Constraints,
) -> Column {
    if !optional {
        return Column {
            name,
            sql_type,
            not_null,
            read: Some(read),
            write: Some(write),
            java_type: inner.to_string(),
            constraints,
        };
    }
    // A null column must come back as Optional.empty(), not as an Optional
    // wrapping null -- and `getLong` on a null column returns 0, not null,
    // so a primitive read has to be guarded by wasNull() rather than by a
    // null check on its result.
    let read = if is_primitive_read(inner) {
        format!("rows.getObject(\"{name}\") == null ? Optional.empty() : Optional.of({read})")
    } else if inner == "Instant" {
        format!(
            "Optional.ofNullable(rows.getObject(\"{name}\", OffsetDateTime.class)).map(OffsetDateTime::toInstant)"
        )
    } else if inner == "URI" {
        format!("Optional.ofNullable(rows.getString(\"{name}\")).map(URI::create)")
    } else if read.starts_with(&format!("{inner}.valueOf(")) {
        format!("Optional.ofNullable(rows.getString(\"{name}\")).map({inner}::valueOf)")
    } else {
        format!("Optional.ofNullable({read})")
    };
    Column {
        name,
        sql_type,
        not_null: false,
        read: Some(read),
        write: Some(write),
        java_type: inner.to_string(),
        constraints,
    }
}

/// A nullable record component is an `Optional<T>`. Transformations must
/// happen *inside* that Optional: `Timestamp.from(optional)` does not compile,
/// and `optional.orElse(null).toString()` throws on the empty case.
fn optional_write(inner: &str, accessor: &str) -> String {
    match inner {
        "Instant" => format!("{accessor}.map(Timestamp::from).orElse(null)"),
        "URI" => format!("{accessor}.map(URI::toString).orElse(null)"),
        _ => format!("{accessor}.orElse(null)"),
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
        "String" => (
            "text",
            format!("rows.getString(\"{column}\")"),
            accessor.to_string(),
        ),
        "Integer" | "int" => (
            "integer",
            format!("rows.getInt(\"{column}\")"),
            accessor.to_string(),
        ),
        "Long" | "long" => (
            "bigint",
            format!("rows.getLong(\"{column}\")"),
            accessor.to_string(),
        ),
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
            "Instant" => &[
                "java.time.Instant",
                "java.time.OffsetDateTime",
                "java.sql.Timestamp",
            ],
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
/// The primary key is whichever columns are marked `@pk`, in declaration
/// order, so a composite key is just several of them. Failing that it is a
/// column named `id`, and failing *that* it is nothing -- jails will not
/// invent a surrogate key, because a record whose components are its natural
/// key (the common case for the value types jails generates) does not want
/// one.
///
/// `@unique`, `@positive`/`@nonnegative` and `@index` come from the same
/// field spec, which is the point: a generated schema that cannot express
/// the constraints the table actually has gets hand-edited the moment it is
/// written, and then the field spec and the schema disagree forever.
///
/// `extra_indexes` carries what a per-column marker cannot: a composite or
/// ordered index (`customer_id, created_at desc`). Passed through as written
/// after its column names are checked against the table, because index
/// ordering is a real schema decision with no shorthand worth inventing.
pub(crate) fn create_table(
    type_name: &str,
    columns: &[Column],
    extra_indexes: &[String],
) -> String {
    let table = table_name(type_name);
    let width = columns
        .iter()
        .map(|c| c.name.len())
        .max()
        .unwrap_or(0)
        .max(4);
    let type_width = columns
        .iter()
        .map(|c| c.sql_type.len())
        .max()
        .unwrap_or(0)
        .max(4);

    let mut body = String::new();
    for column in columns {
        let null = if column.not_null { " not null" } else { "" };
        let check = match column.constraints.check {
            Some(check) => format!(" check ({})", check.predicate(&column.name)),
            None => String::new(),
        };
        let unique = if column.constraints.unique {
            " unique"
        } else {
            ""
        };
        // Trimmed before the comma: a nullable column would otherwise carry
        // the padding that only exists to line `not null` up.
        let declaration = format!("{:type_width$}{null}{unique}{check}", column.sql_type);
        body.push_str(&format!(
            "  {:width$}  {},\n",
            column.name,
            declaration.trim_end()
        ));
    }

    let marked: Vec<&Column> = columns
        .iter()
        .filter(|c| c.constraints.primary_key)
        .collect();
    let key_columns: Vec<&str> = if marked.is_empty() {
        columns
            .iter()
            .filter(|c| c.name == "id")
            .map(|c| c.name.as_str())
            .collect()
    } else {
        marked.iter().map(|c| c.name.as_str()).collect()
    };
    let constraint = if key_columns.is_empty() {
        String::new()
    } else {
        format!(
            "\n  constraint {table}_pk\n    primary key ({})\n",
            key_columns.join(", ")
        )
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

    // Every statement is terminated. An unterminated `create table` followed
    // by a `create index` is one unparseable statement, and Flyway reports it
    // as a syntax error somewhere in the middle of the file.
    let mut out = format!(
        "-- Forward-only migration, generated from the field spec.\n\
         {note}create table {table} (\n{body}{constraint});\n"
    );

    for column in columns.iter().filter(|c| c.constraints.indexed) {
        out.push_str(&format!(
            "\ncreate index {table}_{}_idx\n  on {table} ({});\n",
            column.name, column.name
        ));
    }
    for (n, spec) in extra_indexes.iter().enumerate() {
        // Named by position rather than by content: an index over
        // `created_at desc` cannot go in an identifier, and a name derived by
        // stripping the ordering would collide with the plain one.
        out.push_str(&format!(
            "\ncreate index {table}_idx{}\n  on {table} ({});\n",
            n + 1,
            spec.trim()
        ));
    }
    out
}

/// Check an `--index` spec against the table before it is written into a
/// migration.
///
/// A typo here fails at `flyway migrate` with "column does not exist", which
/// is a slow way to find out and happens on whichever machine runs it first.
pub(crate) fn validate_index(spec: &str, columns: &[Column]) -> Result<(), String> {
    let known: Vec<&str> = columns.iter().map(|c| c.name.as_str()).collect();
    for part in spec.split(',') {
        // `created_at desc` -- the column is the first word, the rest is
        // ordering that Postgres parses and jails does not.
        let column = part.trim().split_whitespace().next().unwrap_or("");
        if column.is_empty() {
            return Err(format!("--index '{spec}': empty column name"));
        }
        if !known.contains(&column) {
            return Err(format!(
                "--index '{spec}': no column '{column}' in this table. Columns: {}",
                known.join(", ")
            ));
        }
    }
    Ok(())
}

/// Two rows of sample data for `src/test/resources/fixtures`, keyed by the
/// same column names as the table and the adapter.
///
/// Rails writes a fixture file for every model it generates, and the reason
/// it earns its place is that the alternative is a test that builds its own
/// sample inline -- which every test then does slightly differently. Two
/// rows rather than one: a single row cannot catch an ordering bug or a
/// `findAll` that returns only the first result.
///
/// The keys are the *column* names, not the component names, so the fixture
/// lines up with what the database actually holds -- which is the point of
/// having it next to a JDBC adapter rather than a Java builder.
pub(crate) fn fixture_json(
    columns: &[Column],
    enum_constant: &dyn Fn(&str) -> Option<String>,
) -> String {
    let rows: Vec<String> = (1..=2)
        .map(|row| {
            let fields: Vec<String> = columns
                .iter()
                .map(|column| {
                    format!(
                        "    \"{}\": {}",
                        column.name,
                        sample_value(column, row, enum_constant)
                    )
                })
                .collect();
            format!("  {{\n{}\n  }}", fields.join(",\n"))
        })
        .collect();
    format!("[\n{}\n]\n", rows.join(",\n"))
}

/// A JSON sample for one column. `row` is 1 or 2, so the two rows differ --
/// two identical rows would let a `findAll` that returns one of them pass.
fn sample_value(
    column: &Column,
    row: u8,
    enum_constant: &dyn Fn(&str) -> Option<String>,
) -> String {
    // A nullable column is null in the second row: the shape most likely to
    // break a mapper is the absent one, so the fixture should contain it.
    if !column.not_null && row == 2 {
        return "null".to_string();
    }
    match column.java_type.as_str() {
        "String" => format!("\"sample-{row}\""),
        "Integer" | "int" | "Long" | "long" => row.to_string(),
        "Double" | "double" => format!("{row}.5"),
        // A number, not a string: this is what a JSON body would carry, and
        // rounding it through a float is the bug BigDecimal exists to avoid.
        "BigDecimal" => format!("{row}.00"),
        "Boolean" | "boolean" => (row == 1).to_string(),
        "UUID" => format!("\"00000000-0000-0000-0000-00000000000{row}\""),
        "Instant" => format!("\"2024-01-0{row}T00:00:00Z\""),
        "LocalDate" => format!("\"2024-01-0{row}\""),
        "LocalDateTime" => format!("\"2024-01-0{row}T12:00:00\""),
        "URI" => format!("\"https://example.test/{row}\""),
        other => match enum_constant(other) {
            Some(constant) => format!("\"{constant}\""),
            // A type jails cannot sample: null is honest, and the reader
            // sees immediately which field needs a real value.
            None => "null".to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::parse_fields_for_test as parse;
    use std::path::PathBuf;

    fn cols(specs: &[&str]) -> Vec<Column> {
        let fields = parse(&specs.iter().map(|s| s.to_string()).collect::<Vec<_>>()).unwrap();
        columns(
            &fields,
            &PathBuf::from("/nonexistent"),
            "com.example",
            "value",
        )
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
        assert!(
            columns[0]
                .read
                .as_ref()
                .unwrap()
                .contains("Optional.ofNullable")
        );
        assert!(
            columns[0]
                .write
                .as_ref()
                .unwrap()
                .ends_with(".orElse(null)")
        );
        assert!(
            columns[0]
                .write
                .as_ref()
                .unwrap()
                .starts_with("value.note()")
        );
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
    fn optional_transformed_types_map_before_unwrapping() {
        let columns = cols(&["finishedAt:instant?", "callback:uri?"]);
        assert_eq!(
            columns[0].read.as_deref(),
            Some(
                "Optional.ofNullable(rows.getObject(\"finished_at\", OffsetDateTime.class)).map(OffsetDateTime::toInstant)"
            )
        );
        assert_eq!(
            columns[1].read.as_deref(),
            Some("Optional.ofNullable(rows.getString(\"callback\")).map(URI::create)")
        );
        assert_eq!(
            columns[0].write.as_deref(),
            Some("value.finishedAt().map(Timestamp::from).orElse(null)")
        );
        assert_eq!(
            columns[1].write.as_deref(),
            Some("value.callback().map(URI::toString).orElse(null)")
        );
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
    fn a_fixture_has_two_rows_keyed_by_column_name() {
        let json = fixture_json(&cols(&["transactionId:uuid", "amount:long"]), &|_| None);
        assert!(json.contains("\"transaction_id\""), "{json}");
        assert!(!json.contains("transactionId"), "camelCase leaked: {json}");
        // Two rows, and they differ -- one row cannot catch an ordering bug.
        assert_eq!(json.matches("transaction_id").count(), 2, "{json}");
        assert!(
            json.contains("...1\"") || json.contains("00000001"),
            "{json}"
        );
        assert!(json.contains("00000002"), "{json}");
    }

    #[test]
    fn a_nullable_column_is_null_in_the_second_row() {
        let json = fixture_json(&cols(&["id:string!", "note:string?"]), &|_| None);
        assert!(json.contains("\"note\": \"sample-1\""), "{json}");
        assert!(json.contains("\"note\": null"), "{json}");
    }

    #[test]
    fn an_enum_column_uses_a_real_constant_when_one_can_be_read() {
        let json = fixture_json(&cols(&["currency:Currency"]), &|t| {
            (t == "Currency").then(|| "GBP".to_string())
        });
        assert!(json.contains("\"currency\": \"GBP\""), "{json}");
        // And null when the constant cannot be read, rather than a guess.
        let unknown = fixture_json(&cols(&["ref:SourceRef"]), &|_| None);
        assert!(unknown.contains("\"ref\": null"), "{unknown}");
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
        let with_id = create_table("Reward", &cols(&["id:string!", "amount:long"]), &[]);
        assert!(with_id.contains("primary key (id)"), "{with_id}");
        assert!(with_id.contains("create table rewards ("), "{with_id}");
        assert!(with_id.trim_end().ends_with(");"), "{with_id}");

        // No padding left dangling in front of a nullable column's comma.
        let nullable = create_table("Reward", &cols(&["id:string!", "note:string?"]), &[]);
        assert!(nullable.contains("note  text,"), "{nullable}");
        assert!(!nullable.contains(" ,"), "{nullable}");

        let without = create_table("Reward", &cols(&["amount:long"]), &[]);
        assert!(!without.contains("primary key"), "{without}");
        // No dangling comma when nothing follows the last column.
        assert!(!without.contains(",\n)"), "{without}");
    }

    /// The migration `~/code/bank/rewards` had to hand-edit after generation:
    /// a composite primary key, a positivity check, and a covering index.
    /// None of it was expressible, so the generated schema was rewritten by
    /// hand the moment it was written.
    #[test]
    fn the_field_spec_can_express_the_constraints_a_real_table_needed() {
        let columns = cols(&[
            "transactionId:uuid@pk",
            "ruleId:string@pk",
            "amount:long@positive",
            "customerId:uuid",
        ]);
        let ddl = create_table(
            "Reward",
            &columns,
            &["customer_id, created_at desc".to_string()],
        );

        assert!(
            ddl.contains("primary key (transaction_id, rule_id)"),
            "composite key, in declaration order: {ddl}"
        );
        assert!(ddl.contains("check (amount > 0)"), "{ddl}");
        assert!(
            ddl.contains("on rewards (customer_id, created_at desc)"),
            "an ordered index is passed through as written: {ddl}"
        );
    }

    /// F2's original complaint: the generated file was one unparseable
    /// statement because nothing was terminated.
    #[test]
    fn every_generated_statement_is_terminated() {
        let ddl = create_table(
            "Reward",
            &cols(&["id:string!", "customerId:uuid@index"]),
            &["id, customer_id".to_string()],
        );
        let statements = ddl
            .lines()
            .filter(|l| l.trim_start().starts_with("create "))
            .count();
        assert_eq!(
            statements, 3,
            "table + column index + explicit index: {ddl}"
        );
        assert_eq!(ddl.matches(';').count(), 3, "each one terminated: {ddl}");
    }

    #[test]
    fn a_unique_column_says_so_in_its_declaration() {
        let ddl = create_table("Reward", &cols(&["email:string@unique"]), &[]);
        assert!(ddl.contains("unique"), "{ddl}");
    }

    /// An `id` column is still the default key, so nothing that worked before
    /// this feature changes.
    #[test]
    fn an_id_column_is_still_the_key_when_nothing_is_marked() {
        let ddl = create_table("Reward", &cols(&["id:string!", "amount:long"]), &[]);
        assert!(ddl.contains("primary key (id)"), "{ddl}");
    }

    /// A typo in `--index` would otherwise surface at `flyway migrate` as
    /// "column does not exist", on whichever machine ran it first.
    #[test]
    fn an_index_naming_a_column_that_does_not_exist_is_rejected() {
        let columns = cols(&["id:string!", "customerId:uuid"]);
        assert!(validate_index("customer_id, created_at desc", &columns).is_err());
        assert!(validate_index("customer_id", &columns).is_ok());
        // The ordering keyword is not mistaken for a column.
        assert!(validate_index("customer_id desc, id", &columns).is_ok());
    }

    #[test]
    fn create_table_flags_the_columns_it_could_not_derive() {
        let ddl = create_table("Thing", &cols(&["id:string!", "ref:SourceRef"]), &[]);
        assert!(ddl.contains("could not derive"), "{ddl}");
        assert!(ddl.contains("ref"), "{ddl}");
    }
}
