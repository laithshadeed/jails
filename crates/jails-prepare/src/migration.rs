//! The pre-schema-2 machine state a first schema-2 commit migrates.
//!
//! `translated_before` is recorded so preparation can re-run the translation
//! from the immutable objects and require the same answer. A migration that
//! could produce a different result on a retry would silently rewrite the
//! store it was meant to preserve.
//!
//! [`LegacyMigrationIdentity::deletable`] is the list the prepared value's
//! legacy-target rule is checked against: every present source *except* the
//! schema-1 ledger, which the guarded ledger create/replace consumes as
//! `ledger_before → ledger_after`. Deleting it here would drop the very rows
//! being migrated.

use crate::Result;
use jails_protocol::conflict::FileMode;
use jails_protocol::identity::{ObjectId, ObjectRef};
use jails_protocol::snapshot::{LegacyDirectoryKind, LegacySourcePath};
use jails_support::codec::{Decoder, Encoder};
use std::collections::BTreeSet;

/// One pre-schema-2 source, exactly as it was found.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LegacySourceImage {
    Absent {
        path: LegacySourcePath,
    },
    Present {
        path: LegacySourcePath,
        object: ObjectRef,
        mode: FileMode,
    },
}

impl LegacySourceImage {
    pub fn path(&self) -> &LegacySourcePath {
        match self {
            Self::Absent { path } | Self::Present { path, .. } => path,
        }
    }

    pub fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        match self {
            Self::Absent { path } => {
                encoder.tag(0);
                path.encode(encoder)
            }
            Self::Present { path, object, mode } => {
                encoder.tag(1);
                path.encode(encoder)?;
                object.encode(encoder);
                mode.encode(encoder);
                Ok(())
            }
        }
    }

    pub fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(match decoder.tag()? {
            0 => Self::Absent {
                path: LegacySourcePath::decode(decoder)?,
            },
            1 => Self::Present {
                path: LegacySourcePath::decode(decoder)?,
                object: ObjectRef::decode(decoder)?,
                mode: FileMode::decode(decoder)?,
            },
            other => Err(format!("unknown legacy source image tag {other}"))?,
        })
    }
}

/// The complete pre-schema-2 machine state a migration was computed from.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegacySnapshotIdentity {
    pub sources: Vec<LegacySourceImage>,
    pub directories: Vec<LegacyDirectoryKind>,
}

impl LegacySnapshotIdentity {
    pub fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.count(self.sources.len())?;
        for source in &self.sources {
            source.encode(encoder)?;
        }
        encoder.count(self.directories.len())?;
        for kind in &self.directories {
            kind.encode(encoder)?;
        }
        Ok(())
    }

    pub fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        let count = decoder.count()?;
        let mut sources = Vec::new();
        for _ in 0..count {
            sources.push(LegacySourceImage::decode(decoder)?);
        }
        let count = decoder.count()?;
        let mut directories = Vec::new();
        for _ in 0..count {
            directories.push(LegacyDirectoryKind::decode(decoder)?);
        }
        Ok(Self {
            sources,
            directories,
        })
    }
}

/// A first schema-2 commit's migration, with the exact inputs it translated.
///
/// `translated_before` is recorded so preparation can re-run the translation
/// from the immutable objects and require the same answer. A migration that
/// could produce a different result on a retry would silently rewrite the
/// store it was meant to preserve.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegacyMigrationIdentity {
    pub snapshot: LegacySnapshotIdentity,
    pub translated_before: ObjectId,
}

impl LegacyMigrationIdentity {
    pub fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.snapshot.encode(encoder)?;
        self.translated_before.encode(encoder);
        Ok(())
    }

    pub fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(Self {
            snapshot: LegacySnapshotIdentity::decode(decoder)?,
            translated_before: ObjectId::decode(decoder)?,
        })
    }

    /// The legacy sources this migration may delete: every present one except
    /// the schema-1 ledger, which the guarded ledger replace consumes.
    pub fn deletable(&self) -> BTreeSet<LegacySourcePath> {
        self.snapshot
            .sources
            .iter()
            .filter_map(|source| match source {
                LegacySourceImage::Present { path, .. }
                    if !matches!(path, LegacySourcePath::Schema1Ledger) =>
                {
                    Some(path.clone())
                }
                _ => None,
            })
            .collect()
    }
}
