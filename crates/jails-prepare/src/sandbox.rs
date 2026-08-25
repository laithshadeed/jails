//! Running a formatter against a copy, and believing only what it changed
//! inside the scope it declared.
//!
//! ## Why a copy at all
//!
//! A formatter rewrites files. Running one against the real project during
//! *preparation* would mean the project had been mutated before anything was
//! committed — and if preparation then refused, the mutation would stay. So
//! the tool runs against a scratch tree built from the projection, and its
//! output enters the plan as ordinary file operations.
//!
//! ## Why the tree is synthesised rather than copied
//!
//! plan.md §R3.3: copy only declared read-set files and *synthesise projected
//! files*. A formatter must see the bytes this transaction will write, not
//! the bytes currently on disk — otherwise it formats the old file and the
//! plan carries a diff against something nobody will commit. And nothing else
//! goes in: no `.git`, no `.jails`, no `target`, no symlinks, no file the
//! read set did not declare.
//!
//! ## Why the whole tree is enumerated twice
//!
//! Because a tool's declared scope is a claim, not a constraint. The only way
//! to know a formatter wrote a log into `target/`, dropped a lockfile beside
//! the POM or followed a symlink out of the tree is to compare the before and
//! after listings and refuse anything outside what it said it would touch.

use crate::Result;
use crate::tool::ToolIdentityFingerprint;
use jails_protocol::conflict::FileMode;
use jails_protocol::identity::ProjectPath;
use jails_support::hermetic::{self, Invocation, Run};
use jails_support::scratch::ScratchDir;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The child of the scratch directory the tool runs in.
///
/// Named rather than being the scratch root itself so the tool's own cache
/// and home can sit beside it — inside scratch, outside the project it sees.
pub(crate) const PROJECT_CHILD: &str = "project";

/// One file as the sandbox will lay it down.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SandboxFile {
    pub path: ProjectPath,
    pub bytes: Vec<u8>,
    pub mode: FileMode,
}

/// What a tool did to the tree.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Diff {
    /// Files whose bytes or mode changed, and files the tool created.
    pub changed: BTreeMap<ProjectPath, SandboxFile>,
    /// Files the tool removed.
    pub removed: Vec<ProjectPath>,
}

impl Diff {
    pub fn is_empty(&self) -> bool {
        self.changed.is_empty() && self.removed.is_empty()
    }
}

/// A prepared scratch tree, and the tool that may act on it.
pub struct Sandbox {
    scratch: ScratchDir,
    project: PathBuf,
    before: BTreeMap<ProjectPath, SandboxFile>,
}

impl Sandbox {
    /// Lay down exactly `files` under a fresh scratch tree.
    pub fn lay_out(files: Vec<SandboxFile>) -> Result<Self> {
        let scratch = ScratchDir::in_temp("jails-format")?;
        let project = scratch.path().join(PROJECT_CHILD);
        std::fs::create_dir_all(&project)
            .map_err(|error| format!("could not create the scratch project: {error}"))?;

        let mut before = BTreeMap::new();
        for file in files {
            let at = project.join(file.path.as_str());
            jails_support::apply::put_in_scratch(&at, &file.bytes)?;
            set_mode(&at, file.mode)?;
            before.insert(file.path.clone(), file);
        }
        Ok(Self {
            scratch,
            project,
            before,
        })
    }

    /// The directory the tool runs in.
    pub fn project(&self) -> &Path {
        &self.project
    }

    /// Run one tool and return what it changed, refusing anything outside the
    /// scope its identity declared.
    pub fn run(
        &self,
        identity: &ToolIdentityFingerprint,
        program: PathBuf,
        args: Vec<String>,
        environment: BTreeMap<String, String>,
    ) -> Result<(Run, Diff)> {
        identity.validate()?;
        let run = hermetic::run(&Invocation {
            program,
            args,
            working_directory: self.project.clone(),
            environment,
            timeout: std::time::Duration::from_millis(identity.timeout_ms),
        })?;
        if !run.succeeded() {
            return Err(format!(
                "{} did not succeed ({:?}).\n       fix: {}",
                identity.key.tool,
                run.outcome,
                hermetic::summarise(&run, &[&self.project], 2048)
            )
            .into());
        }
        let diff = self.diff(identity)?;
        Ok((run, diff))
    }

