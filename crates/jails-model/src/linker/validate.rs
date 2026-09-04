//! Every value a human typed, checked against the rule its projection needs.
//!
//! Split from the walk above by what it knows: the linker walks a document and
//! decides *what* to build, while this decides whether one string may be a
//! Java type, a package, a SQL identifier or a route -- questions that have
//! nothing to do with the shape of the document, and every one of which the
//! walk would otherwise answer inline.

use super::*;

/// The `java.lang` types every Java file imports without saying so.
///
/// Not the whole package: these are the ones a generated record or its
/// components would plausibly be named, and a list nobody can complete is
/// worse than a short one that is right. A name outside it that collides is
/// caught by javac, which is the tier below this one.
const JAVA_LANG_TYPES: &[&str] = &[
    "Boolean",
    "Byte",
    "Character",
    "Class",
    "Double",
    "Enum",
    "Error",
    "Exception",
    "Float",
    "Integer",
    "Iterable",
    "Long",
    "Math",
    "Module",
    "Number",
    "Object",
    "Package",
    "Process",
    "Record",
    "Runnable",
    "Runtime",
    "Short",
    "String",
    "System",
    "Thread",
    "Throwable",
    "Void",
];

/// Standard Java types and Spring framework role names that cannot name an entity.
///
/// If an entity is named after one of these, it either collides with generated
/// stereotypes/roles (e.g. `Controller`, `Repository`, `Service`) or collides with
/// standard library imports (e.g. `List`, `Map`, `Set`, `Optional`, `UUID`) across
/// every generated file that imports them.
const FRAMEWORK_AND_STD_TYPES: &[&str] = &[
    "Controller",
    "Repository",
    "Service",
    "List",
    "Map",
    "Set",
    "Optional",
    "UUID",
    "Collection",
    "Iterator",
];

/// PostgreSQL words that cannot name a table or column unquoted.
///
/// The reserved half of the standard's list, which is what `create table`
/// rejects. jails never quotes an identifier -- a quoted name is
/// case-sensitive forever after, which is a trap rather than a fix -- so a
/// derived name landing here is refused instead.
const POSTGRES_RESERVED: &[&str] = &[
    "all",
    "analyse",
    "analyze",
    "and",
    "any",
    "array",
    "as",
    "asc",
    "asymmetric",
    "both",
    "case",
    "cast",
    "check",
    "collate",
    "column",
    "constraint",
    "create",
    "current_catalog",
    "current_date",
    "current_role",
    "current_time",
    "current_timestamp",
    "current_user",
    "default",
    "deferrable",
    "desc",
    "distinct",
    "do",
    "else",
    "end",
    "except",
    "false",
    "fetch",
    "for",
    "foreign",
    "from",
    "grant",
    "group",
    "having",
    "in",
    "initially",
    "intersect",
    "into",
    "is",
    "lateral",
    "leading",
    "limit",
    "localtime",
    "localtimestamp",
    "not",
    "null",
    "offset",
    "on",
    "only",
    "or",
    "order",
    "placing",
    "primary",
    "references",
    "returning",
    "select",
    "session_user",
    "some",
    "symmetric",
    "table",
    "then",
    "to",
    "trailing",
    "true",
    "union",
    "unique",
    "user",
    "using",
    "variadic",
    "when",
    "where",
    "window",
    "with",
];

/// The zero-argument `java.lang.Object` methods a record accessor would clash
/// with. `getClass`, `notify`, `notifyAll` and `wait` are `final` or would
/// change the type, so a component of that name fails to compile too.
const OBJECT_METHODS: &[&str] = &[
    "clone",
    "equals",
    "finalize",
    "getClass",
    "hashCode",
    "notify",
    "notifyAll",
    "toString",
    "wait",
];

