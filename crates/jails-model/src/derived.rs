//! The inspectable record of every name convention decided rather than written.
//!
//! **JDL v1 §18.4's rule is that convention must not mean hidden
//! behaviour**, and §7.2 puts these records inside `AppModel` — so they are
//! part of the accepted-model and plan digest, not a report generated beside
//! it. A convention that changes has to change a digest, or "the compiler must
//! not silently change a convention" is a sentence with nothing behind it.
//!
//! Each record answers four questions about one derived value: whose it is
//! (`owner`), what kind of name it is (`role`), what the rule was (`rule_id`),
//! and what the rule read (`inputs`). `pinned` marks a value the author wrote
//! instead, in which case `replaces` holds the convention it displaced — which
//! is the pair `model explain` exists to show, because a pin is invisible in
//! generated output and permanent in its effect.
//!
//! ## What is recorded here, and what is not
//!
//! These are the **linker's** conventions: packages, Java type names, SQL
//! tables and columns, and HTTP routes. JDL v1 §18.4 also closes `file`, `test`,
//! `migration`, `cap-prerequisite` and `build-entry`, and those are decided by
//! the compiler, one pass later, against a workspace the model has not seen.
//! Recording them here would mean either duplicating the compiler's answer or
//! letting a filesystem fact into the model, and the second breaks the purity
//! the plan digest rests on. They belong to the plan's own derived array; this
//! is the model's half, and saying so is better than a `role` enum with five
//! variants nothing ever constructs.

use crate::app::ProjectIntent;
use crate::id::StableId;
use crate::layout::{Head, Package};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// What kind of name a derived record is about.
///
/// Closed, and a subset of JDL v1 §18.4's list — see the module docs for which half
/// of it lives here and why.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DerivedRole {
    JavaPackage,
    JavaType,
    SqlTable,
    SqlColumn,
    HttpRoute,
}

impl DerivedRole {
    /// Declaration order, so [`Self::ALL`] and the match below cannot drift.
    pub const ALL: [Self; 5] = [
        Self::JavaPackage,
        Self::JavaType,
        Self::SqlTable,
        Self::SqlColumn,
        Self::HttpRoute,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::JavaPackage => "java-package",
            Self::JavaType => "java-type",
            Self::SqlTable => "sql-table",
            Self::SqlColumn => "sql-column",
            Self::HttpRoute => "http-route",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|role| role.as_str() == value)
    }
}

/// The identity of one derived record.
///
/// `slot` is the third component and is empty for almost every row. It exists
/// because one owner can hold several records of one role: a project derives
/// twenty-three package names, all `java-package`, all owned by the project.
/// Keying on `(owner, role)` alone would silently keep the last one, which is
/// the failure mode this whole module is about.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct DerivedRoleKey {
    pub owner: String,
    pub role: DerivedRole,
    pub slot: String,
}

impl DerivedRoleKey {
    pub fn new(owner: impl Into<String>, role: DerivedRole) -> Self {
        Self {
            owner: owner.into(),
            role,
            slot: String::new(),
        }
    }

    pub fn slotted(owner: impl Into<String>, role: DerivedRole, slot: impl Into<String>) -> Self {
        Self {
            owner: owner.into(),
            role,
            slot: slot.into(),
        }
    }

    /// The wire spelling: `<role>:<owner>` with `/<slot>` when there is one.
    ///
    /// **A struct cannot be a JSON object key**, and this map is inside the
    /// model, which is serialized into the compiler lock and every `--json`
    /// output. So the key has one string form, and it is round-tripped rather
    /// than merely displayed -- a lock is read back and the map has to come
    /// out identical.
    ///
    /// Unambiguous by the shapes of its parts: a role is one of five kebab
    /// literals, a stable id is lowercase ASCII with `_` and `-`, and a slot
    /// is package segments. Neither `:` nor `/` can appear inside any of them,
    /// so splitting on the first of each is exact.
    fn wire(&self) -> String {
        let mut wire = format!("{}:{}", self.role.as_str(), self.owner);
        if !self.slot.is_empty() {
            wire.push('/');
            wire.push_str(&self.slot);
        }
        wire
    }

    fn from_wire(wire: &str) -> Option<Self> {
        let (role, rest) = wire.split_once(':')?;
        let (owner, slot) = rest
            .split_once('/')
            .map_or((rest, ""), |(owner, slot)| (owner, slot));
        Some(Self {
            owner: owner.to_string(),
            role: DerivedRole::parse(role)?,
            slot: slot.to_string(),
        })
    }
}

