//! The test that ships beside every generated domain type.
//!
//! **A generated type without a generated test silently drops coverage**:
//! emitting a guess would produce a test that does not compile, and emitting
//! nothing would leave the suite green over a type nobody asserted anything
//! about.
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

use crate::emit_java::JavaUnit;
use crate::{Diagnostic, emit_java};
use jails_contracts::{FileKind, FileMode, Provenance, RenderedFile};
use jails_model::{AppModel, BuiltinType, Entity, Field, Package, StableId, TypeRef, boundary};
use std::collections::BTreeSet;

pub(crate) const JAVA_TEST_ROOT: &str = jails_contracts::SourceRoot::TestJava.path();

/// The companion test for one entity, or `None` when the facet has none.
pub(crate) fn lower(
    model: &AppModel,
    entity: &Entity,
    facet: jails_model::Facet,
) -> Result<Option<emit_java::Unit>, Diagnostic> {
    let own = match facet {
        jails_model::Facet::Enum => boundary::ENUM,
        jails_model::Facet::Record => boundary::RECORD,
        _ => return Ok(None),
    };
    // **A type jails did not write has nothing for this test to prove.** The
    // null rejection it pins is the compact constructor jails renders, and a
    // record `jails adopt resource` registered may reject nothing; a test
    // failing over a file jails will not touch is not coverage of anything.
    if model.is_adopted(&own.owned_by(entity.id.as_str())) {
        return Ok(None);
    }
    let mut imports = BTreeSet::new();
    let body = if own == boundary::ENUM {
        enum_body(entity, &mut imports)
    } else {
        record_body(model, entity, &mut imports)
    };
    let package = crate::emit_java::entity_package(model, entity, Package::Domain);
    let type_name = format!("{}Test", entity.names.java_type);
    let artifact_id = boundary::TEST.owned_by(entity.id.as_str());
    let rendered = JavaUnit::new(&package, &imports, &body).render(&artifact_id);
    let package_path = package.replace('.', "/");
    let path =
        crate::refuse::project_path(format!("{JAVA_TEST_ROOT}/{package_path}/{type_name}.java"))?;
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
///
/// **A wire name adds two more**, and they are the ones that matter: an enum
/// declared `OPEN=open` renders `@JsonValue`, `fromWire` and a Spring
/// `Converter`, so the name a caller sends is *not* the constant -- and a test
/// that only asks `valueOf` proves the half nothing crosses the wire with.
/// The round trip is asserted over every constant rather than the first,
/// because a duplicated wire value resolves to whichever `fromWire` reaches
/// first and reads as correct from either end.
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
    let wire = if constants
        .iter()
        .any(|constant| constant.wire_name.is_some())
    {
        let round_trip = constants
            .iter()
            .map(|constant| {
                let value = constant
                    .wire_name
                    .as_deref()
                    .unwrap_or(&constant.java_name);
                format!(
                    "        assertEquals(\"{value}\", {name}.{}.wire());\n        assertEquals({name}.{}, {name}.fromWire(\"{value}\"));",
                    constant.java_name, constant.java_name
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        format!(
            "\n    @Test\n    void roundTripsEveryWireValue() {{\n{round_trip}\n    }}\n\n\
             \x20   @Test\n    void rejectsAnUnknownWireValue() {{\n\
             \x20       assertThrows(IllegalArgumentException.class, () -> {name}.fromWire(\"nope\"));\n\
             \x20   }}\n"
        )
    } else {
        String::new()
    };
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
         {wire}}}"
    )
}

/// The first component whose compact constructor rejects null, if the record
/// has one.
///
/// **The predicate is the emitter's own**, `!primitive(ty, required)`, rather
/// than a guess that agrees with it for one type. "A required `string`" is a
/// subset: `record Transaction(UUID id, long amount)` gets
/// `Objects.requireNonNull(id, "id")` like every other non-primitive
/// component, and a narrower predicate ships the companion test `@Disabled`
/// -- "state what Transaction guarantees, then assert it" -- over a record
/// that has just been told what it guarantees.
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
pub(crate) fn sample(
    model: &AppModel,
    field: &Field,
    imports: &mut BTreeSet<String>,
) -> Option<String> {
    sample_with(model, field, imports, &mut BTreeSet::new())
}

/// One builtin's sample expression, carrying the type it names.
///
/// **The one place `BuiltinSemantics::sample` is read.** Every emitter that
/// needs a value for a builtin -- the companion tests, the test-data factory,
/// an outbox sink's contract test, a durable job's payload -- goes through
/// here, so a type is sampled one way and the import that makes the expression
/// compile cannot be forgotten at one of them: left out, the file compiles
/// everywhere the sample happens to be a literal and fails on the first
/// `uuid`, `instant` or `date`.
pub(crate) fn builtin_sample(builtin: BuiltinType, imports: &mut BTreeSet<String>) -> String {
    let semantics = builtin.semantics();
    if let Some(import) = semantics.java_import {
        imports.insert(import.to_string());
    }
    semantics.sample.to_string()
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
        TypeRef::Builtin(builtin) => Some(builtin_sample(*builtin, imports)),
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

/// The same sample, carrying the component's own name where the type allows.
///
/// **A body whose every string is `"sample"` says nothing about which field is
/// which.** The reader's first act with a `.http` collection is to edit the
/// values, and `"sample-subject"` is the one that tells them where they are.
/// Only strings: a number or a date has no room for a name, and putting one
/// there would produce a body that does not parse.
pub(crate) fn named_json_sample(model: &AppModel, ty: &TypeRef, name: &str) -> Option<String> {
    let sample = json_sample(model, ty)?;
    match sample.as_str() {
        "\"sample\"" => Some(format!(
            "\"sample-{}\"",
            jails_model::plural_snake_case(name).trim_end_matches('s')
        )),
        _ => Some(sample),
    }
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
                //
                // **And by its wire name where it has one.** `g enum Stage
                // OPEN=open` renders `@JsonValue` and a `StageConverter`, and
                // both of them reject `OPEN` -- so a sample taken from the
                // Java constant is a request the generated code refuses, on
                // the one wire the proof exists to drive.
                entity.enum_constants.first().map(|constant| {
                    format!(
                        "\"{}\"",
                        constant.wire_name.as_deref().unwrap_or(&constant.java_name)
                    )
                })
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
