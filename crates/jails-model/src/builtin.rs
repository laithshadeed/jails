//! One row per builtin type, and every projection of it read from that row.
//!
//! ## Why this exists
//!
//! `simplify-sol.md`'s fitness rule is *every builtin type has one semantics
//! row*, and its deletion map names the disease: *repeated `Layer`, route and
//! name tables → small typed registries with derived projections*, deleting
//! "synchronized enum/label/package tables".
//!
//! Sixteen builtins were being described by six separate matches — the token a
//! declaration writes, the token it normalises to, the Java type, the import
//! that type needs, the sample a factory emits, the Postgres column. Each was
//! correct on its own and none of them said so about the others, so the
//! failure mode was never a compile error: add a builtin and the model accepts
//! it, the emitter writes a field of it, and the DDL has no column for it.
//! `parse` and `canonical_name` were the sharpest case, being literal inverses
//! written out separately — the pair that has to agree and is checked by
//! nothing.
//!
//! ## What is normative
//!
//! - **[`BuiltinType::semantics`] is an exhaustive match**, so a variant added
//!   to the enum does not compile until its row exists. That is the guarantee
//!   the six matches could not offer between them: each was exhaustive over
//!   the enum and silent about the other five.
//! - **A projection is a field read, never a second match.** Anything that
//!   needs to know something about a builtin asks the row.
//! - **The token is the canonical spelling**, and [`BuiltinSemantics::aliases`]
//!   are the spellings a declaration may use instead. Both live on the row, so
//!   normalising and canonicalising cannot disagree about which is which.

/// How a literal default for this builtin is written.
///
/// The linker used to decide this with its own match over [`BuiltinType`],
/// phrased as a *negation* -- a string default was allowed for anything that
/// was not one of six named numeric types. That fails open: a builtin added
/// to the enum is not in the exclusion list, so it silently accepts a string
/// default it has no way to parse. Stated positively on the row, a new builtin
/// has to say which kind it is before it compiles.
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
}

use crate::model::BuiltinType;

static STRING: BuiltinSemantics = BuiltinSemantics {
    token: "string",
    aliases: &["text", "String"],
    java_boxed: "String",
    java_primitive: None,
    java_import: None,
    sample: "\"sample\"",
    alternate: Some("\"other\""),
    sql_postgres: "text",
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
    alternate: Some("7"),
    sql_postgres: "integer",
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
    alternate: Some("7L"),
    sql_postgres: "bigint",
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
    alternate: Some("7.0d"),
    sql_postgres: "double precision",
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
    alternate: Some("BigDecimal.TEN"),
    sql_postgres: "numeric",
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
    alternate: Some("true"),
    sql_postgres: "boolean",
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
    alternate: Some("UUID.fromString(\"00000000-0000-0000-0000-000000000007\")"),
    sql_postgres: "uuid",
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
    alternate: Some("LocalDate.parse(\"2026-07-07\")"),
    sql_postgres: "date",
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
    alternate: Some("LocalDateTime.parse(\"2026-07-07T07:07:07\")"),
    sql_postgres: "timestamp",
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
    alternate: Some("Instant.parse(\"2026-07-07T07:07:07Z\")"),
    sql_postgres: "timestamptz",
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
    alternate: Some("Duration.ofMinutes(7)"),
    sql_postgres: "interval",
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
    alternate: Some("URI.create(\"https://other.test\")"),
    sql_postgres: "text",
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
    alternate: Some("Path.of(\"other\")"),
    sql_postgres: "text",
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
    alternate: Some("ZoneId.of(\"Europe/Paris\")"),
    sql_postgres: "text",
    literal: LiteralKind::Text,
    defaults: &[],
    numeric: false,
    scopeable: false,
};

static CURRENCY: BuiltinSemantics = BuiltinSemantics {
    token: "currency",
    aliases: &["Currency"],
    java_boxed: "Currency",
    java_primitive: None,
    java_import: Some("java.util.Currency"),
    sample: "Currency.getInstance(\"USD\")",
    alternate: Some("Currency.getInstance(\"EUR\")"),
    sql_postgres: "text",
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
    alternate: None,
    sql_postgres: "bytea",
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
pub(crate) const ALL: &[(BuiltinType, &BuiltinSemantics)] = &[
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
    /// compile here until somebody writes what it means. That is the whole
    /// point of the row — six separate matches were each exhaustive over this
    /// enum, so the compiler forced six edits and checked that none of them
    /// agreed with the others.
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
    pub fn from_token(token: &str) -> Option<Self> {
        ALL.iter()
            .find(|(_, row)| row.token == token)
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

    /// The pair that used to be written out separately, checked as a pair.
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
