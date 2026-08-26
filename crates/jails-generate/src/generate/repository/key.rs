//! **What a stored row is identified by**, as every generated file has to
//! spell it.
//!
//! Split out of `repository.rs` under `abstract.md` rung 11: the parent's
//! secret is *how a repository reaches the database*, and this one's is *what
//! its key is* -- a different question, asked by files that are not adapters
//! at all. The port declares the parameter type, the controller declares the
//! path variable, the in-memory fake declares the map key, the service
//! forwards it, and five templates in other kinds pass a value to it. Every
//! one of those has to agree, and none of them can see the others, so the
//! answer is derived once here and handed down.

use super::*;

/// The Java type a repository port is keyed on, and the import it costs.
///
/// **Derived once and passed down**, never recomputed per template. plan.md
/// P3.3: eleven of twelve generated ports declared `findById(String)` over a
/// `UUID` primary key, and in one real project two ports over two tables in
/// one application disagreed about it -- so every caller had to spell
/// `String.valueOf(x.id())` and the compiler could not say when that was
/// wrong.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct KeyType {
    /// The parameter type, **boxed**: a `Map<K, V>` and an `Optional<K>` need
    /// a reference type, and `long@pk` would otherwise produce neither.
    pub(crate) java: String,
    /// `import java.util.UUID;\n`, or empty. Rendered rather than returned as
    /// a name because every template splices it into an import block.
    pub(crate) import: String,
    /// Two distinct key values, for the generated tests that have to say
    /// "this one is there and that one is not".
    ///
    /// **They are part of the type, not a lookup beside it**, because a key
    /// type with no way to write one down produces a test that does not
    /// compile -- so [`key_type`] declines to leave `String` at all unless it
    /// can also supply these.
    pub(crate) samples: (String, String),
}

impl KeyType {
    /// The fallback, and the one honest answer when jails cannot see the key:
    /// `g repo` on a type it has never had a field spec for, or a key column
    /// whose type it has no mapping for.
    fn opaque() -> Self {
        Self {
            java: "String".to_string(),
            import: String::new(),
            samples: (
                sample_literal("String").to_string(),
                alternate_sample_literal("String")
                    .expect("String has an alternate sample")
                    .to_string(),
            ),
        }
    }

    /// True when the key is the historical untyped one, so a caller that has
    /// to render a conversion knows it still needs one.
    pub(crate) fn is_opaque(&self) -> bool {
        self.java == "String"
    }
}

/// Which type the repository's `findById` takes, read off the same columns
/// [`repository_key`] picks the key column from -- so the port's signature
/// and the adapter's `where` clause cannot disagree about which component is
/// the identity.
pub(crate) fn key_type(columns: &[crate::sql::Column]) -> KeyType {
    let RepositoryKey::Single(Some(column)) = repository_key(columns) else {
        // Composite. The port cannot represent it and says so; a key type
        // would be a second, quieter lie beside the explicit one.
        return KeyType::opaque();
    };
    if !column.mapped() {
        return KeyType::opaque();
    }
    match builtin_by_java_name(&column.java_type) {
        Some((boxed, import)) => {
            let Some(alternate) = alternate_sample_literal(boxed) else {
                // A type jails cannot write two distinct values of has no
                // usable generated test, and a port typed on it would produce
                // one that does not compile. Staying opaque is the honest
                // trade.
                return KeyType::opaque();
            };
            KeyType {
                java: boxed.to_string(),
                import: import
                    .map(|import| format!("import {import};\n"))
                    .unwrap_or_default(),
                samples: (sample_literal(boxed).to_string(), alternate.to_string()),
            }
        }
        // An owned type -- a project enum, say. Its identity is a value jails
        // knows nothing about beyond the name it is stored under, so the port
        // keeps the text form rather than naming a type it cannot construct.
        None => KeyType::opaque(),
    }
}

/// A second sample of a key type, distinct from [`sample_literal`]'s.
///
/// **Deliberately narrower than [`builtin_by_java_name`]'s table.** A typed
/// port is not free: the same value has to survive a `@PathVariable`, a
/// `Map` key and a JDBC parameter, and it has to be writable twice so a test
/// can say "this one is there and that one is not". These six do all of that.
/// A `Duration`, a `URI` or a `Path` primary key would reach a URL path
/// segment and lose, and `boolean`/`double` are not identities -- so those
/// keep the text port they have today rather than getting a typed one that
/// fails at the edge.
fn alternate_sample_literal(java_type: &str) -> Option<&'static str> {
    Some(match java_type {
        "String" => "\"other\"",
        "Integer" | "int" => "2",
        "Long" | "long" => "2L",
        "UUID" => "UUID.fromString(\"00000000-0000-0000-0000-000000000002\")",
        "LocalDate" => "LocalDate.of(2024, 1, 2)",
        "Instant" => "Instant.parse(\"2024-01-02T00:00:00Z\")",
        _ => return None,
    })
}

