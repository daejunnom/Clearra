use clearra_validation::diagnostic::{
    diagnostic::Diagnostic, diagnostic_code::DiagnosticSeverity,
    diagnostic_report::DiagnosticReport,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildValidationDiagnosticSchema {
    severity: DiagnosticSeverity,
    code: String,
    message: String,
}

impl BuildValidationDiagnosticSchema {
    pub fn from_diagnostic(diagnostic: &Diagnostic) -> Self {
        Self {
            severity: diagnostic.severity(),
            code: diagnostic.code().as_str().to_owned(),
            message: diagnostic.message().to_owned(),
        }
    }
}
impl BuildValidationDiagnosticSchema {
    pub fn from_report(report: &DiagnosticReport) -> Vec<Self> {
        report
            .diagnostics()
            .iter()
            .map(Self::from_diagnostic)
            .collect()
    }
}
impl BuildValidationDiagnosticSchema {
    pub fn severity(&self) -> DiagnosticSeverity {
        self.severity
    }
}
impl BuildValidationDiagnosticSchema {
    pub fn code(&self) -> &str {
        &self.code
    }
}
impl BuildValidationDiagnosticSchema {
    pub fn message(&self) -> &str {
        &self.message
    }
}
