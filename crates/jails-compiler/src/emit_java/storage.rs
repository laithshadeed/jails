//! The three storage implementations as recipe rows: the in-memory adapter,
//! the JDBC one, and the JDBC search adapter.
//!
//! **A stored entity is a node of its own, not the entity.** Which owner an
//! adapter belongs to and which one is the project's bean are facts of the
//! *captured build* -- `emit::jdbc_on_classpath` -- and an entity cannot carry
//! them; the artifact id of a storage-scoped boundary is
//! `art_<storage>_<entity>_<role>`, which is not what the recipe loop spells
//! for an entity either. [`Stored`] holds both: the owner the caller resolved,
//! and the id prefix that owner implies. Everything else about these three
//! files -- their templates, their placement, their imports -- is a row.
//!
//! What a template cannot say is a named fragment below: a column list, an
//! insert's column and value lists, the `on conflict` clause, the bind chain,
//! the primary key's Java type in both its spellings, and the annotation the
//! in-memory adapter carries only when it is the bean.
//!
//! The contract both adapters are held to and the two proofs that call it stay
//! functions in [`super::repository`]: they reach across nodes for a sample and
//! for the ancestor rows a foreign key demands, which is the same reason
//! `emit_http` does.

use super::*;
use crate::recipe::{
    BootCondition, Fragment, Import, JavaFile, Naming, Node, Placement, Recipe, Rendered, SourceSet,
};
use jails_model::boundary;

/// Which of the three implementations this node renders.
///
/// The discriminant a recipe cannot carry: three files with three provenances
/// -- a different owner scope, a different `ejection_id`, a different set of
/// semantic ids -- and [`Node::provenance`] is the node's, not the recipe's.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Fake,
    Jdbc,
    Search,
}

/// One entity, stored by one owner.
pub(super) struct Stored {
    kind: Kind,
    entity: Entity,
    /// The capability the adapter belongs to: `cap_db`, `cap_fake`, or
    /// `cap_scaffold_default` where the scaffold asked for the port and no
    /// capability has claimed it.
    capability_id: String,
    /// The artifact id's owner prefix. **Entity-scoped for the in-memory
    /// adapter and storage-scoped for the JDBC ones**, because the in-memory
    /// adapter's owner switches from `cap_scaffold_default` to `cap_fake` the
    /// moment `add fake` is run, at an unchanged path -- a capability-scoped id
    /// would make the same bytes a new artifact with no merge base, which
    /// reconciliation refuses as reader-owned.
    id: String,
    /// Whether the in-memory adapter is this project's repository bean.
    bean: bool,
}

impl Stored {
    /// The in-memory adapter, and whether it is the project's repository bean.
    ///
    /// **`bean` is true exactly when nothing else implements the port.** With
    /// `db` declared the JDBC adapter carries `@Repository` and this one is a
    /// plain class a test constructs; without it, this *is* the
    /// implementation, and leaving it unannotated gives a scaffolded Spring
    /// project a service constructor-injecting a port no bean satisfies.
    /// Annotating both would make two beans qualify for one injection point,
    /// which is the ambiguity `jails beans` reports.
    pub(super) fn fake(capability_id: &str, entity: &Entity, bean: bool) -> Self {
        Self {
            kind: Kind::Fake,
            id: entity.id.as_str().to_string(),
            entity: entity.clone(),
            capability_id: capability_id.to_string(),
            bean,
        }
    }

    pub(super) fn jdbc(capability_id: &str, entity: &Entity) -> Self {
        Self::by_storage(Kind::Jdbc, capability_id, entity)
    }

    pub(super) fn search(capability_id: &str, entity: &Entity) -> Self {
        Self::by_storage(Kind::Search, capability_id, entity)
    }

    fn by_storage(kind: Kind, capability_id: &str, entity: &Entity) -> Self {
        Self {
            kind,
            id: format!("{capability_id}_{}", entity.id.as_str()),
            entity: entity.clone(),
            capability_id: capability_id.to_string(),
            bean: false,
        }
    }

    /// The recipe this node renders through.
    pub(super) fn recipe(&self) -> &'static Recipe<Self> {
        match self.kind {
            Kind::Fake => &FAKE,
            Kind::Jdbc => &JDBC,
            Kind::Search => &SEARCH,
        }
    }
}