/// Everything the in-memory adapter has to know about a resource's key.
///
/// Four values computed together and consumed together -- which component the
/// map is keyed on, what type that is, whether the storage layer assigns it,
/// and what the record looks like rebuilt around an assigned one. They
/// arrived as four positional parameters and took `in_memory_repository_java`
/// past the seven-argument line the lint draws; `abstract.md` rung 1 calls
/// that a parameter object, and this is one.
pub(crate) struct StoredKey<'a> {
    pub(crate) component: Option<&'a Field>,
    pub(crate) key_type: KeyType,
    /// True when the storage layer assigns the key, so a fake of this port
    /// has to assign it too.
    pub(crate) assigned: bool,
    /// The record rebuilt around the assigned key, or `None` when nothing
    /// assigns one.
    pub(crate) rebuilt: Option<String>,
}

impl<'a> StoredKey<'a> {
    pub(crate) fn of(fields: &'a [Field], columns: &[crate::sql::Column], name: &str) -> Self {
        Self {
            component: key_component(fields, columns),
            key_type: key_type(columns),
            assigned: crate::sql::key_assignment(columns)
                == crate::sql::Assignment::DatabaseGenerated,
            rebuilt: rebuilt_record(
                name,
                &lower_first(name),
                columns,
                "assigned",
                "                ",
            ),
        }
    }
}

/// The record rebuilt around a key the storage layer assigned.
///
/// `None` when the database does not assign this table's key, which is also
/// the signal that a caller may return its argument unchanged. plan.md P4.2.
pub(crate) fn rebuilt_record(
    name: &str,
    var: &str,
    columns: &[crate::sql::Column],
    key_expression: &str,
    indent: &str,
) -> Option<String> {
    let key = crate::sql::generated_key(columns)?.name.clone();
    let arguments = columns
        .iter()
        .map(|column| {
            let value = if column.name == key {
                key_expression.to_string()
            } else {
                format!("{var}.{}()", column.component)
            };
            format!("{indent}{value}")
        })
        .collect::<Vec<_>>()
        .join(",\n");
    Some(format!("new {name}(\n{arguments})"))
}

/// The boxed spelling of a key type, for `keys.getKeyAs(Long.class)`.
///
/// `long.class` is a legal expression and the wrong one: `getKeyAs` takes a
/// `Class<T>` and `T` cannot be a primitive.
pub(super) fn boxed_key(java_type: &str) -> &'static str {
    match java_type {
        "int" | "Integer" => "Integer",
        _ => "Long",
    }
}

/// The component a repository is keyed on, as the field it came from.
///
/// `sql::columns` derives one column per field in declaration order, so the
/// index that picks the key column out of one picks the key component out of
/// the other. This is what lets the in-memory adapter key on the same thing
/// the JDBC adapter's `where` clause does -- they used to disagree, the
/// in-memory one keying on `String.valueOf(x.id())` or, with no `id`
/// component at all, on a collision-prone counter while its Javadoc claimed
/// otherwise. plan.md P3.3.
pub(crate) fn key_component<'a>(
    fields: &'a [Field],
    columns: &[crate::sql::Column],
) -> Option<&'a Field> {
    let RepositoryKey::Single(Some(key)) = repository_key(columns) else {
        return None;
    };
    let index = columns
        .iter()
        .position(|column| column.name == key.name)
        .expect("the selected repository key came from this column slice");
    fields.get(index)
}

/// The same key type, asked about a field list rather than a column list.
///
/// The renderers for `usecase`, `transition`, `durable-job` and the outbox
/// all call a scaffolded resource's port and have the target's *fields*, not
/// its columns. Deriving the columns here rather than letting each of them
/// guess is what keeps their `findById` argument in step with the port's
/// parameter -- five templates spelled `String.valueOf(x.id())` because the
/// port was untyped, and any one of them left behind would be a compile
/// error in a freshly generated project. plan.md P3.3.
pub(crate) fn key_type_of(
    fields: &[Field],
    project: &crate::model::Project,
    domain: &str,
) -> KeyType {
    key_type(&crate::sql::columns(fields, project, domain, "value"))
}

/// How a caller that holds the record hands its identity to the port.
///
/// One helper rather than a conditional at each site: an opaque port still
/// takes text, and the conversion has to appear exactly where it is needed
/// and nowhere else.
pub(crate) fn key_argument(expression: &str, key: &KeyType) -> String {
    if key.is_opaque() {
        format!("String.valueOf({expression})")
    } else {
        expression.to_string()
    }
}
#[derive(Clone, Copy)]
pub(super) enum RepositoryKey<'a> {
    Single(Option<&'a crate::sql::Column>),
    Composite,
}

/// The key exposed through the repository port.
///
/// A declared single-column primary key wins over naming convention. With no
/// declaration, `id` and then the first mapped component retain the historical
/// fallback. A composite key is deliberately not collapsed to one component:
/// the current `String id` port cannot represent it without lying.
pub(super) fn repository_key(columns: &[crate::sql::Column]) -> RepositoryKey<'_> {
    let declared = columns
        .iter()
        .filter(|column| column.constraints.primary_key)
        .collect::<Vec<_>>();
    match declared.as_slice() {
        [key] => RepositoryKey::Single(Some(*key)),
        [_, _, ..] => RepositoryKey::Composite,
        [] => RepositoryKey::Single(
            columns
                .iter()
                .filter(|column| column.mapped())
                .find(|column| column.name == "id")
                .or_else(|| columns.iter().find(|column| column.mapped())),
        ),
    }
}
