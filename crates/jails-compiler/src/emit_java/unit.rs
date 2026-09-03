//! The six source-unit kinds that are recipe rows: `class`, `interface`,
//! `service`, `sealed`, `test` and `integration-test`.
//!
//! A [`SourceUnit`] is a [`Node`] like a capability, a component or a stored
//! entity: its id, its Java type, and the two typed values its templates
//! spell. Each kind gets its own recipe rather than one recipe with a row per
//! kind, because a role is the suffix of an artifact id -- every main file is
//! `art_<unit>_main` and every companion test `art_<unit>_test` -- and one
//! recipe carrying four rows named `main` would have two files claiming one
//! role.
//!
//! **Two kinds are not here, and each fails the criterion on its own block.**
//! `strategy` emits a number of files that depends on the model: an ABI, an
//! evaluator, and an implementation and a test *per variant*. A recipe's
//! `files` is a static list and one row renders one file, so the count a
//! strategy needs is not a shape the row table expresses -- the same reason
//! `emit_architecture` stays a function, with the multiplicity the other way
//! round. `controller` needs the captured Boot major: its companion test
//! drives the route through [`crate::emit_mockmvc`], which decides the entry
//! point, the imports and whether the method declares `throws` from the
//! project's Spring version, and a `Fragment::Rendered` is a function of the
//! model and the node and carries no project fact. Both stay in
//! [`crate::emit_unit`] beside the dispatch.

use super::*;
use crate::recipe::{
    BootCondition, Fragment, Import, JavaFile, Naming, Node, Placement, Recipe, Rendered, SourceSet,
};
use jails_model::{SourceUnit, UnitKind};

/// The typed values of a source unit its templates may spell.
#[derive(Clone, Copy)]
pub(crate) enum Key {
    /// `{{variable}}`: the unit's type as a local name.
    Variable,
}

impl Node for SourceUnit {
    type Key = Key;

    fn id(&self) -> &str {
        self.id.as_str()
    }

    fn name(&self) -> &str {
        &self.java_type
    }

    fn describe(&self) -> String {
        format!("unit `{}`", self.java_type)
    }

    fn key(&self, _: &AppModel, key: Key) -> Result<(&'static str, String), Diagnostic> {
        Ok(match key {
            Key::Variable => ("variable", lower_first(&self.java_type)),
        })
    }

    fn file_keys(&self, _: &str, template_class: &str) -> Vec<(&'static str, String)> {
        vec![("class", template_class.to_string())]
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

    /// A unit's companion test is plain JUnit over the type; nothing it emits
    /// starts a Spring context.
    fn splices_test_container(&self, _: SourceSet) -> bool {
        false
    }
}

/// The recipe this kind renders through, or `None` for the two that stay
/// functions.
pub(crate) fn recipe_for(kind: UnitKind) -> Option<&'static Recipe<SourceUnit>> {
    Some(match kind {
        UnitKind::Class => &CLASS,
        UnitKind::Interface => &INTERFACE,
        UnitKind::Service => &SERVICE,
        UnitKind::Sealed => &SEALED,
        UnitKind::Test => &TEST,
        UnitKind::IntegrationTest => &INTEGRATION_TEST,
        UnitKind::Strategy | UnitKind::Controller => return None,
    })
}

/// **A unit's package is projected here, not read off the unit.**
///
/// `SourceUnit::java_package` is written by the linker, which runs before the
/// project's `[layout]` is on the model -- so it always spells the default.
/// Reading it puts a sealed type in `domain` on a project whose records live
/// in `core`: two packages for one layer, in one tree, and nothing to report
/// it.
///
/// So the *layer* travels on the unit and the name is computed with the
/// layout. A unit whose package the reader named carries no layer, and keeps
/// the name they gave it -- they said where it goes, so a rename of that layer
/// is not about them.
pub(crate) fn placed(model: &AppModel, source: &SourceUnit) -> String {
    source.layer.map_or_else(
        || source.java_package.clone(),
        |layer| model.project.package_for(layer),
    )
}

const fn recipe(
    files: &'static [JavaFile<SourceUnit>],
    keys: &'static [Key],
    fragments: &'static [Fragment<SourceUnit>],
) -> Recipe<SourceUnit> {
    Recipe {
        substitutions: &[],
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
        default_package: placed,
        pass: "java-source-units",
        minimum_boot: None,
    }
}

/// The unit's own type, in the package the unit is placed in.
const fn main(
    template: crate::Template,
    ejectable: bool,
    imports: &'static [Import<SourceUnit>],
) -> JavaFile<SourceUnit> {
    JavaFile {
        role: "main",
        template,
        before_boot: None,
        imports,
        only_when: None,
        source_set: SourceSet::Main,
        placement: Placement::Default,
        ejectable,
        class: Naming::Suffix(""),
        template_class: Naming::Suffix(""),
    }
}

