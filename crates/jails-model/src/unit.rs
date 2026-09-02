//! Standalone Java source-unit vocabulary and collision policy.

use crate::UnitId;
use serde::{Deserialize, Serialize};

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
#[cfg_attr(feature = "cli", derive(clap::ValueEnum))]
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

    /// The lowercase spelling: what the CLI parses, what a JDL `route`
    /// member is written with, and what a refusal prints. The uppercase
    /// [`Self::wire_name`] is the same verb on the wire.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Get => "get",
            Self::Post => "post",
            Self::Put => "put",
            Self::Patch => "patch",
            Self::Delete => "delete",
        }
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
#[cfg_attr(feature = "cli", derive(clap::ValueEnum))]
pub enum RequestFormat {
    Json,
    Form,
}

impl RequestFormat {
    /// The canonical spelling: what the CLI parses, what a JDL `consumes`
    /// clause is written with, and what a refusal prints.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Form => "form",
        }
    }

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
