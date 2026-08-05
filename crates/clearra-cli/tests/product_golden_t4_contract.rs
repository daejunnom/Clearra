#![cfg(feature = "native-c-core")]

use crate::{exit::ExitCode, output::CliOutput, run_with_args};
use serde_json::Value;
use std::path::PathBuf;

#[allow(dead_code)]
#[path = "product_contract_json_assert.rs"]
mod product_contract_json_assert;

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

fn json_output(args: &[&str]) -> String {
    let output = cli_output(args);
    assert_eq!(
        output.exit_code(),
        ExitCode::Success,
        "stderr={}",
        output.stderr()
    );
    assert!(output.stderr().is_empty(), "stderr={}", output.stderr());
    output.stdout().to_owned()
}

fn json_stdout_from_owned_args(args: Vec<String>) -> String {
    let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
    json_output(&refs)
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

fn product_fixture_stdout(path: &str) -> String {
    let fixture = read_workspace_json(path);
    let mut args = vec!["--format".to_owned(), "json".to_owned()];
    args.extend(fixture_command(&fixture, "command"));
    json_stdout_from_owned_args(args)
}

fn required_markers(golden: &str) -> Vec<String> {
    let value: Value = serde_json::from_str(golden).expect("golden json");
    value["required_markers"]
        .as_array()
        .expect("required_markers")
        .iter()
        .map(|marker| marker.as_str().expect("string marker").to_owned())
        .collect()
}

fn scalar(value: &Value) -> Option<String> {
    match value {
        Value::Null => Some("null".to_owned()),
        Value::Bool(value) => Some(value.to_string()),
        Value::Number(value) => Some(value.to_string()),
        Value::String(value) => Some(value.clone()),
        Value::Array(_) | Value::Object(_) => None,
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

fn output_marker_text(stdout: &str) -> String {
    let mut markers = stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();

    if let Ok(json) = serde_json::from_str::<Value>(stdout.trim()) {
        collect_json_markers(&json, "", &mut markers);
    }

    markers.join("\n")
}

fn assert_markers(case_name: &str, marker_text: &str, golden: &str) {
    for marker in required_markers(golden) {
        assert!(
            marker_text.contains(&marker),
            "{case_name} missing marker {marker:?}\n{marker_text}"
        );
    }
}

#[test]
fn pc_4l_fixed_candidate_budget_golden_contract_is_stable() {
    let fixture = read_workspace_json("tests/fixtures/product/pc_4l_fixed_candidate_budget.json");
    let mut args = vec!["--format".to_owned(), "json".to_owned()];
    args.extend(fixture_command(&fixture, "command"));
    let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
    let output = cli_output(&refs);

    assert_markers(
        "T4 pc 4L fixed candidate budget command",
        &output_marker_text(output.stdout()),
        include_str!("../../../tests/golden/product/pc_4l_fixed_candidate_budget.json"),
    );
    assert_eq!(output.exit_code(), ExitCode::Success);
    assert!(output.stderr().is_empty());
}

#[test]
fn scenario_clear_to_empty_golden_contract_is_stable() {
    let stdout = product_fixture_stdout("tests/fixtures/product/scenario_clear_to_empty.json");
    let json = product_contract_json_assert::json_from_stdout(&stdout);

    assert_markers(
        "T4 scenario clear-to-empty command",
        &output_marker_text(&stdout),
        include_str!("../../../tests/golden/product/scenario_clear_to_empty.json"),
    );
    assert_eq!(
        product_contract_json_assert::string_field(&json, "problem_preset"),
        "scenario-pc"
    );
    assert_eq!(
        product_contract_json_assert::string_field(&json, "completion_goal"),
        "clear-to-empty"
    );
    assert!(product_contract_json_assert::bool_field(
        &json,
        "solution_found"
    ));
}

#[test]
fn percent_uniform_bag_golden_contract_is_stable() {
    let stdout = product_fixture_stdout("tests/fixtures/product/percent_uniform_bag.json");
    let json = product_contract_json_assert::json_from_stdout(&stdout);

    assert_markers(
        "T4 percent uniform bag command",
        &output_marker_text(&stdout),
        include_str!("../../../tests/golden/product/percent_uniform_bag.json"),
    );
    assert_eq!(
        product_contract_json_assert::string_field(&json, "kind"),
        "percent"
    );
    assert_eq!(
        product_contract_json_assert::string_field(&json, "coverage_reducer"),
        "pattern-bitset-union"
    );
    assert_eq!(
        product_contract_json_assert::bool_field(&json, "probability_complete"),
        cfg!(feature = "native-c-core")
    );
    assert!(!product_contract_json_assert::bool_field(
        &json,
        "renormalized"
    ));
}

#[test]
fn rules_verify_basic_golden_contract_is_stable() {
    let stdout = product_fixture_stdout("tests/fixtures/product/rules_verify_basic.json");
    let json = product_contract_json_assert::json_from_stdout(&stdout);

    assert_markers(
        "T4 rules verify basic command",
        &output_marker_text(&stdout),
        include_str!("../../../tests/golden/product/rules_verify_basic.json"),
    );
    assert_eq!(
        product_contract_json_assert::string_field(&json, "kind"),
        "verify-kicks"
    );
    assert_eq!(
        product_contract_json_assert::string_field(&json, "status"),
        "verified"
    );
    assert_eq!(
        product_contract_json_assert::number_field(&json, "kick_verification_failures"),
        0.0
    );
}
