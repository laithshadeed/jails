//! `jails bench`: run the k6 script `add loadtest` wrote, against a profile
//! the reader named, so a latency number has its load profile attached and is
//! reproducible.
//!
//! **It does not parse k6's output.** k6 prints its own summary with p95 and
//! p99 in it, and its thresholds decide pass or fail -- the generated script
//! sets `http_req_failed rate<0.01` and `http_req_duration p(95)<500,
//! p(99)<1000`. Re-deriving a verdict from a JSON summary would be a second
//! answer to a question k6 has answered, and `--summary-export` is there for a
//! reader who wants the raw numbers. A parser would also be written against an
//! output format no run here has produced, so jails drives k6 and gets out of
//! the way.

use crate::process::CommandSpec;
use jails_support::Result;
use std::path::Path;

/// Where `add loadtest` puts the script.
const SCRIPT: &str = "load-tests/load-test.js";

/// What the run was asked for. Both reach k6 as environment variables, because
/// that is how the generated script reads them -- `__ENV.VUS`, `__ENV.DURATION`.
pub struct Profile {
    pub vus: usize,
    pub duration: String,
    /// Write k6's machine-readable summary here as well as printing it.
    pub export: Option<String>,
}

pub fn bench(profile: Profile, debug: bool) -> Result<()> {
    let root = crate::find_project_root()?;
    if !has_load_test(&root) {
        return Err(format!(
            "no load test at {SCRIPT}.\n       fix: run `jails add loadtest` first."
        )
        .into());
    }
    require_k6()?;

    // Said before the run, not after: a latency number without the load that
    // produced it is not a measurement, and the reader is about to read one.
    println!(
        "load profile: {} virtual users for {}, against {SCRIPT}",
        profile.vus, profile.duration
    );
    println!("k6's own thresholds decide pass or fail; jails reports what it says.");
    println!();

    let mut spec = CommandSpec::new("k6").arg("run");
    if let Some(export) = &profile.export {
        spec = spec.arg("--summary-export").arg(export.as_str());
    }
    let spec = spec
        .arg(SCRIPT)
        .env("VUS", profile.vus.to_string())
        .env("DURATION", profile.duration.as_str())
        .current_dir(&root);
    crate::process::run_checked(&spec, crate::process::Diagnostics::from_flag(debug))?;
    Ok(())
}

fn require_k6() -> Result<()> {
    if crate::process::on_path("k6") {
        return Ok(());
    }
    Err(jails_support::Failure::Told(
        "k6 is not on PATH, and jails does not bundle a load generator.\n       \
         fix: `mise use -g k6`, or see https://grafana.com/docs/k6/latest/set-up/install-k6/"
            .to_string(),
    ))
}

/// Whether this project has a load test to run.
fn has_load_test(root: &Path) -> bool {
    root.join(SCRIPT).is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_script_path_is_the_one_add_loadtest_writes() {
        // Bound together deliberately: `add loadtest` writing somewhere else
        // would make `bench` refuse on a project that has exactly what it
        // needs, and the two live in different files.
        assert_eq!(SCRIPT, "load-tests/load-test.js");
    }

    #[test]
    fn a_project_with_no_load_test_is_not_benchable() {
        let root = jails_support::scratch::ScratchDir::in_temp("jails-bench")
            .unwrap()
            .keep();
        assert!(!has_load_test(&root));

        std::fs::create_dir_all(root.join("load-tests")).unwrap();
        std::fs::write(root.join(SCRIPT), "export default function () {}").unwrap();
        assert!(has_load_test(&root));
    }
}
