//! A reconciliation frozen mid-flight, and the identity that pins it.
//!
//! ## Why the whole candidate is frozen rather than recomputed
//!
//! A conflicted run leaves marker files in the tree. The next run cannot
//! re-derive what it was doing, because the project *is* the uncertain thing:
//! re-reading a marker-bearing file would feed conflict markers back into the
//! plan. So the run that stopped freezes the complete logical state a
//! successful resolution will promote, and finalisation reads that instead of
//! the tree.
//!
//! ## Why `PendingIdentity` is recomputed rather than stored
//!
//! plan.md §R5.4: *"Finalisation recomputes this hash from the current ledger
//! and requires it to equal the value used in `OperationIdentityV1`; it never
//! trusts a separately stored hash."* A stored hash is a second authority for
//! what the pending record says, and the failure it permits is finalising a
//! record that has been edited under a hash that still matches.
//!
//! ## Why `resume_display` is excluded from it
//!
//! It is presentation — the sentence a person is shown. Including it would
//! make improving that sentence invalidate every pending conflict in
//! existence.
//!
//! ## Nothing calls this yet
//!
//! Closing this crate's API to `pub(crate)` (`pending.md` §7.2) made that
//! visible: with `dead_code = "deny"`, 16 items here are reachable from
//! nothing. They are `pub` for that reason and no other. This is not stale
//! code -- it is encoded, round-tripped and unit-tested -- it is `pending.md`
//! §11's "conflicted merges cannot be resumed", which lands as one piece or
//! not at all: the frozen record, the refusal while it stands, and the
//! continue/abort commands. Building only the enter side was tried and backed
//! out, so a project that can enter a conflicted state and not leave it is
//! exactly what these types must not be wired up to produce.

use crate::Result;
use crate::conflict::{
    FrozenPath, LiveFileImage, PendingConflictPath, StoredFileImage, encode_paths, pending_identity,
};
use crate::effect::DeferredEffectIntent;
use crate::entity::{OneShotId, OneShotSpec, SourceInputId};
use crate::identity::{ObjectId, OperationId, ProjectPath};
use crate::provenance::RendererStamp;
use crate::record::AppliedEntity;
use crate::request::{InvocationFingerprint, ManifestSourceId};
use crate::resource::{OneShotLifecycle, OneShotState, ResourceKey, ResourceOwner, ResourceValue};
use jails_support::codec::{Codec, Decoder, Encoder, ordered};
use std::collections::BTreeSet;

/// Which human input a frozen candidate depended on.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum DesiredInputId {
    HumanConfig,
    AppManifest(ManifestSourceId),
    DirectRequest,
    CasesBrief(SourceInputId),
}

impl DesiredInputId {
    fn tag(&self) -> u8 {
        match self {
            Self::HumanConfig => 0,
            Self::AppManifest(_) => 1,
            Self::DirectRequest => 2,
            Self::CasesBrief(_) => 3,
        }
    }
}
impl Codec for DesiredInputId {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.tag(self.tag());
        match self {
            Self::AppManifest(source) => source.encode(encoder),
            Self::CasesBrief(source) => source.encode(encoder),
            Self::HumanConfig | Self::DirectRequest => Ok(()),
        }
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(match decoder.tag()? {
            0 => Self::HumanConfig,
            1 => Self::AppManifest(ManifestSourceId::decode(decoder)?),
            2 => Self::DirectRequest,
            3 => Self::CasesBrief(SourceInputId::decode(decoder)?),
            other => Err(format!("unknown desired input tag {other}"))?,
        })
    }
}

/// What that input has to still be for the candidate to remain applicable.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum DesiredInputGuard {
    Exact {
        sha256: ObjectId,
        len: u64,
    },
    /// An input this same transaction would have produced. Guarded by its
    /// path as well, because the bytes alone would match a file somewhere
    /// else that happened to be identical.
    ProjectedTransactionOutput {
        path: ProjectPath,
        sha256: ObjectId,
        len: u64,
    },
    Absent,
}

