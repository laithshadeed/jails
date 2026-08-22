//! One place that builds and runs a child process.
//!
//! Process construction was spread across `run`, `compose`, `new`, `doctor`,
//! `why`, `kafka`, `migrate` and `console`, and each site decided for itself
//! how to find the tool, whether `--debug` prints, and what a non-zero exit
//! means. That is how the two bugs this module was extracted to fix happened:
//!
//! - `run.rs` and `project.rs` disagreed about whether mvnd is `mvnd` or
//!   `mvnd.cmd`, so `jails about` named a Maven command `jails test` would
//!   not run.
//! - `compose.rs` falls back to the standalone `docker-compose` binary when
//!   `docker` is absent, while `doctor.rs` hardcoded `docker` in all three of
//!   its probes -- so on such a machine `jails start` worked and `doctor`
//!   reported Docker missing.
//!
//! Both are the same shape: two copies of "which tool is this" that drifted.
//! Tool resolution lives here now, so a probe reports the tool that will
//! actually be used.
//!
//! ## What this is not
//!
//! Not a trait, not one abstraction per tool, and not async. A concrete type
//! and a synchronous executor -- a fake executor has no consumer today, and
//! waiting on a child does not need a runtime.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};

use crate::Result;

/// Whether jails prints the command it is about to run.
///
/// **Observability only.** `Debug` prints *and then runs*: a diagnostic flag
/// that also decided whether the work happened is how `jails --debug migrate`
/// came to report "applied cleanly" over SQL that never reached a database.
/// Preview is a separate concept and lives in the planning layer, which
/// decides not to call this at all.
#[derive(Copy, Clone, PartialEq, Eq)]
pub(crate) enum Diagnostics {
    Normal,
    Debug,
}

impl Diagnostics {
    pub(crate) fn from_flag(debug: bool) -> Self {
        if debug { Self::Debug } else { Self::Normal }
    }
}

/// What to do with the child's stdout/stderr.
#[derive(Copy, Clone, PartialEq, Eq)]
pub(crate) enum OutputMode {
    /// Hand the child our streams. Interactive tools need this.
    Inherit,
    /// Capture both for the caller to inspect.
    Capture,
    /// Echo both streams live and retain their tail for diagnostics.
    ///
    /// Maven failures need their output twice: once immediately for the
    /// person watching the build, and once after exit for `jails why` to
    /// explain the root cause. A bounded tail prevents a verbose build from
    /// turning that convenience into unbounded memory use.
    Tee,
}

/// A child process described as data, so it can be asserted on without being
/// run.
pub(crate) struct CommandSpec {
    program: OsString,
    args: Vec<OsString>,
    cwd: Option<PathBuf>,
    env: Vec<(OsString, OsString)>,
    /// Bytes to write to the child's stdin, if any.
    stdin: Option<Vec<u8>>,
    output: OutputMode,
    /// Env var names whose values must never be printed.
    secret_env: Vec<OsString>,
}

