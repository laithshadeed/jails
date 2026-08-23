//! Runtime effects: the work that happens *after* a commit and cannot be
//! rolled back with it.
//!
//! ## Why these are a separate vocabulary
//!
//! plan.md scopes the transaction to "mutations inside one project root". A
//! compose service that has been started is not one of those: stopping it
//! again is a new action with its own failure modes, not an undo. So an effect
//! is recorded as a *state machine with attempts* rather than folded into the
//! file operations, and a failed effect leaves the committed project files
//! exactly as they are.
//!
//! ## Why `Failed` carries an attempt and a code
//!
//! Because "the compose reconcile did not work" is not actionable and
//! "attempt 2 exited non-zero: <summary>" is. The failure codes are closed —
//! spawn, timeout, non-zero exit, interrupted twice, protocol — so a resume
//! can decide whether retrying is even sensible rather than looping on a
//! condition that will never clear.

use crate::Result;
use crate::identity::{ObjectId, OperationId, ProjectPath, ServiceName};
use jails_support::codec::{Decoder, Encoder, ordered};
use std::collections::{BTreeMap, BTreeSet};

/// Where one effect has got to.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EffectState {
    Deferred,
    Pending {
        next_attempt: u32,
    },
    Running {
        attempt: u32,
    },
    Succeeded,
    Failed {
        attempt: u32,
        code: EffectFailureCode,
        summary: String,
    },
    /// A later operation replaced this one. `by` is absent when the superseding
    /// operation is not known — during recovery, for instance.
    Superseded {
        by: Option<OperationId>,
    },
}

/// Why an effect attempt failed, closed so a resume can reason about it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EffectFailureCode {
    Spawn,
    Timeout,
    ExitNonzero,
    InterruptedTwice,
    Protocol,
}

impl EffectFailureCode {
    fn tag(self) -> u8 {
        match self {
            Self::Spawn => 0,
            Self::Timeout => 1,
            Self::ExitNonzero => 2,
            Self::InterruptedTwice => 3,
            Self::Protocol => 4,
        }
    }

    fn from_tag(tag: u8) -> Result<Self> {
        match tag {
            0 => Ok(Self::Spawn),
            1 => Ok(Self::Timeout),
            2 => Ok(Self::ExitNonzero),
            3 => Ok(Self::InterruptedTwice),
            4 => Ok(Self::Protocol),
            other => Err(format!("unknown effect failure code {other}")),
        }
    }
}

impl EffectState {
    pub fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        match self {
            Self::Deferred => encoder.tag(0),
            Self::Pending { next_attempt } => {
                encoder.tag(1);
                nonzero(*next_attempt, "next_attempt")?;
                encoder.u32(*next_attempt);
            }
            Self::Running { attempt } => {
                encoder.tag(2);
                nonzero(*attempt, "attempt")?;
                encoder.u32(*attempt);
            }
            Self::Succeeded => encoder.tag(3),
            Self::Failed {
                attempt,
                code,
                summary,
            } => {
                encoder.tag(4);
                nonzero(*attempt, "attempt")?;
                encoder.u32(*attempt);
                encoder.tag(code.tag());
                encoder.string(summary)?;
            }
            Self::Superseded { by } => {
                encoder.tag(5);
                encoder.option(by.as_ref(), |e, id| {
                    id.encode(e);
                    Ok(())
                })?;
            }
        }
        Ok(())
    }

    pub fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(match decoder.tag()? {
            0 => Self::Deferred,
            1 => {
                let next_attempt = decoder.u32()?;
                nonzero(next_attempt, "next_attempt")?;
                Self::Pending { next_attempt }
            }
            2 => {
                let attempt = decoder.u32()?;
                nonzero(attempt, "attempt")?;
                Self::Running { attempt }
            }
            3 => Self::Succeeded,
            4 => {
                let attempt = decoder.u32()?;
                nonzero(attempt, "attempt")?;
                Self::Failed {
                    attempt,
                    code: EffectFailureCode::from_tag(decoder.tag()?)?,
                    summary: decoder.string()?,
                }
            }
            5 => Self::Superseded {
                by: decoder.option(OperationId::decode)?,
            },
            other => Err(format!("unknown effect state tag {other}"))?,
        })
    }
}

/// An attempt counter starts at one. Zero would make "never attempted" and
/// "attempted once" the same recorded value, which is the distinction a resume
/// has to make.
fn nonzero(value: u32, what: &str) -> Result<()> {
    if value == 0 {
        return Err(format!(
            "{what} is zero; attempts are counted from one so `never attempted` and \
             `attempted once` stay distinguishable"
        ));
    }
    Ok(())
}

/// One effect's retry and idempotency key.
///
/// Distinct from the operation that scheduled it: a retry after a crash must
/// address *this* effect, and an operation may carry several.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct EffectId(ObjectId);

impl EffectId {
    pub fn from_object(id: ObjectId) -> Self {
        Self(id)
    }

    pub fn object(&self) -> ObjectId {
        self.0
    }

