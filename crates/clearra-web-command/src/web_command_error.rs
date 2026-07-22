use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WebCommandErrorCode {
    EmptyCommand,
    UnsupportedCommand,
    MissingValue,
    InvalidValue,
    NativePathSemantics,
    ProcessSemantics,
}

impl WebCommandErrorCode {
    pub const fn as_diagnostic_code(self) -> &'static str {
        match self {
            Self::EmptyCommand => "E_WASM_COMMAND_EMPTY",
            Self::UnsupportedCommand => "E_WASM_COMMAND_UNSUPPORTED",
            Self::MissingValue => "E_WASM_COMMAND_MISSING_VALUE",
            Self::InvalidValue => "E_WASM_COMMAND_INVALID_VALUE",
            Self::NativePathSemantics => "E_WASM_NATIVE_PATH_FORBIDDEN",
            Self::ProcessSemantics => "E_WASM_PROCESS_SEMANTICS_FORBIDDEN",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebCommandError {
    code: WebCommandErrorCode,
    message: String,
}

impl WebCommandError {
    pub fn new(code: WebCommandErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}
impl WebCommandError {
    pub const fn code(&self) -> WebCommandErrorCode {
        self.code
    }
}
impl WebCommandError {
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for WebCommandError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.code, self.message)
    }
}

impl std::error::Error for WebCommandError {}
