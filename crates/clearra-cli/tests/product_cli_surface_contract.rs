#![cfg(feature = "native-c-core")]

use crate::{exit::ExitCode, output::CliOutput, run_with_args};
use serde_json::Value;
use std::path::PathBuf;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("workspace root")
        .to_path_buf()
}

fn cli_output(args: &[&str]) -> CliOutput {
    run_with_args(
        std::iter::once("clearra")
            .chain(args.iter().copied())
            .map(str::to_owned),
    )
}

fn json_stdout_from_owned_args(args: Vec<String>) -> String {
    let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
    let output = cli_output(&refs);
    assert!(
        output.exit_code() == ExitCode::Success,
        "expected success\nstdout:\n{}\nstderr:\n{}",
        output.stdout(),
        output.stderr()
    );
    assert!(output.stderr().is_empty(), "unexpected stderr");
    output.stdout().to_owned()
}

fn product_fixture_stdout(path: &str) -> String {
    let fixture = read_workspace_json(path);
    let mut args = vec!["--format".to_owned(), "json".to_owned()];
    args.extend(fixture_command(&fixture, "command"));
    json_stdout_from_owned_args(args)
}

fn read_workspace_json(path: &str) -> Value {
    let text = std::fs::read_to_string(workspace_root().join(path)).expect("workspace json");
    serde_json::from_str(&text).expect("workspace json value")
}

fn fixture_command(fixture: &Value, key: &str) -> Vec<String> {
    fixture[key]
        .as_array()
        .unwrap_or_else(|| panic!("fixture must contain {key} command array"))
        .iter()
        .map(|arg| arg.as_str().expect("command arg string").to_owned())
        .collect()
}

fn assert_markers(case_name: &str, marker_text: &str, golden: &str) {
    let golden: Value = serde_json::from_str(golden).expect("golden json");
    let required = golden["required_markers"]
        .as_array()
        .expect("required_markers array");
    for marker in required {
        let marker = marker.as_str().expect("marker string");
        assert!(
            marker_text.contains(marker),
            "{case_name} missing marker '{marker}' in:\n{marker_text}"
        );
    }
}

fn output_marker_text(stdout: &str) -> String {
    let json = json_from_stdout(stdout);
    let mut markers = stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| line.replace(": ", "="))
        .collect::<Vec<_>>();
    collect_json_markers(&json, "", &mut markers);
    markers.join("\n")
}

fn json_from_stdout(stdout: &str) -> Value {
    serde_json::from_str(stdout).expect("valid json stdout")
}

fn string_field<'a>(json: &'a Value, field_name: &str) -> &'a str {
    find_field(json, field_name)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("missing string field {field_name}: {json}"))
}

fn bool_field(json: &Value, field_name: &str) -> bool {
    find_field(json, field_name)
        .and_then(bool_value)
        .unwrap_or_else(|| panic!("missing bool field {field_name}: {json}"))
}

fn number_field(json: &Value, field_name: &str) -> f64 {
    find_field(json, field_name)
        .and_then(number_value)
        .unwrap_or_else(|| panic!("missing number field {field_name}: {json}"))
}

