use clearra_validation::diagnostic::{
    diagnostic::Diagnostic, diagnostic_code::DiagnosticSeverity,
    diagnostic_report::DiagnosticReport,
};

use crate::output::{CommandRenderer, RenderField, RenderFieldValue, RenderFormat};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DiagnosticPrinter;

impl DiagnosticPrinter {
    pub fn render(report: &DiagnosticReport) -> String {
        report
            .diagnostics()
            .iter()
            .map(render_diagnostic_text)
            .collect::<Vec<_>>()
            .join("\n")
    }
}
impl DiagnosticPrinter {
    pub fn render_json(report: &DiagnosticReport) -> String {
        CommandRenderer::render("diagnostic", diagnostic_fields(report), RenderFormat::Json)
            .expect("diagnostic JSON is not Fumen output")
    }
}

fn render_diagnostic_text(diagnostic: &Diagnostic) -> String {
    let mut lines = vec![format!(
        "{} {} {}",
        severity_label(diagnostic.severity()),
        diagnostic.code().as_str(),
        diagnostic.message()
    )];
    if let Some(location) = diagnostic.location() {
        let location_text = match location.index() {
            Some(index) => format!("{}#{index}", location.path()),
            None => location.path().to_owned(),
        };
        lines.push(format!("  location: {location_text}"));
    }
    if !diagnostic.evidence().is_empty() {
        let evidence = diagnostic
            .evidence()
            .iter()
            .map(|item| format!("{}={}", item.key(), item.value()))
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(format!("  evidence: {evidence}"));
    }
    if let Some(next_step) = diagnostic.suggested_next_step() {
        lines.push(format!("  next: {}", next_step.text()));
    }
    lines.join("\n")
}

fn severity_label(severity: DiagnosticSeverity) -> &'static str {
    match severity {
        DiagnosticSeverity::Error => "error",
        DiagnosticSeverity::Warning => "warning",
        DiagnosticSeverity::Info => "info",
    }
}

fn diagnostic_fields(report: &DiagnosticReport) -> Vec<RenderField> {
    vec![
        RenderField::new("diagnostic_count", report.diagnostics().len()),
        RenderField::new(
            "diagnostics",
            RenderFieldValue::array(report.diagnostics().iter().map(diagnostic_value)),
        ),
    ]
}

fn diagnostic_value(diagnostic: &Diagnostic) -> RenderFieldValue {
    RenderFieldValue::object([
        ("code", RenderFieldValue::string(diagnostic.code().as_str())),
        (
            "severity",
            RenderFieldValue::string(severity_label(diagnostic.severity())),
        ),
        ("message", RenderFieldValue::string(diagnostic.message())),
        (
            "location",
            diagnostic
                .location()
                .map(|location| RenderFieldValue::string(location.path()))
                .unwrap_or(RenderFieldValue::Null),
        ),
        (
            "suggested_next_step",
            diagnostic
                .suggested_next_step()
                .map(|next_step| RenderFieldValue::string(next_step.text()))
                .unwrap_or(RenderFieldValue::Null),
        ),
        ("evidence", diagnostic_evidence_value(diagnostic)),
    ])
}

fn diagnostic_evidence_value(diagnostic: &Diagnostic) -> RenderFieldValue {
    RenderFieldValue::object(diagnostic.evidence().iter().map(|item| {
        (
            item.key().to_owned(),
            RenderFieldValue::string(item.value()),
        )
    }))
}

#[cfg(test)]
#[path = "diagnostic_printer_tests.rs"]
mod tests;
