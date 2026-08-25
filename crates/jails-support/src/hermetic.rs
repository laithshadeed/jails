//! Running an external tool without letting it escape.
//!
//! ## Why it is not called `runner`
//!
//! It was, and it sat next to [`crate::process`] -- which runs a program with
//! the reader's terminal, inherited environment and no timeout. Two
//! near-identical names for two different safety contracts is how a caller
//! reaches for the wrong one. This module's contract is in its name now:
//! bounded time, bounded output, nothing inherited. `pending.md` §7.5.
//!
//! ## Why this is not `Command::output()`
//!
//! Three things go wrong with the obvious version, and all three have been
//! seen in the wild.
//!
//! A tool that writes more than a pipe buffer holds and is not being drained
//! **deadlocks**: it blocks on write, the parent blocks on wait, and neither
//! moves. So two reader threads drain stdout and stderr continuously,
//! independently of the wait.
//!
//! A tool that spawns children and is killed by pid leaves **the children
//! running**. Maven forks a JVM; killing Maven leaves the JVM holding the
//! scratch directory jails is about to delete. So every tool starts in a new
//! process group and the signal goes to the group.
//!
//! A tool that ignores `SIGTERM` and is then abandoned looks like a
//! **success**. plan.md §R3.3 is explicit that failure to kill or wait is a
//! refusal, never a detached success — so the escalation is `SIGTERM` to the
//! group, poll for two seconds, `SIGKILL` to the group, wait the direct
//! child, and require the group to be gone before returning.
//!
//! ## Why the environment is built rather than filtered
//!
//! An inherited environment is whatever the operator happened to export, and
//! a formatter that reads `MAVEN_OPTS` or a locale-dependent `LC_ALL` from it
//! produces different bytes on two machines for reasons nothing records.
//! `runner_schema` fixes the exact key set; changing it needs a new schema
//! value, because a tool that saw a different environment is a different
//! tool.

use crate::Result;
use nix::sys::signal::{Signal, killpg};
use nix::unistd::Pid;
use std::collections::BTreeMap;
use std::io::Read;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// The environment and process rules one runner version guarantees.
///
/// §R3.3: *"changing any of those rules requires a new schema value"* — the
/// minimal environment keys, the relative working directory, the stdin
/// policy, the output caps and the process-group behaviour are all part of
/// what a tool's identity means.
pub const RUNNER_SCHEMA: u32 = 1;

/// At most this much of each stream is kept. Beyond it the bytes are dropped
/// and a truncation bit is set — a diagnostic that grows without bound is a
/// diagnostic nobody reads and a memory profile nobody predicted.
pub(crate) const MAX_STREAM_BYTES: usize = 64 * 1024;

/// How long a terminated group gets to exit before it is killed.
const GRACE: Duration = Duration::from_secs(2);

/// One captured stream.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Captured {
    pub bytes: Vec<u8>,
    /// Whether bytes were dropped. Recorded rather than inferred from the
    /// length, so a stream that happens to be exactly the cap is not reported
    /// as truncated.
    pub truncated: bool,
}

/// What happened to one invocation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Outcome {
    Exited { code: i32 },
    Signalled { signal: i32 },
    TimedOut,
}

/// One completed run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Run {
    pub outcome: Outcome,
    pub stdout: Captured,
    pub stderr: Captured,
}

impl Run {
    pub fn succeeded(&self) -> bool {
        matches!(self.outcome, Outcome::Exited { code: 0 })
    }
}

/// Everything one invocation needs, with nothing inherited.
#[derive(Clone, Debug)]
pub struct Invocation {
    pub program: PathBuf,
    pub args: Vec<String>,
    /// The working directory. §R3.3 makes this the scratch `project` child,
    /// so every persisted argv can be a relative path.
    pub working_directory: PathBuf,
    /// The complete environment. Nothing is inherited.
    pub environment: BTreeMap<String, String>,
    pub timeout: Duration,
}

impl Invocation {
    /// The minimal environment §R3.3 specifies: a `PATH`, a forced `C`
    /// locale, and whatever cache or home the tool needs, under scratch.
    pub fn minimal_environment(path: &str, extra: &[(&str, &str)]) -> BTreeMap<String, String> {
        let mut environment = BTreeMap::from([
            ("PATH".to_string(), path.to_string()),
            ("LC_ALL".to_string(), "C".to_string()),
            ("LANG".to_string(), "C".to_string()),
        ]);
        for (key, value) in extra {
            environment.insert((*key).to_string(), (*value).to_string());
        }
        environment
    }
}

