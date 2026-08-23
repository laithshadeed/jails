//! The `jails.command-result.v1` encoding, written by hand.
//!
//! ## Why not a derive
//!
//! plan.md §R3.4 calls this encoding normative and says it uses a dedicated
//! encoder, "not Serde defaults". The defaults are wrong here in ways that
//! are invisible until a consumer breaks:
//!
//! A `u64` emitted as a JSON number loses precision in every JavaScript
//! consumer above 2^53. Byte lengths and generations are `u64`, so they are
//! emitted as decimal *strings*.
//!
//! A map with a non-string key depends on the implementation's key coercion.
//! Maps are therefore sorted arrays of `{"key":…,"value":…}`, and sets are
//! sorted arrays, so the same value has one encoding.
//!
//! A field omitted when it is `None` makes a consumer's absent-versus-null
//! distinction depend on a serialiser setting. Every declared field is
//! emitted; `None` is `null` and an empty collection is `[]`.
//!
//! And an enum tagged internally puts a `tag` key inside the payload, which
//! collides the first time a variant has a field called `tag`. Variants are
//! externally tagged: a unit variant is a string, and anything else is a
//! one-key object.

use crate::command::{
    CommandEnvelope, CommandReport, EffectRetryReport, ErrorReport, SCHEMA as COMMAND_SCHEMA,
};
use crate::prepare::{OperationTarget, PreparedKind};
use crate::receipt::AppliedReceipt;
use crate::report::{Report, ReportedLedger, ReportedOp, SCHEMA as REPORT_SCHEMA, Warning};
use jails_protocol::conflict::FileImage;
use jails_protocol::effect::{EffectState, PostCommitEffect};
use jails_protocol::identity::{ObjectId, ObjectRef};
use jails_protocol::resource::ResourceOwner;

/// One compact UTF-8 object followed by a newline.
pub fn envelope(envelope: &CommandEnvelope) -> String {
    let mut out = String::new();
    out.push('{');
    field(&mut out, "schema", &quoted(COMMAND_SCHEMA), true);
    field(&mut out, "status", &quoted(envelope.status.label()), false);
    field(
        &mut out,
        "exit_code",
        &envelope.exit_code().to_string(),
        false,
    );
    field(
        &mut out,
        "project_commit",
        &quoted(envelope.project_commit.label()),
        false,
    );
    // Always present, and ordinarily empty: an observationally clean recovery
    // is omitted from the list rather than from the field.
    field(&mut out, "recovery", "[]", false);
    field(
        &mut out,
        "report",
        &option(envelope.report.as_ref(), report),
        false,
    );
    field(
        &mut out,
        "receipt",
        &option(envelope.receipt.as_ref(), receipt),
        false,
    );
    field(
        &mut out,
        "error",
        &option(envelope.error.as_ref(), error),
        false,
    );
    out.push('}');
    out.push('\n');
    out
}

fn report(value: &CommandReport) -> String {
    match value {
        CommandReport::Prepared(report) => variant("prepared", &prepared(report)),
        CommandReport::EffectRetry(retry) => variant("effect-retry", &effect_retry(retry)),
    }
}

fn prepared(value: &Report) -> String {
    let mut out = String::from("{");
    field(&mut out, "schema", &quoted(REPORT_SCHEMA), true);
    field(
        &mut out,
        "operation",
        &quoted(&value.operation.to_hex()),
        false,
    );
    field(
        &mut out,
        "transaction",
        &quoted(&value.transaction.to_hex()),
        false,
    );
    field(&mut out, "kind", &kind(&value.kind), false);
    field(
        &mut out,
        "operations",
        &array(&value.operations, operation),
        false,
    );
    field(&mut out, "ledger", &ledger(&value.ledger), false);
    field(
        &mut out,
        "post_commit",
        &array(&value.post_commit, |effect| {
            let mut out = String::from("{");
            field(&mut out, "effect", &post_commit(&effect.effect), true);
            field(&mut out, "state", &effect_state(&effect.state), false);
            out.push('}');
            out
        }),
        false,
    );
    field(
        &mut out,
        "warnings",
        &array(&value.warnings, warning),
        false,
    );
    out.push('}');
    out
}

fn kind(value: &PreparedKind) -> String {
    match value {
        PreparedKind::Apply => quoted("apply"),
        PreparedKind::Conflict { paths } => variant(
            "conflict",
            &format!(
                "{{\"paths\":{}}}",
                array(paths, |path| quoted(path.as_str()))
            ),
        ),
        PreparedKind::Finalise { origin } => variant(
            "finalise",
            &format!("{{\"origin\":{}}}", quoted(&origin.to_hex())),
        ),
        PreparedKind::Abort { origin } => variant(
            "abort",
            &format!("{{\"origin\":{}}}", quoted(&origin.to_hex())),
        ),
    }
}

