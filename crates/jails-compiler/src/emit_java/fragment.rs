//! The named fragment renderers: what an entity's structure spells inside
//! a facet's template.
//!
//! Every function here is one [`Fragment::Rendered`] on a row of
//! [`super::entity`]: a list, a switch, a constructor, rendered once from
//! the model and substituted like any other key. The template around it is a
//! real `.java` file, and this is the closed set of things a template cannot
//! say for itself -- a record's components and compact constructor, an
//! enum's constants and the members its wire values need, a primary key's
//! Java type, and the four lists a test-data builder is made of.
//!
//! **Decided once.** The record's component list is the same function the
//! operation `Input` records render through
//! (`input::record_declarations`), and a builder's sample comes from
//! `emit_companion_test::builtin_sample`, the one reader of
//! `BuiltinSemantics::sample`. A second spelling of either in a template's
//! substitutions is the drift these functions exist to remove.
//!
//! [`Fragment::Rendered`]: crate::recipe::Fragment::Rendered

use super::*;
use crate::recipe::Rendered;

/// A record's components, between the parentheses.
///
/// `\n` on both sides of the list, or nothing at all: an empty component
/// list is `()`, not a blank line between parens.
pub(super) fn record_components(_: &AppModel, entity: &Entity) -> Result<Rendered, Diagnostic> {
    let mut imports = BTreeSet::new();
    let components = input::entity_components(entity);
    let declarations = input::record_declarations(&components, &mut imports, None);
    let text = match components.is_empty() {
        true => String::new(),
        false => format!("\n{declarations}\n"),
    };
    Ok(Rendered { text, imports })
}

/// A record's compact constructor, or nothing when no component needs a
/// check. Spells `{{class}}`.
pub(super) fn record_constructor(_: &AppModel, entity: &Entity) -> Result<Rendered, Diagnostic> {
    let mut imports = BTreeSet::new();
    let components = input::entity_components(entity);
    let text = input::record_constructor("{{class}}", &components, &mut imports);
    Ok(Rendered { text, imports })
}

/// An enum's constants, one per line, carrying their wire value when the
/// enum declares any.
pub(super) fn enum_constants(_: &AppModel, entity: &Entity) -> Result<Rendered, Diagnostic> {
    let wired = has_wire_values(entity);
    let text = entity
        .enum_constants
        .iter()
        .map(|constant| match wired {
            true => format!("    {}(\"{}\")", constant.java_name, constant.wire_value()),
            false => format!("    {}", constant.java_name),
        })
        .collect::<Vec<_>>()
        .join(",\n");
    Ok(Rendered::from(text))
}

/// The field, constructor, accessor and factory an enum with wire values
/// carries after its constants, or nothing. Spells `{{class}}`.
pub(super) fn enum_wire_members(_: &AppModel, entity: &Entity) -> Result<Rendered, Diagnostic> {
    if !has_wire_values(entity) {
        return Ok(Rendered::from(String::new()));
    }
    let expected = entity
        .enum_constants
        .iter()
        .map(EnumConstant::wire_value)
        .collect::<Vec<_>>()
        .join(", ");
    Ok(Rendered {
        text: WIRE_MEMBERS.replace("{{expected}}", &expected),
        imports: BTreeSet::from([
            "com.fasterxml.jackson.annotation.JsonCreator".to_string(),
            "com.fasterxml.jackson.annotation.JsonValue".to_string(),
        ]),
    })
}

