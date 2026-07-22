use crate::diagnostic::diagnostic_report::DiagnosticReport;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CompatibilityReport {
    diagnostics: DiagnosticReport,
}

impl CompatibilityReport {
    pub fn new(diagnostics: DiagnosticReport) -> Self {
        Self { diagnostics }
    }
}
impl CompatibilityReport {
    pub fn diagnostics(&self) -> &DiagnosticReport {
        &self.diagnostics
    }
}
impl CompatibilityReport {
    pub fn has_errors(&self) -> bool {
        self.diagnostics.has_errors()
    }
}