#[derive(Default)]
pub(crate) struct Linker {
    pub(crate) diagnostics: Vec<Diagnostic>,
    ids: BTreeMap<String, String>,
    /// Where the document declared each path, so a diagnostic can say which
    /// line to go to. Empty when the caller had no document -- the tests that
    /// link a hand-built `source::Document` -- and a diagnostic with no
    /// location is what it was before.
    spans: crate::jdl::v1::SpanIndex,
    /// The fields that were declared and did not link, keyed by field path.
    ///
    /// **A cascade is one mistake reported as several, and the later ones are
    /// wrong.** A field whose type is misspelled is dropped, so an index on
    /// it reports that the column does not name a field -- which sends the
    /// reader to delete a line that is correct. The first diagnostic is the
    /// true one; the rest are suppressed and reappear the moment it is fixed.
    unlinked_fields: std::collections::BTreeSet<String>,
    /// Each linked entity's model path, by the stable id a later declaration
    /// refers to it with. An operation names an entity by id; the cascade is
    /// recorded by path, and this is the one hop between them.
    entity_paths: BTreeMap<String, String>,
}

/// Which SQL name is being checked, and whether it will be written at all.
///
/// **Two facts, and they travel together.** The reserved-word refusal is only
/// right where DDL is emitted, and the message is only right when it names
/// the thing the reader would go and rename. Carrying them as one value is
/// what stops a call site passing the noun and forgetting the guard.
#[derive(Clone, Copy)]
pub(crate) struct SqlName {
    /// `table`, `column`, `index` or `constraint`, for the message.
    noun: &'static str,
    /// Whether the declaration this name belongs to reaches the DDL.
    reaches_sql: bool,
    fix: &'static str,
}

impl SqlName {
    pub(crate) fn table(stored: bool) -> Self {
        Self {
            noun: "table",
            reaches_sql: stored,
            fix: "choose a name whose plural is not reserved, or pin the table with `@table`",
        }
    }

    pub(crate) fn column(stored: bool) -> Self {
        Self {
            noun: "column",
            reaches_sql: stored,
            fix: "rename the field, or pin the column with `@column`",
        }
    }

    pub(crate) fn index(stored: bool) -> Self {
        Self {
            noun: "index",
            reaches_sql: stored,
            fix: "rename the index, or pin its name in the declaration",
        }
    }

    pub(crate) fn constraint(stored: bool) -> Self {
        Self {
            noun: "constraint",
            reaches_sql: stored,
            fix: "rename the field the constraint is derived from",
        }
    }
}

impl Linker {
    /// The document's own span index, which every diagnostic is located
    /// through.
    pub(crate) fn with_spans(spans: &crate::jdl::v1::SpanIndex) -> Self {
        Self {
            spans: spans.clone(),
            ..Self::default()
        }
    }

    pub(crate) fn problem(
        &mut self,
        code: &'static str,
        path: impl Into<String>,
        message: impl Into<String>,
        fix: impl Into<String>,
    ) {
        let path = path.into();
        let located = self.spans.locate(&path);
        let mut diagnostic = Diagnostic::new(code, path, message, fix);
        if let Some(location) = located {
            diagnostic = diagnostic.at(location.line, location.column);
        }
        self.diagnostics.push(diagnostic);
    }

    /// Record that a declared field did not link, so the references to it
    /// stay quiet.
    pub(crate) fn field_did_not_link(&mut self, field_path: &str) {
        self.unlinked_fields.insert(field_path.to_string());
    }

    /// Whether this field is one whose own diagnostic has already been
    /// reported. A reference to it is a consequence, not a second mistake.
    pub(crate) fn field_did_link(&self, entity_path: &str, label: &str) -> bool {
        !self
            .unlinked_fields
            .contains(&format!("{entity_path}.fields.{label}"))
    }

    /// Where an entity with this stable id was declared.
    pub(crate) fn note_entity_path(&mut self, id: &str, path: &str) {
        self.entity_paths.insert(id.to_string(), path.to_string());
    }

