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

mod constant;
mod field;
mod index;

pub use constant::ConstantSpec;
pub use field::{
    FieldConstraints, FieldSpec, FieldType, NumericConstraint, Optionality, ScalarFieldType,
    parse_fields,
};
pub use index::{IndexColumn, IndexDirection, IndexSpec};

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
jails_support::codec!(struct FieldMapping { child, parent });

/// One component pinned to a constant instead of taken from the request.
///
/// `--set senderType=ADMIN`. Both halves are validated values: a `Name` for
/// the component, so a pin cannot name a SQL fragment, and a `LiteralValue`
/// for what it holds, so it cannot name a Java expression. The generator
/// resolves the literal against the component's *declared type* -- an enum
/// constant that is not one of that enum's constants is refused there, where
/// the type is known, rather than written into a constructor argument and
/// discovered by the compiler.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct PinSpec {
    pub component: Name,
    pub value: crate::identity::LiteralValue,
}

impl PinSpec {
    /// `component=literal`.
    pub fn parse(token: &str) -> Result<Self> {
        let (component, value) = token.split_once('=').ok_or_else(|| {
            format!(
                "`{token}` is not a pinned value.\n       fix: each `--set` is \
                 `component=literal`, for example `--set senderType=ADMIN`."
            )
        })?;
        Ok(Self {
            component: Name::parse(component.trim())?,
            value: crate::identity::LiteralValue::parse(value.trim())?,
        })
    }

    pub fn canonical(&self) -> String {
        format!("{}={}", self.component, self.value)
    }
}
impl Codec for PinSpec {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.component.encode(encoder)?;
        self.value.encode(encoder)
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(Self {
            component: Name::decode(decoder)?,
            value: crate::identity::LiteralValue::decode(decoder)?,
        })
    }
}

/// One component bound from a request parameter of a different name.
///
/// `--bind id=message_id`. Spring's data binder has no naming strategy, and
/// the derived one -- the project's Jackson strategy -- cannot cover a value
/// that is `id` in the response and `message_id` in the request. Both halves
/// are validated values: a `Name` for the component and a `WireName` for what
/// arrives, so a binding cannot smuggle anything into an annotation.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct BindSpec {
    pub component: Name,
    pub wire: crate::identity::WireName,
}

impl BindSpec {
    /// `component=parameter`.
    pub fn parse(token: &str) -> Result<Self> {
        let (component, wire) = token.split_once('=').ok_or_else(|| {
            format!(
                "`{token}` is not a binding.\n       fix: each `--bind` is \
                 `component=parameter`, for example `--bind id=message_id`."
            )
        })?;
        Ok(Self {
            component: Name::parse(component.trim())?,
            wire: crate::identity::WireName::parse(wire.trim())?,
        })
    }

    pub fn canonical(&self) -> String {
        format!("{}={}", self.component, self.wire)
    }
}
impl Codec for BindSpec {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.component.encode(encoder)?;
        self.wire.encode(encoder)
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(Self {
            component: Name::decode(decoder)?,
            wire: crate::identity::WireName::decode(decoder)?,
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
    /// An `enum`'s constants, each of which may say what it is called on the
    /// wire. Separate from `Names` because only this recipe has a wire at all
    /// -- a `sealed` variant and a `strategy` implementation are class names
    /// and nothing else. `missing.md` M14.
    Constants(Vec<ConstantSpec>),
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
            Self::Constants(_) => ArgumentShape::Constants,
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
            Self::Constants(items) => items.is_empty(),
            Self::Mappings(items) => items.is_empty(),
        }
    }

    /// One canonical spelling per argument, whatever was typed.
    pub fn canonical(&self) -> Vec<String> {
        match self {
            Self::Fields(items) => items.iter().map(FieldSpec::canonical).collect(),
            Self::Names(items) => items.iter().map(|name| name.to_string()).collect(),
            Self::Constants(items) => items.iter().map(ConstantSpec::canonical).collect(),
            Self::Mappings(items) => items.iter().map(FieldMapping::canonical).collect(),
        }
    }

