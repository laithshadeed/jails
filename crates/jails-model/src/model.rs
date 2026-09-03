//! `AppModel` — desired-state authority, and the only thing the compiler reads.
//!
//! **The model is desired-state authority**: this value is what the
//! application *should* be, stable IDs carry identity, and every
//! Java type, SQL table, column, route and property name is a projection off
//! a label. Nothing in this struct describes a file, a path or a build tool —
//! those are the compiler's answers to what is declared here, and keeping them
//! out is what makes one model render a Maven project and a Gradle one.
//!
//! Everything is a `BTreeMap` keyed by stable ID, which is three properties at
//! once: identity is the key rather than a position, iteration is
//! deterministic so the compiler's output is, and a rename touches a label
//! while the key stays put.
//!
//! There is no mutation path. A model is whatever its source parses and
//! links to (`parse_jdl`), and a command changes the model by editing the
//! source; what the source cannot say about how to get there is an
//! [`crate::Evolution`] passed to the compiler beside the model. A setter
//! here could edit a model into a state no source describes, and the plan's
//! recorded input would then no longer explain the model it produced.

use crate::EnumConstant;
use crate::Operation;
use crate::SourceUnit;
use crate::app::ProjectIntent;
use crate::constraint::EntityConstraint;
use crate::id::{
    CapabilityId, ComponentId, ConstraintId, DependencyId, EjectionId, EntityId, FieldId, IndexId,
    OperationId, ProjectionId, RelationId, SettingId, UnitId,
};
use crate::projection::Projection;
use crate::relation::Relation;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// The only desired-state value consumed by the application compiler.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AppModel {
    pub schema: String,
    /// Which JDL the source was written in, and which convention registry
    /// produced every derived name in it.
    ///
    /// **Stored separately, per JDL v1 §7.2**, so a plan cannot compare two
    /// models produced by different registries and conclude they agree.
    /// `convention_version` is exactly `1` for JDL v1; `language_version`
    /// follows the `jdl <n>` header. Both default to `1` when absent.
    #[serde(default = "one")]
    pub language_version: u16,
    #[serde(default = "one")]
    pub convention_version: u16,
    pub project: ProjectIntent,
    pub capabilities: BTreeMap<CapabilityId, Capability>,
    pub dependencies: BTreeMap<DependencyId, Dependency>,
    pub settings: BTreeMap<SettingId, Setting>,
    pub ejections: BTreeMap<EjectionId, Ejection>,
    #[serde(default)]
    pub units: BTreeMap<UnitId, SourceUnit>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub components: BTreeMap<ComponentId, crate::Component>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub projections: BTreeMap<ProjectionId, Projection>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub relations: BTreeMap<RelationId, Relation>,
    pub entities: BTreeMap<EntityId, Entity>,
    pub operations: BTreeMap<OperationId, Operation>,
    /// Every name the convention decided rather than the author writing it.
    ///
    /// JDL v1 §7.2 puts it in the model and §18.4 makes it inspectable, which
    /// is one requirement rather than two: being *in* the model is what puts
    /// it in the accepted-model and plan digest, so a convention that moves
    /// cannot move silently. See the `derived` module, including which half of
    /// §18.4's role list belongs here and which belongs to the plan.
    ///
    /// Maintained by [`AppModel::refresh_derived`] and by nothing else.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub derived: BTreeMap<crate::DerivedRoleKey, crate::DerivedValue>,
}

const fn one() -> u16 {
    1
}

impl AppModel {
    /// Recompute every derived record from the model as it now stands.
    ///
    /// **Called after anything that could change a projection** — linking, a
    /// patch, the reader's layout arriving at compile time. It is a pure
    /// function of the rest of the model, so calling it twice is calling it
    /// once, and forgetting to call it is the only way the field can be wrong.
    /// `derived_records_are_a_function_of_the_model` holds that.
    pub fn refresh_derived(&mut self) {
        self.derived = crate::derived::records(self);
    }
}

