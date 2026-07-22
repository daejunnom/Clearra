use crate::diagnostic::diagnostic_code::DiagnosticCode;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisabledFeatureReason {
    code: DiagnosticCode,
    message: String,
}

impl DisabledFeatureReason {
    pub fn new(code: DiagnosticCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}
impl DisabledFeatureReason {
    pub fn code(&self) -> DiagnosticCode {
        self.code
    }
}
impl DisabledFeatureReason {
    pub fn message(&self) -> &str {
        &self.message
    }
}
