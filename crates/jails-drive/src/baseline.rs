//! `jails architecture baseline`: accept the violations an adopted project
//! already has, so the generated ArchUnit suite fails only on new ones.
//!
//! Named `baseline` rather than `architecture` because `jails-generate` has an
//! `architecture` module already, and `module_of` assigns a layer by basename:
//! two modules of one name make the layer checker report an edge about a file
//! neither is.
//!
//! `g scaffold` writes `RAW_JDBC_STAYS_IN_ADAPTERS`, and on an adopted project
//! it fails over the reader's own code. The suite calls
//! `FreezingArchRule.freeze`, so a `.jails/architecture-baseline` records
//! today's violations and the rules fail only on new ones. Creating that store
//! is gated by two ArchUnit permissions, and this command grants them for one
//! run rather than asking the reader to edit the properties file by hand.
//!
//! **Nothing on disk is rewritten.** `ArchConfiguration` merges system
//! properties under the `archunit.` prefix over `archunit.properties`
//! (`PropertiesOverwritableBySystemProperties`, verified in `deps/archunit`),
//! so the permission is granted for one run and `archunit.properties` stays
//! strict. There is no half-applied state and nothing to roll back if the run
//! fails.

use crate::run::run_inherited;
use jails_support::Result;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The class the generated suite lives in. One name, and the refusal below
/// checks for the file rather than trusting the run to say something useful.
const SUITE: &str = "ArchitectureTest";

/// Where the frozen violations are recorded. Matches the
/// `freeze.store.default.path` the generated `archunit.properties` sets, and
/// is checked against it so the two cannot drift.
const STORE: &str = ".jails/architecture-baseline";

/// The two permissions, as system properties. Creation alone writes an empty
/// index and every rule still fails: ArchUnit needs update permission to
/// record what it froze.
const PERMISSIONS: [&str; 2] = [
    "-Darchunit.freeze.store.default.allowStoreCreation=true",
    "-Darchunit.freeze.store.default.allowStoreUpdate=true",
];

/// The generated suite and the two paths that go with it, located once, so
/// no caller re-derives a location from a bare `root: &Path`.
struct Suite {
    root: PathBuf,
    test: PathBuf,
    properties: PathBuf,
}

impl Suite {
    /// Find it, or refuse with the command that writes one.
    fn locate() -> Result<Self> {
        let root = jails_spec::spec::paths::find_project_root()?;
        // Walked rather than computed, and walked here rather than in a helper
        // taking the root back apart: the suite goes in the *base* package,
        // and an adopted project's base package is not always the shallowest
        // directory under `src/test/java`.
        let mut stack = vec![root.join("src/test/java")];
        let mut found = None;
        while let Some(dir) = stack.pop() {
            let candidate = dir.join(format!("{SUITE}.java"));
            if candidate.is_file() {
                found = Some(candidate);
                break;
            }
            for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
                if entry.path().is_dir() {
                    stack.push(entry.path());
                }
            }
        }
        let test = found.ok_or_else(|| {
            jails_support::Failure::Told(format!(
                "this project has no `{SUITE}` to freeze.\n       fix: `jails g scaffold \
                 <Resource> ...` writes the suite on the first scaffold, and names the files it \
                 will fail on."
            ))
        })?;
        let properties = root.join("src/test/resources/archunit.properties");
        let configured = std::fs::read_to_string(&properties).unwrap_or_default();
        if !configured.contains(&format!("freeze.store.default.path={STORE}")) {
            return Err(jails_support::Failure::Told(format!(
                "`src/test/resources/archunit.properties` does not point the freeze store at \
                 `{STORE}`, so jails cannot say what this would write.\n       fix: restore the \
                 file jails generated, or freeze the suite yourself -- the store path is the \
                 one thing jails has to agree with you about."
            )));
        }
        Ok(Self {
            root,
            test,
            properties,
        })
    }

    /// How many rule files the store holds. Zero covers both "no store" and
    /// "an empty one", which are the same answer to the only question asked:
    /// was anything frozen?
    fn frozen(&self) -> usize {
        std::fs::read_dir(self.root.join(STORE))
            .into_iter()
            .flatten()
            .flatten()
            .filter(|entry| entry.path().is_file())
            .count()
    }

    fn shown(&self, path: &Path) -> String {
        path.strip_prefix(&self.root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/")
    }

    /// Run only the architecture suite, through whichever build this project
    /// uses.
    ///
    /// Only that suite: freezing is about the rules, and running the whole
    /// test phase to reach them would make a command that takes a second take
    /// minutes on a real project -- and would fail for reasons that have
    /// nothing to do with what is being frozen.
    fn command(&self) -> Command {
        let mut command = match crate::build::detect(&self.root) {
            crate::build::Build::Gradle => {
                let mut command = Command::new(crate::run::gradlew::binary(&self.root));
                command.args(["test", "--tests", &format!("*{SUITE}")]);
                command
            }
            _ => {
                let mut command = Command::new(crate::maven::binary(&self.root));
                // `failIfNoSpecifiedTests=false` because a module with no
                // suite is not a failure of this command; `locate` is where
                // that is reported, with something the reader can do about it.
                command.args([
                    "test",
                    &format!("-Dtest={SUITE}"),
                    "-DfailIfNoSpecifiedTests=false",
                ]);
                command
            }
        };
        command.args(PERMISSIONS).current_dir(&self.root);
        command
    }
}

pub fn freeze(debug: bool) -> Result<()> {
    let suite = Suite::locate()?;
    let before = suite.frozen();
    println!("  freeze  {}", suite.shown(&suite.test));

    run_inherited(suite.command(), debug)?;

    match suite.frozen() {
        0 => println!(
            "  note    the suite recorded no violations -- nothing needed freezing, and the \
             rules stay strict"
        ),
        n if before == 0 => println!("  create  {STORE} ({n} rule file(s))"),
        n => println!("  replace {STORE} ({n} rule file(s), was {before})"),
    }
    println!(
        "\nfrozen. `{}` is unchanged and still strict, so the next `jails architecture \
         baseline` is as deliberate as this one was.\ncommit `{STORE}`: it is the record of \
         what was already wrong, and a *new* violation still fails the build.",
        suite.shown(&suite.properties)
    );
    Ok(())
}
