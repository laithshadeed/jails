//! An entity's facets as recipe rows.
//!
//! An entity is a [`Node`] like a capability or a component: its id, its Java
//! type, and the two typed values its templates may spell. What differs is
//! that its files are optional one by one -- `use repo` puts the port in and
//! nothing else -- so every row carries an `only_when` reading the facet off
//! the entity, and what a row cannot substitute (a record's components, an
//! enum's constants, a primary key's type) is a named fragment in
//! [`super::fragment`].
//!
//! **Three recipes, because three passes.** The provenance of a facet says
//! `java-facets`; the test-data builder's says `java-test-factory` and the
//! enum's Spring converter's `java-enum-converter`, and the compiler pass a
//! file names is on the recipe rather than the row. The converter's recipe
//! also renders only on a Spring project, which is a recipe-wide condition.

use super::*;
use crate::recipe::{
    BootCondition, Fragment, Import, JavaFile, Naming, Node, Placement, Recipe, SourceSet,
};
use jails_model::boundary;

/// The typed values of an entity its templates may spell.
#[derive(Clone, Copy)]
pub(crate) enum Key {
    /// `{{record}}`: the entity's Java type, and the class `Import::Keyed`
    /// names in `domain`.
    Record,
    /// `{{variable}}`: the record as a parameter name.
    Variable,
}

impl Node for Entity {
    type Key = Key;

    fn id(&self) -> &str {
        self.id.as_str()
    }

    fn name(&self) -> &str {
        &self.names.java_type
    }

    fn describe(&self) -> String {
        format!("entity `{}`", self.names.java_type)
    }

    fn key(&self, _: &AppModel, key: Key) -> Result<(&'static str, String), CompileError> {
        Ok(match key {
            Key::Record => ("record", self.names.java_type.clone()),
            Key::Variable => ("variable", lower_first(&self.names.java_type)),
        })
    }

    fn file_keys(&self, _: &str, template_class: &str) -> Vec<(&'static str, String)> {
        vec![
            ("class", template_class.to_string()),
            ("name", self.names.java_type.clone()),
        ]
    }

    fn provenance(&self, artifact_id: String, ejectable: bool, pass: &'static str) -> Provenance {
        Provenance {
            artifact_id,
            ejection_id: None,
            ejectable,
            semantic_ids: BTreeSet::from([self.id.as_str().to_string()]),
            compiler_pass: pass.to_string(),
        }
    }

    fn header(&self) -> bool {
        true
    }

    /// A facet's companion tests are plain JUnit over the type; the one
    /// integration test an entity gets is the JDBC adapter's, which is not a
    /// row here.
    fn splices_test_container(&self, _: SourceSet) -> bool {
        false
    }

    /// The pinned package, when the entity declares one. See
    /// [`super::entity_package`].
    fn package_for(&self, model: &AppModel, package: Package) -> String {
        entity_package(model, self, package)
    }
}

/// The recipes an entity renders through, in the order they run.
pub(super) const RECIPES: [&Recipe<Entity>; 3] = [&FACETS, &TESTKIT, &CONVERTER];

const fn recipe(
    files: &'static [JavaFile<Entity>],
    fragments: &'static [Fragment<Entity>],
    files_when: BootCondition,
    pass: &'static str,
) -> Recipe<Entity> {
    Recipe {
        substitutions: &[],
        keys: &[Key::Record, Key::Variable],
        fragments,
        requires: &[],
        files,
        files_when,
        resources: &[],
        dependencies: &[],
        properties: &[],
        compose_services: &[],
        build_features: &[],
        default_package: domain_package,
        pass,
        minimum_boot: None,
    }
}

fn domain_package(model: &AppModel, entity: &Entity) -> String {
    entity_package(model, entity, Package::Domain)
}

/// A file one facet puts in one layer. The class the template is written
/// against is the file's own, so `{{class}}` is its name.
const fn facet(
    role: &'static str,
    template: crate::Template,
    layer: Package,
    class: Naming<Entity>,
    only_when: fn(&AppModel, &Entity) -> bool,
    imports: &'static [Import<Entity>],
) -> JavaFile<Entity> {
    JavaFile {
        role,
        template,
        before_boot: None,
        imports,
        only_when: Some(only_when),
        source_set: SourceSet::Main,
        placement: Placement::Layer(layer),
        ejectable: false,
        class,
        template_class: class,
    }
}

fn has_record(_: &AppModel, entity: &Entity) -> bool {
    entity.facets.contains(&Facet::Record)
}

fn has_enum(_: &AppModel, entity: &Entity) -> bool {
    entity.facets.contains(&Facet::Enum)
}

fn has_repository(_: &AppModel, entity: &Entity) -> bool {
    entity.facets.contains(&Facet::Repository)
}

fn has_service(_: &AppModel, entity: &Entity) -> bool {
    entity.facets.contains(&Facet::Service)
}

fn has_events(_: &AppModel, entity: &Entity) -> bool {
    entity.facets.contains(&Facet::Events)
}

fn has_search(_: &AppModel, entity: &Entity) -> bool {
    entity.facets.contains(&Facet::Search)
}