/// Follows the constants directly, so it opens with the `;` that ends them.
const WIRE_MEMBERS: &str = ";

    private final String wire;

    {{class}}(String wire) {
        this.wire = wire;
    }

    @JsonValue
    public String wire() {
        return this.wire;
    }

    @JsonCreator
    public static {{class}} fromWire(String value) {
        for ({{class}} candidate : values()) {
            if (candidate.wire.equals(value)) {
                return candidate;
            }
        }
        throw new IllegalArgumentException(
                \"no {{class}} with wire value '\" + value + \"'; expected one of {{expected}}\");
    }";

pub(crate) fn has_wire_values(entity: &Entity) -> bool {
    entity
        .enum_constants
        .iter()
        .any(|constant| constant.wire_name.is_some())
}

/// The Java type of the entity's primary key: what a port's `findById` and
/// `deleteById` take.
pub(super) fn key_type(_: &AppModel, entity: &Entity) -> Result<Rendered, Diagnostic> {
    let mut imports = BTreeSet::new();
    let text = java_type(primary_key(entity)?, &mut imports);
    Ok(Rendered { text, imports })
}

/// A builder's state: one field per component, started at its sample.
pub(super) fn factory_declarations(_: &AppModel, entity: &Entity) -> Result<Rendered, Diagnostic> {
    let mut imports = BTreeSet::new();
    let text = entity
        .fields
        .iter()
        .map(|field| {
            let ty = builder_type(field, &mut imports);
            let sample = builder_sample(field, &mut imports).unwrap_or_else(|| "null".to_string());
            format!("    private {ty} {} = {sample};", field.names.java_member)
        })
        .collect::<Vec<_>>()
        .join("\n");
    Ok(Rendered { text, imports })
}

/// A builder's fluent overrides, one per component. Spells `{{class}}`.
pub(super) fn factory_methods(_: &AppModel, entity: &Entity) -> Result<Rendered, Diagnostic> {
    let mut imports = BTreeSet::new();
    let text = entity
        .fields
        .iter()
        .map(|field| {
            let ty = builder_type(field, &mut imports);
            let name = &field.names.java_member;
            format!(
                "    public {{{{class}}}} with{}({ty} value) {{\n        this.{name} = value;\n        return this;\n    }}",
                upper_first(name)
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    Ok(Rendered { text, imports })
}

/// The guards `build()` runs first: one per required component jails cannot
/// sample, so the reader who has to supply it is told which. Spells
/// `{{class}}`.
pub(super) fn factory_guards(_: &AppModel, entity: &Entity) -> Result<Rendered, Diagnostic> {
    let mut imports = BTreeSet::new();
    let guards = entity
        .fields
        .iter()
        .filter(|field| field.required && builder_sample(field, &mut imports).is_none())
        .map(|field| {
            let name = &field.names.java_member;
            format!(
                "        if ({name} == null) {{\n            throw new IllegalStateException(\"{{{{class}}}} needs {name}\");\n        }}"
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let text = match guards.is_empty() {
        true => String::new(),
        false => format!("{guards}\n"),
    };
    Ok(Rendered { text, imports })
}

/// The arguments `build()` hands the record's constructor, in component
/// order.
pub(super) fn factory_arguments(_: &AppModel, entity: &Entity) -> Result<Rendered, Diagnostic> {
    let text = entity
        .fields
        .iter()
        .map(|field| format!("                {}", field.names.java_member))
        .collect::<Vec<_>>()
        .join(",\n");
    Ok(Rendered::from(text))
}

fn builder_type(field: &Field, imports: &mut BTreeSet<String>) -> String {
    let java = java_type(field, imports);
    if field.required {
        java
    } else {
        imports.insert("java.util.Optional".to_string());
        format!("Optional<{java}>")
    }
}

/// The default a builder starts a component at, or `None` when the reader has
/// to supply one.
///
/// A project-owned type declines deliberately: the builder is *mutable*, so a
/// component jails cannot spell gets a `withX` override and a guard rather
/// than a fabricated value. The builtin half goes through the one sampler, so
/// a type is sampled the same way here as in every companion test.
fn builder_sample(field: &Field, imports: &mut BTreeSet<String>) -> Option<String> {
    if !field.required {
        imports.insert("java.util.Optional".to_string());
        return Some("Optional.empty()".to_string());
    }
    match &field.ty {
        TypeRef::Builtin(builtin) => Some(crate::emit_companion_test::builtin_sample(
            *builtin, imports,
        )),
        TypeRef::External(_) => None,
    }
}

fn upper_first(value: &str) -> String {
    let mut characters = value.chars();
    characters.next().map_or_else(String::new, |first| {
        first.to_ascii_uppercase().to_string() + characters.as_str()
    })
}
