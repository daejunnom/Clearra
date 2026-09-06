use super::*;

pub(super) fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("workspace root")
        .to_path_buf()
}

pub(super) fn workspace_path(path: &str) -> String {
    workspace_root().join(path).to_string_lossy().into_owned()
}

pub(super) fn cli_output(args: &[&str]) -> CliOutput {
    run_with_args(
        std::iter::once("clearra")
            .chain(args.iter().copied())
            .map(str::to_owned),
    )
}

pub(super) fn json_output(args: &[&str]) -> String {
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

pub(super) fn json_value(args: &[&str]) -> Value {
    product_contract_json_assert::json_from_stdout(&json_output(args))
}

pub(super) fn json_stdout_from_owned_args(args: Vec<String>) -> String {
    let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
    json_output(&refs)
}

pub(super) fn read_workspace_json(path: &str) -> Value {
    let text = std::fs::read_to_string(workspace_root().join(path)).expect("workspace json");
    serde_json::from_str(&text).expect("workspace json value")
}

pub(super) fn fixture_command(fixture: &Value, key: &str) -> Vec<String> {
    fixture[key]
        .as_array()
        .unwrap_or_else(|| panic!("fixture must contain {key} command array"))
        .iter()
        .map(|arg| arg.as_str().expect("command arg string").to_owned())
        .collect()
}

pub(super) fn product_fixture_stdout(path: &str) -> String {
    let fixture = read_workspace_json(path);
    let mut args = vec!["--format".to_owned(), "json".to_owned()];
    args.extend(fixture_command(&fixture, "command"));
    if let Some(input_fumen) = fixture["input_fumen"].as_str() {
        let input = std::fs::read_to_string(workspace_root().join(input_fumen))
            .expect("input fumen fixture");
        args.push("--input".to_owned());
        args.push(input.trim().to_owned());
    }
    json_stdout_from_owned_args(args)
}

pub(super) fn continue_fixture_stdout(path: &str) -> String {
    let fixture = read_workspace_json(path);
    let mut seed_args = vec!["--format".to_owned(), "json".to_owned()];
    seed_args.extend(fixture_command(&fixture, "seed_command"));
    let seed =
        product_contract_json_assert::json_from_stdout(&json_stdout_from_owned_args(seed_args));
    let token = product_contract_json_assert::string_field(&seed, "continuation_token");

    let mut args = vec!["--format".to_owned(), "json".to_owned()];
    args.extend(fixture_command(&fixture, "command").into_iter().map(|arg| {
        if arg == "<continuation_token>" {
            token.to_owned()
        } else {
            arg
        }
    }));
    json_stdout_from_owned_args(args)
}

pub(super) fn fixture_marker_text(fixture: &str, extra_markers: &[&str]) -> String {
    let mut markers = fixture
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let value: Value = serde_json::from_str(fixture).expect("fixture json");
    collect_json_markers(&value, "", &mut markers);
    markers.extend(extra_markers.iter().map(|marker| (*marker).to_owned()));

    if let Some(probability) = value.pointer("/expected/probability").and_then(scalar) {
        markers.push(format!("coverage_probability={probability}"));
    }
    if let Some(forbidden) = value
        .pointer("/expected/forbidden_algorithm")
        .and_then(Value::as_str)
    {
        markers.push(format!("{forbidden}=forbidden"));
    }

    markers.join("\n")
}

pub(super) fn required_markers(golden: &str) -> Vec<String> {
    let value: Value = serde_json::from_str(golden).expect("golden json");
    value["required_markers"]
        .as_array()
        .expect("required_markers")
        .iter()
        .map(|marker| marker.as_str().expect("string marker").to_owned())
        .collect()
}

pub(super) fn scalar(value: &Value) -> Option<String> {
    match value {
        Value::Null => Some("null".to_owned()),
        Value::Bool(value) => Some(value.to_string()),
        Value::Number(value) => Some(value.to_string()),
        Value::String(value) => Some(value.clone()),
        Value::Array(_) | Value::Object(_) => None,
    }
}

pub(super) fn collect_json_markers(value: &Value, path: &str, markers: &mut Vec<String>) {
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

pub(super) fn output_marker_text(stdout: &str) -> String {
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

pub(super) fn assert_markers(case_name: &str, marker_text: &str, golden: &str) {
    for marker in required_markers(golden) {
        assert!(
            marker_text.contains(&marker),
            "{case_name} missing marker {marker:?}\n{marker_text}"
        );
    }
}

pub(super) fn assert_same_number_field(field_name: &str, expected: &Value, actual: &Value) {
    let expected_value = product_contract_json_assert::number_field(expected, field_name);
    let actual_value = product_contract_json_assert::number_field(actual, field_name);
    assert!(
        (expected_value - actual_value).abs() < f64::EPSILON,
        "field {field_name}: expected {expected_value}, got {actual_value}"
    );
}

pub(super) fn assert_same_bool_field(field_name: &str, expected: &Value, actual: &Value) {
    assert_eq!(
        product_contract_json_assert::bool_field(expected, field_name),
        product_contract_json_assert::bool_field(actual, field_name),
        "field {field_name}"
    );
}

pub(super) fn assert_same_string_field(field_name: &str, expected: &Value, actual: &Value) {
    assert_eq!(
        product_contract_json_assert::string_field(expected, field_name),
        product_contract_json_assert::string_field(actual, field_name),
        "field {field_name}"
    );
}

pub(super) fn assert_backend_parity(expected: &Value, actual: &Value) {
    for field in ["coverage_probability", "covered_pattern_count"] {
        assert_same_number_field(field, expected, actual);
    }
    for field in ["next_pc_available", "continuation_token_available"] {
        assert_same_bool_field(field, expected, actual);
    }
    for field in ["continuation_token_version", "continue_hint"] {
        assert_same_string_field(field, expected, actual);
    }
}

pub(super) fn assert_stage_d_backend_equivalence(
    cpu: &Value,
    gpu_with_fallback: &Value,
    hybrid_with_fallback: &Value,
) {
    assert_backend_parity(cpu, gpu_with_fallback);
    assert_backend_parity(cpu, hybrid_with_fallback);

    assert_eq!(
        product_contract_json_assert::string_field(cpu, "backend_requested"),
        "cpu"
    );
    assert_eq!(
        product_contract_json_assert::string_field(cpu, "backend_selected"),
        "cpu"
    );
    assert!(!product_contract_json_assert::bool_field(
        cpu,
        "backend_fallback_used"
    ));

    assert_eq!(
        product_contract_json_assert::string_field(gpu_with_fallback, "backend_requested"),
        "gpu"
    );
    assert_eq!(
        product_contract_json_assert::string_field(gpu_with_fallback, "backend_selected"),
        "cpu"
    );
    assert!(product_contract_json_assert::bool_field(
        gpu_with_fallback,
        "backend_fallback_used"
    ));
    assert_gpu_unavailable_reason(product_contract_json_assert::string_field(
        gpu_with_fallback,
        "backend_fallback_reason",
    ));

    assert_eq!(
        product_contract_json_assert::string_field(hybrid_with_fallback, "backend_requested"),
        "hybrid"
    );
    assert_eq!(
        product_contract_json_assert::string_field(hybrid_with_fallback, "backend_selected"),
        "cpu"
    );
    assert!(!product_contract_json_assert::bool_field(
        hybrid_with_fallback,
        "backend_fallback_used",
    ));
    assert_eq!(
        backend_report_optional_string(hybrid_with_fallback, "backend_fallback_reason"),
        None
    );
}

pub(super) fn opening_2l_backend_values() -> (Value, Value, Value) {
    let cpu = json_value(&[
        "--format",
        "json",
        "pc",
        "--lines",
        "2",
        "--queue",
        "IIOOOIIOOO",
        "--fixed",
        "--no-hold",
        "--objective",
        "min-cover",
        "--backend",
        "cpu",
    ]);
    let gpu_with_fallback = json_value(&[
        "--format",
        "json",
        "pc",
        "--lines",
        "2",
        "--queue",
        "IIOOOIIOOO",
        "--fixed",
        "--no-hold",
        "--objective",
        "min-cover",
        "--backend",
        "gpu",
        "--allow-backend-fallback",
    ]);
    let hybrid_with_fallback = json_value(&[
        "--format",
        "json",
        "pc",
        "--lines",
        "2",
        "--queue",
        "IIOOOIIOOO",
        "--fixed",
        "--no-hold",
        "--objective",
        "min-cover",
        "--backend",
        "hybrid",
        "--allow-backend-fallback",
    ]);

    (cpu, gpu_with_fallback, hybrid_with_fallback)
}

pub(super) fn scenario_4l_backend_values() -> (Value, Value, Value) {
    let fixture = workspace_path("tests/fixtures/pc/scenario_simple_4l.json");
    let cpu = json_value(&[
        "--format",
        "json",
        "pc-scenario",
        "--fixture",
        &fixture,
        "--verify-expected",
        "--backend",
        "cpu",
    ]);
    let gpu_with_fallback = json_value(&[
        "--format",
        "json",
        "pc-scenario",
        "--fixture",
        &fixture,
        "--verify-expected",
        "--backend",
        "gpu",
        "--allow-backend-fallback",
    ]);
    let hybrid_with_fallback = json_value(&[
        "--format",
        "json",
        "pc-scenario",
        "--fixture",
        &fixture,
        "--verify-expected",
        "--backend",
        "hybrid",
        "--allow-backend-fallback",
    ]);

    (cpu, gpu_with_fallback, hybrid_with_fallback)
}

pub(super) fn gpu_no_backend_fallback_output() -> CliOutput {
    cli_output(&[
        "--format",
        "json",
        "pc",
        "--lines",
        "2",
        "--queue",
        "IIOOOIIOOO",
        "--fixed",
        "--no-hold",
        "--objective",
        "min-cover",
        "--backend",
        "gpu",
        "--no-backend-fallback",
    ])
}