/// Run one tool to completion, or kill it and say so.
pub fn run(invocation: &Invocation) -> Result<Run> {
    let mut command = Command::new(&invocation.program);
    command
        .args(&invocation.args)
        .current_dir(&invocation.working_directory)
        .env_clear()
        .envs(&invocation.environment)
        // No stdin at all. A tool that prompts would otherwise block until
        // the timeout and report as a hang rather than as a misconfiguration.
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // Its own group, so the signal reaches the children it forks.
    command.process_group(0);

    let mut child = command
        .spawn()
        .map_err(|error| format!("could not start {}: {error}", invocation.program.display()))?;
    let pid = child.id() as i32;

    let stdout = child.stdout.take().expect("piped");
    let stderr = child.stderr.take().expect("piped");
    let stdout = std::thread::spawn(move || drain(stdout));
    let stderr = std::thread::spawn(move || drain(stderr));

    let deadline = Instant::now() + invocation.timeout;
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(error) => return Err(format!("could not wait for the tool: {error}").into()),
        }
        if Instant::now() >= deadline {
            timed_out = true;
            terminate(pid, &mut child)?;
            break child
                .wait()
                .map_err(|error| format!("could not reap the tool: {error}"))?;
        }
        std::thread::sleep(Duration::from_millis(10));
    };

    let stdout = stdout.join().map_err(|_| "stdout reader panicked")??;
    let stderr = stderr.join().map_err(|_| "stderr reader panicked")??;

    let outcome = if timed_out {
        Outcome::TimedOut
    } else {
        match status.code() {
            Some(code) => Outcome::Exited { code },
            None => Outcome::Signalled {
                signal: signal_of(&status),
            },
        }
    };
    Ok(Run {
        outcome,
        stdout,
        stderr,
    })
}

/// `SIGTERM` to the group, two seconds, then `SIGKILL` to the group.
///
/// `ESRCH` is success: it means the group is already gone, which is the
/// outcome being asked for.
fn terminate(pid: i32, child: &mut std::process::Child) -> Result<()> {
    let group = Pid::from_raw(pid);
    signal_group(group, Signal::SIGTERM)?;
    let deadline = Instant::now() + GRACE;
    while Instant::now() < deadline {
        match child.try_wait() {
            Ok(Some(_)) => return Ok(()),
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(error) => return Err(format!("could not wait for the tool: {error}").into()),
        }
    }
    signal_group(group, Signal::SIGKILL)
}

fn signal_group(group: Pid, signal: Signal) -> Result<()> {
    match killpg(group, signal) {
        Ok(()) => Ok(()),
        Err(nix::errno::Errno::ESRCH) => Ok(()),
        Err(error) => Err(format!(
            "could not signal the tool's process group: {error}.\n       fix: a tool that \
             cannot be stopped is a refusal, never a detached success."
        )
        .into()),
    }
}

fn signal_of(status: &std::process::ExitStatus) -> i32 {
    use std::os::unix::process::ExitStatusExt;
    status.signal().unwrap_or(0)
}

/// Drain one stream to the cap, discarding the rest.
fn drain(mut stream: impl Read) -> Result<Captured> {
    let mut captured = Captured::default();
    let mut buffer = [0u8; 8192];
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => {
                let room = MAX_STREAM_BYTES.saturating_sub(captured.bytes.len());
                if room == 0 {
                    captured.truncated = true;
                    continue;
                }
                let take = room.min(read);
                captured.bytes.extend_from_slice(&buffer[..take]);
                if take < read {
                    captured.truncated = true;
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) => return Err(format!("could not read the tool's output: {error}").into()),
        }
    }
    Ok(captured)
}

/// A diagnostic summary of a failed run, built deterministically.
///
/// §R3.3 and §R3.1 both insist this is *not* raw subprocess output: decode
/// with replacement, normalise line endings, drop control characters, redact
/// every absolute path the operator's machine happens to use, and truncate at
/// a character boundary. A summary that carried a home directory or a
/// `MAVEN_OPTS` value would put it into a journal that outlives the run.
pub fn summarise(run: &Run, redact: &[&Path], limit: usize) -> String {
    let mut text = String::from_utf8_lossy(&run.stderr.bytes).to_string();
    if text.trim().is_empty() {
        text = String::from_utf8_lossy(&run.stdout.bytes).to_string();
    }
    text = text.replace("\r\n", "\n").replace('\r', "\n");
    for path in redact {
        text = text.replace(&path.display().to_string(), "<path>");
    }
    let mut cleaned: String = text
        .chars()
        .map(|c| {
            if c == '\n' || c == '\t' || !c.is_control() {
                c
            } else {
                ' '
            }
        })
        .collect();
    if cleaned.len() > limit {
        let mut cut = limit;
        while cut > 0 && !cleaned.is_char_boundary(cut) {
            cut -= 1;
        }
        cleaned.truncate(cut);
        cleaned.push_str("…[truncated]");
    }
    cleaned
}

#[cfg(test)]
mod tests {
    use super::*;

