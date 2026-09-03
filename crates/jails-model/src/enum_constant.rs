//! One member of a closed Java and wire-level set.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EnumConstant {
    pub java_name: String,
    pub wire_name: Option<String>,
}

impl EnumConstant {
    pub fn parse(value: &str) -> Result<Self, String> {
        let (java_name, wire_name) = value
            .split_once('=')
            .map_or((value, None), |(name, wire)| (name, Some(wire)));
        let mut characters = java_name.chars();
        let valid_name = characters
            .next()
            .is_some_and(|character| character.is_ascii_uppercase() || character == '_')
            && characters.all(|character| {
                character.is_ascii_uppercase() || character.is_ascii_digit() || character == '_'
            });
        if !valid_name {
            return Err(format!(
                "`{java_name}` is not a valid enum constant; use uppercase ASCII letters, digits and `_`"
            ));
        }
        if wire_name.is_some_and(|wire| {
            wire.is_empty() || wire.contains(['\"', '\\']) || wire.chars().any(char::is_control)
        }) {
            return Err(format!(
                "enum constant `{java_name}` has an empty or unrenderable wire value"
            ));
        }
        Ok(Self {
            java_name: java_name.to_string(),
            wire_name: wire_name.map(str::to_string),
        })
    }

    pub fn wire_value(&self) -> &str {
        self.wire_name.as_deref().unwrap_or(&self.java_name)
    }
}
