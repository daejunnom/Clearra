use super::*;
use clearra_pc_graph::request::{PcContinuationToken, PcContinuationTokenCodec};

fn verified_scenario_fixture() -> Value {
    run_json(&[
        "pc-scenario",
        "--fixture",
        "tests/fixtures/pc/scenario_simple_4l.json",
        "--verify-expected",
        "--backend",
        "cpu",
        "--workers",
        "1",
    ])
}

fn assert_exact_single_tiling(summary: &Value) {
    assert_rust_cpu(summary);
    assert_eq!(summary["solution_found"], true);
    assert_eq!(summary["count_complete"], true);
    assert_eq!(summary["normalized_unique_solution_count"], 1);
}

#[test]
fn process_e2e_pc_scenario_fixture_json_counts_solutions() {
    let result = verified_scenario_fixture();
    assert_eq!(result["kind"], "pc-scenario");
    let summary = &result["summary"];
    assert_exact_single_tiling(summary);
    assert_eq!(summary["expected_checked"], true);
    assert_eq!(summary["expected_match"], true);
    assert_eq!(summary["retained_trace_count"], 1);
    assert_eq!(summary["scenario_replay_token_version"], "sr2");
    let token = summary["scenario_replay_token"]
        .as_str()
        .filter(|token| token.starts_with("sr2:w10:v4:"))
        .expect("the verified scenario exposes its original-height input replay");
    let hash = summary["normalized_solution_set_hash"]
        .as_str()
        .filter(|hash| hash.starts_with("cts1:"))
        .expect("complete canonical solution family");
    let replay = run_json(&["continue", token]);
    assert_eq!(replay["kind"], "continue");
    assert_eq!(replay["summary"]["status"], "scenario-replayed-searched");
    assert_eq!(replay["summary"]["interactive_prompt"], false);
    assert_eq!(replay["summary"]["normalized_solution_set_hash"], hash);
}

#[test]
fn process_e2e_pc_scenario_simple_4l_fixture_counts_solutions() {
    let result = verified_scenario_fixture();
    assert_eq!(result["kind"], "pc-scenario");
    let summary = &result["summary"];
    assert_exact_single_tiling(summary);
    assert_eq!(summary["visible_height"], "4");
    assert_eq!(summary["initial_board_mask"], "0x00000000000003f0");
    let token = summary["scenario_replay_token"]
        .as_str()
        .expect("original-input replay token");
    let PcContinuationToken::ScenarioReplay(query) =
        PcContinuationTokenCodec::parse(token).expect("decode scenario replay")
    else {
        panic!("expected a scenario replay token, not a checkpoint");
    };
    assert_eq!(query.exact_pieces(), Some(1));
    assert_eq!(summary["trace_steps"], 1);
    assert_eq!(summary["probability_complete"], true);
    assert_eq!(summary["coverage_probability"].as_f64(), Some(1.0));
}

#[test]
fn process_e2e_pc_scenario_expected_unsupported_fixture_succeeds() {
    let result = run_json(&[
        "pc-scenario",
        "--fixture",
        "tests/fixtures/pc/requires_180_unsupported.json",
        "--verify-expected",
    ]);
    assert_eq!(result["kind"], "pc-scenario");
    let summary = &result["summary"];
    assert_eq!(summary["status"], "scenario-unsupported-expected");
    assert_eq!(summary["expected_match"], true);
    assert_eq!(summary["expected_unsupported"], true);
    assert_eq!(summary["actual_unsupported"], true);
    assert_eq!(summary["unsupported_stage"], "validation");
    assert_eq!(
        summary["actual_unsupported_reason"],
        "scenario_requires_180_unsupported"
    );
}

#[test]
fn process_e2e_pc_scenario_inline_json_counts_solutions() {
    let result = run_json(&[
        "pc-scenario",
        "--field",
        "0x00000000000003f0",
        "--queue",
        "I",
        "--max-pieces",
        "1",
        "--exact-pieces",
        "1",
        "--backend",
        "cpu",
        "--workers",
        "1",
    ]);
    assert_eq!(result["kind"], "pc-scenario");
    assert_eq!(result["summary"]["input_mode"], "inline");
    assert_exact_single_tiling(&result["summary"]);
}
