use std::{fmt, path::PathBuf};

use clearra_validation::{
    diagnostic::{diagnostic::Diagnostic, diagnostic_code::DiagnosticCode},
    evidence::validation_evidence::ValidationEvidence,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingsErrorCode {
    ReadFailed,
    ParseFailed,
    WriteFailed,
}

impl SettingsErrorCode {
    pub const fn diagnostic_code(self) -> DiagnosticCode {
        match self {
            Self::ReadFailed | Self::ParseFailed => DiagnosticCode::WGuiSettingsLoadFailed,
            Self::WriteFailed => DiagnosticCode::WGuiSettingsSaveFailed,
        }
    }
}
impl SettingsErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ReadFailed => "read_failed",
            Self::ParseFailed => "parse_failed",
            Self::WriteFailed => "write_failed",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettingsError {
    code: SettingsErrorCode,
    path: PathBuf,
    message: String,
}

impl SettingsError {
    pub fn new(
        code: SettingsErrorCode,
        path: impl Into<PathBuf>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code,
            path: path.into(),
            message: message.into(),
        }
    }
}
impl SettingsError {
    pub const fn code(&self) -> SettingsErrorCode {
        self.code
    }
}
impl SettingsError {
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
}
impl SettingsError {
    pub fn message(&self) -> &str {
        &self.message
    }
}
impl SettingsError {
    pub fn to_diagnostic(&self) -> Diagnostic {
        Diagnostic::new(
            self.code.diagnostic_code(),
            format!(
                "GUI settings {} at {}; using defaults when loading is possible",
                self.code.as_str(),
                self.path.display()
            ),
        )
        .with_evidence(ValidationEvidence::new(
            "settings_path",
            self.path.display().to_string(),
        ))
        .with_evidence(ValidationEvidence::new(
            "settings_error",
            self.message.clone(),
        ))
    }
}

impl fmt::Display for SettingsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}: {} ({})",
            self.code.as_str(),
            self.message,
            self.path.display()
        )
    }
}

impl std::error::Error for SettingsError {}