impl Entity {
    pub fn field(&self, id: &FieldId) -> Option<&Field> {
        self.fields.iter().find(|field| &field.id == id)
    }

    pub fn has_field(&self, id: &FieldId) -> bool {
        self.field(id).is_some()
    }
}

impl AppModel {
    pub fn entity(&self, id: &EntityId) -> Option<&Entity> {
        self.entities.get(id)
    }

    pub fn canonical_json(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(self)
    }

    pub fn node_count(&self) -> usize {
        1 + self.capabilities.len()
            + self.dependencies.len()
            + self.settings.len()
            + self.ejections.len()
            + self.units.len()
            + self.components.len()
            + self.projections.len()
            + self.relations.len()
            + self.entities.len()
            + self
                .entities
                .values()
                .map(|entity| entity.fields.len() + entity.indexes.len() + entity.constraints.len())
                .sum::<usize>()
            + self.operations.len()
    }
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Capability {
    pub id: CapabilityId,
    pub label: String,
    pub kind: String,
    /// Optional reader-selected base name for capability-owned Java types.
    pub name: Option<String>,
    /// Fully resolved Java package override. `None` selects the backend's
    /// conventional package below the application's base package.
    pub java_package: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Dependency {
    pub id: DependencyId,
    pub label: String,
    pub group: String,
    pub artifact: String,
    pub version: Option<String>,
    pub scope: DependencyScope,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DependencyScope {
    Compile,
    Runtime,
    Test,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Setting {
    pub id: SettingId,
    pub label: String,
    pub key: String,
    pub value: String,
    pub target: SettingTarget,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Ejection {
    pub id: EjectionId,
    pub label: String,
    pub target: String,
    /// The reader wrote the implementation and `jails adopt resource`
    /// registered it, so nothing was transferred: the compiler excludes the
    /// boundary from the managed tree and plans no creation for it, and the
    /// removal guards let the owner go, because taking the declaration away
    /// deletes nothing the reader has.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub adopted: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SettingTarget {
    Main,
    Test,
}

impl SettingTarget {
    pub fn label(self) -> &'static str {
        match self {
            Self::Main => "main",
            Self::Test => "test",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Entity {
    pub id: EntityId,
    pub label: String,
    pub names: EntityNames,
    /// Fully resolved Java package for every class this entity projects.
    ///
    /// **A pinned projection, not a second identity.** Identity is
    /// [`Entity::id`]; the package is one of the names the convention derives,
    /// and this is the author pinning it -- the same relationship
    /// `Capability::java_package` has to its backend's conventional package.
    /// `None` selects the layer packages, which is what almost every entity
    /// wants; `Some` collapses the whole slice into one package, which is how
    /// a vertical slice is spelled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub java_package: Option<String>,
    #[serde(default = "crate::facet::active_entity")]
    pub active: bool,
    pub facets: BTreeSet<Facet>,
    pub enum_constants: Vec<EnumConstant>,
    /// Declaration order, because a Java record's component order is ABI.
    ///
    /// A keyed map would re-sort `zulu, id, alpha` into `record Task(String
    /// alpha, UUID id, String zulu)`. JDL v1 §7.3 lists entity fields first
    /// among the orders that MUST be retained, because a caller compiled
    /// against the old positional constructor keeps compiling against a
    /// re-sorted one and does the wrong thing.
    ///
    /// Lookup is [`Entity::field`]. A linear scan over a handful of fields
    /// costs nothing measurable, and it buys one container rather than an
    /// ordered list beside a keyed one.
    pub fields: Vec<Field>,
    pub indexes: BTreeMap<IndexId, Index>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub constraints: BTreeMap<ConstraintId, EntityConstraint>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EntityNames {
    pub java_type: String,
    pub sql_table: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Field {
    pub id: FieldId,
    pub label: String,
    pub names: FieldNames,
    pub ty: TypeRef,
    pub required: bool,
    pub non_blank: bool,
    pub primary_key: bool,
    pub unique: bool,
    pub indexed: bool,
    #[serde(default)]
    pub length: Option<LengthRange>,
    #[serde(default, skip_serializing_if = "FieldSemantics::is_empty")]
    pub semantics: FieldSemantics,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct FieldSemantics {
    pub positive: bool,
    pub nonnegative: bool,
    pub scope: Option<FieldScope>,
    pub version: bool,
    pub default: Option<FieldDefault>,
    pub updated: bool,
}

impl FieldSemantics {
    fn is_empty(&self) -> bool {
        self == &Self::default()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FieldScope {
    pub claim: String,
    /// True when the claim name was pinned explicitly in source.
    pub pinned: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FieldDefault {
    pub value: crate::operation::Value,
    /// True when the compiler derived the value from another field rule.
    pub derived: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LengthRange {
    pub min: Option<u32>,
    pub max: Option<u32>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FieldNames {
    pub java_member: String,
    pub sql_column: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Index {
    pub id: IndexId,
    pub label: String,
    pub sql_name: String,
    pub columns: Vec<IndexColumn>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct IndexColumn {
    pub field: FieldId,
    pub direction: IndexDirection,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum IndexDirection {
    Asc,
    Desc,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Facet {
    Enum,
    Record,
    Factory,
    Dto,
    Repository,
    Service,
    Http,
    Events,
    Search,
    /// Development seed data and the runner that loads it.
    ///
    /// Its own facet rather than sharing the factory's, because `Facet` is the
    /// emitter's dispatch key, and sharing one would make `use seed` render a
    /// test fixture and report success. The emitter's match is exhaustive, so
    /// a facet with no arm is a compile error.
    Seed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum TypeRef {
    Builtin(BuiltinType),
    External(String),
    /// `list<T>`: an ordered collection of one element type.
    ///
    /// **A collection is a shape, and its element is an ordinary type.** The
    /// element resolves exactly as a bare component's does -- lowercase is
    /// jails', capitalised is the project's -- so `list<Match>` and
    /// `list<string>` need no second table. JDL v1 §9.2 makes them valid on
    /// non-stored records and component payloads only: a column type for one
    /// would be a codec, and a codec that silently became JSON is the thing
    /// the specification forbids by name.
    List(Box<TypeRef>),
    /// `map<K,V>`, whose key is `string` or an enum.
    Map(Box<TypeRef>, Box<TypeRef>),
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum BuiltinType {
    String,
    Integer,
    Long,
    Double,
    Decimal,
    Boolean,
    Uuid,
    Date,
    DateTime,
    Instant,
    Duration,
    Uri,
    Path,
    ZoneId,
    Currency,
    Bytes,
}

impl TypeRef {
    pub(crate) fn parse(value: &str) -> Result<Self, String> {
        if let Some(builtin) = BuiltinType::from_token(value) {
            return Ok(Self::Builtin(builtin));
        }
        if let Some(collection) = Self::parse_collection(value)? {
            return Ok(collection);
        }
        // **Case is the rule, and it is what makes an unknown type an
        // error.** A lowercase token names one of jails' own types, so a
        // lowercase token that is not in that table is a misspelling --
        // passed through as a project type, `value: nosuchtype` renders
        // `record Broken(nosuchtype value)`, which does not compile. A
        // capitalised final segment is a type the project owns and jails is
        // right not to know; the segments before it are its package.
        //
        // **A package segment is asked the same question, keywords
        // included**, through `naming::valid_java_package_segment`: a
        // shape-only check lets `status:enum.PENDING.PAID` link and render
        // `import enum.PENDING.PAID;` -- a file that cannot compile, written
        // with no diagnostic.
        //
        // **A Java spelling of a builtin is refused, not resolved as a project
        // type.** `BuiltinSemantics::aliases` records `String`, `UUID`,
        // `LocalDate` and the rest so the CLI can canonicalise them on the way
        // in; `jdl 1` matches the canonical token alone. Linked as an external
        // type, a hand-written `title: String` renders a component of a type
        // the project does not declare, and the first refusal is one step
        // from the mistake -- `non_blank` is valid only for builtin `string`
        // fields -- about the attribute rather than the type. Only a bare
        // spelling is refused; `com.example.Path` is a project type whose
        // final segment happens to collide, and its package says so.
        if let Some(builtin) = BuiltinType::from_alias(value) {
            return Err(format!(
                "`{value}` is the Java spelling of jails' builtin `{}`, and `jdl 1` states jails' own spelling",
                builtin.semantics().token
            ));
        }
        let (package, name) = match value.rsplit_once('.') {
            Some((package, name)) => (package, name),
            None => ("", value),
        };
        let segments = || package.split('.');
        if crate::naming::valid_java_type(name)
            && (package.is_empty() || segments().all(crate::naming::valid_java_package_segment))
        {
            return Ok(Self::External(value.to_string()));
        }
        // A reserved word is a different mistake from a misspelling, and
        // "capitalise it" is not the answer to it -- so it gets its own
        // sentence naming the segment. **No `fix:` line here**: every caller
        // already supplies one, and two `fix:` lines under one diagnostic is
        // a defect, not twice the help.
        if let Some(reserved) = segments()
            .chain(std::iter::once(name))
            .find(|segment| crate::naming::is_java_keyword(segment))
        {
            return Err(format!(
                "`{value}` cannot be a Java type: `{reserved}` is a reserved word, so the import it would need does not parse"
            ));
        }
        Err(format!(
            "`{value}` is an unknown field type: it is not one of jails' own, and a type this project declares is capitalised"
        ))
    }

    /// `list<T>` and `map<K,V>`, or `None` when this is not one.
    ///
    /// **The element is parsed, not pattern-matched.** An element that is
    /// itself a collection, an optional, or a misspelling is refused here
    /// with the reason, so a nested shape cannot reach an emitter that has
    /// no spelling for it.
    fn parse_collection(value: &str) -> Result<Option<Self>, String> {
        let Some((head, rest)) = value.split_once('<') else {
            return Ok(None);
        };
        let Some(inner) = rest.strip_suffix('>') else {
            if matches!(head, "list" | "map") {
                return Err(format!("`{value}` is missing its closing `>`"));
            }
            return Ok(None);
        };
        match head {
            "list" => Ok(Some(Self::List(Box::new(Self::element(inner, "list")?)))),
            "map" => {
                let Some((key, element)) = inner.split_once(',') else {
                    return Err(format!(
                        "`{value}` needs both a key and a value: a map is `map<K,V>`"
                    ));
                };
                let key = Self::element(key.trim(), "map")?;
                // JDL v1 §9.2: a key is a `string` or an enum, because those
                // are the two things every wire format and every `Map` key
                // spells the same way. An `instant` key reads back as a
                // different instant on the other side of a serialiser.
                if !matches!(key, Self::Builtin(BuiltinType::String) | Self::External(_)) {
                    return Err(format!(
                        "`{}` cannot key a map: a key is `string` or an enum this project declares",
                        key.canonical_name()
                    ));
                }
                Ok(Some(Self::Map(
                    Box::new(key),
                    Box::new(Self::element(element.trim(), "map")?),
                )))
            }
            _ => Ok(None),
        }
    }

    /// One element of a collection: an ordinary type, and not another
    /// collection or an optional.
    fn element(value: &str, outer: &str) -> Result<Self, String> {
        if value.is_empty() {
            return Err(format!("`{outer}` needs an element type"));
        }
        if value.ends_with('?') {
            return Err(format!(
                "`{value}` cannot be optional inside a `{outer}`: optionality applies to the collection as a whole"
            ));
        }
        match Self::parse(value)? {
            nested @ (Self::List(_) | Self::Map(..)) => Err(format!(
                "`{}` cannot be nested inside a `{outer}`: v1 collections hold scalars, enums and declared types",
                nested.canonical_name()
            )),
            element => Ok(element),
        }
    }

    /// Whether this type is one a column can hold.
    ///
    /// The refusal is the caller's, because only the caller knows which
    /// declaration is being stored and what to tell the reader to do.
    pub fn is_scalar(&self) -> bool {
        matches!(self, Self::Builtin(_) | Self::External(_))
    }

    pub fn canonical_name(&self) -> String {
        match self {
            Self::Builtin(builtin) => builtin.semantics().token.to_string(),
            Self::External(name) => name.clone(),
            Self::List(element) => format!("list<{}>", element.canonical_name()),
            Self::Map(key, value) => {
                format!("map<{},{}>", key.canonical_name(), value.canonical_name())
            }
        }
    }
}

impl Entity {
    /// Refuse to evolve a retired entity: its table is preserved, not live.
    pub fn refuse_retired(&self) -> Result<(), String> {
        if self.active {
            return Ok(());
        }
        Err(format!(
            "entity id `{}` is retired\n       fix: revive the preserved entity before evolving it",
            self.id
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Both halves, because a one-sided test is satisfied by refusing
    /// everything: the reserved word has to refuse -- linked,
    /// `status:enum.PENDING.PAID` renders `import enum.PENDING.PAID;` into a
    /// file that cannot compile -- *and* an ordinary package has to keep
    /// resolving.
    #[test]
    fn a_reserved_word_is_never_a_java_type_and_an_ordinary_package_still_is() {
        assert_eq!(
            TypeRef::parse("com.example.Thing"),
            Ok(TypeRef::External("com.example.Thing".to_string()))
        );
        assert_eq!(
            TypeRef::parse("Status"),
            Ok(TypeRef::External("Status".to_string()))
        );
        assert_eq!(
            TypeRef::parse("string"),
            Ok(TypeRef::Builtin(BuiltinType::String))
        );

        for reserved in ["enum.PENDING.PAID", "new.Thing", "com.class.Thing"] {
            let message = TypeRef::parse(reserved).unwrap_err();
            assert!(
                message.contains("is a reserved word"),
                "`{reserved}` should name the reserved word: {message}"
            );
            // No `fix:` here on purpose: the linker call sites carry one,
            // and two under a single diagnostic reads as a defect.
            assert!(!message.contains("fix:"), "`{reserved}`: {message}");
        }

        // The misspelling message is a different mistake and keeps its own
        // wording: `jails g enum` is not the answer to a typo.
        let typo = TypeRef::parse("notatype").unwrap_err();
        assert!(typo.contains("unknown field type"), "{typo}");
    }

    /// The spelling the CLI accepts and `jdl 1` does not.
    ///
    /// Both halves again: the alias refuses and names the canonical token, and
    /// a qualified type whose final segment *is* an alias still resolves --
    /// `com.example.Path` is a type the project declares and its package says
    /// so, which is the difference this rule turns on.
    #[test]
    fn a_java_spelling_of_a_builtin_refuses_and_names_the_canonical_token() {
        for (alias, token) in [
            ("String", "string"),
            ("UUID", "uuid"),
            ("LocalDate", "date"),
            ("BigDecimal", "decimal"),
            ("Boolean", "boolean"),
        ] {
            let message = TypeRef::parse(alias).unwrap_err();
            assert!(
                message.contains(&format!("`{token}`")),
                "{alias}: {message}"
            );
        }
        assert_eq!(
            TypeRef::parse("com.example.Path"),
            Ok(TypeRef::External("com.example.Path".to_string()))
        );
        // `currency` has no aliases on purpose -- an enum of the currencies a
        // project deals in is an ordinary thing to generate -- so the
        // capitalised spelling stays a project type.
        assert_eq!(
            TypeRef::parse("Currency"),
            Ok(TypeRef::External("Currency".to_string()))
        );
    }
}
