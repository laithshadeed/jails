//! What a JDL v1 source may say, as one table both halves read.
//!
//! **The parser refuses from this list and `jails explain jdl` prints it.**
//! The attributes a declaration accepts were twelve array literals at twelve
//! `reject_unknown_attributes` call sites, which is fine while nothing else
//! needs to know them -- and the moment a command explains the language, a
//! hand-written copy beside them is the drift this crate's gates exist to
//! stop. The registries for the other four axes already have owners
//! ([`crate::BuiltinType`], [`crate::ProjectionKind`],
//! [`crate::CapabilityKind`]), so this module only adds the one that did not.
//!
//! `docs/10-language.md` §9 through §12 is the prose these rows answer to.

/// One declaration family and the attributes it accepts.
pub struct Family {
    /// The keyword a source writes.
    pub keyword: &'static str,
    /// One line: what declaring it means.
    pub summary: &'static str,
    /// Every `@attribute` valid on it, in the order the parser lists them.
    pub attributes: &'static [&'static str],
}

/// `app`: the project block.
pub const APP: &[&str] = &["id"];
/// `cap`: an optional capability.
pub const CAPABILITY: &[&str] = &["id"];
/// `dep`: a build dependency.
pub const DEPENDENCY: &[&str] = &["id", "version", "scope"];
/// `prop`: a settings key.
pub const SETTING: &[&str] = &["id", "target"];
/// `entity`: a declared thing.
pub const ENTITY: &[&str] = &["id", "retired", "package"];
/// `enum`: a declared enumeration, which is an entity with constants.
pub const ENUM_DECLARATION: &[&str] = &["id"];
/// An enum constant inside an entity.
pub const ENUM_VALUE: &[&str] = &["id"];
/// A field inside an entity.
pub const FIELD: &[&str] = &[
    "id",
    "map",
    "pk",
    "notBlank",
    "unique",
    "index",
    "length",
    "positive",
    "nonnegative",
    "scope",
    "version",
    "default",
    "updated",
];
/// An operation's input field, which carries no identity of its own.
pub const OPERATION_FIELD: &[&str] = &["default", "notBlank", "length", "positive", "nonnegative"];
/// `index [...]` inside an entity.
pub const INDEX: &[&str] = &["id", "map"];
/// A relation between two entities.
pub const RELATION: &[&str] = &["id", "map"];
/// A component declaration.
pub const COMPONENT: &[&str] = &["id"];
/// One variant of a sealed component.
pub const COMPONENT_VARIANT: &[&str] = &["id"];
/// An operation: a command, query, transition or event.
pub const OPERATION: &[&str] = &["id", "internal"];
/// An event, which has no `@internal` to be.
pub const EVENT: &[&str] = &["id"];
/// `eject`: an implementation boundary handed to the reader.
pub const EJECTION: &[&str] = &["id", "adopted"];

/// The `use` projections an entity may declare, in the order §11 lists them.
///
/// The parser refuses anything else against this list and names it in the
/// fix, so the two cannot disagree.
pub const PROJECTIONS: &[&str] = &[
    "value", "repo", "service", "http", "dto", "factory", "search", "seed", "scaffold",
];

/// Every family, in the order a source writes them.
pub const FAMILIES: &[Family] = &[
    Family {
        keyword: "app",
        summary: "the project: its package, Java release, platform, build and storage",
        attributes: APP,
    },
    Family {
        keyword: "cap",
        summary: "an optional capability this project carries",
        attributes: CAPABILITY,
    },
    Family {
        keyword: "dep",
        summary: "a build dependency, with its version and scope",
        attributes: DEPENDENCY,
    },
    Family {
        keyword: "prop",
        summary: "one settings key and its value",
        attributes: SETTING,
    },
    Family {
        keyword: "entity",
        summary: "a declared thing: its fields, facets, indexes and operations",
        attributes: ENTITY,
    },
    Family {
        keyword: "  <field>",
        summary: "a field inside an entity, as `name: type` with its markers",
        attributes: FIELD,
    },
    Family {
        keyword: "  index",
        summary: "a composite or ordered index on the entity's table",
        attributes: INDEX,
    },
    Family {
        keyword: "  relation",
        summary: "a foreign key from this entity to another",
        attributes: RELATION,
    },
    Family {
        keyword: "  <operation>",
        summary: "a command, query, transition or event on the entity",
        attributes: OPERATION,
    },
    Family {
        keyword: "    <input>",
        summary: "one input field of an operation",
        attributes: OPERATION_FIELD,
    },
    Family {
        keyword: "enum",
        summary: "a declared enumeration and its constants",
        attributes: ENUM_DECLARATION,
    },
    Family {
        keyword: "component",
        summary: "a plain declaration: a record, value, enum, sealed type or service",
        attributes: COMPONENT,
    },
    Family {
        keyword: "eject",
        summary: "an implementation boundary transferred to you",
        attributes: EJECTION,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Every family names an attribute list, and `@id` is on all but the two
    /// that carry no identity of their own.
    #[test]
    fn every_family_declares_what_it_accepts() {
        for family in FAMILIES {
            assert!(
                !family.attributes.is_empty(),
                "`{}` accepts nothing",
                family.keyword
            );
            assert!(
                !family.summary.is_empty(),
                "`{}` says nothing about itself",
                family.keyword
            );
        }
        let without_id: Vec<&str> = FAMILIES
            .iter()
            .filter(|family| !family.attributes.contains(&"id"))
            .map(|family| family.keyword)
            .collect();
        assert_eq!(
            without_id,
            ["    <input>"],
            "only an operation's input has no identity of its own"
        );
    }
}
