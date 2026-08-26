//! What one declared component is.
//!
//! The field syntax and every rule that holds *within* one component: the
//! closed scalar vocabulary, the collection shapes, optionality, and the table
//! constraints that ride on the type. Nothing here knows what a whole
//! declaration looks like -- that is the parent module's subject.
//!
//! The cross-checks live in [`FieldSpec::parse`] rather than at three call
//! sites, because a rule enforced in one place and not another is the shape of
//! every drift bug in this repository's history.

use crate::Result;
use crate::identity::{JavaType, Name, Package};
use jails_support::codec::{Codec, Decoder, Encoder};

/// A built-in scalar, or a type the project owns.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum ScalarFieldType {
    Text,
    Integer,
    Long,
    Boolean,
    LocalDate,
    LocalDateTime,
    Instant,
    Uuid,
    Currency,
    Decimal,
    Bytes,
    Duration,
    ZoneId,
    Uri,
    Path,
    Double,
    /// A capitalised spelling: a type this project owns, fully qualified.
    Project(JavaType),
}

impl ScalarFieldType {
    /// Fixed wire tags. Rust discriminants are never serialised, and these
    /// numbers may never be reused for a different meaning.
    fn tag(&self) -> u8 {
        match self {
            Self::Text => 0,
            Self::Integer => 1,
            Self::Long => 2,
            Self::Boolean => 3,
            Self::LocalDate => 4,
            Self::LocalDateTime => 5,
            Self::Instant => 6,
            Self::Uuid => 7,
            Self::Currency => 8,
            Self::Decimal => 9,
            Self::Bytes => 10,
            Self::Duration => 11,
            Self::ZoneId => 12,
            Self::Uri => 13,
            Self::Path => 14,
            Self::Double => 15,
            Self::Project(_) => 16,
        }
    }

    /// The canonical declaration spelling — one per scalar, whatever the user
    /// typed. This is what a report shows and what a ledger records.
    pub fn canonical(&self) -> String {
        match self {
            Self::Text => "string".to_string(),
            Self::Integer => "int".to_string(),
            Self::Long => "long".to_string(),
            Self::Boolean => "boolean".to_string(),
            Self::LocalDate => "date".to_string(),
            Self::LocalDateTime => "datetime".to_string(),
            Self::Instant => "instant".to_string(),
            Self::Uuid => "uuid".to_string(),
            Self::Currency => "currency".to_string(),
            Self::Decimal => "decimal".to_string(),
            Self::Bytes => "bytes".to_string(),
            Self::Duration => "duration".to_string(),
            Self::ZoneId => "zone-id".to_string(),
            Self::Uri => "uri".to_string(),
            Self::Path => "path".to_string(),
            Self::Double => "double".to_string(),
            Self::Project(ty) => ty.qualified(),
        }
    }

    /// Whether `!` (non-blank) is meaningful. Only text has blankness.
    pub fn is_text(&self) -> bool {
        matches!(self, Self::Text)
    }