impl DesiredInputGuard {
    fn tag(&self) -> u8 {
        match self {
            Self::Exact { .. } => 0,
            Self::ProjectedTransactionOutput { .. } => 1,
            Self::Absent => 2,
        }
    }
}
impl Codec for DesiredInputGuard {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.tag(self.tag());
        match self {
            Self::Exact { sha256, len } => {
                sha256.encode(encoder)?;
                encoder.u64(*len);
                Ok(())
            }
            Self::ProjectedTransactionOutput { path, sha256, len } => {
                path.encode(encoder)?;
                sha256.encode(encoder)?;
                encoder.u64(*len);
                Ok(())
            }
            Self::Absent => Ok(()),
        }
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(match decoder.tag()? {
            0 => Self::Exact {
                sha256: ObjectId::decode(decoder)?,
                len: decoder.u64()?,
            },
            1 => Self::ProjectedTransactionOutput {
                path: ProjectPath::decode(decoder)?,
                sha256: ObjectId::decode(decoder)?,
                len: decoder.u64()?,
            },
            2 => Self::Absent,
            other => Err(format!("unknown desired input guard tag {other}"))?,
        })
    }
}

/// One frozen input and its guard.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct FrozenDesiredInput {
    pub id: DesiredInputId,
    pub guard: DesiredInputGuard,
}

impl Codec for FrozenDesiredInput {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.id.encode(encoder)?;
        self.guard.encode(encoder)
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(Self {
            id: DesiredInputId::decode(decoder)?,
            guard: DesiredInputGuard::decode(decoder)?,
        })
    }
}

/// One output in the frozen candidate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingOutput {
    pub path: ProjectPath,
    pub contributors: BTreeSet<ResourceOwner>,
    pub current: crate::conflict::PendingCurrent,
    pub base: StoredFileImage,
    pub renderer: RendererStamp,
}

impl Codec for PendingOutput {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.path.encode(encoder)?;
        encoder.set(&self.contributors)?;
        self.current.encode(encoder)?;
        self.base.encode(encoder)?;
        self.renderer.encode(encoder)
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(Self {
            path: ProjectPath::decode(decoder)?,
            contributors: decoder.set()?,
            current: crate::conflict::PendingCurrent::decode(decoder)?,
            base: StoredFileImage::decode(decoder)?,
            renderer: RendererStamp::decode(decoder)?,
        })
    }
}

/// One resource row in the frozen candidate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingResource {
    pub key: ResourceKey,
    pub owners: BTreeSet<ResourceOwner>,
    pub value: ResourceValue,
}

impl Codec for PendingResource {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.value.agrees_with(&self.key)?;
        self.key.encode(encoder)?;
        encoder.set(&self.owners)?;
        self.value.encode(encoder)
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        let key = ResourceKey::decode(decoder)?;
        let owners = decoder.set()?;
        let value = ResourceValue::decode(decoder)?;
        value.agrees_with(&key)?;
        Ok(Self { key, owners, value })
    }
}

/// One one-shot receipt in the frozen candidate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingOneShot {
    pub id: OneShotId,
    pub spec: OneShotSpec,
    pub state: OneShotState,
    pub lifecycle: OneShotLifecycle,
}

impl Codec for PendingOneShot {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.id.encode(encoder)?;
        self.spec.encode(encoder)?;
        self.state.encode(encoder)?;
        self.lifecycle.encode(encoder)
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(Self {
            id: OneShotId::decode(decoder)?,
            spec: OneShotSpec::decode(decoder)?,
            state: OneShotState::decode(decoder)?,
            lifecycle: OneShotLifecycle::decode(decoder)?,
        })
    }
}

/// The complete logical state a successful resolution promotes.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PendingLedgerState {
    pub applied: Vec<AppliedEntity>,
    pub one_shots: Vec<PendingOneShot>,
    pub resources: Vec<PendingResource>,
    pub outputs: Vec<PendingOutput>,
}

