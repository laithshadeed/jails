//! Semantic checks for field constraints.

use super::Linker;
use crate::model::{BuiltinType, LengthRange, TypeRef};

pub(super) fn constraints(
    non_blank: bool,
    min: Option<u32>,
    max: Option<u32>,
    required: bool,
    ty: Option<&TypeRef>,
    path: &str,
    linker: &mut Linker,
) -> Option<LengthRange> {
    if non_blank && !matches!(ty, Some(TypeRef::Builtin(BuiltinType::String))) {
        linker.problem(
            "model-non-blank-type",
            format!("{path}.non_blank"),
            "`non_blank` is valid only for builtin `string` fields",
            "remove `non_blank` or change the field type to `string`",
        );
    }
    length_range(min, max, required, ty, path, linker)
}

fn length_range(
    min: Option<u32>,
    max: Option<u32>,
    required: bool,
    ty: Option<&TypeRef>,
    path: &str,
    linker: &mut Linker,
) -> Option<LengthRange> {
    if min.is_none() && max.is_none() {
        return None;
    }
    if !matches!(ty, Some(TypeRef::Builtin(BuiltinType::String))) {
        linker.problem(
            "model-length-type",
            format!("{path}.min_length"),
            "length bounds are valid only for builtin `string` fields",
            "remove the bounds or change the field type to `string`",
        );
    }
    if matches!((min, max), (Some(min), Some(max)) if min > max) {
        linker.problem(
            "model-length-order",
            format!("{path}.min_length"),
            "the minimum length is greater than the maximum",
            "choose bounds where `min_length <= max_length`",
        );
    }
    if !required {
        linker.problem(
            "model-length-optional",
            format!("{path}.min_length"),
            "length-bounded fields must currently be required",
            "remove `?` or remove the length bounds",
        );
    }
    Some(LengthRange { min, max })
}
