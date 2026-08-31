//! The test that ships beside every generated domain type.
//!
//! **A generated type without a generated test silently drops coverage**, and
//! `CLAUDE.md` says so where it explains the legacy behaviour: emitting a
//! guess would produce a test that does not compile, and emitting nothing
//! would leave the suite green over a type nobody asserted anything about.
//! The canonical emitter did the second one -- `g record`, `g value` and
//! `g enum` wrote a class and no test at all -- and no gate saw it, because a
//! differential suite compares the files it names and an artifact only one
//! side writes is not a difference.
//!
//! Three shapes, and which one applies is a fact about the record:
//!
//! - a component that can be null-checked gives the test something real to
//!   pin, so it asserts the rejection and names the component;
//! - a record with no such component has nothing to assert yet, so the test is
//!   `@Disabled` and says what to write instead -- asserting that an accessor
//!   returns what was passed in only tests that javac generated the accessor;
//! - a component whose type jails cannot build a sample for disables the
//!   *class*, naming the component, because the constructor call would not
//!   compile.

use crate::{CompileError, emit_java};
use jails_contracts::{FileKind, FileMode, ProjectPath, Provenance, RenderedFile};
use jails_model::{AppModel, Entity, Field, Package, StableId, TypeRef};
use std::collections::BTreeSet;

pub(crate) const JAVA_TEST_ROOT: &str = ".jails/generated/test/java";

/// The companion test for one entity, or `None` when the facet has none.
pub(crate) fn lower(
    model: &AppModel,
    entity: &Entity,
    facet: jails_model::Facet,
) -> Result<Option<emit_java::Unit>, CompileError> {
    let mut imports = BTreeSet::new();
    let body = match facet {
        jails_model::Facet::Enum => enum_body(entity, &mut imports),
        jails_model::Facet::Record => record_body(model, entity, &mut imports),
        _ => return Ok(None),
    };
    let package = model.project.package_for(Package::Domain);
    let type_name = format!("{}Test", entity.names.java_type);
    let artifact_id = format!("art_{}_test", entity.id.as_str());
    let rendered = emit_java::render(&package, &imports, &body, &artifact_id);
    let package_path = package.replace('.', "/");
    let path = ProjectPath::parse(format!("{JAVA_TEST_ROOT}/{package_path}/{type_name}.java"))
        .map_err(CompileError::new)?;
    Ok(Some(emit_java::Unit {
        path,
        file: RenderedFile {
            kind: FileKind::JavaTest,
            mode: FileMode::Regular,
            bytes: rendered.into_bytes(),
            provenance: Provenance {
                artifact_id,
                ejection_id: None,
                ejectable: true,
                semantic_ids: BTreeSet::from([entity.id.as_str().to_string()]),
                compiler_pass: "java-companion-test".to_string(),
            },
        },
    }))
}

/// What an enum can be asked without knowing anything about the domain.
///
/// `valueOf` throwing rather than returning null is the failure mode worth
/// pinning, and the count catches a constant added twice.
fn enum_body(entity: &Entity, imports: &mut BTreeSet<String>) -> String {
    imports.insert("org.junit.jupiter.api.Test".to_string());
    imports.insert("static org.junit.jupiter.api.Assertions.assertEquals".to_string());
    imports.insert("static org.junit.jupiter.api.Assertions.assertThrows".to_string());
    let name = &entity.names.java_type;
    let constants = &entity.enum_constants;
    let first = constants
        .first()
        .map(|constant| constant.java_name.clone())
        .unwrap_or_else(|| "VALUE".to_string());
    let count = constants.len();
    format!(
        "class {name}Test {{\n\n\
         \x20   @Test\n\
         \x20   void parsesItsOwnNames() {{\n\
         \x20       assertEquals({name}.{first}, {name}.valueOf(\"{first}\"));\n\
         \x20   }}\n\n\
         \x20   /** The failure mode worth pinning: valueOf throws, it does not return null. */\n\
         \x20   @Test\n\
         \x20   void rejectsAnUnknownName() {{\n\
         \x20       assertThrows(IllegalArgumentException.class, () -> {name}.valueOf(\"NOPE\"));\n\
         \x20   }}\n\n\
         \x20   @Test\n\
         \x20   void declaresEveryConstantExactlyOnce() {{\n\
         \x20       assertEquals({count}, {name}.values().length);\n\
         \x20   }}\n\
         }}"
    )
}

