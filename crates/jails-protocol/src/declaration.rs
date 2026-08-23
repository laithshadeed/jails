//! What an entity was declared to *be*: content, as distinct from identity.
//!
//! Named `declaration` rather than `spec` because `jails_spec::spec` is a
//! different module at a lower layer, and two modules sharing a name is how a
//! path-based scanner comes to check one of them against the other's rules.
//!
//! ## One spelling, one value
//!
//! plan.md §R1.1: *"Field syntax is normalised once at the declaration edge."*
//! `string` and `text` are the same scalar; so are `int` and `integer`,
//! `decimal` and `bigdecimal`, `zone-id` and `zoneid`, and every Java spelling
//! of the same built-in. Today those survive as distinct strings all the way
//! into the ledger, which means one declaration has several recorded forms and
//! a comparison between two of them reports a change nobody made.
//!
//! ## What is refused, and why refusing is the feature
//!
//! - **Nested collections** (`list<list<string>>`) — the SQL and Java sides
//!   have no column shape for them, so accepting one would produce code that
//!   does not compile at a point far from the declaration.
//! - **`!` on anything but text** — non-blank is a string property. `count:int!`
//!   reads like it means something and does not.
//! - **`?` on a collection or a primary key** — a null `List` is the one thing
//!   worse than an empty one, and a nullable primary key is not a key.
//! - **Duplicate or conflicting numeric markers** — `@positive@nonnegative` is
//!   a contradiction, and picking one silently is how a schema ends up with a
//!   constraint nobody asked for.
//! - **Anything after an index column but `asc`/`desc`** — the current
//!   pass-through tail would persist arbitrary text as trusted generated SQL.

use crate::Result;
use crate::identity::{JavaType, Name, Package};
use jails_support::codec::{Decoder, Encoder, ordered};

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
            "instant" | "Instant" => Some(Self::Instant),
            "uuid" | "UUID" => Some(Self::Uuid),
            "currency" | "Currency" => Some(Self::Currency),
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
            ));
        }
        // Already qualified stays as written; a bare name joins the base.
        let qualified = if token.contains('.') {
            JavaType::parse(token)?
        } else {
            JavaType::new(base.clone(), Name::parse(token)?)
        };
        Ok(Self::Project(qualified))
    }

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
            other => return Err(format!("unknown scalar field type tag {other}")),
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
            ));
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
            other => return Err(format!("unknown field type tag {other}")),
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
    Err(format!(
        "`map<{inner}>` needs a key and a value separated by a comma"
    ))
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
            other => Err(format!("unknown optionality tag {other}")),
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
                            ));
                        }
                    }
                }
                other => {
                    return Err(format!(
                        "unknown constraint `@{other}`.\n       fix: one of @pk, @unique, \
                         @index, @scope, @positive, @nonnegative. An unrecognised marker is an \
                         error rather than a no-op, so a schema cannot quietly lack a \
                         constraint somebody asked for."
                    ));
                }
            };
            if already {
                return Err(format!("`@{marker}` is repeated on the same field"));
            }
        }
        Ok(out)
    }

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
                other => Err(format!("unknown numeric constraint tag {other}")),
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
            ));
        }
        if optionality == Optionality::Nullable && field_type.is_collection() {
            return Err(format!(
                "`{}` is a collection, and a null collection is worse than an empty one.\n       \
                 fix: drop the `?`; an absent collection is the empty one.",
                field_type.canonical()
            ));
        }
        if optionality == Optionality::Nullable && constraints.primary_key {
            return Err(
                "a nullable primary key is not a key.\n       fix: drop the `?` or the \
                        `@pk`."
                    .to_string(),
            );
        }
        if constraints.numeric.is_some() && !is_numeric(field_type.element()) {
            return Err(format!(
                "`{}` is not numeric, so a numeric constraint cannot be checked against \
                 it.\n       fix: drop the constraint.",
                field_type.canonical()
            ));
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

    pub fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.name.encode(encoder)?;
        self.field_type.encode(encoder)?;
        encoder.tag(self.optionality.tag());
        self.constraints.encode(encoder)
    }

    pub fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
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

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum IndexDirection {
    Ascending,
    Descending,
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct IndexColumn {
    pub field: Name,
    pub direction: IndexDirection,
}

/// A composite or ordered index, which a per-column `@index` marker cannot say.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct IndexSpec {
    /// Order is semantic and is never sorted: a composite index on `(a, b)` is
    /// not the index on `(b, a)`.
    pub columns: Vec<IndexColumn>,
}