    /// How long a tool gets before the escalation starts, for the tests that
    /// are not about the timeout. Production callers all pass their own: a
    /// default nobody defaults to is a number that drifts from every real one.
    const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);

    fn sh(script: &str, timeout: Duration) -> Invocation {
        Invocation {
            program: PathBuf::from("/bin/sh"),
            args: vec!["-c".to_string(), script.to_string()],
            working_directory: PathBuf::from("/tmp"),
            environment: Invocation::minimal_environment("/usr/bin:/bin", &[]),
            timeout,
        }
    }

    #[test]
    fn a_successful_run_reports_its_output() {
        let run = run(&sh("printf hello", DEFAULT_TIMEOUT)).unwrap();
        assert!(run.succeeded());
        assert_eq!(run.stdout.bytes, b"hello");
        assert!(!run.stdout.truncated);
    }

    #[test]
    fn a_failing_run_reports_its_code_and_stderr() {
        let run = run(&sh("echo boom >&2; exit 3", DEFAULT_TIMEOUT)).unwrap();
        assert_eq!(run.outcome, Outcome::Exited { code: 3 });
        assert_eq!(run.stderr.bytes, b"boom\n");
    }

    /// The deadlock this module exists for: a tool that writes more than a
    /// pipe buffer holds while nobody drains it blocks forever.
    #[test]
    fn a_tool_that_outwrites_the_pipe_buffer_does_not_deadlock() {
        let run = run(&sh(
            "i=0; while [ $i -lt 2000 ]; do echo aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa; \
             i=$((i+1)); done",
            DEFAULT_TIMEOUT,
        ))
        .unwrap();
        assert!(run.succeeded());
        assert_eq!(run.stdout.bytes.len(), MAX_STREAM_BYTES);
        assert!(run.stdout.truncated, "the cap was reached and not recorded");
    }

    /// A tool that ignores SIGTERM must still be gone when this returns.
    /// Abandoning it and reporting success is the failure §R3.3 names.
    #[test]
    fn a_tool_that_ignores_sigterm_is_killed_and_reported_as_timed_out() {
        let run = run(&sh(
            "trap '' TERM; while true; do sleep 0.1; done",
            Duration::from_millis(300),
        ))
        .unwrap();
        assert_eq!(run.outcome, Outcome::TimedOut);
    }

    /// Maven forks a JVM; killing Maven by pid would leave it holding the
    /// scratch directory jails is about to delete.
    #[test]
    fn a_child_the_tool_forked_is_killed_with_the_group() {
        let marker =
            std::env::temp_dir().join(format!("jails-runner-group-{}", std::process::id()));
        let _ = std::fs::remove_file(&marker);
        let script = format!(
            "sh -c 'sleep 3; : > {}' & trap '' TERM; while true; do sleep 0.1; done",
            marker.display()
        );
        let run = run(&sh(&script, Duration::from_millis(300))).unwrap();
        assert_eq!(run.outcome, Outcome::TimedOut);
        std::thread::sleep(Duration::from_secs(4));
        assert!(
            !marker.exists(),
            "the forked child outlived its group and finished its work"
        );
    }

    /// An inherited environment is whatever the operator happened to export,
    /// and a tool that reads one produces different bytes for reasons nothing
    /// records.
    #[test]
    fn nothing_is_inherited_from_the_parent_environment() {
        unsafe { std::env::set_var("JAILS_RUNNER_LEAK", "visible") };
        let run = run(&sh(
            "printf %s \"${JAILS_RUNNER_LEAK-unset}\"",
            DEFAULT_TIMEOUT,
        ))
        .unwrap();
        assert_eq!(run.stdout.bytes, b"unset");
    }

    /// A tool that prompts would otherwise block until the timeout and be
    /// reported as a hang rather than as a misconfiguration.
    #[test]
    fn a_tool_reading_stdin_sees_end_of_file_immediately() {
        let run = run(&sh("cat; printf done", DEFAULT_TIMEOUT)).unwrap();
        assert!(run.succeeded());
        assert_eq!(run.stdout.bytes, b"done");
    }

    #[test]
    fn a_summary_redacts_absolute_paths_and_truncates_on_a_boundary() {
        let run = Run {
            outcome: Outcome::Exited { code: 1 },
            stderr: Captured {
                bytes: format!(
                    "failed in /home/someone/project\r\nlínea {}",
                    "é".repeat(50)
                )
                .into_bytes(),
                truncated: false,
            },
            stdout: Captured::default(),
        };
        let summary = summarise(&run, &[Path::new("/home/someone/project")], 40);
        assert!(summary.contains("<path>"), "{summary}");
        assert!(!summary.contains("/home/someone"), "{summary}");
        assert!(!summary.contains('\r'), "{summary}");
        assert!(summary.ends_with("…[truncated]"), "{summary}");
    }
}