fn bool_value(value: &Value) -> Option<bool> {
    match value {
        Value::Bool(value) => Some(*value),
        Value::String(value) => match value.as_str() {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

fn number_value(value: &Value) -> Option<f64> {
    match value {
        Value::Number(value) => value.as_f64(),
        Value::String(value) => value.parse::<f64>().ok(),
        _ => None,
    }
}

fn find_field<'a>(json: &'a Value, field_name: &str) -> Option<&'a Value> {
    match json {
        Value::Object(map) => map
            .get(field_name)
            .or_else(|| map.values().find_map(|value| find_field(value, field_name))),
        Value::Array(values) => values
            .iter()
            .find_map(|value| find_field(value, field_name)),
        _ => None,
    }
}

fn collect_json_markers(value: &Value, path: &str, markers: &mut Vec<String>) {
    match value {
        Value::Object(object) => {
            for (key, child) in object {
                let next_path = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                collect_json_markers(child, &next_path, markers);
            }
        }
        Value::Array(items) => {
            for (index, child) in items.iter().enumerate() {
                collect_json_markers(child, &format!("{path}[{index}]"), markers);
            }
        }
        _ => {
            if let Some(value) = scalar(value) {
                markers.push(format!("{path}={value}"));
                if let Some(leaf) = path
                    .rsplit('.')
                    .next()
                    .map(|leaf| {
                        leaf.trim_end_matches(|c: char| c == ']' || c.is_ascii_digit() || c == '[')
                    })
                    .filter(|leaf| !leaf.is_empty() && *leaf != path)
                {
                    markers.push(format!("{leaf}={value}"));
                }
            }
        }
    }
}

fn scalar(value: &Value) -> Option<String> {
    match value {
        Value::Bool(value) => Some(value.to_string()),
        Value::Number(value) => Some(value.to_string()),
        Value::String(value) => Some(value.clone()),
        Value::Null => Some("null".to_owned()),
        _ => None,
    }
}

#[test]
fn path_reports_representative_trace() {
    let stdout = product_fixture_stdout("tests/fixtures/product/path_representative.json");
    let json = json_from_stdout(&stdout);

    assert_markers(
        "MVP1 path command",
        &output_marker_text(&stdout),
        include_str!("../../../tests/golden/product/path_representative.json"),
    );
    assert_eq!(string_field(&json, "route"), "search-problem-core-executor");
    assert!(bool_field(&json, "sample_trace_available"));
    assert!(bool_field(&json, "path_reports_representative_trace"));
    assert!(bool_field(&json, "retained_representative_trace"));
    assert!(bool_field(
        &json,
        "path_distinguishes_retained_trace_from_total_count"
    ));
    assert!(
        number_field(&json, "total_solution_count") >= number_field(&json, "retained_trace_count")
    );
}

#[test]
fn percent_reports_total_and_covered_pattern_count() {
    let stdout = product_fixture_stdout("tests/fixtures/product/percent_bag_pattern.json");
    let json = json_from_stdout(&stdout);

    assert_markers(
        "MVP1 percent command",
        &output_marker_text(&stdout),
        include_str!("../../../tests/golden/product/percent_bag_pattern.json"),
    );
    assert_eq!(string_field(&json, "route"), "search-problem-core-executor");
    assert_eq!(number_field(&json, "total_pattern_count"), 1.0);
    assert_eq!(number_field(&json, "covered_pattern_count"), 1.0);
    assert_eq!(
        bool_field(&json, "probability_complete"),
        cfg!(feature = "native-c-core")
    );
    assert!(!bool_field(&json, "renormalized"));
    assert!(bool_field(&json, "percent_reports_total_pattern_count"));
    assert!(bool_field(&json, "percent_reports_covered_pattern_count"));
    assert!(bool_field(&json, "percent_reports_probability_complete"));
    assert_eq!(
        string_field(&json, "coverage_reducer"),
        "pattern-bitset-union"
    );
}

#[test]
fn cover_reports_build_union_probability() {
    let stdout = product_fixture_stdout("tests/fixtures/product/cover_template_basic.json");
    let json = json_from_stdout(&stdout);

    assert_markers(
        "MVP1 cover command",
        &output_marker_text(&stdout),
        include_str!("../../../tests/golden/product/cover_template_basic.json"),
    );
    assert_eq!(string_field(&json, "route"), "search-problem-core-executor");
    assert!(!bool_field(&json, "c_buildup_coverage_row_generated"));
    assert!(bool_field(&json, "coverage_row_identity_validated"));
    assert!(bool_field(&json, "cover_reports_union_probability"));
    assert!(bool_field(&json, "cover_reports_c_coverage_row_count"));
    assert!(bool_field(
        &json,
        "slot_assignment_count_is_not_success_probability"
    ));
    assert_eq!(
        string_field(&json, "success_probability_source"),
        "UnionProbability"
    );
    assert_eq!(number_field(&json, "c_coverage_row_count"), 0.0);
    assert_eq!(
        string_field(&json, "coverage_reducer"),
        "pattern-bitset-union"
    );
}
