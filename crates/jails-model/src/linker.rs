//! The one boundary between human labels and semantic identities.

mod component;
mod enum_type;
mod field;
mod operation;
mod unit;

use crate::ProjectIntent;
use crate::diagnostic::{Diagnostic, Diagnostics};
use crate::id::{
    CapabilityId, ComponentId, ComponentVariantId, ConstraintId, DependencyId, EjectionId,
    EntityId, FieldId, IndexId, OperationId, ProjectId, ProjectionId, RelationId, SettingId,
    StableId, UnitId,
};
use crate::model::{AppModel, Entity, EntityNames, Field, FieldNames, TypeRef};
use crate::naming::{
    lower_camel_case, snake_case, upper_camel_case, valid_java_member, valid_java_type,
    valid_label, valid_route,
};
use crate::source;
use std::collections::{BTreeMap, BTreeSet};

const SCHEMA: &str = "jails.model.v1";

/// The zero-argument `java.lang.Object` methods a record accessor would clash
/// with. `getClass`, `notify`, `notifyAll` and `wait` are `final` or would
/// change the type, so a component of that name fails to compile too.
const OBJECT_METHODS: &[&str] = &[
    "clone", "equals", "finalize", "getClass", "hashCode", "notify", "notifyAll", "toString",
    "wait",
];

