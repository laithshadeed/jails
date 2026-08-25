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

mod field;
mod index;

pub use field::{
    FieldConstraints, FieldSpec, FieldType, NumericConstraint, Optionality, ScalarFieldType,
};
pub(crate) use index::{IndexColumn, IndexSpec};

use crate::Result;
use crate::entity::Recipe;
use crate::identity::{JavaType, Name, Package};
use crate::recipe::ArgumentShape;
use jails_support::codec::{Codec, Decoder, Encoder, ordered};

/// One `childField=parentField` pair, which only `association` declares.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FieldMapping {
    pub child: Name,
    pub parent: Name,
}

impl FieldMapping {
    /// `childField=parentField`. Both halves are `Name`s, so a mapping cannot
    /// smuggle a SQL fragment through as a column reference.
    pub fn parse(token: &str) -> Result<Self> {
        let (child, parent) = token.split_once('=').ok_or_else(|| {
            format!(
                "`{token}` is not a mapping.\n       fix: each argument is \
                 `childField=parentField`."
            )
        })?;
        Ok(Self {
            child: Name::parse(child.trim())?,
            parent: Name::parse(parent.trim())?,
        })
    }

    pub fn canonical(&self) -> String {
        format!("{}={}", self.child, self.parent)
    }
}
impl Codec for FieldMapping {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.child.encode(encoder)?;
        self.parent.encode(encoder)
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(Self {
            child: Name::decode(decoder)?,
            parent: Name::decode(decoder)?,
        })
    }
}

/// A recipe's positional arguments, in the shape that recipe takes.
///
/// plan.md §R1.1's amendment. Which variant a spec holds is a total function
/// of the identity's recipe -- [`ArgumentShape`] -- and never a guess made by
/// looking at the tokens. Order is semantic inside every variant and is never
/// sorted: it is the record component order, the enum constant order, the
/// permits order and the mapping order, each of which changes the Java or the
/// DDL that comes out.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IntentArguments {
    Fields(Vec<FieldSpec>),
    Names(Vec<Name>),
    Mappings(Vec<FieldMapping>),
}

impl Default for IntentArguments {
    fn default() -> Self {
        Self::Fields(Vec::new())
    }
}

impl IntentArguments {
    pub fn shape(&self) -> ArgumentShape {
        match self {
            Self::Fields(_) => ArgumentShape::Fields,
            Self::Names(_) => ArgumentShape::Names,
            Self::Mappings(_) => ArgumentShape::Mappings,
        }
    }

    /// The declared fields, or nothing when this recipe does not take any.
    ///
    /// Deliberately not an `Option`: every caller that wants fields wants to
    /// do nothing when there are none, and a recipe whose arguments are names
    /// has no fields in exactly the same sense a recipe with no arguments has
    /// none.
    pub fn fields(&self) -> &[FieldSpec] {
        match self {
            Self::Fields(fields) => fields,
            _ => &[],
        }
    }

    pub fn is_empty(&self) -> bool {
        match self {
            Self::Fields(items) => items.is_empty(),
            Self::Names(items) => items.is_empty(),
            Self::Mappings(items) => items.is_empty(),
        }
    }

    /// One canonical spelling per argument, whatever was typed.
    pub fn canonical(&self) -> Vec<String> {
        match self {
            Self::Fields(items) => items.iter().map(FieldSpec::canonical).collect(),
            Self::Names(items) => items.iter().map(|name| name.to_string()).collect(),
            Self::Mappings(items) => items.iter().map(FieldMapping::canonical).collect(),
        }
    }

    /// Parse a token list into the shape `recipe` takes.
    pub fn parse(recipe: Recipe, tokens: &[String], base: &Package) -> Result<Self> {
        match crate::recipe::argument_shape(recipe) {
            ArgumentShape::Fields => {
                let mut fields: Vec<FieldSpec> = Vec::new();
                for token in tokens {
                    let field = FieldSpec::parse(token, base)?;
                    if fields.iter().any(|existing| existing.name == field.name) {
                        return Err(format!("field `{}` is declared twice", field.name).into());
                    }
                    fields.push(field);
                }
                Ok(Self::Fields(fields))
            }
            ArgumentShape::Names => {
                let mut names: Vec<Name> = Vec::new();
                for token in tokens {
                    let name = Name::parse(token.trim())?;
                    if names.contains(&name) {
                        return Err(format!("`{name}` is declared twice").into());
                    }
                    names.push(name);
                }
                Ok(Self::Names(names))
            }
            ArgumentShape::Mappings => {
                let mut mappings: Vec<FieldMapping> = Vec::new();
                for token in tokens {
                    let mapping = FieldMapping::parse(token)?;
                    if mappings.iter().any(|held| held.child == mapping.child) {
                        return Err(format!("`{}` is mapped twice", mapping.child).into());
                    }
                    mappings.push(mapping);
                }
                Ok(Self::Mappings(mappings))
            }
        }
    }