impl Serialize for DerivedRoleKey {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.wire())
    }
}

impl<'de> Deserialize<'de> for DerivedRoleKey {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = String::deserialize(deserializer)?;
        Self::from_wire(&wire).ok_or_else(|| {
            serde::de::Error::custom(format!("`{wire}` is not a derived-record key"))
        })
    }
}

/// One derived value, and enough to argue with it.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DerivedValue {
    /// The name the project actually gets.
    pub value: String,
    /// Which rule produced it. Stable, so a reader can look one up and a
    /// diff of two models says which convention moved.
    pub rule_id: String,
    /// The stable IDs the rule read. Empty when the rule read only its owner.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inputs: Vec<String>,
    /// True only when the author wrote the value instead of the rule deriving
    /// it — a contract pin, in JDL v1 §18.4's terms.
    #[serde(default, skip_serializing_if = "is_false")]
    pub pinned: bool,
    /// What the pin displaced. `Some` exactly when `pinned` is true and the
    /// convention would have produced something else.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replaces: Option<String>,
}

fn is_false(value: &bool) -> bool {
    !*value
}

impl DerivedValue {
    /// A value the convention produced.
    pub fn derived(value: impl Into<String>, rule_id: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            rule_id: rule_id.into(),
            inputs: Vec::new(),
            pinned: false,
            replaces: None,
        }
    }

    /// A name, compared against what the convention would have produced.
    ///
    /// **`pinned` is decided by the difference, not by a flag carried from the
    /// source**, and that is a deliberate limitation with a reason. The linked
    /// model does not remember whether an author wrote `@map(notes)` on an
    /// entity whose convention already derives `notes` — and if it did,
    /// `derived` would stop being a function of the model, so a patched model
    /// and a re-linked one would disagree about a field that is in the plan
    /// digest.
    ///
    /// The cost is that a redundant pin reads as a derivation. Nothing
    /// observable turns on it: JDL v1 §18.4's `pinned` exists so a reader can see a
    /// value the convention did *not* produce, and a pin that agrees with the
    /// convention has displaced nothing.
    pub fn named(
        value: impl Into<String>,
        rule_id: impl Into<String>,
        convention: impl Into<String>,
    ) -> Self {
        let value = value.into();
        let convention = convention.into();
        let pinned = value != convention;
        Self {
            value,
            rule_id: rule_id.into(),
            inputs: Vec::new(),
            pinned,
            replaces: pinned.then_some(convention),
        }
    }

    #[must_use]
    pub fn reading(mut self, inputs: impl IntoIterator<Item = String>) -> Self {
        self.inputs = inputs.into_iter().collect();
        self
    }
}

/// The package convention, as twenty-three inspectable rows.
///
/// **This is where the §9.7 divergence is visible.** JDL v1 §9.7 closes
/// eleven layers and the compiler emits into twenty-three packages, six of
/// which sit under a head §9.7 does not name — `repository`, `application`
/// and `ports`. A `Head::Facet` is renamed by nothing, so a project whose
/// `jails.toml` renames a layer gets the rename for `domain` and not for
/// these.
///
/// The `rule_id` says which it is (`convention.layer.*` against
/// `convention.facet.*`), so the divergence is displayed by `model explain`
/// and digested with the model. Reconciling it moves files in every project
/// generated so far, which is why it is recorded rather than quietly
/// corrected — JDL v1 §3.1 rule 4 makes conventions part of `jdl 1`.
pub fn package_conventions(
    project: &ProjectIntent,
    into: &mut BTreeMap<DerivedRoleKey, DerivedValue>,
) {
    for package in Package::ALL {
        let (head, tail) = package.placement();
        // The slot is the convention's own identity, so it is built from the
        // *default* names -- `adapters.jdbc` stays `adapters.jdbc` in a
        // project that renamed the layer, and the rename shows up where it
        // belongs, in the value.
        let (rule, slot) = match head {
            None => ("convention.base-package".to_string(), String::new()),
            Some(Head::Layer(layer)) => (
                format!("convention.layer.{}", layer.package()),
                segments(layer.package(), tail),
            ),
            Some(Head::Facet(facet)) => {
                (format!("convention.facet.{facet}"), segments(facet, tail))
            }
        };
        into.insert(
            DerivedRoleKey::slotted(project.id.as_str(), DerivedRole::JavaPackage, slot),
            DerivedValue::derived(project.package_for(package), rule),
        );
    }
}

