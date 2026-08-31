# `jails-testkit`

Shared test infrastructure and concurrency synchronization helpers for integration and unit tests.

---

## Purpose & Overview

`jails-testkit` provides test-only utilities that need to be shared across multiple crates in the workspace without polluting the public production APIs of `jails-support`.

### Key Component: [`CWD_LOCK`](../../crates/jails-testkit/src/lib.rs#L22)
Rust integration tests run multi-threaded within a single test binary process. Any test that invokes `std::env::set_current_dir` mutates process-global state.
- Holding [`CWD_LOCK`](../../crates/jails-testkit/src/lib.rs#L22) ensures tests mutating the current working directory run in isolation and do not cause race conditions in concurrent test suites.

---

## Usage in Crates

Included as a `[dev-dependencies]` entry in `Cargo.toml`:
```toml
[dev-dependencies]
jails-testkit.workspace = true
```

Example in test code:
```rust
use jails_testkit::CWD_LOCK;

#[test]
fn changes_working_directory() {
    let _guard = CWD_LOCK.lock().unwrap();
    std::env::set_current_dir("/path/to/project").unwrap();
    // Test logic here...
}
```
