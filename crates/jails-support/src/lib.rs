//! The bottom of the stack: **writing, running, encoding, naming.**
//!
//! Nothing here knows what a Java project is, let alone what jails generates
//! into one. That is the boundary: a module belongs at this layer only when it
//! would still make sense in a tool that had never heard of Maven.
//!
//! [`identity`] and [`identifier`] are the naming half: `ObjectId`, `Name`,
//! `Package`, `JavaType`, `ProjectPath` and `SqlName` know nothing about a
//! plan. `identifier` sits here because `SqlName` needs its `snake_case`, and
//! a crate cannot depend upward.
//!
//! [`Result`] and [`debug_cmd`] are here because every crate above needs them,
//! and a type alias the whole workspace shares has to sit below all of it.
//!
//! **`runner` is [`hermetic`]**: `process` runs a program with the reader's
//! terminal and this one runs it with a timeout, a byte cap and no inherited
//! environment. Different safety rules, and a name has to say which is which.
//!
//! Two things deliberately live elsewhere. `codemod` -- the `# jails:<marker>`
//! block splice -- is its own dependency-free crate, because two ladders that
//! cannot see each other both need it. `CWD_LOCK` is in `jails-testkit`, taken
//! as a `[dev-dependency]`: it is test infrastructure, and a `#[cfg(test)]`
//! item is invisible to a dependent crate's tests.

// `#[derive(Codec)]` writes `jails_support::codec::...` into every impl it
// generates, and that path does not resolve inside this crate. Naming ourselves
// makes the macro's output compile here exactly as it does in a dependent.
extern crate self as jails_support;

pub mod apply;
pub mod codec;
pub mod git;
pub mod hermetic;
pub mod identifier;
pub mod identity;
pub mod json;
pub mod lock;
pub mod process;
pub mod scratch;
pub mod unified;

/// Every fallible jails operation returns a message a human can act on, or
/// says that it has already said everything.
///
/// The message stays free text: the only consumer is `main`, which prints it,
/// so an enum per failure mode would buy pattern-matching nobody does and cost
/// a variant per message. "Already reported" is a variant rather than an empty
/// message, because a code path that happens to build an empty message would
/// otherwise exit non-zero having printed nothing at all.
pub type Result<T> = std::result::Result<T, Failure>;

/// Why a command stopped.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Failure {
    /// A message for the reader. Free text, and by convention it carries a
    /// `fix:` line saying what to do next.
    Told(String),
    /// The command has already reported. Set the exit code and say nothing
    /// more -- `doctor` prints a full report and then fails only to make the
    /// shell see it.
    Reported,
}

impl Failure {
    /// The message to print, or `None` when the command has already spoken.
    pub fn message(&self) -> Option<&str> {
        match self {
            Self::Told(what) => Some(what),
            Self::Reported => None,
        }
    }
}

/// A failure reads as its message.
///
/// Assertions say `error.contains("...")`, and `Deref<Target = str>` keeps that
/// spelling. The target is `str` rather than `String` because `Reported` has
/// no message and borrowing `""` is the honest answer -- which is why
/// [`Self::message`] exists: code that needs to *know* whether anything was
/// said asks for the `Option`, and nothing else can reconstruct that
/// distinction from the text.
impl std::ops::Deref for Failure {
    type Target = str;

    fn deref(&self) -> &str {
        self.message().unwrap_or_default()
    }
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message().unwrap_or_default())
    }
}

impl From<String> for Failure {
    fn from(what: String) -> Self {
        Self::Told(what)
    }
}

impl From<&str> for Failure {
    fn from(what: &str) -> Self {
        Self::Told(what.to_string())
    }
}

/// Prints the program, args and working directory of a command about to be
/// run, for `--debug`.
pub fn debug_cmd(cmd: &std::process::Command) {
    let program = cmd.get_program().to_string_lossy();
    let args: Vec<String> = cmd
        .get_args()
        .map(|a| a.to_string_lossy().to_string())
        .collect();
    let dir = cmd
        .get_current_dir()
        .map(|d| format!("  (in {})", d.display()))
        .unwrap_or_default();
    eprintln!("+ {program} {}{dir}", args.join(" "));
}
