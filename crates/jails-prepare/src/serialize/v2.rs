//! Manual `jails.command-result.v2` encoding.

use super::*;

/// The exact `jails.command-result.v2` encoding: one compact object and one
/// trailing newline, with every declared field present in schema order.
pub fn envelope_v2(envelope: &CommandEnvelopeV2) -> String {
    envelope_v2_with_review(envelope, None)
}

pub fn envelope_v2_with_review(
    envelope: &CommandEnvelopeV2,
    review: Option<(
        &crate::review::PreparedReview,
        crate::review::ReviewSelection,
    )>,
) -> String {
    let mut out = String::new();
    out.push('{');
    field(&mut out, "schema", &quoted(SCHEMA_V2), true);
    field(
        &mut out,
        "command",
        &command_identity(&envelope.command),
        false,
    );
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
    field(
        &mut out,
        "recovery",
        &array(&envelope.recovery, recovery),
        false,
    );
    field(
        &mut out,
        "report",
        &option(envelope.report.as_ref(), |report| report_v2(report, review)),
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
        &option(envelope.error.as_ref(), error_v2),
        false,
    );
    field(
        &mut out,
        "timings",
        &array(&envelope.timings, |span| {
            let mut out = String::from("{");
            field(&mut out, "phase", &quoted(span.phase.label()), true);
            field(
                &mut out,
                "duration_micros",
                &span.duration_micros.to_string(),
                false,
            );
            out.push('}');
            out
        }),
        false,
    );
    out.push('}');
    out.push('\n');
    out
}

fn command_identity(value: &crate::command::CommandIdentity) -> String {
    let mut out = String::from("{");
    field(
        &mut out,
        "path",
        &array(&value.path, |part| quoted(part)),
        true,
    );
    field(
        &mut out,
        "fingerprint",
        &quoted(&format!("sha256:{}", value.fingerprint.to_hex())),
        false,
    );
    field(&mut out, "read_only", &value.read_only.to_string(), false);
    out.push('}');
    out
}

fn report_v2(
    value: &CommandReportV2,
    review: Option<(
        &crate::review::PreparedReview,
        crate::review::ReviewSelection,
    )>,
) -> String {
    let (kind, schema, data) = match value {
        CommandReportV2::Prepared(report) => (
            "prepared",
            "jails.prepared-report.v1",
            prepared_data(report, review),
        ),
        CommandReportV2::EffectRetry(retry) => {
            ("effect-retry", "jails.effect-retry.v1", effect_retry(retry))
        }
    };
    let mut out = String::from("{");
    field(&mut out, "kind", &quoted(kind), true);
    field(&mut out, "schema", &quoted(schema), false);
    field(&mut out, "data", &data, false);
    out.push('}');
    out
}

fn error_v2(value: &ErrorReportV2) -> String {
    let mut out = String::from("{");
    field(&mut out, "code", &quoted(value.code.label()), true);
    field(&mut out, "message", &quoted(&value.message), false);
    field(
        &mut out,
        "diagnostics",
        &array(&value.diagnostics, diagnostic),
        false,
    );
    out.push('}');
    out
}

fn diagnostic(value: &Diagnostic) -> String {
    match *value {}
}

fn prepared_data(
    value: &Report,
    review: Option<(
        &crate::review::PreparedReview,
        crate::review::ReviewSelection,
    )>,
) -> String {
    let mut out = String::from("{");
    prepared_fields(&mut out, value, true);
    if let Some((review, selection)) = review {
        review_fields(&mut out, review, selection);
    }
    out.push('}');
    out
}

fn review_fields(
    out: &mut String,
    review: &crate::review::PreparedReview,
    selection: crate::review::ReviewSelection,
) {
    if selection.diff {
        field(
            out,
            "diffs",
            &array(&review.files, |file| {
                let mut row = String::from("{");
                field(&mut row, "kind", &quoted(file.kind.label()), true);
                field(&mut row, "path", &quoted(file.path.as_str()), false);
                field(
                    &mut row,
                    "reconciliation",
                    &quoted(file.reconciliation.label()),
                    false,
                );
                field(
                    &mut row,
                    "patch",
                    &quoted(&crate::review::render_patch(file)),
                    false,
                );
                row.push('}');
                row
            }),
            false,
        );
    }
    if selection.ast {
        let mut rows = Vec::new();
        for file in &review.files {
            let kind = match (file.kind, file.reconciliation) {
                (crate::review::ReviewFileKind::Create, _) => "CreateFile",
                (crate::review::ReviewFileKind::Replace, crate::review::Reconciliation::Direct) => {
                    "ReplaceFile"
                }
                (
                    crate::review::ReviewFileKind::Replace,
                    crate::review::Reconciliation::ThreeWay,
                ) => "MergeFile",
                (crate::review::ReviewFileKind::Delete, _) => "DeleteFile",
            };
            rows.push(format!(
                "{{\"kind\":{},\"path\":{}}}",
                quoted(kind),
                quoted(file.path.as_str())
            ));
        }
        for edit in &review.edits {
            rows.push(format!(
                "{{\"kind\":{}}}",
                quoted(crate::review::semantic_kind(edit))
            ));
        }
        field(out, "ast", &format!("[{}]", rows.join(",")), false);
    }
}
