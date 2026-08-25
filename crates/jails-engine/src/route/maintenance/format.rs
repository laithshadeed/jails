//! `jails fmt` — the formatter's output, committed as a reviewed diff.

use super::*;

/// Reformat the project's sources, without letting the formatter near them.
///
/// §R6.4's row is explicit: *"scratch-format and commit exact changed sources.
/// Do not let Maven/Spotless mutate the live source tree directly."* V1 runs
/// `mvn spotless:apply` against the real project, so a formatter that fails
/// halfway leaves some files rewritten and some not, with nothing to say which
/// -- and a formatter that decides to rewrite something outside `src/` has
/// already done it by the time anybody notices.
///
/// Here Spotless runs against a scratch tree synthesised from the projection,
/// its output is diffed against what went in, and what it changed enters the
/// plan as ordinary file operations. Two things fall out that V1 cannot have:
/// anything the formatter touches outside its declared `mutable_scopes` is a
/// refusal rather than a fait accompli, and a run that changes nothing is a
/// transition with no operations rather than a rewrite of every file to
/// identical bytes.
///
/// `resource: None` throughout. Formatting rewrites bytes without changing
/// what any of them mean, so nothing here claims ownership it did not have --
/// a generated file stays its entity's, and a hand-written one stays the
/// reader's.
pub fn format(run: &Run) -> Result<Outcome> {
    let project = run.project();
    // Maven only, and by name rather than by trying and failing. `add format`
    // *does* configure a Gradle build -- the `plugins {}` entry, the
    // `spotless {}` block, and `spotlessCheck` wired into `check` by the
    // plugin itself -- so the formatting is enforced. What is missing is this
    // route's guarantee, not the formatter: it runs the tool in a sandbox laid
    // out from the projection, so the reformat is a reviewed diff committed in
    // the same transaction. Gradle in a throwaway tree needs its wrapper, its
    // caches and a writable `build/`, which is a different bargain and is
    // recorded in `pending.md` rather than half-taken here.
    if project.build() == jails_spec::build::Build::Gradle {
        return Err(jails_support::Failure::Told(
            "`jails fmt` runs the formatter inside a sandbox laid out from this transaction, \
             and it drives that with Maven.\n       fix: `./gradlew spotlessApply` -- `jails \
             add format` has already configured it, and `check` fails on an unformatted \
             file. The command is refused rather than silently doing something else."
                .to_string(),
        ));
    }
    let scope = ProjectPath::parse("src")?;
    let mut reads = capture::capability_reads()?;
    let mut sources = Vec::new();
    for absolute in jails_java::java::source_files(&project.root().join("src")) {
        let relative = super::relative_path(project, &absolute)?;
        reads = reads.file(relative.clone());
        sources.push(relative);
    }
    if sources.is_empty() {
        return Err(jails_support::Failure::Told(
            "no .java file under src/ to format.\n       fix: run this from a project with \
             sources."
                .to_string(),
        ));
    }
    // The pom is read because the formatter is *invoked through it* -- the
    // Spotless plugin's configuration is what decides the result, so a plan
    // made against one pom must not be committed against another.
    reads = reads.file(ProjectPath::parse("pom.xml")?);
    let (snapshot, _) = capture::projected(project, &reads)?;

    // Synthesised from the projection, not copied from disk: a formatter must
    // see the bytes this transaction will write, or the plan carries a diff
    // against something nobody will commit.
    let mut laid_out = Vec::new();
    for path in sources
        .iter()
        .chain(std::iter::once(&ProjectPath::parse("pom.xml")?))
    {
        if let jails_protocol::snapshot::Captured::Present(file) = snapshot.read(path)? {
            laid_out.push(jails_prepare::sandbox::SandboxFile {
                path: path.clone(),
                bytes: file.bytes.to_vec(),
                mode: file.mode,
            });
        }
    }
    // Plain Maven, never the daemon, and not because the daemon is slow. mvnd
    // keeps a registry under the Maven user home and reuses a long-lived
    // process across invocations; a one-shot run in a throwaway tree that is
    // deleted the moment it finishes is the one case where that buys nothing
    // and can leave a daemon holding a directory that no longer exists.
    let program = jails_project::maven::plain(project);
    let identity = jails_prepare::tool::ToolIdentityFingerprint {
        key: jails_prepare::tool::ToolInvocationKey {
            tool: jails_protocol::identity::ToolId::parse("spotless")?,
            // A project-wide formatter is not about one file.
            subject: None,
        },
        executable_sha256: ObjectId::from_bytes(jails_support::codec::sha256(
            program.to_string_lossy().as_bytes(),
        )),
        version_stdout_sha256: ObjectId::from_bytes(jails_support::codec::sha256(b"spotless")),
        runner_schema: 1,
        timeout_ms: 300_000,
        // The whole point of the fingerprint: a formatter that writes outside
        // this is refused, and widening it changes the identity, so a policy
        // cannot be loosened without the change being visible.
        mutable_scopes: BTreeSet::from([scope]),
        offline_inputs: Vec::new(),
    };

    let sandbox = jails_prepare::sandbox::Sandbox::lay_out(laid_out)?;
    let (_, diff) = sandbox.run(
        &identity,
        program,
        vec!["-q".to_string(), "spotless:apply".to_string()],
        // Minimal, not empty. Maven's own launcher is a shell script that
        // shells out to `uname`, `dirname` and `expr`, so a run with no `PATH`
        // dies with "command not found" before Maven starts -- and without
        // `HOME` it cannot find the local repository that holds the plugin.
        // Everything else is deliberately absent: the fewer keys reach the
        // tool, the fewer ways one machine's environment can change what it
        // produces.
        ["PATH", "HOME", "JAVA_HOME"]
            .into_iter()
            .filter_map(|key| {
                std::env::var(key)
                    .ok()
                    .map(|value| (key.to_string(), value))
            })
            .collect(),
    )?;
    let mut change = DesiredChange::maintenance(MaintenanceAttribution::Format);
    let mut scopes = BTreeSet::new();
    for (path, file) in &diff.changed {
        scopes.insert(path.clone());
        change.files.push(DesiredFile {
            path: path.clone(),
            body: DesiredBody::Bytes(file.bytes.clone().into()),
            mode: Some(file.mode),
            resource: None,
            renderer: None,
        });
    }
    // A formatter that deletes a source is not formatting. Refusing here
    // rather than committing the deletion is the difference between a policy
    // and a description of what happened.
    if !diff.removed.is_empty() {
        sandbox.close()?;
        return Err(format!(
            "the formatter removed {} file(s), which formatting does not do.\n       fix: this \
             is a formatter or configuration problem; nothing was written.",
            diff.removed.len()
        )
        .into());
    }
    sandbox.close()?;

    if change.files.is_empty() {
        println!("already formatted -- nothing to change.");
    } else {
        println!("{} file(s) reformatted.", change.files.len());
    }

    let observed = observed(project)?;
    let set = DesiredChangeSet {
        ledger_intent: LedgerIntent {
            generation_before: observed.generation(),
            entities_after: Vec::new(),
            one_shots_after: Vec::new(),
            resources_after: Vec::new(),
            entities_removed: Vec::new(),
        },
        ordered: vec![change],
        subject: PlannedSubject::Format {
            scopes: scopes.clone(),
        },
    };
    set.validate()?;
    commit_set(
        run,
        set,
        &reads,
        &Asked::plain(CanonicalMutationRequest::Format { scopes }, &["fmt"], &[]),
    )
}