fn has_factory(_: &AppModel, entity: &Entity) -> bool {
    entity.facets.contains(&Facet::Factory)
}

fn has_wire_values(_: &AppModel, entity: &Entity) -> bool {
    fragment::has_wire_values(entity)
}

/// The facets that are one file each: the record, the enum, the repository
/// port, the service, the events port and the search port.
///
/// `dto`, `http` and `seed` are several files from one facet and stay in
/// [`super::emit`] beside these.
const FACETS: Recipe<Entity> = recipe(
    &[
        facet(
            boundary::RECORD.role,
            crate::template!("spring/entity_record_java.java"),
            Package::Domain,
            Naming::Suffix(""),
            has_record,
            &[],
        ),
        facet(
            boundary::ENUM.role,
            crate::template!("spring/entity_enum_java.java"),
            Package::Domain,
            Naming::Suffix(""),
            has_enum,
            &[],
        ),
        facet(
            boundary::REPOSITORY.role,
            crate::template!("spring/entity_repository_java.java"),
            Package::Repository,
            Naming::Suffix("Repository"),
            has_repository,
            &[Import::Role("record")],
        ),
        // **The whole resource surface, not a one-method stub**, and it is the
        // generated `ArchitectureTest` that settles it. That suite's
        // `CONTROLLERS_DO_NOT_EXPOSE_PERSISTENCE` rule forbids a `*Controller`
        // depending on the repository package, and the controller has four
        // methods to serve -- so a service that only saves leaves the two
        // generators contradicting each other, and a freshly scaffolded
        // project fails its own architecture test on the first `mvn test`.
        //
        // A forwarding service is ceremony right up until the first business
        // rule, which is the moment a project without one has to touch every
        // call site in the web layer. The scaffold is code the reader grows;
        // the boundary is the point.
        //
        // A concrete bean rather than a second port: the repository
        // interface is already the seam a test substitutes at, and a
        // service interface with exactly one implementation is a level of
        // indirection with nothing behind it. Plain Maven gets the same
        // class without the annotation -- the type is the boundary, the
        // annotation only says who constructs it.
        facet(
            boundary::SERVICE.role,
            crate::template!("spring/entity_service_java.java"),
            Package::Service,
            Naming::Suffix("Service"),
            has_service,
            &[Import::Role("record"), Import::Role("repository")],
        ),
        facet(
            boundary::EVENTS.role,
            crate::template!("spring/entity_events_java.java"),
            Package::PortsEvents,
            Naming::Suffix("Events"),
            has_events,
            &[Import::Role("record")],
        ),
        // `matching(query, limit)`, not `search(query)`. There is no
        // unbounded overload on purpose: a search with no limit is a full
        // scan waiting for the table to grow, and the caller who wants
        // everything can say so.
        facet(
            boundary::SEARCH.role,
            crate::template!("spring/entity_search_java.java"),
            Package::PortsSearch,
            Naming::Suffix("Search"),
            has_search,
            &[Import::Role("record")],
        ),
    ],
    &[
        Fragment::Rendered {
            key: "components",
            render: fragment::record_components,
        },
        Fragment::Rendered {
            key: "compact_constructor",
            render: fragment::record_constructor,
        },
        Fragment::Rendered {
            key: "constants",
            render: fragment::enum_constants,
        },
        Fragment::Rendered {
            key: "wire_members",
            render: fragment::enum_wire_members,
        },
        Fragment::Rendered {
            key: "key_type",
            render: fragment::key_type,
        },
        Fragment::WhenBoot {
            key: "component",
            boot: BootCondition::Spring,
            body: "@Component\n",
            imports: &["org.springframework.stereotype.Component"],
        },
    ],
    BootCondition::Any,
    "java-facets",
);

/// The mutable test-data builder `use factory` asks for.
const TESTKIT: Recipe<Entity> = recipe(
    &[JavaFile {
        source_set: SourceSet::Test,
        ejectable: true,
        ..facet(
            boundary::FACTORY.role,
            crate::template!("spring/entity_factory_java.java"),
            Package::Testkit,
            Naming::Suffix("Factory"),
            has_factory,
            &[Import::Keyed(Package::Domain, Key::Record)],
        )
    }],
    &[
        Fragment::Rendered {
            key: "declarations",
            render: fragment::factory_declarations,
        },
        Fragment::Rendered {
            key: "methods",
            render: fragment::factory_methods,
        },
        Fragment::Rendered {
            key: "guards",
            render: fragment::factory_guards,
        },
        Fragment::Rendered {
            key: "arguments",
            render: fragment::factory_arguments,
        },
    ],
    BootCondition::Any,
    "java-test-factory",
);

/// The Spring `Converter` that binds an enum's wire value off a request
/// parameter. Only an enum with wire values needs one, and only a Spring
/// project can register it.
const CONVERTER: Recipe<Entity> = recipe(
    &[facet(
        boundary::ENUM_CONVERTER.role,
        crate::template!("spring/enum_converter_java.java"),
        Package::Web,
        Naming::Suffix("Converter"),
        has_wire_values,
        &[Import::Keyed(Package::Domain, Key::Record)],
    )],
    &[],
    BootCondition::Spring,
    "java-enum-converter",
);
