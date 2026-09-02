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

mod ddl;
pub use ddl::*;

use crate::generate::{Field, Optionality};

/// How one record component crosses the JDBC boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Column {
    /// The column name: the component name in snake_case.
    pub name: String,
    /// The Java component this column binds to, as the record declares it.
    ///
    /// Carried because [`Self::write`] bakes the receiver in -- it is
    /// `Timestamp.from(x.at())`, not an accessor -- so a caller that needs to
    /// name the component itself (rebuilding a record around a
    /// database-assigned key, say) had nothing to read. plan.md P4.2.
    pub component: String,
    /// The column type for a `create table`, in the project's dialect.
    ///
    /// Postgres unless the project's driver says otherwise -- see
    /// `Project::sql_dialect`. Derived once, in `columns`, so the DDL and
    /// `add_column` cannot spell one column two ways.
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
    /// The dialect this column's type was resolved in.
    ///
    /// Carried beside `sql_type` because the dialect decides more than the
    /// type name and every one of those decisions is made by a DDL emitter
    /// that has only the column: a unique index on `lower(email)` is a
    /// PostgreSQL expression index and a **syntax error** in H2, which has
    /// none. Setting it here, once, is what stops each emitter needing the
    /// project threaded through it.
    pub dialect: jails_spec::spec::kind::Dialect,
    /// The table constraints declared on the field spec. Carried through
    /// unchanged -- `create_table` is the only reader.
    pub constraints: crate::generate::Constraints,
    /// True when the field spec said `!`: the Java constructor rejects a
    /// blank value, so the column should too.
    ///
    /// plan.md P5.4, modern.md §4.7: the constructor enforced it and the
    /// database did not, so any import path -- a `copy`, a backfill, another
    /// service -- put a blank row in a column the application believes cannot
    /// hold one.
    pub non_blank: bool,
    /// The constants of the project enum this column stores, if it stores one.
    ///
    /// **The closed set, carried into the schema.** plan.md P5.1: the reader
    /// declared `g enum`, jails generated the Java enum and the column, jails
    /// held the constant list -- and still wrote a `text` column that accepts
    /// `'banana'`. Zero `check (` appeared in twenty migrations across seven
    /// real projects, and nothing was missing except the emit.
    pub closed_set: Vec<String>,
}

impl Column {
    /// True when jails knows how to move this column both ways.
    pub fn mapped(&self) -> bool {
        self.read.is_some() && self.write.is_some()
    }
}

/// Who decides a new row's primary key.
///
/// plan.md P4.2. There was no policy, so every generated create named the key
/// itself: `usecase_default` handed `0L` to every create over an integer key,
/// in every project, which means the primary create path works exactly once
/// and then violates the primary key. Naming the three answers is what lets
/// `create_table`, the adapters, the in-memory fake and the use case agree on
/// one of them.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Assignment {
    /// The caller supplies it -- the key is a component of the create command.
    ClientSupplied,
    /// The application writes it before the insert: `UUID.randomUUID()`.
    ServerGenerated,
    /// The database assigns it. The column is `generated always as identity`,
    /// the insert omits it, and `returning` carries the row back.
    DatabaseGenerated,
}

/// The policy this table's own key follows, read off its declared columns.
///
/// **Derived from the key's type, not configured**, because the two answers
/// are not preferences: an application can write a UUID and cannot write a
/// unique integer without asking the database, and a database can assign an
/// integer and has no business inventing a UUID the application already knows
/// how to make. [`Assignment::ClientSupplied`] is the one a *use case* adds,
/// since only a command can say the caller supplied it.
pub fn key_assignment(columns: &[Column]) -> Assignment {
    // A composite key has no single assigned component, so `key_column`
    // answers `None` and nothing here assigns one.
    let key = key_column(columns);
    match key {
        // **Named `id`, not merely marked `@pk`.** A key called something
        // else is a natural one the caller chose -- an order `reference`, a
        // country `code` -- and assigning it would overwrite the value that
        // is the whole point of declaring it. That is the convention
        // `usecase_default` has always used for a `String` key; it is stated
        // once here now.
        // A `@scope` key is proved against the caller's own token, so the
        // caller is precisely who supplies it.
        Some(key) if key.constraints.scoped => Assignment::ClientSupplied,
        Some(key) if key.component != "id" => Assignment::ClientSupplied,
        Some(key) if is_integer_key(&key.java_type) => Assignment::DatabaseGenerated,
        Some(_) => Assignment::ServerGenerated,
        None => Assignment::ClientSupplied,
    }
}

