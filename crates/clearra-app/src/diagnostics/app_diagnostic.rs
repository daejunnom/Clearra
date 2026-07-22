#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppDiagnostic {
    code: String,
    message: String,
}

impl AppDiagnostic {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}
impl AppDiagnostic {
    pub fn code(&self) -> &str {
        &self.code
    }
}
impl AppDiagnostic {
    pub fn message(&self) -> &str {
        &self.message
    }
}
