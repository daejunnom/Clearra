use clearra_validation::diagnostic::{diagnostic::Diagnostic, diagnostic_report::DiagnosticReport};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct GuiValidationSummary {
    report: DiagnosticReport,
}

impl GuiValidationSummary {
    pub fn new() -> Self {
        Self::default()
    }
}
impl GuiValidationSummary {
    pub fn push(&mut self, diagnostic: Diagnostic) {
        self.report.push(diagnostic);
    }
}
impl GuiValidationSummary {
    pub fn append(&mut self, other: Self) {
        self.report.append(other.report);
    }
}
impl GuiValidationSummary {
    pub fn diagnostics(&self) -> &[Diagnostic] {
        self.report.diagnostics()
    }
}
impl GuiValidationSummary {
    pub fn has_errors(&self) -> bool {
        self.report.has_errors()
    }
}
impl GuiValidationSummary {
    pub fn is_valid(&self) -> bool {
        !self.has_errors()
    }
}
impl GuiValidationSummary {
    pub fn into_report(self) -> DiagnosticReport {
        self.report
    }
}
