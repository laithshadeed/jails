//! Authenticated export and import of an already prepared transaction.

use super::*;
use jails_protocol::snapshot::CanonicalRoot;
use jails_support::codec::{self, Encoder};
use serde::{Deserialize, Serialize};

const SCHEMA: &str = "jails.prepared-plan.v1";
const PROTOCOL_VERSION: u16 = 1;

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PreparedPlanWire {
    schema: String,
    project_root_digest: String,
    observed_generation: u64,
    prepared_after: String,
    protocol_version: u16,
    tool_version: String,
    payload_encoding: String,
    prepared_change: String,
    template_digests: Vec<String>,
    minimum_verification: String,
    environment_refs: Vec<String>,
    plan_digest: String,
}

pub(super) fn encode(bundle: &pipeline::PreparedBundle) -> Result<Vec<u8>> {
    let payload = bundle.change.portable_bytes()?;
    let root = root_digest(&bundle.root);
    let prepared_after = jails_prepare::prepared_after::digest(&bundle.root, &bundle.change)?;
    let observed_generation = bundle
        .change
        .operation_identity
        .proposed_generation
        .saturating_sub(1);
    let plan_digest = plan_digest(
        root,
        observed_generation,
        prepared_after,
        PROTOCOL_VERSION,
        env!("CARGO_PKG_VERSION"),
        &payload,
    )?;
    let wire = PreparedPlanWire {
        schema: SCHEMA.to_string(),
        project_root_digest: root.to_hex(),
        observed_generation,
        prepared_after: prepared_after.to_hex(),
        protocol_version: PROTOCOL_VERSION,
        tool_version: env!("CARGO_PKG_VERSION").to_string(),
        payload_encoding: "canonical-hex-v1".to_string(),
        prepared_change: codec::hex_bytes(&payload),
        template_digests: Vec::new(),
        minimum_verification: "fast".to_string(),
        environment_refs: Vec::new(),
        plan_digest: plan_digest.to_hex(),
    };
    let mut bytes = serde_json::to_vec_pretty(&wire)
        .map_err(|error| format!("failed to encode prepared plan: {error}"))?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn decode(bytes: &[u8], current_root: CanonicalRoot) -> Result<pipeline::PreparedBundle> {
    let wire: PreparedPlanWire = serde_json::from_slice(bytes)
        .map_err(|error| format!("prepared plan is not valid JSON: {error}"))?;
    if wire.schema != SCHEMA {
        return Err(format!(
            "prepared plan schema `{}` is unsupported; expected `{SCHEMA}`.\n       \
             fix: re-export the plan with this jails version.",
            wire.schema
        )
        .into());
    }
    if wire.protocol_version != PROTOCOL_VERSION {
        return Err(format!(
            "prepared plan protocol {} is not supported by protocol {PROTOCOL_VERSION}.\n       \
             fix: re-export the plan with this jails version.",
            wire.protocol_version
        )
        .into());
    }
    if wire.tool_version != env!("CARGO_PKG_VERSION") {
        return Err(format!(
            "prepared plan was made by jails {}, but this is jails {}.\n       fix: re-export the plan with this jails version.",
            wire.tool_version,
            env!("CARGO_PKG_VERSION")
        )
        .into());
    }
    if wire.payload_encoding != "canonical-hex-v1" {
        return Err(format!(
            "prepared plan payload encoding `{}` is unsupported.\n       \
             fix: re-export the plan with this jails version.",
            wire.payload_encoding
        )
        .into());
    }
    if wire.minimum_verification != "fast" {
        return Err(format!(
            "prepared plan verification policy `{}` is unsupported.\n       \
             fix: export a plan using the supported fast verification policy.",
            wire.minimum_verification
        )
        .into());
    }
    if !wire.environment_refs.is_empty() {
        return Err(concat!(
            "prepared plans with environment references are not supported yet.\n       ",
            "fix: export a file-only plan without external effects."
        )
        .into());
    }
    let expected_root = ObjectId::parse_hex(&wire.project_root_digest)?;
    let actual_root = root_digest(&current_root);
    if expected_root != actual_root {
        return Err(
            concat!(
                "prepared plan belongs to a different canonical project root; nothing was written.\n       ",
                "fix: apply it from the exact project root where it was exported."
            )
            .into(),
        );
    }
    let payload = codec::unhex_bytes(&wire.prepared_change)?;
    let expected_after = ObjectId::parse_hex(&wire.prepared_after)?;
    let expected_plan = ObjectId::parse_hex(&wire.plan_digest)?;
    let actual_plan = plan_digest(
        expected_root,
        wire.observed_generation,
        expected_after,
        wire.protocol_version,
        &wire.tool_version,
        &payload,
    )?;
    if expected_plan != actual_plan {
        return Err(concat!(
            "prepared plan digest does not match its fields.\n       ",
            "fix: discard the altered plan and export it again."
        )
        .into());
    }
    let change = jails_prepare::prepare::PreparedChange::from_portable_bytes(&payload)?;
    let actual_after = jails_prepare::prepared_after::digest(&current_root, &change)?;
    if expected_after != actual_after {
        return Err(concat!(
            "prepared plan after-state digest does not match its transaction.\n       ",
            "fix: discard the altered plan and export it again."
        )
        .into());
    }
    if change
        .operation_identity
        .proposed_generation
        .saturating_sub(1)
        != wire.observed_generation
    {
        return Err(concat!(
            "prepared plan generation does not match its transaction.\n       ",
            "fix: discard the corrupt plan and export it again."
        )
        .into());
    }
    let review = jails_prepare::review::PreparedReview::for_portable_plan(
        std::path::Path::new(current_root.as_str()),
        &change,
    )?;
    Ok(pipeline::PreparedBundle {
        change,
        root: current_root,
        review,
    })
}

fn root_digest(root: &CanonicalRoot) -> ObjectId {
    ObjectId::from_bytes(codec::domain_hash(
        "JAILS-PREPARED-PLAN-ROOT-1",
        root.as_str().as_bytes(),
    ))
}

fn plan_digest(
    root: ObjectId,
    generation: u64,
    prepared_after: ObjectId,
    protocol: u16,
    tool: &str,
    payload: &[u8],
) -> Result<ObjectId> {
    let mut encoder = Encoder::new();
    encoder.string(SCHEMA)?;
    root.encode(&mut encoder)?;
    encoder.u64(generation);
    prepared_after.encode(&mut encoder)?;
    encoder.u32(u32::from(protocol));
    encoder.string(tool)?;
    encoder.string("canonical-hex-v1")?;
    encoder.object(payload, codec::DEFAULT_MAX_OBJECT_BYTES)?;
    encoder.count(0)?;
    encoder.string("fast")?;
    encoder.count(0)?;
    Ok(ObjectId::from_bytes(codec::domain_hash(
        "JAILS-PREPARED-PLAN-1",
        &encoder.finish()?,
    )))
}

/// Apply one decoded plan through the normal lock, stale checks, journal and
/// receipt path. No request syntax is available here, so importing cannot
/// accidentally replan the original command.
pub fn apply_plan(run: &Run<'_>, bytes: &[u8]) -> Result<Outcome> {
    if !run.writes() {
        return Err(concat!(
            "--plan-in applies a prepared plan and cannot be combined with --pretend.\n       ",
            "fix: remove --pretend and apply the already reviewed plan."
        )
        .into());
    }
    let root = capture::canonical_root(run.project().root())?;
    let bundle = decode(bytes, root)?;
    let request_fingerprint = bundle
        .change
        .operation_identity
        .invocation
        .as_ref()
        .ok_or("prepared plan omitted its canonical invocation fingerprint")?
        .request_syntax;
    let review = bundle.review.clone();
    let project = run.project();
    let (locked, result) = run.measure(jails_prepare::timing::TimingPhase::Commit, || {
        let handle = ProjectHandle::at(project.root())?;
        let locked =
            LockedProject::acquire(handle, "apply prepared plan").map_err(commit::describe)?;
        let result = execute::commit(&locked, &bundle).map_err(commit::describe)?;
        Ok::<_, jails_support::Failure>((locked, result))
    })?;
    drop(locked);
    Ok(Outcome::Committed(
        commit::reconciled(run, result)?,
        Box::new(review),
        run.timing_trace(),
        request_fingerprint,
    ))
}
