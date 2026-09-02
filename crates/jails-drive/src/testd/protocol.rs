//! The daemon's frames: an encoding, not a second vocabulary.
//!
//! `templates/testd/JailsTestDaemon.java` is the other end of this file and
//! the two change together. Everything below is either a fact the daemon
//! observed (which tests ran, what JUnit printed) or the authentication and
//! version negotiation that framing needs; the report a reader sees is built
//! from these by [`super::client`], out of `crate::testing`'s one vocabulary.
//! The daemon does not describe a [`crate::testing::TestReport`] and must
//! never learn how to: a peer that restates a field list restates it wrongly
//! the first time a field moves.
//!
//! ## The framing
//!
//! A frame is a four-byte big-endian payload length followed by that many
//! bytes of JSON. The length prefix is what lets a reader know when a frame
//! has arrived without parsing it, and the cap is checked against the declared
//! length before anything is allocated: the four bytes come from a process
//! this one did not write.
//!
//! JSON rather than a private binary format because the daemon is a
//! single-file Java program with only JUnit on its classpath, and the previous
//! shape made it restate every tag by number -- `body.tag(2); body.tag(3); //
//! compile owner: none` -- with nothing on either side to notice when a
//! meaning moved. Named fields make a mismatch a decode error rather than a
//! plausible wrong answer.

use crate::testing::{TestOutcome, TestSelector};
use jails_support::Result;
use jails_support::codec::DIGEST_BYTES;
use jails_support::identity::ObjectId;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

/// Bumped whenever these frames change: the metadata file records the range a
/// daemon was started for, so a client from another release restarts it rather
/// than talking past it.
pub(crate) const TESTD_PROTOCOL_MIN: u16 = 3;
pub(crate) const TESTD_PROTOCOL_MAX: u16 = 3;
pub(crate) const TESTD_MAX_PAYLOAD: usize = 8 * 1024 * 1024;

/// 32 random bytes naming one request, so a retry over a reconnected socket
/// is answered from the daemon's cache rather than run twice.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub(crate) struct RequestId(#[serde(with = "digest_hex")] [u8; DIGEST_BYTES]);

impl RequestId {
    pub fn from_bytes(bytes: [u8; DIGEST_BYTES]) -> Self {
        Self(bytes)
    }
}

/// The daemon's start-up cookie. Never rendered: a `Debug` that printed it
/// would put it in every diagnostic that formats a request.
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub(crate) struct SecretBytes(#[serde(with = "digest_hex")] [u8; DIGEST_BYTES]);

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

/// A normalized project-relative path to one compiled output.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
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

impl TryFrom<String> for OutputPath {
    type Error = jails_support::Failure;

    fn try_from(text: String) -> Result<Self> {
        Self::parse(&text)
    }
}

impl From<OutputPath> for String {
    fn from(path: OutputPath) -> Self {
        path.0
    }
}

/// One compiled file as the coordinator saw it when it planned the run.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub(crate) struct OutputEntry {
    pub path: OutputPath,
    pub modified_ns: u64,
    #[serde(with = "digest_hex")]
    pub digest: [u8; DIGEST_BYTES],
}

/// Every compiled output, sorted and distinct.
///
/// Sortedness is not tidiness: the daemon compares this list against what is
/// on disk *now* and refuses the run when they differ, and two spellings of
/// one set would make that comparison depend on iteration order.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct OutputSnapshot {
    pub entries: Vec<OutputEntry>,
}

impl OutputSnapshot {
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

/// Whether the selected tests may share the daemon's JVM.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TestIsolation {
    Isolated,
    ForkSensitive,
}

/// What the daemon is doing, for a run that has not finished.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum TestEvent {
    Ready {
        epoch: u64,
        output_current: bool,
    },
    ClassesStale {
        epoch: u64,
        path: Option<OutputPath>,
    },
    Delegating {
        epoch: u64,
        reason: String,
    },
    Recycling {
        epoch: u64,
        reason: String,
    },
}

/// Why the daemon would not run something, and what to do instead.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct Diagnostic {
    pub code: String,
    pub message: String,
    pub fix: Option<String>,
}

/// One test the daemon executed.
///
/// Three facts, because three is all the daemon can see. Which engine ran it
/// and why it was selected are the coordinator's to know, and it is the
/// coordinator that builds the `TestCaseResult`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct DaemonCase {
    pub selector: TestSelector,
    pub outcome: TestOutcome,
    pub duration_us: u64,
}

