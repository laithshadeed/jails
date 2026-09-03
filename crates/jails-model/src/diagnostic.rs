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
//! JDL v1 §18.3 asks for one. This type is it, and `Diagnostic::new` and
//! [`Diagnostics::from_vec`] are public so the crates above adopt it rather
//! than a second vocabulary: a `Result<_, String>` has no code and no path,
//! so a refusal shaped that way cannot be pointed at anything.
//!
//! **The code namespace is closed, one range per phase.** A code says which
//! pass refused, so the namespace is owned by the crate that owns the pass:
//!
//! | prefix | phase | crate |
//! |---|---|---|
//! | `JDL####` | lexing and parsing JDL v1 | `jails-model`, `jdl/v1/` |
//! | `model-*` | linking (JDL v1 §18.2) | `jails-model` |
//! | `compile-*` | semantic lowering | `jails-compiler` |
//! | `workspace-*` | capture, materialization, execution | `jails-workspace`, and `jails-project`'s `capture`, `documents` and `merge` |
//!
//! `every_diagnostic_code_belongs_to_the_crate_that_owns_its_phase` in
//! `tests/architecture/` holds that, so the third vocabulary cannot reappear
//! under a prefix that already means something else.
//!
//! **The workspace prefix is `workspace-*`, not `plan-*`**: `plan-refused`
//! already names a command *outcome*, beside `input-invalid` and
//! `tool-failed`, and two vocabularies under one prefix is the thing this
//! table exists to prevent.
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
/// JDL v1 §18.3: "Warnings cannot stand in for refused safety checks. A fact that
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
    /// Where in [`crate::MODEL_FILE`] the declaration this is about was
    /// written, one-based, when the document that produced it recorded one.
    ///
    /// **The path says what is wrong and the line says where to go.** A
    /// reader given `$.entities.loan.fields.status.type` and a file of
    /// eighty declarations is being asked to do the search the parser
    /// already did. `None` is honest rather than approximate: a diagnostic
    /// raised about a node no declaration owns -- a collision between two, a
    /// model built without a document -- has no line to point at, and a
    /// guessed one is worse than none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column: Option<u32>,
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
            line: None,
            column: None,
        }
    }

    /// A refusal that can only say what it found.
    ///
    /// The `fix` is mandatory on [`Self::new`] because a refusal a reader can
    /// act on has to say what to do next. Some cannot: a bundle whose blob
    /// does not match its digest, a decoded tag that is not a tag, a tree
    /// entry pointing at nothing. These are corruption reports, and a `fix:`
    /// line on one would be an invented instruction -- the same reasoning the
    /// `refusals with no fix: line` ratchet records for withdrawing its
    /// target. The constructor is named so the hole is greppable rather than
    /// spelled `""` at a call site and indistinguishable from an oversight.
    pub fn without_a_fix(
        code: &'static str,
        path: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self::new(code, path, message, String::new())
    }

    /// The same diagnostic, told where it came from.
    #[must_use]
    pub fn at(self, line: u32, column: u32) -> Self {
        Self {
            line: Some(line),
            column: Some(column),
            ..self
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

/// One diagnostic reads as the sentence a reader is shown.
///
/// **This is what keeps adopting the contract from rewriting any message.** A
/// phase below the CLI used to return a `String` shaped
/// `"<what is wrong>\n       fix: <what to do>"`, and callers interpolate that
/// string into a sentence of their own (`could not apply the plan: {error}`).
/// Splitting it into `message` and `fix` and rendering it back here produces
/// the same bytes, so a `?` through `jails_support::Failure` carries the
/// text unchanged and only the code is added. The seven-space indent is the
/// one [`Diagnostics`] already writes.
///
/// The code and the path are deliberately *not* rendered: they are what
/// `--output json` carries, and putting them in the human line would change
/// every refusal the reader has ever seen.
impl Display for Diagnostic {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)?;
        if !self.fix.is_empty() {
            write!(formatter, "\n       fix: {}", self.fix)?;
        }
        Ok(())
    }
}

impl std::error::Error for Diagnostic {}

/// Every problem found in one parse/link pass.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Diagnostics {
    pub diagnostics: Vec<Diagnostic>,
}

impl Diagnostics {
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
            diagnostics: vec![
                Diagnostic::new("jdl-syntax", format!("line {line}"), message, fix)
                    // Column one: the caller knows which line the shape is
                    // wrong on and nothing narrower, and a column it made up
                    // would put the reader's cursor in the wrong place with
                    // the confidence of a measurement.
                    .at(u32::try_from(line).unwrap_or(u32::MAX), 1),
            ],
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
                "  [{}] {}: {}",
                diagnostic.code, diagnostic.path, diagnostic.message
            )?;
            // **Its own line, so nothing that reads the first one moves.**
            // The code, the path and the message are what every message
            // gate, every golden and every reader already knows; the
            // location is new information and goes where an editor's
            // `file:line:column` jump finds it.
            if let (Some(line), Some(column)) = (diagnostic.line, diagnostic.column) {
                writeln!(formatter, "       at {}:{line}:{column}", crate::MODEL_FILE)?;
            }
            writeln!(formatter, "       fix: {}", diagnostic.fix)?;
        }
        Ok(())
    }
}

impl std::error::Error for Diagnostics {}