    /// Parse a token list into the shape `recipe` takes.
    pub fn parse(recipe: Recipe, tokens: &[String], base: &Package) -> Result<Self> {
        match crate::recipe::argument_shape(recipe) {
            ArgumentShape::Fields => {
                let fields = tokens
                    .iter()
                    .map(|token| FieldSpec::parse(token, base))
                    .collect::<Result<Vec<_>>>()?;
                field::validate_field_names(&fields)?;
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
            ArgumentShape::Constants => {
                let mut constants: Vec<ConstantSpec> = Vec::new();
                for token in tokens {
                    let constant = ConstantSpec::parse(token)?;
                    if constants.iter().any(|held| held.name == constant.name) {
                        return Err(format!(
                            "`{}` is declared twice.\n       fix: drop one of them -- two \
                             tokens can reach one constant, since `gbp` and `GBP` are the \
                             same name.",
                            constant.name
                        )
                        .into());
                    }
                    if let Some(clash) = constants
                        .iter()
                        .find(|held| held.wire_value() == constant.wire_value())
                    {
                        // Two constants one wire value: whichever arrives is
                        // decoded as one of them and the other is unreachable,
                        // silently.
                        return Err(format!(
                            "`{}` and `{}` are both called `{}` on the wire.\n       fix: give \
                             them different wire values.",
                            clash.name,
                            constant.name,
                            constant.wire_value()
                        )
                        .into());
                    }
                    constants.push(constant);
                }
                Ok(Self::Constants(constants))
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
            Self::Constants(_) => 3,
        }
    }
}
impl Codec for IntentArguments {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.tag(self.tag());
        match self {
            Self::Fields(fields) => {
                field::validate_field_names(fields)?;
                encoder.count(fields.len())?;
                for field in fields {
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
            Self::Constants(constants) => {
                encoder.count(constants.len())?;
                let mut previous: Option<&Name> = None;
                for constant in constants {
                    if previous == Some(&constant.name) {
                        return Err(format!("`{}` is declared twice", constant.name).into());
                    }
                    previous = Some(&constant.name);
                    constant.encode(encoder)?;
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
            3 => {
                let mut constants = Vec::new();
                for _ in 0..count {
                    constants.push(ConstantSpec::decode(decoder)?);
                }
                Self::Constants(constants)
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
    /// The resource a `query` joins through to reach a filter it does not own.
    ///
    /// `g query ... --on Message --via User` reads `users` alongside
    /// `messages`, so a filter may name a component of either. Content rather
    /// than identity, the same as `on` and `yields`: adding the join to an
    /// existing query is an edit to a known entity, not a new one.
    ///
    /// The **parent type**, not the association's name. An association records
    /// its mapping only in the migration it wrote, and re-reading generated
    /// SQL to recover a decision is exactly the guessing `build.rs` refuses to
    /// do with a build file. The join column is derived from the two records
    /// instead, and refused when more than one component could be it.
    pub via: Option<JavaType>,
    /// A `query`'s explicit result order, as components of the target.
    ///
    /// Empty means the adapter's own rule -- newest first with the key as the
    /// tiebreak (`sql::ordering`). Recorded because it is content: changing it
    /// is an edit to a known entity, and a regeneration has to reproduce it.
    ///
    /// Shape-validated here and resolved against the target's components in
    /// the generator, the same split `on` and `yields` have.
    pub order_by: Vec<IndexColumn>,
    /// A `query`'s explicit row ceiling. `None` is the adapter's default of
    /// 100, stated at the one place that renders it.
    pub limit: Option<u32>,
    /// The target component whose unique constraint makes a `usecase` a
    /// get-or-create rather than an insert.
    ///
    /// Content, like every other reference here: adding it to an existing
    /// intent is an edit the three-way merge carries, not a new entity.
    pub on_conflict: Option<crate::identity::Name>,
    /// The route a generated endpoint answers, when the caller names one
    /// instead of taking the derived shape. `missing.md` M8.
    pub path: Option<crate::identity::RoutePath>,
    /// Which component identifies the row a `transition` updates.
    ///
    /// Content, not identity: changing `--select id` to `--select userId` is
    /// an edit to a known entity. Recorded because a regeneration has to
    /// reproduce it -- `g field` re-derives every companion of the record it
    /// touches, and a selector that came back as the default would flip the
    /// adapter's `where` clause to a different column without saying so.
    pub select: Option<Name>,
    /// Whether a `transition` insists on the caller's `If-Match`, or only
    /// checks one when it arrives.
    ///
    /// `None` is "not asked", never `Required`: the default belongs at the one
    /// place that renders the header parameter, not spread across every recipe
    /// that has no precondition to describe.
    pub if_match: Option<jails_spec::spec::kind::Precondition>,
    /// Components this recipe pins to a constant rather than reading from the
    /// request.
    ///
    /// Empty is "the caller supplies every component", which is what every
    /// recipe held before `--set` existed. Content for the same reason
    /// `order_by` is, and shape-validated here while the *meaning* -- is
    /// `ADMIN` a constant of this component's enum? -- is resolved in the
    /// generator, where the target's declared types are known.
    pub pins: Vec<PinSpec>,
    /// Components bound from a request parameter of a different name.
    ///
    /// Empty is "every component binds by its own name, through the project's
    /// wire naming", which is what every recipe held before `--bind` existed.
    pub binds: Vec<BindSpec>,
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
    /// How a generated endpoint reads the request it is sent.
    ///
    /// Content, not identity, for the same reason `method` is: moving an
    /// endpoint from JSON to form data is an *edit* to a known entity, so the
    /// regenerate-and-merge repair applies rather than orphaning the class.
    ///
    /// `None` is "not asked", not "JSON" -- the default belongs at the one
    /// place that renders a binding annotation. `missing.md` M15.
    pub consumes: Option<jails_spec::spec::kind::WireFormat>,
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
            consumes: None,
            via: None,
            order_by: Vec::new(),
            limit: None,
            on_conflict: None,
            path: None,
            select: None,
            if_match: None,
            pins: Vec::new(),
            binds: Vec::new(),
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
        encoder.option(self.method.as_ref(), |e, method| e.string(method.label()))?;
        encoder.option(self.via.as_ref(), |e, ty| ty.encode(e))?;
        encoder.count(self.order_by.len())?;
        for column in &self.order_by {
            column.field.encode(encoder)?;
            encoder.tag(match column.direction {
                IndexDirection::Ascending => 0,
                IndexDirection::Descending => 1,
            });
        }
        encoder.option(self.limit.as_ref(), |e, limit| e.string(&limit.to_string()))?;
        encoder.option(self.on_conflict.as_ref(), |e, name| name.encode(e))?;
        encoder.option(self.path.as_ref(), |e, path| path.encode(e))?;
        // The label, not the discriminant, on the same rule as `method`: a
        // recorded value must not change meaning when the enum is reordered.
        encoder.option(self.consumes.as_ref(), |e, format| e.string(format.label()))?;
        encoder.option(self.select.as_ref(), |e, name| name.encode(e))?;
        // The label, not the discriminant, on the same rule as `method`.
        encoder.option(self.if_match.as_ref(), |e, policy| e.string(policy.label()))?;
        encoder.count(self.pins.len())?;
        for pin in &self.pins {
            pin.encode(encoder)?;
        }
        encoder.count(self.binds.len())?;
        for bind in &self.binds {
            bind.encode(encoder)?;
        }
        Ok(())
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
            via: decoder.option(JavaType::decode)?,
            order_by: {
                let count = decoder.count()?;
                let mut columns = Vec::new();
                for _ in 0..count {
                    let field = crate::identity::Name::decode(decoder)?;
                    let direction = match decoder.tag()? {
                        0 => IndexDirection::Ascending,
                        1 => IndexDirection::Descending,
                        other => {
                            return Err(format!("unknown index direction tag {other}").into());
                        }
                    };
                    columns.push(IndexColumn { field, direction });
                }
                columns
            },
            limit: decoder
                .option(|d| {
                    d.string()?
                        .parse::<u32>()
                        .map_err(|_| jails_support::Failure::Told("bad query limit".to_string()))
                })
                .map_err(|_| jails_support::Failure::Told("bad query limit".to_string()))?,
            on_conflict: decoder.option(crate::identity::Name::decode)?,
            path: decoder.option(crate::identity::RoutePath::decode)?,
            consumes: decoder
                .option(|d| jails_spec::spec::kind::WireFormat::parse(&d.string()?))?,
            select: decoder.option(crate::identity::Name::decode)?,
            if_match: decoder
                .option(|d| jails_spec::spec::kind::Precondition::parse(&d.string()?))?,
            pins: {
                let count = decoder.count()?;
                let mut pins = Vec::new();
                for _ in 0..count {
                    pins.push(PinSpec::decode(decoder)?);
                }
                pins
            },
            binds: {
                let count = decoder.count()?;
                let mut binds = Vec::new();
                for _ in 0..count {
                    binds.push(BindSpec::decode(decoder)?);
                }
                binds
            },
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

    /// The base package does not reach the derived field.
    ///
    /// This was `a_projected_field_spec_equals_the_parsed_one`, and it ran
    /// twenty-six tokens through *two* parsers of one user-facing syntax and
    /// compared all seven fields of the result. `pending.md` §6.3 merged them,
    /// so that comparison is now a tautology -- but one real assumption came
    /// out of the merge and this is it.
    ///
    /// [`parse_fields`] resolves against [`Package::base`], because a `Field`
    /// records `owned` and a simple `java_type` and no package at all. That is
    /// only sound if the base a spec was parsed against cannot change what it
    /// projects to. So: the same tokens, parsed against a real project package
    /// and against the base one, must derive identical fields.
    ///
    /// A failure here means the projection started reading the package, and
    /// the fix is to give `parse_fields` a base rather than to relax this.
    #[test]
    fn the_base_package_does_not_reach_the_derived_field() {
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
            "day:date",
            "seen:datetime",
            "ok:boolean",
            "ratio:double",
            "money:currency",
            "blob:bytes",
            "took:duration",
            "zone:zone-id",
            "href:uri",
            "file:path",
            "tags:list<string>",
            "counts:map<string,int>",
            "id:String",
            "observedAt:Instant",
            "key:uuid@pk@index",
            // The one that found a live divergence: capitalised means a type
            // the project owns, and `Currency` was being read as the built-in
            // by one parser and as a project enum by the other.
            "paid:Currency",
        ];
        for token in tokens {
            let against_project = field(token)
                .projected()
                .unwrap_or_else(|e| panic!("{token} does not project: {e}"));
            let against_base = parse_fields(&[token.to_string()])
                .unwrap_or_else(|e| panic!("{token} does not parse: {e}"))
                .pop()
                .expect("one token, one field");
            assert_eq!(against_project, against_base, "{token}");
        }
    }

    /// A declaration `FieldSpec::parse` accepts is one `derive_field` accepts.
    ///
    /// The two are one parser now, but they are still two *checks*: the
    /// projection reruns the rules that need a resolved Java type -- `!` on
    /// something that is not text, `@positive` on a non-numeric column --
    /// because only `resolve_type` knows what the type became. A token the
    /// first half lets through and the second refuses would be a request
    /// accepted at the edge and rejected mid-transition, which is the failure
    /// this keeps out.
    #[test]
    fn a_declaration_that_parses_is_one_the_projection_derives() {
        for token in ["title:uuid!", "tags:list<string>?", "at:instant@positive"] {
            assert!(
                FieldSpec::parse(token, &base()).is_err(),
                "{token} should be refused at the edge"
            );
        }
        for token in ["id:uuid@pk", "title:string!", "total:long@positive"] {
            FieldSpec::parse(token, &base())
                .and_then(|spec| spec.projected())
                .unwrap_or_else(|e| panic!("{token} parses but does not derive: {e}"));
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
    fn sql_column_collisions_refuse_at_the_declaration_edge() {
        for (tokens, java, column) in [
            (vec!["id:uuid@pk", "Id:string"], "id", "id"),
            (vec!["userId:uuid", "user_id:string"], "userId", "user_id"),
        ] {
            let tokens = tokens.into_iter().map(str::to_string).collect::<Vec<_>>();
            let error =
                IntentSpec::parse(Recipe::Scaffold, &tokens, &[], false, &base()).unwrap_err();
            assert!(error.contains(java), "{error}");
            assert!(error.contains(column), "{error}");
            assert!(error.contains("declared twice"), "{error}");
        }
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