/// The component a null is worth throwing at, if the record has one.
/// The first component whose compact constructor rejects null.
///
/// **The predicate is the emitter's own**, `!primitive(ty, required)`, rather
/// than a guess that happened to agree with it for one type. It was "a
/// required `string`", which is a subset: `record Transaction(UUID id, long
/// amount)` gets `Objects.requireNonNull(id, "id")` like every other
/// non-primitive component, and the companion test shipped `@Disabled` --
/// "state what Transaction guarantees, then assert it" -- over a project that
/// had just been told what it guarantees. Three of them in one proof
/// application, which is how it surfaced: the toolbox's own bar is that every
/// Surefire test runs, and three did not.
fn null_checked(entity: &Entity) -> Option<&Field> {
    entity
        .fields
        .iter()
        .find(|field| field.required && !crate::emit_java::primitive(&field.ty, field.required))
}

fn record_body(model: &AppModel, entity: &Entity, imports: &mut BTreeSet<String>) -> String {
    imports.insert("org.junit.jupiter.api.Test".to_string());
    let name = &entity.names.java_type;
    // A component jails cannot build disables the class rather than the
    // method: every construction below would fail to compile, so there is no
    // half of this test that still runs.
    let unbuildable = entity
        .fields
        .iter()
        .find(|field| sample(model, field, &mut BTreeSet::new()).is_none());
    let class_disabled = match unbuildable {
        Some(field) => {
            imports.insert("org.junit.jupiter.api.Disabled".to_string());
            format!(
                "@Disabled(\"todo: supply a sample for {} -- jails cannot know how to build one\")\n",
                field.names.java_member
            )
        }
        None => String::new(),
    };
    match null_checked(entity) {
        Some(field) => {
            imports.insert("static org.junit.jupiter.api.Assertions.assertThrows".to_string());
            imports.insert("static org.junit.jupiter.api.Assertions.assertTrue".to_string());
            let arguments = entity
                .fields
                .iter()
                .map(
                    |other| match other.names.java_member == field.names.java_member {
                        true => "null".to_string(),
                        false => {
                            sample(model, other, imports).unwrap_or_else(|| "null".to_string())
                        }
                    },
                )
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "{class_disabled}class {name}Test {{\n\n\
                 \x20   @Test\n\
                 \x20   void rejectsANullComponent() {{\n\
                 \x20       NullPointerException thrown =\n\
                 \x20               assertThrows(NullPointerException.class, () -> new {name}({arguments}));\n\
                 \x20       assertTrue(thrown.getMessage().contains(\"{}\"));\n\
                 \x20   }}\n\
                 }}",
                field.names.java_member
            )
        }
        None => {
            imports.insert("org.junit.jupiter.api.Disabled".to_string());
            let arguments = entity
                .fields
                .iter()
                .map(|field| sample(model, field, imports).unwrap_or_else(|| "null".to_string()))
                .collect::<Vec<_>>()
                .join(", ");
            let variable = lower_first(name);
            format!(
                "{class_disabled}class {name}Test {{\n\n\
                 \x20   @Test\n\
                 \x20   @Disabled(\"todo: state what {name} guarantees, then assert it\")\n\
                 \x20   void todo() {{\n\
                 \x20       {name} {variable} = new {name}({arguments});\n\n\
                 \x20       // {name} has no validation to pin, so assert on what it is\n\
                 \x20       // *for*. Asserting that an accessor returns what was passed in\n\
                 \x20       // only tests that javac generated the accessor.\n\
                 \x20   }}\n\
                 }}"
            )
        }
    }
}

/// A model-declared type, sampled from what the model already knows.
///
/// An enum is one of its own constants -- **by name, not `values()[0]`**,
/// which starts standing for a different value the moment somebody reorders
/// the enum with nothing in the diff to say so. A record is a constructor call
/// over its own components, which is why this recurses.
///
/// `seen` stops a record that reaches itself, directly or through another,
/// from recursing forever: such a type has no finite sample, so it is treated
/// as unsampleable and the class says so.
fn declared_sample(
    model: &AppModel,
    java_type: &str,
    imports: &mut BTreeSet<String>,
    seen: &mut BTreeSet<String>,
) -> Option<String> {
    // A sealed component is a unit rather than an entity, and its zero-argument
    // variant is a complete sample: `Outcome.Accepted()` needs nothing else.
    if let Some(component) = model.components.values().find(|component| {
        component.kind == jails_model::ComponentKind::Sealed && component.name == java_type
    }) && let Some(variant) = component
        .variants
        .iter()
        .find(|variant| variant.parameters.is_empty())
    {
        return Some(format!("new {java_type}.{}()", variant.name));
    }
    let entity = model
        .entities
        .values()
        .find(|entity| entity.active && entity.names.java_type == java_type)?;
    if !seen.insert(java_type.to_string()) {
        return None;
    }
    let rendered = if entity.facets.contains(&jails_model::Facet::Enum) {
        entity
            .enum_constants
            .first()
            .map(|constant| format!("{java_type}.{}", constant.java_name))
    } else if entity.facets.contains(&jails_model::Facet::Record) {
        entity
            .fields
            .iter()
            .map(|field| sample_with(model, field, imports, seen))
            .collect::<Option<Vec<_>>>()
            .map(|arguments| format!("new {java_type}({})", arguments.join(", ")))
    } else {
        None
    };
    seen.remove(java_type);
    rendered
}

