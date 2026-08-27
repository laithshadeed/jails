//! `jails modernize` — the version facts a project carries, moved to jails'.

use super::*;

/// Upgrade the build to Spring Boot 4.1 on JDK 26, as one commit.
///
/// One transition, not five commands. The edits are interdependent -- a
/// wrapper bumped without the toolchain block fails evaluation, a toolchain
/// bumped without the wrapper fails on "unsupported class file version" -- so
/// a run that stops halfway leaves a build that is broken in a way neither
/// half explains. Committing them together means the project either moved or
/// did not.
///
/// `resource: None` throughout, on `format`'s rule. `build.gradle`,
/// `gradle-wrapper.properties` and `schema.sql` are the reader's files;
/// changing a version in one claims nothing jails would later reconcile, and
/// claiming them would let a `remove` somewhere else take them away.
pub fn modernize(run: &Run) -> Result<Outcome> {
    let project = run.project();
    let mut reads = capture::capability_reads()?;
    let build_file = match project.build() {
        jails_spec::build::Build::Gradle => jails_project::gradle::FILE,
        jails_spec::build::Build::Maven => "pom.xml",
        other => {
            return Err(format!(
                "`jails modernize` upgrades a Maven or Gradle build, and this project's is \
                 {other:?}.\n       fix: nothing here is safe to guess at."
            )
            .into());
        }
    };
    let build = ProjectPath::parse(build_file)?;
    reads = reads.file(build.clone());
    let wrapper = ProjectPath::parse(jails_project::gradle::WRAPPER)?;
    reads = reads.file(wrapper.clone());
    // The two files Spring initialises a datasource from, and only those.
    // Flyway migrations under `db/migration` are applied-once history:
    // rewriting one that has already run changes a checksum rather than a
    // schema, which is a worse outcome than the type error it would fix.
    let mut sql = Vec::new();
    for name in ["schema.sql", "data.sql"] {
        let path = ProjectPath::parse(&format!("src/main/resources/{name}"))?;
        reads = reads.file(path.clone());
        sql.push(path);
    }
    let mut java = Vec::new();
    for absolute in jails_java::java::source_files(&project.root().join("src")) {
        let relative = super::relative_path(project, &absolute)?;
        reads = reads.file(relative.clone());
        java.push(relative);
    }
    let (snapshot, _) = capture::projected(project, &reads)?;

    let text = |path: &ProjectPath| -> Result<Option<String>> {
        Ok(match snapshot.read(path)? {
            jails_protocol::snapshot::Captured::Present(file) => {
                Some(String::from_utf8_lossy(&file.bytes).into_owned())
            }
            _ => None,
        })
    };
    let mut sources = jails_project::modernize::Sources {
        build: text(&build)?.map(|body| (build.to_string(), body)),
        wrapper: text(&wrapper)?.map(|body| (wrapper.to_string(), body)),
        ..Default::default()
    };
    for path in &sql {
        if let Some(body) = text(path)? {
            sources.sql.push((path.to_string(), body));
        }
    }
    for path in &java {
        if let Some(body) = text(path)? {
            sources.java.push((path.to_string(), body));
        }
    }
    let upgrade = jails_project::modernize::plan(project, &sources)?;

    for line in &upgrade.current {
        println!("  ok      {line}");
    }
    for step in &upgrade.edits {
        for line in &step.what {
            println!("  change  {line}");
        }
    }
    for finding in &upgrade.findings {
        println!("  ask     {}", finding.what);
        println!("          fix: {}", finding.fix);
    }
    if upgrade.edits.is_empty() {
        return Err(jails_support::Failure::Told(
            "nothing to modernize: this project already declares the versions jails generates \
             against.\n       fix: nothing to do -- `jails doctor` prints the Boot, JDK and \
             build-tool versions it read."
                .to_string(),
        ));
    }

    let mut change = DesiredChange::maintenance(MaintenanceAttribution::Modernize);
    let mut files = BTreeSet::new();
    for step in &upgrade.edits {
        let path = ProjectPath::parse(&step.artifact.path.to_string_lossy())?;
        files.insert(path.clone());
        change.files.push(DesiredFile {
            path,
            body: DesiredBody::Bytes(step.artifact.contents.clone().into_bytes().into()),
            mode: None,
            resource: None,
            renderer: None,
        });
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
        subject: PlannedSubject::Modernize {
            files: files.clone(),
        },
    };
    set.validate()?;
    commit_set(
        run,
        set,
        &reads,
        &Asked::plain(
            CanonicalMutationRequest::Modernize { files },
            &["modernize"],
            &[],
        ),
    )
}
