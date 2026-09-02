//! The global result encoding vocabulary.

/// How a command result is encoded.
#[derive(Clone, Copy, Debug, Eq, PartialEq, clap::ValueEnum)]
pub(crate) enum Output {
    Human,
    Json,
}

impl Output {
    pub(crate) fn is_json(self) -> bool {
        !matches!(self, Self::Human)
    }
}