/// The typed values of a stored entity its templates and rows may spell.
#[derive(Clone, Copy)]
pub(super) enum Key {
    /// `{{table}}`: the entity's SQL table.
    Table,
    /// `{{key_column}}`: the primary key's column.
    PrimaryColumn,
    /// `{{key}}`: the primary key's record accessor.
    PrimaryMember,
    /// The entity's Java type, in `domain`.
    Record,
    /// The repository port, in `repository`.
    Port,
    /// The search port, in `ports.search`.
    SearchPort,
}

impl Node for Stored {
    type Key = Key;

    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.entity.names.java_type
    }

    fn describe(&self) -> String {
        format!("entity `{}`", self.entity.names.java_type)
    }

    fn key(&self, _: &AppModel, key: Key) -> Result<(&'static str, String), Diagnostic> {
        let record = &self.entity.names.java_type;
        Ok(match key {
            Key::Table => ("table", self.entity.names.sql_table.clone()),
            Key::PrimaryColumn => (
                "key_column",
                primary_key(&self.entity)?.names.sql_column.clone(),
            ),
            Key::PrimaryMember => ("key", primary_key(&self.entity)?.names.java_member.clone()),
            Key::Record => ("record", record.clone()),
            Key::Port => ("port", format!("{record}Repository")),
            Key::SearchPort => ("search_port", format!("{record}Search")),
        })
    }

    fn file_keys(&self, _: &str, template_class: &str) -> Vec<(&'static str, String)> {
        vec![
            ("class", template_class.to_string()),
            ("name", self.entity.names.java_type.clone()),
        ]
    }

    /// **The capability shows in `semantic_ids`, and the search adapter's does
    /// not.** Both spellings are what the emitters wrote before these became
    /// rows, and an artifact's semantic ids are what `destroy` and the removal
    /// guards read.
    fn provenance(&self, artifact_id: String, ejectable: bool, pass: &'static str) -> Provenance {
        let entity = self.entity.id.as_str().to_string();
        let (ejection_id, semantic_ids) = match self.kind {
            Kind::Fake => (None, BTreeSet::from([self.capability_id.clone(), entity])),
            Kind::Jdbc => (None, BTreeSet::from([self.capability_id.clone(), entity])),
            Kind::Search => (Some(self.capability_id.clone()), BTreeSet::from([entity])),
        };
        Provenance {
            artifact_id,
            ejection_id,
            ejectable,
            semantic_ids,
            compiler_pass: pass.to_string(),
        }
    }

    fn header(&self) -> bool {
        true
    }

    /// None of the three is a test.
    fn splices_test_container(&self, _: SourceSet) -> bool {
        false
    }

    fn package_for(&self, model: &AppModel, package: Package) -> String {
        entity_package(model, &self.entity, package)
    }
}

const fn adapter(
    role: &'static str,
    template: crate::Template,
    layer: Package,
    class: Naming<Stored>,
    imports: &'static [Import<Stored>],
) -> JavaFile<Stored> {
    JavaFile {
        role,
        template,
        before_boot: None,
        imports,
        only_when: None,
        source_set: SourceSet::Main,
        placement: Placement::Layer(layer),
        ejectable: true,
        class,
        template_class: class,
    }
}

const fn recipe(
    files: &'static [JavaFile<Stored>],
    keys: &'static [Key],
    fragments: &'static [Fragment<Stored>],
    substitutions: &'static [(&'static str, &'static str)],
    default_package: fn(&AppModel, &Stored) -> String,
    pass: &'static str,
) -> Recipe<Stored> {
    Recipe {
        substitutions,
        keys,
        fragments,
        requires: &[],
        files,
        files_when: BootCondition::Any,
        resources: &[],
        dependencies: &[],
        properties: &[],
        compose_services: &[],
        build_features: &[],
        default_package,
        pass,
        minimum_boot: None,
    }
}

fn memory_package(model: &AppModel, node: &Stored) -> String {
    node.package_for(model, Package::AdaptersMemory)
}

fn jdbc_package(model: &AppModel, node: &Stored) -> String {
    node.package_for(model, Package::AdaptersJdbc)
}