    /// Every accepted spelling, lowercase and Java alike, onto one value.
    ///
    /// `base` resolves a capitalised project type into a fully qualified name;
    /// the project's own types have no import list at this layer.
    pub fn parse(token: &str, base: &Package) -> Result<Self> {
        let builtin = match token {
            "string" | "text" | "String" => Some(Self::Text),
            "int" | "integer" | "Integer" => Some(Self::Integer),
            "long" | "Long" => Some(Self::Long),
            "boolean" | "Boolean" => Some(Self::Boolean),
            "date" | "LocalDate" => Some(Self::LocalDate),
            "datetime" | "LocalDateTime" => Some(Self::LocalDateTime),
            "instant" | "timestamp" | "Instant" => Some(Self::Instant),
            "uuid" | "UUID" => Some(Self::Uuid),
            // Lowercase only, and this is the one arm where that matters.
            // `Currency` capitalised is a type the project owns -- an enum of
            // the currencies it deals in is an ordinary thing to generate, and
            // `jails g enum Currency GBP USD` then `amount:Currency` is the
            // case. Every other Java spelling here (`String`, `Instant`,
            // `UUID`) is a name nobody declares themselves, which is exactly
            // the line `jails_spec::spec::builtin_by_java_name` draws and this
            // arm crossed. `pending.md` §6.3 found it by merging the two
            // parsers: with two, the divergence was invisible.
            "currency" => Some(Self::Currency),
            "decimal" | "bigdecimal" | "BigDecimal" => Some(Self::Decimal),
            "bytes" => Some(Self::Bytes),
            "duration" | "Duration" => Some(Self::Duration),
            "zone-id" | "zoneid" | "ZoneId" => Some(Self::ZoneId),
            "uri" | "URI" => Some(Self::Uri),
            "path" | "Path" => Some(Self::Path),
            "double" | "Double" => Some(Self::Double),
            _ => None,
        };
        if let Some(scalar) = builtin {
            return Ok(scalar);
        }
        // Case is the rule, and it applies to the *simple* name:
        // `com.other.Thing` is qualified and starts lowercase.
        let simple = token.rsplit('.').next().unwrap_or(token);
        let first = simple.chars().next().ok_or("field type is empty")?;
        if !first.is_ascii_uppercase() {
            return Err(format!(
                "unknown field type `{token}`.\n       fix: capitalise it to mean a type this \
                 project owns, or use one of: string, int, long, boolean, date, datetime, \
                 instant, uuid, currency, decimal, bytes, duration, zone-id, uri, path, double."
            )
            .into());
        }
        // Already qualified stays as written; a bare name joins the base.
        let qualified = if token.contains('.') {
            JavaType::parse(token)?
        } else {
            JavaType::new(base.clone(), Name::parse(token)?)
        };
        Ok(Self::Project(qualified))
    }
}
impl Codec for ScalarFieldType {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.tag(self.tag());
        match self {
            Self::Project(ty) => ty.encode(encoder),
            _ => Ok(()),
        }
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(match decoder.tag()? {
            0 => Self::Text,
            1 => Self::Integer,
            2 => Self::Long,
            3 => Self::Boolean,
            4 => Self::LocalDate,
            5 => Self::LocalDateTime,
            6 => Self::Instant,
            7 => Self::Uuid,
            8 => Self::Currency,
            9 => Self::Decimal,
            10 => Self::Bytes,
            11 => Self::Duration,
            12 => Self::ZoneId,
            13 => Self::Uri,
            14 => Self::Path,
            15 => Self::Double,
            16 => Self::Project(JavaType::decode(decoder)?),
            other => return Err(format!("unknown scalar field type tag {other}").into()),
        })
    }
}

/// A field's shape. Nested collections are deliberately unrepresentable.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum FieldType {
    Scalar(ScalarFieldType),
    List(ScalarFieldType),
    Map {
        key: ScalarFieldType,
        value: ScalarFieldType,
    },
}

impl FieldType {
    pub fn is_collection(&self) -> bool {
        !matches!(self, Self::Scalar(_))
    }

    /// The scalar `!` and the numeric constraints apply to.
    pub fn element(&self) -> &ScalarFieldType {
        match self {
            Self::Scalar(scalar) | Self::List(scalar) => scalar,
            Self::Map { value, .. } => value,
        }
    }

    /// `list<T>` and `map<K,V>`, with nesting refused rather than half-handled.
    pub fn parse(token: &str, base: &Package) -> Result<Self> {
        if let Some(inner) = wrapped(token, "list") {
            let scalar = Self::no_nesting(inner, base, "list")?;
            return Ok(Self::List(scalar));
        }
        if let Some(inner) = wrapped(token, "map") {
            let (key, value) = split_map(inner)?;
            return Ok(Self::Map {
                key: Self::no_nesting(key, base, "map")?,
                value: Self::no_nesting(value, base, "map")?,
            });
        }
        Ok(Self::Scalar(ScalarFieldType::parse(token, base)?))
    }