impl Codec for PendingLedgerState {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.count(self.applied.len())?;
        for entity in &self.applied {
            entity.encode(encoder)?;
        }
        encoder.count(self.one_shots.len())?;
        for one_shot in &self.one_shots {
            one_shot.encode(encoder)?;
        }
        encoder.count(self.resources.len())?;
        for resource in &self.resources {
            resource.encode(encoder)?;
        }
        encoder.count(self.outputs.len())?;
        for output in &self.outputs {
            output.encode(encoder)?;
        }
        Ok(())
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        let mut state = Self::default();
        for _ in 0..decoder.count()? {
            state.applied.push(AppliedEntity::decode(decoder)?);
        }
        for _ in 0..decoder.count()? {
            state.one_shots.push(PendingOneShot::decode(decoder)?);
        }
        for _ in 0..decoder.count()? {
            state.resources.push(PendingResource::decode(decoder)?);
        }
        for _ in 0..decoder.count()? {
            state.outputs.push(PendingOutput::decode(decoder)?);
        }
        Ok(state)
    }
}

/// The whole frozen conflict.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingConflict {
    pub operation: OperationId,
    pub generation: u64,
    pub invocation: InvocationFingerprint,
    /// The sentence a person is shown. Presentation, and therefore *not* part
    /// of the identity: improving it must not invalidate every pending
    /// conflict in existence.
    pub resume_display: String,
    pub desired_inputs: Vec<FrozenDesiredInput>,
    pub candidate: PendingLedgerState,
    pub paths: Vec<PendingConflictPath>,
    pub frozen_nonconflict_postimages: Vec<FrozenPath>,
    pub effect_intents: Vec<DeferredEffectIntent>,
}

impl PendingConflict {
    /// Everything semantic, in canonical order, with presentation excluded.
    fn identity_bytes(&self) -> Result<Vec<u8>> {
        let mut encoder = Encoder::new();
        self.operation.encode(&mut encoder)?;
        encoder.u64(self.generation);
        self.invocation.encode(&mut encoder)?;
        encoder.count(self.desired_inputs.len())?;
        let mut previous: Option<&FrozenDesiredInput> = None;
        for input in &self.desired_inputs {
            ordered(previous, input)?;
            previous = Some(input);
            input.encode(&mut encoder)?;
        }
        self.candidate.encode(&mut encoder)?;
        encode_paths(&mut encoder, &self.paths)?;
        encoder.count(self.frozen_nonconflict_postimages.len())?;
        for frozen in &self.frozen_nonconflict_postimages {
            frozen.encode(&mut encoder)?;
        }
        encoder.count(self.effect_intents.len())?;
        for intent in &self.effect_intents {
            intent.encode(&mut encoder)?;
        }
        encoder.finish()
    }

    /// `SHA256("JAILS-PENDING-1" || encode(PendingIdentityV1))`.
    ///
    /// Derived, never stored beside the record: a stored hash is a second
    /// authority for what the record says, and it permits finalising a record
    /// that has been edited under a hash that still matches.
    pub fn identity(&self) -> Result<crate::conflict::PendingIdentity> {
        Ok(crate::conflict::PendingIdentity::from_object(
            pending_identity(&self.identity_bytes()?),
        ))
    }

    /// A resolution is only for a path this conflict actually named.
    pub fn conflicted(&self, path: &ProjectPath) -> bool {
        self.paths.iter().any(|one| &one.path == path)
    }

    /// The frozen postimage of a path the conflict did *not* touch.
    pub fn frozen(&self, path: &ProjectPath) -> Option<&FrozenPath> {
        self.frozen_nonconflict_postimages
            .iter()
            .find(|frozen| &frozen.path == path)
    }
}

