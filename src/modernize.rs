//! `jails modernize`: the version facts a project carries, moved to jails'.
//!
//! **The edits are interdependent, so they land together or not at all.** A
//! wrapper bumped without the toolchain block fails evaluation; a toolchain
//! bumped without the wrapper fails on "unsupported class file version". A run
//! that stopped halfway would leave a build broken in a way neither half
//! explains, so every rewritten body is computed first and written after the
//! last one is known.
//!
//! Like [`crate::adopt`], this runs *before* a project has a model and does
//! not initialise one: `build.gradle`, `gradle-wrapper.properties` and
//! `schema.sql` stay the reader's files. Changing a version in one claims
//! nothing jails would later reconcile, which is exactly why it does not go
//! through the compiler -- there is no declaration here for a model to hold.

use crate::Invocation;
use jails_project::modernize::Sources;
use jails_project::project::Project;
use jails_support::{Failure, Result};

pub(crate) fn run(invocation: Invocation) -> Result<()> {
    let project = Project::discover()?;
    let root = project.root();
    let read = |relative: &str| -> Option<(String, String)> {
        std::fs::read_to_string(root.join(relative))
            .ok()
            .map(|body| (relative.to_string(), body))
    };
    let (build, wrapper) = match project.build() {
        jails_spec::build::Build::Gradle => (
            read(jails_project::gradle::FILE)
                .or_else(|| read("build.gradle.kts"))
                .or_else(|| read("build.gradle")),
            read(jails_project::gradle::WRAPPER),
        ),
        _ => (read("pom.xml"), None),
    };
    let mut sources = Sources {
        build,
        wrapper,
        ..Default::default()
    };
    // A `schema.sql` jails did not write is still a schema this upgrade has an
    // opinion about, and the Java is read for the `javax` -> `jakarta` move.
    for name in ["schema.sql", "data.sql"] {
        if let Some(found) = read(&format!("src/main/resources/{name}")) {
            sources.sql.push(found);
        }
    }
    for absolute in jails_project::java::source_files(&root.join("src")) {
        let Ok(relative) = absolute.strip_prefix(root) else {
            continue;
        };
        let relative = relative.to_string_lossy().replace('\\', "/");
        if let Some(found) = read(&relative) {
            sources.java.push(found);
        }
    }

    let upgrade = jails_project::modernize::plan(&project, &sources)?;
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
        return Err(Failure::Told(
            "nothing to modernize: this project already declares the versions jails generates \
             against.\n       fix: nothing to do -- `jails doctor` prints the Boot, JDK and \
             build-tool versions it read."
                .to_string(),
        ));
    }
    // **The versions this moves decide what jails' own output says** -- the
    // `@AutoConfigureMockMvc` package, `javax` against `jakarta`, the MockMvc
    // form -- so a modelled project is recompiled beside the edit, the way
    // `jails sync` does, rather than left with generated files shaped by the
    // Boot it no longer declares.
    let modelled = crate::model_command::owns(root);
    if invocation.pretend {
        println!(
            "--pretend: nothing was written. ({} file(s) would change{})",
            upgrade.edits.len(),
            if modelled {
                ", then the model would be recompiled against them"
            } else {
                ""
            }
        );
        return Ok(());
    }
    for step in &upgrade.edits {
        jails_support::apply::put_one_shot(&step.artifact.path, step.artifact.contents.clone())?;
    }
    println!("modernized {} file(s)", upgrade.edits.len());
    if modelled {
        println!("recompiling the model against the versions it now declares");
        crate::model_command::sync(true, invocation)?;
    }
    Ok(())
}
