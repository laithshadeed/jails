//! Rust client and process lifecycle for testd v2.

use crate::launcher;
use crate::model::Project;
use crate::process::CommandSpec;
use jails_protocol::identity::ObjectId;
use jails_protocol::testing::TestReportV1;
use jails_protocol::testing::testd::{
    OutputEntryV1, OutputPath, OutputSnapshotV1, RequestId, SecretBytes, TESTD_V2_MAX_PAYLOAD,
    TESTD_V2_PROTOCOL_MAX, TESTD_V2_PROTOCOL_MIN, TestIsolation, TestdRequestV2, TestdResponseV2,
    decode_frame, encode_frame,
};
use jails_support::Result;
use jails_support::codec::{DIGEST_BYTES, domain_hash, hex, sha256, unhex};
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const IDLE_SECONDS: u64 = 600;
const START_TIMEOUT: Duration = Duration::from_secs(90);
const HEAP_LIMIT: &str = "-Xmx512m";
const METASPACE_LIMIT: &str = "-XX:MaxMetaspaceSize=256m";

pub(super) struct Client {
    root: PathBuf,
    project: ObjectId,
    socket: PathBuf,
    meta: PathBuf,
    source: PathBuf,
}

impl Client {
    pub(super) fn for_project(project: &Project) -> Result<Self> {
        let root = project
            .root()
            .canonicalize()
            .unwrap_or_else(|_| project.root().to_path_buf());
        let project_id = ObjectId::from_bytes(domain_hash(
            "JAILS-TESTD-PROJECT-2",
            root.to_string_lossy().as_bytes(),
        ));
        let run = root.join(".jails/run");
        Ok(Self {
            root,
            project: project_id,
            socket: run.join("testd-v2.sock"),
            meta: run.join("testd-v2.meta"),
            source: run.join("testd-v2.java"),
        })
    }

    pub(super) fn socket(&self) -> &Path {
        &self.socket
    }

    pub(super) fn status(&self) -> Result<()> {
        match self.metadata().and_then(|meta| {
            self.exchange(
                &meta,
                TestdRequestV2::Status {
                    request_id: request_id()?,
                    project: self.project,
                    cookie: meta.cookie,
                },
            )
        }) {
            Ok(_) => println!("testd: running ({})", self.socket.display()),
            Err(_) => println!("testd: not running"),
        }
        Ok(())
    }

    pub(super) fn stop(&self) -> Result<()> {
        let stopped = self
            .metadata()
            .and_then(|meta| {
                self.exchange(
                    &meta,
                    TestdRequestV2::Stop {
                        request_id: request_id()?,
                        project: self.project,
                        cookie: meta.cookie,
                    },
                )
            })
            .is_ok();
        self.remove_runtime_files();
        println!("testd: {}", if stopped { "stopped" } else { "not running" });
        Ok(())
    }

    pub(super) fn stop_quietly(&self) {
        if let Ok(meta) = self.metadata()
            && let Ok(id) = request_id()
        {
            let _ = self.exchange(
                &meta,
                TestdRequestV2::Stop {
                    request_id: id,
                    project: self.project,
                    cookie: meta.cookie,
                },
            );
        }
        self.remove_runtime_files();
    }

    pub(super) fn ensure_running(
        &self,
        project: &Project,
        classpath: &launcher::TestClasspath,
        debug: bool,
    ) -> Result<()> {
        let classpath_id = classpath_id(classpath);
        if let Ok(meta) = self.metadata()
            && meta.project == self.project
            && meta.classpath == classpath_id
            && self.hello(&meta).is_ok()
        {
            return Ok(());
        }
        self.stop_quietly();
        self.start(project, classpath, classpath_id, debug)
    }

    pub(super) fn run(
        &self,
        classpath: &launcher::TestClasspath,
        selectors: &[String],
        epoch: u64,
    ) -> Result<TestReportV1> {
        let meta = self.metadata()?;
        let requested = selectors
            .iter()
            .map(|selector| jails_protocol::testing::TestSelector::parse(selector))
            .collect::<Result<Vec<_>>>()?;
        let request = TestdRequestV2::Run {
            request_id: request_id()?,
            project: self.project,
            cookie: meta.cookie,
            epoch,
            selectors: requested,
            classpath: classpath_id(classpath),
            outputs: output_snapshot(&self.root, classpath)?,
            isolation: TestIsolation::Isolated,
        };
        for response in self.exchange(&meta, request)? {
            match response {
                TestdResponseV2::Completed { result, .. } if result.epoch == epoch => {
                    return Ok(result);
                }
                TestdResponseV2::Completed { result, .. } => {
                    return Err(format!(
                        "testd returned stale epoch {} while {epoch} is active\n       fix: retry the run against the current daemon",
                        result.epoch
                    )
                    .into());
                }
                TestdResponseV2::Refused { diagnostic, .. } => {
                    return Err(format!(
                        "testd refused [{}]: {}\n       fix: {}",
                        diagnostic.code,
                        diagnostic.message,
                        diagnostic
                            .fix
                            .unwrap_or_else(|| "choose `--engine build`".into())
                    )
                    .into());
                }
                _ => {}
            }
        }
        Err("testd closed without a completed report\n       fix: retry once, then restart the daemon"
            .into())
    }

