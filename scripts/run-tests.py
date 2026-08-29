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
    # explicit `-- --test-threads` both still mean what they say. The default
    # is generous because most of these targets exit in milliseconds and the
    # few that do not are process-spawn bound rather than compute bound.
    jobs = known.jobs or max(4, cores * 2)

    logs = REPO / "target" / "jails-test-logs"
    logs.mkdir(parents=True, exist_ok=True)

    print(
        f"run-tests: {len(binaries)} binaries, {jobs} at a time"
        + (f" ({len(unmeasured)} unmeasured, scheduled first)" if unmeasured else "")
    )

    started = time.monotonic()
    observed: dict[str, float] = {}

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

    slowest = sorted(observed.items(), key=lambda row: -row[1])[:5]
    print(f"\nrun-tests: {elapsed:.1f}s wall for {len(binaries)} binaries")
    print("run-tests: slowest " + ", ".join(f"{n} {t:.1f}s" for n, t in slowest))
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
