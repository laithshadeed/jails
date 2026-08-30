mod cst;
mod edit;
mod format;
mod parser;
mod token;

pub use cst::{DeclarationCst, DocumentCst, MemberCst};
pub use edit::{
    append_declaration as append_jdl_declaration, insert_entity_member as insert_jdl_entity_member,
    remove_declaration as remove_jdl_declaration, remove_entity_member as remove_jdl_entity_member,
    rename_declaration as rename_jdl_declaration,
    replace_entity_member as replace_jdl_entity_member, set_app_property as set_jdl_app_property,
    set_entity_attribute as set_jdl_entity_attribute,
};
pub use format::format;
pub use token::{Span, Token, TokenKind};

use crate::{AppModel, Diagnostics};

pub(super) fn is_v1(input: &str) -> bool {
    let Ok(tokens) = token::lex(input) else {
        return first_non_comment_line(input)
            .is_some_and(|line| line.split_whitespace().next() == Some("jdl"));
    };
    let mut syntax = tokens.iter().filter(|token| {
        !matches!(
            token.kind,
            TokenKind::Whitespace
                | TokenKind::Comment
                | TokenKind::Newline
                | TokenKind::TriviaNewline
                | TokenKind::Eof
        )
    });
    syntax
        .next()
        .is_some_and(|token| token.text(input) == "jdl")
}

pub fn parse_cst(input: &str) -> Result<DocumentCst, Diagnostics> {
    let tokens = token::lex(input)?;
    Ok(parser::parse(input, tokens)?.cst)
}

pub(super) fn parse(input: &str) -> Result<AppModel, Diagnostics> {
    let tokens = token::lex(input)?;
    let parsed = parser::parse(input, tokens)?;
    crate::linker::link(parsed.source)
}