const FAKE: Recipe<Stored> = recipe(
    &[adapter(
        boundary::REPOSITORY_FAKE.role,
        crate::template!("spring/repository_memory_java.java"),
        Package::AdaptersMemory,
        Naming::Wrap("InMemory", "Repository"),
        &[
            Import::Keyed(Package::Domain, Key::Record),
            Import::Keyed(Package::Repository, Key::Port),
        ],
    )],
    &[Key::PrimaryMember],
    &[
        Fragment::Rendered {
            key: "component",
            render: bean_annotation,
        },
        Fragment::Rendered {
            key: "key_type",
            render: key_type,
        },
        Fragment::Rendered {
            key: "boxed_key_type",
            render: boxed_key_type,
        },
    ],
    &[],
    memory_package,
    "capability-fake",
);

const JDBC: Recipe<Stored> = recipe(
    &[adapter(
        boundary::REPOSITORY_POSTGRES.role,
        crate::template!("spring/repository_jdbc_java.java"),
        Package::AdaptersJdbc,
        Naming::Wrap("Jdbc", "Repository"),
        &[
            Import::Keyed(Package::Domain, Key::Record),
            Import::Keyed(Package::Repository, Key::Port),
        ],
    )],
    &[Key::Table, Key::PrimaryColumn],
    &[
        Fragment::Rendered {
            key: "key_type",
            render: key_type,
        },
        Fragment::Rendered {
            key: "columns",
            render: column_list,
        },
        Fragment::Rendered {
            key: "insert_columns",
            render: insert_columns,
        },
        Fragment::Rendered {
            key: "insert_values",
            render: insert_values,
        },
        Fragment::Rendered {
            key: "conflict",
            render: conflict_clause,
        },
        Fragment::Rendered {
            key: "bindings",
            render: bindings,
        },
    ],
    &[],
    jdbc_package,
    "capability-db",
);

/// The search port's JDBC implementation.
///
/// Two details decide whether this works at all, and both are easy to undo.
///
/// **`websearch_to_tsquery`, not `to_tsquery`.** The latter demands operator
/// syntax and throws a syntax error on anything a person would actually type,
/// a bare two-word phrase included. The former accepts what a search box
/// produces -- quotes, `OR`, `-` -- and never throws on malformed input. A
/// search endpoint that 500s on an apostrophe is what that avoids.
///
/// **The query is a bind parameter.** It is text PostgreSQL parses, not SQL it
/// executes, so there is no injection surface and no escaping to get right.
const SEARCH: Recipe<Stored> = recipe(
    &[adapter(
        boundary::SEARCH_POSTGRES.role,
        crate::template!("spring/search_jdbc_java.java"),
        Package::AdaptersJdbc,
        Naming::Wrap("Jdbc", "Search"),
        &[
            Import::Keyed(Package::Domain, Key::Record),
            Import::Keyed(Package::PortsSearch, Key::SearchPort),
        ],
    )],
    &[Key::Table],
    &[Fragment::Rendered {
        key: "columns",
        render: column_list,
    }],
    &[
        ("search_column", crate::emit_sql::SEARCH_COLUMN),
        (
            "search_configuration",
            crate::emit_sql::SEARCH_CONFIGURATION,
        ),
    ],
    jdbc_package,
    "java-search-adapter",
);

/// **`@Component`, not `@Repository`.** The extra meaning `@Repository`
/// carries is persistence-exception translation, which registers a
/// post-processor that CGLIB-proxies every such bean -- and the in-memory
/// adapter is `final`, so the context dies with "Cannot subclass final class".
/// There is nothing here to translate either: a `LinkedHashMap` throws no
/// `SQLException`. The JDBC adapter keeps `@Repository`, where both halves of
/// that annotation are true.
fn bean_annotation(_: &AppModel, node: &Stored) -> Result<Rendered, Diagnostic> {
    if !node.bean {
        return Ok(Rendered::from(String::new()));
    }
    Ok(Rendered {
        text: "@Component\n".to_string(),
        imports: BTreeSet::from(["org.springframework.stereotype.Component".to_string()]),
    })
}

/// The primary key as a method parameter: `long` where the port declares one.
fn key_type(_: &AppModel, node: &Stored) -> Result<Rendered, Diagnostic> {
    let mut imports = BTreeSet::new();
    let text = java_type(primary_key(&node.entity)?, &mut imports);
    Ok(Rendered { text, imports })
}