/// The column this table's key is, whoever assigns it.
pub(crate) fn key_column(columns: &[Column]) -> Option<&Column> {
    let declared: Vec<&Column> = columns
        .iter()
        .filter(|column| column.constraints.primary_key)
        .collect();
    match declared.as_slice() {
        [key] => Some(*key),
        [_, _, ..] => None,
        [] => columns.iter().find(|column| column.name == "id"),
    }
}

/// The Java types a database identity column can carry. `smallint` is
/// deliberately absent: a table whose key runs out at 32,767 is a bug waiting
/// rather than a design.
fn is_integer_key(java_type: &str) -> bool {
    matches!(java_type, "int" | "Integer" | "long" | "Long")
}

/// Derive a column for every component. The project and `pkg` are needed to recognise
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
pub fn columns(
    fields: &[Field],
    project: &crate::model::Project,
    pkg: &str,
    receiver: &str,
) -> Vec<Column> {
    let dialect = project.sql_dialect();
    fields
        .iter()
        .map(|field| {
            let mut column = column(field, project, pkg, receiver);
            // Translated here rather than in `create_table`, because this is
            // where the type is *decided*. A dialect applied at the DDL would
            // leave `Column.sql_type` saying one thing and the schema saying
            // another, and `add_column` -- a second reader of the same value
            // -- would have to remember to apply it too.
            column.sql_type = dialect.column_type(&column.sql_type).to_string();
            column.dialect = dialect;
            column
        })
        .collect()
}

fn column(field: &Field, project: &crate::model::Project, pkg: &str, receiver: &str) -> Column {
    // Read, not re-derived. plan.md P3.2: after a `--column preserve` rename
    // the two halves no longer agree, and snake-casing the Java name here
    // would quietly contradict the binding the ledger records.
    let name = field.column.clone();
    let component = field.name.clone();
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
            dialect: jails_spec::spec::kind::Dialect::Postgres,
            name,
            component,
            sql_type: "jsonb".into(),
            not_null,
            read: None,
            write: None,
            java_type: field.java_type.clone(),
            constraints: field.constraints,
            closed_set: Vec::new(),
            non_blank: field.optionality == Optionality::NonBlank,
        };
    }

    let inner = inner_type(&field.java_type);
    if let Some((sql_type, read, mut write)) = builtin_mapping(&inner, &name, &accessor) {
        if optional {
            write = optional_write(&inner, &accessor);
        }
        return finish(
            Column {
                dialect: jails_spec::spec::kind::Dialect::Postgres,
                name,
                component,
                sql_type,
                not_null,
                read: Some(read),
                write: Some(write),
                java_type: inner.clone(),
                constraints: field.constraints,
                closed_set: Vec::new(),
                non_blank: field.optionality == Optionality::NonBlank,
            },
            optional,
        );
    }

    // The one owned type with a knowable representation -- and the one whose
    // *values* jails also knows, which is what the schema gets to say.
    if field.owned && project.declares_enum(pkg, &inner) {
        let read = format!("{inner}.valueOf(rows.getString(\"{name}\"))");
        let write = if optional {
            format!("{accessor}.map({inner}::name).orElse(null)")
        } else {
            format!("{accessor}.name()")
        };
        return finish(
            Column {
                dialect: jails_spec::spec::kind::Dialect::Postgres,
                name,
                component,
                sql_type: "text".into(),
                not_null,
                read: Some(read),
                write: Some(write),
                java_type: inner.clone(),
                constraints: field.constraints,
                non_blank: field.optionality == Optionality::NonBlank,
                closed_set: crate::generate::enum_constants(project, pkg, &inner)
                    .unwrap_or_default(),
            },
            optional,
        );
    }

    Column {
        dialect: jails_spec::spec::kind::Dialect::Postgres,
        name,
        component,
        sql_type: "text".into(),
        not_null,
        read: None,
        write: None,
        java_type: inner,
        constraints: field.constraints,
        closed_set: Vec::new(),
        non_blank: field.optionality == Optionality::NonBlank,
    }
}

