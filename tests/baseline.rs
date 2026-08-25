//! Repeatable CLI baselines over checked-in Java project shapes.
//!
//! The ordinary test proves the corpus remains valid. The ignored test is the
//! measurement entry point documented by `tests/fixtures/benchmarks/README.md`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Instant;

const ARGS: &[&str] = &[
    "generate",
    "record",
    "BenchmarkProbe",
    "id:uuid",
    "--pretend",
    "--output",
    "json",
];

#[derive(Clone, Copy)]
struct Fixture {
    name: &'static str,
    workdir: &'static str,
    sources: usize,
}

const FIXTURES: &[Fixture] = &[
    Fixture {
        name: "small",
        workdir: ".",
        sources: 5,
    },
    Fixture {
        name: "medium",
        workdir: ".",
        sources: 60,
    },
    Fixture {
        name: "multi-module",
        workdir: "web",
        sources: 60,
    },
];

#[test]
fn benchmark_fixtures_have_stable_shapes_and_are_real_projects() {
    for fixture in FIXTURES {
        let root = fixture_path(fixture.name);
        assert_eq!(
            java_sources(&root),
            fixture.sources,
            "{} drifted",
            fixture.name
        );
        let output = command(&root.join(fixture.workdir), &["about", "--json"]);
        assert_success(fixture.name, &output);
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("\"schema_version\": 4"),
            "{} did not produce the project contract",
            fixture.name
        );
    }
}

/// Wall-clock baselines are opt-in: absolute latency is runner-specific, while
/// fixture validity belongs in every test run.
#[test]
#[ignore = "performance baseline; run with --ignored --nocapture"]
fn record_cli_baseline() {
    let samples = std::env::var("JAILS_BENCH_SAMPLES")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(30)
        .clamp(5, 10_000);

    for fixture in FIXTURES {
        record_cold(*fixture, samples);
        record_warm(*fixture, samples);
    }
}

fn record_cold(fixture: Fixture, samples: usize) {
    let mut elapsed = Vec::with_capacity(samples);
    for _ in 0..samples {
        let scratch = tempfile::tempdir().unwrap();
        let root = scratch.path().join(fixture.name);
        copy_tree(&fixture_path(fixture.name), &root);
        elapsed.push(measure(&root.join(fixture.workdir), fixture.name));
    }
    emit(
        fixture.name,
        "cold",
        "fresh-copy-no-machine-state-or-build-output",
        elapsed,
    );
}

fn record_warm(fixture: Fixture, samples: usize) {
    let scratch = tempfile::tempdir().unwrap();
    let root = scratch.path().join(fixture.name);
    copy_tree(&fixture_path(fixture.name), &root);
    let workdir = root.join(fixture.workdir);
    assert_success(fixture.name, &command(&workdir, ARGS));

    let elapsed = (0..samples)
        .map(|_| measure(&workdir, fixture.name))
        .collect();
    emit(
        fixture.name,
        "warm",
        "same-project-after-one-unmeasured-prime",
        elapsed,
    );
}

fn measure(workdir: &Path, fixture: &str) -> u64 {
    let started = Instant::now();
    let output = command(workdir, ARGS);
    let micros = started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64;
    assert_success(fixture, &output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    for phase in [
        "discover", "observe", "parse", "project", "prepare", "verify",
    ] {
        assert!(
            stdout.contains(&format!("\"phase\": \"{phase}\"")),
            "{fixture} omitted {phase}: {stdout}"
        );
    }
    assert!(!workdir.ancestors().any(|path| path.join(".jails").exists()));
    micros
}

fn emit(fixture: &str, state: &str, cache_reason: &str, mut elapsed: Vec<u64>) {
    elapsed.sort_unstable();
    let p50 = percentile(&elapsed, 50);
    let p95 = percentile(&elapsed, 95);
    let mut deviations: Vec<u64> = elapsed.iter().map(|value| value.abs_diff(p50)).collect();
    deviations.sort_unstable();
    let mad = percentile(&deviations, 50);
    println!(
        concat!(
            "{{\"schema\":\"jails.cli-baseline.v1\",",
            "\"fixture\":\"{}\",\"state\":\"{}\",",
            "\"samples\":{},\"p50_micros\":\"{}\",",
            "\"p95_micros\":\"{}\",\"mad_micros\":\"{}\",",
            "\"min_micros\":\"{}\",\"max_micros\":\"{}\",",
            "\"cache_reason\":\"{}\",",
            "\"scope\":\"new jails process; fixture copy excluded; no JVM or container\"}}"
        ),
        fixture,
        state,
        elapsed.len(),
        p50,
        p95,
        mad,
        elapsed[0],
        elapsed[elapsed.len() - 1],
        cache_reason,
    );
}

fn percentile(sorted: &[u64], percentage: usize) -> u64 {
    let rank = (percentage * sorted.len()).div_ceil(100);
    sorted[rank.saturating_sub(1)]
}

fn command(workdir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_jails"))
        .args(args)
        .current_dir(workdir)
        .output()
        .unwrap()
}

fn assert_success(fixture: &str, output: &Output) {
    assert!(
        output.status.success(),
        "{fixture} failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/benchmarks")
        .join(name)
}

fn java_sources(root: &Path) -> usize {
    let mut count = 0;
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else if path
                .extension()
                .is_some_and(|extension| extension == "java")
            {
                count += 1;
            }
        }
    }
    count
}

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let path = entry.unwrap().path();
        let target = to.join(path.file_name().unwrap());
        if path.is_dir() {
            copy_tree(&path, &target);
        } else {
            fs::copy(path, target).unwrap();
        }
    }
}