    fn tag(&self) -> u8 {
        match self {
            Self::Fields(_) => 0,
            Self::Names(_) => 1,
            Self::Mappings(_) => 2,
        }
    }
}
impl Codec for IntentArguments {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.tag(self.tag());
        match self {
            Self::Fields(fields) => {
                encoder.count(fields.len())?;
                let mut previous: Option<&Name> = None;
                for field in fields {
                    // Field *order* is semantic (it is the record component
                    // order), so fields are a list rather than a set -- but
                    // two fields may not share a name, and that is checked
                    // here rather than trusted.
                    if previous == Some(&field.name) {
                        return Err(format!("field `{}` is declared twice", field.name).into());
                    }
                    previous = Some(&field.name);
                    field.encode(encoder)?;
                }
            }
            Self::Names(names) => {
                encoder.count(names.len())?;
                let mut previous: Option<&Name> = None;
                for name in names {
                    if previous == Some(name) {
                        return Err(format!("`{name}` is declared twice").into());
                    }
                    previous = Some(name);
                    name.encode(encoder)?;
                }
            }
            Self::Mappings(mappings) => {
                encoder.count(mappings.len())?;
                let mut previous: Option<&Name> = None;
                for mapping in mappings {
                    if previous == Some(&mapping.child) {
                        return Err(format!("`{}` is mapped twice", mapping.child).into());
                    }
                    previous = Some(&mapping.child);
                    mapping.encode(encoder)?;
                }
            }
        }
        Ok(())
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        let tag = decoder.tag()?;
        let count = decoder.count()?;
        Ok(match tag {
            0 => {
                let mut fields = Vec::new();
                for _ in 0..count {
                    fields.push(FieldSpec::decode(decoder)?);
                }
                Self::Fields(fields)
            }
            1 => {
                let mut names = Vec::new();
                for _ in 0..count {
                    names.push(Name::decode(decoder)?);
                }
                Self::Names(names)
            }
            2 => {
                let mut mappings = Vec::new();
                for _ in 0..count {
                    mappings.push(FieldMapping::decode(decoder)?);
                }
                Self::Mappings(mappings)
            }
            other => return Err(format!("unknown intent argument tag {other}").into()),
        })
    }
}

/// Everything a persistent intent was declared to be, minus its identity.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct IntentSpec {
    pub arguments: IntentArguments,
    pub indexes: Vec<IndexSpec>,
    pub timestamps: bool,
    /// Typed references. `ResolvedRef` arrives with R1.2's graph validation;
    /// until then these carry the resolved target type.
    pub on: Option<JavaType>,
    pub yields: Option<JavaType>,
    /// The HTTP method a generated endpoint answers.
    ///
    /// Content rather than identity, like every other field here: changing
    /// `g controller Foo --method post` to `--method put` is an *edit* to a
    /// known entity, so the regenerate-and-merge repair applies and the class
    /// is not orphaned and rewritten from nothing.
    ///
    /// `None` is not "GET". It is "this recipe was never asked", which is
    /// what every recipe that has no endpoint holds -- and the renderer's
    /// default is stated where the default belongs, at the one call site.
    pub method: Option<jails_spec::spec::kind::HttpMethod>,
}

impl IntentSpec {
    /// The declared fields, or nothing when this recipe takes names or
    /// mappings instead.
    pub fn fields(&self) -> &[FieldSpec] {
        self.arguments.fields()
    }

    /// Parse a whole declaration: positional tokens in the shape this recipe
    /// takes, then index tokens, which are validated against the fields.
    ///
    /// The recipe is a parameter rather than something inferred from the
    /// tokens, which is the whole point of §R1.1's amendment: `ACTIVE` is a
    /// valid enum constant and an invalid field, and which of those it is has
    /// to be decided by the command that was run.
    pub fn parse(
        recipe: Recipe,
        argument_tokens: &[String],
        index_tokens: &[String],
        timestamps: bool,
        base: &Package,
    ) -> Result<Self> {
        Self::from_arguments(
            recipe,
            IntentArguments::parse(recipe, argument_tokens, base)?,
            index_tokens,
            timestamps,
        )
    }