impl CommandSpec {
    pub(crate) fn new(program: impl Into<OsString>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            cwd: None,
            env: Vec::new(),
            stdin: None,
            output: OutputMode::Inherit,
            secret_env: Vec::new(),
        }
    }

    pub(crate) fn arg(mut self, arg: impl Into<OsString>) -> Self {
        self.args.push(arg.into());
        self
    }

    /// Arguments are kept as `OsString` end to end. Joining them into one
    /// string and splitting it again loses the boundary of any argument
    /// containing a space, and forwarded arguments (`jails mvn -- ...`) are
    /// exactly where that happens.
    pub(crate) fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.args
            .extend(args.into_iter().map(|a| a.as_ref().to_os_string()));
        self
    }

    pub(crate) fn current_dir(mut self, dir: impl AsRef<Path>) -> Self {
        self.cwd = Some(dir.as_ref().to_path_buf());
        self
    }

    pub(crate) fn env(mut self, key: impl Into<OsString>, value: impl Into<OsString>) -> Self {
        self.env.push((key.into(), value.into()));
        self
    }

    /// An environment variable whose *value* must never appear in debug
    /// output. `PGPASSWORD` is passed to psql this way.
    pub(crate) fn secret_env(
        mut self,
        key: impl Into<OsString>,
        value: impl Into<OsString>,
    ) -> Self {
        let key = key.into();
        self.secret_env.push(key.clone());
        self.env.push((key, value.into()));
        self
    }

    pub(crate) fn stdin(mut self, input: impl Into<Vec<u8>>) -> Self {
        self.stdin = Some(input.into());
        self
    }

    pub(crate) fn output(mut self, mode: OutputMode) -> Self {
        self.output = mode;
        self
    }

    /// The line `--debug` prints. Secret values are replaced, never shown;
    /// the name is still printed so the reader knows it was set.
    pub(crate) fn render(&self) -> String {
        let mut out = String::from("+ ");
        for (key, value) in &self.env {
            let shown = if self.secret_env.contains(key) || is_always_secret(key) {
                "<redacted>".to_string()
            } else {
                value.to_string_lossy().into_owned()
            };
            out.push_str(&format!("{}={shown} ", key.to_string_lossy()));
        }
        out.push_str(&self.program.to_string_lossy());
        for arg in &self.args {
            out.push(' ');
            out.push_str(&arg.to_string_lossy());
        }
        if let Some(dir) = &self.cwd {
            out.push_str(&format!("  (in {})", dir.display()));
        }
        if self.stdin.is_some() {
            out.push_str("  (with stdin)");
        }
        out
    }

    fn to_command(&self) -> Command {
        let mut cmd = Command::new(&self.program);
        cmd.args(&self.args);
        if let Some(dir) = &self.cwd {
            cmd.current_dir(dir);
        }
        for (key, value) in &self.env {
            cmd.env(key, value);
        }
        cmd
    }
}

/// Environment variables whose values are never printed, whoever set them.
///
/// `secret_env` is the explicit marker, but relying on every call site to
/// remember it is the kind of rule that decays -- and this one decays into
/// printing a password. `console.rs` passes `PGPASSWORD` to psql through a
/// plain `Command`, which reaches debug rendering by way of `run_inherited`.
/// A name-based backstop costs nothing and does not depend on being
/// remembered.
const ALWAYS_SECRET: &[&str] = &["PGPASSWORD", "PGPASSFILE", "MYSQL_PWD"];

fn is_always_secret(key: &OsStr) -> bool {
    ALWAYS_SECRET
        .iter()
        .any(|name| key.eq_ignore_ascii_case(name))
}

/// What a finished child left behind.
#[derive(Debug)]
pub(crate) struct Done {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

impl Done {
    pub(crate) fn stdout_string(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }
}

/// Run a command to completion.
///
/// Returns the outcome rather than deciding what a non-zero exit means --
/// `doctor` treats one as a finding, `run` treats one as a failure, and that
/// is the caller's judgement.
pub(crate) fn run(spec: &CommandSpec, diagnostics: Diagnostics) -> Result<Done> {
    if diagnostics == Diagnostics::Debug {
        eprintln!("{}", spec.render());
    }

    let mut cmd = spec.to_command();
    match spec.output {
        OutputMode::Inherit => {
            cmd.stdout(Stdio::inherit()).stderr(Stdio::inherit());
        }
        OutputMode::Capture | OutputMode::Tee => {
            cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        }
    }
    if spec.stdin.is_some() {
        cmd.stdin(Stdio::piped());
    }

    let program = spec.program.to_string_lossy().into_owned();
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("failed to run {program}: {e}"))?;

    if let Some(input) = &spec.stdin {
        use std::io::Write;
        child
            .stdin
            .as_mut()
            .ok_or_else(|| format!("failed to open stdin to {program}"))?
            .write_all(input)
            .map_err(|e| format!("failed to send input to {program}: {e}"))?;
        // Dropped so the child sees EOF; without this a tool that reads to
        // end of input waits forever.
        drop(child.stdin.take());
    }

    if spec.output != OutputMode::Tee {
        let out = child
            .wait_with_output()
            .map_err(|e| format!("failed to wait for {program}: {e}"))?;
        return Ok(Done {
            status: out.status,
            stdout: out.stdout,
            stderr: out.stderr,
        });
    }

