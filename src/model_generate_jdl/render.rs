//! How a JDL v1 declaration is *spelled*.
//!
//! Split out of `model_generate_jdl` because it is a different secret from the
//! one that file keeps. Dispatch decides which mutation a `jails g` invocation
//! means and what it must refuse; nothing in it needs to know that an entity
//! member is two spaces in, that a field line puts `@id(..)` before its
//! constraints, or that a quoted list is separated by `, `. Rendering is read
//! against the parser and the formatter, and those are the only things it has
//! to agree with.
//!
//! `edit.rs` is the neighbouring half: where a rendered declaration is spliced
//! into an existing document. This module produces the text; that one places
//! it.

use super::*;

/// A CLI name, as the Java type it names.
///
/// `jails g enum currency GBP EUR` writes `Currency.java`, and every generator
/// that later says `currency:Currency` resolves against it. The model requires
/// a real Java type name and refuses the lower-camel spelling, so the
/// capitalising happens here rather than by loosening the model: `java_name`
/// is a projection the model is right to hold to, and this is the CLI sugar
/// resolving what the reader typed.
pub(crate) fn java_type_name(name: &str) -> String {
    let mut characters = name.chars();
    match characters.next() {
        Some(first) => first.to_uppercase().collect::<String>() + characters.as_str(),
        None => String::new(),
    }
}

