//! What a run does to the project, as a closed set.
//!
//! §R2.3 gives exactly two transitions and three commit plans. `Apply` may
//! only run on a project with no frozen conflict, and `Finalise`/`Abort` only
//! on one that has one — *"every other pairing is an internal invariant
//! error"*. [`CommitPlan::for_bootstrap`] is where that is enforced, so no
//! caller has to remember it.

use crate::Result;
use crate::bootstrap::LoadedProject;
use crate::change::{decode_all, encode_all};
use crate::conflict::{PendingIdentity, ResolutionIdentity, RestoreIdentity};
use crate::effect::{DeferredEffectIntent, EffectId, EffectState, PostCommitEffect};
use crate::identity::{ObjectId, OperationId, TransactionId};
use crate::plan::DesiredChangeSet;
use crate::request::InvocationFingerprint;
use jails_support::codec::{Codec, Decoder, Encoder};
use std::collections::BTreeSet;

// ---------------------------------------------------------------------------
// Transitions
// ---------------------------------------------------------------------------

/// The receipt state a resume must find unchanged before it acts.
///
/// All three fields together: a transaction id alone would match a receipt
/// that has since been rewritten, and a generation alone would match a
/// different transaction at the same generation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReceiptGuard {
    pub transaction: TransactionId,
    pub generation: u64,
    pub record_checksum: ObjectId,
}

impl Codec for ReceiptGuard {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.transaction.encode(encoder)?;
        encoder.u64(self.generation);
        self.record_checksum.encode(encoder)?;
        Ok(())
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(Self {
            transaction: TransactionId::decode(decoder)?,
            generation: decoder.u64()?,
            record_checksum: ObjectId::decode(decoder)?,
        })
    }
}

/// The operation a frozen conflict came from.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConflictOrigin {
    pub operation: OperationId,
    pub transaction: TransactionId,
    pub generation: u64,
    pub receipt: ReceiptGuard,
    pub pending: PendingIdentity,
}

impl Codec for ConflictOrigin {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.operation.encode(encoder)?;
        self.transaction.encode(encoder)?;
        encoder.u64(self.generation);
        self.receipt.encode(encoder)?;
        self.pending.encode(encoder)?;
        Ok(())
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(Self {
            operation: OperationId::decode(decoder)?,
            transaction: TransactionId::decode(decoder)?,
            generation: decoder.u64()?,
            receipt: ReceiptGuard::decode(decoder)?,
            pending: PendingIdentity::decode(decoder)?,
        })
    }
}

/// Finish what the conflicted operation started, with the human's resolutions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FinalisationPlan {
    pub origin: ConflictOrigin,
    pub resolutions: Vec<ResolutionIdentity>,
    pub effect_intents: Vec<DeferredEffectIntent>,
}

impl FinalisationPlan {
    /// One resolution per path. Two would make the last one win silently, over
    /// a file the user hand-edited.
    pub fn validate(&self) -> Result<()> {
        let mut paths = BTreeSet::new();
        for resolution in &self.resolutions {
            if !paths.insert(&resolution.path) {
                return Err(format!("{} is resolved twice", resolution.path));
            }
        }
        Ok(())
    }
}
impl Codec for FinalisationPlan {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.validate()?;
        self.origin.encode(encoder)?;
        encode_all(encoder, &self.resolutions, ResolutionIdentity::encode)?;
        encode_all(encoder, &self.effect_intents, DeferredEffectIntent::encode)
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        let plan = Self {
            origin: ConflictOrigin::decode(decoder)?,
            resolutions: decode_all(decoder, ResolutionIdentity::decode)?,
            effect_intents: decode_all(decoder, DeferredEffectIntent::decode)?,
        };
        plan.validate()?;
        Ok(plan)
    }
}

/// Put the project back the way it was before the conflicted operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AbortPlan {
    pub origin: ConflictOrigin,
    pub restores: Vec<RestoreIdentity>,
}

