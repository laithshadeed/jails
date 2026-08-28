//! Link an enum's ordered Java/wire vocabulary and its exclusive shape.

use super::Linker;
use crate::{EnumConstant, Facet};
use std::collections::BTreeSet;

pub(super) fn link(
    linker: &mut Linker,
    path: &str,
    facets: &BTreeSet<Facet>,
    values: &[String],
    has_fields: bool,
    has_indexes: bool,
) -> Vec<EnumConstant> {
    let mut constants = Vec::new();
    let mut java_names = BTreeSet::new();
    let mut wire_names = BTreeSet::new();
    for (offset, value) in values.iter().enumerate() {
        let value_path = format!("{path}.values[{offset}]");
        match EnumConstant::parse(value) {
            Ok(constant) => {
                if !java_names.insert(constant.java_name.clone()) {
                    linker.problem(
                        "model-enum-name-collision",
                        &value_path,
                        format!(
                            "enum constant `{}` is declared more than once",
                            constant.java_name
                        ),
                        "give every enum constant one Java name",
                    );
                }
                if !wire_names.insert(constant.wire_value().to_string()) {
                    linker.problem(
                        "model-enum-wire-collision",
                        &value_path,
                        format!(
                            "enum wire value `{}` is declared more than once",
                            constant.wire_value()
                        ),
                        "give every enum constant one wire value",
                    );
                }
                constants.push(constant);
            }
            Err(message) => linker.problem(
                "model-enum-constant",
                &value_path,
                message,
                "use `NAME` or `NAME=wire-value`",
            ),
        }
    }

    let is_enum = facets.contains(&Facet::Enum);
    if is_enum && facets.len() != 1 {
        linker.problem(
            "model-enum-facets",
            format!("{path}.facets"),
            "an enum cannot also be a record, port, or adapter facet",
            "keep only `enum` in the facet list",
        );
    }
    if is_enum && (has_fields || has_indexes) {
        linker.problem(
            "model-enum-shape",
            path,
            "an enum declares ordered `values`, not fields or indexes",
            "remove fields and indexes from the enum",
        );
    }
    if is_enum && constants.is_empty() {
        linker.problem(
            "model-enum-empty",
            format!("{path}.values"),
            "an enum needs at least one constant",
            "add a value such as `OPEN`",
        );
    }
    if !is_enum && !constants.is_empty() {
        linker.problem(
            "model-values-without-enum",
            format!("{path}.values"),
            "only an enum facet may declare `values`",
            "add the `enum` facet or remove `values`",
        );
    }
    constants
}
