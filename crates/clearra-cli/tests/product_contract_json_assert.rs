use serde_json::Value;

pub fn json_from_stdout(stdout: &str) -> Value {
    serde_json::from_str(stdout.trim()).expect("product command stdout must be JSON")
}

pub fn json_from_fixture(fixture: &str) -> Value {
    serde_json::from_str(fixture).expect("product fixture must be JSON")
}

pub fn assert_opening_2l_empty_json(json: &Value) {
    assert_string_field(json, "kind", "pc");
    assert_string_field(json, "problem_preset", "opening-pc");
    assert_string_field(json, "compiled_goal", "clear-to-empty");
    assert_number_field(json, "compiled_piece_window", 5.0);
    assert_bool_field(json, "packing_candidate_is_solution", false);
    let expected_probability = if cfg!(feature = "native-c-core") {
        1.0
    } else {
        0.0
    };
    assert_number_field(json, "coverage_probability", expected_probability);
}

pub fn assert_coverage_overlap_json(json: &Value) {
    assert_number_field(json, "pattern_universe_id", 1001.0);
    assert_number_field(json, "pattern_weight_model_id", 2001.0);
    assert_string_field(json, "row_kind", "Build");
    assert_number_field(json, "covered_pattern_count", 1.0);
    assert_number_field(json, "probability", 0.4);
    assert_forbidden_algorithm(json, "variant_probability_sum");
}

pub fn assert_setup_family_json(json: &Value) {
    assert_number_field(json, "shape_family_id", 1.0);
    assert_number_field(json, "covered_pattern_count", 3.0);
    assert_number_field(json, "probability", 0.75);
    assert_bool_field(json, "renormalized", false);
    assert_forbidden_algorithm(json, "variant_probability_sum");
}

fn assert_string_field(json: &Value, field_name: &str, expected: &str) {
    let actual = string_field(json, field_name);
    assert_eq!(actual, expected, "field {field_name}");
}

fn assert_bool_field(json: &Value, field_name: &str, expected: bool) {
    let actual = bool_field(json, field_name);
    assert_eq!(actual, expected, "field {field_name}");
}

fn assert_number_field(json: &Value, field_name: &str, expected: f64) {
    let actual = number_field(json, field_name);
    assert!(
        (actual - expected).abs() < f64::EPSILON,
        "field {field_name}: expected {expected}, got {actual}"
    );
}

fn assert_forbidden_algorithm(json: &Value, expected: &str) {
    let actual = json
        .pointer("/expected/forbidden_algorithm")
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("expected forbidden_algorithm {expected}"));
    assert_eq!(actual, expected);
}

pub fn string_field<'a>(json: &'a Value, field_name: &str) -> &'a str {
    find_field(json, field_name)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("expected string field {field_name}"))
}

pub fn bool_field(json: &Value, field_name: &str) -> bool {
    find_field(json, field_name)
        .and_then(value_as_bool)
        .unwrap_or_else(|| panic!("expected bool field {field_name}"))
}

pub fn number_field(json: &Value, field_name: &str) -> f64 {
    find_field(json, field_name)
        .and_then(value_as_f64)
        .unwrap_or_else(|| panic!("expected numeric field {field_name}"))
}

pub fn find_field<'a>(value: &'a Value, field_name: &str) -> Option<&'a Value> {
    match value {
        Value::Object(object) => object.get(field_name).or_else(|| {
            object
                .values()
                .find_map(|child| find_field(child, field_name))
        }),
        Value::Array(items) => items.iter().find_map(|child| find_field(child, field_name)),
        _ => None,
    }
}

fn value_as_f64(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str().and_then(|text| text.parse::<f64>().ok()))
}

fn value_as_bool(value: &Value) -> Option<bool> {
    value.as_bool().or_else(|| match value.as_str()? {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    })
}
