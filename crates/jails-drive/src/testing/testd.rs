//! Authenticated, length-framed protocol for the resident test engine.
//!
//! The daemon is an execution detail of `jails test`. These values keep its
//! transport independent from both CLI spelling and JUnit's presentation
//! output, and make retries and stale-epoch rejection explicit.

use super::{TestReportV1, TestSelector};
use jails_support::Result;
use jails_support::codec::{Codec, DIGEST_BYTES, Decoder, Encoder};
use jails_support::identity::{ObjectId, ProjectPath};

pub(crate) const TESTD_V2_PROTOCOL_MIN: u16 = 2;
pub(crate) const TESTD_V2_PROTOCOL_MAX: u16 = 2;
pub(crate) const TESTD_V2_MAX_PAYLOAD: usize = 8 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct RequestId([u8; DIGEST_BYTES]);

impl RequestId {
    pub fn from_bytes(bytes: [u8; DIGEST_BYTES]) -> Self {
        Self(bytes)
    }
}

impl Codec for RequestId {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.digest(&self.0);
        Ok(())
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        decoder.digest().map(Self)
    }
}

#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct SecretBytes([u8; DIGEST_BYTES]);

impl SecretBytes {
    pub fn from_bytes(bytes: [u8; DIGEST_BYTES]) -> Self {
        Self(bytes)
    }

    pub fn expose(&self) -> &[u8; DIGEST_BYTES] {
        &self.0
    }
}

impl std::fmt::Debug for SecretBytes {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("SecretBytes([redacted])")
    }
}