fn operation(value: &ReportedOp) -> String {
    let mut out = String::from("{");
    field(&mut out, "kind", &quoted(value.kind.label()), true);
    field(&mut out, "path", &target(&value.path), false);
    field(
        &mut out,
        "before",
        &option(value.before.as_ref(), digest),
        false,
    );
    field(
        &mut out,
        "after",
        &option(value.after.as_ref(), digest),
        false,
    );
    // A `u64` as a JSON number loses precision above 2^53 in every JavaScript
    // consumer, so a byte length is a decimal string.
    field(
        &mut out,
        "bytes",
        &option(value.bytes.as_ref(), |bytes| quoted(&bytes.to_string())),
        false,
    );
    field(
        &mut out,
        "mode",
        &option(value.mode.as_ref(), |mode| mode.bits().to_string()),
        false,
    );
    field(
        &mut out,
        "contributors",
        &array(&value.contributors.iter().collect::<Vec<_>>(), |owner| {
            owner_of(owner)
        }),
        false,
    );
    out.push('}');
    out
}

fn owner_of(owner: &&ResourceOwner) -> String {
    match owner {
        ResourceOwner::Entity(id) => variant("entity", &quoted(&format!("{id:?}"))),
        ResourceOwner::OneShot(id) => variant("one-shot", &quoted(&format!("{id:?}"))),
    }
}

fn target(value: &OperationTarget) -> String {
    match value {
        OperationTarget::Project(path) => variant("project", &quoted(path.as_str())),
        // Never disguised as an ordinary project output.
        OperationTarget::LegacyMachine(path) => {
            variant("legacy-machine", &quoted(&legacy_spelling(path)))
        }
    }
}

/// The exact `.jails/...` spelling §R3.4 requires.
fn legacy_spelling(path: &jails_protocol::snapshot::LegacySourcePath) -> String {
    use jails_protocol::snapshot::LegacySourcePath as L;
    match path {
        L::Schema1Ledger => ".jails/ledger.toml".to_string(),
        L::AppState => ".jails/app-state-v1".to_string(),
        L::IntentFiles { name } => format!(".jails/intents/{}", name.as_str()),
        L::ModelFiles { name } => format!(".jails/models/{}", name.as_str()),
        L::GlobalFiles => ".jails/files".to_string(),
        L::VersionFile => ".jails/version".to_string(),
    }
}

fn ledger(value: &ReportedLedger) -> String {
    let mut out = String::from("{");
    field(&mut out, "kind", &quoted(value.kind.label()), true);
    field(&mut out, "before", &image(&value.before), false);
    field(&mut out, "after", &image(&value.after), false);
    out.push('}');
    out
}

fn image(value: &FileImage) -> String {
    match value {
        FileImage::Absent => quoted("absent"),
        FileImage::Present { object, mode } => variant(
            "present",
            &format!(
                "{{\"object\":{},\"mode\":{}}}",
                object_ref(object),
                mode.bits()
            ),
        ),
    }
}

fn object_ref(value: &ObjectRef) -> String {
    format!(
        "{{\"id\":{},\"len\":{}}}",
        quoted(&value.id.to_hex()),
        quoted(&value.len.to_string())
    )
}

fn post_commit(value: &PostCommitEffect) -> String {
    match value {
        PostCommitEffect::ComposeReconcile {
            compose_output,
            desired_services,
            stop_services,
            ..
        } => variant(
            "compose-reconcile",
            &format!(
                "{{\"compose_output\":{},\"desired_services\":{},\"stop_services\":{}}}",
                quoted(compose_output.as_str()),
                array(&desired_services.keys().collect::<Vec<_>>(), |name| quoted(
                    name.as_str()
                )),
                array(&stop_services.iter().collect::<Vec<_>>(), |name| quoted(
                    name.as_str()
                )),
            ),
        ),
    }
}

fn effect_state(value: &EffectState) -> String {
    match value {
        EffectState::Deferred => quoted("deferred"),
        EffectState::Pending { next_attempt } => {
            variant("pending", &format!("{{\"next_attempt\":{next_attempt}}}"))
        }
        EffectState::Running { attempt } => {
            variant("running", &format!("{{\"attempt\":{attempt}}}"))
        }
        EffectState::Succeeded => quoted("succeeded"),
        EffectState::Failed {
            attempt,
            code,
            summary,
        } => variant(
            "failed",
            &format!(
                "{{\"attempt\":{attempt},\"code\":{},\"summary\":{}}}",
                quoted(&format!("{code:?}").to_lowercase()),
                quoted(summary)
            ),
        ),
        EffectState::Superseded { by } => variant(
            "superseded",
            &format!(
                "{{\"by\":{}}}",
                option(by.as_ref(), |id| quoted(&id.to_hex()))
            ),
        ),
    }
}

