//! The three version numbers a project jails creates is pinned to.
//!
//! **Here rather than inside the module that reads `pom.xml`.** None of them
//! is read off anything: they are the product's choice of what a generated
//! project targets, and the CLI defaults, the toolchain gate, `modernize` and
//! the pom `new` writes all have to name the same one. Keeping them beside a
//! parser makes every caller that needs a number depend on a reader nobody
//! asked to run.

/// The Java release every generated project targets by default. Referenced by
/// `new-cli`'s `--release` default, `new`'s `--java` default, and the tier-3
/// test gate.
///
/// **This is the one place the generated-project target is decided.** Machine
/// toolchain configuration such as `mise.toml` has to provide a JDK capable of
/// compiling it. Integration tests import this constant rather than
/// maintaining a second release number.
///
/// JDK 26 is the product default for new projects. It is deliberately newer
/// than the Java 21 compatibility floor below: adopted Java 21+ projects keep
/// their configured release, while new projects start on 26. Strict
/// real-toolchain tests must fail rather than silently skip when this JDK is
/// unavailable.
pub const TARGET_RELEASE: &str = "26";

/// The oldest release the *generated* Java actually needs. Everything jails
/// emits -- records, sealed interfaces, text blocks, pattern-matching switch,
/// `SequencedMap`, `List.getFirst()`, virtual threads -- went final in 21, so
/// that is the honest floor. `add` checks against this rather than
/// [`TARGET_RELEASE`], so a project pinned to an older release than jails'
/// default is still one `add` can grow.
pub const MIN_RELEASE: u32 = 21;

/// The Spring Boot line jails' own templates are written against.
///
/// One owner, for the same reason [`TARGET_RELEASE`] has one: `jails new
/// --gradle` has to *name* the Boot version in the build file it writes, and a
/// second literal is how the Maven fixture and the Gradle fixture come to
/// bootstrap different Boot versions with nothing saying so.
///
/// It is a default, not a floor. `--boot` overrides it, and a Boot version
/// older than this one is the case that flag exists for.
pub const TARGET_BOOT: &str = "4.1.0";