    const DIAGNOSTIC_TAIL_BYTES: usize = 4 * 1024 * 1024;
    fn read_and_tee<R: std::io::Read, W: std::io::Write>(mut reader: R, mut writer: W) -> Vec<u8> {
        let mut captured = std::collections::VecDeque::with_capacity(DIAGNOSTIC_TAIL_BYTES);
        let mut chunk = [0_u8; 8192];
        loop {
            let read = match reader.read(&mut chunk) {
                Ok(0) => break,
                Ok(read) => read,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => break,
            };
            let bytes = &chunk[..read];
            let _ = writer.write_all(bytes);
            let _ = writer.flush();
            let excess = captured
                .len()
                .saturating_add(bytes.len())
                .saturating_sub(DIAGNOSTIC_TAIL_BYTES);
            for _ in 0..excess {
                captured.pop_front();
            }
            captured.extend(bytes);
        }
        captured.into_iter().collect()
    }

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| format!("failed to capture stdout from {program}"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| format!("failed to capture stderr from {program}"))?;
    let stdout_thread = std::thread::spawn(move || read_and_tee(stdout, std::io::stdout()));
    let stderr_thread = std::thread::spawn(move || read_and_tee(stderr, std::io::stderr()));
    let status = child
        .wait()
        .map_err(|e| format!("failed to wait for {program}: {e}"))?;
    let stdout = stdout_thread.join().unwrap_or_default();
    let stderr = stderr_thread.join().unwrap_or_default();
    Ok(Done {
        status,
        stdout,
        stderr,
    })
}

/// Run, and treat a non-zero exit as an error naming the program.
#[cfg(test)]
pub(crate) fn run_checked(spec: &CommandSpec, diagnostics: Diagnostics) -> Result<Done> {
    let done = run(spec, diagnostics)?;
    if !done.status.success() {
        let program = spec.program.to_string_lossy();
        return Err(format!("{program} exited with {}", done.status));
    }
    Ok(done)
}

/// Whether an executable of this name is on PATH.
///
/// The one implementation. `run.rs`, `compose.rs` and `project.rs` each had
/// their own, which is how the mvnd naming drifted between them.
pub(crate) fn on_path(bin: &str) -> bool {
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    on_path_in(bin, std::env::split_paths(&paths))
}

/// The lookup over an explicit list of directories, so it can be tested
/// without touching the process environment.
pub(crate) fn on_path_in(bin: &str, dirs: impl Iterator<Item = PathBuf>) -> bool {
    dirs.into_iter().any(|dir| dir.join(bin).is_file())
}

/// How to invoke Docker Compose here: `docker compose` (v2, a CLI plugin) or
/// the standalone `docker-compose` binary.
///
/// **One resolver, so a probe tests the tool that will actually run.**
/// `compose.rs` fell back to the standalone binary while `doctor.rs`
/// hardcoded `docker`, so on a machine with only `docker-compose` installed
/// `jails start` worked and `doctor` reported Docker missing.
pub(crate) fn compose_program() -> Option<(&'static str, &'static [&'static str])> {
    if on_path("docker") {
        Some(("docker", &["compose"]))
    } else if on_path("docker-compose") {
        Some(("docker-compose", &[]))
    } else {
        None
    }
}

/// A `CommandSpec` for a compose subcommand, or `None` when neither form is
/// installed.
pub(crate) fn compose_spec<I, S>(args: I) -> Option<CommandSpec>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let (program, prefix) = compose_program()?;
    Some(CommandSpec::new(program).args(prefix).args(args))
}

