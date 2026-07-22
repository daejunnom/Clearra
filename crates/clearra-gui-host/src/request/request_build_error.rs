use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RequestBuildErrorCode {
    InvalidLineCount,
    UnknownBackend,
    InvalidGpuDevice,
    InvalidBudget,
    UnknownPiece,
    UnsupportedRule,
    UnsupportedProblemForm,
    ValidationFailed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequestBuildError {
    code: RequestBuildErrorCode,
    message: String,
}

impl RequestBuildError {
    pub fn new(code: RequestBuildErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}
impl RequestBuildError {
    pub const fn code(&self) -> RequestBuildErrorCode {
        self.code
    }
}
impl RequestBuildError {
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for RequestBuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.code, self.message)
    }
}

impl std::error::Error for RequestBuildError {}
