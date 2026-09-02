//! Model problems, as values with a path and a fix.
//!
//! A `Diagnostic` is a code, the canonical model path it is about, what is
//! wrong, and what to do — the last of those is not optional, because a
//! refusal that does not say what to do next is a refusal the reader has to
//! guess at, which is the property `tests/architecture/` counts across the
//! whole binary.
//!
//! `Diagnostics` is a *set* rather than a first error, and that is the point of
//! separating this from `Result`: a parse or link pass reports everything it
//! found in one go. Fixing one problem and being told about the next one is
//! the experience this shape exists to avoid.
//!
//! # The one diagnostic contract
//!
//! §18.3 asks for one. This type is it, and the crates above are meant to
//! adopt it rather than invent a third vocabulary -- `jails-compiler` and
//! `jails-workspace` return `Result<_, String>` today, with no code and no
//! path, so a refusal from the compiler and a refusal from the parser are
//! different kinds of object and only one of them can be pointed at anything.
//! `Diagnostic::new` and [`Diagnostics::from_vec`] are public for exactly that
//! reason: nothing above this crate could construct one before.
//!
//! **The code namespace is closed, one range per phase.** A code says which
//! pass refused, so the namespace is owned by the crate that owns the pass:
//!
//! | prefix | phase | crate |
//! |---|---|---|
//! | `JDL####` | lexing and parsing JDL v1 | `jails-model`, `jdl/v1/` |
//! | `model-*` | linking, §18.2's passes 2-9 | `jails-model` |
//! | `compile-*` | semantic lowering | `jails-compiler` |
//! | `workspace-*` | capture, materialization, execution | `jails-workspace` |
//!
//! `every_diagnostic_code_belongs_to_the_crate_that_owns_its_phase` in
//! `tests/architecture/` holds that, so the third vocabulary cannot reappear
//! under a prefix that already means something else.
//!
//! **`plan-*` was the obvious prefix for the workspace and is taken.** The
//! gate found `plan-refused` on its first run -- a member of `jails-prepare`'s
//! `label()` table of command *outcomes*, beside `input-invalid` and
//! `tool-failed`, which is a different vocabulary that happens to share a
//! word. Two vocabularies under one prefix is the thing this table exists to
//! prevent, so the workspace gets `workspace-*`.
//!
//! **The root binary shares `model-*`, deliberately.** `model-io` and
//! `model-generated-drift` are emitted by `src/model_command.rs` into the
//! JSON a reader parses, and they are about the model in the same vocabulary
//! the linker uses -- reading its file, and finding the committed tree
//! disagrees with it. What the gate is defending against is an *emitter*
//! reaching for `model-*` because it is the prefix already in the tree to
//! copy; the command that reports on the model is not that.
//!
//! **Below the parser a diagnostic points at a model path, not a source
//! span**, and that is a decision rather than a limitation. The compiler and
//! the workspace never see the bytes the reader wrote -- the compiler is pure
//! over a `WorkspaceSnapshot`, and by then a source span would have to be
//! carried through linking for every node that might later refuse. What they
//! do have is the canonical model path the linker already uses
//! (`$.entities.task.fields.title`), which resolves to a declaration the
//! reader can find. Where the subject is a file rather than a model node --
//! a reader-owned patch, a stale precondition -- the path is the
//! project-relative path of that file. Both are `path`; neither is a guess at
//! a line.

use serde::Serialize;
use std::fmt::{Display, Formatter};

/// Whether a diagnostic refuses the model or only reports on it.
///
/// §18.3: "Warnings cannot stand in for refused safety checks. A fact that
/// would change generated behavior is either modeled, derived and displayed,
/// or rejected." So [`Severity::Warning`] is for what a reader may ignore
/// without changing what jails emits, and nothing else.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Severity {
    #[default]
    Error,
    Warning,
}

/// One model problem tied to its canonical model path.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Diagnostic {
    pub code: &'static str,
    pub path: String,
    pub message: String,
    pub fix: String,
    #[serde(default, skip_serializing_if = "is_error")]
    pub severity: Severity,
}

fn is_error(severity: &Severity) -> bool {
    matches!(severity, Severity::Error)
}

impl Diagnostic {
    /// A refusal: the model is not valid and nothing is written.
    pub fn new(
        code: &'static str,
        path: impl Into<String>,
        message: impl Into<String>,
        fix: impl Into<String>,
    ) -> Self {
        Self {
            code,
            path: path.into(),
            message: message.into(),
            fix: fix.into(),
            severity: Severity::Error,
        }
    }

    /// A report a reader may act on or ignore without changing what is emitted.
    pub fn warning(
        code: &'static str,
        path: impl Into<String>,
        message: impl Into<String>,
        fix: impl Into<String>,
    ) -> Self {
        Self {
            severity: Severity::Warning,
            ..Self::new(code, path, message, fix)
        }
    }
}

/// Every problem found in one parse/link pass.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Diagnostics {
    pub diagnostics: Vec<Diagnostic>,
}

impl Diagnostics {
    pub(crate) fn syntax(error: toml::de::Error) -> Self {
        Self {
            diagnostics: vec![Diagnostic::new(
                "model-syntax",
                "$",
                error.to_string(),
                "fix the TOML syntax or remove an unknown model key",
            )],
        }
    }

    /// Public so the crates above this one can adopt the contract.
    pub fn from_vec(diagnostics: Vec<Diagnostic>) -> Self {
        Self { diagnostics }
    }

    pub(crate) fn jdl_syntax(
        line: usize,
        message: impl Into<String>,
        fix: impl Into<String>,
    ) -> Self {
        Self {
            diagnostics: vec![Diagnostic::new(
                "jdl-syntax",
                format!("line {line}"),
                message,
                fix,
            )],
        }
    }

    pub fn is_empty(&self) -> bool {
        self.diagnostics.is_empty()
    }

    pub fn len(&self) -> usize {
        self.diagnostics.len()
    }
}

impl Display for Diagnostics {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        writeln!(formatter, "application model is invalid:")?;
        for diagnostic in &self.diagnostics {
            writeln!(
                formatter,
                "  [{}] {}: {}\n       fix: {}",
                diagnostic.code, diagnostic.path, diagnostic.message, diagnostic.fix
            )?;
        }
        Ok(())
    }
}

impl std::error::Error for Diagnostics {}