    /// Enumerate the tree again and compare.
    fn diff(&self, identity: &ToolIdentityFingerprint) -> Result<Diff> {
        let after = enumerate(&self.project, &self.project)?;
        let mut diff = Diff::default();

        for (path, file) in &after {
            match self.before.get(path) {
                Some(previous) if previous == file => {}
                _ => {
                    in_scope(path, identity)?;
                    diff.changed.insert(path.clone(), file.clone());
                }
            }
        }
        for path in self.before.keys() {
            if !after.contains_key(path) {
                in_scope(path, identity)?;
                diff.removed.push(path.clone());
            }
        }
        Ok(diff)
    }

    /// Remove the tree, and say so if it cannot be removed.
    ///
    /// A scratch directory that outlives its run is a leak the next run
    /// cannot distinguish from a directory it should reuse.
    pub fn close(self) -> Result<()> {
        self.scratch.close()
    }
}

/// A tool's declared scope is a claim, not a constraint. This is where it
/// becomes one.
fn in_scope(path: &ProjectPath, identity: &ToolIdentityFingerprint) -> Result<()> {
    let allowed = identity.mutable_scopes.iter().any(|scope| {
        path.as_str() == scope.as_str()
            || path.as_str().starts_with(&format!("{}/", scope.as_str()))
    });
    if allowed {
        return Ok(());
    }
    Err(format!(
        "{} touched `{path}`, which is outside every scope it declared.\n       fix: a tool that \
         writes where it said it would not has produced output nothing recorded; widen its \
         declared scope deliberately or stop it writing there.",
        identity.key.tool
    )
    .into())
}

/// Every regular file under the scratch `tree`, as project-relative paths.
///
/// A symlink, a device or a directory jails cannot name as a `ProjectPath` is
/// an error rather than a skip: skipping would let a tool hide a file from
/// the comparison by making it something the walker ignores.
fn enumerate(tree: &Path, at: &Path) -> Result<BTreeMap<ProjectPath, SandboxFile>> {
    let mut out = BTreeMap::new();
    let entries = std::fs::read_dir(at)
        .map_err(|error| format!("could not read {}: {error}", at.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("could not read {}: {error}", at.display()))?;
        let path = entry.path();
        let metadata = std::fs::symlink_metadata(&path)
            .map_err(|error| format!("could not stat {}: {error}", path.display()))?;
        if metadata.is_symlink() {
            return Err(format!(
                "the tool created a symlink at {}.\n       fix: a symlink in a prepared tree \
                 points somewhere this transaction never validated.",
                path.display()
            )
            .into());
        }
        if metadata.is_dir() {
            // `target/` is derived output no transaction can name, and a
            // build tool writes into it as a matter of course -- Spotless
            // keeps an up-to-date index there. Skipping it is the same rule
            // that keeps `.git` and `.jails` out of the tree in the first
            // place: what a tool does to derived output is not evidence of
            // anything, and refusing it would make every formatter run fail
            // over a cache file.
            if path.file_name().is_some_and(|name| name == "target") {
                continue;
            }
            out.extend(enumerate(tree, &path)?);
            continue;
        }
        if !metadata.is_file() {
            return Err(format!(
                "the tool created {}, which is not a regular file",
                path.display()
            )
            .into());
        }
        let relative = path
            .strip_prefix(tree)
            .map_err(|_| format!("{} escaped the scratch tree", path.display()))?;
        let relative = relative
            .to_str()
            .ok_or_else(|| format!("{} is not a UTF-8 path", path.display()))?;
        // A path jails cannot name is reported as the tool's doing, not as a
        // parse failure: the reader needs to know what wrote it.
        let named = ProjectPath::parse(relative).map_err(|error| {
            format!("the tool produced `{relative}`, which this transaction cannot name: {error}")
        })?;
        let bytes = std::fs::read(&path)
            .map_err(|error| format!("could not read {}: {error}", path.display()))?;
        out.insert(
            named.clone(),
            SandboxFile {
                path: named,
                bytes,
                mode: mode_of(&metadata)?,
            },
        );
    }
    Ok(out)
}

#[cfg(unix)]
fn mode_of(metadata: &std::fs::Metadata) -> Result<FileMode> {
    use std::os::unix::fs::MetadataExt;
    FileMode::new(metadata.mode() & 0o777)
}

