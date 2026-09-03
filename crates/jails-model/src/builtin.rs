//! One row per builtin type, and every projection of it read from that row.
//!
//! Sixteen builtins each have several projections -- the token a declaration
//! writes, the aliases it normalises from, the Java type, the import that
//! type needs, the sample a factory emits, the Postgres column -- and separate
//! matches for each would be exhaustive over the enum and silent about the
//! others, so a builtin missing from one is never a compile error: the model
//! accepts it, the emitter writes a field of it, and the DDL has no column
//! for it.
//!
//! - **[`BuiltinType::semantics`] is an exhaustive match**, so a variant added
//!   to the enum does not compile until its row exists.
//! - **A projection is a field read, never a second match.** Anything that
//!   needs to know something about a builtin asks the row.
//! - **The token is the canonical spelling**, and [`BuiltinSemantics::aliases`]
//!   are the spellings a declaration may use instead. Both live on the row, so
//!   normalising and canonicalising cannot disagree about which is which.

/// How a literal default for this builtin is written.
///
/// Stated positively on the row rather than as an exclusion list of numeric
/// types, because an exclusion list fails open: a builtin added to the enum
/// is not in it, so it silently accepts a string default it has no way to
/// parse. Here a new builtin has to say which kind it is before it compiles.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LiteralKind {
    /// Written as a quoted string and parsed by the Java type.
    Text,
    /// A whole number that must fit in 32 bits.
    Int32,
    /// A whole number that must fit in 64 bits.
    Int64,
    /// A number with or without a fractional part.
    Fractional,
    /// `true` or `false`.
    Boolean,
    /// No literal syntax at all.
    Opaque,
}

/// Everything jails knows about one builtin type.
pub struct BuiltinSemantics {
    /// The canonical spelling: what a declaration normalises to, and what is
    /// written back out when the model is rendered.
    pub token: &'static str,
    /// The other spellings a declaration may use. The Java names are here
    /// because a reader coming from Java writes `LocalDate`, not `date`, and
    /// refusing that would be pedantry rather than a check.
    pub aliases: &'static [&'static str],
    /// The Java type when the field may be absent — always a reference type,
    /// because an optional `int` has nothing to be absent as.
    pub java_boxed: &'static str,
    /// The Java type when the field is required, where a primitive exists.
    /// `None` means the boxed spelling is the only one.
    pub java_primitive: Option<&'static str>,
    /// The import `java_boxed` needs, if it is not in `java.lang`.
    pub java_import: Option<&'static str>,
    /// A Java expression of this type, for a generated factory.
    pub sample: &'static str,
    /// A *second* expression of this type, guaranteed unequal to [`sample`].
    ///
    /// A generated test sometimes has to show that two values differ -- a
    /// durable queue's idempotency conflict is one payload against another --
    /// and the honest way to build one is a value stated beside the first
    /// rather than derived from it. `bytes` is the exception: Java arrays
    /// compare by identity, so two of them are unequal whatever they hold and
    /// a "different" one would prove nothing.
    ///
    /// [`sample`]: Self::sample
    pub alternate: Option<&'static str>,
    /// The same value as [`sample`], spelled as JSON.
    ///
    /// A seed file, an `.http` request body and a fixture all need one, and it
    /// is not derivable from the Java expression: `UUID.fromString("…")` is a
    /// *string* on the wire and `1L` is a bare number. Keeping it on this row
    /// is what stops a type being accepted by a field parser that no sample
    /// table knows about -- which is how a generated request documented a body
    /// the record it came from refuses.
    ///
    /// [`sample`]: Self::sample
    pub json: &'static str,
    /// The same value as [`alternate`], spelled as JSON.
    ///
    /// A seed file writes two rows so the loader is proved to read more than
    /// one, and the rows have to *differ*: an entity whose key is `@pk` gets a
    /// duplicate-key failure at start-up from two identical ones, which is a
    /// file that fails to bind rather than a file that seeds. `bytes` has no
    /// Java alternate -- arrays compare by identity, so a second one proves
    /// nothing -- but on the wire it is an ordinary distinct string.
    ///
    /// [`alternate`]: Self::alternate
    pub json_alternate: &'static str,
    /// The Postgres column type. One dialect on purpose: a second one is a
    /// column on this row, not a second table somewhere else.
    pub sql_postgres: &'static str,
    /// How a literal default for this builtin is spelled.
    pub literal: LiteralKind,
    /// The generator defaults this builtin accepts -- `now()`, `uuid7()` and
    /// the rest. Empty means literals only.
    pub defaults: &'static [&'static str],
    /// Whether `@positive` and `@nonnegative` mean anything for it.
    pub numeric: bool,
    /// Whether it can carry a request scope -- a value proved against a
    /// same-named JWT claim, so it has to be something a claim can hold.
    pub scopeable: bool,
    /// One value of this builtin **as SQL**, for a generated proof row.
    ///
    /// Not [`sample`], which is a Java expression: these go into a statement
    /// rather than a record, so a `uuid` is quoted and a `boolean` is bare.
    /// It lives on this row for the reason every other projection does -- a
    /// second exhaustive match over the enum is a second answer, and the
    /// compiler cannot say which one is stale because both compile.
    ///
    /// The digits start at 7 so a seeded proof row cannot collide with a
    /// hand-written fixture on a unique column.
    ///
    /// [`sample`]: Self::sample
    pub sql_sample: &'static str,
    /// A *second* SQL value, for the row a proof needs to be distinguishable
    /// from [`sql_sample`] -- the parent key that is deliberately absent.
    ///
    /// Equal to `sql_sample` for the three builtins whose domain cannot supply
    /// a second value worth distinguishing (`boolean`, `zone`, `currency`).
    /// None of them is a foreign key, which is the only thing that reads it.
    ///
    /// [`sql_sample`]: Self::sql_sample
    pub sql_alternate: &'static str,
}

