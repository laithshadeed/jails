use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};

/// The representation shared by every semantic identity.
pub trait StableId {
    fn as_str(&self) -> &str;
}

fn validate(kind: &str, value: String) -> Result<String, String> {
    let mut characters = value.chars();
    let starts_well = characters
        .next()
        .is_some_and(|character| character.is_ascii_lowercase());
    let rest_is_valid = characters.all(|character| {
        character.is_ascii_lowercase()
            || character.is_ascii_digit()
            || matches!(character, '_' | '-')
    });
    if value.len() < 3 || value.len() > 80 || !starts_well || !rest_is_valid {
        return Err(format!(
            "{kind} `{value}` is not a stable id; use 3-80 lowercase ASCII letters, digits, `_` or `-`, starting with a letter"
        ));
    }
    Ok(value)
}

macro_rules! stable_id {
    ($name:ident, $kind:literal) => {
        #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn parse(value: impl Into<String>) -> Result<Self, String> {
                validate($kind, value.into()).map(Self)
            }
        }

        impl StableId for $name {
            fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl Display for $name {
            fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
                formatter.write_str(&self.0)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                Self::parse(value).map_err(serde::de::Error::custom)
            }
        }
    };
}

stable_id!(ProjectId, "project id");
stable_id!(EntityId, "entity id");
stable_id!(FieldId, "field id");
stable_id!(IndexId, "index id");
stable_id!(ConstraintId, "constraint id");
stable_id!(ProjectionId, "projection id");
stable_id!(RelationId, "relation id");
stable_id!(OperationId, "operation id");
stable_id!(ComponentId, "component id");
stable_id!(ComponentVariantId, "component variant id");
stable_id!(UnitId, "source unit id");
stable_id!(CapabilityId, "capability id");
stable_id!(DependencyId, "dependency id");
stable_id!(SettingId, "setting id");
stable_id!(EjectionId, "ejection id");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_ids_are_explicit_opaque_values() {
        assert_eq!(EntityId::parse("ent_order").unwrap().as_str(), "ent_order");
        assert!(EntityId::parse("Order").is_err());
        assert!(EntityId::parse("x").is_err());
        assert!(EntityId::parse("ent/order").is_err());
    }
}
