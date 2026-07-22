use std::collections::BTreeSet;

use clearra_core_executor::CoreExecutionResult;
use clearra_validation::diagnostic::diagnostic_report::DiagnosticReport;

use crate::commands::scenario_app_validation_fields::{
    first_diagnostic_reason, report_contains_reason,
};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ScenarioAppExpected {
    solution_exists: bool,
    count_complete: Option<bool>,
    expected_total_solution_count: Option<usize>,
    unsupported: bool,
    unsupported_reason: Option<String>,
    accepted_retained_trace_keys: Vec<String>,
    normalized_solution_oracle: Option<String>,
    expected_normalized_solution_set_hash: Option<String>,
    expected_normalized_solution_keys: Vec<String>,
    operation_replay_available: Option<bool>,
}

impl ScenarioAppExpected {
    pub fn new(solution_exists: bool, count_complete: Option<bool>) -> Self {
        Self {
            solution_exists,
            count_complete,
            expected_total_solution_count: None,
            unsupported: false,
            unsupported_reason: None,
            accepted_retained_trace_keys: Vec::new(),
            normalized_solution_oracle: None,
            expected_normalized_solution_set_hash: None,
            expected_normalized_solution_keys: Vec::new(),
            operation_replay_available: None,
        }
    }
}
impl ScenarioAppExpected {
    pub fn with_normalized_solution_set(
        mut self,
        oracle: Option<String>,
        hash: Option<String>,
        keys: Vec<String>,
        operation_replay_available: Option<bool>,
    ) -> Self {
        self.normalized_solution_oracle = oracle;
        self.expected_normalized_solution_set_hash = hash;
        self.expected_normalized_solution_keys = keys;
        self.operation_replay_available = operation_replay_available;
        self
    }
}
impl ScenarioAppExpected {
    pub fn with_expected_total_solution_count(mut self, value: Option<usize>) -> Self {
        self.expected_total_solution_count = value;
        self
    }
}
impl ScenarioAppExpected {
    pub fn with_unsupported(mut self, unsupported: bool, reason: Option<String>) -> Self {
        self.unsupported = unsupported;
        self.unsupported_reason = reason;
        self
    }
}
impl ScenarioAppExpected {
    pub fn with_accepted_retained_trace_keys(mut self, keys: Vec<String>) -> Self {
        self.accepted_retained_trace_keys = keys;
        self
    }
}
impl ScenarioAppExpected {
    pub(super) fn expects_unsupported(&self) -> bool {
        self.unsupported
    }
}
impl ScenarioAppExpected {
    pub(super) fn verify_validation(
        &self,
        report: &DiagnosticReport,
    ) -> Result<Vec<(String, String)>, String> {
        if !self.unsupported {
            return Err("fixture expected supported search but validation failed".to_owned());
        }
        if !report.has_errors() {
            return Err("fixture expected unsupported but validation had no errors".to_owned());
        }
        let actual_reason = first_diagnostic_reason(report).unwrap_or("none");
        if let Some(expected_reason) = self.unsupported_reason.as_deref() {
            if !report_contains_reason(report, expected_reason) {
                return Err(format!(
                    "unsupported_reason expected {expected_reason} but actual {actual_reason}"
                ));
            }
        }
        let mut fields = self.expected_unsupported_fields(actual_reason, "validation");
        fields.extend([
            ("solution_found".to_owned(), "false".to_owned()),
            ("total_solution_count".to_owned(), "0".to_owned()),
            ("count_complete".to_owned(), "false".to_owned()),
        ]);
        Ok(fields)
    }
}
impl ScenarioAppExpected {
    pub(super) fn verify_search(
        &self,
        result: &CoreExecutionResult,
    ) -> Result<Vec<(String, String)>, String> {
        let result_fields = result.summary_fields();
        if self.unsupported {
            let actual_reason =
                field_value(&result_fields, "search_unsupported_reason").unwrap_or("none");
            if actual_reason == "none" {
                return Err("fixture expected unsupported but scenario search completed".to_owned());
            }
            if let Some(expected_reason) = self.unsupported_reason.as_deref() {
                if actual_reason != expected_reason {
                    return Err(format!(
                        "unsupported_reason expected {expected_reason} but actual {actual_reason}"
                    ));
                }
            }
            return Ok(self.expected_unsupported_fields(actual_reason, "search"));
        }

        let mut mismatches = Vec::new();
        compare_string_field(
            &result_fields,
            "solution_found",
            &self.solution_exists.to_string(),
            &mut mismatches,
        );
        if let Some(count_complete) = self.count_complete {
            compare_string_field(
                &result_fields,
                "count_complete",
                &count_complete.to_string(),
                &mut mismatches,
            );
        }
        if let Some(expected_total) = self.expected_total_solution_count {
            compare_string_field(
                &result_fields,
                "total_solution_count",
                &expected_total.to_string(),
                &mut mismatches,
            );
        }
        let retained_trace_keys_checked =
            self.compare_accepted_retained_trace_keys(&result_fields, &mut mismatches);
        let normalized_solution_set_checked =
            self.compare_normalized_solution_set(result, &result_fields, &mut mismatches);
        if !mismatches.is_empty() {
            return Err(mismatches.join("; "));
        }

        Ok(vec![
            ("expected_checked".to_owned(), "true".to_owned()),
            ("expected_match".to_owned(), "true".to_owned()),
            (
                "expected_solution_exists".to_owned(),
                self.solution_exists.to_string(),
            ),
            (
                "expected_total_solution_count".to_owned(),
                expected_total_solution_count_label(self.expected_total_solution_count),
            ),
            (
                "expected_count_complete".to_owned(),
                optional_bool_label(self.count_complete),
            ),
            ("expected_unsupported".to_owned(), "false".to_owned()),
            (
                "expected_retained_trace_key_count".to_owned(),
                self.accepted_retained_trace_keys.len().to_string(),
            ),
            (
                "retained_trace_keys_match".to_owned(),
                if retained_trace_keys_checked {
                    "true"
                } else {
                    "none"
                }
                .to_owned(),
            ),
            (
                "retained_trace_keys_checked".to_owned(),
                retained_trace_keys_checked.to_string(),
            ),
            (
                "normalized_solution_set_checked".to_owned(),
                normalized_solution_set_checked.to_string(),
            ),
            (
                "normalized_solution_set_match".to_owned(),
                if normalized_solution_set_checked {
                    "true"
                } else {
                    "not_requested"
                }
                .to_owned(),
            ),
            (
                "normalized_solution_oracle".to_owned(),
                self.normalized_solution_oracle
                    .clone()
                    .unwrap_or_else(|| "not_requested".to_owned()),
            ),
            (
                "expected_normalized_solution_set_hash".to_owned(),
                self.expected_normalized_solution_set_hash
                    .clone()
                    .unwrap_or_else(|| "none".to_owned()),
            ),
            (
                "operation_replay_available".to_owned(),
                optional_bool_label(self.operation_replay_available),
            ),
            ("missing_solution_keys".to_owned(), "none".to_owned()),
            ("unexpected_solution_keys".to_owned(), "none".to_owned()),
        ])
    }
}
impl ScenarioAppExpected {
    fn compare_normalized_solution_set(
        &self,
        result: &CoreExecutionResult,
        result_fields: &[(String, String)],
        mismatches: &mut Vec<String>,
    ) -> bool {
        if self.expected_normalized_solution_keys.is_empty()
            && self.expected_normalized_solution_set_hash.is_none()
        {
            return false;
        }

        let expected = self
            .expected_normalized_solution_keys
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        let actual = result
            .normalized_solution_keys()
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        let missing = expected.difference(&actual).cloned().collect::<Vec<_>>();
        let unexpected = actual.difference(&expected).cloned().collect::<Vec<_>>();
        if !missing.is_empty() {
            mismatches.push(format!("missing_solution_keys={}", missing.join(";")));
        }
        if !unexpected.is_empty() {
            mismatches.push(format!("unexpected_solution_keys={}", unexpected.join(";")));
        }
        if let Some(expected_hash) = self.expected_normalized_solution_set_hash.as_deref() {
            compare_string_field(
                result_fields,
                "actual_normalized_solution_set_hash",
                expected_hash,
                mismatches,
            );
        }
        true
    }
}
impl ScenarioAppExpected {
    fn expected_unsupported_fields(
        &self,
        actual_reason: &str,
        stage: &str,
    ) -> Vec<(String, String)> {
        vec![
            ("expected_checked".to_owned(), "true".to_owned()),
            ("expected_match".to_owned(), "true".to_owned()),
            (
                "expected_solution_exists".to_owned(),
                self.solution_exists.to_string(),
            ),
            (
                "expected_total_solution_count".to_owned(),
                expected_total_solution_count_label(self.expected_total_solution_count),
            ),
            (
                "expected_count_complete".to_owned(),
                optional_bool_label(self.count_complete),
            ),
            ("expected_unsupported".to_owned(), "true".to_owned()),
            (
                "expected_unsupported_reason".to_owned(),
                self.unsupported_reason
                    .clone()
                    .unwrap_or_else(|| match stage {
                        "validation" => "any_validation_error".to_owned(),
                        _ => "any_search_unsupported".to_owned(),
                    }),
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
}
impl ScenarioAppExpected {
    fn compare_accepted_retained_trace_keys(
        &self,
        result_fields: &[(String, String)],
        mismatches: &mut Vec<String>,
    ) -> bool {
        if self.accepted_retained_trace_keys.is_empty() {
            return false;
        }
        let Some(actual_field) = field_value(result_fields, "retained_trace_keys") else {
            mismatches.push("retained_trace_keys missing from scenario search result".to_owned());
            return true;
        };
        let actual_keys = parse_trace_key_list(actual_field);
        if actual_keys.is_empty() {
            mismatches.push(
                "accepted_retained_trace_keys listed but no retained trace keys were exported"
                    .to_owned(),
            );
            return true;
        }

        let accepted = self
            .accepted_retained_trace_keys
            .iter()
            .collect::<BTreeSet<_>>();
        let unexpected = actual_keys
            .iter()
            .filter(|key| !accepted.contains(*key))
            .cloned()
            .collect::<Vec<_>>();
        if !unexpected.is_empty() {
            mismatches.push(format!(
                "retained_trace_keys include unaccepted retained keys: {}",
                unexpected.join(",")
            ));
        }
        true
    }
}

fn compare_string_field(
    fields: &[(String, String)],
    key: &str,
    expected: &str,
    mismatches: &mut Vec<String>,
) {
    match field_value(fields, key) {
        Some(actual) if actual == expected => {}
        Some(actual) => mismatches.push(format!("{key} expected {expected} but actual {actual}")),
        None => mismatches.push(format!("{key} missing from scenario search result")),
    }
}

fn field_value<'a>(fields: &'a [(String, String)], key: &str) -> Option<&'a str> {
    fields
        .iter()
        .find_map(|(field_key, value)| (field_key == key).then_some(value.as_str()))
}

fn expected_total_solution_count_label(expected: Option<usize>) -> String {
    expected
        .map(|value| value.to_string())
        .unwrap_or_else(|| "none".to_owned())
}

fn optional_bool_label(value: Option<bool>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "none".to_owned())
}

fn parse_trace_key_list(value: &str) -> Vec<String> {
    if value.is_empty() || value == "none" {
        return Vec::new();
    }
    value.split(',').map(ToOwned::to_owned).collect()
}