fn segments(head: &str, tail: &str) -> String {
    if tail.is_empty() {
        head.to_string()
    } else {
        format!("{head}.{tail}")
    }
}

/// Every name the linker derived rather than read, for one whole model.
///
/// **Recomputed from the model, never accumulated.** `AppModel::apply` moves a
/// model one patch at a time and `Compiler::compile` applies the reader's
/// layout on top, so a map built once at link time would go stale in exactly
/// the two places that matter. Being a pure function of the model is also what
/// makes the digest honest: two models that agree cannot carry different
/// derived records.
pub fn records(model: &crate::AppModel) -> BTreeMap<DerivedRoleKey, DerivedValue> {
    let mut into = BTreeMap::new();
    package_conventions(&model.project, &mut into);
    for entity in model.entities.values() {
        into.insert(
            DerivedRoleKey::new(entity.id.as_str(), DerivedRole::JavaType),
            DerivedValue::named(
                &entity.names.java_type,
                "convention.java-type.entity",
                crate::naming::upper_camel_case(&entity.label),
            ),
        );
        // Only a stored entity has a table. An enum carries constants and no
        // SQL at all, and emitting a `sql-table` row for one would advertise a
        // table `model explain` could send somebody looking for.
        if !entity.facets.contains(&crate::Facet::Enum) {
            into.insert(
                DerivedRoleKey::new(entity.id.as_str(), DerivedRole::SqlTable),
                DerivedValue::named(
                    &entity.names.sql_table,
                    "convention.sql-table.pluralize",
                    crate::naming::plural_snake_case(&entity.label),
                ),
            );
        }
        for field in &entity.fields {
            into.insert(
                DerivedRoleKey::new(field.id.as_str(), DerivedRole::SqlColumn),
                DerivedValue::named(
                    &field.names.sql_column,
                    "convention.sql-column.snake-case",
                    crate::naming::snake_case(&field.label),
                )
                .reading([entity.id.as_str().to_string()]),
            );
        }
    }
    for component in model.components.values() {
        // The *stem* is the author's and the suffix is the convention, so this
        // row is never pinned: what a reader wants to see is which suffix the
        // kind added, which is precisely what would otherwise be invisible
        // until a file appeared with a name nobody typed.
        into.insert(
            DerivedRoleKey::new(component.id.as_str(), DerivedRole::JavaType),
            DerivedValue::derived(
                component.kind.primary_type(&component.name),
                format!("convention.java-type.component.{}", component.kind.label()),
            ),
        );
    }
    for operation in model.operations.values() {
        into.insert(
            DerivedRoleKey::new(operation.id.as_str(), DerivedRole::JavaType),
            DerivedValue::named(
                &operation.names.java_type,
                "convention.java-type.operation",
                crate::naming::upper_camel_case(&operation.label),
            ),
        );
        let (declared, resolved) = crate::operation::routes(&operation.kind);
        if let Some(resolved) = resolved {
            into.insert(
                DerivedRoleKey::new(operation.id.as_str(), DerivedRole::HttpRoute),
                DerivedValue {
                    // `canonical`, not `{:?}`: the record is what `jails model
                    // explain` prints and what the digest carries, and `Post
                    // /actions/create` is a spelling no route grammar, Spring
                    // annotation or collision key uses.
                    value: resolved.canonical(),
                    rule_id: "convention.http-route".to_string(),
                    inputs: Vec::new(),
                    // The one row where a pin *is* recoverable: the
                    // declaration keeps the author's path beside the linked
                    // route, because JDL v1 §12.6 makes an external contract pin
                    // something a project promises rather than a spelling.
                    pinned: declared.is_some(),
                    replaces: None,
                },
            );
        }
    }
    into
}

#[cfg(test)]
mod tests {
    use super::*;

    const MODEL: &str = "jdl 1\napp Notes @id(project_notes) {\n  pkg com.example.notes\n                           java 26\n  platform spring\n  build maven\n  storage postgres\n}\n\n                         entity SupportPerson @id(ent_person) {\n  use repo\n                           id: uuid @id(fld_person_id) @pk\n                           familyName: string @id(fld_person_family)\n\n                           command Create(familyName) @id(op_person_create)\n}\n\n                         component service Billing @id(cmp_billing) {\n}\n";

    fn model() -> crate::AppModel {
        crate::parse_jdl(MODEL).unwrap()
    }