fn first_non_comment_line(input: &str) -> Option<&str> {
    input.lines().find_map(|line| {
        let line = line.trim();
        (!line.is_empty() && !line.starts_with("//")).then_some(line)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        BindingSource, ComponentKind, ComponentReference, DependencyScope, Facet, ModelPatch,
        OperationKind, ParameterSource, Precondition, RequestFormat, SettingTarget, SortDirection,
        StableId, UnitId, Value,
    };
    use std::collections::BTreeSet;

    const CORE: &str = r#"// retained lead comment
jdl 1

app Notes @id(project_notes) {
  pkg com.example.notes
  java 26
  platform spring
  build maven
  storage postgres
}

cap api
dep org.example:widget @id(dep_widget) @version("1.2.3") @scope(test)
prop server.port = 8080 @target(test)

enum Status {
  OPEN
  IN_PROGRESS = "in_progress"
}

entity Task @id(ent_task) {
  use scaffold, factory, dto

  id: uuid @id(fld_task_id) @pk
  title: string @notBlank @length(1..200) @index
  done: boolean?
  index [title desc]
}
"#;

    #[test]
    fn cst_round_trips_every_byte_and_finds_declaration_spans() {
        let cst = parse_cst(CORE).unwrap();
        assert_eq!(cst.reconstruct(), CORE);
        assert_eq!(cst.declarations.len(), 6);
        let task = cst
            .declarations
            .iter()
            .find(|declaration| declaration.name.as_deref() == Some("Task"))
            .unwrap();
        assert!(cst.declaration_text(task).starts_with("entity Task"));
        assert!(
            cst.declaration_text(task)
                .trim_end_matches(['\r', '\n'])
                .ends_with('}')
        );
        let edited = cst
            .replace_declaration(task, "entity WorkItem @id(ent_task) {}\n")
            .unwrap();
        assert!(edited.contains("// retained lead comment\n"));
        assert!(edited.contains("entity WorkItem @id(ent_task) {}\n"));
        assert!(!edited.contains("entity Task @id(ent_task)"));
    }

    #[test]
    fn v1_lowers_directly_to_the_existing_typed_linker_boundary() {
        let model = parse(CORE).unwrap();
        assert_eq!(model.project.id.as_str(), "project_notes");
        assert_eq!(model.project.dialect, "postgresql");
        assert!(
            model
                .capabilities
                .values()
                .any(|capability| capability.kind == "db"),
            "primary PostgreSQL storage must derive database support"
        );
        let task = model
            .entities
            .values()
            .find(|entity| entity.label == "task")
            .unwrap();
        assert!(task.facets.contains(&Facet::Record));
        assert!(task.facets.contains(&Facet::Repository));
        assert!(task.facets.contains(&Facet::Factory));
        assert!(task.facets.contains(&Facet::Dto));
        let title = task
            .fields
            .iter()
            .find(|field| field.label == "title")
            .unwrap();
        assert_eq!(title.length.as_ref().unwrap().min, Some(1));
        assert_eq!(title.length.as_ref().unwrap().max, Some(200));
        assert!(title.indexed);
        let status = model
            .entities
            .values()
            .find(|entity| entity.label == "status")
            .unwrap();
        assert_eq!(status.enum_constants[1].wire_value(), "in_progress");
        assert_eq!(
            model.dependencies.values().next().unwrap().scope,
            DependencyScope::Test
        );
        assert_eq!(
            model.settings.values().next().unwrap().target,
            SettingTarget::Test
        );
    }

    #[test]
    fn field_attributes_link_explicit_and_derived_semantics() {
        let model = parse(
            r#"jdl 1
app Work {
  pkg com.example.work
  java 26
  platform spring
  build maven
  storage postgres
}

enum Status {
  OPEN
}

entity Task {
  id: uuid @pk
  tenantId: uuid @scope
  organizationId: long @scope(claim: "organization")
  attempts: int @positive @default(1)
  version: long @version @nonnegative
  status: Status @default(OPEN)
  createdAt: instant @default(now())
  updatedAt: datetime @default(now()) @updated
  note: string? @length(..200)
}
"#,
        )
        .unwrap();
        let task = model
            .entities
            .values()
            .find(|entity| entity.label == "task")
            .unwrap();
        let field = |label: &str| {
            task.fields
                .iter()
                .find(|field| field.label == label)
                .unwrap()
        };

        let id_default = field("id").semantics.default.as_ref().unwrap();
        assert!(id_default.derived);
        assert!(matches!(
            id_default.value,
            Value::Function { ref name, ref arguments }
                if name == "uuid7" && arguments.is_empty()
        ));
        assert_eq!(
            field("tenant_id").semantics.scope.as_ref().unwrap().claim,
            "tenantId"
        );
        assert!(!field("tenant_id").semantics.scope.as_ref().unwrap().pinned);
        assert_eq!(
            field("organization_id")
                .semantics
                .scope
                .as_ref()
                .unwrap()
                .claim,
            "organization"
        );
        assert!(
            field("organization_id")
                .semantics
                .scope
                .as_ref()
                .unwrap()
                .pinned
        );
        assert!(field("attempts").semantics.positive);
        assert!(
            !field("attempts")
                .semantics
                .default
                .as_ref()
                .unwrap()
                .derived
        );
        assert!(field("version").semantics.version);
        assert!(matches!(
            field("version")
                .semantics
                .default
                .as_ref()
                .unwrap()
                .value,
            Value::Integer(ref value) if value == "0"
        ));
        assert!(field("updated_at").semantics.updated);
        assert_eq!(field("note").length.as_ref().unwrap().max, Some(200));
    }

    #[test]
    fn field_semantics_fail_closed_with_specific_diagnostics() {
        let app = "jdl 1\napp Demo {\n pkg com.example.demo\n java 26\n platform spring\n build maven\n storage postgres\n}\n";
        let diagnostic = |field: &str| {
            parse(&format!("{app}entity Task {{\n {field}\n}}\n"))
                .unwrap_err()
                .diagnostics[0]
                .code
        };

        assert_eq!(
            diagnostic("value: string @positive"),
            "model-numeric-constraint-type"
        );
        assert_eq!(diagnostic("tenantId: uuid? @scope"), "model-scope-required");
        assert_eq!(diagnostic("version: int @version"), "model-version-type");
        assert_eq!(
            diagnostic("updatedAt: string @updated"),
            "model-updated-type"
        );
        assert_eq!(
            diagnostic("id: long @default(identity())"),
            "model-field-default-type"
        );
        assert_eq!(diagnostic("name: string @default(name)"), "JDL0917");
        assert_eq!(diagnostic("id: uuid @pk @pk"), "jdl-syntax");
        assert_eq!(diagnostic("id: uuid @pk()"), "jdl-syntax");
        assert_eq!(diagnostic("tenantId: uuid @scope()"), "JDL0513");

        let duplicate_claim = parse(&format!(
            "{app}entity Task {{\n tenantId: uuid @scope(claim: \"tenant\")\n ownerId: long @scope(claim: \"tenant\")\n}}\n"
        ))
        .unwrap_err();
        assert!(
            duplicate_claim
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "model-scope-claim-collision")
        );
    }

    #[test]
    fn scope_and_managed_fields_are_checked_across_operations() {
        let valid = parse(
            r#"jdl 1
app Work {
  pkg com.example.work
  java 26
  platform spring
  build maven
  storage postgres
}

cap security

entity Task {
  id: uuid @pk
  tenantId: uuid @scope(claim: "tenant")
  title: string
  version: long @version @nonnegative
  updatedAt: instant @default(now()) @updated

  query All() {
    route GET "/tasks"
  }

  transition Rename(version, title) {
    update [title]
    if-match required
    route PATCH "/tasks/{id}"
  }
}
"#,
        )
        .unwrap();
        assert_eq!(valid.operations.len(), 2);

        let without_security = parse(
            r#"jdl 1
app Work {
  pkg com.example.work
  java 26
  platform spring
  build maven
  storage postgres
}
entity Task {
  id: uuid @pk
  tenantId: uuid @scope
  query All() {
    route GET "/tasks"
  }
}
"#,
        )
        .unwrap_err();
        assert!(
            without_security
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "model-scope-security")
        );

        let app = "jdl 1\napp Work {\n pkg com.example.work\n java 26\n platform spring\n build maven\n storage postgres\n}\ncap security\n";
        let managed = parse(&format!(
            "{app}entity Task {{\n id: uuid @pk\n tenantId: uuid @scope\n updatedAt: instant @updated\n command Create(tenantId) {{\n  set updatedAt = ZERO\n }}\n}}\n"
        ))
        .unwrap_err();
        let codes = managed
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<BTreeSet<_>>();
        assert!(codes.contains("model-managed-field-input"), "{codes:?}");
        assert!(codes.contains("model-managed-field-target"), "{codes:?}");

        let missing_version = parse(&format!(
            "{app}entity Task {{\n id: uuid @pk\n title: string\n transition Rename(title) {{\n  update [title]\n  if-match required\n }}\n}}\n"
        ))
        .unwrap_err();
        assert!(
            missing_version
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "model-transition-version-count")
        );

        let duplicate_version = parse(&format!(
            "{app}entity Task {{\n id: uuid @pk\n firstVersion: long @version\n secondVersion: long @version\n}}\n"
        ))
        .unwrap_err();
        assert!(
            duplicate_version
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "model-version-count")
        );
    }

    #[test]
    fn missing_version_and_unknown_v1_words_have_stable_diagnostics() {
        let missing = parse("app Demo {}\n").unwrap_err();
        assert_eq!(missing.diagnostics[0].code, "JDL0001");
        let unknown = parse(
            "jdl 1\napp Demo {\n pkg com.example.demo\n java 26\n platform spring\n build maven\n storage postgres\n}\nwat ever\n",
        )
        .unwrap_err();
        assert_eq!(unknown.diagnostics[0].code, "JDL0101");

        let unsupported = crate::parse_jdl("jdl 2\n").unwrap_err();
        assert_eq!(unsupported.diagnostics[0].code, "JDL0001");
    }

    #[test]
    fn v1_parser_has_no_toml_frontend_dependency() {
        let parser = include_str!("parser.rs");
        assert!(!parser.contains("parse_toml"));
        assert!(!parser.contains("toml::"));
    }

    #[test]
    fn operation_vocabulary_links_without_dropping_semantic_facts() {
        let model = parse(
            r#"jdl 1
app Work {
  pkg com.example.work
  java 26
  platform spring
  build maven
  storage postgres
}

entity Author {
  use scaffold
  id: uuid @pk
  email: string @unique
}

entity Task {
  use scaffold
  id: uuid @pk
  authorId: uuid
  title: string
  status: string
  version: long @version

  command Open(Author.email as email, title) {
    resolve authorId from Author.id where Author.email = email
    set status = OPEN
    conflict on [title]
    emit TaskChanged
    route POST "/tasks/actions/open" consumes form
    bind email from form "author_email"
  }

  query ByAuthor(author.email? as email) {
    join Author as author on authorId -> author.id
    order by [title desc, id]
    limit 20
  }

  transition Rename(id, version, title) {
    select [id]
    update [title]
    if-match required
    emit TaskChanged
  }
}

event TaskChanged(id: uuid, source: string @notBlank) {
  partition by id
}
"#,
        )
        .unwrap();

        let open = model
            .operations
            .values()
            .find(|operation| operation.label == "open")
            .unwrap();
        let OperationKind::Command(command) = &open.kind else {
            panic!("Open must be a command");
        };
        assert_eq!(command.semantics.parameters.len(), 2);
        assert!(matches!(
            command.semantics.parameters[0].source,
            ParameterSource::Field(_)
        ));
        assert_eq!(command.semantics.assignments.len(), 1);
        assert!(matches!(
            command.semantics.assignments[0].value,
            Value::EnumConstant(ref value) if value == "OPEN"
        ));
        assert_eq!(command.semantics.resolutions.len(), 1);
        assert_eq!(command.semantics.conflict_key.len(), 1);
        assert_eq!(command.semantics.emits.len(), 1);
        assert_eq!(
            command.semantics.route.as_ref().unwrap().consumes,
            Some(RequestFormat::Form)
        );
        assert_eq!(command.semantics.bindings[0].source, BindingSource::Form);
        assert_eq!(
            command.semantics.bindings[0].wire_name.as_deref(),
            Some("author_email")
        );
        assert_eq!(command.route.as_deref(), Some("POST /tasks/actions/open"));

        let by_author = model
            .operations
            .values()
            .find(|operation| operation.label == "by_author")
            .unwrap();
        let OperationKind::Query(query) = &by_author.kind else {
            panic!("ByAuthor must be a query");
        };
        assert!(query.semantics.parameters[0].optional_filter);
        assert_eq!(query.semantics.joins.len(), 1);
        assert_eq!(query.semantics.order.len(), 2);
        assert_eq!(query.semantics.order[0].direction, SortDirection::Desc);
        assert_eq!(query.semantics.limit, Some(20));
        assert_eq!(query.semantics.limit, Some(20));

        let rename = model
            .operations
            .values()
            .find(|operation| operation.label == "rename")
            .unwrap();
        let OperationKind::Transition(transition) = &rename.kind else {
            panic!("Rename must be a transition");
        };
        assert_eq!(transition.semantics.select.len(), 1);
        assert_eq!(transition.semantics.update.len(), 1);
        assert_eq!(
            transition.semantics.precondition,
            Some(Precondition::Required)
        );
        assert_eq!(transition.semantics.emits.len(), 1);

        let changed = model
            .operations
            .values()
            .find(|operation| operation.label == "task_changed")
            .unwrap();
        let OperationKind::Event(event) = &changed.kind else {
            panic!("TaskChanged must be an event");
        };
        assert_eq!(event.semantics.parameters.len(), 2);
        assert_eq!(event.semantics.partition_by.as_deref(), Some("id"));
        assert!(matches!(
            event.semantics.parameters[0].source,
            ParameterSource::Typed(_)
        ));
    }

    #[test]
    fn operation_grammar_fails_closed_for_wrong_scope_and_wrong_member() {
        let app = "jdl 1\napp Demo {\n pkg com.example.demo\n java 26\n platform spring\n build maven\n storage postgres\n}\n";
        let top_command = parse(&format!("{app}command Bad()\n")).unwrap_err();
        assert_eq!(top_command.diagnostics[0].code, "JDL0903");

        let untyped_event = parse(&format!("{app}event Bad(id)\n")).unwrap_err();
        assert_eq!(untyped_event.diagnostics[0].code, "JDL0906");

        let wrong_member = parse(&format!(
            "{app}entity Task {{\n id: uuid @pk\n query Bad() {{\n  set id = 1\n }}\n}}\n"
        ))
        .unwrap_err();
        assert_eq!(wrong_member.diagnostics[0].code, "JDL0916");
    }

    #[test]
    fn closed_component_vocabulary_links_and_projects_supported_emitters() {
        let model = parse(
            r#"jdl 1
app Work {
  pkg com.example.work
  java 26
  platform spring
  build maven
  storage postgres
}

entity Task {
  use scaffold
  id: uuid @pk
  name: string

  command AddItem(name) {}
}

component class Clock

component strategy RewardRule {
  on Task
  variant Coffee
  variant LargeTransaction
}

component sealed Outcome {
  variant Accepted(id: uuid)
  variant Rejected(reason: string @notBlank)
}

component controller Health(id: uuid) {
  on Task
  route POST "/health" consumes json
  bind id from query "task_id"
}

component durable-job ItemDispatcher(id: uuid, name: string @notBlank) {
  on AddItem
  yields Task
}

component cases Checkout {
  source "specs/checkout.md"
}
"#,
        )
        .unwrap();

        assert_eq!(model.components.len(), 6);
        let strategy = model
            .components
            .values()
            .find(|component| component.kind == ComponentKind::Strategy)
            .unwrap();
        assert_eq!(strategy.variants.len(), 2);
        assert!(matches!(strategy.on, Some(ComponentReference::Entity(_))));

        let sealed = model
            .components
            .values()
            .find(|component| component.kind == ComponentKind::Sealed)
            .unwrap();
        assert_eq!(sealed.variants[0].parameters.len(), 1);

        let durable = model
            .components
            .values()
            .find(|component| component.kind == ComponentKind::DurableJob)
            .unwrap();
        assert!(matches!(durable.on, Some(ComponentReference::Operation(_))));
        assert!(matches!(
            durable.yields,
            Some(ComponentReference::Entity(_))
        ));

        let cases = model
            .components
            .values()
            .find(|component| component.kind == ComponentKind::Cases)
            .unwrap();
        assert_eq!(cases.source.as_deref(), Some("specs/checkout.md"));

        assert_eq!(model.units.len(), 4);
        let controller = model
            .units
            .values()
            .find(|unit| unit.kind == crate::UnitKind::Controller)
            .unwrap();
        assert_eq!(controller.java_type, "HealthController");
        assert_eq!(controller.on.as_deref(), Some("Task"));
        assert_eq!(controller.endpoint.as_ref().unwrap().path, "/health");

        let add_item = model
            .operations
            .values()
            .find(|operation| operation.label == "add_item")
            .unwrap();
        let mut removal = model.clone();
        assert!(
            removal
                .apply(ModelPatch::RemoveOperation(add_item.id.clone()))
                .unwrap_err()
                .contains("item_dispatcher")
        );
        let mut removal = model.clone();
        let derived = UnitId::parse(controller.id.to_string()).unwrap();
        assert!(
            removal
                .apply(ModelPatch::RemoveUnit(derived))
                .unwrap_err()
                .contains("derived from a typed component")
        );
    }

    #[test]
    fn component_registry_is_exhaustive_and_shapes_fail_closed() {
        assert_eq!(ComponentKind::ALL.len(), 23);
        for kind in ComponentKind::ALL {
            assert_eq!(ComponentKind::parse(kind.label()).unwrap(), kind);
        }

        let app = "jdl 1\napp Demo {\n pkg com.example.demo\n java 26\n platform spring\n build maven\n storage postgres\n}\n";
        let unknown = parse(&format!("{app}component mystery Thing\n")).unwrap_err();
        assert_eq!(unknown.diagnostics[0].code, "JDL0931");

        let incomplete = parse(&format!(
            "{app}component strategy RewardRule {{\n variant Coffee\n}}\n"
        ))
        .unwrap_err();
        assert_eq!(
            incomplete.diagnostics[0].code,
            "model-component-member-missing"
        );

        let forbidden = parse(&format!(
            "{app}component handler Ping(id: uuid) {{\n bind id from query\n}}\n"
        ))
        .unwrap_err();
        assert_eq!(
            forbidden.diagnostics[0].code,
            "model-component-bindings-forbidden"
        );

        let internal_http = parse(&format!(
            "{app}entity Task {{\n id: uuid @pk\n query Hidden() @internal {{\n  route GET \"/hidden\"\n }}\n}}\n"
        ))
        .unwrap_err();
        assert_eq!(
            internal_http.diagnostics[0].code,
            "model-operation-internal-http"
        );

        let route_collision = parse(&format!(
            "{app}entity Task {{\n id: uuid @pk\n query Ping() {{\n  route GET \"/ping\"\n }}\n}}\ncomponent controller Other {{\n route GET \"/ping\"\n}}\n"
        ))
        .unwrap_err();
        assert_eq!(route_collision.diagnostics[0].code, "model-route-collision");
    }

    #[test]
    fn projection_selectors_expand_after_collection_and_keep_typed_arguments() {
        let model = parse(
            r#"jdl 1
app Work {
  pkg com.example.work
  java 26
  platform spring
  build maven
  storage postgres
}

use dto for * except AuditEntry
use factory for Invoice, RetiredEntity
use search(fields: [title]) for Invoice

entity AuditEntry {
  id: uuid @pk
}

entity Invoice {
  id: uuid @pk
  title: string
}

entity RetiredEntity @retired {
  id: uuid @pk
}
"#,
        )
        .unwrap();

        assert_eq!(model.projections.len(), 5);
        let invoice = model
            .entities
            .values()
            .find(|entity| entity.label == "invoice")
            .unwrap();
        let audit = model
            .entities
            .values()
            .find(|entity| entity.label == "audit_entry")
            .unwrap();
        let retired = model
            .entities
            .values()
            .find(|entity| entity.label == "retired_entity")
            .unwrap();
        assert!(invoice.facets.contains(&Facet::Dto));
        assert!(invoice.facets.contains(&Facet::Factory));
        assert!(invoice.facets.contains(&Facet::Search));
        assert!(!audit.facets.contains(&Facet::Dto));
        assert!(retired.facets.contains(&Facet::Dto));
        assert!(retired.facets.contains(&Facet::Factory));
        assert!(
            !retired.active,
            "retired entities retain selector membership"
        );

        let search = model
            .projections
            .values()
            .find(|projection| projection.kind.label() == "search")
            .unwrap();
        let crate::ProjectionKind::Search { fields } = &search.kind else {
            panic!("search must retain its typed field list");
        };
        assert_eq!(fields.len(), 1);
        assert_eq!(invoice.field(&fields[0]).unwrap().label, "title");
    }

    #[test]
    fn selectors_fail_closed_for_unknown_names_and_conflicting_arguments() {
        let app = "jdl 1\napp Demo {\n pkg com.example.demo\n java 26\n platform spring\n build maven\n storage postgres\n}\n";
        let unknown = parse(&format!(
            "{app}use dto for Missing\nentity Task {{\n id: uuid @pk\n}}\n"
        ))
        .unwrap_err();
        assert_eq!(
            unknown.diagnostics[0].code,
            "model-projection-selector-reference"
        );

        let conflict = parse(&format!(
            "{app}use http(path: \"/other\") for Task\nentity Task {{\n use scaffold(path: \"/tasks\")\n id: uuid @pk\n}}\n"
        ))
        .unwrap_err();
        assert_eq!(
            conflict.diagnostics[0].code,
            "model-projection-configuration-conflict"
        );
    }

    #[test]
    fn relations_link_ordered_composite_keys_actions_and_cardinality() {
        let model = parse(
            r#"jdl 1
app Work {
  pkg com.example.work
  java 26
  platform spring
  build maven
  storage postgres
}

entity Owner {
  tenantId: uuid
  id: uuid
  pk [tenantId, id] @id(pk_owner)
}

entity Item {
  id: uuid @pk
  ownerTenantId: uuid
  ownerId: uuid
  unique [ownerTenantId, ownerId] @id(uq_item_owner)

  relation owner to Owner @id(rel_item_owner) {
    map ownerTenantId -> Owner.tenantId
    map ownerId -> Owner.id
    on delete cascade
    on update restrict
  }
}
"#,
        )
        .unwrap();

        let owner = model
            .entities
            .values()
            .find(|entity| entity.label == "owner")
            .unwrap();
        assert_eq!(owner.constraints.len(), 1);
        let relation = model.relations.values().next().unwrap();
        assert_eq!(relation.id.as_str(), "rel_item_owner");
        assert_eq!(relation.mappings.len(), 2);
        assert_eq!(relation.on_delete, crate::ReferentialAction::Cascade);
        assert_eq!(relation.on_update, crate::ReferentialAction::Restrict);
        assert_eq!(relation.cardinality, crate::RelationCardinality::OneToOne);
        assert_eq!(relation.sql_name, "fk_items_owner");

        let mut removal = model.clone();
        let owner_id = owner.id.clone();
        assert!(
            removal
                .apply(ModelPatch::RemoveEntity(owner_id))
                .unwrap_err()
                .contains("owner")
        );
        let local = relation.mappings[0].local.clone();
        let mut removal = model.clone();
        assert!(
            removal
                .apply(ModelPatch::RemoveField {
                    entity: relation.child.clone(),
                    field: local,
                    confirmed_column: "owner_tenant_id".to_string(),
                })
                .unwrap_err()
                .contains("owner")
        );
    }

    #[test]
    fn relation_invariants_reject_partial_nullable_non_keys_and_set_null_required() {
        let source = r#"jdl 1
app Work {
  pkg com.example.work
  java 26
  platform spring
  build maven
  storage postgres
}

entity Parent {
  first: uuid
  second: uuid
  unique [first, second] @id(uq_parent_pair)
}

entity Child {
  id: uuid @pk
  first: uuid
  second: uuid?
  relation parent to Parent {
    map first -> first
    map second -> second
    on delete set-null
  }
}
"#;
        let error = parse(source).unwrap_err();
        let codes = error
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>();
        assert!(codes.contains(&"model-relation-partial-nullability"));
        assert!(codes.contains(&"model-relation-set-null-required"));
    }

    #[test]
    fn required_cascade_cycles_are_rejected_after_all_relations_link() {
        let error = parse(
            r#"jdl 1
app Work {
  pkg com.example.work
  java 26
  platform spring
  build maven
  storage postgres
}

entity Alpha {
  id: uuid @pk
  betaId: uuid
  relation beta to Beta {
    map betaId -> id
    on delete cascade
  }
}

entity Beta {
  id: uuid @pk
  alphaId: uuid
  relation alpha to Alpha {
    map alphaId -> id
    on delete cascade
  }
}
"#,
        )
        .unwrap_err();
        assert!(
            error
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "model-relation-cascade-cycle")
        );
    }
}