    pub fn to_hex(&self) -> String {
        self.0.to_hex()
    }

    pub fn encode(&self, encoder: &mut Encoder) {
        self.0.encode(encoder);
    }

    pub fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(Self(ObjectId::decode(decoder)?))
    }
}

impl std::fmt::Display for EffectId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// The runtime work a committed transaction asks for.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PostCommitEffect {
    ComposeReconcile {
        compose_output: ProjectPath,
        before_document: Option<ObjectId>,
        after_document: Option<ObjectId>,
        prior_managed_services: BTreeMap<ServiceName, ObjectId>,
        desired_services: BTreeMap<ServiceName, ObjectId>,
        stop_services: BTreeSet<ServiceName>,
    },
}

impl PostCommitEffect {
    pub fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        match self {
            Self::ComposeReconcile {
                compose_output,
                before_document,
                after_document,
                prior_managed_services,
                desired_services,
                stop_services,
            } => {
                encoder.tag(0);
                compose_output.encode(encoder)?;
                encode_optional_object(encoder, before_document.as_ref())?;
                encode_optional_object(encoder, after_document.as_ref())?;
                encode_service_map(encoder, prior_managed_services)?;
                encode_service_map(encoder, desired_services)?;
                encode_service_set(encoder, stop_services)?;
            }
        }
        Ok(())
    }

    pub fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        match decoder.tag()? {
            0 => Ok(Self::ComposeReconcile {
                compose_output: ProjectPath::decode(decoder)?,
                before_document: decoder.option(ObjectId::decode)?,
                after_document: decoder.option(ObjectId::decode)?,
                prior_managed_services: decode_service_map(decoder)?,
                desired_services: decode_service_map(decoder)?,
                stop_services: decode_service_set(decoder)?,
            }),
            other => Err(format!("unknown post-commit effect tag {other}")),
        }
    }
}

/// The same work, before a transaction has been prepared to carry it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DeferredEffectIntent {
    ComposeReconcile {
        before_document: Option<ObjectId>,
        compose_output: ProjectPath,
        prior_managed_services: BTreeMap<ServiceName, ObjectId>,
        desired_services: BTreeMap<ServiceName, ObjectId>,
    },
}

impl DeferredEffectIntent {
    pub fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        match self {
            Self::ComposeReconcile {
                before_document,
                compose_output,
                prior_managed_services,
                desired_services,
            } => {
                encoder.tag(0);
                encode_optional_object(encoder, before_document.as_ref())?;
                compose_output.encode(encoder)?;
                encode_service_map(encoder, prior_managed_services)?;
                encode_service_map(encoder, desired_services)?;
            }
        }
        Ok(())
    }

    pub fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        match decoder.tag()? {
            0 => Ok(Self::ComposeReconcile {
                before_document: decoder.option(ObjectId::decode)?,
                compose_output: ProjectPath::decode(decoder)?,
                prior_managed_services: decode_service_map(decoder)?,
                desired_services: decode_service_map(decoder)?,
            }),
            other => Err(format!("unknown deferred effect tag {other}")),
        }
    }
}

fn encode_optional_object(encoder: &mut Encoder, value: Option<&ObjectId>) -> Result<()> {
    encoder.option(value, |e, id| {
        id.encode(e);
        Ok(())
    })
}

fn encode_service_map(
    encoder: &mut Encoder,
    services: &BTreeMap<ServiceName, ObjectId>,
) -> Result<()> {
    encoder.count(services.len())?;
    let mut previous: Option<&ServiceName> = None;
    for (name, object) in services {
        ordered(previous, name)?;
        previous = Some(name);
        name.encode(encoder)?;
        object.encode(encoder);
    }
    Ok(())
}

fn decode_service_map(decoder: &mut Decoder<'_>) -> Result<BTreeMap<ServiceName, ObjectId>> {
    let count = decoder.count()?;
    let mut out = BTreeMap::new();
    let mut previous: Option<ServiceName> = None;
    for _ in 0..count {
        let name = ServiceName::decode(decoder)?;
        ordered(previous.as_ref(), &name)?;
        previous = Some(name.clone());
        out.insert(name, ObjectId::decode(decoder)?);
    }
    Ok(out)
}

fn encode_service_set(encoder: &mut Encoder, services: &BTreeSet<ServiceName>) -> Result<()> {
    encoder.count(services.len())?;
    let mut previous: Option<&ServiceName> = None;
    for name in services {
        ordered(previous, name)?;
        previous = Some(name);
        name.encode(encoder)?;
    }
    Ok(())
}