    fn no_nesting(token: &str, base: &Package, outer: &str) -> Result<ScalarFieldType> {
        if wrapped(token, "list").is_some() || wrapped(token, "map").is_some() {
            return Err(format!(
                "`{outer}<{token}>` nests a collection, which jails has no column or component \
                 shape for.\n       fix: introduce a record for the inner value and hold a \
                 collection of that."
            )
            .into());
        }
        ScalarFieldType::parse(token, base)
    }

    pub fn canonical(&self) -> String {
        match self {
            Self::Scalar(scalar) => scalar.canonical(),
            Self::List(scalar) => format!("list<{}>", scalar.canonical()),
            Self::Map { key, value } => {
                format!("map<{},{}>", key.canonical(), value.canonical())
            }
        }
    }
}
impl Codec for FieldType {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        match self {
            Self::Scalar(scalar) => {
                encoder.tag(0);
                scalar.encode(encoder)
            }
            Self::List(scalar) => {
                encoder.tag(1);
                scalar.encode(encoder)
            }
            Self::Map { key, value } => {
                encoder.tag(2);
                key.encode(encoder)?;
                value.encode(encoder)
            }
        }
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(match decoder.tag()? {
            0 => Self::Scalar(ScalarFieldType::decode(decoder)?),
            1 => Self::List(ScalarFieldType::decode(decoder)?),
            2 => Self::Map {
                key: ScalarFieldType::decode(decoder)?,
                value: ScalarFieldType::decode(decoder)?,
            },
            other => return Err(format!("unknown field type tag {other}").into()),
        })
    }
}

/// `list<...>` / `map<...>` unwrapping, exact rather than by `contains`.
fn wrapped<'a>(token: &'a str, name: &str) -> Option<&'a str> {
    token
        .strip_prefix(name)?
        .strip_prefix('<')?
        .strip_suffix('>')
}

/// Split `K,V` on the *separating* comma.
///
/// The same rule the ledger's array splitting needed: `map<string,double>` is
/// a documented type, so a naive split of the whole token is what cut a field
/// spec in half.
fn split_map(inner: &str) -> Result<(&str, &str)> {
    let mut depth = 0usize;
    for (index, character) in inner.char_indices() {
        match character {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => return Ok((&inner[..index], &inner[index + 1..])),
            _ => {}
        }
    }
    Err(format!("`map<{inner}>` needs a key and a value separated by a comma").into())
}

/// Whether a value may be absent, and how absence is expressed.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum Optionality {
    Required,
    /// `!` — present *and* not blank. A text property only.
    NonBlank,
    /// `?` — an `Optional<T>` component.
    Nullable,
}

impl Optionality {
    fn tag(self) -> u8 {
        match self {
            Self::Required => 0,
            Self::NonBlank => 1,
            Self::Nullable => 2,
        }
    }

