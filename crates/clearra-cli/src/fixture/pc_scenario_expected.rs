use std::collections::BTreeSet;

use crate::fixture::{
    pc_scenario_fixture::PcScenarioFixture,
    pc_scenario_unsupported::{
        expected_total_solution_count_label, field_value, PcScenarioUnsupportedVerifier,
    },
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PcScenarioExpectedVerifier;

impl PcScenarioExpectedVerifier {
    pub fn verify(
        verify_expected: bool,
        fixture: Option<&PcScenarioFixture>,
        result_fields: &[(String, String)],
    ) -> Result<Vec<(String, String)>, String> {
        if !verify_expected {
            return Ok(vec![("expected_checked".to_owned(), "false".to_owned())]);
        }
        let Some(fixture) = fixture else {
            return Ok(vec![
                ("expected_checked".to_owned(), "false".to_owned()),
                (
                    "expected_skip_reason".to_owned(),
                    "inline_scenario_has_no_expected_contract".to_owned(),
                ),
            ]);
        };

        let expected = fixture.expected();
        if expected.unsupported() {
            return PcScenarioUnsupportedVerifier::verify_search(expected, result_fields);
        }

        let mut mismatches = Vec::new();
        compare_bool_field(
            result_fields,
            "solution_found",
            expected.solution_exists(),
            &mut mismatches,
        );
        if let Some(count_complete) = expected.count_complete() {
            compare_bool_field(
                result_fields,
                "count_complete",
                count_complete,
                &mut mismatches,
            );
        }
        if let Some(expected_total) = expected.expected_total_solution_count() {
            compare_usize_field(
                result_fields,
                "total_solution_count",
                expected_total,
                &mut mismatches,
            );
        }
        let retained_trace_keys_checked = compare_accepted_retained_trace_keys(
            result_fields,
            expected.accepted_retained_trace_keys(),
            &mut mismatches,
        );

        if !mismatches.is_empty() {
            return Err(mismatches.join("; "));
        }

        let mut fields = vec![
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
                optional_bool_label(expected.count_complete()),
            ),
            (
                "expected_unsupported".to_owned(),
                expected.unsupported().to_string(),
            ),
            (
                "expected_retained_trace_key_count".to_owned(),
                expected.accepted_retained_trace_keys().len().to_string(),
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
        ];
        if let Some(reason) = expected.unsupported_reason() {
            fields.push(("expected_unsupported_reason".to_owned(), reason.to_owned()));
        }
        Ok(fields)
    }
}

fn optional_bool_label(value: Option<bool>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "none".to_owned())
}

fn compare_bool_field(
    fields: &[(String, String)],
    key: &str,
    expected: bool,
    mismatches: &mut Vec<String>,
) {
    compare_string_field(fields, key, &expected.to_string(), mismatches);
}

fn compare_usize_field(
    fields: &[(String, String)],
    key: &str,
    expected: usize,
    mismatches: &mut Vec<String>,
) {
    compare_string_field(fields, key, &expected.to_string(), mismatches);
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

fn compare_accepted_retained_trace_keys(
    fields: &[(String, String)],
    accepted_trace_keys: &[String],
    mismatches: &mut Vec<String>,
) -> bool {
    if accepted_trace_keys.is_empty() {
        return false;
    }

    let Some(actual_field) = field_value(fields, "retained_trace_keys") else {
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

    let accepted = accepted_trace_keys.iter().collect::<BTreeSet<_>>();
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

fn parse_trace_key_list(value: &str) -> Vec<String> {
    if value.is_empty() || value == "none" {
        return Vec::new();
    }
    value.split(',').map(ToOwned::to_owned).collect()
}