use crate::model::BuiltinType;

static STRING: BuiltinSemantics = BuiltinSemantics {
    token: "string",
    aliases: &["text", "String"],
    java_boxed: "String",
    java_primitive: None,
    java_import: None,
    sample: "\"sample\"",
    json: "\"sample\"",
    json_alternate: "\"other\"",
    alternate: Some("\"other\""),
    sql_postgres: "text",
    sql_sample: "'sample'",
    sql_alternate: "'other'",
    literal: LiteralKind::Text,
    defaults: &[],
    numeric: false,
    scopeable: true,
};

static INTEGER: BuiltinSemantics = BuiltinSemantics {
    token: "int",
    aliases: &["integer", "Integer"],
    java_boxed: "Integer",
    java_primitive: Some("int"),
    java_import: None,
    sample: "1",
    json: "1",
    json_alternate: "7",
    alternate: Some("7"),
    sql_postgres: "integer",
    sql_sample: "1",
    sql_alternate: "7",
    literal: LiteralKind::Int32,
    defaults: &["identity"],
    numeric: true,
    scopeable: true,
};

static LONG: BuiltinSemantics = BuiltinSemantics {
    token: "long",
    aliases: &["Long"],
    java_boxed: "Long",
    java_primitive: Some("long"),
    java_import: None,
    sample: "1L",
    json: "1",
    json_alternate: "7",
    alternate: Some("7L"),
    sql_postgres: "bigint",
    sql_sample: "1",
    sql_alternate: "7",
    literal: LiteralKind::Int64,
    defaults: &["identity"],
    numeric: true,
    scopeable: true,
};

static DOUBLE: BuiltinSemantics = BuiltinSemantics {
    token: "double",
    aliases: &["Double"],
    java_boxed: "Double",
    java_primitive: Some("double"),
    java_import: None,
    sample: "1.0d",
    json: "1.0",
    json_alternate: "7.0",
    alternate: Some("7.0d"),
    sql_postgres: "double precision",
    sql_sample: "1.5",
    sql_alternate: "7.5",
    literal: LiteralKind::Fractional,
    defaults: &[],
    numeric: true,
    scopeable: false,
};

static DECIMAL: BuiltinSemantics = BuiltinSemantics {
    token: "decimal",
    aliases: &["bigdecimal", "BigDecimal"],
    java_boxed: "BigDecimal",
    java_primitive: None,
    java_import: Some("java.math.BigDecimal"),
    sample: "BigDecimal.ONE",
    json: "1",
    json_alternate: "10",
    alternate: Some("BigDecimal.TEN"),
    sql_postgres: "numeric",
    sql_sample: "1.50",
    sql_alternate: "7.50",
    literal: LiteralKind::Fractional,
    defaults: &[],
    numeric: true,
    scopeable: false,
};