/// A finished run, as the daemon observed it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct DaemonRun {
    pub epoch: u64,
    pub passed: bool,
    /// JUnit's own output for the run, already bounded by the daemon.
    pub output: String,
    pub cases: Vec<DaemonCase>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "request", rename_all = "snake_case")]
pub(crate) enum Request {
    Hello {
        request_id: RequestId,
        protocol_min: u16,
        protocol_max: u16,
        #[serde(with = "object_id_hex")]
        project: ObjectId,
        cookie: SecretBytes,
    },
    Run {
        request_id: RequestId,
        #[serde(with = "object_id_hex")]
        project: ObjectId,
        cookie: SecretBytes,
        epoch: u64,
        selectors: Vec<TestSelector>,
        #[serde(with = "object_id_hex")]
        classpath: ObjectId,
        outputs: OutputSnapshot,
        isolation: TestIsolation,
    },
    Status {
        request_id: RequestId,
        #[serde(with = "object_id_hex")]
        project: ObjectId,
        cookie: SecretBytes,
    },
    Cancel {
        request_id: RequestId,
        #[serde(with = "object_id_hex")]
        project: ObjectId,
        cookie: SecretBytes,
    },
    Stop {
        request_id: RequestId,
        #[serde(with = "object_id_hex")]
        project: ObjectId,
        cookie: SecretBytes,
    },
}

impl Request {
    pub fn request_id(&self) -> RequestId {
        match self {
            Self::Hello { request_id, .. }
            | Self::Run { request_id, .. }
            | Self::Status { request_id, .. }
            | Self::Cancel { request_id, .. }
            | Self::Stop { request_id, .. } => *request_id,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "response", rename_all = "snake_case")]
pub(crate) enum Response {
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
        event: TestEvent,
    },
    Completed {
        request_id: RequestId,
        result: DaemonRun,
    },
    Refused {
        request_id: RequestId,
        diagnostic: Diagnostic,
    },
}

impl Response {
    pub fn request_id(&self) -> RequestId {
        match self {
            Self::Hello { request_id, .. }
            | Self::Accepted { request_id, .. }
            | Self::Event { request_id, .. }
            | Self::Completed { request_id, .. }
            | Self::Refused { request_id, .. } => *request_id,
        }
    }
}

/// One request or response, ready to write to the socket.
pub(crate) fn encode_frame<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let payload = serde_json::to_vec(value)
        .map_err(|error| format!("testd could not encode a frame: {error}"))?;
    if payload.len() > TESTD_MAX_PAYLOAD {
        return Err(format!(
            "testd payload is {} bytes, over the {TESTD_MAX_PAYLOAD}-byte limit\n       fix: narrow the test selection or output snapshot",
            payload.len()
        )
        .into());
    }
    let length = u32::try_from(payload.len()).expect("8 MiB fits in u32");
    let mut frame = Vec::with_capacity(4 + payload.len());
    frame.extend_from_slice(&length.to_be_bytes());
    frame.extend_from_slice(&payload);
    Ok(frame)
}

/// The value in a complete frame, refusing anything the header does not
/// describe exactly.
pub(crate) fn decode_frame<T: DeserializeOwned>(frame: &[u8]) -> Result<T> {
    let declared = declared_length(frame)?;
    if frame.len() != 4 + declared {
        return Err(format!(
            "testd frame declares {declared} payload bytes but carries {}\n       fix: retry once, then restart the daemon if truncation repeats",
            frame.len().saturating_sub(4)
        )
        .into());
    }
    serde_json::from_slice(&frame[4..])
        .map_err(|error| format!("testd sent a frame this jails cannot read: {error}\n       fix: restart the daemon with a matching jails version").into())
}

/// The payload length a frame header declares, checked against the cap before
/// a reader allocates for it.
pub(crate) fn declared_length(header: &[u8]) -> Result<usize> {
    let header: [u8; 4] = header
        .get(..4)
        .ok_or("testd frame is missing its 4-byte length\n       fix: restart the daemon with a matching jails version")?
        .try_into()
        .expect("slice has four bytes");
    let declared = u32::from_be_bytes(header) as usize;
    if declared > TESTD_MAX_PAYLOAD {
        return Err(format!(
            "testd frame claims {declared} bytes, over the {TESTD_MAX_PAYLOAD}-byte limit\n       fix: restart the daemon with a matching jails version"
        )
        .into());
    }
    Ok(declared)
}

