#!/usr/bin/env python3
"""High-performance workspace test runner for jails.

Runs all test executables and doctests concurrently using asyncio subprocesses.
Eliminates shell loop polling, subshell forks, and disk exit files.
"""

import asyncio
import json
import os
import re
import shutil
import sys
import time
from pathlib import Path

PRIORITIES = {
    "cli": 100,
    "crash": 90,
    "jails_support": 85,
    "product_loop": 80,
    "agreement": 75,
    "golden": 70,
    "architecture": 60,
    "jails_drive": 50,
    "jails_model": 40,
    "jails_project": 30,
    "jails_compiler": 20,
}


def clean_display_name(raw_name: str) -> str:
    return re.sub(r"-[0-9a-f]{8,}$", "", raw_name)


async def compile_test_binaries(root_dir: Path):
    cmd = [
        "cargo",
        "test",
        "--workspace",
        "--bins",
        "--tests",
        "--no-run",
        "--message-format=json",
    ]
    proc = await asyncio.create_subprocess_exec(
        *cmd,
        cwd=root_dir,
        stdout=asyncio.subprocess.PIPE,
        stderr=asyncio.subprocess.PIPE,
    )
    stdout, stderr = await proc.communicate()
    if proc.returncode != 0:
        sys.stderr.write(stderr.decode("utf-8", errors="replace"))
        sys.exit(proc.returncode)

    artifacts = []
    for line in stdout.decode("utf-8", errors="replace").splitlines():
        if not line.strip():
            continue
        try:
            msg = json.loads(line)
        except ValueError:
            continue
        if (
            msg.get("reason") == "compiler-artifact"
            and msg.get("executable")
            and msg.get("profile", {}).get("test")
        ):
            exe = msg["executable"]
            manifest = msg.get("manifest_path", "")
            manifest_dir = str(Path(manifest).parent) if manifest else str(root_dir)
            artifacts.append((exe, manifest_dir))
    return artifacts


async def run_single_test(
    name: str,
    cmd: list[str],
    cwd: str,
    env: dict[str, str],
    log_file: Path,
    sem: asyncio.Semaphore,
):
    async with sem:
        start_time = time.monotonic()
        proc = await asyncio.create_subprocess_exec(
            *cmd,
            cwd=cwd,
            env=env,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.STDOUT,
        )
        output, _ = await proc.communicate()
        elapsed = time.monotonic() - start_time
        returncode = proc.returncode

        log_file.write_bytes(output)

        out_text = output.decode("utf-8", errors="replace")
        summary = ""
        for line in reversed(out_text.splitlines()):
            if line.startswith("test result:"):
                summary = line.replace("test result: ", "").strip()
                break

        passed = (returncode == 0) and ("ok" in summary.lower())
        return {
            "name": name,
            "passed": passed,
            "returncode": returncode,
            "summary": summary,
            "elapsed": elapsed,
            "log_file": log_file,
            "output": out_text,
        }


async def main():
    script_dir = Path(__file__).resolve().parent
    root_dir = script_dir.parent

    logs_dir = root_dir / "target" / "jails-test-logs" / "gate"
    shutil.rmtree(logs_dir, ignore_errors=True)
    logs_dir.mkdir(parents=True, exist_ok=True)

    cores = os.cpu_count() or 4
    default_concurrency = cores * 3 if cores <= 4 else min(cores, 8)
    concurrency = int(os.environ.get("TEST_CONCURRENCY", default_concurrency))
    concurrency = max(concurrency, 4)

    default_threads = "4"
    rust_test_threads = os.environ.get("RUST_TEST_THREADS", default_threads)

    base_env = os.environ.copy()
    base_env["RUST_TEST_THREADS"] = rust_test_threads

    extra_args = sys.argv[1:]

    artifacts = await compile_test_binaries(root_dir)
    if len(artifacts) < 10:
        sys.stderr.write(
            f"test: only {len(artifacts)} test executables found; compilation may have failed\n"
        )
        sys.exit(2)

    def sort_key(item):
        base = clean_display_name(Path(item[0]).name)
        return -PRIORITIES.get(base, 0)

    artifacts.sort(key=sort_key)

    sem = asyncio.Semaphore(concurrency)
    tasks = []

    for exe, manifest_dir in artifacts:
        raw_name = Path(exe).name
        disp_name = clean_display_name(raw_name)
        log_file = logs_dir / f"{disp_name}.log"
        env = base_env.copy()
        env["CARGO_MANIFEST_DIR"] = manifest_dir
        cmd = [exe] + extra_args
        tasks.append(
            asyncio.create_task(
                run_single_test(disp_name, cmd, str(root_dir), env, log_file, sem)
            )
        )

    # Doctests
    doc_log = logs_dir / "doctests.log"
    doc_cmd = ["cargo", "test", "--workspace", "--doc", "--"] + extra_args
    tasks.append(
        asyncio.create_task(
            run_single_test("doctests", doc_cmd, str(root_dir), base_env, doc_log, sem)
        )
    )

    results = await asyncio.gather(*tasks)
    results.sort(key=lambda r: (-PRIORITIES.get(r["name"], 0), r["name"]))

    all_passed = True
    for r in results:
        name = r["name"]
        if r["passed"]:
            summary_part = r["summary"] or "ok"
            print(f"test: {name:<32} ok   {summary_part}")
        else:
            all_passed = False
            code = r["returncode"]
            print(f"test: {name:<32} FAILED (exit {code})")

    if not all_passed:
        print("\n==========================================")
        print("TEST FAILURES:")
        print("==========================================")
        for r in results:
            if not r["passed"]:
                print(f"---- {r['name']} ({r['log_file']}) ----")
                lines = r["output"].splitlines()
                fail_lines = [l for l in lines if "failures:" in l]
                if fail_lines:
                    idx = lines.index(fail_lines[0])
                    for l in lines[idx : idx + 100]:
                        print(l)
                else:
                    for l in lines[-40:]:
                        print(l)
                print("-----------------------------------------")
        sys.exit(1)


if __name__ == "__main__":
    asyncio.run(main())