/// Wrap the mapping for an `Optional` component. The column stays the same;
/// only the two expressions change.
///
/// It takes the finished non-optional column rather than the seven values it
/// is made of, which is what it was: the caller already knows every one of
/// them, and passing them separately meant the two branches could disagree
/// about which column they were describing.
fn finish(column: Column, optional: bool) -> Column {
    if !optional {
        return column;
    }
    let Column {
        name,
        component,
        sql_type,
        read,
        write,
        java_type: inner,
        constraints,
        closed_set,
        non_blank,
        ..
    } = column;
    let read = read.expect("a mapped column carries its read expression");
    let write = write.expect("a mapped column carries its write expression");
    let inner = inner.as_str();
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
        dialect: jails_spec::spec::kind::Dialect::Postgres,
        name,
        component,
        sql_type,
        not_null: false,
        read: Some(read),
        write: Some(write),
        java_type: inner.to_string(),
        constraints,
        closed_set,
        non_blank,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::parse_fields as parse;
    use std::path::PathBuf;

    fn cols(specs: &[&str]) -> Vec<Column> {
        let fields = parse(&specs.iter().map(|s| s.to_string()).collect::<Vec<_>>()).unwrap();
        // A project that does not exist: these cases are pure spec -> column
        // mapping, and the only thing the project is asked is whether a type
        // is an enum, which needs a file that is deliberately absent here.
        let project = crate::model::Project::inspect(&PathBuf::from("/nonexistent")).unwrap();
        columns(&fields, &project, "com.example", "value")
    }

    /// The one type name H2 does not take, and the reason a dialect exists.
    ///
    /// `timestamptz` is a Postgres spelling. H2 knows it only inside its
    /// PostgreSQL wire-protocol server, so a `create table` using it over JDBC
    /// fails to parse -- verified against H2's own type table in
    /// `deps/h2database/h2/src/main/org/h2/value/DataType.java`. Everything
    /// else jails emits is in that table verbatim, which this asserts too:
    /// a dialect that quietly started rewriting more than it had to would be
    /// changing a schema people read.
    #[test]
    fn only_the_timestamptz_spelling_differs_between_the_two_dialects() {
        use jails_spec::spec::kind::Dialect;
        assert_eq!(
            Dialect::H2.column_type("timestamptz"),
            "timestamp with time zone"
        );
        for shared in [
            "text",
            "integer",
            "bigint",
            "boolean",
            "double precision",
            "numeric",
            "uuid",
            "date",
            "timestamp",
        ] {
            assert_eq!(Dialect::H2.column_type(shared), shared);
            assert_eq!(Dialect::Postgres.column_type(shared), shared);
        }
        assert_eq!(Dialect::Postgres.column_type("timestamptz"), "timestamptz");
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
    fn table_names_pluralise_regular_suffixes_without_inventing_irregulars() {
        assert_eq!(table_name("Reward"), "rewards");
        assert_eq!(table_name("WorkItem"), "work_items");
        assert_eq!(table_name("Box"), "boxes");
        assert_eq!(table_name("Address"), "addresses");
        assert_eq!(table_name("Category"), "categories");
        assert_eq!(table_name("Toy"), "toys");
        // Already plural: appending a second `s` would be worse than nothing.
        assert_eq!(table_name("News"), "news");
        assert_eq!(table_name("Shelf"), "shelves");
        assert_eq!(table_name("Knife"), "knives");
        // `ff` is not the `f -> ves` case: staffs, cliffs, not stayves.
        assert_eq!(table_name("Cliff"), "cliffs");
    }

    #[test]
    fn the_short_irregular_list_covers_what_a_schema_actually_contains() {
        assert_eq!(table_name("Person"), "people");
        assert_eq!(table_name("Child"), "children");
        // The last word decides, so a compound gets it right too.
        assert_eq!(table_name("SupportPerson"), "support_people");
        // Uncountable: `equipments` is not a word.
        assert_eq!(table_name("Equipment"), "equipment");
        assert_eq!(table_name("Metadata"), "metadata");
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
}