fn warning(value: &Warning) -> String {
    let mut out = String::from("{");
    field(&mut out, "code", &quoted(value.code.label()), true);
    field(&mut out, "paths", &array(&value.paths, target), false);
    field(&mut out, "message", &quoted(&value.message), false);
    out.push('}');
    out
}

fn effect_retry(value: &EffectRetryReport) -> String {
    let mut out = String::from("{");
    field(
        &mut out,
        "operation",
        &quoted(&value.operation.to_hex()),
        true,
    );
    field(
        &mut out,
        "transaction",
        &quoted(&value.transaction.to_hex()),
        false,
    );
    field(
        &mut out,
        "effect_index",
        &value.effect_index.to_string(),
        false,
    );
    field(
        &mut out,
        "effect_id",
        &quoted(&value.effect_id.to_hex()),
        false,
    );
    field(&mut out, "effect", &post_commit(&value.effect), false);
    field(
        &mut out,
        "reason",
        &quoted(match value.reason {
            jails_protocol::transition::EffectResumeReason::Interrupted => "interrupted",
            jails_protocol::transition::EffectResumeReason::ExplicitRetry => "explicit-retry",
        }),
        false,
    );
    field(&mut out, "before", &effect_state(&value.before), false);
    field(
        &mut out,
        "after",
        &option(value.after.as_ref(), effect_state),
        false,
    );
    out.push('}');
    out
}

fn receipt(value: &AppliedReceipt) -> String {
    let mut out = String::from("{");
    field(
        &mut out,
        "operation_id",
        &quoted(&value.operation_id.to_hex()),
        true,
    );
    field(
        &mut out,
        "transaction_id",
        &quoted(&value.transaction_id.to_hex()),
        false,
    );
    field(
        &mut out,
        "files",
        &array(&value.files, |file| {
            let mut out = String::from("{");
            field(&mut out, "path", &target(&file.path), true);
            field(&mut out, "before", &image(&file.before), false);
            field(&mut out, "after", &image(&file.after), false);
            field(
                &mut out,
                "contributors",
                &array(&file.contributors.iter().collect::<Vec<_>>(), owner_of),
                false,
            );
            out.push('}');
            out
        }),
        false,
    );
    field(
        &mut out,
        "directories",
        &array(&value.directories, |directory| {
            format!("{{\"path\":{}}}", quoted(directory.path.as_str()))
        }),
        false,
    );
    field(
        &mut out,
        "ledger_before",
        &image(&value.ledger_before),
        false,
    );
    field(&mut out, "ledger_after", &image(&value.ledger_after), false);
    field(&mut out, "outcome", &quoted(value.outcome.label()), false);
    field(
        &mut out,
        "post_commit",
        &array(&value.post_commit, |effect| {
            let mut out = String::from("{");
            field(&mut out, "id", &quoted(&effect.id.to_hex()), true);
            field(&mut out, "effect", &post_commit(&effect.effect), false);
            field(&mut out, "state", &effect_state(&effect.state), false);
            out.push('}');
            out
        }),
        false,
    );
    out.push('}');
    out
}

fn error(value: &ErrorReport) -> String {
    let mut out = String::from("{");
    field(&mut out, "code", &quoted(value.code.label()), true);
    field(&mut out, "message", &quoted(&value.message), false);
    field(&mut out, "paths", &array(&value.paths, target), false);
    out.push('}');
    out
}

// ---------------------------------------------------------------------------
// Primitives
// ---------------------------------------------------------------------------

fn digest(value: &ObjectId) -> String {
    quoted(&value.to_hex())
}

fn field(out: &mut String, name: &str, value: &str, first: bool) {
    if !first {
        out.push(',');
    }
    out.push('"');
    out.push_str(name);
    out.push_str("\":");
    out.push_str(value);
}

fn variant(name: &str, value: &str) -> String {
    format!("{{{}:{value}}}", quoted(name))
}

fn option<T>(value: Option<&T>, encode: impl FnOnce(&T) -> String) -> String {
    value.map(encode).unwrap_or_else(|| "null".to_string())
}

