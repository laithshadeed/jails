//! Standalone Java source-unit vocabulary and collision policy.

use crate::UnitId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SourceUnit {
    pub id: UnitId,
    pub label: String,
    pub kind: UnitKind,
    pub java_type: String,
    pub java_package: String,
    /// The JDL v1 §9.7 layer this unit's package was *derived* from, when it
    /// was derived rather than named.
    ///
    /// **The package alone cannot answer a renamed layout.** `java_package` is
    /// computed by the linker, which runs before the project's `[layout]` is
    /// on the model at all -- so it spells the default, and an emitter reading
    /// it would put a sealed type in `domain` on a project whose records live
    /// in `core`: two packages for one layer, with nothing to report it.
    ///
    /// `None` means the reader named the package themselves, and a rename must
    /// not touch it: they said where it goes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer: Option<crate::Package>,
    #[serde(default)]
    pub variants: Vec<String>,
    #[serde(default)]
    pub on: Option<String>,
    #[serde(default)]
    pub yields: Option<String>,
    #[serde(default)]
    pub endpoint: Option<HttpEndpoint>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HttpEndpoint {
    pub method: EndpointMethod,
    pub path: String,
    pub accepts: Option<String>,
    pub returns: Option<String>,
    pub consumes: RequestFormat,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EndpointMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

impl EndpointMethod {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "get" => Ok(Self::Get),
            "post" => Ok(Self::Post),
            "put" => Ok(Self::Put),
            "patch" => Ok(Self::Patch),
            "delete" => Ok(Self::Delete),
            _ => Err(format!(
                "unknown endpoint method `{value}`\n       fix: use get, post, put, patch, or delete"
            )),
        }
    }

    pub(crate) fn takes_body(self) -> bool {
        matches!(self, Self::Post | Self::Put | Self::Patch)
    }

    /// The uppercase spelling every HTTP surface uses: the route grammar's own
    /// `METHOD /path`, Spring's `RequestMethod` constant, and the collision
    /// key. `{:?}` renders `Post`, which is none of those.
    pub fn wire_name(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RequestFormat {
    Json,
    Form,
}

impl RequestFormat {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "json" => Ok(Self::Json),
            "form" => Ok(Self::Form),
            _ => Err(format!(
                "unknown request format `{value}`\n       fix: use json or form"
            )),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum UnitKind {
    Class,
    Interface,
    Service,
    Test,
    IntegrationTest,
    Sealed,
    Strategy,
    Controller,
}

pub(crate) fn insert(
    units: &mut BTreeMap<UnitId, SourceUnit>,
    unit: SourceUnit,
) -> Result<(), String> {
    let id = unit.id.clone();
    if units.contains_key(&id) {
        return Err(format!("source unit id `{id}` already exists"));
    }
    if units.values().any(|existing| {
        existing.java_package == unit.java_package && existing.java_type == unit.java_type
    }) {
        return Err(format!(
            "Java source unit `{}.{}` already exists",
            unit.java_package, unit.java_type
        ));
    }
    units.insert(id, unit);
    Ok(())
}

pub(crate) fn replace(
    units: &mut BTreeMap<UnitId, SourceUnit>,
    unit: SourceUnit,
) -> Result<(), String> {
    let id = unit.id.clone();
    if !units.contains_key(&id) {
        return Err(format!("source unit id `{id}` does not exist"));
    }
    let mut proof = units.clone();
    proof.remove(&id);
    insert(&mut proof, unit)?;
    *units = proof;
    Ok(())
}