pub(crate) fn link(document: source::Document) -> Result<AppModel, Diagnostics> {
    let mut linker = Linker::default();

    if document.schema != SCHEMA {
        linker.problem(
            "model-schema",
            "$.schema",
            format!("unsupported model schema `{}`", document.schema),
            format!("set `schema = \"{SCHEMA}\"`"),
        );
    }

    linker.register_id(&document.project.id, "$.project.id");
    let project_id = linker.stable_id::<ProjectId>(&document.project.id, "$.project.id");
    linker.java_type(&document.project.name, "$.project.name");
    linker.java_package(&document.project.base_package, "$.project.base_package");
    if document.project.java_release < 21 {
        linker.problem(
            "model-java-release",
            "$.project.java_release",
            format!(
                "Java {} is below the supported release floor",
                document.project.java_release
            ),
            "use Java 21 or newer",
        );
    }
    if !matches!(
        document.project.dialect.as_str(),
        "postgresql" | "sqlite" | "h2" | "none"
    ) {
        linker.problem(
            "model-dialect",
            "$.project.dialect",
            format!("unknown SQL dialect `{}`", document.project.dialect),
            "use `postgresql`, `sqlite`, `h2`, or `none`",
        );
    }
    if !matches!(document.project.platform.as_str(), "spring" | "plain") {
        linker.problem(
            "model-platform",
            "$.project.platform",
            format!("unknown platform `{}`", document.project.platform),
            "use `spring` or `plain`",
        );
    }
    if !matches!(document.project.build.as_str(), "maven" | "gradle") {
        linker.problem(
            "model-build",
            "$.project.build",
            format!("unknown build system `{}`", document.project.build),
            "use `maven` or `gradle`",
        );
    }

    let capabilities = crate::capability::link(
        document.capabilities,
        &document.project.base_package,
        &mut linker,
    );

    let dependencies = crate::dependency::link(document.dependencies, &mut linker);
    let settings = crate::setting::link(document.settings, &mut linker);

    let mut units = unit::link(document.units, &document.project.base_package, &mut linker);

    let mut entities = BTreeMap::new();
    let mut entity_labels = BTreeMap::new();
    let mut entity_fields = BTreeMap::<EntityId, BTreeMap<String, FieldId>>::new();
    let mut java_types = BTreeMap::<String, String>::new();
    let mut sql_tables = BTreeMap::<String, String>::new();
    let mut entity_projections = BTreeMap::<EntityId, Vec<source::Projection>>::new();
    let mut entity_relations = BTreeMap::<EntityId, BTreeMap<String, source::Relation>>::new();

    for (label, entity) in document.entities {
        let path = format!("$.entities.{label}");
        linker.label(&label, &path);
        linker.register_id(&entity.id, &format!("{path}.id"));
        let id = linker.stable_id::<EntityId>(&entity.id, &format!("{path}.id"));

        let java_type = entity.java_name.unwrap_or_else(|| upper_camel_case(&label));
        // Pluralized, per §9.7. `@table` still wins: a contract pin is the
        // reader saying what the database already calls it.
        let sql_table = entity
            .table
            .unwrap_or_else(|| crate::naming::plural_snake_case(&label));
        linker.java_type(&java_type, &format!("{path}.java_name"));
        linker.sql_identifier(&sql_table, &format!("{path}.table"));
        collision(
            &mut linker,
            &mut java_types,
            &java_type,
            &path,
            "model-java-type-collision",
            "Java type",
        );
        collision(
            &mut linker,
            &mut sql_tables,
            &sql_table,
            &path,
            "model-sql-table-collision",
            "SQL table",
        );

        let mut fields = Vec::new();
        let mut field_labels = BTreeMap::new();
        let mut java_members = BTreeMap::<String, String>::new();
        let mut sql_columns = BTreeMap::<String, String>::new();
        let mut primary_keys = 0_usize;

        // Declaration order when the frontend stated one, label order when it
        // could not. A Java record's component order is ABI, so re-sorting
        // here is not a presentation choice: a caller compiled against the
        // positional constructor keeps compiling against a re-sorted one and
        // silently passes the wrong arguments.
        let mut declared = entity.fields;
        let ordered = entity
            .field_order
            .iter()
            .filter_map(|label| declared.remove_entry(label))
            .collect::<Vec<_>>();
        let ordered = ordered.into_iter().chain(declared).collect::<Vec<_>>();
        for (field_label, field) in ordered {
            let field_path = format!("{path}.fields.{field_label}");
            linker.label(&field_label, &field_path);
            linker.register_id(&field.id, &format!("{field_path}.id"));
            let field_id = linker.stable_id::<FieldId>(&field.id, &format!("{field_path}.id"));
            let java_member = field
                .java_name
                .unwrap_or_else(|| lower_camel_case(&field_label));
            let sql_column = field.column.unwrap_or_else(|| snake_case(&field_label));
            linker.java_member(&java_member, &format!("{field_path}.java_name"));
            linker.sql_identifier(&sql_column, &format!("{field_path}.column"));
            collision(
                &mut linker,
                &mut java_members,
                &java_member,
                &field_path,
                "model-java-member-collision",
                "Java member",
            );
            collision(
                &mut linker,
                &mut sql_columns,
                &sql_column,
                &field_path,
                "model-sql-column-collision",
                "SQL column",
            );

            let ty = match TypeRef::parse(&field.type_name) {
                Ok(ty) => Some(ty),
                Err(message) => {
                    linker.problem(
                        "model-field-type",
                        format!("{field_path}.type"),
                        message,
                        "use one of jails' lowercase types, or name a capitalised type this project declares",
                    );
                    None
                }
            };
            if field.primary_key {
                primary_keys += 1;
                if !field.required {
                    linker.problem(
                        "model-primary-key-required",
                        format!("{field_path}.required"),
                        "a primary key cannot be optional",
                        "set `required = true`",
                    );
                }
            }
            let length = field::constraints(
                field.non_blank,
                field.min_length,
                field.max_length,
                field.required,
                ty.as_ref(),
                &field_path,
                &mut linker,
            );
            let semantics = field::semantics(
                field.semantics,
                &java_member,
                field.required,
                field.primary_key,
                ty.as_ref(),
                &field_path,
                &mut linker,
            );

            if let (Some(field_id), Some(ty)) = (field_id, ty) {
                field_labels.insert(field_label.clone(), field_id.clone());
                fields.push(Field {
                    id: field_id,
                    label: field_label,
                    names: FieldNames {
                        java_member,
                        sql_column,
                    },
                    ty,
                    required: field.required,
                    non_blank: field.non_blank,
                    primary_key: field.primary_key,
                    unique: field.unique,
                    indexed: field.indexed,
                    length,
                    semantics,
                });
            }
        }

        field::validate_scope_claims(&path, &fields, &mut linker);

        let requires_primary_key = entity.facets.iter().any(|facet| {
            matches!(
                facet,
                crate::model::Facet::Repository | crate::model::Facet::Search
            )
        });
        let enum_constants = enum_type::link(
            &mut linker,
            &path,
            &entity.facets,
            &entity.values,
            !fields.is_empty(),
            !entity.indexes.is_empty(),
        );
        let constraints = crate::constraint::link(
            &mut linker,
            &path,
            &sql_table,
            &fields,
            &field_labels,
            entity.constraints,
        );
        let explicit_primary_keys = constraints
            .values()
            .filter(|constraint| constraint.kind == crate::ConstraintKind::PrimaryKey)
            .count();
        let indexes = crate::index::link(
            &mut linker,
            &path,
            &label,
            &sql_table,
            &fields,
            &field_labels,
            entity.indexes,
        );

        if primary_keys + explicit_primary_keys > 1
            || (requires_primary_key && primary_keys + explicit_primary_keys != 1)
        {
            linker.problem(
                "model-primary-key-count",
                format!("{path}.constraints"),
                format!(
                    "entity `{label}` declares {} primary keys",
                    primary_keys + explicit_primary_keys
                ),
                if requires_primary_key {
                    "repository and search entities need exactly one primary-key constraint"
                } else {
                    "an entity may declare at most one primary-key constraint"
                },
            );
        }

        if let Some(id) = id {
            entity_labels.insert(label.clone(), id.clone());
            entity_fields.insert(id.clone(), field_labels);
            entity_projections.insert(id.clone(), entity.projections);
            entity_relations.insert(id.clone(), entity.relations);
            entities.insert(
                id.clone(),
                Entity {
                    id,
                    label,
                    names: EntityNames {
                        java_type,
                        sql_table,
                    },
                    active: entity.active,
                    // **`http` implies `service`**, and the compatibility
                    // input is why this is here rather than only in
                    // `validate_prerequisites`. JDL v1 states the requirement
                    // and refuses a model that breaks it; the pre-v1 draft
                    // lists facets directly and reaches no such check, so a
                    // `.jails/model.toml` could declare `http` without
                    // `service` -- and the controller that serves the resource
                    // delegates to the service, so the project compiled
                    // against a package that did not exist. Two dialects for
                    // one model must mean the same application.
                    facets: {
                        let mut facets = entity.facets;
                        if facets.contains(&crate::model::Facet::Http) {
                            facets.insert(crate::model::Facet::Service);
                        }
                        facets
                    },
                    enum_constants,
                    fields,
                    indexes,
                    constraints,
                },
            );
        }
    }

    let projections = crate::projection::link(
        entity_projections,
        document.projection_rules,
        &mut entities,
        &entity_labels,
        &document.project.platform,
        &document.project.dialect,
        &mut linker,
    );
    let relations = crate::relation::link(entity_relations, &entities, &entity_labels, &mut linker);

    let mut routes = BTreeMap::new();
    let operations = operation::link(
        document.operations,
        &entities,
        &entity_labels,
        &entity_fields,
        &mut routes,
        &mut linker,
    );
    operation::validate_field_rules(&operations, &entities, &capabilities, &mut linker);
    let components = component::link(
        document.components,
        &entities,
        &operations,
        &document.project.base_package,
        &mut units,
        &mut routes,
        &mut linker,
    );

    let known_targets = capabilities
        .keys()
        .map(StableId::as_str)
        .chain(units.keys().map(StableId::as_str))
        .chain(components.keys().map(StableId::as_str))
        .chain(entities.keys().map(StableId::as_str))
        .chain(operations.keys().map(StableId::as_str))
        .collect::<BTreeSet<_>>();
    let ejections = crate::ejection::link(document.ejections, &known_targets, &mut linker);

    if !linker.diagnostics.is_empty() {
        return Err(Diagnostics::from_vec(linker.diagnostics));
    }

    let Some(project_id) = project_id else {
        unreachable!("a missing project id always records a diagnostic")
    };
    let mut model = AppModel {
        schema: document.schema,
        // Both default here: JDL v1 is convention registry 1, and a linked
        // model is language 1 whichever dialect the source was in, because the
        // pre-v1 draft reaches this function through the same `Document`.
        language_version: 1,
        convention_version: 1,
        project: ProjectIntent {
            id: project_id,
            name: document.project.name,
            base_package: document.project.base_package,
            java_release: document.project.java_release,
            dialect: document.project.dialect,
            platform: document.project.platform,
            build: document.project.build,
            // JDL does not declare a layout yet, so a linked model carries the
            // defaults and capture supplies the reader's renames.
            layout: crate::Layout::default(),
        },
        capabilities,
        dependencies,
        settings,
        ejections,
        units,
        components,
        projections,
        relations,
        entities,
        operations,
        derived: std::collections::BTreeMap::new(),
    };
    // Last, because it reads the finished model. The reader's layout has not
    // arrived yet -- capture supplies it and the compiler applies it -- so the
    // package rows here carry the default names and are recomputed there.
    model.refresh_derived();
    Ok(model)
}

