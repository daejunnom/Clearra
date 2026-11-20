use std::path::PathBuf;

use serde_json::Value;

use crate::{execution_resource_test_support::execution_resource_test_guard, exit::ExitCode};

pub(super) fn workspace_path(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}

pub(super) fn json_value(output: &crate::output::CliOutput) -> Value {
    assert_eq!(
        output.exit_code(),
        ExitCode::Success,
        "stdout={}\nstderr={}",
        output.stdout(),
        output.stderr()
    );
    assert!(output.stderr().is_empty(), "{}", output.stderr());
    serde_json::from_str(output.stdout()).expect("product CLI JSON")
}

pub(super) fn fixture(path: &str) -> Value {
    let content = std::fs::read_to_string(workspace_path(path)).expect("read product fixture");
    serde_json::from_str(&content).expect("product fixture JSON")
}

pub(super) fn fixture_args(fixture: &Value, key: &str) -> Vec<String> {
    fixture[key]
        .as_array()
        .unwrap_or_else(|| panic!("fixture must specify {key}"))
        .iter()
        .map(|value| value.as_str().expect("fixture CLI argument").to_owned())
        .collect()
}

pub(super) fn run_json<I, S>(args: I) -> Value
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let _guard = execution_resource_test_guard();
    json_value(&crate::run_with_args(
        [
            "clearra".to_owned(),
            "--format".to_owned(),
            "json".to_owned(),
        ]
        .into_iter()
        .chain(args.into_iter().map(Into::into)),
    ))
}

pub(super) fn fixture_json(path: &str) -> Value {
    fixture_json_with_flags(path, &[])
}

pub(super) fn fixture_json_with_flags(path: &str, flags: &[&str]) -> Value {
    let example = fixture(path);
    let mut args = fixture_args(&example, "command");
    if let Some(path) = example["input_fumen"].as_str() {
        let source = std::fs::read_to_string(workspace_path(path)).expect("read Fumen fixture");
        args.extend(["--input".to_owned(), source.trim().to_owned()]);
    }
    args.extend(flags.iter().map(|flag| (*flag).to_owned()));
    run_json(args)
}

pub(super) fn number(summary: &Value, field: &str) -> f64 {
    summary[field]
        .as_f64()
        .or_else(|| summary[field].as_str()?.parse().ok())
        .unwrap_or_else(|| panic!("numeric {field} missing from {summary}"))
}

pub(super) fn bool_value(summary: &Value, field: &str) -> bool {
    summary[field]
        .as_bool()
        .or_else(|| summary[field].as_str()?.parse().ok())
        .unwrap_or_else(|| panic!("boolean {field} missing from {summary}"))
}

pub(super) fn assert_exact_pc(value: &Value, kind: &str, solution_count: u64) {
    assert_eq!(value["kind"], kind);
    let summary = &value["summary"];
    assert_eq!(summary["actual_backend"], "wasm-cpu", "{summary}");
    assert_eq!(summary["solution_found"], true);
    assert_eq!(summary["count_complete"], true);
    assert_eq!(summary["probability_complete"], true);
    assert_eq!(summary["covered_pattern_count"], 1);
    assert_eq!(summary["coverage_probability"].as_f64(), Some(1.0));
    assert_eq!(summary["normalized_unique_solution_count"], solution_count);
    assert!(
        summary["normalized_solution_set_hash"]
            .as_str()
            .is_some_and(|key| key.starts_with("cts1:")),
        "{summary}"
    );
}