impl Codec for SecretBytes {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.digest(&self.0);
        Ok(())
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        decoder.digest().map(Self)
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct OutputPath(String);

impl OutputPath {
    pub fn parse(text: &str) -> Result<Self> {
        if text.is_empty()
            || text.starts_with('/')
            || text.ends_with('/')
            || text.contains(['\\', '\0', '\n', '\r'])
            || text
                .split('/')
                .any(|part| part.is_empty() || part == "." || part == "..")
        {
            return Err(format!(
                "invalid output path `{text}`\n       fix: use a normalized project-relative class or resource path"
            )
            .into());
        }
        Ok(Self(text.to_string()))
    }
}

impl std::fmt::Display for OutputPath {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Codec for OutputPath {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.path(&self.0)
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Self::parse(&decoder.path()?)
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, jails_codec_derive::Codec)]
pub(crate) struct OutputEntryV1 {
    pub path: OutputPath,
    pub size: u64,
    pub modified_ns: u64,
    pub digest: ObjectId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct OutputSnapshotV1 {
    pub entries: Vec<OutputEntryV1>,
}

impl OutputSnapshotV1 {
    pub fn validate(&self) -> Result<()> {
        for pair in self.entries.windows(2) {
            if pair[0].path >= pair[1].path {
                return Err(format!(
                    "output snapshot paths are not sorted and unique at `{}`\n       fix: sort by \
                     project-relative path and remove duplicates before encoding",
                    pair[1].path
                )
                .into());
            }
        }
        Ok(())
    }
}

impl Codec for OutputSnapshotV1 {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.validate()?;
        encoder.seq(self.entries.len(), &self.entries)
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        let snapshot = Self {
            entries: decoder.seq()?,
        };
        snapshot.validate()?;
        Ok(snapshot)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, jails_codec_derive::Codec)]
#[codec(unknown_fix = "upgrade both testd protocol peers")]
pub(crate) enum TestIsolation {
    #[codec(tag = 0)]
    Isolated,
    #[codec(tag = 1)]
    ForkSensitive,
}

#[derive(Clone, Debug, Eq, PartialEq, jails_codec_derive::Codec)]
#[codec(unknown_fix = "upgrade both testd protocol peers")]
pub(crate) enum TestEventV2 {
    #[codec(tag = 0)]
    Ready { epoch: u64, output_current: bool },
    #[codec(tag = 1)]
    ClassesStale {
        epoch: u64,
        path: Option<ProjectPath>,
    },
    #[codec(tag = 2)]
    Delegating { epoch: u64, reason: String },
    #[codec(tag = 3)]
    Recycling { epoch: u64, reason: String },
}

#[derive(Clone, Debug, Eq, PartialEq, jails_codec_derive::Codec)]
pub(crate) struct TestdDiagnosticV1 {
    pub code: String,
    pub message: String,
    pub fix: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TestdRequestV2 {
    Hello {
        request_id: RequestId,
        protocol_min: u16,
        protocol_max: u16,
        project: ObjectId,
        cookie: SecretBytes,
    },
    Run {
        request_id: RequestId,
        project: ObjectId,
        cookie: SecretBytes,
        epoch: u64,
        selectors: Vec<TestSelector>,
        classpath: ObjectId,
        outputs: OutputSnapshotV1,
        isolation: TestIsolation,
    },
    Status {
        request_id: RequestId,
        project: ObjectId,
        cookie: SecretBytes,
    },
    Cancel {
        request_id: RequestId,
        project: ObjectId,
        cookie: SecretBytes,
    },
    Stop {
        request_id: RequestId,
        project: ObjectId,
        cookie: SecretBytes,
    },
}

impl Codec for TestdRequestV2 {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        match self {
            Self::Hello {
                request_id,
                protocol_min,
                protocol_max,
                project,
                cookie,
            } => {
                if protocol_min > protocol_max {
                    return Err("testd protocol minimum exceeds its maximum\n       fix: provide an ordered compatible protocol range".into());
                }
                encoder.tag(0);
                request_id.encode(encoder)?;
                encoder.u32(u32::from(*protocol_min));
                encoder.u32(u32::from(*protocol_max));
                project.encode(encoder)?;
                cookie.encode(encoder)?;
            }
            Self::Run {
                request_id,
                project,
                cookie,
                epoch,
                selectors,
                classpath,
                outputs,
                isolation,
            } => {
                encoder.tag(1);
                request_id.encode(encoder)?;
                project.encode(encoder)?;
                cookie.encode(encoder)?;
                encoder.u64(*epoch);
                encoder.seq(selectors.len(), selectors)?;
                classpath.encode(encoder)?;
                outputs.encode(encoder)?;
                isolation.encode(encoder)?;
            }
            Self::Status {
                request_id,
                project,
                cookie,
            } => encode_authenticated(2, request_id, project, cookie, encoder)?,
            Self::Cancel {
                request_id,
                project,
                cookie,
            } => encode_authenticated(3, request_id, project, cookie, encoder)?,
            Self::Stop {
                request_id,
                project,
                cookie,
            } => encode_authenticated(4, request_id, project, cookie, encoder)?,
        }
        Ok(())
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        match decoder.tag()? {
            0 => {
                let request_id = RequestId::decode(decoder)?;
                let protocol_min = decode_u16(decoder)?;
                let protocol_max = decode_u16(decoder)?;
                if protocol_min > protocol_max {
                    return Err("testd protocol minimum exceeds its maximum\n       fix: upgrade the peer that emitted the invalid range".into());
                }
                Ok(Self::Hello {
                    request_id,
                    protocol_min,
                    protocol_max,
                    project: ObjectId::decode(decoder)?,
                    cookie: SecretBytes::decode(decoder)?,
                })
            }
            1 => Ok(Self::Run {
                request_id: RequestId::decode(decoder)?,
                project: ObjectId::decode(decoder)?,
                cookie: SecretBytes::decode(decoder)?,
                epoch: decoder.u64()?,
                selectors: decoder.seq()?,
                classpath: ObjectId::decode(decoder)?,
                outputs: OutputSnapshotV1::decode(decoder)?,
                isolation: TestIsolation::decode(decoder)?,
            }),
            2 => decode_authenticated(decoder, |request_id, project, cookie| Self::Status {
                request_id,
                project,
                cookie,
            }),
            3 => decode_authenticated(decoder, |request_id, project, cookie| Self::Cancel {
                request_id,
                project,
                cookie,
            }),
            4 => decode_authenticated(decoder, |request_id, project, cookie| Self::Stop {
                request_id,
                project,
                cookie,
            }),
            other => Err(format!(
                "unknown TestdRequestV2 tag {other}\n       fix: upgrade both testd protocol peers"
            )
            .into()),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TestdResponseV2 {
    Hello {
        request_id: RequestId,
        protocol: u16,
    },
    Accepted {
        request_id: RequestId,
        epoch: u64,
    },
    Event {
        request_id: RequestId,
        event: TestEventV2,
    },
    Completed {
        request_id: RequestId,
        result: TestReportV1,
    },
    Refused {
        request_id: RequestId,
        diagnostic: TestdDiagnosticV1,
    },
}

impl Codec for TestdResponseV2 {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        match self {
            Self::Hello {
                request_id,
                protocol,
            } => {
                encoder.tag(0);
                request_id.encode(encoder)?;
                encoder.u32(u32::from(*protocol));
            }
            Self::Accepted { request_id, epoch } => {
                encoder.tag(1);
                request_id.encode(encoder)?;
                encoder.u64(*epoch);
            }
            Self::Event { request_id, event } => {
                encoder.tag(2);
                request_id.encode(encoder)?;
                event.encode(encoder)?;
            }
            Self::Completed { request_id, result } => {
                encoder.tag(3);
                request_id.encode(encoder)?;
                result.encode(encoder)?;
            }
            Self::Refused {
                request_id,
                diagnostic,
            } => {
                encoder.tag(4);
                request_id.encode(encoder)?;
                diagnostic.encode(encoder)?;
            }
        }
        Ok(())
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        let tag = decoder.tag()?;
        let request_id = RequestId::decode(decoder)?;
        match tag {
            0 => Ok(Self::Hello {
                request_id,
                protocol: decode_u16(decoder)?,
            }),
            1 => Ok(Self::Accepted {
                request_id,
                epoch: decoder.u64()?,
            }),
            2 => Ok(Self::Event {
                request_id,
                event: TestEventV2::decode(decoder)?,
            }),
            3 => Ok(Self::Completed {
                request_id,
                result: TestReportV1::decode(decoder)?,
            }),
            4 => Ok(Self::Refused {
                request_id,
                diagnostic: TestdDiagnosticV1::decode(decoder)?,
            }),
            other => Err(format!(
                "unknown TestdResponseV2 tag {other}\n       fix: upgrade both testd protocol peers"
            )
            .into()),
        }
    }
}

pub(crate) fn encode_frame<T: Codec>(value: &T) -> Result<Vec<u8>> {
    let mut encoder = Encoder::new();
    value.encode(&mut encoder)?;
    let payload = encoder.finish()?;
    if payload.len() > TESTD_V2_MAX_PAYLOAD {
        return Err(format!(
            "testd payload is {} bytes, over the {}-byte limit\n       fix: narrow the test selection or output snapshot",
            payload.len(), TESTD_V2_MAX_PAYLOAD
        )
        .into());
    }
    let length = u32::try_from(payload.len()).expect("8 MiB fits in u32");
    let mut frame = Vec::with_capacity(4 + payload.len());
    frame.extend_from_slice(&length.to_be_bytes());
    frame.extend_from_slice(&payload);
    Ok(frame)
}

pub(crate) fn decode_frame<T: Codec>(frame: &[u8]) -> Result<T> {
    let header: [u8; 4] = frame
        .get(..4)
        .ok_or("testd frame is missing its 4-byte length\n       fix: restart the daemon with a matching jails version")?
        .try_into()
        .expect("slice has four bytes");
    let declared = u32::from_be_bytes(header) as usize;
    if declared > TESTD_V2_MAX_PAYLOAD {
        return Err(format!(
            "testd frame claims {declared} bytes, over the {TESTD_V2_MAX_PAYLOAD}-byte limit\n       fix: restart the daemon with a matching jails version"
        )
        .into());
    }
    if frame.len() != 4 + declared {
        return Err(format!(
            "testd frame declares {declared} payload bytes but carries {}\n       fix: retry once, then restart the daemon if truncation repeats",
            frame.len().saturating_sub(4)
        )
        .into());
    }
    let mut decoder = Decoder::new(&frame[4..])?;
    let value = T::decode(&mut decoder)?;
    decoder.finish()?;
    Ok(value)
}

fn encode_authenticated(
    tag: u8,
    request_id: &RequestId,
    project: &ObjectId,
    cookie: &SecretBytes,
    encoder: &mut Encoder,
) -> Result<()> {
    encoder.tag(tag);
    request_id.encode(encoder)?;
    project.encode(encoder)?;
    cookie.encode(encoder)
}

fn decode_authenticated<T>(
    decoder: &mut Decoder<'_>,
    make: impl FnOnce(RequestId, ObjectId, SecretBytes) -> T,
) -> Result<T> {
    Ok(make(
        RequestId::decode(decoder)?,
        ObjectId::decode(decoder)?,
        SecretBytes::decode(decoder)?,
    ))
}

fn decode_u16(decoder: &mut Decoder<'_>) -> Result<u16> {
    u16::try_from(decoder.u32()?).map_err(|_| {
        "testd protocol version exceeds u16\n       fix: upgrade the peer that emitted the invalid version".into()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digest(byte: u8) -> ObjectId {
        ObjectId::from_bytes([byte; DIGEST_BYTES])
    }
    fn request_id() -> RequestId {
        RequestId::from_bytes([7; DIGEST_BYTES])
    }
    fn cookie() -> SecretBytes {
        SecretBytes::from_bytes([9; DIGEST_BYTES])
    }

    #[test]
    fn authenticated_run_frame_round_trips() {
        let request = TestdRequestV2::Run {
            request_id: request_id(),
            project: digest(1),
            cookie: cookie(),
            epoch: 42,
            selectors: vec![TestSelector::parse("ExampleTest#works").unwrap()],
            classpath: digest(2),
            outputs: OutputSnapshotV1 {
                entries: vec![OutputEntryV1 {
                    path: OutputPath::parse("target/test-classes/ExampleTest.class").unwrap(),
                    size: 123,
                    modified_ns: 456,
                    digest: digest(3),
                }],
            },
            isolation: TestIsolation::Isolated,
        };
        let frame = encode_frame(&request).unwrap();
        assert_eq!(decode_frame::<TestdRequestV2>(&frame).unwrap(), request);
        assert_eq!(
            u32::from_be_bytes(frame[..4].try_into().unwrap()) as usize,
            frame.len() - 4
        );
    }

    #[test]
    fn malformed_and_oversized_frames_fail_closed() {
        assert!(decode_frame::<TestdRequestV2>(&[0, 0, 0]).is_err());
        assert!(decode_frame::<TestdRequestV2>(&[0, 0, 0, 2, 0]).is_err());
        let claimed = (TESTD_V2_MAX_PAYLOAD as u32 + 1).to_be_bytes();
        assert!(decode_frame::<TestdRequestV2>(&claimed).is_err());
    }

    #[test]
    fn output_snapshot_requires_sorted_unique_paths() {
        let entry = |path| OutputEntryV1 {
            path: OutputPath::parse(path).unwrap(),
            size: 1,
            modified_ns: 2,
            digest: digest(3),
        };
        let snapshot = OutputSnapshotV1 {
            entries: vec![entry("z.class"), entry("a.class")],
        };
        assert!(snapshot.validate().is_err());
    }

    #[test]
    fn cookies_are_redacted_from_debug_output() {
        let rendered = format!("{:?}", cookie());
        assert_eq!(rendered, "SecretBytes([redacted])");
        assert!(!rendered.contains('9'));
    }
}
