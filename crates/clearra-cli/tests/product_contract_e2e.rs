use clearra_coverage::row::coverage_row_kind::CoverageRowKind;

use crate::{
    product_contract_json_assert::{
        assert_exact_pc, bool_value, fixture, fixture_args, fixture_json, fixture_json_with_flags,
        number, run_json, workspace_path,
    },
    run_with_args,
};

#[path = "product_backend_capability_assert.rs"]
mod product_backend_capability_assert;
use product_backend_capability_assert::assert_cpu_route;

#[path = "product_contract_e2e/support.rs"]
mod product_contract_e2e_support;
use product_contract_e2e_support::assert_fixture_probability;

fn opening_2l(backend: &str, fallback: Option<&str>) -> serde_json::Value {
    let mut args = vec![
        "pc",
        "--lines",
        "2",
        "--queue",
        "IIOOOIIOOO",
        "--fixed",
        "--no-hold",
        "--backend",
        backend,
        "--workers",
        "1",
    ];
    args.extend(fallback);
    run_json(args)
}

#[test]
fn library_route_product_e2e_opening_2l_empty_matches_golden() {
    let result = opening_2l("cpu", None);
    // Two horizontal Is share one aligned two-column strip, leaving three
    // O strips. The four possible placements are fields, not trace variants.
    assert_exact_pc(&result, "pc", 4);
    assert!(!bool_value(
        &result["contract"]["pc"]["execution_report"]["packing"],
        "packing_candidate_is_solution",
    ));
}

#[test]
fn backend_report_present_in_json() {
    assert_cpu_route(&opening_2l("cpu", None));
}

#[test]
fn product_gpu_and_hybrid_requests_are_retired_without_cpu_fallback() {
    use crate::{error::CliErrorCode, exit::ExitCode};
    let _guard = crate::execution_resource_test_support::execution_resource_test_guard();
    for backend in ["gpu", "hybrid"] {
        for fallback in ["--allow-backend-fallback", "--no-backend-fallback"] {
            let output = run_with_args([
                "clearra",
                "--format",
                "json",
                "pc",
                "--lines",
                "2",
                "--queue",
                "IIOOO",
                "--fixed",
                "--no-hold",
                "--backend",
                backend,
                fallback,
                "--workers",
                "1",
            ]);
            assert_eq!(
                output.exit_code(),
                ExitCode::Unsupported,
                "{}",
                output.stderr()
            );
            assert!(output.stdout().is_empty(), "{}", output.stdout());
            assert!(
                output
                    .stderr()
                    .contains(CliErrorCode::BackendGpuUnavailable.as_str()),
                "{}",
                output.stderr()
            );
            assert!(
                output.stderr().contains("webgpu_backend_retired"),
                "{}",
                output.stderr()
            );
        }
    }
}

fn scenario_4l(backend: &str, fallback: Option<&str>) -> serde_json::Value {
    let path = workspace_path("tests/fixtures/pc/scenario_simple_4l.json");
    let mut args = vec![
        "pc-scenario",
        "--fixture",
        path.to_str().expect("UTF-8 fixture path"),
        "--verify-expected",
        "--backend",
        backend,
        "--workers",
        "1",
    ];
    args.extend(fallback);
    run_json(args)
}

#[test]
fn library_route_product_e2e_scenario_simple_4l_matches_golden() {
    let result = scenario_4l("cpu", None);
    assert_exact_pc(&result, "pc-scenario", 1);
    assert_eq!(result["summary"]["expected_match"], true);
    assert_eq!(result["summary"]["scenario_replay_token_version"], "sr2");
    assert_eq!(result["summary"]["retained_trace_count"], 1);
}

#[test]
fn library_route_product_e2e_unsupported_180_reports_capability_reason() {
    let path = workspace_path("tests/fixtures/pc/requires_180_unsupported.json");
    let result = run_json([
        "pc-scenario",
        "--fixture",
        path.to_str().expect("UTF-8 fixture path"),
        "--verify-expected",
    ]);
    assert_eq!(result["kind"], "pc-scenario");
    let summary = &result["summary"];
    assert_eq!(summary["expected_match"], true);
    assert_eq!(summary["actual_unsupported"], true);
    assert_eq!(summary["unsupported_stage"], "validation");
    assert_eq!(
        summary["actual_unsupported_reason"],
        "scenario_requires_180_unsupported"
    );
}

#[test]
fn library_route_product_e2e_continuation_matches_golden() {
    let seed = opening_2l("cpu", None);
    assert_exact_pc(&seed, "pc", 4);
    let summary = &seed["summary"];
    assert_eq!(summary["next_pc_available"], true);
    assert_eq!(summary["continuation_token_version"], "pc2");
    assert!(summary["continuation_token"]
        .as_str()
        .is_some_and(|token| token.starts_with("pc2:")));
}

#[test]
fn library_route_product_e2e_coverage_overlap_fixture_matches_golden() {
    assert_fixture_probability(
        &fixture("tests/fixtures/coverage/overlap_two_variants_one_pattern.json"),
        CoverageRowKind::Build,
        "candidate_id",
    );
}

