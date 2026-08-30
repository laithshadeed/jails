//! Lower parsed JDL declarations into the closed TOML source boundary.

use super::DocumentDraft;
use crate::Diagnostics;

pub(super) fn render(document: DocumentDraft) -> Result<String, Diagnostics> {
    let DocumentDraft {
        project,
        entities,
        enums,
        units,
        capabilities,
        dependencies,
        settings,
        ejections,
    } = document;
    let missing = |name: &str, fix: &str| {
        super::problem(1, format!("the JDL has no `{name}` declaration"), fix)
    };
    let name = project
        .name
        .ok_or_else(|| missing("application", "add `application MyApp`"))?;
    let id = project.id.expect("an application name assigns an id");
    let package = project
        .package
        .ok_or_else(|| missing("package", "add `package com.example.app`"))?;
    let java = project
        .java
        .ok_or_else(|| missing("java", "add `java 21` or newer"))?;
    let dialect = project
        .dialect
        .ok_or_else(|| missing("dialect", "add `dialect postgresql`"))?;
    let mut output = format!(
        "schema = \"jails.model.v1\"\n\n[project]\nid = {}\nname = {}\nbase_package = {}\njava_release = {java}\ndialect = {}\n",
        quote(&id),
        quote(&name),
        quote(&package),
        quote(&dialect)
    );
    for capability in capabilities {
        output.push_str(&format!(
            "\n[capabilities.{}]\nid = {}\nkind = {}\n",
            quote(&capability.label),
            quote(&capability.id),
            quote(&capability.kind)
        ));
        if let Some(name) = capability.name {
            output.push_str(&format!("name = {}\n", quote(&name)));
        }
        if let Some(package) = capability.package {
            output.push_str(&format!("package = {}\n", quote(&package)));
        }
    }
    for dependency in dependencies {
        output.push_str(&format!(
            "\n[dependencies.{}]\nid = {}\ngroup = {}\nartifact = {}\n",
            quote(&dependency.label),
            quote(&dependency.id),
            quote(&dependency.group),
            quote(&dependency.artifact),
        ));
        if let Some(version) = dependency.version {
            output.push_str(&format!("version = {}\n", quote(&version)));
        }
        output.push_str(&format!("scope = {}\n", quote(&dependency.scope)));
    }
    for setting in settings {
        output.push_str(&format!(
            "\n[settings.{}]\nid = {}\nkey = {}\nvalue = {}\ntarget = {}\n",
            quote(&setting.label),
            quote(&setting.id),
            quote(&setting.key),
            quote(&setting.value),
            quote(&setting.target),
        ));
    }
    for ejection in ejections {
        output.push_str(&format!(
            "\n[ejections.{}]\nid = {}\ntarget = {}\n",
            quote(&ejection.label),
            quote(&ejection.id),
            quote(&ejection.target),
        ));
    }
    for unit in units {
        output.push_str(&format!(
            "\n[units.{}]\nid = {}\nkind = {}\njava_name = {}\n",
            quote(&unit.label),
            quote(&unit.id),
            quote(unit.kind),
            quote(&unit.name),
        ));
        if let Some(package) = unit.package {
            output.push_str(&format!("package = {}\n", quote(&package)));
        }
        if !unit.variants.is_empty() {
            output.push_str(&format!(
                "variants = [{}]\n",
                unit.variants
                    .iter()
                    .map(|variant| quote(variant))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        if let Some(on) = unit.on {
            output.push_str(&format!("on = {}\n", quote(&on)));
        }
        if let Some(yields) = unit.yields {
            output.push_str(&format!("yields = {}\n", quote(&yields)));
        }
        if let Some(method) = unit.method {
            output.push_str(&format!(
                "method = {}\n",
                quote(&format!("{method:?}").to_ascii_lowercase())
            ));
        }
        if let Some(path) = unit.path {
            output.push_str(&format!("path = {}\n", quote(&path)));
        }
        if let Some(consumes) = unit.consumes {
            output.push_str(&format!(
                "consumes = {}\n",
                quote(&format!("{consumes:?}").to_ascii_lowercase())
            ));
        }
    }
    for entity in entities {
        output.push_str(&format!(
            "\n[entities.{}]\nid = {}\njava_name = {}\nfacets = [{}]\n",
            quote(&entity.label),
            quote(&entity.id),
            quote(&entity.name),
            entity
                .facets
                .iter()
                .map(|facet| quote(facet))
                .collect::<Vec<_>>()
                .join(", ")
        ));
        if !entity.active {
            output.push_str("active = false\n");
        }
        if let Some(table) = entity.table {
            output.push_str(&format!("table = {}\n", quote(&table)));
        }
        // **The order the author declared, carried across the TOML hop.**
        //
        // `audit.md` A2.2b: this renderer knows the order -- the draft holds a
        // `Vec` -- and `parse_toml` reads the fields back into a `BTreeMap`,
        // so without this line a pre-v1 entity declaring `zulu, id, alpha`
        // links as `alpha, id, zulu`. A Java record's component order is ABI,
        // so that is not a presentation difference: a caller compiled against
        // the positional constructor keeps compiling against a re-sorted one
        // and silently passes the wrong arguments.
        //
        // The entry hesitated because emitting it "makes `.jails/model.toml`
        // able to state an order it is documented as unable to state". It
        // already is: `source::Entity::field_order` is `Deserialize` and has
        // been since v1 needed it, so this adds no surface to the
        // compatibility input -- it only stops the pre-v1 path throwing away
        // an answer it had.
        if !entity.fields.is_empty() {
            output.push_str(&format!(
                "field_order = [{}]\n",
                entity
                    .fields
                    .iter()
                    .map(|field| quote(&field.label))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        for field in entity.fields {
            output.push_str(&format!(
                "\n[entities.{}.fields.{}]\nid = {}\njava_name = {}\ntype = {}\nrequired = {}\nnon_blank = {}\nprimary_key = {}\nunique = {}\nindexed = {}\n",
                quote(&entity.label),
                quote(&field.label),
                quote(&field.id),
                quote(&field.name),
                quote(&field.type_name),
                field.required,
                field.non_blank,
                field.primary_key,
                field.unique,
                field.indexed
            ));
            if let Some(min) = field.min_length {
                output.push_str(&format!("min_length = {min}\n"));
            }
            if let Some(max) = field.max_length {
                output.push_str(&format!("max_length = {max}\n"));
            }
            if let Some(column) = field.column {
                output.push_str(&format!("column = {}\n", quote(&column)));
            }
        }
        for index in entity.indexes {
            output.push_str(&format!(
                "\n[entities.{}.indexes.{}]\nid = {}\ncolumns = [{}]\n",
                quote(&entity.label),
                quote(&index.label),
                quote(&index.id),
                index
                    .columns
                    .iter()
                    .map(|column| quote(column))
                    .collect::<Vec<_>>()
                    .join(", "),
            ));
            if let Some(name) = index.name {
                output.push_str(&format!("name = {}\n", quote(&name)));
            }
        }
        for operation in entity.operations {
            operation.render_toml(&mut output)?;
        }
    }
    for enumeration in enums {
        output.push_str(&format!(
            "\n[entities.{}]\nid = {}\njava_name = {}\nfacets = [\"enum\"]\nvalues = [{}]\n",
            quote(&enumeration.label),
            quote(&enumeration.id),
            quote(&enumeration.name),
            enumeration
                .values
                .iter()
                .map(|value| quote(value))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    Ok(output)
}

fn quote(value: &str) -> String {
    serde_json::to_string(value).expect("a Rust string always has a JSON representation")
}