/// A live image, for a frozen clean postimage.
pub fn exact(image: LiveFileImage) -> crate::conflict::PendingCurrent {
    crate::conflict::PendingCurrent::Exact(image)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conflict::{FileMode, MarkerTokens};
    use crate::identity::ObjectRef;
    use crate::request::{CanonicalMutationRequest, CanonicalRequestSyntaxV1};
    use jails_support::codec::sha256;

    fn object(seed: &str) -> ObjectId {
        ObjectId::from_bytes(sha256(seed.as_bytes()))
    }

    fn stored(seed: &str) -> StoredFileImage {
        StoredFileImage {
            object: ObjectRef::new(object(seed), seed.len() as u64),
            mode: FileMode::new(0o644).unwrap(),
        }
    }

    fn conflict() -> PendingConflict {
        PendingConflict {
            operation: OperationId::from_bytes(sha256(b"op")),
            generation: 4,
            invocation: InvocationFingerprint {
                request_syntax: CanonicalRequestSyntaxV1::default().fingerprint().unwrap(),
                request: CanonicalMutationRequest::Sync { no_start: false },
                manifest_source: None,
                desired_input_sha256: object("inputs"),
            },
            resume_display: "jails app apply".to_string(),
            desired_inputs: vec![FrozenDesiredInput {
                id: DesiredInputId::HumanConfig,
                guard: DesiredInputGuard::Absent,
            }],
            candidate: PendingLedgerState::default(),
            paths: vec![PendingConflictPath {
                path: ProjectPath::parse("pom.xml").unwrap(),
                prior_base: stored("B"),
                desired_base: stored("N"),
                marker_image: stored("markers"),
                markers: MarkerTokens::new("<<<<<<<", "=======", ">>>>>>>").unwrap(),
                hunk_count: 1,
            }],
            frozen_nonconflict_postimages: Vec::new(),
            effect_intents: Vec::new(),
        }
    }

    /// Improving the sentence a person is shown must not invalidate every
    /// pending conflict in existence.
    #[test]
    fn the_resume_sentence_is_not_part_of_the_identity() {
        let one = conflict();
        let mut reworded = conflict();
        reworded.resume_display = "run `jails app apply` again".to_string();
        assert_eq!(one.identity().unwrap(), reworded.identity().unwrap());
    }

    /// Every semantic field is, though — otherwise a record could be edited
    /// under a hash that still matches.
    #[test]
    fn every_semantic_field_changes_the_identity() {
        let base = conflict().identity().unwrap();

        let mut other_generation = conflict();
        other_generation.generation = 5;
        assert_ne!(base, other_generation.identity().unwrap());

        let mut other_path = conflict();
        other_path.paths[0].path = ProjectPath::parse("compose.yaml").unwrap();
        assert_ne!(base, other_path.identity().unwrap());

        let mut other_marker = conflict();
        other_marker.paths[0].marker_image = stored("edited");
        assert_ne!(base, other_marker.identity().unwrap());

        let mut other_input = conflict();
        other_input.desired_inputs[0].guard = DesiredInputGuard::Exact {
            sha256: object("something"),
            len: 9,
        };
        assert_ne!(base, other_input.identity().unwrap());
    }

    /// An unsorted input list would give one record two encodings and
    /// therefore two identities.
    #[test]
    fn an_unsorted_desired_input_list_is_refused() {
        let mut unsorted = conflict();
        unsorted.desired_inputs = vec![
            FrozenDesiredInput {
                id: DesiredInputId::DirectRequest,
                guard: DesiredInputGuard::Absent,
            },
            FrozenDesiredInput {
                id: DesiredInputId::HumanConfig,
                guard: DesiredInputGuard::Absent,
            },
        ];
        assert!(unsorted.identity().is_err());
    }

    /// A resolution is only accepted for a path the conflict actually named.
    #[test]
    fn a_conflict_knows_which_paths_it_froze() {
        let one = conflict();
        assert!(one.conflicted(&ProjectPath::parse("pom.xml").unwrap()));
        assert!(!one.conflicted(&ProjectPath::parse("compose.yaml").unwrap()));
    }

    /// The bytes alone would match a file somewhere else that happened to be
    /// identical, so a projected output is guarded by its path too.
    #[test]
    fn a_projected_output_guard_carries_its_path() {
        let guard = DesiredInputGuard::ProjectedTransactionOutput {
            path: ProjectPath::parse("jails.toml").unwrap(),
            sha256: object("config"),
            len: 6,
        };
        let mut encoder = Encoder::new();
        guard.encode(&mut encoder).unwrap();
        let bytes = encoder.finish().unwrap();
        let mut decoder = Decoder::new(&bytes).unwrap();
        assert_eq!(DesiredInputGuard::decode(&mut decoder).unwrap(), guard);
    }
}