    /// [`Self::field_did_link`], for a caller holding the entity's id rather
    /// than its path.
    fn field_of_entity_did_link(&self, entity: &EntityId, label: &str) -> bool {
        self.entity_paths
            .get(entity.as_str())
            .is_none_or(|path| self.field_did_link(path, label))
    }

    pub(crate) fn register_id(&mut self, id: &str, path: &str) {
        if let Some(first) = self.ids.insert(id.to_string(), path.to_string()) {
            self.problem(
                "model-id-collision",
                path,
                format!("stable id `{id}` is already used at {first}"),
                "give every model node a globally unique stable id",
            );
        }
    }

    pub(crate) fn stable_id<T>(&mut self, value: &str, path: &str) -> Option<T>
    where
        T: ParseStableId,
    {
        match T::parse_stable_id(value) {
            Ok(id) => Some(id),
            Err(message) => {
                self.problem("model-stable-id", path, message, "use a valid stable id");
                None
            }
        }
    }

    pub(crate) fn dependency_id(&mut self, value: &str, path: &str) -> Option<DependencyId> {
        self.stable_id(value, path)
    }

    pub(crate) fn capability_id(&mut self, value: &str, path: &str) -> Option<CapabilityId> {
        self.stable_id(value, path)
    }

    pub(crate) fn setting_id(&mut self, value: &str, path: &str) -> Option<SettingId> {
        self.stable_id(value, path)
    }

    pub(crate) fn ejection_id(&mut self, value: &str, path: &str) -> Option<EjectionId> {
        self.stable_id(value, path)
    }

    pub(crate) fn index_id(&mut self, value: &str, path: &str) -> Option<IndexId> {
        self.stable_id(value, path)
    }

    pub(crate) fn constraint_id(&mut self, value: &str, path: &str) -> Option<ConstraintId> {
        self.stable_id(value, path)
    }

    pub(crate) fn projection_id(&mut self, value: &str, path: &str) -> Option<ProjectionId> {
        self.stable_id(value, path)
    }

    pub(crate) fn relation_id(&mut self, value: &str, path: &str) -> Option<RelationId> {
        self.stable_id(value, path)
    }

    pub(crate) fn label(&mut self, value: &str, path: &str) {
        if !valid_label(value) {
            self.problem(
                "model-label",
                path,
                format!("`{value}` is not a model label"),
                "use lowercase letters, digits, `_` or `-`, starting with a letter",
            );
        }
    }

    pub(crate) fn java_type(&mut self, value: &str, path: &str) {
        if !self.java_identifier(value, path) {
            return;
        }
        self.java_lang_shadow(value, path);
        self.java_framework_or_std_collision(value, path);
    }

    /// Whether the name is a Java identifier at all, reporting it if not.
    fn java_identifier(&mut self, value: &str, path: &str) -> bool {
        if valid_java_type(value) {
            return true;
        }
        self.problem(
            "model-java-type",
            path,
            format!("`{value}` is not valid in a Java identifier"),
            "use an upper-camel-case Java identifier",
        );
        false
    }

    /// **A package member outranks `java.lang`'s implicit import.**
    ///
    /// `record String(String value)` types its own component as *itself* and
    /// compiles, as does its generated test, so no tier catches it. Refused
    /// where the name is *declared*, not where one is referenced: a
    /// `value:String` is the ordinary case.
    fn java_lang_shadow(&mut self, value: &str, path: &str) {
        if JAVA_LANG_TYPES.contains(&value) {
            self.problem(
                "model-java-lang-shadow",
                path,
                format!("`{value}` is a type in `java.lang`, which every Java file imports"),
                "choose another name -- a class here would outrank the one every file already has",
            );
        }
    }

    fn java_framework_or_std_collision(&mut self, value: &str, path: &str) {
        if FRAMEWORK_AND_STD_TYPES.contains(&value) {
            self.problem(
                "model-java-framework-collision",
                path,
                format!("`{value}` collides with a standard Java type or Spring framework role"),
                "choose another name that does not collide with standard Java library types or framework roles (e.g. Controller, Repository, Service, List, UUID)",
            );
        }
    }

