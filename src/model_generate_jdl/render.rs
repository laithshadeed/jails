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
/// `jails g enum currency GBP EUR` writes `Currency.java` on the legacy path
/// and every generator that later says `currency:Currency` resolves against
/// it -- which is the whole of
/// `generators_compose_through_user_owned_field_types`. The canonical model
/// requires a real Java type name and refused the lower-camel spelling
/// outright, so the same command produced a project on one engine and a
/// diagnostic on the other.
///
/// Capitalising here rather than loosening the model: `java_name` is a
/// projection the model is right to hold to, and this is the CLI sugar
/// resolving what the reader typed, which is where the legacy path does it
/// too.
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
/// member they are naming -- which they did, and the destroy half reported the
/// declaration as missing from the entity it was sitting in.
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

pub(crate) fn entity_declaration_at(
    model: &jails_model::AppModel,
    declaration: &EntityDeclaration<'_>,
) -> Result<String> {
    let EntityDeclaration {
        java_name,
        entity_label,
        scaffold,
        fields,
        v1,
        path,
        uniques,
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
    // **A component called `version` is the row version.** The engine this
    // replaces inferred it, every `--if-match` transition depends on it, and
    // an entity that declared `version:long` without the marker got a plain
    // column: the transition then refused with "entity `note` has 0 version
    // fields" about a field the reader had just declared and named. Inferred
    // in the frontend and written into the model as `@version`, so the
    // convention is visible in `.jails/model.jdl` rather than hidden in the
    // compiler -- and an entity that means something else by the word says so
    // by editing the declaration.
    //
    // v1 only, because `@version` is a v1 marker; the draft dialect cannot
    // express it and inferring one it cannot write would be a lie.
    if v1 {
        for field in &mut parsed {
            if field.label == "version" && matches!(field.type_name.as_str(), "long" | "int") {
                field.version = true;
            }
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
    let mut output = format!("entity {java_name} @id(ent_{entity_label}) {{\n");
    if scaffold {
        if v1 {
            match path {
                Some(path) => output.push_str(&format!(
                    "  use scaffold(path: {})\n\n",
                    serde_json::to_string(path).expect("a route path encodes as a JSON string")
                )),
                None => output.push_str("  use scaffold\n\n"),
            }
        } else {
            if path.is_some() {
                return Err(Failure::Told(
                    "pinning a resource route needs a `jdl 1` model.\n       fix: run `jails model upgrade` and repeat the command"
                        .to_string(),
                ));
            }
            output = output.replacen(" {", " @scaffold {", 1);
        }
    }
    for field in &parsed {
        let line = if v1 {
            render_v1_field_line(entity_label, field)
        } else {
            render_field_line(entity_label, field)?
        };
        output.push_str(&line);
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
        if !v1 {
            return Err(Failure::Told(
                "a composite unique key needs a `jdl 1` model.\n       fix: run `jails model upgrade` and repeat the command"
                    .to_string(),
            ));
        }
        output.push_str(&format!("  unique [{}]\n", components.join(", ")));
    }
    output.push_str("}\n");
    Ok(output)
}

pub(crate) fn enum_declaration(
    java_name: &str,
    label: &str,
    values: &[String],
    v1: bool,
) -> Result<String> {
    let values = values
        .iter()
        .map(|value| {
            jails_protocol::declaration::ConstantSpec::parse(value)
                .map(|constant| constant.canonical())
        })
        .collect::<Result<Vec<_>>>()?;
    let mut output = format!("enum {java_name} @id(ent_{label}) {{\n");
    for value in values {
        output.push_str("  ");
        if v1 {
            if let Some((constant, wire)) = value.split_once('=') {
                output.push_str(constant);
                output.push_str(" = ");
                output.push_str(&serde_json::to_string(wire).map_err(|error| {
                    Failure::Told(format!("could not quote enum wire value: {error}"))
                })?);
            } else {
                output.push_str(&value);
            }
        } else {
            output.push_str(&value);
        }
        output.push('\n');
    }
    output.push_str("}\n");
    Ok(output)
}

pub(crate) fn render_field_line(entity_label: &str, field: &ParsedField) -> Result<String> {
    field.require_v1_for_rich_semantics()?;
    let suffix = if !field.required {
        "?"
    } else if field.non_blank {
        "!"
    } else {
        ""
    };
    let range = if field.min_length.is_some() || field.max_length.is_some() {
        format!(
            "({}..{})",
            field
                .min_length
                .map(|value| value.to_string())
                .unwrap_or_default(),
            field
                .max_length
                .map(|value| value.to_string())
                .unwrap_or_default(),
        )
    } else {
        String::new()
    };
    let mut output = format!(
        "  {}: {}{}{} @id(fld_{}_{})",
        field.java_name, field.type_name, suffix, range, entity_label, field.label
    );
    if field.primary_key {
        output.push_str(" @pk");
    }
    if field.unique {
        output.push_str(" @unique");
    }
    if field.indexed {
        output.push_str(" @index");
    }
    if let Some(column) = &field.mapped_column {
        output.push_str(&format!(" @column({column})"));
    }
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
