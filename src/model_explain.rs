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
use jails_model::{DerivedRoleKey, DerivedValue};
use jails_support::{Failure, Result};
use serde_json::json;

const SCHEMA: &str = "jails.model-explain.v1";

pub(crate) fn run(filter: Option<String>, invocation: Invocation) -> Result<()> {
    let manifest = crate::model_command::resolve_manifest(None)?;
    let (source, model) = crate::model_command::load_model(&manifest, invocation.output)?;
    // Captured rather than taken from the parsed model, because the reader's
    // layer renames arrive with the workspace and a linked model carries the
    // defaults. Showing `com.example.domain` to a project whose `jails.toml`
    // says `domain = "core"` would be a report about a project nobody has.
    let root = crate::model_command::root()?;
    let snapshot = jails_workspace::capture(&root, &manifest, source.as_bytes(), model)
        .map_err(|error| Failure::Told(format!("could not capture workspace: {error}")))?;
    let mut model = snapshot.model.model;
    model.project.layout = snapshot.project.layout;
    model.refresh_derived();

    let matched = model
        .derived
        .iter()
        .filter(|(key, value)| filter.as_deref().is_none_or(|f| matches(key, value, f)))
        .collect::<Vec<_>>();

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