fn decode_service_set(decoder: &mut Decoder<'_>) -> Result<BTreeSet<ServiceName>> {
    let count = decoder.count()?;
    let mut out = BTreeSet::new();
    let mut previous: Option<ServiceName> = None;
    for _ in 0..count {
        let name = ServiceName::decode(decoder)?;
        ordered(previous.as_ref(), &name)?;
        previous = Some(name.clone());
        out.insert(name);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use jails_support::codec::sha256;

    fn service(name: &str) -> ServiceName {
        ServiceName::parse(name).unwrap()
    }

    fn object(seed: &str) -> ObjectId {
        ObjectId::from_bytes(sha256(seed.as_bytes()))
    }

    #[test]
    fn every_effect_state_round_trips() {
        for state in [
            EffectState::Deferred,
            EffectState::Pending { next_attempt: 1 },
            EffectState::Running { attempt: 3 },
            EffectState::Succeeded,
            EffectState::Failed {
                attempt: 2,
                code: EffectFailureCode::Timeout,
                summary: "compose up timed out after 120s".to_string(),
            },
            EffectState::Superseded { by: None },
            EffectState::Superseded {
                by: Some(OperationId::from_bytes(sha256(b"op"))),
            },
        ] {
            let mut encoder = Encoder::new();
            state.encode(&mut encoder).unwrap();
            let bytes = encoder.finish().unwrap();
            let mut decoder = Decoder::new(&bytes).unwrap();
            assert_eq!(EffectState::decode(&mut decoder).unwrap(), state);
            decoder.finish().unwrap();
        }
    }

    /// Zero would make "never attempted" and "attempted once" the same
    /// recorded value, and that is exactly the distinction a resume makes.
    #[test]
    fn an_attempt_counter_starts_at_one() {
        let mut encoder = Encoder::new();
        let error = EffectState::Running { attempt: 0 }
            .encode(&mut encoder)
            .unwrap_err();
        assert!(error.contains("counted from one"), "{error}");

        // And a decoder refuses it too, so a hand-edited record cannot smuggle
        // one in.
        let mut encoder = Encoder::new();
        encoder.tag(2);
        encoder.u32(0);
        let bytes = encoder.finish().unwrap();
        let mut decoder = Decoder::new(&bytes).unwrap();
        assert!(EffectState::decode(&mut decoder).is_err());
    }

    #[test]
    fn every_failure_code_round_trips_and_an_unknown_one_rejects() {
        for code in [
            EffectFailureCode::Spawn,
            EffectFailureCode::Timeout,
            EffectFailureCode::ExitNonzero,
            EffectFailureCode::InterruptedTwice,
            EffectFailureCode::Protocol,
        ] {
            assert_eq!(EffectFailureCode::from_tag(code.tag()).unwrap(), code);
        }
        assert!(
            EffectFailureCode::from_tag(5)
                .unwrap_err()
                .contains("unknown effect failure code")
        );
    }

    #[test]
    fn a_compose_effect_round_trips_with_its_service_maps() {
        let effect = PostCommitEffect::ComposeReconcile {
            compose_output: ProjectPath::parse("compose.yaml").unwrap(),
            before_document: Some(object("before")),
            after_document: None,
            prior_managed_services: BTreeMap::from([
                (service("kafka"), object("kafka-old")),
                (service("postgres"), object("pg-old")),
            ]),
            desired_services: BTreeMap::from([(service("postgres"), object("pg-new"))]),
            stop_services: BTreeSet::from([service("kafka")]),
        };
        let mut encoder = Encoder::new();
        effect.encode(&mut encoder).unwrap();
        let bytes = encoder.finish().unwrap();
        let mut decoder = Decoder::new(&bytes).unwrap();
        assert_eq!(PostCommitEffect::decode(&mut decoder).unwrap(), effect);
        decoder.finish().unwrap();
    }

    #[test]
    fn a_deferred_intent_round_trips() {
        let intent = DeferredEffectIntent::ComposeReconcile {
            before_document: None,
            compose_output: ProjectPath::parse("compose.yaml").unwrap(),
            prior_managed_services: BTreeMap::new(),
            desired_services: BTreeMap::from([(service("postgres"), object("pg"))]),
        };
        let mut encoder = Encoder::new();
        intent.encode(&mut encoder).unwrap();
        let bytes = encoder.finish().unwrap();
        let mut decoder = Decoder::new(&bytes).unwrap();
        assert_eq!(DeferredEffectIntent::decode(&mut decoder).unwrap(), intent);
        decoder.finish().unwrap();
    }

    /// One value, one encoding — the same rule the codec applies to every set.
    #[test]
    fn an_unsorted_service_map_rejects_on_decode() {
        let mut encoder = Encoder::new();
        encoder.count(2).unwrap();
        service("postgres").encode(&mut encoder).unwrap();
        object("a").encode(&mut encoder);
        service("kafka").encode(&mut encoder).unwrap();
        object("b").encode(&mut encoder);
        let bytes = encoder.finish().unwrap();

        let mut decoder = Decoder::new(&bytes).unwrap();
        let error = decode_service_map(&mut decoder).unwrap_err();
        assert!(error.contains("not canonically ordered"), "{error}");
    }

    #[test]
    fn an_unknown_effect_tag_rejects() {
        let mut decoder = Decoder::new(&[7]).unwrap();
        assert!(
            PostCommitEffect::decode(&mut decoder)
                .unwrap_err()
                .contains("unknown post-commit effect tag")
        );
    }
}
