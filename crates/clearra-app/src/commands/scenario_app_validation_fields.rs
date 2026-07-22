use clearra_validation::diagnostic::{
    diagnostic::Diagnostic, diagnostic_code::DiagnosticSeverity,
    diagnostic_report::DiagnosticReport,
};

pub(super) fn validation_fields(report: &DiagnosticReport) -> Vec<(String, String)> {
    let first_error = report
        .diagnostics()
        .iter()
        .find(|diagnostic| diagnostic.severity() == DiagnosticSeverity::Error);
    vec![
        (
            "validation_error_count".to_owned(),
            report
                .diagnostics()
                .iter()
                .filter(|diagnostic| diagnostic.severity() == DiagnosticSeverity::Error)
                .count()
                .to_string(),
        ),
        (
            "validation_code".to_owned(),
            first_error
                .map(|diagnostic| diagnostic.code().as_str())
                .unwrap_or("none")
                .to_owned(),
        ),
        (
            "validation_reason".to_owned(),
            first_diagnostic_reason(report).unwrap_or("none").to_owned(),
        ),
    ]
}

pub(super) fn first_diagnostic_reason(report: &DiagnosticReport) -> Option<&str> {
    report.diagnostics().iter().find_map(diagnostic_reason)
}

fn diagnostic_reason(diagnostic: &Diagnostic) -> Option<&str> {
    diagnostic
        .evidence()
        .iter()
        .find_map(|evidence| (evidence.key() == "reason").then_some(evidence.value()))
}

pub(super) fn report_contains_reason(report: &DiagnosticReport, reason: &str) -> bool {
    report
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic_reason(diagnostic) == Some(reason))
}
