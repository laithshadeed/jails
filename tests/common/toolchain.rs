//! **Whether the real-toolchain tier runs at all, and the one place that
//! decides.**
//!
//! The tier that shells out to Maven, Gradle, a JDK and a container runtime is
//! the only one that answers the question jails exists for -- *does it produce
//! a project that actually compiles?* -- and it is also, measured on this
//! repository, essentially the entire cost of the suite: 859s of Maven inside
//! a 346s wall, ~7 GB of resident JVMs, against 30 test binaries that finish
//! in seconds without it.
//!
//! It used to run **by default, whenever the machine happened to have the
//! tools**, each probed off `PATH`. That single decision produced all three of
//! the complaints this module exists to answer:
//!
//! - **Slow.** A developer with a full toolchain could not run `cargo test`
//!   without also running every JVM in the suite. There was no cheap answer.
//! - **Machine-dependent.** The same command ran a different suite on two
//!   machines and reported the same green. That is why
//!   `JAILS_REQUIRE_TOOLCHAIN` had to be invented: a second variable whose
//!   whole job was to notice that the first default was wrong.
//! - **Unbounded.** Nothing sized the tier against the machine, so a laptop
//!   with the tools installed got every JVM at once.
//!
//! So the default is inverted. **`cargo test` is Rust only** -- no JVM, no
//! container, no build tool -- which makes it fast, bounded in memory, and
//! identical on every machine. `JAILS_TOOLCHAIN=1` opts the tier in, and the
//! gate (`mise run verify-rewrite`, `.githooks/pre-push`, CI) sets it.
//!
//! **That collapses two variables into one**, because opting in *is* the
//! requirement: once the tier is switched on, a missing tool is a failure
//! naming it rather than a silent skip. `JAILS_REQUIRE_TOOLCHAIN` no longer
//! exists, and there is no combination of the two left to get wrong.
#![allow(dead_code)]

/// Whether the caller asked for the real-toolchain tier.
///
/// Read it through the `real_*` probes rather than directly: a probe that
/// answers "the tool is here" without asking whether the tier is switched on
/// is exactly the PATH-dependent default this module removed.
pub fn toolchain_enabled() -> bool {
    std::env::var_os("JAILS_TOOLCHAIN").is_some_and(|value| value != "0")
}

/// Report that a test cannot run, and decide whether that is acceptable.
///
/// With the tier off this is an ordinary skip and the test is cheap and
/// absent. With the tier on it is a **failure naming what was missing** --
/// asking for the tier and silently not getting it is the hole that let a
/// green run cover two of three tiers for months.
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
/// a JDK that accepts `TARGET_RELEASE`, a container runtime, git -- are all
/// things a machine can be given, so a run that silently omits that tier is
/// hiding a fixable gap. That reasoning does not reach a property of the
/// *user*: nothing installs "is not root", and the one test guarded this way
/// needs a directory whose mode bits actually refuse a write, which root
/// bypasses through `CAP_DAC_OVERRIDE`.
///
/// Promoting that to a failure would make the gate permanently red anywhere
/// the suite runs as root -- every Claude Code on the web session, among
/// others -- and a gate that is always red is a gate people learn to pass
/// with `--no-verify`. It still prints, loudly and with the same prefix, so a
/// run that lost this coverage says so.
///
/// **Use it only where no installation could satisfy the precondition.** A
/// missing tool is [`skip`].
#[track_caller]
pub fn skip_unsupported_environment(reason: &str) {
    eprintln!("skipping (this environment cannot express the precondition): {reason}");
}
