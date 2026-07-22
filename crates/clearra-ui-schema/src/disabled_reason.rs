use clearra_validation::diagnostic::diagnostic_code::DiagnosticCode;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiDisabledReason {
    code: DiagnosticCode,
    reason: String,
}

impl UiDisabledReason {
    pub fn new(code: DiagnosticCode, reason: impl Into<String>) -> Self {
        Self {
            code,
            reason: reason.into(),
        }
    }
}
impl UiDisabledReason {
    pub fn code(&self) -> DiagnosticCode {
        self.code
    }
}
impl UiDisabledReason {
    pub fn code_str(&self) -> &'static str {
        self.code.as_str()
    }
}
impl UiDisabledReason {
    pub fn reason(&self) -> &str {
        &self.reason
    }
}