#[derive(Default)]
pub(crate) struct Linker {
    diagnostics: Vec<Diagnostic>,
    ids: BTreeMap<String, String>,
}

impl Linker {
    pub(crate) fn problem(
        &mut self,
        code: &'static str,
        path: impl Into<String>,
        message: impl Into<String>,
        fix: impl Into<String>,
    ) {
        self.diagnostics
            .push(Diagnostic::new(code, path, message, fix));
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

    fn stable_id<T>(&mut self, value: &str, path: &str) -> Option<T>
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
        if !valid_java_type(value) {
            self.problem(
                "model-java-type",
                path,
                format!("`{value}` is not a Java type name"),
                "use an upper-camel-case Java identifier",
            );
        }
    }

    fn java_member(&mut self, value: &str, path: &str) {
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
            });
        if !valid {
            self.problem(
                "model-java-package",
                path,
                format!("`{value}` is not a canonical Java package"),
                "use dot-separated lowercase Java package segments",
            );
        }
    }

    pub(crate) fn sql_identifier(&mut self, value: &str, path: &str) {
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
                format!("`{value}` is not a canonical SQL identifier"),
                "use lowercase snake_case without quoting",
            );
        }
    }

    fn entity_ref(
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
                "use an entity label declared under `[entities]`",
            );
            None
        })
    }

    fn field_refs(
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
                        self.problem(
                            "model-field-reference",
                            path,
                            format!("`{label}` is not a field on entity id `{entity}`"),
                            "use a field label declared on the referenced entity",
                        );
                        None
                    })
            })
            .collect()
    }

    fn route(&mut self, route: Option<&str>, path: &str, routes: &mut BTreeMap<String, String>) {
        let Some(route) = route else {
            return;
        };
        if !valid_route(route) {
            self.problem(
                "model-route",
                format!("{path}.route"),
                format!("`{route}` is not a canonical HTTP route"),
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

trait ParseStableId: StableId + Sized {
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

fn collision(
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
            format!("{kind} projection `{projection}` is already used at {first}"),
            format!("give each declaration a unique {kind} projection"),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::OperationKind;

    const VALID: &str = r#"
schema = "jails.model.v1"

[project]
id = "project_notes"
name = "Notes"
base_package = "com.example.notes"
java_release = 26
dialect = "postgresql"

[capabilities.database]
id = "cap_database"
kind = "db"

[entities.note]
id = "ent_note"
facets = ["record", "repository", "http"]

[entities.note.fields.id]
id = "fld_note_id"
type = "uuid"
primary_key = true

[entities.note.fields.title]
id = "fld_note_title"
type = "string"
non_blank = true

[operations.note_created]
kind = "event"
id = "op_note_created"
on = "note"
fields = ["id", "title"]

[operations.create_note]
kind = "command"
id = "op_create_note"
on = "note"
fields = ["title"]
route = "POST /notes"

[operations.open_notes]
kind = "query"
id = "op_open_notes"
on = "note"
filters = ["title"]
order_by = ["id"]
limit = 50
route = "GET /notes"

[operations.rename_note]
kind = "transition"
id = "op_rename_note"
on = "note"
fields = ["title"]
sets = ["title"]
yields = "note_created"
route = "PATCH /notes/{id}"
"#;

    #[test]
    fn links_every_label_to_a_stable_identity() {
        let model = crate::parse_toml(VALID).unwrap();
        let entity = model.entity(&EntityId::parse("ent_note").unwrap()).unwrap();
        assert_eq!(entity.names.java_type, "Note");
        // Pluralized per §9.7: the table is `notes`, and importing a legacy
        // project must not rename it.
        assert_eq!(entity.names.sql_table, "notes");
        assert_eq!(model.node_count(), 9);
        assert_eq!(
            model.canonical_json().unwrap(),
            model.canonical_json().unwrap()
        );

        let operation = model
            .operations
            .get(&OperationId::parse("op_create_note").unwrap())
            .unwrap();
        let OperationKind::Command(command) = &operation.kind else {
            panic!("create_note did not link as a command")
        };
        assert_eq!(operation.names.java_type, "CreateNote");
        assert_eq!(command.on.as_str(), "ent_note");
        assert_eq!(command.fields[0].as_str(), "fld_note_title");
    }

    #[test]
    fn reports_independent_semantic_problems_together() {
        let invalid = VALID
            .replace("id = \"cap_database\"", "id = \"ent_note\"")
            .replace("route = \"GET /notes\"", "route = \"POST /notes\"")
            .replace("filters = [\"title\"]", "filters = [\"missing\"]");
        let diagnostics = crate::parse_toml(&invalid).unwrap_err();
        let codes = diagnostics
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<BTreeSet<_>>();
        assert!(codes.contains("model-id-collision"));
        assert!(codes.contains("model-route-collision"));
        assert!(codes.contains("model-field-reference"));
    }

    #[test]
    fn a_label_rename_preserves_explicit_identity() {
        let renamed = VALID
            .replace("[entities.note]", "[entities.memo]")
            .replace("[entities.note.fields", "[entities.memo.fields")
            .replace("on = \"note\"", "on = \"memo\"");
        let before = crate::parse_toml(VALID).unwrap();
        let after = crate::parse_toml(&renamed).unwrap();
        let id = EntityId::parse("ent_note").unwrap();
        assert_eq!(
            before.entity(&id).unwrap().id,
            after.entity(&id).unwrap().id
        );
        assert_eq!(after.entity(&id).unwrap().label, "memo");
    }

    #[test]
    fn semantic_removal_refuses_dangling_operation_edges() {
        let mut model = crate::parse_toml(VALID).unwrap();
        let event = OperationId::parse("op_note_created").unwrap();
        let error = model
            .apply(crate::ModelPatch::RemoveOperation(event))
            .unwrap_err();
        assert!(error.contains("rename_note"), "{error}");
        assert!(error.contains("remove those transitions"), "{error}");

        let entity = EntityId::parse("ent_note").unwrap();
        let error = model
            .apply(crate::ModelPatch::RemoveEntity(entity))
            .unwrap_err();
        assert!(error.contains("create_note"), "{error}");
        assert!(error.contains("open_notes"), "{error}");
    }

    #[test]
    fn ejection_is_a_semantic_ownership_edge() {
        let source = format!(
            "{VALID}\n[ejections.database]\nid = \"eject_database\"\ntarget = \"art_cap_database_ent_note_repository\"\n"
        );
        let mut model = crate::parse_toml(&source).unwrap();
        let capability = CapabilityId::parse("cap_database").unwrap();
        let error = model
            .apply(crate::ModelPatch::RemoveCapability(capability))
            .unwrap_err();
        assert!(error.contains("reader-owned"), "{error}");

        let error = model
            .apply(crate::ModelPatch::AddEjection(crate::Ejection {
                id: EjectionId::parse("eject_missing").unwrap(),
                label: "missing".to_string(),
                target: "missing".to_string(),
            }))
            .unwrap_err();
        assert!(error.contains("does not exist"), "{error}");
    }

    #[test]
    fn unknown_source_keys_fail_closed() {
        let diagnostics = crate::parse_toml(
            &VALID.replace("java_release = 26", "java_release = 26\njava_relese = 26"),
        )
        .unwrap_err();
        assert_eq!(diagnostics.diagnostics[0].code, "model-syntax");
    }
}
