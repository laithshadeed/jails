#!/usr/bin/env python3
"""Run every workspace test binary at once, longest first.

`cargo test` builds the test binaries in parallel and then runs them **one
after another**, waiting for each to exit before starting the next. That is
the last serial edge in this suite and it is a large one: measured over this
workspace, the sum of the per-target times was within four seconds of the
whole run's wall clock, so essentially nothing overlapped. The consequence is
worst at each target's tail, where one straggling test holds a whole core-set
idle -- `engine` spends six of its seven seconds inside a single real Maven
spotless run, and `cargo test` will not start another binary during it.

So this starts them all and waits once. Three things it does that a bare
`for binary in *; do ... & done` would not:

- **Longest-processing-time first.** The binaries differ by three orders of
  magnitude -- `cli` against a crate whose unit tests finish in ten
  milliseconds -- and the makespan of a bounded run is set by whatever starts
  last. Each run records how long every binary took under `target/`, and the
  next run starts them in descending order of that. The first run has no
  measurements and says so; it is the only one that schedules blind.
- **Output kept whole.** Interleaved stdout from thirty concurrent binaries is
  unreadable, and a failure that cannot be read is a failure nobody fixes.
  Each binary's output goes to its own file and is printed in full, in
  schedule order, once everything has finished.
- **A real exit status.** Non-zero when any binary failed, with a summary
  naming which -- so this is usable as the gate `mise run verify-rewrite`
  invokes rather than only as a convenience.

It is a *runner*, not a second definition of what the suite is: the binaries
come from `cargo test --no-run`, so a target added to `Cargo.toml` appears
here with no edit, and there is no list to keep in step.

Usage:
    scripts/run-tests.py [--jobs N] [cargo args...] [-- test-binary args...]

Everything before `--` is passed to `cargo test --no-run`, so
`scripts/run-tests.py -p jails-model` and `--release` work as they do there.
Everything after `--` is passed to each test binary, so `-- --ignored` and a
filter substring work as they do under `cargo test`.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
import time
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
# Beside the in-binary scenario ledgers, and under `target/` for the same
# reason: it is a scheduling hint, never an input to a result, so it must not
# be reviewable state and must survive being deleted.
LEDGER = REPO / "target" / "jails-test-costs" / "binaries.tsv"
# Where each test binary leaves what its subprocesses cost. Under `target/` for
# the ledger's reason -- it is a report, never an input to a result -- and
# cleared before every run, or a binary that did not run this time would
# contribute its last run's numbers to this run's summary.
PROFILE = REPO / "target" / "jails-test-profile"


def read_subprocess_totals() -> tuple[float, float, float, dict[str, tuple[int, float]]]:
    """What every test binary's subprocesses cost, summed across the run.

    The span is the largest any single binary reported rather than the sum:
    the binaries overlap, so summing their spans would count the same wall
    clock several times and make concurrency look impossible.
    """
    span = 0.0
    work = 0.0
    queued = 0.0
    tools: dict[str, tuple[int, float]] = {}
    for report in sorted(PROFILE.glob("*.tsv")):
        for line in report.read_text(errors="replace").splitlines():
            fields = line.split("\t")
            if fields[0] == "span_ms" and len(fields) == 2:
                span = max(span, int(fields[1]) / 1000)
            elif fields[0] == "queue_ms" and len(fields) == 2:
                queued += int(fields[1]) / 1000
            elif fields[0] == "tool" and len(fields) == 4:
                count, run_ms = tools.get(fields[1], (0, 0.0))
                tools[fields[1]] = (count + int(fields[2]), run_ms + int(fields[3]) / 1000)
                work += int(fields[3]) / 1000
    return span, work, queued, tools


def report_subprocess_totals(elapsed: float) -> None:
    """Say where the wall clock went, in two lines, on every run.

    This is the only thing that can answer why the same suite takes 147s on a
    developer machine and 296s on a four-core CI runner: both numbers are
    real, and neither says whether the difference is more work, less overlap,
    or time spent queueing. Printing it unconditionally is the point -- the
    run that raises the question is never the run you thought to instrument.
    """
    _, work, queued, tools = read_subprocess_totals()
    if not tools:
        return
    busiest = sorted(tools.items(), key=lambda row: -row[1][1])
    print(
        "run-tests: subprocess cost "
        + ", ".join(f"{name} {run:.1f}s over {count}" for name, (count, run) in busiest)
    )
    print(
        f"run-tests: {work:.1f}s of subprocess work in {elapsed:.1f}s"
        f" (mean concurrency {work / elapsed:.2f}), {queued:.1f}s queued for a permit"
    )


def build(cargo_args: list[str]) -> list[Path]:
    """Compile the test binaries and return them, without running any."""
    result = subprocess.run(
        ["cargo", "test", "--no-run", "--message-format=json-render-diagnostics"]
        + cargo_args,
        cwd=REPO,
        stdout=subprocess.PIPE,
        text=True,
    )
    if result.returncode != 0:
        sys.exit(result.returncode)
    binaries: list[Path] = []
    for line in result.stdout.splitlines():
        try:
            message = json.loads(line)
        except json.JSONDecodeError:
            continue
        # `executable` is null for a non-test artifact and set for a test one;
        # `profile.test` distinguishes the test harness from a plain binary
        # that happens to have been built along the way.
        if message.get("reason") != "compiler-artifact":
            continue
        if not message.get("profile", {}).get("test"):
            continue
        if executable := message.get("executable"):
            binaries.append(Path(executable))
    return binaries


def read_ledger() -> dict[str, float]:
    try:
        rows = LEDGER.read_text().splitlines()
    except OSError:
        return {}
    known: dict[str, float] = {}
    for row in rows:
        name, _, seconds = row.partition("\t")
        try:
            known[name] = float(seconds)
        except ValueError:
            continue
    return known


def write_ledger(observed: dict[str, float]) -> None:
    known = read_ledger()
    known.update(observed)
    try:
        LEDGER.parent.mkdir(parents=True, exist_ok=True)
        staging = LEDGER.with_suffix(f".{os.getpid()}.tmp")
        staging.write_text(
            "".join(f"{name}\t{seconds:.3f}\n" for name, seconds in sorted(known.items()))
        )
        staging.replace(LEDGER)
    except OSError:
        # A scheduling hint that cannot be written is a slower next run, never
        # a failed one.
        pass


def runtime_environment() -> dict[str, str]:
    """The environment cargo would have run these binaries under.

    Running a test binary directly is not quite the same as `cargo test`
    running it, and one difference bites: a proc-macro crate's test harness
    links `libstd` **dynamically** against the toolchain's sysroot, and cargo
    puts that sysroot on the loader path for it. Without this,
    `jails-codec-derive`'s tests die before `main` with `error while loading
    shared libraries: libstd-*.so`, which this runner then reports as a failed
    binary -- a runner that cannot run part of the suite, reporting it as a
    test failure rather than as its own defect.

    Also carries `target/debug/deps`, which is where a dylib built by the
    workspace itself would live.
    """
    environment = dict(os.environ)
    roots = []
    try:
        sysroot = Path(
            subprocess.run(
                ["rustc", "--print", "sysroot"],
                capture_output=True,
                text=True,
                check=True,
            ).stdout.strip()
        )
        host = subprocess.run(
            ["rustc", "--print", "host-tuple"],
            capture_output=True,
            text=True,
            check=True,
        ).stdout.strip()
        # Both, and the second is the one that matters: `libstd-*.so` lives
        # under the per-target directory, not directly under `<sysroot>/lib`.
        roots.append(str(sysroot / "lib"))
        roots.append(str(sysroot / "lib" / "rustlib" / host / "lib"))
    except (OSError, subprocess.CalledProcessError):
        # Without a sysroot the proc-macro binaries will fail loudly, which is
        # the right outcome: better a named failure than a silent skip.
        pass
    roots.append(str(REPO / "target" / "debug" / "deps"))
    if existing := environment.get("LD_LIBRARY_PATH"):
        roots.append(existing)
    environment["LD_LIBRARY_PATH"] = os.pathsep.join(roots)
    return environment


def report_leaked_containers() -> None:
    """Name anything the suite started and did not take down.

    Reporting, never deleting: a run that removes containers by name could
    take out a concurrent run's, and the point here is to make a regression
    visible rather than to tidy up after one.

    This exists because the leak's failure mode is *delayed*. Two tests let
    `jails add` start compose services and never took them down; each run left
    a container and its compose network behind, and after three runs Docker
    had no address pool left. What failed then was not either of those tests
    -- it was `canonical_toxiproxy_pack_keeps_testkit_edits_and_runs_with_real_maven`,
    with `all predefined address pools have been fully subnetted`, in a run
    whose own tests were all correct. A line here would have named the cause
    on the first run instead of the symptom on the third.
    """
    def docker(*args: str) -> list[str]:
        try:
            done = subprocess.run(
                ["docker", *args], capture_output=True, text=True, timeout=30
            )
        except (OSError, subprocess.SubprocessError):
            return []
        if done.returncode != 0:
            return []
        return [line for line in done.stdout.splitlines() if line.startswith("jails-")]

    containers = docker("ps", "-a", "--format", "{{.Names}}")
    networks = docker("network", "ls", "--format", "{{.Name}}")
    if not containers and not networks:
        return
    print(
        f"\nrun-tests: WARNING -- the suite left {len(containers)} container(s) and "
        f"{len(networks)} network(s) behind."
    )
    for name in sorted(set(containers) | set(networks))[:10]:
        print(f"  {name}")
    print(
        "  A test let `jails add` start compose services without `--no-start` "
        "and did not take them down.\n"
        "  Left to accumulate this exhausts Docker's address pool and unrelated "
        "container tests begin failing."
    )


def key_for(binary: Path) -> str:
    """A binary's name without cargo's content hash.

    `target/debug/deps/cli-de7241df471a56cc` becomes `cli`: the hash changes
    on every rebuild, so keying by the file name would make every run the
    first run.
    """
    name = binary.name
    stem, dash, tail = name.rpartition("-")
    return stem if dash and len(tail) == 16 else name


def main() -> int:
    parser = argparse.ArgumentParser(add_help=False)
    parser.add_argument("--jobs", type=int, default=None)
    parser.add_argument("--help", action="help")
    known, rest = parser.parse_known_args()

    cargo_args, _, harness_args = (
        (rest[: rest.index("--")], "--", rest[rest.index("--") + 1 :])
        if "--" in rest
        else (rest, "", [])
    )

    # `--workspace` unless the caller selected packages themselves. `cargo
    # test` at a workspace root builds the root package only -- CLAUDE.md
    # records the run where that reported 390 passing over a tree of 418 with
    # nothing to say the other 28 had not run -- and a *runner* that quietly
    # covered less than the gate it replaces would be the same failure with a
    # faster wall clock.
    selects_packages = any(
        arg in ("-p", "--package", "--workspace", "--all", "--exclude")
        or arg.startswith(("-p=", "--package=", "--exclude="))
        for arg in cargo_args
    )
    if not selects_packages:
        cargo_args = ["--workspace"] + cargo_args

    binaries = build(cargo_args)
    if not binaries:
        print("run-tests: cargo built no test binaries", file=sys.stderr)
        return 1

    costs = read_ledger()
    # Unmeasured first, for the reason the in-binary ledger records: an
    # unknown cost is most likely a new target, and starting a cheap one early
    # costs nothing while starting an expensive one late costs its whole
    # duration on the critical path.
    schedule = sorted(binaries, key=lambda b: -costs.get(key_for(b), float("inf")))
    unmeasured = [key_for(b) for b in schedule if key_for(b) not in costs]

    cores = os.cpu_count() or 4
    # Concurrency is over *binaries*; each one parallelises internally, and
    # `--test-threads` is left to the harness so `RUST_TEST_THREADS` and an
    # explicit `-- --test-threads` both still mean what they say.
    #
    # **One binary per core, and the generous `cores * 2` it replaces was
    # measured doing harm.** `cli` saturates the machine by itself -- profiled
    # over `tests/cli`, mean concurrency 4.4 on four cores with *zero* idle
    # seconds -- so the other thirty-one binaries have no gaps to fill and can
    # only add contention. On the four-core CI runner that showed up exactly
    # as the arithmetic predicts: `cli` slowed 257.9s -> 281.1s while the other
    # binaries' 79.8s disappeared into its shadow, a net 21s bought with a much
    # busier box.
    #
    # That busier box then broke something. A generated `http-sink` test whose
    # request timeout is 5000ms against a *localhost* stub timed out under the
    # load -- a threshold that is comfortable on an idle machine and marginal
    # on a starved one. Overlap is worth having; oversubscription is what turns
    # a timing-sensitive test into a flaky one, and no scheduling gain is worth
    # that.
    #
    # A machine with cores to spare is a different case, and this scales with
    # it rather than assuming four.
    jobs = known.jobs or max(2, cores)

    logs = REPO / "target" / "jails-test-logs"
    logs.mkdir(parents=True, exist_ok=True)
    shutil.rmtree(PROFILE, ignore_errors=True)

    print(
        f"run-tests: {len(binaries)} binaries, {jobs} at a time"
        + (f" ({len(unmeasured)} unmeasured, scheduled first)" if unmeasured else "")
    )

    started = time.monotonic()
    observed: dict[str, float] = {}
    environment = runtime_environment()

    def run(binary: Path) -> tuple[Path, int, Path, float]:
        name = key_for(binary)
        log = logs / f"{name}.log"
        began = time.monotonic()
        with log.open("wb") as sink:
            code = subprocess.call(
                [str(binary)] + harness_args,
                cwd=REPO,
                stdout=sink,
                stderr=subprocess.STDOUT,
                env=environment,
            )
        return binary, code, log, time.monotonic() - began

    with ThreadPoolExecutor(max_workers=jobs) as pool:
        outcomes = list(pool.map(run, schedule))

    elapsed = time.monotonic() - started
    failed: list[tuple[str, float]] = []
    for binary, code, log, took in outcomes:
        name = key_for(binary)
        observed[name] = took
        if code != 0:
            failed.append((name, took))
            print(f"\n=== FAILED {name} ({took:.1f}s) ===")
            sys.stdout.write(log.read_text(errors="replace"))

    write_ledger(observed)

    report_leaked_containers()

    slowest = sorted(observed.items(), key=lambda row: -row[1])[:5]
    print(f"\nrun-tests: {elapsed:.1f}s wall for {len(binaries)} binaries")
    print("run-tests: slowest " + ", ".join(f"{n} {t:.1f}s" for n, t in slowest))
    report_subprocess_totals(elapsed)
    if failed:
        print(
            f"run-tests: {len(failed)} binary(ies) failed: "
            + ", ".join(name for name, _ in failed)
        )
        print(f"run-tests: full output under {logs}")
        return 1
    print("run-tests: all green")
    return 0


if __name__ == "__main__":
    sys.exit(main())
