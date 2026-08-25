# `jails-support`

Foundational utilities, operating-system abstractions, file locks, process runners, and shared error primitives.

---

## Purpose & Overview

`jails-support` is the lowest-level layer in the workspace. It contains general-purpose systems programming primitives that do not know about Java, Maven, or Spring:
- **Unified Result and Error Modeling**: Provides [`Result<T>`](file:///home/laith/code/jails/crates/jails-support/src/lib.rs#L50) and [`Failure`](file:///home/laith/code/jails/crates/jails-support/src/lib.rs#L53-L62) used across all crates.
- **Cross-Process File Locking**: Advisory file locks using `flock` ([`lock::Lock`](file:///home/laith/code/jails/crates/jails-support/src/lock.rs)) to prevent concurrent mutations in the same project root.
- **Process Runners**: Standard process launching with `--debug` command echo ([`process`](file:///home/laith/code/jails/crates/jails-support/src/process.rs)) and sandboxed runner with byte/time caps ([`hermetic`](file:///home/laith/code/jails/crates/jails-support/src/hermetic.rs)).
- **Custom Codecs & Serialization**: Compact JSON and custom byte encoding utilities.

---

## Key Modules

```mermaid
flowchart TD
    SUPPORT["jails-support"]
    SUPPORT --> ERR["Failure\n(Told(msg) | Reported)"]
    SUPPORT --> LOCK["lock::Lock\n(flock-based cross-process project mutex)"]
    SUPPORT --> PROC["process / hermetic\n(Subprocess invocation & resource caps)"]
    SUPPORT --> SCRATCH["scratch\n(Atomic temporary directory allocation)"]
    SUPPORT --> CODEC["codec / json\n(Wire formatting & parsing)"]
```

- [`lock`](file:///home/laith/code/jails/crates/jails-support/src/lock.rs):
  - Acquires `.jails/lock` with custom owner descriptions.
  - Reports contention clearly (`Contention::Held(owner)`) rather than hanging indefinitely.
- [`Failure`](file:///home/laith/code/jails/crates/jails-support/src/lib.rs#L53-L62):
  - `Failure::Told(String)`: Human-facing error message with actionable `fix:` instructions.
  - `Failure::Reported`: Indicates that a command (like `doctor`) has already printed its full report to stdout/stderr and requires a non-zero exit code without printing a duplicate error trailer.
- [`process`](file:///home/laith/code/jails/crates/jails-support/src/process.rs):
  - Subprocess runner that automatically connects stdio, manages working directories, and formats command traces when `--debug` is active.
- [`hermetic`](file:///home/laith/code/jails/crates/jails-support/src/hermetic.rs):
  - Runs commands with execution timeouts and output buffer size limits.

---

## How It Connects to Other Crates

- Re-exported by all higher crates as the fundamental `Result<T>` and `Failure` error type.
- Direct dependency of [`jails-commit`](file:///home/laith/code/jails/crates/jails-commit/README.md) for filesystem locks.