/// The same rule the factory uses, so one type is sampled one way.
///
/// The sample carries the builtin's own import with it, which the factory gets
/// for free by also declaring the field's type: `uuid`'s sample names `UUID`,
/// and a test that only constructs a record never mentions the type anywhere
/// else. Left out, the file compiles everywhere the sample happens to be a
/// literal and fails on the first `uuid`, `instant` or `date`.
fn sample(model: &AppModel, field: &Field, imports: &mut BTreeSet<String>) -> Option<String> {
    sample_with(model, field, imports, &mut BTreeSet::new())
}

fn sample_with(
    model: &AppModel,
    field: &Field,
    imports: &mut BTreeSet<String>,
    seen: &mut BTreeSet<String>,
) -> Option<String> {
    if !field.required {
        imports.insert("java.util.Optional".to_string());
        return Some("Optional.empty()".to_string());
    }
    match &field.ty {
        TypeRef::Builtin(builtin) => {
            let semantics = builtin.semantics();
            if let Some(import) = semantics.java_import {
                imports.insert(import.to_string());
            }
            Some(semantics.sample.to_string())
        }
        // A type the *model* declares is one jails can build, which is the
        // whole of "generators compose through user-owned field types": the
        // enum and the record were generated two commands ago, so refusing to
        // fabricate one would be the tool forgetting what it just wrote.
        TypeRef::External(external) => declared_sample(model, external, imports, seen),
    }
}

fn lower_first(value: &str) -> String {
    let mut characters = value.chars();
    characters.next().map_or_else(String::new, |first| {
        first.to_ascii_lowercase().to_string() + characters.as_str()
    })
}

/// A sample of one model-declared Java type, for a caller that has the name
/// rather than the field.
///
/// The HTTP proof needs exactly this: a controller's port answers with the
/// entity the operation targets, and the test has to build one to stub the
/// port with.
pub(crate) fn declared_sample_of(
    model: &AppModel,
    java_type: &str,
    imports: &mut BTreeSet<String>,
) -> Option<String> {
    declared_sample(model, java_type, imports, &mut BTreeSet::new())
}

/// The same value [`sample`] builds, spelled as JSON.
///
/// **Not derivable from the Java expression**, which is why `BuiltinSemantics`
/// carries both: `UUID.fromString("…")` is a bare string on the wire and `1L`
/// is a number with no suffix. A generated request body that rendered the Java
/// spelling would document a payload the record it came from refuses.
/// Takes no import set, deliberately: JSON names no Java type. A `uuid` is a
/// string on the wire and an enum constant is its own name, so a caller that
/// only builds a request body needs nothing imported for it.
pub(crate) fn json_sample(model: &AppModel, ty: &TypeRef) -> Option<String> {
    json_sample_with(model, ty, &mut BTreeSet::new())
}

fn json_sample_with(model: &AppModel, ty: &TypeRef, seen: &mut BTreeSet<String>) -> Option<String> {
    match ty {
        TypeRef::Builtin(builtin) => Some(builtin.semantics().json.to_string()),
        TypeRef::External(external) => {
            let entity = model
                .entities
                .values()
                .find(|entity| entity.active && entity.names.java_type == *external)?;
            if !seen.insert(external.clone()) {
                return None;
            }
            let rendered = if entity.facets.contains(&jails_model::Facet::Enum) {
                // By name, for the reason `declared_sample` gives: the first
                // constant stands for a different value the moment somebody
                // reorders the enum, with nothing in the diff to say so.
                entity
                    .enum_constants
                    .first()
                    .map(|constant| format!("\"{}\"", constant.java_name))
            } else if entity.facets.contains(&jails_model::Facet::Record) {
                entity
                    .fields
                    .iter()
                    .map(|field| {
                        if !field.required {
                            return Some(format!("\"{}\": null", field.names.java_member));
                        }
                        let value = json_sample_with(model, &field.ty, seen)?;
                        Some(format!("\"{}\": {value}", field.names.java_member))
                    })
                    .collect::<Option<Vec<_>>>()
                    .map(|entries| format!("{{{}}}", entries.join(", ")))
            } else {
                None
            };
            seen.remove(external);
            rendered
        }
    }
}