    /// **The property the field's placement in the digest rests on.**
    ///
    /// `derived` is inside `AppModel`, so it travels in the compiler lock and
    /// in every plan digest. If it were an accumulator rather than a function,
    /// two models that agree could carry different records and the digest
    /// would report a difference that is not one -- or, worse, a stale record
    /// would keep a retired convention alive in the lock.
    #[test]
    fn derived_records_are_a_function_of_the_model() {
        let mut model = model();
        let once = model.derived.clone();
        model.refresh_derived();
        assert_eq!(model.derived, once);
        assert_eq!(records(&model), once);
    }

    /// The conventions this exists to display, on one model that exercises
    /// each rule.
    #[test]
    fn every_role_records_the_rule_that_produced_it() {
        let model = model();
        let get = |wire: &str| {
            model
                .derived
                .get(&DerivedRoleKey::from_wire(wire).expect("a well-formed key"))
                .unwrap_or_else(|| panic!("no derived record for `{wire}`"))
        };
        // The pluralizer, on the case JDL v1 §9.7 names: the last word only.
        assert_eq!(get("sql-table:ent_person").value, "support_people");
        assert_eq!(get("java-type:ent_person").value, "SupportPerson");
        assert_eq!(get("sql-column:fld_person_family").value, "family_name");
        // The suffix a component kind adds, which is otherwise invisible until
        // a file appears with a name nobody typed.
        assert_eq!(get("java-type:cmp_billing").value, "BillingService");
        assert_eq!(
            get("java-type:cmp_billing").rule_id,
            "convention.java-type.component.service"
        );
    }

    /// **The §9.7 divergence, as data.**
    ///
    /// Six of the twenty-three emitted packages sit under a head JDL v1 §9.7
    /// does not close, and a head like that is renamed by nothing -- so a
    /// project that renames `adapters` keeps `repository` and `application`
    /// exactly as they were. The `rule_id` is where that shows, which is what
    /// makes the divergence inspectable and digested.
    #[test]
    fn a_package_outside_the_nine_seven_layers_says_so_in_its_rule() {
        let model = model();
        let facets = model
            .derived
            .iter()
            .filter(|(key, value)| {
                key.role == DerivedRole::JavaPackage
                    && value.rule_id.starts_with("convention.facet.")
            })
            .map(|(key, _)| key.slot.clone())
            .collect::<Vec<_>>();
        assert_eq!(
            facets,
            [
                "application",
                "application.commands",
                "application.queries",
                "application.transitions",
                "ports.events",
                "ports.http",
                "ports.search",
                "repository",
            ]
        );
    }

    /// A key is a JSON object key, so it has to survive being one.
    #[test]
    fn a_derived_key_round_trips_through_its_wire_spelling() {
        for key in [
            DerivedRoleKey::new("ent_note", DerivedRole::SqlTable),
            DerivedRoleKey::slotted("project_demo", DerivedRole::JavaPackage, "adapters.jdbc"),
        ] {
            assert_eq!(DerivedRoleKey::from_wire(&key.wire()), Some(key));
        }
        assert_eq!(DerivedRoleKey::from_wire("not-a-role:ent_note"), None);
    }

    /// A name the author wrote is shown as a pin, with what it displaced.
    ///
    /// **On a column, because JDL v1 has no way to pin an entity's table** --
    /// an entity takes `id` and `retired` and nothing else, so `sql-table` is
    /// always the pluralizer's answer for a v1 source. `.jails/model.toml` can
    /// state one, which is why the record still compares against the
    /// convention rather than assuming it.
    #[test]
    fn a_written_name_replaces_the_convention_it_displaced() {
        let model = crate::parse_jdl(
            "jdl 1\napp Notes @id(project_notes) {\n  pkg com.example.notes\n  java 26\n  platform plain\n  build maven\n  storage none\n}\n\nentity Note @id(ent_note) {\n  id: uuid @id(fld_note_id) @pk\n  writtenAt: instant @id(fld_note_written) @map(created_ts)\n}\n",
        )
        .unwrap();
        let column = model
            .derived
            .get(&DerivedRoleKey::new(
                "fld_note_written",
                DerivedRole::SqlColumn,
            ))
            .expect("every field has a column");
        assert_eq!(column.value, "created_ts");
        assert!(column.pinned);
        assert_eq!(column.replaces.as_deref(), Some("written_at"));
        assert_eq!(column.inputs, ["ent_note"]);

        let table = model
            .derived
            .get(&DerivedRoleKey::new("ent_note", DerivedRole::SqlTable))
            .expect("a stored entity has a table");
        assert!(!table.pinned);
        assert_eq!(table.value, "notes");
    }
}
