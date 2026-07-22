use clearra_validation::diagnostic::diagnostic_report::DiagnosticReport;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AppDiagnosticReport {
    validation: DiagnosticReport,
}

impl AppDiagnosticReport {
    pub fn new(validation: DiagnosticReport) -> Self {
        Self { validation }
    }
}
impl AppDiagnosticReport {
    pub fn empty() -> Self {
        Self::default()
    }
}
impl AppDiagnosticReport {
    pub fn validation(&self) -> &DiagnosticReport {
        &self.validation
    }
}
impl AppDiagnosticReport {
    pub fn append(&mut self, report: DiagnosticReport) {
        self.validation.append(report);
    }
}
impl AppDiagnosticReport {
    pub fn has_errors(&self) -> bool {
        self.validation.has_errors()
    }
}