fn array<T>(values: &[T], mut encode: impl FnMut(&T) -> String) -> String {
    let mut out = String::from("[");
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&encode(value));
    }
    out.push(']');
    out
}

/// Escape only what JSON requires, and preserve every other byte of UTF-8.
fn quoted(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::{CommandEnvelope, ErrorCode, ErrorReport};
    use crate::prepare::tests::{change_with, create};
    use crate::report::Report;

    fn preview(bodies: Vec<(crate::prepare::FileOp, Vec<u8>)>) -> String {
        envelope(&CommandEnvelope::preview(
            Report::of(&change_with(bodies)).unwrap(),
        ))
    }

    #[test]
    fn one_object_on_one_line() {
        let json = preview(vec![create("pom.xml", b"<project/>")]);
        assert!(json.starts_with('{'), "{json}");
        assert!(json.ends_with("}\n"), "{json}");
        assert_eq!(json.matches('\n').count(), 1);
    }

    /// A `u64` as a JSON number loses precision above 2^53 in every
    /// JavaScript consumer, so a byte length is a decimal string.
    #[test]
    fn byte_lengths_are_decimal_strings_and_modes_are_numbers() {
        let json = preview(vec![create("pom.xml", b"<project/>")]);
        assert!(json.contains("\"bytes\":\"10\""), "{json}");
        assert!(json.contains("\"mode\":420"), "{json}");
    }

    /// Absent-versus-null must not depend on a serialiser setting.
    #[test]
    fn every_declared_field_is_emitted_even_when_empty() {
        let json = preview(Vec::new());
        for name in [
            "\"schema\":",
            "\"status\":",
            "\"exit_code\":",
            "\"project_commit\":",
            "\"recovery\":[]",
            "\"report\":",
            "\"receipt\":null",
            "\"error\":null",
        ] {
            assert!(json.contains(name), "{name} missing from {json}");
        }
    }

    #[test]
    fn a_unit_variant_is_a_string_and_a_struct_variant_is_a_one_key_object() {
        let json = preview(vec![create("pom.xml", b"<project/>")]);
        assert!(json.contains("\"kind\":\"apply\""), "{json}");
        assert!(json.contains("\"before\":\"absent\""), "{json}");
        assert!(json.contains("{\"project\":\"pom.xml\"}"), "{json}");
    }

    /// A `tag` key inside the payload collides the first time a variant has a
    /// field called `tag`.
    #[test]
    fn no_internal_tag_key_appears_anywhere() {
        let json = preview(vec![create("pom.xml", b"<project/>")]);
        assert!(!json.contains("\"tag\":"), "{json}");
        assert!(!json.contains("\"content\":"), "{json}");
    }

    #[test]
    fn an_error_envelope_carries_its_code_and_no_receipt() {
        let json = envelope(&CommandEnvelope::refused(ErrorReport::new(
            ErrorCode::CorruptMachineState,
            "the ledger did not parse",
        )));
        assert!(
            json.contains("\"code\":\"corrupt-machine-state\""),
            "{json}"
        );
        assert!(json.contains("\"receipt\":null"), "{json}");
        assert!(json.contains("\"exit_code\":1"), "{json}");
    }

    /// Machine state being retired renders as its exact `.jails/...` spelling
    /// and says what it is.
    #[test]
    fn a_legacy_target_renders_as_its_exact_spelling() {
        let mut report = Report::of(&change_with(Vec::new())).unwrap();
        report.operations.push(crate::report::ReportedOp {
            kind: crate::report::ReportedOpKind::Delete,
            path: OperationTarget::LegacyMachine(
                jails_protocol::snapshot::LegacySourcePath::GlobalFiles,
            ),
            before: None,
            after: None,
            bytes: None,
            mode: None,
            contributors: Default::default(),
        });
        let json = envelope(&CommandEnvelope::preview(report));
        assert!(
            json.contains("{\"legacy-machine\":\".jails/files\"}"),
            "{json}"
        );
    }

    #[test]
    fn control_characters_are_escaped_and_other_utf8_is_preserved() {
        assert_eq!(quoted("a\u{1}b"), "\"a\\u0001b\"");
        assert_eq!(quoted("héllo"), "\"héllo\"");
        assert_eq!(quoted("say \"hi\""), "\"say \\\"hi\\\"\"");
    }

    /// Two identical values must encode identically, or a golden test proves
    /// nothing.
    #[test]
    fn the_encoding_is_stable_across_runs() {
        let one = preview(vec![create("pom.xml", b"<project/>")]);
        let two = preview(vec![create("pom.xml", b"<project/>")]);
        assert_eq!(one, two);
    }
}
