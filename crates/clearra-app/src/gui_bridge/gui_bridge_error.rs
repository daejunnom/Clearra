#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GuiBridgeErrorCode {
    InvalidLineCount,
    UnsupportedProblemPreset,
    UnsupportedBackend,
    UnknownBackendOption,
    UnsupportedRule,
    UnsupportedLineTarget,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuiBridgeError {
    code: GuiBridgeErrorCode,
    message: String,
}

impl GuiBridgeError {
    pub fn new(code: GuiBridgeErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}
impl GuiBridgeError {
    pub fn code(&self) -> GuiBridgeErrorCode {
        self.code
    }
}
impl GuiBridgeError {
    pub fn message(&self) -> &str {
        &self.message
    }
}
