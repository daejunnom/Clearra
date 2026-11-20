use crate::product_contract_json_assert::{
    assert_exact_pc, fixture_json, fixture_json_with_flags, number,
};

#[test]
fn pc_4l_fixed_candidate_budget_golden_contract_is_stable() {
    let result = fixture_json_with_flags(
        "tests/fixtures/product/pc_4l_fixed_candidate_budget.json",
        &["--backend", "cpu", "--workers", "1"],
    );
    assert_eq!(result["kind"], "pc");
    let summary = &result["summary"];
    assert_eq!(summary["actual_backend"], "wasm-cpu");
    assert_eq!(summary["count_complete"], false);
    assert_eq!(summary["probability_complete"], false);
    assert_eq!(
        summary["resource_truncation_reason"],
        "candidate_budget_exceeded"
    );
}

#[test]
fn scenario_clear_to_empty_golden_contract_is_stable() {
    let result = fixture_json("tests/fixtures/product/scenario_clear_to_empty.json");
    assert_exact_pc(&result, "pc-scenario", 1);
    assert_eq!(result["summary"]["completion_goal"], "clear-to-empty");
    assert_eq!(result["summary"]["scenario_replay_token_version"], "sr2");
    assert_eq!(result["summary"]["retained_trace_count"], 1);
}

#[test]
fn percent_uniform_bag_golden_contract_is_stable() {
    let result = fixture_json("tests/fixtures/product/percent_uniform_bag.json");
    assert_eq!(result["kind"], "percent");
    let summary = &result["summary"];
    assert_eq!(summary["actual_backend"], "wasm-cpu");
    assert_eq!(number(summary, "total_pattern_count"), 1.0);
    assert_eq!(number(summary, "covered_pattern_count"), 1.0);
    assert_eq!(number(summary, "coverage_probability"), 1.0);
    assert_eq!(summary["probability_complete"], true);
    assert_eq!(summary["renormalized"], false);
}

#[test]
fn rules_verify_basic_golden_contract_is_stable() {
    let result = fixture_json("tests/fixtures/product/rules_verify_basic.json");
    assert_eq!(result["kind"], "verify-kicks");
    assert_eq!(result["summary"]["status"], "verified");
    assert_eq!(
        number(&result["summary"], "kick_verification_failures"),
        0.0
    );
}