static BOOLEAN: BuiltinSemantics = BuiltinSemantics {
    token: "boolean",
    aliases: &["bool", "Boolean"],
    java_boxed: "Boolean",
    java_primitive: Some("boolean"),
    java_import: None,
    sample: "false",
    json: "false",
    json_alternate: "true",
    alternate: Some("true"),
    sql_postgres: "boolean",
    sql_sample: "true",
    sql_alternate: "false",
    literal: LiteralKind::Boolean,
    defaults: &[],
    numeric: false,
    scopeable: false,
};

static UUID: BuiltinSemantics = BuiltinSemantics {
    token: "uuid",
    aliases: &["UUID"],
    java_boxed: "UUID",
    java_primitive: None,
    java_import: Some("java.util.UUID"),
    sample: "UUID.fromString(\"00000000-0000-0000-0000-000000000001\")",
    json: "\"00000000-0000-0000-0000-000000000001\"",
    json_alternate: "\"00000000-0000-0000-0000-000000000007\"",
    alternate: Some("UUID.fromString(\"00000000-0000-0000-0000-000000000007\")"),
    sql_postgres: "uuid",
    sql_sample: "'00000000-0000-0000-0000-000000000001'",
    sql_alternate: "'00000000-0000-0000-0000-000000000002'",
    literal: LiteralKind::Text,
    defaults: &["uuid7"],
    numeric: false,
    scopeable: true,
};

static DATE: BuiltinSemantics = BuiltinSemantics {
    token: "date",
    aliases: &["LocalDate"],
    java_boxed: "LocalDate",
    java_primitive: None,
    java_import: Some("java.time.LocalDate"),
    sample: "LocalDate.parse(\"2026-01-01\")",
    json: "\"2026-01-01\"",
    json_alternate: "\"2026-07-07\"",
    alternate: Some("LocalDate.parse(\"2026-07-07\")"),
    sql_postgres: "date",
    sql_sample: "'2026-01-01'",
    sql_alternate: "'2026-01-02'",
    literal: LiteralKind::Text,
    defaults: &["today"],
    numeric: false,
    scopeable: false,
};

static DATE_TIME: BuiltinSemantics = BuiltinSemantics {
    token: "datetime",
    aliases: &["LocalDateTime"],
    java_boxed: "LocalDateTime",
    java_primitive: None,
    java_import: Some("java.time.LocalDateTime"),
    sample: "LocalDateTime.parse(\"2026-01-01T00:00:00\")",
    json: "\"2026-01-01T00:00:00\"",
    json_alternate: "\"2026-07-07T07:07:07\"",
    alternate: Some("LocalDateTime.parse(\"2026-07-07T07:07:07\")"),
    sql_postgres: "timestamp",
    sql_sample: "'2026-01-01T00:00:00'",
    sql_alternate: "'2026-01-02T00:00:00'",
    literal: LiteralKind::Text,
    defaults: &["now"],
    numeric: false,
    scopeable: false,
};

static INSTANT: BuiltinSemantics = BuiltinSemantics {
    token: "instant",
    aliases: &["Instant"],
    java_boxed: "Instant",
    java_primitive: None,
    java_import: Some("java.time.Instant"),
    sample: "Instant.parse(\"2026-01-01T00:00:00Z\")",
    json: "\"2026-01-01T00:00:00Z\"",
    json_alternate: "\"2026-07-07T07:07:07Z\"",
    alternate: Some("Instant.parse(\"2026-07-07T07:07:07Z\")"),
    sql_postgres: "timestamptz",
    sql_sample: "'2026-01-01T00:00:00Z'",
    sql_alternate: "'2026-01-02T00:00:00Z'",
    literal: LiteralKind::Text,
    defaults: &["now"],
    numeric: false,
    scopeable: false,
};