impl AbortPlan {
    pub fn validate(&self) -> Result<()> {
        let mut paths = BTreeSet::new();
        for restore in &self.restores {
            if !paths.insert(&restore.path) {
                return Err(format!("{} is restored twice", restore.path));
            }
        }
        Ok(())
    }
}
impl Codec for AbortPlan {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.validate()?;
        self.origin.encode(encoder)?;
        encode_all(encoder, &self.restores, RestoreIdentity::encode)
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        let plan = Self {
            origin: ConflictOrigin::decode(decoder)?,
            restores: decode_all(decoder, RestoreIdentity::decode)?,
        };
        plan.validate()?;
        Ok(plan)
    }
}

/// Why an effect is being run again.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EffectResumeReason {
    Interrupted,
    ExplicitRetry,
}

impl EffectResumeReason {
    fn tag(self) -> u8 {
        match self {
            Self::Interrupted => 0,
            Self::ExplicitRetry => 1,
        }
    }

    fn from_tag(tag: u8) -> Result<Self> {
        Ok(match tag {
            0 => Self::Interrupted,
            1 => Self::ExplicitRetry,
            other => Err(format!("unknown effect resume reason tag {other}"))?,
        })
    }
}

/// Re-run one committed operation's post-commit effect.
///
/// `expected_state` is the guard, and the reason this is not simply "run the
/// effect again": an effect that has since succeeded must not be retried, and
/// the only way to know is to say what state this plan was made against.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EffectRetryPlan {
    pub invocation: InvocationFingerprint,
    pub receipt: ReceiptGuard,
    pub operation: OperationId,
    pub effect_index: u32,
    pub effect_id: EffectId,
    pub effect: PostCommitEffect,
    pub expected_state: EffectState,
    pub reason: EffectResumeReason,
}

impl Codec for EffectRetryPlan {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.invocation.encode(encoder)?;
        self.receipt.encode(encoder)?;
        self.operation.encode(encoder)?;
        encoder.u32(self.effect_index);
        self.effect_id.encode(encoder)?;
        self.effect.encode(encoder)?;
        self.expected_state.encode(encoder)?;
        encoder.tag(self.reason.tag());
        Ok(())
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(Self {
            invocation: InvocationFingerprint::decode(decoder)?,
            receipt: ReceiptGuard::decode(decoder)?,
            operation: OperationId::decode(decoder)?,
            effect_index: decoder.u32()?,
            effect_id: EffectId::decode(decoder)?,
            effect: PostCommitEffect::decode(decoder)?,
            expected_state: EffectState::decode(decoder)?,
            reason: EffectResumeReason::from_tag(decoder.tag()?)?,
        })
    }
}

/// What a run does to the project.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommitPlan {
    Apply(DesiredChangeSet),
    Finalise(FinalisationPlan),
    Abort(AbortPlan),
}

impl CommitPlan {
    /// The pairing §R2.3 calls an internal invariant, checked once here.
    ///
    /// A project with a frozen conflict cannot take new work — the tree it
    /// would plan against contains conflict markers — and a project without
    /// one has nothing to finalise or abort. Making the bootstrap prove it
    /// means no caller can construct the impossible pairing.
    pub fn for_bootstrap(self, project: &LoadedProject) -> Result<Self> {
        match (&self, project) {
            (Self::Apply(_), LoadedProject::Ready(_)) => Ok(self),
            (Self::Finalise(_) | Self::Abort(_), LoadedProject::Pending(_)) => Ok(self),
            (Self::Apply(_), LoadedProject::Pending(_)) => Err(
                "this project has a frozen conflict.\n       fix: resolve the marked files, \
                 then `jails continue` — or `jails abort` to put them back."
                    .to_string(),
            ),
            (Self::Finalise(_) | Self::Abort(_), LoadedProject::Ready(_)) => {
                Err("there is no conflict to finish here.".to_string())
            }
        }
    }
}