    fn hello(&self, meta: &Metadata) -> Result<()> {
        let request_id = request_id()?;
        let responses = self.exchange(
            meta,
            TestdRequestV2::Hello {
                request_id,
                protocol_min: TESTD_V2_PROTOCOL_MIN,
                protocol_max: TESTD_V2_PROTOCOL_MAX,
                project: self.project,
                cookie: meta.cookie,
            },
        )?;
        match responses.as_slice() {
            [TestdResponseV2::Hello {
                request_id: returned,
                protocol,
            }] if *returned == request_id && *protocol == TESTD_V2_PROTOCOL_MAX => Ok(()),
            _ => Err("testd handshake returned an incompatible response\n       fix: restart the daemon with this jails version".into()),
        }
    }

    fn start(
        &self,
        project: &Project,
        classpath: &launcher::TestClasspath,
        classpath_id: ObjectId,
        debug: bool,
    ) -> Result<()> {
        self.ensure_run_directory()?;
        let source = self.daemon_source()?;
        let dependencies = std::env::join_paths(&classpath.dependencies)
            .map_err(|error| format!("failed to join daemon classpath: {error}"))?;
        let outputs = std::env::join_paths(&classpath.outputs)
            .map_err(|error| format!("failed to join test outputs: {error}"))?;
        if !project.pom().contains("junit-platform-console") {
            return Err("testd needs junit-platform-console on the test classpath\n       fix: run `jails test --fast` once to install the matching launcher".into());
        }
        let cookie = secret()?;
        let spec = CommandSpec::new("java")
            .arg(HEAP_LIMIT)
            .arg(METASPACE_LIMIT)
            .arg("-cp")
            .arg(&dependencies)
            .arg(&source)
            .arg(&self.socket)
            .arg(IDLE_SECONDS.to_string())
            .arg(&outputs)
            .arg(self.project.to_hex())
            .arg(hex(cookie.expose()))
            .current_dir(&self.root);
        let mut child =
            crate::process::spawn(&spec, crate::process::Diagnostics::from_flag(debug))?;
        let metadata = Metadata {
            project: self.project,
            classpath: classpath_id,
            pid: child.id(),
            started_ms: now_millis(),
            cookie,
        };
        self.write_metadata(&metadata)?;
        let started = Instant::now();
        while started.elapsed() < START_TIMEOUT {
            if self.hello(&metadata).is_ok() {
                return Ok(());
            }
            if let Ok(Some(status)) = child.try_wait() {
                let mut stderr = String::new();
                if let Some(mut pipe) = child.stderr.take() {
                    pipe.read_to_string(&mut stderr).ok();
                }
                self.remove_runtime_files();
                return Err(format!(
                    "testd exited with {status} before its v2 handshake\n{}       fix: choose `--engine build`, or inspect the daemon diagnostic above",
                    indent(&stderr)
                )
                .into());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = child.kill();
        self.remove_runtime_files();
        Err("testd did not complete its v2 handshake in time\n       fix: choose `--engine build` and inspect daemon startup".into())
    }

    fn exchange(&self, _meta: &Metadata, request: TestdRequestV2) -> Result<Vec<TestdResponseV2>> {
        let expected_id = match &request {
            TestdRequestV2::Hello { request_id, .. }
            | TestdRequestV2::Run { request_id, .. }
            | TestdRequestV2::Status { request_id, .. }
            | TestdRequestV2::Cancel { request_id, .. }
            | TestdRequestV2::Stop { request_id, .. } => *request_id,
        };
        let run_request = matches!(&request, TestdRequestV2::Run { .. });
        let mut stream = UnixStream::connect(&self.socket)
            .map_err(|error| format!("testd: not running ({error})"))?;
        stream.set_read_timeout(Some(START_TIMEOUT)).ok();
        stream
            .write_all(&encode_frame(&request)?)
            .map_err(|error| format!("testd could not send a v2 frame: {error}"))?;
        let mut responses = Vec::new();
        loop {
            let response: TestdResponseV2 = read_frame(&mut stream)?;
            let returned_id = match &response {
                TestdResponseV2::Hello { request_id, .. }
                | TestdResponseV2::Accepted { request_id, .. }
                | TestdResponseV2::Event { request_id, .. }
                | TestdResponseV2::Completed { request_id, .. }
                | TestdResponseV2::Refused { request_id, .. } => *request_id,
            };
            if returned_id != expected_id {
                return Err("testd returned a response for a different request\n       fix: restart the daemon before retrying"
                    .into());
            }
            let terminal = matches!(
                response,
                TestdResponseV2::Hello { .. }
                    | TestdResponseV2::Completed { .. }
                    | TestdResponseV2::Refused { .. }
            ) || (!run_request && matches!(response, TestdResponseV2::Event { .. }));
            responses.push(response);
            if terminal {
                return Ok(responses);
            }
        }
    }

    fn ensure_run_directory(&self) -> Result<()> {
        jails_support::apply::ensure_runtime_directory(&self.root)?;
        Ok(())
    }

    fn write_metadata(&self, meta: &Metadata) -> Result<()> {
        jails_support::apply::put_runtime_state(&self.root, &self.meta, meta.render().as_bytes())
    }

    fn metadata(&self) -> Result<Metadata> {
        let text = std::fs::read_to_string(&self.meta)
            .map_err(|error| format!("testd metadata is unavailable: {error}"))?;
        Metadata::parse(&text)
    }

    fn remove_runtime_files(&self) {
        for path in [&self.socket, &self.meta, &self.source] {
            if let Err(error) = jails_support::apply::remove_runtime_state(&self.root, path) {
                eprintln!("testd: could not remove {}: {error}", path.display());
            }
        }
    }

    fn daemon_source(&self) -> Result<PathBuf> {
        let source = rendered_daemon_source();
        if std::fs::read_to_string(&self.source).ok().as_deref() != Some(source.as_str()) {
            jails_support::apply::put_runtime_state(&self.root, &self.source, source.as_bytes())?;
        }
        Ok(self.source.clone())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Metadata {
    project: ObjectId,
    classpath: ObjectId,
    pid: u32,
    started_ms: u64,
    cookie: SecretBytes,
}

impl Metadata {
    fn render(&self) -> String {
        format!(
            "schema=jails.testd-v2.meta.v1\nprotocol_min={}\nprotocol_max={}\nproject={}\nclasspath={}\npid={}\nstarted_ms={}\ncookie={}\n",
            TESTD_V2_PROTOCOL_MIN,
            TESTD_V2_PROTOCOL_MAX,
            self.project,
            self.classpath,
            self.pid,
            self.started_ms,
            hex(self.cookie.expose())
        )
    }

    fn parse(text: &str) -> Result<Self> {
        let field = |name: &str| -> Result<&str> {
            text.lines()
                .find_map(|line| line.strip_prefix(&format!("{name}=")))
                .ok_or_else(|| {
                    format!("testd metadata is missing `{name}`\n       fix: restart the daemon")
                        .into()
                })
        };
        if field("schema")? != "jails.testd-v2.meta.v1"
            || field("protocol_min")? != TESTD_V2_PROTOCOL_MIN.to_string()
            || field("protocol_max")? != TESTD_V2_PROTOCOL_MAX.to_string()
        {
            return Err("testd metadata uses an incompatible protocol\n       fix: restart the daemon with this jails version".into());
        }
        Ok(Self {
            project: ObjectId::parse_hex(field("project")?)?,
            classpath: ObjectId::parse_hex(field("classpath")?)?,
            pid: field("pid")?
                .parse()
                .map_err(|_| "testd metadata has an invalid pid\n       fix: restart the daemon")?,
            started_ms: field("started_ms")?.parse().map_err(
                |_| "testd metadata has an invalid start time\n       fix: restart the daemon",
            )?,
            cookie: SecretBytes::from_bytes(unhex(field("cookie")?)?),
        })
    }
}

fn read_frame<T: jails_support::codec::Codec>(stream: &mut UnixStream) -> Result<T> {
    let mut header = [0u8; 4];
    stream
        .read_exact(&mut header)
        .map_err(|error| format!("testd reply header is truncated: {error}"))?;
    let length = u32::from_be_bytes(header) as usize;
    if length > TESTD_V2_MAX_PAYLOAD {
        return Err(format!(
            "testd reply exceeds the {TESTD_V2_MAX_PAYLOAD}-byte limit\n       fix: restart the daemon with a matching jails version"
        )
        .into());
    }
    let mut frame = Vec::with_capacity(4 + length);
    frame.extend_from_slice(&header);
    frame.resize(4 + length, 0);
    stream
        .read_exact(&mut frame[4..])
        .map_err(|error| format!("testd reply payload is truncated: {error}"))?;
    decode_frame(&frame)
}

fn classpath_id(classpath: &launcher::TestClasspath) -> ObjectId {
    let mut bytes = Vec::new();
    for path in classpath.outputs.iter().chain(&classpath.dependencies) {
        let path = path.to_string_lossy();
        bytes.extend_from_slice(&(path.len() as u64).to_be_bytes());
        bytes.extend_from_slice(path.as_bytes());
    }
    ObjectId::from_bytes(domain_hash("JAILS-TESTD-CLASSPATH-2", &bytes))
}

fn output_snapshot(root: &Path, classpath: &launcher::TestClasspath) -> Result<OutputSnapshotV1> {
    let mut entries = Vec::new();
    for output in &classpath.outputs {
        let mut stack = vec![output.clone()];
        while let Some(dir) = stack.pop() {
            let Ok(children) = std::fs::read_dir(&dir) else {
                continue;
            };
            for child in children.flatten() {
                let path = child.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                let relative = path.strip_prefix(root).map_err(|_| {
                    format!(
                        "test output {} escapes the project\n       fix: regenerate the build classpath",
                        path.display()
                    )
                })?;
                let bytes = std::fs::read(&path)
                    .map_err(|error| format!("failed to snapshot {}: {error}", path.display()))?;
                let metadata = child
                    .metadata()
                    .map_err(|error| format!("failed to inspect {}: {error}", path.display()))?;
                entries.push(OutputEntryV1 {
                    path: OutputPath::parse(&relative.to_string_lossy().replace('\\', "/"))?,
                    size: metadata.len(),
                    modified_ns: metadata
                        .modified()
                        .ok()
                        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                        .map(|duration| u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX))
                        .unwrap_or(0),
                    digest: ObjectId::from_bytes(sha256(&bytes)),
                });
            }
        }
    }
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    let snapshot = OutputSnapshotV1 { entries };
    snapshot.validate()?;
    Ok(snapshot)
}

fn request_id() -> Result<RequestId> {
    random_bytes().map(RequestId::from_bytes)
}

fn secret() -> Result<SecretBytes> {
    random_bytes().map(SecretBytes::from_bytes)
}

fn random_bytes() -> Result<[u8; DIGEST_BYTES]> {
    let mut bytes = [0u8; DIGEST_BYTES];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut random| random.read_exact(&mut bytes))
        .map_err(|error| format!("failed to obtain testd request entropy: {error}"))?;
    Ok(bytes)
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub(super) fn rendered_daemon_source() -> String {
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../templates/testd/JailsTestDaemon.java"
    ))
    .replace(
        "@JAILS_TESTD_PROTOCOL_MIN@",
        &TESTD_V2_PROTOCOL_MIN.to_string(),
    )
    .replace(
        "@JAILS_TESTD_PROTOCOL_MAX@",
        &TESTD_V2_PROTOCOL_MAX.to_string(),
    )
}

fn indent(text: &str) -> String {
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| format!("  {line}\n"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_round_trips_without_exposing_cookie_in_debug() {
        let metadata = Metadata {
            project: ObjectId::from_bytes([1; 32]),
            classpath: ObjectId::from_bytes([2; 32]),
            pid: 42,
            started_ms: 99,
            cookie: SecretBytes::from_bytes([3; 32]),
        };
        assert_eq!(Metadata::parse(&metadata.render()).unwrap(), metadata);
        assert!(!format!("{metadata:?}").contains("03030303"));
    }

    #[test]
    fn socket_and_metadata_live_under_the_project_run_directory() {
        let root = jails_support::scratch::ScratchDir::in_temp("testd-v2-path").unwrap();
        std::fs::create_dir_all(root.path().join("project")).unwrap();
        let project = Project::inspect(&root.path().join("project")).unwrap();
        let client = Client::for_project(&project).unwrap();
        assert_eq!(
            client.socket,
            project.root().join(".jails/run/testd-v2.sock")
        );
        assert_eq!(client.meta, project.root().join(".jails/run/testd-v2.meta"));
        assert_eq!(
            client.source,
            project.root().join(".jails/run/testd-v2.java")
        );
    }
}