static DURATION: BuiltinSemantics = BuiltinSemantics {
    token: "duration",
    aliases: &["Duration"],
    java_boxed: "Duration",
    java_primitive: None,
    java_import: Some("java.time.Duration"),
    sample: "Duration.ofMinutes(1)",
    json: "\"PT1M\"",
    json_alternate: "\"PT7M\"",
    alternate: Some("Duration.ofMinutes(7)"),
    sql_postgres: "interval",
    sql_sample: "'PT1S'",
    sql_alternate: "'PT7S'",
    literal: LiteralKind::Text,
    defaults: &[],
    numeric: false,
    scopeable: false,
};

/// `text`, not a URI column type: Postgres has none, and jails does not invent
/// a domain for one. The Java side keeps the type, so the narrowing is at the
/// storage boundary where it is visible in the DDL.
static URI: BuiltinSemantics = BuiltinSemantics {
    token: "uri",
    aliases: &["URI"],
    java_boxed: "URI",
    java_primitive: None,
    java_import: Some("java.net.URI"),
    sample: "URI.create(\"https://example.test\")",
    json: "\"https://example.test\"",
    json_alternate: "\"https://other.test\"",
    alternate: Some("URI.create(\"https://other.test\")"),
    sql_postgres: "text",
    sql_sample: "'https://example.invalid/1'",
    sql_alternate: "'https://example.invalid/2'",
    literal: LiteralKind::Text,
    defaults: &[],
    numeric: false,
    scopeable: false,
};

static PATH: BuiltinSemantics = BuiltinSemantics {
    token: "path",
    aliases: &["Path"],
    java_boxed: "Path",
    java_primitive: None,
    java_import: Some("java.nio.file.Path"),
    sample: "Path.of(\"sample\")",
    json: "\"sample\"",
    json_alternate: "\"other\"",
    alternate: Some("Path.of(\"other\")"),
    sql_postgres: "text",
    sql_sample: "'/sample/1'",
    sql_alternate: "'/sample/2'",
    literal: LiteralKind::Text,
    defaults: &[],
    numeric: false,
    scopeable: false,
};

static ZONE_ID: BuiltinSemantics = BuiltinSemantics {
    token: "zone-id",
    aliases: &["ZoneId", "zoneid"],
    java_boxed: "ZoneId",
    java_primitive: None,
    java_import: Some("java.time.ZoneId"),
    sample: "ZoneId.of(\"UTC\")",
    json: "\"UTC\"",
    json_alternate: "\"Europe/Paris\"",
    alternate: Some("ZoneId.of(\"Europe/Paris\")"),
    sql_postgres: "text",
    sql_sample: "'UTC'",
    sql_alternate: "'Europe/London'",
    literal: LiteralKind::Text,
    defaults: &[],
    numeric: false,
    scopeable: false,
};

/// **`Currency` is deliberately not an alias**, which is why this row lists
/// none while every neighbour lists its Java spelling.
///
/// The rule is the field syntax's: lowercase is jails' table, capitalised is a
/// type the project owns. An enum of the currencies a project deals in is an
/// ordinary thing to generate, and an alias would resolve `currency:Currency`
/// to `java.util.Currency` in a project whose own `enum Currency` sits right
/// beside it -- a record compiled against the wrong type, failing at its
/// first use.
static CURRENCY: BuiltinSemantics = BuiltinSemantics {
    token: "currency",
    aliases: &[],
    java_boxed: "Currency",
    java_primitive: None,
    java_import: Some("java.util.Currency"),
    sample: "Currency.getInstance(\"USD\")",
    json: "\"USD\"",
    json_alternate: "\"EUR\"",
    alternate: Some("Currency.getInstance(\"EUR\")"),
    sql_postgres: "text",
    sql_sample: "'GBP'",
    sql_alternate: "'EUR'",
    literal: LiteralKind::Text,
    defaults: &[],
    numeric: false,
    scopeable: false,
};

/// `byte[]` is an array, not a class, so it takes no import and has no boxed
/// spelling to fall back to — the same array type serves both optionalities.
static BYTES: BuiltinSemantics = BuiltinSemantics {
    token: "bytes",
    aliases: &[],
    java_boxed: "byte[]",
    java_primitive: None,
    java_import: None,
    sample: "new byte[]{1}",
    json: "\"AQ==\"",
    json_alternate: "\"Ag==\"",
    alternate: None,
    sql_postgres: "bytea",
    sql_sample: "'\\x01'",
    sql_alternate: "'\\x02'",
    literal: LiteralKind::Opaque,
    defaults: &[],
    numeric: false,
    scopeable: false,
};