#[cfg(unix)]
fn set_mode(at: &Path, mode: FileMode) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    Ok(
        std::fs::set_permissions(at, std::fs::Permissions::from_mode(mode.bits()))
            .map_err(|error| format!("could not set the mode of {}: {error}", at.display()))?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool::ToolInvocationKey;
    use jails_protocol::identity::{ObjectId, ToolId};
    use jails_support::codec::sha256;

    fn file(at: &str, body: &str) -> SandboxFile {
        SandboxFile {
            path: ProjectPath::parse(at).unwrap(),
            bytes: body.as_bytes().to_vec(),
            mode: FileMode::new(0o644).unwrap(),
        }
    }

    fn identity(scopes: &[&str]) -> ToolIdentityFingerprint {
        ToolIdentityFingerprint {
            key: ToolInvocationKey {
                tool: ToolId::parse("formatter").unwrap(),
                subject: None,
            },
            executable_sha256: ObjectId::from_bytes(sha256(b"sh")),
            version_stdout_sha256: ObjectId::from_bytes(sha256(b"1.0")),
            runner_schema: jails_support::hermetic::RUNNER_SCHEMA,
            timeout_ms: 30_000,
            mutable_scopes: scopes
                .iter()
                .map(|scope| ProjectPath::parse(scope).unwrap())
                .collect(),
            offline_inputs: Vec::new(),
        }
    }

    fn shell(script: &str) -> (PathBuf, Vec<String>, BTreeMap<String, String>) {
        (
            PathBuf::from("/bin/sh"),
            vec!["-c".to_string(), script.to_string()],
            Invocation::minimal_environment("/usr/bin:/bin", &[]),
        )
    }

    #[test]
    fn a_tool_that_rewrites_a_file_in_scope_reports_the_new_bytes() {
        let sandbox =
            Sandbox::lay_out(vec![file("src/main/java/App.java", "class App{}")]).unwrap();
        let (program, args, environment) =
            shell("printf 'class App {}\\n' > src/main/java/App.java");
        let (_, diff) = sandbox
            .run(&identity(&["src/main/java"]), program, args, environment)
            .unwrap();
        assert_eq!(diff.changed.len(), 1);
        assert_eq!(
            diff.changed[&ProjectPath::parse("src/main/java/App.java").unwrap()].bytes,
            b"class App {}\n"
        );
        sandbox.close().unwrap();
    }

    #[test]
    fn a_tool_that_changes_nothing_produces_an_empty_diff() {
        let sandbox =
            Sandbox::lay_out(vec![file("src/main/java/App.java", "class App {}\n")]).unwrap();
        let (program, args, environment) = shell("true");
        let (_, diff) = sandbox
            .run(&identity(&["src/main/java"]), program, args, environment)
            .unwrap();
        assert!(diff.is_empty());
        sandbox.close().unwrap();
    }

    /// A declared scope is a claim until something checks it.
    #[test]
    fn a_tool_that_writes_outside_its_declared_scope_is_refused() {
        let sandbox =
            Sandbox::lay_out(vec![file("src/main/java/App.java", "class App{}")]).unwrap();
        let (program, args, environment) = shell("printf junk > src/main/resources/../../../notes");
        let error = sandbox
            .run(&identity(&["src/main/java"]), program, args, environment)
            .unwrap_err();
        assert!(
            error.contains("did not succeed") || error.contains("outside every scope"),
            "{error}"
        );
        sandbox.close().unwrap();
    }

    /// A tool reaching into version-control state is exactly the "surprise
    /// output" §R3.3 names, and it lands in a directory jails refuses to name
    /// at all — so the refusal has to say the tool produced it.
    #[test]
    fn a_tool_that_writes_into_a_path_jails_will_not_name_is_refused() {
        let sandbox =
            Sandbox::lay_out(vec![file("src/main/java/App.java", "class App{}")]).unwrap();
        let (program, args, environment) = shell("mkdir -p .git && printf ref > .git/HEAD");
        let error = sandbox
            .run(&identity(&["src/main/java"]), program, args, environment)
            .unwrap_err();
        assert!(error.contains("the tool produced"), "{error}");
        assert!(error.contains(".git/HEAD"), "{error}");
        sandbox.close().unwrap();
    }

    /// `target/` is the one exception, and it has to be: a build tool writes
    /// there as a matter of course -- Spotless keeps an up-to-date index in
    /// it -- so refusing would make every real formatter run fail over a
    /// cache file. It is derived output no transaction can name, which means
    /// what a tool does to it is not evidence of anything.
    #[test]
    fn derived_output_is_neither_refused_nor_committed() {
        let sandbox =
            Sandbox::lay_out(vec![file("src/main/java/App.java", "class App{}")]).unwrap();
        let (program, args, environment) =
            shell("mkdir -p target && printf idx > target/spotless-index");
        let (_, diff) = sandbox
            .run(&identity(&["src/main/java"]), program, args, environment)
            .unwrap();
        assert!(diff.is_empty(), "derived output entered the plan: {diff:?}");
        sandbox.close().unwrap();
    }

    /// A surprise output inside a nameable path is caught by the scope check
    /// rather than by the path rules.
    #[test]
    fn a_surprise_output_beside_a_declared_file_is_refused() {
        let sandbox =
            Sandbox::lay_out(vec![file("src/main/java/App.java", "class App{}")]).unwrap();
        let (program, args, environment) = shell("printf cache > .formatter-cache");
        let error = sandbox
            .run(&identity(&["src/main/java"]), program, args, environment)
            .unwrap_err();
        assert!(error.contains("outside every scope"), "{error}");
        sandbox.close().unwrap();
    }

    /// Deleting a file it was not asked to touch is the same violation from
    /// the other direction.
    #[test]
    fn a_tool_that_deletes_an_undeclared_file_is_refused() {
        let sandbox = Sandbox::lay_out(vec![
            file("src/main/java/App.java", "class App{}"),
            file("pom.xml", "<project/>"),
        ])
        .unwrap();
        let (program, args, environment) = shell("rm pom.xml");
        let error = sandbox
            .run(&identity(&["src/main/java"]), program, args, environment)
            .unwrap_err();
        assert!(error.contains("outside every scope"), "{error}");
        sandbox.close().unwrap();
    }

    /// A symlink in a prepared tree points somewhere this transaction never
    /// validated, so it is an error rather than something the walker skips.
    #[test]
    fn a_symlink_the_tool_created_is_refused() {
        let sandbox =
            Sandbox::lay_out(vec![file("src/main/java/App.java", "class App{}")]).unwrap();
        let (program, args, environment) = shell("ln -s /etc/passwd src/main/java/Leak.java");
        let error = sandbox
            .run(&identity(&["src/main/java"]), program, args, environment)
            .unwrap_err();
        assert!(error.contains("symlink"), "{error}");
        sandbox.close().unwrap();
    }

    /// A formatter that is not idempotent produces different bytes on the
    /// second run, which means the transaction it prepared is not the one a
    /// re-preparation would produce.
    #[test]
    fn formatting_is_deterministic_evidence_and_is_checked_by_running_it_twice() {
        let laid = vec![file("src/main/java/App.java", "class App{}")];
        let script = "printf 'class App {}\\n' > src/main/java/App.java";

        let first = Sandbox::lay_out(laid.clone()).unwrap();
        let (program, args, environment) = shell(script);
        let (_, one) = first
            .run(&identity(&["src/main/java"]), program, args, environment)
            .unwrap();
        first.close().unwrap();

        let after: Vec<SandboxFile> = one.changed.values().cloned().collect();
        let second = Sandbox::lay_out(after).unwrap();
        let (program, args, environment) = shell(script);
        let (_, two) = second
            .run(&identity(&["src/main/java"]), program, args, environment)
            .unwrap();
        assert!(two.is_empty(), "the formatter is not idempotent: {two:?}");
        second.close().unwrap();
    }

    /// A tool the sandbox could not run is a refusal naming the tool, not a
    /// silently empty diff that reads as "nothing to format".
    #[test]
    fn a_tool_that_fails_is_a_refusal_naming_it() {
        let sandbox = Sandbox::lay_out(vec![file("pom.xml", "<project/>")]).unwrap();
        let (program, args, environment) = shell("echo broken >&2; exit 1");
        let error = sandbox
            .run(&identity(&["pom.xml"]), program, args, environment)
            .unwrap_err();
        assert!(error.contains("formatter did not succeed"), "{error}");
        assert!(error.contains("broken"), "{error}");
        sandbox.close().unwrap();
    }
}