    fn from_tag(tag: u8) -> Result<Self> {
        match tag {
            0 => Ok(Self::Required),
            1 => Ok(Self::NonBlank),
            2 => Ok(Self::Nullable),
            other => Err(format!("unknown optionality tag {other}").into()),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum NumericConstraint {
    Positive,
    NonNegative,
}

/// The closed set of table constraints. They change SQL and nothing about the
/// Java type — except `scoped`, which touches no SQL at all.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub struct FieldConstraints {
    pub primary_key: bool,
    pub unique: bool,
    pub indexed: bool,
    /// `@scope`: a request-boundary field proved against a same-named JWT claim.
    pub scoped: bool,
    pub numeric: Option<NumericConstraint>,
}

impl FieldConstraints {
    /// Markers in any order, with a duplicate or contradiction refused.
    ///
    /// An unknown marker is an error rather than a no-op: `@primary` silently
    /// meaning "no constraint" would produce a schema quietly missing the
    /// primary key somebody believed they had asked for, which is the failure
    /// this feature exists to remove.
    pub fn parse(markers: &[&str]) -> Result<Self> {
        let mut out = Self::default();
        for marker in markers {
            let already = match *marker {
                "pk" => std::mem::replace(&mut out.primary_key, true),
                "unique" => std::mem::replace(&mut out.unique, true),
                "index" => std::mem::replace(&mut out.indexed, true),
                "scope" => std::mem::replace(&mut out.scoped, true),
                "positive" | "nonnegative" => {
                    let wanted = if *marker == "positive" {
                        NumericConstraint::Positive
                    } else {
                        NumericConstraint::NonNegative
                    };
                    match out.numeric.replace(wanted) {
                        None => false,
                        Some(previous) if previous == wanted => true,
                        Some(previous) => {
                            return Err(format!(
                                "`@{marker}` contradicts `@{}` on the same field.\n       \
                                 fix: keep exactly one numeric constraint.",
                                match previous {
                                    NumericConstraint::Positive => "positive",
                                    NumericConstraint::NonNegative => "nonnegative",
                                }
                            )
                            .into());
                        }
                    }
                }
                other => {
                    return Err(format!(
                        "unknown constraint `@{other}`.\n       fix: one of @pk, @unique, \
                         @index, @scope, @positive, @nonnegative. An unrecognised marker is an \
                         error rather than a no-op, so a schema cannot quietly lack a \
                         constraint somebody asked for."
                    )
                    .into());
                }
            };
            if already {
                return Err(format!("`@{marker}` is repeated on the same field").into());
            }
        }
        Ok(out)
    }
}
impl Codec for FieldConstraints {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.bool(self.primary_key);
        encoder.bool(self.unique);
        encoder.bool(self.indexed);
        encoder.bool(self.scoped);
        encoder.option(self.numeric.as_ref(), |e, numeric| {
            e.tag(match numeric {
                NumericConstraint::Positive => 0,
                NumericConstraint::NonNegative => 1,
            });
            Ok(())
        })
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(Self {
            primary_key: decoder.bool()?,
            unique: decoder.bool()?,
            indexed: decoder.bool()?,
            scoped: decoder.bool()?,
            numeric: decoder.option(|d| match d.tag()? {
                0 => Ok(NumericConstraint::Positive),
                1 => Ok(NumericConstraint::NonNegative),
                other => Err(format!("unknown numeric constraint tag {other}").into()),
            })?,
        })
    }
}

/// One declared field, fully resolved.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct FieldSpec {
    pub name: Name,
    pub field_type: FieldType,
    pub optionality: Optionality,
    pub constraints: FieldConstraints,
}

/// `name:type[!?]@marker` tokens as the Java facts they imply.
///
/// **The one parser.** `pending.md` §6.3: this syntax had two, and they were
/// pinned to each other by a test running twenty-six tokens through both --
/// which is what you write when you cannot merge them. The block was layering:
/// `Field` and the derivation live in `jails-spec`, `FieldSpec` lives here, and
/// a lower crate cannot call an upper one.
///
/// The merge went the other way in the end, and is smaller than either option
/// the item proposed: nothing moved. `parse_fields` is *parsing*, so it comes
/// up to the parser; `derive_field` is *derivation*, so it stays below with the
/// Java facts it computes. This function is the seam, and there is now exactly
/// one place that decides what `name:type[!?]@marker` means.
///
/// The base package is [`Package::base`] deliberately. A [`Field`] records
/// `owned` and a simple `java_type` and no package at all -- the generators put
/// an owned type in the same package as the record that names it -- so
/// qualifying it against the project's base and then discarding the
/// qualification would be an argument every caller had to supply and none
/// could get wrong.
///
/// [`Field`]: jails_spec::spec::Field
pub fn parse_fields(tokens: &[String]) -> Result<Vec<jails_spec::spec::Field>> {
    tokens
        .iter()
        .map(|token| FieldSpec::parse(token, &Package::base())?.projected())
        .collect()
}