/// Every builtin, for the lookups that scan by token or alias.
///
/// The rows are `static`, not `const`: a `const` is a value each use may
/// inline into a fresh temporary, so two references to one row need not be
/// the same address and `the_scan_table_and_the_exhaustive_match_agree` could
/// not check what it is for. A `static` is one instance with one address,
/// which is what a registry of single rows actually is.
/// Every builtin field type, in declaration order.
///
/// `pub` because `jails explain jdl` prints the language's own type list and
/// a second copy of it beside this one is the drift the registry exists to
/// stop.
pub const ALL: &[(BuiltinType, &BuiltinSemantics)] = &[
    (BuiltinType::String, &STRING),
    (BuiltinType::Integer, &INTEGER),
    (BuiltinType::Long, &LONG),
    (BuiltinType::Double, &DOUBLE),
    (BuiltinType::Decimal, &DECIMAL),
    (BuiltinType::Boolean, &BOOLEAN),
    (BuiltinType::Uuid, &UUID),
    (BuiltinType::Date, &DATE),
    (BuiltinType::DateTime, &DATE_TIME),
    (BuiltinType::Instant, &INSTANT),
    (BuiltinType::Duration, &DURATION),
    (BuiltinType::Uri, &URI),
    (BuiltinType::Path, &PATH),
    (BuiltinType::ZoneId, &ZONE_ID),
    (BuiltinType::Currency, &CURRENCY),
    (BuiltinType::Bytes, &BYTES),
];

