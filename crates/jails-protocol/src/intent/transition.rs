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
#[derive(Clone, Copy, Debug, Eq, PartialEq, jails_codec_derive::Codec)]
pub struct ReceiptGuard {
    pub transaction: TransactionId,
    pub generation: u64,
    pub record_checksum: ObjectId,
}

/// The operation a frozen conflict came from.
#[derive(Clone, Copy, Debug, Eq, PartialEq, jails_codec_derive::Codec)]
pub struct ConflictOrigin {
    pub operation: OperationId,
    pub transaction: TransactionId,
    pub generation: u64,
    pub receipt: ReceiptGuard,
    pub pending: PendingIdentity,
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
                return Err(format!("{} is resolved twice", resolution.path).into());
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
                return Err(format!("{} is restored twice", restore.path).into());
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
#[derive(Clone, Copy, Debug, Eq, PartialEq, jails_codec_derive::Codec)]
pub enum EffectResumeReason {
    #[codec(tag = 0)]
    Interrupted,
    #[codec(tag = 1)]
    ExplicitRetry,
}

/// Re-run one committed operation's post-commit effect.
///
/// `expected_state` is the guard, and the reason this is not simply "run the
/// effect again": an effect that has since succeeded must not be retried, and
/// the only way to know is to say what state this plan was made against.
#[derive(Clone, Debug, Eq, PartialEq, jails_codec_derive::Codec)]
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
            // **The message names no verb, because there is none to name.**
            // `jails continue` and `jails abort` do not exist:
            // `PendingIdentity`, `ResolutionIdentity` and `RestoreIdentity` are
            // here with no route behind them (research.md §3.3), so naming
            // either sends a reader to "unrecognized subcommand". A `fix:` line
            // that
            // refuses is worse than none: it leaves the reader unable to tell
            // which of jails' answers to believe. Say what is true instead,
            // and say that finishing it forward is a gap rather than
            // something they typed wrongly.
            (Self::Apply(_), LoadedProject::Pending(_)) => Err(jails_support::Failure::Told(
                "this project has a frozen conflict, and jails cannot finish one yet -- \
                 the resolve verbs are not built.\n       fix: move the marked files aside \
                 and run the command again. That leaves the project in a state jails can \
                 plan against; your own version control is where the previous one lives."
                    .to_string(),
            )),
            (Self::Finalise(_) | Self::Abort(_), LoadedProject::Ready(_)) => Err(
                jails_support::Failure::Told("there is no conflict to finish here.".to_string()),
            ),
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
            lifecycles: vec![],
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