impl IndexSpec {
    /// `created_at desc, title` against the fields actually declared.
    ///
    /// This replaces a pass-through tail that persisted whatever followed a
    /// column name as trusted generated SQL. Only `asc` and `desc` are
    /// accepted, and an unknown column refuses here rather than at
    /// `flyway migrate`.
    pub fn parse(token: &str, fields: &[FieldSpec]) -> Result<Self> {
        let mut columns = Vec::new();
        for part in token.split(',') {
            let part = part.trim();
            if part.is_empty() {
                return Err(format!("index `{token}` has an empty column"));
            }
            let mut words = part.split_whitespace();
            let field = words.next().expect("a non-empty part has a first word");
            let direction = match words.next() {
                None | Some("asc") => IndexDirection::Ascending,
                Some("desc") => IndexDirection::Descending,
                Some(other) => {
                    return Err(format!(
                        "`{other}` follows the index column `{field}`, and only `asc` or `desc` \
                         may.\n       fix: arbitrary SQL is refused here rather than recorded as \
                         trusted generated SQL."
                    ));
                }
            };
            if let Some(trailing) = words.next() {
                return Err(format!(
                    "`{trailing}` follows the index column `{field}` and its direction"
                ));
            }
            let field = Name::parse(field)?;
            if !fields.iter().any(|declared| declared.name == field) {
                return Err(format!(
                    "index column `{field}` is not a declared field.\n       fix: index one of: \
                     {}.",
                    fields
                        .iter()
                        .map(|f| f.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            if columns
                .iter()
                .any(|existing: &IndexColumn| existing.field == field)
            {
                return Err(format!("index column `{field}` is repeated"));
            }
            columns.push(IndexColumn { field, direction });
        }
        if columns.is_empty() {
            return Err("an index needs at least one column".to_string());
        }
        Ok(Self { columns })
    }

    pub fn canonical(&self) -> String {
        self.columns
            .iter()
            .map(|column| match column.direction {
                IndexDirection::Ascending => column.field.to_string(),
                IndexDirection::Descending => format!("{} desc", column.field),
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    pub fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.count(self.columns.len())?;
        for column in &self.columns {
            column.field.encode(encoder)?;
            encoder.tag(match column.direction {
                IndexDirection::Ascending => 0,
                IndexDirection::Descending => 1,
            });
        }
        Ok(())
    }

    pub fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        let count = decoder.count()?;
        let mut columns = Vec::new();
        for _ in 0..count {
            let field = Name::decode(decoder)?;
            let direction = match decoder.tag()? {
                0 => IndexDirection::Ascending,
                1 => IndexDirection::Descending,
                other => return Err(format!("unknown index direction tag {other}")),
            };
            columns.push(IndexColumn { field, direction });
        }
        if columns.is_empty() {
            return Err("an index needs at least one column".to_string());
        }
        Ok(Self { columns })
    }
}

/// Everything a persistent intent was declared to be, minus its identity.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct IntentSpec {
    pub fields: Vec<FieldSpec>,
    pub indexes: Vec<IndexSpec>,
    pub timestamps: bool,
    /// Typed references. `ResolvedRef` arrives with R1.2's graph validation;
    /// until then these carry the resolved target type.
    pub on: Option<JavaType>,
    pub yields: Option<JavaType>,
}

impl IntentSpec {
    pub fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.count(self.fields.len())?;
        let mut previous: Option<&Name> = None;
        for field in &self.fields {
            // Field *order* is semantic (it is the record component order), so
            // fields are a list rather than a set -- but two fields may not
            // share a name, and that is checked here rather than trusted.
            if previous == Some(&field.name) {
                return Err(format!("field `{}` is declared twice", field.name));
            }
            previous = Some(&field.name);
            field.encode(encoder)?;
        }
        encoder.count(self.indexes.len())?;
        for index in &self.indexes {
            index.encode(encoder)?;
        }
        encoder.bool(self.timestamps);
        encoder.option(self.on.as_ref(), |e, ty| ty.encode(e))?;
        encoder.option(self.yields.as_ref(), |e, ty| ty.encode(e))
    }

    pub fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        let field_count = decoder.count()?;
        let mut fields = Vec::new();
        for _ in 0..field_count {
            fields.push(FieldSpec::decode(decoder)?);
        }
        let index_count = decoder.count()?;
        let mut indexes = Vec::new();
        for _ in 0..index_count {
            indexes.push(IndexSpec::decode(decoder)?);
        }
        Ok(Self {
            fields,
            indexes,
            timestamps: decoder.bool()?,
            on: decoder.option(JavaType::decode)?,
            yields: decoder.option(JavaType::decode)?,
        })
    }

    /// Parse a whole declaration: field tokens first, then index tokens, which
    /// are validated against the fields.
    pub fn parse(
        field_tokens: &[String],
        index_tokens: &[String],
        timestamps: bool,
        base: &Package,
    ) -> Result<Self> {
        let mut fields = Vec::new();
        for token in field_tokens {
            let field = FieldSpec::parse(token, base)?;
            if fields
                .iter()
                .any(|existing: &FieldSpec| existing.name == field.name)
            {
                return Err(format!("field `{}` is declared twice", field.name));
            }
            fields.push(field);
        }
        let mut indexes = Vec::new();
        for token in index_tokens {
            let index = IndexSpec::parse(token, &fields)?;
            let mut seen: Option<&IndexColumn> = None;
            for column in &index.columns {
                let _ = ordered(seen.map(|c| &c.field), &column.field);
                seen = Some(column);
            }
            if indexes.contains(&index) {
                return Err(format!("index `{}` is declared twice", index.canonical()));
            }
            indexes.push(index);
        }
        Ok(Self {
            fields,
            indexes,
            timestamps,
            on: None,
            yields: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Package {
        Package::parse("com.example.demo.domain").unwrap()
    }

    fn field(token: &str) -> FieldSpec {
        FieldSpec::parse(token, &base()).unwrap()
    }

    /// One spelling, one value. Today these survive as distinct strings into
    /// the ledger, so a comparison between two of them reports a change
    /// nobody made.
    #[test]
    fn every_spelling_of_a_builtin_normalises_to_one_scalar() {
        for group in [
            vec!["string", "text", "String"],
            vec!["int", "integer", "Integer"],
            vec!["decimal", "bigdecimal", "BigDecimal"],
            vec!["zone-id", "zoneid", "ZoneId"],
            vec!["date", "LocalDate"],
            vec!["uuid", "UUID"],
        ] {
            let first = ScalarFieldType::parse(group[0], &base()).unwrap();
            for spelling in &group[1..] {
                assert_eq!(
                    ScalarFieldType::parse(spelling, &base()).unwrap(),
                    first,
                    "{spelling} should mean the same as {}",
                    group[0]
                );
            }
            // And all of them render as the one canonical spelling.
            for spelling in &group {
                assert_eq!(
                    ScalarFieldType::parse(spelling, &base())
                        .unwrap()
                        .canonical(),
                    first.canonical()
                );
            }
        }
    }

    #[test]
    fn a_capitalised_type_is_one_the_project_owns_and_is_fully_qualified() {
        let scalar = ScalarFieldType::parse("SourceRef", &base()).unwrap();
        assert_eq!(scalar.canonical(), "com.example.demo.domain.SourceRef");

        // Already qualified stays as written.
        let explicit = ScalarFieldType::parse("com.other.Thing", &base()).unwrap();
        assert_eq!(explicit.canonical(), "com.other.Thing");

        // Lowercase and unknown is a typo, not a project type.
        let error = ScalarFieldType::parse("strng", &base()).unwrap_err();
        assert!(error.contains("unknown field type"), "{error}");
        assert!(error.contains("capitalise it"), "{error}");
    }

    /// `map<string,double>` is a documented type, and a naive split cuts the
    /// field spec that most needs recording in half.
    #[test]
    fn a_comma_inside_a_map_does_not_end_the_key() {
        let spec = field("totals:map<string,double>");
        assert_eq!(spec.field_type.canonical(), "map<string,double>");
        match &spec.field_type {
            FieldType::Map { key, value } => {
                assert_eq!(*key, ScalarFieldType::Text);
                assert_eq!(*value, ScalarFieldType::Double);
            }
            other => panic!("{other:?}"),
        }
    }

    /// There is no column or component shape for a nested collection, so it is
    /// refused at the declaration rather than compiled into broken Java.
    #[test]
    fn a_nested_collection_is_refused_where_it_is_written() {
        for token in ["tags:list<list<string>>", "m:map<string,list<int>>"] {
            let error = FieldSpec::parse(token, &base()).unwrap_err();
            assert!(error.contains("nests a collection"), "{token}: {error}");
            assert!(error.contains("fix:"), "{token}: {error}");
        }
    }

    /// Each of these reads like it means something and does not.
    #[test]
    fn a_suffix_that_has_no_meaning_for_the_type_is_refused() {
        let blank = FieldSpec::parse("count:int!", &base()).unwrap_err();
        assert!(blank.contains("not text"), "{blank}");

        let nullable_list = FieldSpec::parse("tags:list<string>?", &base()).unwrap_err();
        assert!(nullable_list.contains("null collection"), "{nullable_list}");

        let nullable_key = FieldSpec::parse("id:uuid?@pk", &base()).unwrap_err();
        assert!(nullable_key.contains("not a key"), "{nullable_key}");

        let numeric_text = FieldSpec::parse("title:string@positive", &base()).unwrap_err();
        assert!(numeric_text.contains("not numeric"), "{numeric_text}");
    }

    /// `!` on text and `?` on a scalar are the shapes that do mean something.
    #[test]
    fn the_suffixes_that_do_apply_are_accepted() {
        assert_eq!(field("title:string!").optionality, Optionality::NonBlank);
        assert_eq!(field("memo:string?").optionality, Optionality::Nullable);
        assert_eq!(field("count:int").optionality, Optionality::Required);
        assert_eq!(
            field("amount:decimal@positive").constraints.numeric,
            Some(NumericConstraint::Positive)
        );
    }

    /// An unknown marker is an error, not a no-op. `@primary` silently meaning
    /// "no constraint" is the failure this feature exists to remove.
    #[test]
    fn an_unknown_constraint_is_an_error_rather_than_silence() {
        let error = FieldSpec::parse("id:uuid@primary", &base()).unwrap_err();
        assert!(error.contains("unknown constraint `@primary`"), "{error}");
        assert!(error.contains("quietly lack a constraint"), "{error}");
    }

    #[test]
    fn marker_order_is_irrelevant_but_a_contradiction_refuses() {
        let one = field("id:uuid@pk@unique@index");
        let other = field("id:uuid@index@unique@pk");
        assert_eq!(one.constraints, other.constraints);
        assert_eq!(one.canonical(), other.canonical());

        let clash = FieldSpec::parse("n:int@positive@nonnegative", &base()).unwrap_err();
        assert!(clash.contains("contradicts"), "{clash}");

        let repeated = FieldSpec::parse("id:uuid@pk@pk", &base()).unwrap_err();
        assert!(repeated.contains("repeated"), "{repeated}");
    }

    /// Two declarations that mean the same thing render the same bytes.
    #[test]
    fn canonical_form_collapses_equivalent_declarations() {
        for (written, canonical) in [
            ("title:text!", "title:string!"),
            ("n:integer@nonnegative", "n:int@nonnegative"),
            ("id:UUID@pk", "id:uuid@pk"),
            ("at:LocalDateTime", "at:datetime"),
            ("t:map<text,BigDecimal>", "t:map<string,decimal>"),
        ] {
            assert_eq!(field(written).canonical(), canonical, "{written}");
            // And the canonical form parses back to the identical value.
            assert_eq!(field(written), field(canonical));
        }
    }

    /// Column order is semantic: `(a, b)` is not the index on `(b, a)`.
    #[test]
    fn index_column_order_is_kept() {
        let fields = vec![field("a:string"), field("b:string")];
        let one = IndexSpec::parse("a, b", &fields).unwrap();
        let other = IndexSpec::parse("b, a", &fields).unwrap();
        assert_ne!(one, other);
        assert_eq!(one.canonical(), "a, b");
    }

    /// The pass-through tail this replaces would have persisted arbitrary text
    /// as trusted generated SQL.
    #[test]
    fn only_asc_and_desc_may_follow_an_index_column() {
        let fields = vec![field("created_at:datetime"), field("title:string")];
        assert_eq!(
            IndexSpec::parse("created_at desc", &fields)
                .unwrap()
                .canonical(),
            "created_at desc"
        );
        assert_eq!(
            IndexSpec::parse("created_at asc", &fields)
                .unwrap()
                .canonical(),
            "created_at"
        );

        let error = IndexSpec::parse("created_at desc nulls last", &fields).unwrap_err();
        assert!(error.contains("follows the index column"), "{error}");

        let sql = IndexSpec::parse("created_at) where deleted = false --", &fields).unwrap_err();
        assert!(!sql.is_empty(), "arbitrary SQL must not be accepted");
    }

    /// An unknown column refuses here, not at `flyway migrate`.
    #[test]
    fn an_index_on_a_field_that_was_not_declared_refuses() {
        let fields = vec![field("title:string")];
        let error = IndexSpec::parse("author", &fields).unwrap_err();
        assert!(error.contains("not a declared field"), "{error}");
        assert!(
            error.contains("title"),
            "the fix names what is available: {error}"
        );

        let repeated = IndexSpec::parse("title, title", &fields).unwrap_err();
        assert!(repeated.contains("repeated"), "{repeated}");
    }

    #[test]
    fn a_duplicate_field_name_refuses() {
        let error = IntentSpec::parse(
            &["a:string".to_string(), "a:int".to_string()],
            &[],
            false,
            &base(),
        )
        .unwrap_err();
        assert!(error.contains("declared twice"), "{error}");
    }

    #[test]
    fn an_intent_spec_round_trips_through_the_codec() {
        let spec = IntentSpec::parse(
            &[
                "id:uuid@pk".to_string(),
                "title:string!@index".to_string(),
                "totals:map<string,double>".to_string(),
                "memo:string?".to_string(),
                "owner:SourceRef".to_string(),
                "amount:decimal@positive".to_string(),
            ],
            &["title, id desc".to_string()],
            true,
            &base(),
        )
        .unwrap();

        let mut encoder = Encoder::new();
        spec.encode(&mut encoder).unwrap();
        let bytes = encoder.finish().unwrap();

        let mut decoder = Decoder::new(&bytes).unwrap();
        let back = IntentSpec::decode(&mut decoder).unwrap();
        decoder.finish().unwrap();
        assert_eq!(back, spec);
    }

    /// The same declaration written two ways encodes to identical bytes, which
    /// is what makes "has this changed?" answerable.
    #[test]
    fn equivalent_declarations_encode_to_identical_bytes() {
        let encode = |tokens: &[&str]| {
            let owned: Vec<String> = tokens.iter().map(|t| t.to_string()).collect();
            let spec = IntentSpec::parse(&owned, &[], false, &base()).unwrap();
            let mut encoder = Encoder::new();
            spec.encode(&mut encoder).unwrap();
            encoder.finish().unwrap()
        };
        assert_eq!(
            encode(&["id:UUID@pk@unique", "title:text!"]),
            encode(&["id:uuid@unique@pk", "title:string!"])
        );
    }

    #[test]
    fn an_unknown_tag_rejects_rather_than_being_skipped() {
        let mut decoder = Decoder::new(&[99]).unwrap();
        assert!(
            FieldType::decode(&mut decoder)
                .unwrap_err()
                .contains("unknown field type tag")
        );
    }
}