    /// The same rules, plus the one that only applies where a **variable** of
    /// this type is written.
    ///
    /// **The variable name is derived, so it is jails' to get right.** An
    /// entity and a unit are both emitted as `{Type} {variable} = ...` -- in
    /// the record's own test, in a repository round-trip, in an enum's
    /// constant test -- and the lower-camel form of a type like `Class` is a
    /// Java keyword, so the type validates, the table validates, and the code
    /// does not compile.
    ///
    /// **A component is deliberately not checked**, because nothing derives a
    /// variable from its name: `g command Import` writes `ImportCommand`, and
    /// refusing it would refuse a program that compiles.
    ///
    /// Checked before the `java.lang` shadow rule, because a name that trips
    /// both -- `Class` and `Void` are the two -- fails more concretely as the
    /// variable it derives than as the type it hides.
    pub(crate) fn java_type_and_variable(&mut self, value: &str, path: &str) {
        if !self.java_identifier(value, path) {
            return;
        }
        let variable = lower_camel_case(value);
        if !valid_java_member(&variable) {
            self.problem(
                "model-java-variable",
                path,
                format!("`{value}` derives the Java variable `{variable}`, which is a keyword"),
                "choose a name whose lower-camel-case form is a Java identifier",
            );
            return;
        }
        self.java_lang_shadow(value, path);
        self.java_framework_or_std_collision(value, path);
    }

    pub(crate) fn java_member(&mut self, value: &str, path: &str) {
        if !valid_java_member(value) {
            self.problem(
                "model-java-member",
                path,
                format!("`{value}` is not a Java member name"),
                "use a lower-camel-case Java identifier",
            );
            return;
        }
        // **A record component may not be named after a method every record
        // already has.** `record Box(String hashCode)` does not compile:
        // `java.lang.Object` declares the accessor the component would need,
        // and every entity here becomes a record. It is a Java identifier by
        // every other test, which is exactly why the refusal has to name the
        // reason rather than the shape.
        if OBJECT_METHODS.contains(&value) {
            self.problem(
                "model-java-object-method",
                path,
                format!("`{value}` is a method every record inherits from java.lang.Object"),
                "rename the component -- a record cannot declare an accessor that overrides one",
            );
        }
    }

    pub(crate) fn java_package(&mut self, value: &str, path: &str) {
        let valid = !value.is_empty()
            && value.split('.').all(|part| {
                let mut chars = part.chars();
                chars
                    .next()
                    .is_some_and(|character| character.is_ascii_lowercase())
                    && chars.all(|character| {
                        character.is_ascii_lowercase()
                            || character.is_ascii_digit()
                            || character == '_'
                    })
                    && !crate::naming::is_java_keyword(part)
            });
        if !valid {
            self.problem(
                "model-java-package",
                path,
                format!("`{value}` is not a valid Java package"),
                "use dot-separated lowercase Java package segments that are not reserved keywords",
            );
        }
    }

    pub(crate) fn sql_identifier(&mut self, value: &str, path: &str, name: SqlName) {
        // **A reserved word has to be quoted, and jails does not quote.**
        // `create table as (...)` is a syntax error, and the name is derived
        // -- `A` pluralizes to `as` -- so the reader has no way to see it
        // coming from what they typed.
        //
        // **Only where SQL is written.** An entity with no repository has no
        // table, no column and no DDL, so a reserved word in it breaks
        // nothing; refusing anyway made `jails g record Timing when:instant`
        // impossible on a project with no database at all. And the message
        // says which name it is about: a column called `when` is not a table.
        if name.reaches_sql && POSTGRES_RESERVED.contains(&value) {
            self.problem(
                "model-sql-reserved",
                path,
                format!(
                    "`{value}` derives the PostgreSQL {} `{value}`, which is a reserved word",
                    name.noun
                ),
                name.fix,
            );
            return;
        }
        let mut chars = value.chars();
        let valid = chars
            .next()
            .is_some_and(|character| character.is_ascii_lowercase())
            && chars.all(|character| {
                character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
            });
        if !valid {
            self.problem(
                "model-sql-identifier",
                path,
                format!("`{value}` is not a valid SQL identifier"),
                "use lowercase snake_case without quoting",
            );
        }
    }

