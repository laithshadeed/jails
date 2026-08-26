//! Closed editor protocol request vocabulary.

use clap::{Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum EditorSymbolKindArg {
    Routes,
    Beans,
    Queries,
    Tests,
    Types,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum EditorDiagnosticScopeArg {
    Buffer,
    Project,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum EditorEvidenceArg {
    Parsed,
    Offline,
    Live,
}

#[derive(Subcommand)]
pub(crate) enum EditorCommand {
    /// Negotiate the versioned, read-only editor protocol
    Handshake {
        /// File or directory used as the starting point for root discovery
        #[arg(long)]
        path: Option<PathBuf>,
    },
    /// Complete one already-tokenized CLI argument
    Complete {
        #[arg(long)]
        arg_index: u32,
        #[arg(long)]
        byte_offset: u32,
        #[arg(long)]
        path: Option<PathBuf>,
        /// Tokenized argv excluding the jails executable
        #[arg(last = true, allow_hyphen_values = true)]
        argv: Vec<String>,
    },
    /// Return project symbols with stable semantic identities
    Symbols {
        #[arg(value_enum)]
        kind: EditorSymbolKindArg,
        #[arg(long)]
        query: Option<String>,
        #[arg(long)]
        path: Option<PathBuf>,
    },
    /// Return structured, evidence-tagged diagnostics
    Diagnostics {
        #[arg(long, value_enum)]
        scope: EditorDiagnosticScopeArg,
        #[arg(long, required_if_eq("scope", "buffer"))]
        file: Option<PathBuf>,
        #[arg(long)]
        path: Option<PathBuf>,
        #[arg(long, value_enum, default_value = "parsed")]
        evidence: EditorEvidenceArg,
        /// Required for live evidence; live diagnostics never start services
        #[arg(long, requires_if("live", "evidence"))]
        datasource: Option<String>,
    },
}
