//! Whether the real-toolchain tier runs at all, and the one place that decides.
//!
//! The tier that shells out to Maven, Gradle, a JDK and a container runtime is
//! the only one that answers the question jails exists for -- *does it produce
//! a project that actually compiles?* -- and it is essentially the entire cost
//! of the suite. So **`cargo test` is Rust only** -- no JVM, no container, no
//! build tool -- which keeps it fast, bounded in memory, and identical on every
//! machine. `JAILS_TOOLCHAIN=1` opts the tier in, and the gate
//! (`mise run verify-rewrite`, `.githooks/pre-push`, CI) sets it.
//!
//! Opting in *is* the requirement: once the tier is switched on, a missing tool
//! is a failure naming it rather than a silent skip. One variable decides, so
//! there is no combination of two to get wrong.
#![allow(dead_code)]

/// Whether the caller asked for the real-toolchain tier.
///
/// Read it through the `real_*` probes rather than directly: a probe that
/// answers "the tool is here" without asking whether the tier is switched on
/// makes the suite depend on what happens to be on `PATH`.
pub fn toolchain_enabled() -> bool {
    std::env::var_os("JAILS_TOOLCHAIN").is_some_and(|value| value != "0")
}

/// Report that a test cannot run, and decide whether that is acceptable.
///
/// With the tier off this is an ordinary skip and the test is cheap and
/// absent. With the tier on it is a **failure naming what was missing**:
/// asking for the tier and silently not getting it lets a green run cover two
/// of three tiers.
#[track_caller]
pub fn skip(reason: &str) {
    assert!(
        !toolchain_enabled(),
        "JAILS_TOOLCHAIN is set, but this test cannot run: {reason}"
    );
    eprintln!("skipping: {reason}");
}

/// Skip a test whose precondition **cannot be installed**, and stay skipped
/// even under `JAILS_TOOLCHAIN`.
///
/// [`skip`] promotes a skip to a failure because the things it guards -- Maven,
/// a JDK that accepts `TARGET_RELEASE`, a container runtime, git -- can all be
/// installed. That reasoning does not reach a property of the *user*: nothing
/// installs "is not root", and a test needing a directory whose mode bits
/// refuse a write cannot have one under root's `CAP_DAC_OVERRIDE`. Promoting
/// that to a failure would make the gate permanently red wherever the suite
/// runs as root, and a gate that is always red is one people learn to pass
/// with `--no-verify`. It still prints with the same prefix, so a run that
/// lost this coverage says so.
///
/// **Use it only where no installation could satisfy the precondition.** A
/// missing tool is [`skip`].
#[track_caller]
pub fn skip_unsupported_environment(reason: &str) {
    eprintln!("skipping (this environment cannot express the precondition): {reason}");
}
