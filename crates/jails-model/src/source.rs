use crate::model::{DependencyScope, Facet, SettingTarget};
use crate::{EndpointMethod, RequestFormat, UnitKind};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Document {
    pub(crate) schema: String,
    pub(crate) project: Project,
    #[serde(default)]
    pub(crate) capabilities: BTreeMap<String, Capability>,
    #[serde(default)]
    pub(crate) dependencies: BTreeMap<String, Dependency>,
    #[serde(default)]
    pub(crate) settings: BTreeMap<String, Setting>,
    #[serde(default)]
    pub(crate) ejections: BTreeMap<String, Ejection>,
    #[serde(default)]
    pub(crate) units: BTreeMap<String, Unit>,
    #[serde(default)]
    pub(crate) entities: BTreeMap<String, Entity>,
    #[serde(default)]
    pub(crate) operations: BTreeMap<String, Operation>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Project {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) base_package: String,
    pub(crate) java_release: u16,
    pub(crate) dialect: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Capability {
    pub(crate) id: String,
    pub(crate) kind: String,
    pub(crate) name: Option<String>,
    pub(crate) package: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Dependency {
    pub(crate) id: String,
    pub(crate) group: String,
    pub(crate) artifact: String,
    pub(crate) version: Option<String>,
    #[serde(default = "compile_scope")]
    pub(crate) scope: DependencyScope,
}

const fn compile_scope() -> DependencyScope {
    DependencyScope::Compile
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Setting {
    pub(crate) id: String,
    pub(crate) key: String,
    pub(crate) value: String,
    pub(crate) target: SettingTarget,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Ejection {
    pub(crate) id: String,
    pub(crate) target: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Unit {
    pub(crate) id: String,
    pub(crate) kind: UnitKind,
    pub(crate) java_name: Option<String>,
    pub(crate) package: Option<String>,
    #[serde(default)]
    pub(crate) variants: Vec<String>,
    #[serde(default)]
    pub(crate) on: Option<String>,
    #[serde(default)]
    pub(crate) yields: Option<String>,
    #[serde(default)]
    pub(crate) method: Option<EndpointMethod>,
    pub(crate) path: Option<String>,
    #[serde(default)]
    pub(crate) consumes: Option<RequestFormat>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Entity {
    pub(crate) id: String,
    #[serde(default = "active")]
    pub(crate) active: bool,
    pub(crate) java_name: Option<String>,
    pub(crate) table: Option<String>,
    #[serde(default)]
    pub(crate) facets: BTreeSet<Facet>,
    #[serde(default)]
    pub(crate) values: Vec<String>,
    #[serde(default)]
    pub(crate) fields: BTreeMap<String, Field>,
    #[serde(default)]
    pub(crate) indexes: BTreeMap<String, Index>,
}

const fn active() -> bool {
    true
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Field {
    pub(crate) id: String,
    pub(crate) java_name: Option<String>,
    pub(crate) column: Option<String>,
    #[serde(rename = "type")]
    pub(crate) type_name: String,
    #[serde(default = "required")]
    pub(crate) required: bool,
    #[serde(default)]
    pub(crate) non_blank: bool,
    #[serde(default)]
    pub(crate) primary_key: bool,
    #[serde(default)]
    pub(crate) unique: bool,
    #[serde(default)]
    pub(crate) indexed: bool,
    pub(crate) min_length: Option<u32>,
    pub(crate) max_length: Option<u32>,
}

const fn required() -> bool {
    true
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Index {
    pub(crate) id: String,
    pub(crate) name: Option<String>,
    pub(crate) columns: Vec<String>,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub(crate) enum Operation {
    Command {
        id: String,
        java_name: Option<String>,
        on: String,
        #[serde(default)]
        fields: Vec<String>,
        route: Option<String>,
    },
    Query {
        id: String,
        java_name: Option<String>,
        on: String,
        #[serde(default)]
        filters: Vec<String>,
        #[serde(default)]
        order_by: Vec<String>,
        limit: Option<u32>,
        route: Option<String>,
    },
    Transition {
        id: String,
        java_name: Option<String>,
        on: String,
        #[serde(default)]
        fields: Vec<String>,
        #[serde(default)]
        sets: Vec<String>,
        yields: Option<String>,
        route: Option<String>,
    },
    Event {
        id: String,
        java_name: Option<String>,
        on: Option<String>,
        #[serde(default)]
        fields: Vec<String>,
    },
}

impl Operation {
    pub(crate) fn id(&self) -> &str {
        match self {
            Self::Command { id, .. }
            | Self::Query { id, .. }
            | Self::Transition { id, .. }
            | Self::Event { id, .. } => id,
        }
    }

    pub(crate) fn java_name(&self) -> Option<&str> {
        match self {
            Self::Command { java_name, .. }
            | Self::Query { java_name, .. }
            | Self::Transition { java_name, .. }
            | Self::Event { java_name, .. } => java_name.as_deref(),
        }
    }
}
