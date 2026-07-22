use clearra_validation::diagnostic::{
    diagnostic::Diagnostic, diagnostic_code::DiagnosticSeverity,
    diagnostic_report::DiagnosticReport,
};

use crate::fixture::ScenarioFixtureExpected;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PcScenarioUnsupportedVerifier;

impl PcScenarioUnsupportedVerifier {
    pub fn verify_validation(
        expected: &ScenarioFixtureExpected,
        report: &DiagnosticReport,
    ) -> Result<Vec<(String, String)>, String> {
        if !expected.unsupported() {
            return Err("fixture expected supported search but validation failed".to_owned());
        }
        if !report.has_errors() {
            return Err("fixture expected unsupported but validation had no errors".to_owned());
        }

        let actual_reason = first_diagnostic_reason(report).unwrap_or("none");
        if let Some(expected_reason) = expected.unsupported_reason() {
            if !report_contains_reason(report, expected_reason) {
                return Err(format!(
                    "unsupported_reason expected {expected_reason} but actual {actual_reason}"
                ));
            }
        }

        let mut fields = expected_unsupported_fields(expected, actual_reason, "validation");
        fields.extend([
            ("solution_found".to_owned(), "false".to_owned()),
            ("total_solution_count".to_owned(), "0".to_owned()),
            ("count_complete".to_owned(), "false".to_owned()),
        ]);
        Ok(fields)
    }
}
impl PcScenarioUnsupportedVerifier {
    pub fn verify_search(
        expected: &ScenarioFixtureExpected,
        result_fields: &[(String, String)],
    ) -> Result<Vec<(String, String)>, String> {
        let actual_reason =
            field_value(result_fields, "search_unsupported_reason").unwrap_or("none");
        if actual_reason == "none" {
            return Err("fixture expected unsupported but scenario search completed".to_owned());
        }
        if let Some(expected_reason) = expected.unsupported_reason() {
            if actual_reason != expected_reason {
                return Err(format!(
                    "unsupported_reason expected {expected_reason} but actual {actual_reason}"
                ));
            }
        }

        Ok(expected_unsupported_fields(
            expected,
            actual_reason,
            "search",
        ))
    }
}
impl PcScenarioUnsupportedVerifier {
    pub fn validation_fields(report: &DiagnosticReport) -> Vec<(String, String)> {
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
}

fn expected_unsupported_fields(
    expected: &ScenarioFixtureExpected,
    actual_reason: &str,
    stage: &str,
) -> Vec<(String, String)> {
    vec![
        ("expected_checked".to_owned(), "true".to_owned()),
        ("expected_match".to_owned(), "true".to_owned()),
        (
            "expected_solution_exists".to_owned(),
            expected.solution_exists().to_string(),
        ),
        (
            "expected_total_solution_count".to_owned(),
            expected_total_solution_count_label(expected.expected_total_solution_count()),
        ),
        (
            "expected_count_complete".to_owned(),
            expected
                .count_complete()
                .map(|value| value.to_string())
                .unwrap_or_else(|| "none".to_owned()),
        ),
        ("expected_unsupported".to_owned(), "true".to_owned()),
        (
            "expected_unsupported_reason".to_owned(),
            expected
                .unsupported_reason()
                .unwrap_or(match stage {
                    "validation" => "any_validation_error",
                    _ => "any_search_unsupported",
                })
                .to_owned(),
        ),
        ("actual_unsupported".to_owned(), "true".to_owned()),
        ("unsupported_stage".to_owned(), stage.to_owned()),
        (
            "actual_unsupported_reason".to_owned(),
            actual_reason.to_owned(),
        ),
        (
            "status".to_owned(),
            "scenario-unsupported-expected".to_owned(),
        ),
    ]
}

pub(crate) fn field_value<'a>(fields: &'a [(String, String)], key: &str) -> Option<&'a str> {
    fields
        .iter()
        .find_map(|(field_key, value)| (field_key == key).then_some(value.as_str()))
}

pub(crate) fn expected_total_solution_count_label(expected: Option<usize>) -> String {
    expected
        .map(|value| value.to_string())
        .unwrap_or_else(|| "none".to_owned())
}

fn first_diagnostic_reason(report: &DiagnosticReport) -> Option<&str> {
    report.diagnostics().iter().find_map(diagnostic_reason)
}

fn diagnostic_reason(diagnostic: &Diagnostic) -> Option<&str> {
    diagnostic
        .evidence()
        .iter()
        .find_map(|evidence| (evidence.key() == "reason").then_some(evidence.value()))
}

fn report_contains_reason(report: &DiagnosticReport, reason: &str) -> bool {
    report
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic_reason(diagnostic) == Some(reason))
}
