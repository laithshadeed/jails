//! The bottom of the stack: writing, running, splicing and encoding.
//!
//! Nothing here knows what a Java project is, let alone what jails generates
//! into one. That is the boundary — a module belongs at this layer only when it
//! would still make sense in a tool that had never heard of Maven.
//!
//! [`Result`] and [`debug_cmd`] live here rather than in the binary because
//! every crate above needs them, and a type alias the whole workspace shares has
//! to sit below all of it.

pub mod apply;
pub mod codec;
pub mod codemod;
pub mod json;
pub mod process;
pub mod runner;
pub mod scratch;

/// Every fallible jails operation returns a message a human can act on.
///
/// A `String` rather than an error enum, deliberately: the only consumer is
/// `main`, which prints it. An enum would buy pattern-matching nobody does and
/// cost a variant per failure mode.
pub type Result<T> = std::result::Result<T, String>;

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

/// The process-global current directory, as a lock.
///
/// Unit tests within one crate share a test binary and therefore one cwd, so a
/// test that changes it must hold this for the duration. It lives here, and is
/// not `#[cfg(test)]`, because the crates that need it are not the crate that
/// would define it: each dependent crate's test binary links one instance,
/// which is exactly the scope the lock has to cover.
pub static CWD_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
