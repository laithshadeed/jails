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

use serde::Serialize;
use std::fmt::{Display, Formatter};

/// One model problem tied to its canonical model path.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Diagnostic {
    pub code: &'static str,
    pub path: String,
    pub message: String,
    pub fix: String,
}

impl Diagnostic {
    pub(crate) fn new(
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
        }
    }
}

/// Every problem found in one parse/link pass.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Diagnostics {
    pub diagnostics: Vec<Diagnostic>,
}

impl Diagnostics {
    pub(crate) fn from_vec(diagnostics: Vec<Diagnostic>) -> Self {
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
