//! A reconciliation that stopped half-way, frozen so the next run can finish
//! it rather than start again.
//!
//! ## Why any of this is stored
//!
//! A three-way merge that cannot be resolved automatically leaves conflict
//! markers in the working tree. The naive design has the next run recompute
//! everything — but it cannot: the project *is* the uncertain thing, and a
//! marker-bearing file re-read as input would feed conflict markers back into
//! the merge. So the run that stopped freezes what it knew, and the run that
//! resumes reads that instead of the tree.
//!
//! ## The two halves of every conflicted path
//!
//! `prior_base` and `desired_base` are the two sides the merge was between,
//! kept as stored images rather than hashes. A hash proves which bytes were
//! meant; only the bytes themselves let a later run redo the merge or explain
//! the result. `marker_image` is what was actually written to disk, so
//! finalisation can tell an unresolved file from one the user has since fixed.
//!
//! ## `PendingCurrent` is the interesting distinction
//!
//! An unaffected path's postimage is known now and frozen exactly. A
//! conflicted path's final content is not knowable until the user resolves it,
//! so it is `ResolveFromLive` — recorded as an explicit "learned later" rather
//! than as a placeholder hash that would look like a real image.
//!
//! `resume_display` is excluded from the identity because it is presentation.
//! Every semantic, object and effect field is included, and finalisation
//! recomputes the hash from the current ledger rather than trusting a stored
//! one.

use crate::Result;
use crate::identity::{ObjectId, ProjectPath};
use jails_support::codec::{self, Decoder, Encoder, ordered};

/// The committed live image of a file: content, length and mode together.
///
/// Length and mode are not optional validation details. Commit, reconciliation
/// and conflict finalisation compare the complete image, because a file with
/// the right bytes and the wrong mode is not the file that was meant.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LiveFileImage {
    pub sha256: ObjectId,
    pub len: u64,
    pub mode: FileMode,
}

/// The desired bytes and their mode, by reference into the object store.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StoredFileImage {
    pub object: ObjectId,
    pub mode: FileMode,
}

/// A POSIX mode, restricted to the permission bits.
///
/// Only `0o777` is permitted: setuid, setgid and sticky are not things jails
/// generates, and a platform that cannot apply and verify the prepared mode
/// refuses before activation rather than writing a file it cannot reproduce.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct FileMode(u32);

impl FileMode {
    pub const PERMITTED: u32 = 0o777;

    pub fn new(bits: u32) -> Result<Self> {
        if bits & !Self::PERMITTED != 0 {
            return Err(format!(
                "file mode {bits:#o} sets bits outside {:#o}; jails generates no setuid, setgid \
                 or sticky files",
                Self::PERMITTED
            ));
        }
        Ok(Self(bits))
    }

    pub fn bits(self) -> u32 {
        self.0
    }

    fn encode(self, encoder: &mut Encoder) {
        encoder.u32(self.0);
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Self::new(decoder.u32()?)
    }
}

impl LiveFileImage {
    pub fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.sha256.encode(encoder);
        encoder.u64(self.len);
        self.mode.encode(encoder);
        Ok(())
    }

    pub fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(Self {
            sha256: ObjectId::decode(decoder)?,
            len: decoder.u64()?,
            mode: FileMode::decode(decoder)?,
        })
    }
}

impl StoredFileImage {
    pub fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.object.encode(encoder);
        self.mode.encode(encoder);
        Ok(())
    }

    pub fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(Self {
            object: ObjectId::decode(decoder)?,
            mode: FileMode::decode(decoder)?,
        })
    }
}

/// What a pending output's final content will be.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PendingCurrent {
    /// A clean or unaffected path: its postimage is known now and frozen.
    Exact(LiveFileImage),
    /// A conflicted path: not knowable until the user resolves it. Recorded as
    /// an explicit "learned later" rather than a placeholder that would look
    /// like a real image.
    ResolveFromLive,
}