impl FieldSpec {
    /// `name:type[!?]` with `@marker`s, normalised and checked together.
    ///
    /// The cross-checks live here rather than at three call sites because a
    /// rule enforced in one place and not another is the shape of every drift
    /// bug in this repository's history.
    pub fn parse(token: &str, base: &Package) -> Result<Self> {
        let (name, rest) = token
            .split_once(':')
            .ok_or_else(|| format!("field `{token}` needs a `name:type`"))?;
        let name = Name::parse(name)?;
        reject_sql_keyword(name.as_str())?;

        let mut parts = rest.split('@');
        let head = parts.next().unwrap_or_default();
        let markers: Vec<&str> = parts.collect();
        let constraints = FieldConstraints::parse(&markers)?;

        // Suffix before or after the markers: `id:uuid@pk` and `id:uuid!@pk`
        // both work, and so does `id:uuid@pk` with the suffix on the type.
        let (type_token, optionality) = match head.strip_suffix('!') {
            Some(stem) => (stem, Optionality::NonBlank),
            None => match head.strip_suffix('?') {
                Some(stem) => (stem, Optionality::Nullable),
                None => (head, Optionality::Required),
            },
        };
        let field_type = FieldType::parse(type_token, base)?;

        if optionality == Optionality::NonBlank && !field_type.element().is_text() {
            return Err(format!(
                "`{}` is not text, so `!` (non-blank) has no meaning for it.\n       fix: drop \
                 the `!`, or use `@positive`/`@nonnegative` for a numeric bound.",
                field_type.canonical()
            )
            .into());
        }
        if optionality == Optionality::Nullable && field_type.is_collection() {
            return Err(format!(
                "`{}` is a collection, and a null collection is worse than an empty one.\n       \
                 fix: drop the `?`; an absent collection is the empty one.",
                field_type.canonical()
            )
            .into());
        }
        if optionality == Optionality::Nullable && constraints.primary_key {
            return Err(jails_support::Failure::Told(
                "a nullable primary key is not a key.\n       fix: drop the `?` or the \
                        `@pk`."
                    .to_string(),
            ));
        }
        if constraints.numeric.is_some() && !is_numeric(field_type.element()) {
            return Err(format!(
                "`{}` is not numeric, so a numeric constraint cannot be checked against \
                 it.\n       fix: drop the constraint.",
                field_type.canonical()
            )
            .into());
        }
        Ok(Self {
            name,
            field_type,
            optionality,
            constraints,
        })
    }

    /// One spelling per field, whatever was typed. Two declarations that mean
    /// the same thing render the same bytes, so a comparison between them
    /// cannot report a change nobody made.
    /// This component as the rendering layer's [`jails_spec::spec::Field`].
    ///
    /// The one direction that should exist between the two models. `FieldSpec`
    /// is what a request declares -- validated, on the wire, identity-bearing;
    /// `Field` is what a template needs -- a Java type and the imports it
    /// costs. The second is *derived* from the first.
    ///
    /// It used to be re-parsed from it. `route::field` rendered this value back
    /// to a `name:type@marker` token with [`Self::canonical`] and handed the
    /// string to `parse_fields`, so a field spec was parsed up to three times
    /// per request and one of those parses read text this program had printed
    /// a line earlier. `pending.md` §6.3 names that as the deepest seam between
    /// the two engines, and `declaration/field.rs`'s own header names why it
    /// matters: *"a rule enforced in one place and not another is the shape of
    /// every drift bug in this repository."*
    ///
    /// The type token is still rendered, because the *type* vocabularies are
    /// two spellings of one set and only `resolve_type` knows the Java side.
    /// Everything else -- the name, the suffix, the markers and every
    /// cross-check they imply -- is passed as the value it already is.
    /// `a_projected_field_spec_equals_the_parsed_one` pins the two together.
    pub fn projected(&self) -> Result<jails_spec::spec::Field> {
        let constraints = jails_spec::spec::Constraints {
            primary_key: self.constraints.primary_key,
            unique: self.constraints.unique,
            indexed: self.constraints.indexed,
            scoped: self.constraints.scoped,
            check: self.constraints.numeric.map(|numeric| match numeric {
                NumericConstraint::Positive => jails_spec::spec::NumericCheck::Positive,
                NumericConstraint::NonNegative => jails_spec::spec::NumericCheck::NonNegative,
            }),
        };
        let optionality = match self.optionality {
            Optionality::Required => jails_spec::spec::Optionality::Required,
            Optionality::NonBlank => jails_spec::spec::Optionality::NonBlank,
            Optionality::Nullable => jails_spec::spec::Optionality::Nullable,
        };
        jails_spec::spec::derive_field(
            self.name.as_str(),
            &self.field_type.canonical(),
            optionality,
            constraints,
            &self.canonical(),
        )
    }

