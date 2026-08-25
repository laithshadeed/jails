//! What an operation *means*, as opposed to what it does to files.
//!
//! The two are separate because they are checked against each other. A plan
//! whose file operations abort while its semantics apply is not a transition
//! anything can execute, and [`OperationSemanticsV1::agrees_with`] is where
//! that is caught — once, rather than at each of the three places a prepared
//! value can arrive from.
//!
//! The operation id is a hash of this value and *only* this value. It is
//! computed before any ID-bearing argument or byte exists, which is what lets
//! a formatter's argv reference the operation without the identity having to
//! contain its own hash.

use crate::Result;
use crate::prepare::PreparedKind;
use crate::tool::OperationContextFingerprint;
use jails_protocol::conflict::{PendingIdentity, ResolutionIdentity, RestoreIdentity};
use jails_protocol::identity::{ObjectId, OperationId, TransactionId};
use jails_protocol::plan::{LedgerIntent, PlannedSubject};
use jails_protocol::request::InvocationFingerprint;
use jails_support::codec::{self, Codec, Decoder, Encoder};

/// An apply's meaning: what is wanted, and what the store should say after.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApplySemantics {
    pub subject: PlannedSubject,
    pub ledger_intent: LedgerIntent,
}

/// What this operation *means*, as opposed to what it does to files.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OperationSemanticsV1 {
    /// Boxed: an apply carries the whole desired state and ledger intent,
    /// and a finalise or an abort would otherwise be sized for it.
    Apply(Box<ApplySemantics>),
    Finalise {
        origin: OperationId,
        origin_transaction: TransactionId,
        pending: PendingIdentity,
        resolutions: Vec<ResolutionIdentity>,
    },
    Abort {
        origin: OperationId,
        origin_transaction: TransactionId,
        restores: Vec<RestoreIdentity>,
    },
}

impl OperationSemanticsV1 {
    fn tag(&self) -> u8 {
        match self {
            Self::Apply(_) => 0,
            Self::Finalise { .. } => 1,
            Self::Abort { .. } => 2,
        }
    }

    /// The kind and the semantics must describe the same transition. They are
    /// two fields of one value, and a plan whose file operations abort while
    /// its semantics apply is not a transition anything can execute.
    pub fn agrees_with(&self, kind: &PreparedKind) -> Result<()> {
        let ok = match (self, kind) {
            (Self::Apply(_), PreparedKind::Apply | PreparedKind::Conflict { .. }) => true,
            (Self::Finalise { origin, .. }, PreparedKind::Finalise { origin: named }) => {
                origin == named
            }
            (Self::Abort { origin, .. }, PreparedKind::Abort { origin: named }) => origin == named,
            _ => false,
        };
        if !ok {
            return Err(jails_support::Failure::Told(
                "the prepared kind and its semantics describe different transitions".to_string(),
            ));
        }
        Ok(())
    }
}
impl Codec for OperationSemanticsV1 {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.tag(self.tag());
        match self {
            Self::Apply(apply) => {
                apply.subject.encode(encoder)?;
                apply.ledger_intent.encode(encoder)?;
                Ok(())
            }
            Self::Finalise {
                origin,
                origin_transaction,
                pending,
                resolutions,
            } => {
                origin.encode(encoder)?;
                origin_transaction.encode(encoder)?;
                pending.encode(encoder)?;
                encoder.count(resolutions.len())?;
                for resolution in resolutions {
                    resolution.encode(encoder)?;
                }
                Ok(())
            }
            Self::Abort {
                origin,
                origin_transaction,
                restores,
            } => {
                origin.encode(encoder)?;
                origin_transaction.encode(encoder)?;
                encoder.count(restores.len())?;
                for restore in restores {
                    restore.encode(encoder)?;
                }
                Ok(())
            }
        }
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(match decoder.tag()? {
            0 => Self::Apply(Box::new(ApplySemantics {
                subject: PlannedSubject::decode(decoder)?,
                ledger_intent: LedgerIntent::decode(decoder)?,
            })),
            1 => {
                let origin = OperationId::decode(decoder)?;
                let origin_transaction = TransactionId::decode(decoder)?;
                let pending = PendingIdentity::decode(decoder)?;
                let count = decoder.count()?;
                let mut resolutions = Vec::new();
                for _ in 0..count {
                    resolutions.push(ResolutionIdentity::decode(decoder)?);
                }
                Self::Finalise {
                    origin,
                    origin_transaction,
                    pending,
                    resolutions,
                }
            }
            2 => {
                let origin = OperationId::decode(decoder)?;
                let origin_transaction = TransactionId::decode(decoder)?;
                let count = decoder.count()?;
                let mut restores = Vec::new();
                for _ in 0..count {
                    restores.push(RestoreIdentity::decode(decoder)?);
                }
                Self::Abort {
                    origin,
                    origin_transaction,
                    restores,
                }
            }
            other => Err(format!("unknown operation semantics tag {other}"))?,
        })
    }
}

/// What the plan depended on, and what it will do about it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperationIdentityV1 {
    pub snapshot: ObjectId,
    pub operation_context: OperationContextFingerprint,
    pub invocation: Option<InvocationFingerprint>,
    pub proposed_generation: u64,
    pub semantics: OperationSemanticsV1,
}

impl OperationIdentityV1 {
    /// `SHA256("JAILS-OPERATION-1" || encode(self))`.
    ///
    /// Computed *before* any ID-bearing argument or byte exists, which is what
    /// lets a tool argv reference the operation without the identity having to
    /// contain its own hash.
    pub fn operation_id(&self) -> Result<OperationId> {
        let mut encoder = Encoder::new();
        self.encode(&mut encoder)?;
        Ok(OperationId::from_bytes(codec::domain_hash(
            "JAILS-OPERATION-1",
            &encoder.finish()?,
        )))
    }
}
impl Codec for OperationIdentityV1 {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.snapshot.encode(encoder)?;
        self.operation_context.encode(encoder)?;
        encoder.option(self.invocation.as_ref(), |e, one| one.encode(e))?;
        encoder.u64(self.proposed_generation);
        self.semantics.encode(encoder)
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(Self {
            snapshot: ObjectId::decode(decoder)?,
            operation_context: OperationContextFingerprint::decode(decoder)?,
            invocation: decoder.option(InvocationFingerprint::decode)?,
            proposed_generation: decoder.u64()?,
            semantics: OperationSemanticsV1::decode(decoder)?,
        })
    }
}