pub(crate) fn quoted_list(labels: &[String]) -> String {
    labels
        .iter()
        .map(|label| format!("`{label}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The model label a `name:type` field spec declares.
///
/// The same fold the parser applies: `userId` and `user_id` are one field, so
/// matching a requested label against a typed spec has to agree with it.
pub(crate) fn field_label_of(spec: &str) -> String {
    let name = spec.split(':').next().unwrap_or_default();
    let mut label = String::new();
    for (index, character) in name.chars().enumerate() {
        if character.is_ascii_uppercase() {
            if index > 0 {
                label.push('_');
            }
            label.push(character.to_ascii_lowercase());
        } else if character == '-' {
            label.push('_');
        } else {
            label.push(character);
        }
    }
    label
}

/// The lowerCamel member name a relation is declared under.
///
/// The single owner of the `item_owner` -> `itemOwner` direction, so
/// `g association` and `destroy association` cannot disagree about which
/// member they are naming.
pub(crate) fn relation_member_name(label: &str) -> String {
    let mut out = String::with_capacity(label.len());
    let mut capitalise = false;
    for character in label.chars() {
        if character == '_' {
            capitalise = true;
            continue;
        }
        if capitalise {
            out.extend(character.to_uppercase());
            capitalise = false;
        } else {
            out.push(character);
        }
    }
    out
}

/// One entity declaration's inputs, together because they are decided
/// together: what to call it, what it holds, and which dialect it is written
/// in.
pub(crate) struct EntityDeclaration<'a> {
    pub(crate) java_name: &'a str,
    pub(crate) entity_label: &'a str,
    pub(crate) scaffold: bool,
    pub(crate) fields: &'a [String],
    pub(crate) path: Option<&'a str>,
    pub(crate) uniques: &'a [String],
    /// `--package`, already normalized against the base package.
    pub(crate) package: Option<&'a str>,
}

pub(crate) fn entity_declaration_at(
    model: &jails_model::AppModel,
    declaration: &EntityDeclaration<'_>,
) -> Result<String> {
    let EntityDeclaration {
        java_name,
        entity_label,
        scaffold,
        fields,
        path,
        uniques,
        package: _,
    } = *declaration;
    let mut labels = BTreeSet::new();
    let mut parsed = Vec::new();
    for token in fields {
        let field = parse_field(token)?;
        // **One column, named twice.** `id` and `Id` converge on the same
        // Java component and the same SQL column, so this is not two fields
        // colliding -- it is one field spelled two ways, and the refusal says
        // so in both projections rather than echoing whichever spelling came
        // second.
        if !labels.insert(field.label.clone()) {
            return Err(Failure::Told(format!(
                "`{}` is declared twice: `{}` and the column `{}` are one field, whatever the spelling.\n       fix: keep one declaration",
                field.java_name,
                jails_model::lower_camel_case(&field.label),
                field
                    .mapped_column
                    .clone()
                    .unwrap_or_else(|| field.label.clone()),
            )));
        }
        parsed.push(field);
    }
    // **A component called `version` is the row version.** Every `--if-match`
    // transition depends on it, and an entity declaring `version:long`
    // without the marker would get a plain column and a transition refusing
    // with "entity `note` has 0 version fields" about a field the reader just
    // named. Inferred in the frontend and written into the model as
    // `@version`, so the convention is visible in `.jails/model.jdl` rather
    // than hidden in the compiler -- and an entity that means something else
    // by the word says so by editing the declaration.
    //
    for field in &mut parsed {
        if field.label == "version" && matches!(field.type_name.as_str(), "long" | "int") {
            field.version = true;
        }
    }
    if scaffold {
        // **`scaffold` is four Spring facets, so it needs Spring.** The
        // linker reaches the same conclusion one projection at a time --
        // `projection `dto` on `note` requires platform spring` -- which
        // names symbols the reader never typed and says nothing about which
        // of the two ways out they want.
        if model.project.platform != "spring" {
            return Err(Failure::Told(format!(
                "`scaffold` is a Spring Boot capability -- a DTO, a controller and a service are Spring types -- and this project declares `platform {}`.\n       fix: `jails g record {java_name}` for the record and its repository, or declare Spring in `{MODEL_PATH}`",
                model.project.platform
            )));
        }
        refuse_unstorable_identity(&parsed, java_name)?;
        refuse_unstorable_components(model, &parsed, java_name)?;
    }
    let package = match declaration.package {
        Some(package) => format!(" @package({package})"),
        None => String::new(),
    };
    let mut output = format!("entity {java_name} @id(ent_{entity_label}){package} {{\n");
    if scaffold {
        match path {
            Some(path) => output.push_str(&format!(
                "  use scaffold(path: {})\n\n",
                serde_json::to_string(path).expect("a route path encodes as a JSON string")
            )),
            None => output.push_str("  use scaffold\n\n"),
        }
    }
    for field in &parsed {
        output.push_str(&render_v1_field_line(entity_label, field));
        output.push('\n');
    }
    // **A composite unique is a constraint on the table, not a marker on one
    // component**, so it is its own member. PostgreSQL requires the columns a
    // foreign key names to carry a unique constraint of their own, which is
    // why a tenant-scoped reference needs `(workspaceId, id)` stated even
    // where `id` alone is already the key.
    for columns in uniques {
        let components = columns
            .split(',')
            .map(str::trim)
            .filter(|component| !component.is_empty())
            .map(|component| labels.get(&java_to_label(component)).cloned().ok_or_else(|| {
                Failure::Told(format!(
                    "`{component}` is not a component of `{java_name}`.\n       fix: name components this entity declares"
                ))
            }))
            .collect::<Result<Vec<_>>>()?;
        if components.is_empty() {
            return Err(Failure::Told(
                "a composite unique key needs at least one component.\n       fix: give `--unique` a comma-separated component list"
                    .to_string(),
            ));
        }
        output.push_str(&format!("  unique [{}]\n", components.join(", ")));
    }
    output.push_str("}\n");
    Ok(output)
}

pub(crate) fn enum_declaration(java_name: &str, label: &str, values: &[String]) -> Result<String> {
    let values = values
        .iter()
        .map(|value| {
            jails_spec::spec::constant::ConstantSpec::parse(value)
                .map(|constant| constant.canonical())
        })
        .collect::<Result<Vec<_>>>()?;
    let mut output = format!("enum {java_name} @id(ent_{label}) {{\n");
    for value in values {
        output.push_str("  ");
        if let Some((constant, wire)) = value.split_once('=') {
            output.push_str(constant);
            output.push_str(" = ");
            output.push_str(&serde_json::to_string(wire).map_err(|error| {
                Failure::Told(format!("could not quote enum wire value: {error}"))
            })?);
        } else {
            output.push_str(&value);
        }
        output.push('\n');
    }
    output.push_str("}\n");
    Ok(output)
}

pub(crate) fn render_v1_field_line(entity_label: &str, field: &ParsedField) -> String {
    let optional = if field.required { "" } else { "?" };
    let mut output = format!(
        "  {}: {}{} @id(fld_{}_{})",
        field.java_name, field.type_name, optional, entity_label, field.label
    );
    if let Some(default) = &field.default {
        output.push_str(&format!(" @default({default})"));
    }
    if field.primary_key {
        output.push_str(" @pk");
    }
    if field.version {
        output.push_str(" @version");
    }
    if field.min_length.is_some() || field.max_length.is_some() {
        output.push_str(&format!(
            " @length({}..{})",
            field
                .min_length
                .map(|value| value.to_string())
                .unwrap_or_default(),
            field
                .max_length
                .map(|value| value.to_string())
                .unwrap_or_default(),
        ));
    }
    if field.nonnegative {
        output.push_str(" @nonnegative");
    }
    if field.non_blank {
        output.push_str(" @notBlank");
    }
    if field.positive {
        output.push_str(" @positive");
    }
    if field.indexed {
        output.push_str(" @index");
    }
    if field.scoped {
        output.push_str(" @scope");
    }
    if field.unique {
        output.push_str(" @unique");
    }
    if field.updated {
        output.push_str(" @updated");
    }
    if let Some(column) = &field.mapped_column {
        let column = serde_json::to_string(column).expect("string serialization cannot fail");
        output.push_str(&format!(" @map({column})"));
    }
    output
}

/// `--package` as the model states it: relative to the application's base.
///
/// **Both spellings are accepted and mean one place.** A reader types what
/// their editor shows -- `com.example.demo.billing` -- while the model stores
/// what a capability's `@package` stores, the part below the base. Appending
/// the absolute form to the base would produce
/// `com.example.demo.com.example.demo.billing`, which is a directory nobody
/// asked for and a package nothing imports.
pub(crate) fn normalize_package(base: &str, package: &str) -> Result<String> {
    let package = package.trim();
    if package == base {
        return Ok(String::new());
    }
    if let Some(relative) = package
        .strip_prefix(base)
        .and_then(|rest| rest.strip_prefix('.'))
    {
        return Ok(relative.to_string());
    }
    // A package naming a *different* base is refused rather than nested under
    // this one: `--package com.other.billing` in a `com.example.demo` project
    // is either a typo or a request jails cannot honour, and silently writing
    // `com.example.demo.com.other.billing` answers neither reading.
    if package.contains('.') && package.split('.').count() > 1 && package.starts_with("com.")
        || package.starts_with("org.")
        || package.starts_with("net.")
    {
        return Err(Failure::Told(format!(
            "`--package {package}` names a package outside this application's base `{base}`.\n       fix: pass a package below the base, or the base itself"
        )));
    }
    Ok(package.to_string())
}