    /// The same declaration, from arguments somebody has already parsed.
    ///
    /// The caller that needs this is the one translating `--index created_at`
    /// into the field it names: that translation needs the fields, so the
    /// arguments have to be parsed before the indexes can be. Handing the
    /// parsed value back in is what stops the tokens being parsed a second
    /// time here -- and it keeps [`Self::parse`] the one authority on what a
    /// valid declaration is, because that is now this function.
    pub fn from_arguments(
        recipe: Recipe,
        arguments: IntentArguments,
        index_tokens: &[String],
        timestamps: bool,
    ) -> Result<Self> {
        if !index_tokens.is_empty() && arguments.shape() != ArgumentShape::Fields {
            return Err(format!(
                "`{}` does not take fields, so there is nothing for an index to name.",
                crate::entity::recipe_label(recipe)
            )
            .into());
        }
        let fields = arguments.fields();
        let mut indexes = Vec::new();
        for token in index_tokens {
            let index = IndexSpec::parse(token, fields)?;
            let mut seen: Option<&IndexColumn> = None;
            for column in &index.columns {
                let _ = ordered(seen.map(|c| &c.field), &column.field);
                seen = Some(column);
            }
            if indexes.contains(&index) {
                return Err(format!("index `{}` is declared twice", index.canonical()).into());
            }
            indexes.push(index);
        }
        Ok(Self {
            arguments,
            indexes,
            timestamps,
            on: None,
            yields: None,
            method: None,
        })
    }
}
impl Codec for IntentSpec {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.arguments.encode(encoder)?;
        encoder.count(self.indexes.len())?;
        for index in &self.indexes {
            index.encode(encoder)?;
        }
        encoder.bool(self.timestamps);
        encoder.option(self.on.as_ref(), |e, ty| ty.encode(e))?;
        encoder.option(self.yields.as_ref(), |e, ty| ty.encode(e))?;
        // By label, not by discriminant: §R1.4's rule for every closed
        // vocabulary on the wire, so reordering the enum cannot change a
        // recorded value.
        encoder.option(self.method.as_ref(), |e, method| e.string(method.label()))
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        let arguments = IntentArguments::decode(decoder)?;
        let index_count = decoder.count()?;
        let mut indexes = Vec::new();
        for _ in 0..index_count {
            indexes.push(IndexSpec::decode(decoder)?);
        }
        Ok(Self {
            arguments,
            indexes,
            timestamps: decoder.bool()?,
            on: decoder.option(JavaType::decode)?,
            yields: decoder.option(JavaType::decode)?,
            method: decoder.option(|d| jails_spec::spec::kind::HttpMethod::parse(&d.string()?))?,
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

    /// The projection and the other parser agree, token for token.
    ///
    /// Two parsers of one user-facing syntax is this repository's most reliable
    /// drift generator, and `pending.md` §6.3 is the entry about it. The
    /// parse-print-reparse bridge is gone -- `FieldSpec::projected` derives the
    /// `Field` from the value instead -- but the two parsers themselves are
    /// still there, so this is what stops them separating: every token below
    /// must reach the same `Field` through both.
    ///
    /// A failure here means one parser learned something the other did not.
    /// The fix is to teach the projection, never to relax the assertion.
    #[test]
    fn a_projected_field_spec_equals_the_parsed_one() {
        let tokens = [
            "title:string",
            "title:string!",
            "note:string?",
            "count:int",
            "total:long@positive",
            "balance:decimal@nonnegative",
            "id:uuid@pk",
            "email:string@unique",
            "owner:string@index",
            "tenant:string@scope",
            "at:instant",
            "on:date",
            "seen:datetime",
            "ok:boolean",
            "ratio:double",
            "money:currency",
            "blob:bytes",
            "took:duration",
            "zone:zone-id",
            "href:uri",
            "where:path",
            "tags:list<string>",
            "counts:map<string,int>",
            "id:String",
            "when:Instant",
            "key:uuid@pk@index",
        ];
        for token in tokens {
            let projected = field(token)
                .projected()
                .unwrap_or_else(|e| panic!("{token} does not project: {e}"));
            let parsed = jails_spec::spec::parse_fields(&[token.to_string()])
                .unwrap_or_else(|e| panic!("{token} does not parse: {e}"))
                .pop()
                .expect("one token, one field");
            assert_eq!(projected.name, parsed.name, "{token}: name");
            assert_eq!(projected.java_type, parsed.java_type, "{token}: java type");
            assert_eq!(projected.imports, parsed.imports, "{token}: imports");
            assert_eq!(
                projected.optionality, parsed.optionality,
                "{token}: optionality"
            );
            assert_eq!(projected.owned, parsed.owned, "{token}: owned");
            assert_eq!(
                projected.collection, parsed.collection,
                "{token}: collection"
            );
            assert_eq!(
                projected.constraints, parsed.constraints,
                "{token}: constraints"
            );
        }
    }

    /// A refusal one parser makes, the other makes too.
    ///
    /// The projection reruns the checks that need a *resolved Java type* --
    /// `!` on something that is not text, `@positive` on a non-numeric column
    /// -- because only `resolve_type` knows what the type became. A spec that
    /// `FieldSpec::parse` lets through and `derive_field` refuses would be a
    /// request accepted at the edge and rejected mid-transition.
    #[test]
    fn a_spec_the_projection_refuses_is_one_the_parser_refuses() {
        for token in [
            "title:uuid!",
            "tags:list<string>?",
            "id:uuid@pk",
            "at:instant@positive",
        ] {
            let projected = FieldSpec::parse(token, &base()).and_then(|spec| spec.projected());
            let parsed = jails_spec::spec::parse_fields(&[token.to_string()]);
            assert_eq!(
                projected.is_err(),
                parsed.is_err(),
                "{token}: projection {:?}, parser {:?}",
                projected.map(|_| ()),
                parsed.map(|_| ())
            );
        }
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
            Recipe::Record,
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
            Recipe::Scaffold,
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
            let spec = IntentSpec::parse(Recipe::Record, &owned, &[], false, &base()).unwrap();
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

    /// §R1.1's amendment, at the constructor. The recipe decides what the
    /// positional list is, and `ACTIVE` is a good enum constant and a bad
    /// field -- so which it is cannot be decided by looking at it.
    #[test]
    fn a_recipe_decides_what_its_positional_arguments_are() {
        let constants = ["ACTIVE".to_string(), "CLOSED".to_string()];
        let spec = IntentSpec::parse(Recipe::Enum, &constants, &[], false, &base()).unwrap();
        assert_eq!(spec.arguments.canonical(), vec!["ACTIVE", "CLOSED"]);
        assert!(spec.fields().is_empty(), "an enum declares no components");

        let error = IntentSpec::parse(Recipe::Record, &constants, &[], false, &base()).unwrap_err();
        assert!(error.contains("needs a `name:type`"), "{error}");
    }

    /// The mapping shape, which only `association` takes.
    #[test]
    fn an_association_declares_mappings_and_nothing_else() {
        let spec = IntentSpec::parse(
            Recipe::Association,
            &["orderId=id".to_string(), "tenantId=tenantId".to_string()],
            &[],
            false,
            &base(),
        )
        .unwrap();
        assert_eq!(
            spec.arguments.canonical(),
            vec!["orderId=id", "tenantId=tenantId"]
        );

        let error = IntentSpec::parse(
            Recipe::Association,
            &["orderId".to_string()],
            &[],
            false,
            &base(),
        )
        .unwrap_err();
        assert!(error.contains("childField=parentField"), "{error}");
    }

    /// Order is semantic in every shape, and a round trip preserves it. An
    /// enum whose constants came back sorted would renumber `ordinal()` for
    /// every one of them.
    #[test]
    fn every_argument_shape_survives_a_round_trip_in_its_declared_order() {
        for (recipe, tokens) in [
            (
                Recipe::Record,
                vec!["title:string!".to_string(), "at:instant".to_string()],
            ),
            (
                Recipe::Sealed,
                vec!["Rejected".to_string(), "Accepted".to_string()],
            ),
            (
                Recipe::Association,
                vec!["zebra=id".to_string(), "alpha=id".to_string()],
            ),
        ] {
            let spec = IntentSpec::parse(recipe, &tokens, &[], false, &base()).unwrap();
            let mut encoder = Encoder::new();
            spec.encode(&mut encoder).unwrap();
            let bytes = encoder.finish().unwrap();
            let back = IntentSpec::decode(&mut Decoder::new(&bytes).unwrap()).unwrap();
            assert_eq!(back, spec, "{recipe:?}");
            assert_eq!(
                back.arguments.canonical(),
                spec.arguments.canonical(),
                "{recipe:?}: the declared order is not a set"
            );
        }
    }

    /// An index names a field, so a recipe with no fields has nothing to
    /// index. Accepting it would record an index over a column that does not
    /// exist and fail at `flyway migrate`.
    #[test]
    fn an_index_on_a_recipe_that_takes_no_fields_is_refused() {
        let error = IntentSpec::parse(
            Recipe::Enum,
            &["ACTIVE".to_string()],
            &["ACTIVE".to_string()],
            false,
            &base(),
        )
        .unwrap_err();
        assert!(error.contains("does not take fields"), "{error}");
    }
}