/// 32 raw bytes as 64 lowercase hex characters, which is what both peers
/// already spell digests as on the command line and in the metadata file.
mod digest_hex {
    use jails_support::codec::{DIGEST_BYTES, hex, unhex};
    use serde::{Deserialize, Deserializer, Serializer};

    pub(super) fn serialize<S: Serializer>(
        bytes: &[u8; DIGEST_BYTES],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&hex(bytes))
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<[u8; DIGEST_BYTES], D::Error> {
        let text = String::deserialize(deserializer)?;
        unhex(&text).map_err(serde::de::Error::custom)
    }
}

mod object_id_hex {
    use jails_support::identity::ObjectId;
    use serde::{Deserialize, Deserializer, Serializer};

    pub(super) fn serialize<S: Serializer>(
        id: &ObjectId,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&id.to_hex())
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<ObjectId, D::Error> {
        let text = String::deserialize(deserializer)?;
        ObjectId::parse_hex(&text).map_err(serde::de::Error::custom)
    }
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
        let request = Request::Run {
            request_id: request_id(),
            project: digest(1),
            cookie: cookie(),
            epoch: 42,
            selectors: vec![TestSelector::parse("ExampleTest#works").unwrap()],
            classpath: digest(2),
            outputs: OutputSnapshot {
                entries: vec![OutputEntry {
                    path: OutputPath::parse("target/test-classes/ExampleTest.class").unwrap(),
                    modified_ns: 456,
                    digest: [3; DIGEST_BYTES],
                }],
            },
            isolation: TestIsolation::Isolated,
        };
        let frame = encode_frame(&request).unwrap();
        assert_eq!(decode_frame::<Request>(&frame).unwrap(), request);
        assert_eq!(
            u32::from_be_bytes(frame[..4].try_into().unwrap()) as usize,
            frame.len() - 4
        );
        // The daemon reads these names, so they are part of the contract.
        let json = String::from_utf8(frame[4..].to_vec()).unwrap();
        assert!(json.contains("\"request\":\"run\""), "{json}");
        assert!(json.contains("\"isolation\":\"isolated\""), "{json}");
    }

    #[test]
    fn a_completed_response_round_trips_the_daemon_s_own_observation() {
        let response = Response::Completed {
            request_id: request_id(),
            result: DaemonRun {
                epoch: 7,
                passed: true,
                output: "ok".into(),
                cases: vec![DaemonCase {
                    selector: TestSelector::parse("a.BTest#works").unwrap(),
                    outcome: TestOutcome::Passed,
                    duration_us: 8_000,
                }],
            },
        };
        let frame = encode_frame(&response).unwrap();
        assert_eq!(decode_frame::<Response>(&frame).unwrap(), response);
        assert_eq!(
            decode_frame::<Response>(&frame).unwrap().request_id(),
            request_id()
        );
    }

    /// The daemon writes these by hand, so the shape is asserted rather than
    /// assumed: an event is one named kind with its own fields beside it.
    #[test]
    fn an_event_response_is_one_named_kind() {
        let response = Response::Event {
            request_id: request_id(),
            event: TestEvent::Ready {
                epoch: 3,
                output_current: true,
            },
        };
        let frame = encode_frame(&response).unwrap();
        let json = String::from_utf8(frame[4..].to_vec()).unwrap();
        assert!(json.contains("\"response\":\"event\""), "{json}");
        assert!(json.contains("\"kind\":\"ready\""), "{json}");
        assert_eq!(decode_frame::<Response>(&frame).unwrap(), response);
    }

    #[test]
    fn malformed_and_oversized_frames_fail_closed() {
        assert!(decode_frame::<Request>(&[0, 0, 0]).is_err());
        assert!(decode_frame::<Request>(&[0, 0, 0, 2, 0]).is_err());
        let claimed = (TESTD_MAX_PAYLOAD as u32 + 1).to_be_bytes();
        assert!(decode_frame::<Request>(&claimed).is_err());
        assert!(declared_length(&claimed).is_err());
    }

    #[test]
    fn output_snapshot_requires_sorted_unique_paths() {
        let entry = |path| OutputEntry {
            path: OutputPath::parse(path).unwrap(),
            modified_ns: 2,
            digest: [3; DIGEST_BYTES],
        };
        let snapshot = OutputSnapshot {
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