impl PendingCurrent {
    pub fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        match self {
            Self::Exact(image) => {
                encoder.tag(0);
                image.encode(encoder)
            }
            Self::ResolveFromLive => {
                encoder.tag(1);
                Ok(())
            }
        }
    }

    pub fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(match decoder.tag()? {
            0 => Self::Exact(LiveFileImage::decode(decoder)?),
            1 => Self::ResolveFromLive,
            other => return Err(format!("unknown pending current tag {other}")),
        })
    }
}

/// The three tokens that delimit a conflict hunk.
///
/// Recorded rather than assumed: they are what finalisation looks for to tell
/// a still-conflicted file from a resolved one, and a project may configure
/// its own.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MarkerTokens {
    pub open: String,
    pub separator: String,
    pub close: String,
}

impl MarkerTokens {
    pub fn new(open: &str, separator: &str, close: &str) -> Result<Self> {
        for (label, token) in [("open", open), ("separator", separator), ("close", close)] {
            if token.is_empty() {
                return Err(format!("the {label} conflict marker is empty"));
            }
            if token.contains('\n') {
                return Err(format!("the {label} conflict marker spans a line"));
            }
        }
        if open == separator || separator == close || open == close {
            return Err(
                "two conflict markers are identical, so a hunk could not be delimited".to_string(),
            );
        }
        Ok(Self {
            open: open.to_string(),
            separator: separator.to_string(),
            close: close.to_string(),
        })
    }

    pub fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.string(&self.open)?;
        encoder.string(&self.separator)?;
        encoder.string(&self.close)
    }

    pub fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        let open = decoder.string()?;
        let separator = decoder.string()?;
        let close = decoder.string()?;
        Self::new(&open, &separator, &close)
    }
}

/// One path the merge could not resolve.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingConflictPath {
    pub path: ProjectPath,
    /// The base the working tree diverged from.
    pub prior_base: StoredFileImage,
    /// The bytes the generator wanted.
    pub desired_base: StoredFileImage,
    /// What was actually written, markers and all.
    pub marker_image: StoredFileImage,
    pub markers: MarkerTokens,
    pub hunk_count: u32,
}

impl PendingConflictPath {
    pub fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.path.encode(encoder)?;
        self.prior_base.encode(encoder)?;
        self.desired_base.encode(encoder)?;
        self.marker_image.encode(encoder)?;
        self.markers.encode(encoder)?;
        if self.hunk_count == 0 {
            return Err(format!(
                "`{}` is recorded as conflicted with zero hunks, which is not a conflict",
                self.path
            ));
        }
        encoder.u32(self.hunk_count);
        Ok(())
    }

    pub fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        let path = ProjectPath::decode(decoder)?;
        let prior_base = StoredFileImage::decode(decoder)?;
        let desired_base = StoredFileImage::decode(decoder)?;
        let marker_image = StoredFileImage::decode(decoder)?;
        let markers = MarkerTokens::decode(decoder)?;
        let hunk_count = decoder.u32()?;
        if hunk_count == 0 {
            return Err(format!(
                "`{path}` is recorded as conflicted with zero hunks, which is not a conflict"
            ));
        }
        Ok(Self {
            path,
            prior_base,
            desired_base,
            marker_image,
            markers,
            hunk_count,
        })
    }
}

/// A path the transaction wrote cleanly, frozen so the resume does not have to
/// re-derive it from a tree that is mid-conflict.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FrozenPath {
    pub path: ProjectPath,
    pub postimage: Option<LiveFileImage>,
}

impl FrozenPath {
    pub fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.path.encode(encoder)?;
        // `None` is a deletion: the path's postimage is that it is absent.
        encoder.option(self.postimage.as_ref(), |e, image| image.encode(e))
    }

    pub fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(Self {
            path: ProjectPath::decode(decoder)?,
            postimage: decoder.option(LiveFileImage::decode)?,
        })
    }
}

/// Compute the finalisation identity of a frozen conflict.
///
/// `resume_display` is deliberately excluded: it is presentation, and a
/// reworded message must not make a stored conflict unrecognisable. Every
/// semantic, object and effect field is included.
pub fn pending_identity(encoded_fields: &[u8]) -> ObjectId {
    ObjectId::from_bytes(codec::domain_hash("JAILS-PENDING-1", encoded_fields))
}