impl BuiltinType {
    /// This builtin's one row.
    ///
    /// Exhaustive on purpose: a variant added to [`BuiltinType`] fails to
    /// compile here until somebody writes what it means.
    pub fn semantics(self) -> &'static BuiltinSemantics {
        match self {
            Self::String => &STRING,
            Self::Integer => &INTEGER,
            Self::Long => &LONG,
            Self::Double => &DOUBLE,
            Self::Decimal => &DECIMAL,
            Self::Boolean => &BOOLEAN,
            Self::Uuid => &UUID,
            Self::Date => &DATE,
            Self::DateTime => &DATE_TIME,
            Self::Instant => &INSTANT,
            Self::Duration => &DURATION,
            Self::Uri => &URI,
            Self::Path => &PATH,
            Self::ZoneId => &ZONE_ID,
            Self::Currency => &CURRENCY,
            Self::Bytes => &BYTES,
        }
    }

    /// The builtin a canonical token names, if it names one.
    pub(crate) fn from_token(token: &str) -> Option<Self> {
        ALL.iter()
            .find(|(_, row)| row.token == token)
            .map(|(builtin, _)| *builtin)
    }

    /// The builtin a Java spelling names: its boxed type, its primitive, or
    /// its fully qualified import.
    ///
    /// **The table read backwards, which is what `jails adopt resource` needs
    /// and nothing else may guess at.** A reader's `UUID id` component maps to
    /// `uuid` because this row says `UUID` is what `uuid` renders to; a
    /// spelling no row renders to is `None`, and the caller refuses by name
    /// rather than passing a `LocalTime` through as a project type.
    pub fn from_java(spelling: &str) -> Option<Self> {
        ALL.iter()
            .find(|(_, row)| {
                row.java_boxed == spelling
                    || row.java_primitive == Some(spelling)
                    || row.java_import == Some(spelling)
            })
            .map(|(builtin, _)| *builtin)
    }

    /// Every Java spelling [`Self::from_java`] accepts, in table order, for
    /// a refusal that has to name them.
    pub fn java_spellings() -> Vec<&'static str> {
        ALL.iter()
            .flat_map(|(_, row)| [row.java_primitive, Some(row.java_boxed)])
            .flatten()
            .collect()
    }

    /// The builtin an *alias* names, and only an alias.
    ///
    /// **`canonicalize` cannot answer this**, because it returns the token
    /// unchanged for anything it does not recognise -- so it says nothing
    /// about whether a spelling was one of ours. A caller deciding what to do
    /// about `String` needs the difference between "this is `string` written
    /// the Java way" and "this is a type the project owns".
    pub(crate) fn from_alias(token: &str) -> Option<Self> {
        ALL.iter()
            .find(|(_, row)| row.aliases.contains(&token))
            .map(|(builtin, _)| *builtin)
    }

    /// The canonical spelling of whatever a declaration wrote.
    ///
    /// An unrecognised token is returned unchanged: it is a project type, and
    /// deciding that is the type resolver's job rather than this table's.
    pub fn canonicalize(token: &str) -> &str {
        ALL.iter()
            .find(|(_, row)| row.token == token || row.aliases.contains(&token))
            .map(|(_, row)| row.token)
            .unwrap_or(token)
    }

    /// The Java type, and the import it needs.
    ///
    /// `required` picks the primitive where one exists: a required `int` is
    /// `int`, an optional one is `Integer`, and a record component cannot be a
    /// primitive that is sometimes absent.
    pub fn java_type(self, required: bool) -> (&'static str, Option<&'static str>) {
        let row = self.semantics();
        let name = match row.java_primitive {
            Some(primitive) if required => primitive,
            _ => row.java_boxed,
        };
        (name, row.java_import)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `ALL` and [`BuiltinType::semantics`] must name the same row.
    ///
    /// `semantics` is what the compiler checks; `ALL` is what the token
    /// lookups scan. A builtin in one and not the other is the drift this
    /// module exists to remove, reintroduced one level down.
    #[test]
    fn the_scan_table_and_the_exhaustive_match_agree() {
        for (builtin, row) in ALL {
            assert!(
                std::ptr::eq(builtin.semantics(), *row),
                "`{}` resolves to a different row through `semantics()` than through `ALL`",
                row.token
            );
        }
    }

    /// Two builtins answering to one spelling makes `canonicalize` depend on
    /// declaration order, which is not a thing a reader can see.
    #[test]
    fn no_spelling_names_two_builtins() {
        let mut seen = std::collections::BTreeSet::new();
        for (_, row) in ALL {
            assert!(seen.insert(row.token), "`{}` is claimed twice", row.token);
            for alias in row.aliases {
                assert!(seen.insert(alias), "`{alias}` is claimed twice");
            }
        }
    }

    /// `from_token` and `semantics().token` are inverses, checked as a pair.
    #[test]
    fn a_token_round_trips_through_its_builtin() {
        for (builtin, row) in ALL {
            assert_eq!(BuiltinType::from_token(row.token), Some(*builtin));
            assert_eq!(builtin.semantics().token, row.token);
            for alias in row.aliases {
                assert_eq!(BuiltinType::canonicalize(alias), row.token);
            }
        }
    }

    /// The table read backwards: every Java spelling a row renders to names
    /// that row, a spelling no row renders to names nothing, and the list a
    /// refusal prints is exactly the spellings accepted.
    #[test]
    fn a_java_spelling_names_the_builtin_that_renders_to_it_and_nothing_else() {
        for (builtin, row) in ALL {
            assert_eq!(BuiltinType::from_java(row.java_boxed), Some(*builtin));
            if let Some(primitive) = row.java_primitive {
                assert_eq!(BuiltinType::from_java(primitive), Some(*builtin));
            }
            if let Some(import) = row.java_import {
                assert_eq!(BuiltinType::from_java(import), Some(*builtin));
            }
        }
        assert_eq!(BuiltinType::from_java("LocalTime"), None);
        assert_eq!(BuiltinType::from_java("string"), None);
        for spelling in BuiltinType::java_spellings() {
            assert!(BuiltinType::from_java(spelling).is_some(), "{spelling}");
        }
    }

    /// A primitive is only ever the required spelling of a boxed type, and an
    /// optional field must never be given one.
    #[test]
    fn an_optional_field_never_takes_a_primitive() {
        for (builtin, row) in ALL {
            let (optional, _) = builtin.java_type(false);
            assert_eq!(optional, row.java_boxed);
            let (required, _) = builtin.java_type(true);
            if let Some(primitive) = row.java_primitive {
                assert_eq!(required, primitive);
            }
        }
    }
}