/// The Docker CLI itself (not compose), for probes like `docker info`.
///
/// `docker-compose` is not a Docker CLI, so on a standalone-only machine
/// there is nothing to probe with and callers should say so rather than
/// report a failure that means "not installed".
pub(crate) fn docker_program() -> Option<&'static str> {
    on_path("docker").then_some("docker")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The failure this module exists to prevent, as a test: the value of a
    /// secret must never reach debug output, while the fact that it was set
    /// stays visible.
    #[test]
    fn debug_rendering_redacts_a_secret_value() {
        let spec = CommandSpec::new("psql")
            .args(["-h", "localhost"])
            .secret_env("PGPASSWORD", "hunter2");
        let line = spec.render();
        assert!(!line.contains("hunter2"), "{line}");
        assert!(line.contains("PGPASSWORD=<redacted>"), "{line}");
        assert!(line.contains("psql -h localhost"), "{line}");
    }

    /// A non-secret env value is shown -- redaction is opt-in per variable,
    /// so nothing quietly hides information the reader needs.
    /// The backstop, for a call site that did not say the variable was
    /// secret -- which is how a password reaches debug output in practice.
    #[test]
    fn a_known_secret_is_redacted_even_when_not_marked() {
        let spec = CommandSpec::new("psql").env("PGPASSWORD", "hunter2");
        let line = spec.render();
        assert!(!line.contains("hunter2"), "{line}");
        assert!(line.contains("PGPASSWORD=<redacted>"), "{line}");
    }

    /// Via `run_inherited`'s path: envs copied off a plain `Command` are
    /// rendered too, so the backstop has to hold there.
    #[test]
    fn a_secret_copied_off_a_plain_command_is_redacted() {
        let mut cmd = std::process::Command::new("psql");
        cmd.env("PGPASSWORD", "hunter2");
        let mut spec = CommandSpec::new(cmd.get_program());
        for (key, value) in cmd.get_envs() {
            if let Some(value) = value {
                spec = spec.env(key, value);
            }
        }
        assert!(!spec.render().contains("hunter2"), "{}", spec.render());
    }

    #[test]
    fn a_plain_env_value_is_shown() {
        let spec = CommandSpec::new("mvn").env("MAVEN_OPTS", "-Xmx1g");
        assert!(spec.render().contains("MAVEN_OPTS=-Xmx1g"));
    }

    /// Arguments stay `OsString` end to end, so one containing a space is
    /// still one argument.
    #[test]
    fn an_argument_containing_a_space_stays_one_argument() {
        let spec = CommandSpec::new("git").args(["commit", "-m", "two words"]);
        assert_eq!(spec.args.len(), 3);
        assert_eq!(spec.args[2], OsString::from("two words"));
    }

    #[test]
    fn stdin_and_cwd_are_visible_in_debug_output() {
        let spec = CommandSpec::new("kafka-console-producer")
            .current_dir("/tmp/demo")
            .stdin("a record");
        let line = spec.render();
        assert!(line.contains("(in /tmp/demo)"), "{line}");
        assert!(line.contains("(with stdin)"), "{line}");
    }

    /// Debug prints *and then runs*. A diagnostic flag that also decided
    /// whether the work happened is how `--debug migrate` reported success
    /// over SQL that never ran.
    #[test]
    fn debug_still_executes_the_command() {
        let spec = CommandSpec::new("true").output(OutputMode::Capture);
        let done = run(&spec, Diagnostics::Debug).unwrap();
        assert!(done.status.success());
    }

    #[test]
    fn stdin_is_delivered_and_stdout_captured() {
        let spec = CommandSpec::new("cat")
            .stdin("hello")
            .output(OutputMode::Capture);
        let done = run(&spec, Diagnostics::Normal).unwrap();
        assert_eq!(done.stdout_string(), "hello");
    }

    #[test]
    fn tee_keeps_output_for_a_caller_after_echoing_it() {
        let spec = CommandSpec::new("sh")
            .args(["-c", "printf out; printf err >&2"])
            .output(OutputMode::Tee);
        let done = run(&spec, Diagnostics::Normal).unwrap();
        assert!(done.status.success());
        assert_eq!(done.stdout, b"out");
        assert_eq!(done.stderr, b"err");
    }

    /// A non-zero exit is an outcome, not an error: `doctor` reads one as a
    /// finding. Only `run_checked` turns it into a failure.
    #[test]
    fn a_non_zero_exit_is_an_outcome_not_an_error() {
        let spec = CommandSpec::new("false").output(OutputMode::Capture);
        assert!(!run(&spec, Diagnostics::Normal).unwrap().status.success());
        assert!(run_checked(&spec, Diagnostics::Normal).is_err());
    }

    #[test]
    fn on_path_in_finds_an_executable_in_one_of_the_dirs() {
        let dir = std::env::temp_dir().join(format!("jails-on-path-{}", std::process::id()));
        let other =
            std::env::temp_dir().join(format!("jails-on-path-other-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::create_dir_all(&other).unwrap();
        std::fs::write(dir.join("mvnd"), "").unwrap();

        assert!(on_path_in("mvnd", [other.clone(), dir.clone()].into_iter()));
        assert!(!on_path_in("mvn", [other.clone(), dir.clone()].into_iter()));

        std::fs::remove_dir_all(&dir).ok();
        std::fs::remove_dir_all(&other).ok();
    }

    #[test]
    fn a_missing_program_names_itself_in_the_error() {
        let spec = CommandSpec::new("jails-no-such-program-exists");
        let err = run(&spec, Diagnostics::Normal).unwrap_err();
        assert!(err.contains("jails-no-such-program-exists"), "{err}");
    }
}
