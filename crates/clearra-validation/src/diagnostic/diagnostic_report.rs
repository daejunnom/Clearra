use crate::diagnostic::{
    diagnostic::Diagnostic,
    diagnostic_code::{DiagnosticCode, DiagnosticSeverity},
};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DiagnosticReport {
    diagnostics: Vec<Diagnostic>,
}

impl DiagnosticReport {
    pub fn new() -> Self {
        Self::default()
    }
}
impl DiagnosticReport {
    pub fn push(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }
}
impl DiagnosticReport {
    pub fn extend(&mut self, diagnostics: impl IntoIterator<Item = Diagnostic>) {
        self.diagnostics.extend(diagnostics);
    }
}
impl DiagnosticReport {
    pub fn append(&mut self, mut other: DiagnosticReport) {
        self.diagnostics.append(&mut other.diagnostics);
    }
}
impl DiagnosticReport {
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
}
impl DiagnosticReport {
    pub fn is_empty(&self) -> bool {
        self.diagnostics.is_empty()
    }
}
impl DiagnosticReport {
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity() == DiagnosticSeverity::Error)
    }
}
impl DiagnosticReport {
    pub fn contains_code(&self, code: DiagnosticCode) -> bool {
        self.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code() == code)
    }
}
