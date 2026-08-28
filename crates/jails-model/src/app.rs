//! Typed application-level compiler intent.

use crate::ProjectId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProjectIntent {
    pub id: ProjectId,
    pub name: String,
    pub base_package: String,
    pub java_release: u16,
    pub dialect: String,
    #[serde(default = "default_platform")]
    pub platform: String,
    #[serde(default = "default_build")]
    pub build: String,
}

fn default_platform() -> String {
    "spring".to_string()
}

fn default_build() -> String {
    "maven".to_string()
}