/// The closed set of things a resolved invocation becomes.
///
/// Nothing constructs one: the routes pass `CommitPlan` and `EffectRetryPlan`
/// separately rather than through the sum. `pending.md` §7.2 surfaced it, and
/// §6.2 is where the sum belongs once one request object exists.
///
/// Both arms are boxed. Each carries a few hundred bytes of plan, exactly one
/// exists per run, and an unboxed enum would size every one of them for the
/// larger arm — for no gain, since nothing here is on a hot path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PlannedTransition {
    Commit(Box<CommitPlan>),
    RetryEffect(Box<EffectRetryPlan>),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bootstrap::Bootstrap;
    use crate::change::tests::path;
    use crate::change::{DesiredChange, MaintenanceAttribution};
    use crate::envelope::{LedgerV2, PendingMarker};
    use crate::plan::PlannedSubject;
    use crate::plan::tests::change_set;
    use crate::request::CanonicalRequestSyntaxV1;
    use crate::snapshot::{CanonicalRoot, MachineRootPresence};
    use jails_support::codec::sha256;
    use std::collections::BTreeMap;

    fn ledger(pending: Option<PendingMarker>) -> LedgerV2 {
        LedgerV2 {
            written_by: "0.1.0".to_string(),
            generation: 3,
            last_operation: None,
            applied: Vec::new(),
            one_shots: Vec::new(),
            resources: Vec::new(),
            outputs: Vec::new(),
            pending_conflict: pending,
        }
    }

    fn marker() -> PendingMarker {
        PendingMarker {
            operation: OperationId::from_bytes(sha256(b"op")),
            generation: 3,
            request_syntax: CanonicalRequestSyntaxV1 {
                command_path: vec!["app".to_string(), "apply".to_string()],
                positionals: Vec::new(),
                options: BTreeMap::new(),
                flags: BTreeSet::new(),
            }
            .fingerprint()
            .unwrap(),
            resume_display: "jails app apply".to_string(),
        }
    }

    fn loaded(pending: bool) -> crate::bootstrap::LoadedProject {
        Bootstrap::begin(
            CanonicalRoot::new("/srv/demo").unwrap(),
            MachineRootPresence::Present,
        )
        .with_ledger(Some(ledger(pending.then(marker))))
        .unwrap()
        .classify()
        .unwrap()
    }

    fn apply_plan() -> CommitPlan {
        CommitPlan::Apply(change_set(
            PlannedSubject::AdoptLayout,
            DesiredChange::maintenance(MaintenanceAttribution::AdoptLayout),
        ))
    }

    fn abort_plan() -> CommitPlan {
        CommitPlan::Abort(AbortPlan {
            origin: ConflictOrigin {
                operation: OperationId::from_bytes(sha256(b"op")),
                transaction: crate::identity::TransactionId::from_bytes(sha256(b"tx")),
                generation: 3,
                receipt: ReceiptGuard {
                    transaction: crate::identity::TransactionId::from_bytes(sha256(b"tx")),
                    generation: 3,
                    record_checksum: ObjectId::from_bytes(sha256(b"receipt")),
                },
                pending: PendingIdentity::from_object(ObjectId::from_bytes(sha256(b"pending"))),
            },
            restores: Vec::new(),
        })
    }

    /// The pairing §R2.3 calls an internal invariant. A project with a frozen
    /// conflict cannot take new work — planning against it would read files
    /// that contain conflict markers on purpose.
    #[test]
    fn new_work_is_refused_while_a_conflict_is_frozen() {
        let error = apply_plan().for_bootstrap(&loaded(true)).unwrap_err();
        assert!(error.contains("frozen conflict"), "{error}");
        assert!(apply_plan().for_bootstrap(&loaded(false)).is_ok());
    }

    #[test]
    fn finishing_a_conflict_is_refused_when_there_is_none() {
        let error = abort_plan().for_bootstrap(&loaded(false)).unwrap_err();
        assert!(error.contains("no conflict to finish"), "{error}");
        assert!(abort_plan().for_bootstrap(&loaded(true)).is_ok());
    }

    #[test]
    fn a_resolution_of_one_path_twice_is_refused() {
        let plan = FinalisationPlan {
            origin: match abort_plan() {
                CommitPlan::Abort(abort) => abort.origin,
                _ => unreachable!(),
            },
            resolutions: vec![
                ResolutionIdentity {
                    path: path("pom.xml"),
                    resolved: crate::conflict::FileImage::Absent,
                },
                ResolutionIdentity {
                    path: path("pom.xml"),
                    resolved: crate::conflict::FileImage::Absent,
                },
            ],
            effect_intents: Vec::new(),
        };
        assert!(plan.validate().unwrap_err().contains("resolved twice"));
    }
}
