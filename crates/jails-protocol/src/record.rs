//! What the store says was applied.
//!
//! Split out of `envelope.rs` by secret: that module is about a *file* -- its
//! magic, its schema number, its checksum -- while these three types are about
//! what a project contains. R1.4's
//! schema names them together (`applied`, `one_shots`, `outputs`) and they
//! change together, for reasons that have nothing to do with framing bytes.
//!
//! Each is a row in a canonical *set*: one per entity, one per one-shot id,
//! one per path. Two rows for one identity would let the written order decide
//! which of them a removal consults, which is why the encoders here check
//! ordering rather than trusting the caller to have sorted.

use crate::Result;
use crate::conflict::{LiveFileImage, StoredFileImage};
use crate::entity::{EntityId, EntitySpec, OneShotId, OneShotSpec, OwnerId};
use crate::identity::{OperationId, ProjectPath};
use crate::provenance::RendererStamp;
use crate::resource::{OneShotLifecycle, OneShotState, ResourceOwner};
use jails_support::codec::{Codec, Decoder, Encoder, ordered};
use std::collections::BTreeSet;

/// One applied entity: its identity, who claims it, and what was applied.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppliedEntity {
    pub id: EntityId,
    pub owners: BTreeSet<OwnerId>,
    pub version: AppliedVersion,
}

/// What was applied, and by which operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppliedVersion {
    pub spec: EntitySpec,
    pub operation: OperationId,
}

impl Codec for AppliedEntity {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.id.encode(encoder)?;
        if self.owners.is_empty() {
            // An entity with no owner is one nobody wants, and a row for it is
            // a contradiction: reconciliation would remove it on sight.
            return Err(format!(
                "{:?} is recorded with no owner, which is not a thing that can be applied",
                self.id
            ));
        }
        if !self.version.spec.matches(&self.id) {
            return Err(
                "an applied row pairs an identity and a spec of different kinds".to_string(),
            );
        }
        encoder.count(self.owners.len())?;
        let mut previous: Option<&OwnerId> = None;
        for owner in &self.owners {
            ordered(previous, owner)?;
            previous = Some(owner);
            encoder.tag(owner.tag());
        }
        self.version.spec.encode(encoder)?;
        self.version.operation.encode(encoder)?;
        Ok(())
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        let id = EntityId::decode(decoder)?;
        let count = decoder.count()?;
        let mut owners = BTreeSet::new();
        let mut previous: Option<OwnerId> = None;
        for _ in 0..count {
            let owner = OwnerId::from_tag(decoder.tag()?)?;
            ordered(previous.as_ref(), &owner)?;
            previous = Some(owner);
            owners.insert(owner);
        }
        if owners.is_empty() {
            return Err("an applied row carries no owner".to_string());
        }
        let spec = EntitySpec::decode(decoder)?;
        if !spec.matches(&id) {
            return Err(
                "an applied row pairs an identity and a spec of different kinds".to_string(),
            );
        }
        Ok(Self {
            id,
            owners,
            version: AppliedVersion {
                spec,
                operation: OperationId::decode(decoder)?,
            },
        })
    }
}

/// One applied one-shot, and whether its target is still there.
///
/// Kept even when retired. A migration that has been applied to a database
/// cannot be un-applied by deleting its record, and a receipt that vanished
/// with its target would make the same `g field` run a second time.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OneShotReceipt {
    pub id: OneShotId,
    pub spec: OneShotSpec,
    pub state: OneShotState,
    pub lifecycle: OneShotLifecycle,
    pub operation: OperationId,
}

impl Codec for OneShotReceipt {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.id.encode(encoder)?;
        self.spec.encode(encoder)?;
        self.state.encode(encoder)?;
        self.lifecycle.encode(encoder)?;
        self.operation.encode(encoder)?;
        Ok(())
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(Self {
            id: OneShotId::decode(decoder)?,
            spec: OneShotSpec::decode(decoder)?,
            state: OneShotState::decode(decoder)?,
            lifecycle: OneShotLifecycle::decode(decoder)?,
            operation: OperationId::decode(decoder)?,
        })
    }
}

/// One canonical row per path jails has written.
///
/// Three images rather than one, and that is the whole point of it: `base` is
/// what jails last rendered, `current` is what was on disk when it was last
/// looked at, and the desired bytes arrive with the next plan. A row where all
/// three differ is a merge; a row where the live image has moved away from the
/// base is a file the reader edited, and their bytes win. With only a hash of
/// the last write, jails could detect that and not repair it.
///
/// `contributors` is what keeps a shared file alive: a path stays when its
/// last owner leaves only if somebody else still claims it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutputRecord {
    pub path: ProjectPath,
    pub contributors: BTreeSet<ResourceOwner>,
    pub current: LiveFileImage,
    pub base: StoredFileImage,
    pub renderer: RendererStamp,
}

impl Codec for OutputRecord {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.path.encode(encoder)?;
        encoder.count(self.contributors.len())?;
        let mut previous: Option<&ResourceOwner> = None;
        for owner in &self.contributors {
            ordered(previous, owner)?;
            previous = Some(owner);
            owner.encode(encoder)?;
        }
        self.current.encode(encoder)?;
        self.base.encode(encoder)?;
        self.renderer.encode(encoder)
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        let path = ProjectPath::decode(decoder)?;
        let contributors: BTreeSet<ResourceOwner> = decoder.set()?;
        Ok(Self {
            path,
            contributors,
            current: LiveFileImage::decode(decoder)?,
            base: StoredFileImage::decode(decoder)?,
            renderer: RendererStamp::decode(decoder)?,
        })
    }
}
