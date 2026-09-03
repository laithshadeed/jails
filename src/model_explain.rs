//! `jails model explain` — what the convention decided, and why.
//!
//! **JDL v1 §18.4: convention must not mean hidden behaviour.** A generated
//! project is full of names nobody typed — the package a repository port
//! lands in, the plural of a table, the suffix a component kind adds, the
//! route an operation answers on. Each is a rule the compiler applied, and
//! this command says which rule, so a rule that moves is visible without
//! reading the emitted tree and inferring it.
//!
//! The records themselves live in the model (`jails_model::derived`), which is
//! what makes them part of the plan digest rather than a report generated
//! beside one. This module is the view: it captures the workspace first, so
//! the packages shown are the ones this project actually gets after its
//! `jails.toml` renames, and filters by owner, role, rule or value.
//!
//! **The `java-package` rows carry the §9.7 divergence.** Six of the
//! twenty-three packages sit under a head JDL v1 §9.7 does not close —
//! `repository`, `application`, `ports` — and a head like that is renamed by
//! nothing, so a project that renames `adapters` does not rename them. Their
//! `rule_id` says `convention.facet.*` where a layer's says
//! `convention.layer.*`, so the divergence is displayed and digested.
//! Reconciling it moves files in every generated project, which is why it is
//! shown rather than quietly corrected.

use crate::{Invocation, Output};
use jails_model::{DerivedRoleKey, DerivedValue, StableId};
use jails_support::{Failure, Result};
use serde_json::json;
use std::collections::BTreeSet;

const SCHEMA: &str = "jails.model-explain.v1";

pub(crate) fn run(filter: Option<String>, invocation: Invocation) -> Result<()> {
    let manifest = crate::model_command::resolve_manifest(None)?;
    // The source itself is not needed: this command reports what the linked
    // model derived, and the layout is the one thing outside it.
    let (_, model) =
        crate::model_command::load_model(&invocation.root()?, &manifest, invocation.output)?;
    // **The layout, and only the layout.** The reader's layer renames arrive
    // with `jails.toml` and a linked model carries the defaults, so showing
    // `com.example.domain` to a project whose file says `domain = "core"`
    // would be a report about a project nobody has.
    //
    // Read through `capture::facts`, which is the same reader the capture
    // boundary uses, rather than by capturing the workspace: this command
    // answers from the model and needs no file in the tree. A full capture
    // read 1,421 files to learn one table's worth of package names, which is
    // why `jails model explain` cost 149 ms at a hundred entities.
    let root = crate::model_command::root()?;
    let facts = jails_project::capture::facts(&root)
        .map_err(|error| Failure::diagnosed(error.code, error.to_string()))?;
    let mut model = model;
    model.project.layout = facts.layout;
    model.refresh_derived();

    // **An entity's name means the entity and its fields.** A reader asking
    // about `Note` wants its Java type, its table, its route and one row per
    // column -- not the single row whose text happens to contain the word.
    // Resolved from the model, so the answer is the same set `destroy` and
    // `entity status` would act on.
    let owners = filter.as_deref().and_then(|f| entity_owners(&model, f));
    let mut matched = model
        .derived
        .iter()
        .filter(|(key, value)| match (&owners, filter.as_deref()) {
            (Some(owners), _) => owners.contains(&key.owner),
            (None, Some(filter)) => matches(key, value, filter),
            (None, None) => true,
        })
        .collect::<Vec<_>>();
    // **What the reader declared, before what the convention filled in.** A
    // fresh project derives twenty-three package names and five names from
    // the one entity somebody wrote, and printing them in id order buried the
    // five under the twenty-three.
    matched.sort_by_key(|(key, _)| (is_convention_package(&model, key), key.owner.clone()));

    if invocation.output != Output::Human {
        return crate::model_command::print_json(&json!({
            "schema": SCHEMA,
            "manifest": manifest,
            "filter": filter,
            "language_version": model.language_version,
            "convention_version": model.convention_version,
            "derived": matched
                .iter()
                .map(|(key, value)| json!({ "key": key, "value": value }))
                .collect::<Vec<_>>(),
        }));
    }

    if matched.is_empty() {
        // Not an error: "nothing matched" is an answer, and the fix line names
        // the two things a reader most often typed wrong.
        println!(
            "no derived value matches `{}`\n       fix: pass a stable id, a role \
             (`java-package`, `java-type`, `sql-table`, `sql-column`, `http-route`) \
             or nothing to list every record",
            filter.unwrap_or_default()
        );
        return Ok(());
    }
    let width = matched
        .iter()
        .map(|(key, _)| owner_column(key).len())
        .max()
        .unwrap_or(0);
    for (key, value) in matched {
        let pin = match (&value.pinned, &value.replaces) {
            (true, Some(replaced)) => format!("  pinned, replaces {replaced}"),
            (true, None) => "  pinned".to_string(),
            (false, _) => String::new(),
        };
        println!(
            "{:width$}  {:<12}  {}  [{}]{pin}",
            owner_column(key),
            key.role.as_str(),
            value.value,
            value.rule_id
        );
    }
    Ok(())
}

/// The owner as a reader would name it: the stable id, plus the slot when one
/// owner holds several records of a role.
fn owner_column(key: &DerivedRoleKey) -> String {
    if key.slot.is_empty() {
        key.owner.clone()
    } else {
        format!("{}/{}", key.owner, key.slot)
    }
}

/// The owner ids an entity name covers: the entity, and each of its fields.
///
/// `None` when the filter names no entity, which leaves the substring match
/// below to answer -- a reader arriving with a package, a role or a stable id
/// is asking a different question.
fn entity_owners(model: &jails_model::AppModel, filter: &str) -> Option<BTreeSet<String>> {
    let label = jails_model::field_syntax::java_to_label(filter);
    let entity = model
        .entities
        .values()
        .find(|entity| entity.label == label || entity.names.java_type == filter)?;
    let mut owners = BTreeSet::from([entity.id.as_str().to_string()]);
    owners.extend(
        entity
            .fields
            .iter()
            .map(|field| field.id.as_str().to_string()),
    );
    Some(owners)
}

/// Whether a row is one of the project's own layer packages.
///
/// These are the convention filling in a name nobody typed, and there are
/// twenty-three of them on every project; a row a declaration owns is the
/// answer to "why is my class called that".
fn is_convention_package(model: &jails_model::AppModel, key: &DerivedRoleKey) -> bool {
    key.owner == model.project.id.as_str()
}

/// **A substring match over every field, deliberately.** JDL v1 §18.4 says
/// `model explain <stable-id-or-boundary>`, and a boundary is not one closed
/// vocabulary: a reader arrives with an id, a role, a package they saw in a
/// stack trace, or the suffix of a class they did not expect. Matching all of
/// them beats refusing three quarters of what people type at a read-only
/// command that writes nothing.
fn matches(key: &DerivedRoleKey, value: &DerivedValue, filter: &str) -> bool {
    key.owner.contains(filter)
        || key.slot.contains(filter)
        || key.role.as_str() == filter
        || value.value.contains(filter)
        || value.rule_id.contains(filter)
        || value.inputs.iter().any(|input| input.contains(filter))
}
