//! The bottom of the stack: **writing, running, encoding, naming.**
//!
//! Nothing here knows what a Java project is, let alone what jails generates
//! into one. That is the boundary — a module belongs at this layer only when it
//! would still make sense in a tool that had never heard of Maven.
//!
//! ## What left when that rule was applied honestly
//!
//! (`pending.md` §7.5.) `codemod` -- the `# jails:<marker>` block splice --
//! went to `jails-project`, because its subject is a marked block in a
//! *project's* `compose.yaml`, keyed to jails' own comment syntax, which does
//! not clear the bar above. **It has since gone further, to a crate of its
//! own with no dependencies**, once it turned out that two ladders which
//! cannot see each other both need it. `runner` is [`hermetic`], because
//! `process` runs a program with the reader's terminal and this one runs it
//! with a timeout, a byte cap and no inherited environment: different safety
//! rules, and the two names said nothing about which was which. And
//! `CWD_LOCK` moved to `jails-testkit`, taken as a `[dev-dependency]` -- it is
//! test infrastructure, and the reason it could not be `#[cfg(test)]` (a
//! dependent crate's tests cannot see one) was a reason to give it a crate,
//! not a reason to ship it.
//!
//! ## What arrived, and why the summary line gained a verb
//!
//! [`identity`] and [`identifier`] came *down* from `jails-protocol`.
//! `ObjectId`, `Name`, `Package`, `JavaType`, `ProjectPath` and `SqlName` know
//! nothing about a plan — they clear the bar above easily — and they were
//! parked beside the legacy engine only by history. The crates that outlive
//! the cutover need them and must not depend on one that dies, which is the
//! same force that gave `codemod` its own crate. `identifier` had to follow,
//! because `SqlName` needs its `snake_case` and a crate cannot depend upward.
//!
//! [`Result`] and [`debug_cmd`] stay, because every crate above needs them and
//! a type alias the whole workspace shares has to sit below all of it.

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

/// Every fallible jails operation returns a message a human can act on, or
/// says that it has already said everything.
///
/// **The message stays free text**, deliberately, and the doc comment this
/// replaces got that trade right: the only consumer is `main`, which prints it,
/// so an enum per failure mode would buy pattern-matching nobody does and cost
/// a variant per message.
///
/// What it did *not* get right is that there were two outcomes, not one, and
/// the second was encoded as **the absence of characters in the first**.
/// `main` read `if !err.is_empty()`, and an empty string meant "the command has
/// already printed its report; set the exit code and say nothing" -- a
/// control-flow decision spelled as an empty message, which `doctor`, `lint`,
/// `run`, `migrate`, `testd`, `reports` and `invoke` all depended on and
/// nothing named. The failure mode it allows is quiet: any code path that
/// happens to build an empty message becomes "already reported" and the process
/// exits non-zero having printed nothing at all.
///
/// `pending.md` §6.5. Two variants, which is exactly the number of distinctions
/// actually in use.
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
/// Deliberate, and the alternative was worse: 131 assertions say
/// `error.contains("...")`, and rewriting every one to `error.to_string()
/// .contains(...)` would have made the migration to this type look like a
/// change to what the tests assert. The target is `str` rather than `String`
/// because `Reported` has no message and borrowing `""` is the honest answer
/// -- which is also why [`Self::message`] exists: code that needs to *know*
/// whether anything was said asks for the `Option`, and nothing else can
/// reconstruct that distinction from the text.
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