    pub(crate) fn entity_ref(
        &mut self,
        label: &str,
        path: &str,
        entities: &BTreeMap<String, EntityId>,
    ) -> Option<EntityId> {
        entities.get(label).cloned().or_else(|| {
            self.problem(
                "model-entity-reference",
                path,
                format!("`{label}` does not name an entity"),
                "use an entity label `.jails/model.jdl` declares",
            );
            None
        })
    }

    pub(crate) fn field_refs(
        &mut self,
        labels: &[String],
        path: &str,
        entity: &EntityId,
        all_fields: &BTreeMap<EntityId, BTreeMap<String, FieldId>>,
    ) -> Vec<FieldId> {
        let fields = all_fields.get(entity);
        let mut seen = BTreeSet::new();
        labels
            .iter()
            .filter_map(|label| {
                if !seen.insert(label) {
                    self.problem(
                        "model-duplicate-reference",
                        path,
                        format!("field `{label}` is named more than once"),
                        "keep each field reference once",
                    );
                    return None;
                }
                fields
                    .and_then(|fields| fields.get(label))
                    .cloned()
                    .or_else(|| {
                        // Silent when the field is declared on that entity
                        // and failed to link: the operation is right and the
                        // field's own diagnostic is the one to fix.
                        if self.field_of_entity_did_link(entity, label) {
                            self.problem(
                                "model-field-reference",
                                path,
                                format!("`{label}` is not a field on entity id `{entity}`"),
                                "use a field label declared on the referenced entity",
                            );
                        }
                        None
                    })
            })
            .collect()
    }

    pub(crate) fn route(
        &mut self,
        route: Option<&str>,
        path: &str,
        routes: &mut BTreeMap<String, String>,
    ) {
        let Some(route) = route else {
            return;
        };
        if !valid_route(route) {
            self.problem(
                "model-route",
                format!("{path}.route"),
                crate::naming::route_problem(route)
                    .unwrap_or_else(|| format!("`{route}` is not a valid HTTP route")),
                "use `METHOD /path`, for example `GET /notes/{id}`",
            );
            return;
        }
        if let Some(first) = routes.insert(route.to_string(), path.to_string()) {
            self.problem(
                "model-route-collision",
                format!("{path}.route"),
                format!("HTTP route `{route}` is already declared at {first}"),
                "give every HTTP operation a unique method and path",
            );
        }
    }
}

pub(crate) trait ParseStableId: StableId + Sized {
    fn parse_stable_id(value: &str) -> Result<Self, String>;
}

macro_rules! parse_stable_id {
    ($($id:ty),+ $(,)?) => {
        $(
            impl ParseStableId for $id {
                fn parse_stable_id(value: &str) -> Result<Self, String> {
                    Self::parse(value)
                }
            }
        )+
    };
}

parse_stable_id!(
    ProjectId,
    CapabilityId,
    DependencyId,
    SettingId,
    EjectionId,
    EntityId,
    FieldId,
    IndexId,
    ConstraintId,
    ProjectionId,
    RelationId,
    OperationId,
    ComponentId,
    ComponentVariantId,
    UnitId
);

pub(crate) fn collision(
    linker: &mut Linker,
    seen: &mut BTreeMap<String, String>,
    projection: &str,
    path: &str,
    code: &'static str,
    kind: &str,
) {
    if let Some(first) = seen.insert(projection.to_string(), path.to_string()) {
        linker.problem(
            code,
            path,
            format!("{kind} name `{projection}` is already used at {first}"),
            format!("give each declaration a unique {kind} name"),
        );
    }
}
