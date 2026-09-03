//! How an operation spells itself to the templates of its recipes.
//!
//! Two recipes render from an operation: the Kafka slice of an `event`, and
//! the outbox of a command that delivers through one. Both name the same
//! things -- the event record, its publisher, the stable label as a topic, a
//! property prefix or a table -- so the vocabulary is one enum, and which
//! event a key means is decided here once: an event names itself, and an
//! outbox command names the one event it relays.

use crate::Diagnostic;
use crate::recipe::{Node, SourceSet};
use jails_contracts::Provenance;
use jails_model::{AppModel, Operation, OperationKind, StableId};
use std::collections::BTreeSet;

/// The typed values of an operation its templates may spell.
#[derive(Clone, Copy)]
pub(crate) enum Key {
    /// `{{topic}}`: `payout_settled` as `payout-settled`. The stable label
    /// rather than the Java name, so renaming the type leaves the deployed
    /// topic alone -- the same rule every other projection follows.
    Topic,
    /// `{{event}}`: the event record's type -- `<Name>Event`, without doubling
    /// a suffix the name already carries. For a command, the event its outbox
    /// relays.
    Event,
    /// `{{publisher}}`: the publisher of that event, which the `kafka` slice
    /// of the event emits.
    Publisher,
    /// `{{usecase}}`: the command's own Java type.
    Usecase,
    /// `{{property}}`: the stable label with dashes, as a settings prefix.
    Property,
    /// `{{table}}`: the stable label plus this suffix, so a renamed Java type
    /// does not strand the rows already in it.
    Table(&'static str),
}

/// The event a key about "the event" means for this operation.
fn event<'a>(model: &'a AppModel, operation: &'a Operation) -> Result<&'a Operation, Diagnostic> {
    match &operation.kind {
        OperationKind::Event(_) => Ok(operation),
        _ => super::outbox::relayed(model, operation),
    }
}

impl Node for Operation {
    type Key = Key;

    fn id(&self) -> &str {
        self.id.as_str()
    }

    fn name(&self) -> &str {
        &self.names.java_type
    }

    fn describe(&self) -> String {
        match &self.kind {
            OperationKind::Command(_) => format!("command `{}`", self.label),
            OperationKind::Event(_) => format!("event `{}`", self.label),
            _ => format!("operation `{}`", self.label),
        }
    }

    fn key(&self, model: &AppModel, key: Key) -> Result<(&'static str, String), Diagnostic> {
        Ok(match key {
            Key::Topic => ("topic", self.label.replace('_', "-")),
            Key::Event => (
                "event",
                crate::emit_java::with_suffix(&event(model, self)?.names.java_type, "Event"),
            ),
            Key::Publisher => (
                "publisher",
                format!("{}Publisher", event(model, self)?.names.java_type),
            ),
            Key::Usecase => ("usecase", self.names.java_type.clone()),
            Key::Property => ("property", self.label.replace('_', "-")),
            Key::Table(suffix) => ("table", format!("{}{suffix}", self.label)),
        })
    }

    fn file_keys(&self, _: &str, template_class: &str) -> Vec<(&'static str, String)> {
        vec![
            ("class", template_class.to_string()),
            ("name", self.names.java_type.clone()),
        ]
    }

    fn provenance(&self, artifact_id: String, ejectable: bool, pass: &'static str) -> Provenance {
        Provenance {
            artifact_id,
            ejection_id: None,
            ejectable,
            semantic_ids: BTreeSet::from([self.id.as_str().to_string()]),
            compiler_pass: pass.to_string(),
        }
    }

    fn header(&self) -> bool {
        true
    }

    fn splices_test_container(&self, _: SourceSet) -> bool {
        false
    }
}