    pub fn canonical(&self) -> String {
        let mut out = format!("{}:{}", self.name, self.field_type.canonical());
        match self.optionality {
            Optionality::Required => {}
            Optionality::NonBlank => out.push('!'),
            Optionality::Nullable => out.push('?'),
        }
        // Marker order is not semantic, so it is fixed here.
        for (flag, marker) in [
            (self.constraints.primary_key, "pk"),
            (self.constraints.unique, "unique"),
            (self.constraints.indexed, "index"),
            (self.constraints.scoped, "scope"),
        ] {
            if flag {
                out.push('@');
                out.push_str(marker);
            }
        }
        if let Some(numeric) = self.constraints.numeric {
            out.push('@');
            out.push_str(match numeric {
                NumericConstraint::Positive => "positive",
                NumericConstraint::NonNegative => "nonnegative",
            });
        }
        out
    }
}

fn reject_sql_keyword(name: &str) -> Result<()> {
    if matches!(
        name.to_ascii_lowercase().as_str(),
        "all"
            | "any"
            | "array"
            | "asc"
            | "cast"
            | "check"
            | "collate"
            | "column"
            | "constraint"
            | "cross"
            | "current_date"
            | "current_time"
            | "current_user"
            | "desc"
            | "distinct"
            | "end"
            | "except"
            | "foreign"
            | "from"
            | "full"
            | "grant"
            | "group"
            | "having"
            | "ilike"
            | "in"
            | "inner"
            | "into"
            | "is"
            | "join"
            | "leading"
            | "left"
            | "like"
            | "limit"
            | "natural"
            | "offset"
            | "on"
            | "only"
            | "or"
            | "order"
            | "outer"
            | "primary"
            | "references"
            | "right"
            | "select"
            | "similar"
            | "some"
            | "table"
            | "then"
            | "to"
            | "union"
            | "unique"
            | "user"
            | "using"
            | "when"
            | "where"
            | "window"
            | "with"
    ) {
        return Err(format!(
            "field name `{name}` is reserved by PostgreSQL and would make generated SQL invalid.\n       \
             fix: choose a domain-specific name such as `source`, `target`, or `sortOrder`."
        )
        .into());
    }
    Ok(())
}
impl Codec for FieldSpec {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.name.encode(encoder)?;
        self.field_type.encode(encoder)?;
        encoder.tag(self.optionality.tag());
        self.constraints.encode(encoder)
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(Self {
            name: Name::decode(decoder)?,
            field_type: FieldType::decode(decoder)?,
            optionality: Optionality::from_tag(decoder.tag()?)?,
            constraints: FieldConstraints::decode(decoder)?,
        })
    }
}