/// The primary key as a *type argument*, where `long` has to be boxed.
///
/// `Map<long, Note>` is not a type: `int` and `long` are the only builtins
/// with a Java primitive, and a required one is spelled with it everywhere
/// except inside angle brackets. The parameters above stay primitive, because
/// the port they override declares them that way.
fn boxed_key_type(_: &AppModel, node: &Stored) -> Result<Rendered, Diagnostic> {
    let mut imports = BTreeSet::new();
    let text = crate::emit_java::boxed_java_type(primary_key(&node.entity)?, &mut imports);
    Ok(Rendered { text, imports })
}

/// Every column of the entity, in declaration order: what a `select` reads,
/// what a `returning` hands back, and the order the row mapper binds by.
fn column_list(_: &AppModel, node: &Stored) -> Result<Rendered, Diagnostic> {
    Ok(Rendered::from(columns(&node.entity).join(", ")))
}

/// The columns an insert may name.
///
/// **A `generated always as identity` key is not writable, so `save` must not
/// name it.** PostgreSQL answers an insert that does with *"cannot insert a
/// non-DEFAULT value into column \"id\""*, which makes such a `save`
/// impossible on every entity whose key jails assigns -- the default shape for
/// `id:long@pk`.
fn insert_columns(_: &AppModel, node: &Stored) -> Result<Rendered, Diagnostic> {
    Ok(Rendered::from(written(&node.entity)?.join(", ")))
}

fn insert_values(_: &AppModel, node: &Stored) -> Result<Rendered, Diagnostic> {
    Ok(Rendered::from(
        written(&node.entity)?
            .iter()
            .map(|column| format!(":{column}"))
            .collect::<Vec<_>>()
            .join(", "),
    ))
}

/// The upsert, or nothing where the database assigns the key.
///
/// Insert-only in that case, rather than an upsert on a key the caller cannot
/// have. That is what the row coming back is *for*: the stored row and the
/// argument differ by exactly the key the database assigned.
fn conflict_clause(_: &AppModel, node: &Stored) -> Result<Rendered, Diagnostic> {
    let entity = &node.entity;
    let primary_key = primary_key(entity)?;
    if crate::emit_sql::database_assigned(primary_key)? {
        return Ok(Rendered::from(String::new()));
    }
    let key_column = &primary_key.names.sql_column;
    let updates = entity
        .fields
        .iter()
        .filter(|field| field.id != primary_key.id)
        .map(|field| {
            format!(
                "{} = excluded.{}",
                field.names.sql_column, field.names.sql_column
            )
        })
        .collect::<Vec<_>>();
    let updates = match updates.is_empty() {
        true => format!("{key_column} = excluded.{key_column}"),
        false => updates.join(", "),
    };
    Ok(Rendered::from(format!(
        " on conflict ({key_column}) do update set {updates}"
    )))
}

/// The `.param(..)` chain the insert binds, one line per written column.
///
/// Through the one write expression, because the PostgreSQL driver cannot
/// infer a type for every Java value the record can hold -- and through the
/// *optional* one when the component may be absent, because a conversion
/// applied after `orElse(null)` calls a method on null.
fn bindings(model: &AppModel, node: &Stored) -> Result<Rendered, Diagnostic> {
    let entity = &node.entity;
    let primary_key = primary_key(entity)?;
    let generated_key = crate::emit_sql::database_assigned(primary_key)?;
    let mut imports = BTreeSet::new();
    let mut text = String::new();
    // A loop rather than a `map`, because the write expression may need an
    // import of its own and a closure cannot borrow the set it is filling.
    for field in &entity.fields {
        if generated_key && field.id == primary_key.id {
            continue;
        }
        let accessor = format!("value.{}()", field.names.java_member);
        let value = match field.required {
            true => crate::emit_sql::bound_value(model, field, &accessor, &mut imports),
            false => crate::emit_sql::optional_bound_value(model, field, &accessor, &mut imports),
        };
        text.push_str(&format!(
            "\n                .param(\"{}\", {value})",
            field.names.sql_column
        ));
    }
    Ok(Rendered { text, imports })
}

fn columns(entity: &Entity) -> Vec<&str> {
    entity
        .fields
        .iter()
        .map(|field| field.names.sql_column.as_str())
        .collect()
}

fn written(entity: &Entity) -> Result<Vec<&str>, Diagnostic> {
    let primary_key = primary_key(entity)?;
    let generated_key = crate::emit_sql::database_assigned(primary_key)?;
    Ok(entity
        .fields
        .iter()
        .filter(|field| !(generated_key && field.id == primary_key.id))
        .map(|field| field.names.sql_column.as_str())
        .collect())
}
