#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Checked, and fine.
    Ok,
    /// Checked, and broken in a way that will stop the app from working.
    Fail,
    /// Worth knowing, but not on its own a reason the app will not start.
    Warn,
    /// Could not be checked from here (a tool is missing, or the check would
    /// need the app running). Never counted as a failure.
    Skip,
}

impl Status {
    /// The machine-readable spelling, which is deliberately *not* the display
    /// mark: `--` reads as "skipped" to a person and as nothing to a parser.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Status::Ok => "ok",
            Status::Fail => "fail",
            Status::Warn => "warn",
            Status::Skip => "skip",
        }
    }

    pub(crate) fn mark(self) -> &'static str {
        match self {
            Status::Ok => "ok  ",
            Status::Fail => "FAIL",
            Status::Warn => "warn",
            Status::Skip => "--  ",
        }
    }
}

pub struct Check {
    pub(crate) status: Status,
    pub(crate) title: String,
    pub(crate) detail: String,
    /// The command that fixes it. Empty when there is nothing to run.
    pub(crate) fix: String,
}

impl Check {
    pub fn new(status: Status, title: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            status,
            title: title.into(),
            detail: detail.into(),
            fix: String::new(),
        }
    }

    pub fn fix(mut self, command: impl Into<String>) -> Self {
        self.fix = command.into();
        self
    }
}