/// A companion test beside the type it is about.
///
/// `template_class` is the type under test rather than the file's own class,
/// so the template spells `{{class}}` for what it constructs and writes
/// `class {{class}}Test` for itself -- one spelling of the name, on the row.
const fn companion_test(template: crate::Template) -> JavaFile<SourceUnit> {
    JavaFile {
        role: "test",
        template,
        before_boot: None,
        imports: &[],
        only_when: None,
        source_set: SourceSet::Test,
        placement: Placement::Default,
        ejectable: true,
        class: Naming::Suffix("Test"),
        template_class: Naming::Suffix(""),
    }
}

/// A unit that *is* a test: its own class, in the test source set.
const fn standalone_test(template: crate::Template) -> JavaFile<SourceUnit> {
    JavaFile {
        role: "test",
        template,
        before_boot: None,
        imports: &[],
        only_when: None,
        source_set: SourceSet::Test,
        placement: Placement::Default,
        ejectable: true,
        class: Naming::Suffix(""),
        template_class: Naming::Suffix(""),
    }
}

const CLASS: Recipe<SourceUnit> = recipe(
    &[
        main(crate::template!("spring/unit_class_java.java"), true, &[]),
        companion_test(crate::template!("spring/unit_class_test_java.java")),
    ],
    &[Key::Variable],
    &[],
);

/// An interface has no companion test: there is nothing to construct and
/// nothing to assert, and a test over a type with no implementation would
/// pass over anything.
const INTERFACE: Recipe<SourceUnit> = recipe(
    &[main(
        crate::template!("spring/unit_interface_java.java"),
        false,
        &[],
    )],
    &[],
    &[],
);

const SERVICE: Recipe<SourceUnit> = recipe(
    &[
        main(crate::template!("spring/unit_service_java.java"), true, &[]),
        companion_test(crate::template!("spring/unit_service_test_java.java")),
    ],
    &[],
    &[],
);

/// The sealed hierarchy and the exhaustive switch that proves it is closed.
///
/// Four fragments, and each is an independent walk of the same list rather
/// than four readings of one pass: the permits clause names the variants, the
/// nested records declare them, the switch has one arm each and the test one
/// case each. No arm's answer depends on what another fragment decided.
const SEALED: Recipe<SourceUnit> = recipe(
    &[
        JavaFile {
            ejectable: false,
            ..main(crate::template!("spring/unit_sealed_java.java"), false, &[])
        },
        companion_test(crate::template!("spring/unit_sealed_test_java.java")),
    ],
    &[],
    &[
        Fragment::Rendered {
            key: "permits",
            render: sealed_permits,
        },
        Fragment::Rendered {
            key: "records",
            render: sealed_records,
        },
        Fragment::Rendered {
            key: "arms",
            render: sealed_arms,
        },
        Fragment::Rendered {
            key: "tests",
            render: sealed_tests,
        },
    ],
);

const TEST: Recipe<SourceUnit> = recipe(
    &[standalone_test(crate::template!(
        "spring/unit_test_java.java"
    ))],
    &[],
    &[],
);

const INTEGRATION_TEST: Recipe<SourceUnit> = recipe(
    &[standalone_test(crate::template!(
        "spring/unit_integration_test_java.java"
    ))],
    &[],
    &[],
);

/// The permits clause: every variant, qualified by the interface that nests
/// it.
fn sealed_permits(_: &AppModel, unit: &SourceUnit) -> Result<Rendered, Diagnostic> {
    Ok(Rendered::from(
        unit.variants
            .iter()
            .map(|variant| format!("{}.{variant}", unit.java_type))
            .collect::<Vec<_>>()
            .join(", "),
    ))
}

fn sealed_records(_: &AppModel, unit: &SourceUnit) -> Result<Rendered, Diagnostic> {
    let name = &unit.java_type;
    Ok(Rendered::from(
        unit.variants
            .iter()
            .map(|variant| {
                format!(
                    "    /** TODO: give {variant} the components it carries. */\n    record {variant}() implements {name} {{}}"
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n"),
    ))
}

fn sealed_arms(_: &AppModel, unit: &SourceUnit) -> Result<Rendered, Diagnostic> {
    let name = &unit.java_type;
    Ok(Rendered::from(
        unit.variants
            .iter()
            .map(|variant| {
                format!(
                    "            case {name}.{variant} ignored -> \"{}\";",
                    variant.to_ascii_lowercase()
                )
            })
            .collect::<Vec<_>>()
            .join("\n"),
    ))
}

fn sealed_tests(_: &AppModel, unit: &SourceUnit) -> Result<Rendered, Diagnostic> {
    let name = &unit.java_type;
    Ok(Rendered::from(
        unit.variants
            .iter()
            .map(|variant| {
                format!(
                    "    @Test\n    void describes{variant}() {{\n        assertEquals(\"{}\", describe(new {name}.{variant}()));\n    }}",
                    variant.to_ascii_lowercase()
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n"),
    ))
}

fn lower_first(value: &str) -> String {
    let mut characters = value.chars();
    characters.next().map_or_else(String::new, |first| {
        first.to_ascii_lowercase().to_string() + characters.as_str()
    })
}