#[test]
fn library_route_product_e2e_setup_family_probability_matches_golden() {
    assert_fixture_probability(
        &fixture("tests/fixtures/setup/simple_family_union.json"),
        CoverageRowKind::Setup,
        "build_variant_id",
    );
}

#[test]
fn library_route_max_score_materializes_profile_specific_nonzero_matrix() {
    let result = run_json([
        "pc",
        "--lines",
        "2",
        "--queue",
        "IIOOO",
        "--fixed",
        "--no-hold",
        "--objective",
        "all",
        "--score",
        "--score-profile",
        "jstris-ultra",
        "--backend",
        "cpu",
        "--workers",
        "1",
    ]);
    assert_exact_pc(&result, "pc", 4);
    let summary = &result["summary"];
    assert_eq!(
        summary["score_matrix_profile_id"],
        "jstris-ultra-pc-t-spins"
    );
    assert_eq!(summary["score_matrix_materialized"], true);
    assert_eq!(summary["score_matrix_complete"], true);
    assert!(number(summary, "score_matrix_cell_count") > 0.0);
    assert!(number(summary, "score_best_score") > 0.0);
}

#[test]
fn pc_command_uses_search_problem_core_executor() {
    let result = fixture_json_with_flags(
        "tests/fixtures/product/pc_2l_fixed_queue.json",
        &["--backend", "cpu", "--workers", "1"],
    );
    assert_exact_pc(&result, "pc", 4);
    assert!(!bool_value(
        &result["contract"]["pc"]["execution_report"]["packing"],
        "packing_candidate_is_solution",
    ));
}

#[test]
fn setup_command_assembles_exact_residue_request() {
    use clearra_setup_search::query::SetupCycleResetBorrowPolicy;

    let query =
        crate::assemble::SetupQueryAssembler::assemble(&crate::args::SetupArgs::new("I,T,O", true))
            .expect("setup residue request");

    assert_eq!(query.residue().remaining_count(), 3);
    assert_eq!(query.residue().cycle(), Some(7));
    assert_eq!(
        query.cycle_reset_borrow_policy(),
        SetupCycleResetBorrowPolicy::AllowPostCyclePieceUse
    );
}

#[test]
fn continue_token_roundtrip_compiles_to_search_problem() {
    let example = fixture("tests/fixtures/product/continue_token_basic.json");
    let seed = run_json(fixture_args(&example, "seed_command"));
    let token = seed["summary"]["continuation_token"]
        .as_str()
        .expect("opening continuation token");
    let continued = run_json(["continue", token]);
    let direct = run_json([
        "pc",
        "--lines",
        "2",
        "--queue",
        "IIOOO",
        "--fixed",
        "--no-hold",
        "--backend",
        "cpu",
        "--workers",
        "1",
    ]);
    assert_exact_pc(&direct, "pc", 4);
    assert_eq!(continued["kind"], "continue");
    assert_eq!(continued["summary"]["continuation_kind"], "opening");
    assert_eq!(
        continued["summary"]["normalized_solution_set_hash"],
        direct["summary"]["normalized_solution_set_hash"]
    );
}

#[test]
fn rules_command_uses_app_response_route() {
    let result = fixture_json("tests/fixtures/product/rules_list.json");
    assert_eq!(result["kind"], "rules");
    assert_eq!(result["summary"]["action"], "list");
}

#[test]
fn scoring_command_uses_app_response_route() {
    let result = fixture_json("tests/fixtures/product/scoring_list.json");
    assert_eq!(result["kind"], "scoring");
    assert_eq!(result["summary"]["action"], "list");
}

#[test]
fn convert_command_uses_app_response_route() {
    let result = fixture_json("tests/fixtures/product/convert_fumen_like_json.json");
    assert_eq!(result["kind"], "convert");
    assert_eq!(result["summary"]["from"], "fumen-like");
    assert_eq!(number(&result["summary"], "page_count"), 1.0);
}

#[test]
fn verify_command_uses_app_response_route() {
    // Run through the public CLI rather than using a fixture marker as proof.
    let _guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let result = crate::product_contract_json_assert::json_value(&run_with_args([
        "clearra", "--format", "json", "verify",
    ]));
    assert_eq!(result["kind"], "verify");
    for probe in ["pc", "setup", "build_coverage", "kicks"] {
        assert_eq!(result["summary"][probe], "ok", "{probe}");
    }
    assert_eq!(
        number(&result["summary"], "kick_verification_failures"),
        0.0
    );
}

#[test]
fn verify_kicks_command_uses_app_response_route() {
    let result = fixture_json("tests/fixtures/product/verify_kicks.json");
    assert_eq!(result["kind"], "verify-kicks");
    assert_eq!(result["summary"]["status"], "verified");
    assert_eq!(
        number(&result["summary"], "kick_verification_failures"),
        0.0
    );
}