fn is_numeric(scalar: &ScalarFieldType) -> bool {
    matches!(
        scalar,
        ScalarFieldType::Integer
            | ScalarFieldType::Long
            | ScalarFieldType::Decimal
            | ScalarFieldType::Double
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    // The derived side's own vocabulary, which `parse_fields` produces.
    use jails_spec::spec::Optionality as Derived;
    use jails_spec::spec::{Constraints, NumericCheck, field_type};

    #[test]
    fn field_type_maps_known_tokens() {
        assert_eq!(field_type("string").unwrap().0, "String");
        assert_eq!(field_type("text").unwrap(), ("String", None));
        assert_eq!(field_type("int").unwrap().0, "Integer");
        assert_eq!(field_type("integer").unwrap().0, "Integer");
        assert_eq!(field_type("long").unwrap().0, "Long");
        assert_eq!(field_type("boolean").unwrap().0, "Boolean");
        assert_eq!(field_type("double").unwrap().0, "Double");
        assert_eq!(
            field_type("uuid").unwrap(),
            ("UUID", Some("java.util.UUID"))
        );
        assert_eq!(
            field_type("currency").unwrap(),
            ("Currency", Some("java.util.Currency"))
        );
        assert_eq!(
            field_type("date").unwrap(),
            ("LocalDate", Some("java.time.LocalDate"))
        );
        assert_eq!(
            field_type("datetime").unwrap(),
            ("LocalDateTime", Some("java.time.LocalDateTime"))
        );
        assert_eq!(
            field_type("timestamp").unwrap(),
            ("Instant", Some("java.time.Instant"))
        );
    }

    #[test]
    fn postgresql_keywords_are_refused_before_they_reach_ddl_or_dml() {
        for name in ["from", "to", "order", "user", "current_date"] {
            let error = FieldSpec::parse(
                &format!("{name}:string"),
                &Package::parse("com.example.demo").unwrap(),
            )
            .unwrap_err();
            assert!(error.contains("reserved by PostgreSQL"), "{name}: {error}");
            assert!(error.contains("fix:"), "{name}: {error}");
        }
    }

    #[test]
    fn field_type_rejects_unknown_tokens() {
        assert!(field_type("nope").is_err());
    }

    #[test]
    fn column_markers_parse_in_any_order_and_combine() {
        let fields = parse_fields(&[
            "transactionId:uuid@pk".to_string(),
            "amount:long@positive@index".to_string(),
            "email:string!@unique".to_string(),
            "workspaceId:uuid@scope@index".to_string(),
        ])
        .unwrap();
        assert!(fields[0].constraints.primary_key);
        assert_eq!(fields[1].constraints.check, Some(NumericCheck::Positive));
        assert!(fields[1].constraints.indexed);
        assert!(fields[2].constraints.unique);
        assert!(fields[3].constraints.scoped);
        assert!(fields[3].constraints.indexed);
        // The markers do not disturb the type or the optionality suffix.
        assert_eq!(fields[0].java_type, "UUID");
        assert_eq!(fields[2].java_type, "String");
        assert_eq!(fields[2].optionality, Derived::NonBlank);
    }

    /// A marker typo that parsed as "no constraint" would produce a schema
    /// quietly missing the primary key someone thought they had asked for --
    /// the exact failure this feature exists to prevent.
    #[test]
    fn an_unknown_column_marker_is_an_error_listing_the_real_ones() {
        let err = parse_fields(&["id:uuid@primary".to_string()]).unwrap_err();
        assert!(err.contains("@primary"), "{err}");
        assert!(err.contains("@pk"), "{err}");
    }

    /// `check (name > 0)` on a text column fails at `flyway migrate`, which is
    /// a slow and remote way to learn about a typo.
    #[test]
    fn a_numeric_check_on_a_non_numeric_column_is_rejected() {
        let err = parse_fields(&["name:string@positive".to_string()]).unwrap_err();
        assert!(err.contains("numeric"), "{err}");
        assert!(parse_fields(&["amount:long@positive".to_string()]).is_ok());
        assert!(parse_fields(&["price:decimal@nonnegative".to_string()]).is_ok());
    }

    #[test]
    fn a_nullable_primary_key_is_rejected() {
        let err = parse_fields(&["id:uuid?@pk".to_string()]).unwrap_err();
        assert!(err.contains("nullable"), "{err}");
    }

    #[test]
    fn a_field_with_no_markers_has_no_constraints() {
        let fields = parse_fields(&["title:string".to_string()]).unwrap();
        assert_eq!(fields[0].constraints, Constraints::default());
    }

    #[test]
    fn parse_fields_splits_name_and_type() {
        let fields = parse_fields(&["title:string".to_string(), "body:Text".to_string()]).unwrap();
        assert_eq!(fields[0].name, "title");
        assert_eq!(fields[0].java_type, "String");
        // Capitalised means "a type this project owns", so `Text` is no longer
        // the built-in -- that is the whole point of the rule.
        assert_eq!(fields[1].java_type, "Text");
        assert!(fields[1].owned);
        assert_eq!(
            parse_fields(&["body:text".to_string()]).unwrap()[0].java_type,
            "String"
        );
    }

    /// The Java spellings of the built-in types stay built-in: `id:String`
    /// must not be read as an unknown project type.
    #[test]
    fn parse_fields_treats_java_type_names_as_builtins() {
        let fields = parse_fields(&["id:String".to_string(), "day:LocalDate".to_string()]).unwrap();
        assert!(!fields[0].owned);
        assert_eq!(fields[0].java_type, "String");
        assert!(!fields[1].owned);
        assert!(fields[1].imports.contains(&"java.time.LocalDate"));
    }

    #[test]
    fn parse_fields_resolves_collection_types() {
        let fields = parse_fields(&[
            "matched:list<Match>".to_string(),
            "ids:list<string>".to_string(),
            "rates:map<string,double>".to_string(),
            "at:instant".to_string(),
        ])
        .unwrap();

        assert_eq!(fields[0].java_type, "List<Match>");
        assert!(fields[0].collection);
        assert_eq!(fields[1].java_type, "List<String>");
        // Generics cannot hold a primitive, so the element is the wrapper.
        assert_eq!(fields[2].java_type, "Map<String, Double>");
        assert!(fields[2].imports.contains(&"java.util.Map"));
        assert_eq!(fields[3].java_type, "Instant");
        assert!(fields[3].imports.contains(&"java.time.Instant"));
    }

    #[test]
    fn parse_fields_rejects_malformed_collection_types() {
        // A bare `list` would otherwise become List<Object>, silently.
        assert!(parse_fields(&["items:list".to_string()]).is_err());
        assert!(parse_fields(&["items:list<nope>".to_string()]).is_err());
        assert!(parse_fields(&["items:map<string>".to_string()]).is_err());
        assert!(parse_fields(&["items:list<list<string>>".to_string()]).is_err());
        // A collection already models absence; `?` on one is a mistake.
        assert!(parse_fields(&["items:list<string>?".to_string()]).is_err());
    }

    #[test]
    fn parse_fields_reads_the_optionality_suffixes() {
        let fields = parse_fields(&[
            "id:string!".to_string(),
            "note:string?".to_string(),
            "name:string".to_string(),
            "source:SourceRef?".to_string(),
        ])
        .unwrap();
        assert_eq!(fields[0].optionality, Derived::NonBlank);
        assert_eq!(fields[1].optionality, Derived::Nullable);
        assert_eq!(fields[2].optionality, Derived::Required);
        assert_eq!(fields[3].optionality, Derived::Nullable);
        assert!(fields[3].owned);
        assert_eq!(fields[3].java_type, "SourceRef");
    }

    #[test]
    fn parse_fields_rejects_args_without_a_colon() {
        assert!(parse_fields(&["title".to_string()]).is_err());
    }
}
