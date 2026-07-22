use super::*;

#[test]
fn process_e2e_pc_scenario_fixture_json_counts_solutions() {
    let output = clearra()
        .args([
            "pc-scenario",
            "--fixture",
            "tests/fixtures/pc/example.json",
            "--verify-expected",
            "--format",
            "json",
        ])
        .output()
        .expect("clearra-cli process runs");

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8(output.stdout).expect("stdout utf8");
    for marker in [
        "\"kind\":\"pc-scenario\"",
        "\"expected_checked\":true",
        "\"expected_match\":true",
        "\"expected_total_solution_count\":\"none\"",
        "\"expected_retained_trace_key_count\":1",
        "\"retained_trace_keys_checked\":true",
        "\"retained_trace_keys_match\":true",
        "\"exact_pieces\":1",
        "\"min_remaining_queue\":0",
        "\"allow_hold\":true",
        "\"count_policy\":\"count-all\"",
        "\"retained_trace_limit\":1",
        "\"solution_found\":true",
        "\"retained_trace_count\":1",
        "\"retained_trace_key_count\":1",
        "\"scenario_replay_token_version\":\"sr2\"",
        "\"scenario_replay_token\":\"sr2:",
        "\"replay_hint\":\"clearra continue sr2:",
    ] {
        assert!(stdout.contains(marker), "missing marker {marker}");
    }
    assert!(stdout.contains(&format!(
        "\"total_solution_count\":{}",
        expected_scenario_solution_count()
    )));
    assert!(stdout.contains(if native_core_enabled() {
        "\"count_complete\":true"
    } else {
        "\"count_complete\":false"
    }));
    assert!(stdout.contains(expected_retained_trace_key_prefix()));
    assert!(stdout.contains("\"continuation_token_version\":\"none\""));
    assert!(stdout.contains("\"continuation_token\":\"none\""));
    assert!(stdout.contains("\"continue_hint\":\"none\""));
}

#[test]
fn process_e2e_pc_scenario_simple_4l_fixture_counts_solutions() {
    let output = clearra()
        .args([
            "pc-scenario",
            "--fixture",
            "tests/fixtures/pc/scenario_simple_4l.json",
            "--verify-expected",
            "--format",
            "json",
        ])
        .output()
        .expect("clearra-cli process runs");

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8(output.stdout).expect("stdout utf8");
    for marker in [
        "\"kind\":\"pc-scenario\"",
        "\"expected_checked\":true",
        "\"expected_match\":true",
        "\"visible_height\":\"4\"",
        "\"initial_board_mask\":\"0x00000000000003f0\"",
        "\"exact_pieces\":1",
        "\"solution_found\":true",
        expected_scenario_coverage_probability_json(),
        "\"scenario_replay_token_version\":\"sr2\"",
        "\"scenario_replay_token\":\"sr2:w10:v4:",
    ] {
        assert!(stdout.contains(marker), "missing marker {marker}");
    }
    assert!(stdout.contains(&format!(
        "\"total_solution_count\":{}",
        expected_scenario_solution_count()
    )));
}

#[test]
fn process_e2e_pc_scenario_expected_unsupported_fixture_succeeds() {
    let output = clearra()
        .args([
            "pc-scenario",
            "--fixture",
            "tests/fixtures/pc/requires_180_unsupported.json",
            "--verify-expected",
            "--format",
            "json",
        ])
        .output()
        .expect("clearra-cli process runs");

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8(output.stdout).expect("stdout utf8");
    for marker in [
        "\"kind\":\"pc-scenario\"",
        "\"status\":\"scenario-unsupported-expected\"",
        "\"expected_match\":true",
        "\"expected_unsupported\":true",
        "\"actual_unsupported\":true",
        "\"unsupported_stage\":\"validation\"",
        "\"actual_unsupported_reason\":\"scenario_requires_180_unsupported\"",
    ] {
        assert!(stdout.contains(marker), "missing marker {marker}");
    }
}

#[test]
fn process_e2e_pc_scenario_inline_json_counts_solutions() {
    let output = clearra()
        .args([
            "pc-scenario",
            "--field",
            "0x00000000000003f0",
            "--queue",
            "I",
            "--max-pieces",
            "1",
            "--exact-pieces",
            "1",
            "--format",
            "json",
        ])
        .output()
        .expect("clearra-cli process runs");

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8(output.stdout).expect("stdout utf8");
    assert!(stdout.contains("\"kind\":\"pc-scenario\""));
    assert!(stdout.contains("\"input_mode\":\"inline\""));
    assert!(stdout.contains("\"solution_found\":true"));
}