/// Encode a conflicted path list, refusing an unsorted or duplicated one.
pub fn encode_paths(encoder: &mut Encoder, paths: &[PendingConflictPath]) -> Result<()> {
    encoder.count(paths.len())?;
    let mut previous: Option<&ProjectPath> = None;
    for entry in paths {
        ordered(previous, &entry.path)?;
        previous = Some(&entry.path);
        entry.encode(encoder)?;
    }
    Ok(())
}

pub fn decode_paths(decoder: &mut Decoder<'_>) -> Result<Vec<PendingConflictPath>> {
    let count = decoder.count()?;
    let mut out: Vec<PendingConflictPath> = Vec::new();
    for _ in 0..count {
        let entry = PendingConflictPath::decode(decoder)?;
        ordered(out.last().map(|last| &last.path), &entry.path)?;
        out.push(entry);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use jails_support::codec::sha256;

    fn object(seed: &str) -> ObjectId {
        ObjectId::from_bytes(sha256(seed.as_bytes()))
    }

    fn mode() -> FileMode {
        FileMode::new(0o644).unwrap()
    }

    fn stored(seed: &str) -> StoredFileImage {
        StoredFileImage {
            object: object(seed),
            mode: mode(),
        }
    }

    fn markers() -> MarkerTokens {
        MarkerTokens::new("<<<<<<<", "=======", ">>>>>>>").unwrap()
    }

    fn conflicted(path: &str) -> PendingConflictPath {
        PendingConflictPath {
            path: ProjectPath::parse(path).unwrap(),
            prior_base: stored("prior"),
            desired_base: stored("desired"),
            marker_image: stored("markers"),
            markers: markers(),
            hunk_count: 2,
        }
    }

    /// A file with the right bytes and the wrong mode is not the file that was
    /// meant, so the image carries all three.
    #[test]
    fn a_file_image_is_content_length_and_mode_together() {
        let image = LiveFileImage {
            sha256: object("body"),
            len: 4,
            mode: mode(),
        };
        let mut encoder = Encoder::new();
        image.encode(&mut encoder).unwrap();
        let bytes = encoder.finish().unwrap();
        let mut decoder = Decoder::new(&bytes).unwrap();
        assert_eq!(LiveFileImage::decode(&mut decoder).unwrap(), image);
        decoder.finish().unwrap();

        // Same bytes, different mode: a different image.
        let executable = LiveFileImage {
            mode: FileMode::new(0o755).unwrap(),
            ..image
        };
        assert_ne!(executable, image);
    }

    /// jails generates no setuid, setgid or sticky files, so those bits cannot
    /// be recorded at all — including through a decoder.
    #[test]
    fn a_mode_outside_the_permission_bits_refuses() {
        assert!(FileMode::new(0o644).is_ok());
        assert!(FileMode::new(0o777).is_ok());
        for forbidden in [0o4755, 0o2755, 0o1777, 0o100644] {
            let error = FileMode::new(forbidden).unwrap_err();
            assert!(error.contains("outside"), "{forbidden:o}: {error}");
        }

        let mut encoder = Encoder::new();
        encoder.u32(0o4755);
        let bytes = encoder.finish().unwrap();
        let mut decoder = Decoder::new(&bytes).unwrap();
        assert!(FileMode::decode(&mut decoder).is_err());
    }

    /// A conflicted path's content is not knowable yet, and saying so
    /// explicitly is different from storing a placeholder that would read as a
    /// real image.
    #[test]
    fn a_conflicted_path_records_that_its_content_is_learned_later() {
        for current in [
            PendingCurrent::Exact(LiveFileImage {
                sha256: object("clean"),
                len: 10,
                mode: mode(),
            }),
            PendingCurrent::ResolveFromLive,
        ] {
            let mut encoder = Encoder::new();
            current.encode(&mut encoder).unwrap();
            let bytes = encoder.finish().unwrap();
            let mut decoder = Decoder::new(&bytes).unwrap();
            assert_eq!(PendingCurrent::decode(&mut decoder).unwrap(), current);
            decoder.finish().unwrap();
        }
    }

    #[test]
    fn a_conflicted_path_round_trips_with_both_sides_of_the_merge() {
        let entry = conflicted("src/main/java/A.java");
        let mut encoder = Encoder::new();
        entry.encode(&mut encoder).unwrap();
        let bytes = encoder.finish().unwrap();
        let mut decoder = Decoder::new(&bytes).unwrap();
        assert_eq!(PendingConflictPath::decode(&mut decoder).unwrap(), entry);
        decoder.finish().unwrap();

        // The two sides are distinct, which is what lets a resume redo or
        // explain the merge rather than only detect it.
        assert_ne!(entry.prior_base, entry.desired_base);
    }

    #[test]
    fn a_conflict_with_no_hunks_is_not_a_conflict() {
        let mut entry = conflicted("a.txt");
        entry.hunk_count = 0;
        let mut encoder = Encoder::new();
        let error = entry.encode(&mut encoder).unwrap_err();
        assert!(error.contains("not a conflict"), "{error}");
    }

    /// Markers are recorded rather than assumed: finalisation looks for these
    /// exact tokens to tell an unresolved file from a fixed one.
    #[test]
    fn marker_tokens_must_be_distinct_single_line_and_present() {
        assert!(MarkerTokens::new("<<<", "===", ">>>").is_ok());
        for (open, separator, close) in [
            ("", "===", ">>>"),
            ("<<<", "", ">>>"),
            ("<<<", "===", ""),
            ("<<<", "<<<", ">>>"),
            ("<<<", "===", "<<<"),
            ("<<\n<", "===", ">>>"),
        ] {
            assert!(
                MarkerTokens::new(open, separator, close).is_err(),
                "{open:?}/{separator:?}/{close:?}"
            );
        }
    }

    /// A deletion's postimage is that the path is absent, which is a real
    /// value rather than a missing row.
    #[test]
    fn a_frozen_path_can_record_an_absence() {
        for postimage in [
            Some(LiveFileImage {
                sha256: object("kept"),
                len: 3,
                mode: mode(),
            }),
            None,
        ] {
            let frozen = FrozenPath {
                path: ProjectPath::parse("pom.xml").unwrap(),
                postimage,
            };
            let mut encoder = Encoder::new();
            frozen.encode(&mut encoder).unwrap();
            let bytes = encoder.finish().unwrap();
            let mut decoder = Decoder::new(&bytes).unwrap();
            assert_eq!(FrozenPath::decode(&mut decoder).unwrap(), frozen);
            decoder.finish().unwrap();
        }
    }

    #[test]
    fn a_conflicted_path_list_must_be_sorted_and_unique() {
        let sorted = [conflicted("a.txt"), conflicted("b.txt")];
        let mut encoder = Encoder::new();
        encode_paths(&mut encoder, &sorted).unwrap();
        let bytes = encoder.finish().unwrap();
        let mut decoder = Decoder::new(&bytes).unwrap();
        assert_eq!(decode_paths(&mut decoder).unwrap(), sorted);
        decoder.finish().unwrap();

        let unsorted = [conflicted("b.txt"), conflicted("a.txt")];
        let mut encoder = Encoder::new();
        assert!(encode_paths(&mut encoder, &unsorted).is_err());

        let duplicated = [conflicted("a.txt"), conflicted("a.txt")];
        let mut encoder = Encoder::new();
        assert!(encode_paths(&mut encoder, &duplicated).is_err());
    }

    /// A reworded message must not make a stored conflict unrecognisable, so
    /// the identity is over the semantic fields only.
    #[test]
    fn the_pending_identity_is_the_specified_domain_hash() {
        let mut encoder = Encoder::new();
        encode_paths(&mut encoder, &[conflicted("a.txt")]).unwrap();
        let encoded = encoder.finish().unwrap();
        assert_eq!(
            pending_identity(&encoded).as_bytes(),
            &codec::domain_hash("JAILS-PENDING-1", &encoded)
        );
    }
}
