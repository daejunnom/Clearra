#![cfg(feature = "native-c-core")]

use crate::product_contract_json_assert;
use crate::{exit::ExitCode, output::CliOutput, run_with_args};
use serde_json::Value;
use std::path::PathBuf;

#[path = "product_backend_capability_assert.rs"]
mod product_backend_capability_assert;
use product_backend_capability_assert::{
    assert_gpu_unavailable_reason, assert_hybrid_unavailable_reason,
    assert_u0_backend_capability_report, backend_report_bool, backend_report_optional_string,
    backend_report_string,
};

#[path = "product_contract_e2e/support.rs"]
mod product_contract_e2e_support;
use product_contract_e2e_support::*;

mod case_library_route_product_e2e_opening_2l_empty_matches_golden {
    use super::*;

    #[test]
    fn library_route_product_e2e_opening_2l_empty_matches_golden() {
        let stdout = json_output(&[
            "--format",
            "json",
            "pc",
            "--lines",
            "2",
            "--queue",
            "IIOOO",
            "--fixed",
            "--no-hold",
        ]);
        assert_markers(
            "opening 2L",
            &output_marker_text(&stdout),
            include_str!("../../../tests/golden/pc/opening_2l_empty.json"),
        );
        product_contract_json_assert::assert_opening_2l_empty_json(
            &product_contract_json_assert::json_from_stdout(&stdout),
        );
    }
}

mod case_library_route_product_e2e_backend_fallback_parity_matches_cpu_product_contract {
    use super::*;

    #[test]
    fn library_route_product_e2e_backend_fallback_parity_matches_cpu_product_contract() {
        let (cpu, gpu_with_fallback, hybrid_with_fallback) = opening_2l_backend_values();
        assert_stage_d_backend_equivalence(&cpu, &gpu_with_fallback, &hybrid_with_fallback);
    }
}

mod case_product_backend_cpu_gpu_hybrid_same_opening_2l {
    use super::*;

    #[test]
    fn product_backend_cpu_gpu_hybrid_same_opening_2l() {
        let (cpu, gpu_with_fallback, hybrid_with_fallback) = opening_2l_backend_values();
        assert_stage_d_backend_equivalence(&cpu, &gpu_with_fallback, &hybrid_with_fallback);
    }
}

mod case_backend_report_present_in_json {
    use super::*;

    #[test]
    fn backend_report_present_in_json() {
        let (cpu, gpu_with_fallback, hybrid_with_fallback) = opening_2l_backend_values();
        assert_u0_backend_capability_report(&cpu, &gpu_with_fallback, &hybrid_with_fallback);
    }
}

mod case_gpu_unavailable_reports_reason {
    use super::*;

    #[test]
    fn gpu_unavailable_reports_reason() {
        let (_, gpu_with_fallback, _) = opening_2l_backend_values();

        let disabled_reason = backend_report_string(&gpu_with_fallback, "gpu_disabled_reason");
        let fallback_reason = backend_report_string(&gpu_with_fallback, "backend_fallback_reason");
        assert_gpu_unavailable_reason(disabled_reason);
        assert_gpu_unavailable_reason(fallback_reason);
        assert_eq!(fallback_reason, disabled_reason);
    }
}

mod case_hybrid_disabled_reports_reason {
    use super::*;

    #[test]
    fn hybrid_disabled_reports_reason() {
        let (_, _, hybrid_with_fallback) = opening_2l_backend_values();

        assert_eq!(
            backend_report_string(&hybrid_with_fallback, "hybrid_status"),
            "cpu-selected"
        );
        let gpu_disabled_reason =
            backend_report_string(&hybrid_with_fallback, "gpu_disabled_reason");
        let hybrid_disabled_reason =
            backend_report_string(&hybrid_with_fallback, "hybrid_disabled_reason");
        assert_hybrid_unavailable_reason(gpu_disabled_reason);
        assert_hybrid_unavailable_reason(hybrid_disabled_reason);
        assert_eq!(gpu_disabled_reason, hybrid_disabled_reason);
    }
}

mod case_fallback_used_reports_reason {
    use super::*;

    #[test]
    fn fallback_used_reports_reason() {
        let (_, gpu_with_fallback, hybrid_with_fallback) = opening_2l_backend_values();

        assert!(backend_report_bool(&gpu_with_fallback, "fallback_used"));
        assert_eq!(
            backend_report_string(&gpu_with_fallback, "fallback_backend"),
            "cpu"
        );
        assert_gpu_unavailable_reason(backend_report_string(
            &gpu_with_fallback,
            "backend_fallback_reason",
        ));

        assert!(!backend_report_bool(&hybrid_with_fallback, "fallback_used"));
        assert_eq!(
            backend_report_string(&hybrid_with_fallback, "fallback_backend"),
            "none"
        );
        assert_eq!(
            backend_report_optional_string(&hybrid_with_fallback, "backend_fallback_reason"),
            None
        );
    }
}

mod case_product_backend_cpu_gpu_hybrid_same_scenario_4l {
    use super::*;

    #[test]
    fn product_backend_cpu_gpu_hybrid_same_scenario_4l() {
        let (cpu, gpu_with_fallback, hybrid_with_fallback) = scenario_4l_backend_values();
        assert_stage_d_backend_equivalence(&cpu, &gpu_with_fallback, &hybrid_with_fallback);
    }
}

mod case_library_route_product_e2e_gpu_no_backend_fallback_reports_error_without_cpu_selection {
    use super::*;

    #[test]
    fn library_route_product_e2e_gpu_no_backend_fallback_reports_error_without_cpu_selection() {
        let output = gpu_no_backend_fallback_output();

        assert_eq!(output.exit_code(), ExitCode::Unsupported);
        assert!(output.stdout().is_empty(), "stdout={}", output.stdout());
        assert!(output.stderr().contains("E_BACKEND_GPU_UNAVAILABLE"));
        assert!(output.stderr().contains("gpu_kernel_unavailable"));
        assert!(!output.stderr().contains("backend_selected=cpu"));
    }
}

mod case_product_gpu_no_fallback_returns_error_when_unavailable {
    use super::*;

    #[test]
    fn product_gpu_no_fallback_returns_error_when_unavailable() {
        let output = gpu_no_backend_fallback_output();

        assert_eq!(output.exit_code(), ExitCode::Unsupported);
        assert!(output.stdout().is_empty(), "stdout={}", output.stdout());
        assert!(output.stderr().contains("E_BACKEND_GPU_UNAVAILABLE"));
        assert!(output.stderr().contains("gpu_kernel_unavailable"));
        assert!(!output.stderr().contains("backend_selected=cpu"));
    }
}

mod case_product_gpu_allow_fallback_reports_reason {
    use super::*;

    #[test]
    fn product_gpu_allow_fallback_reports_reason() {
        let (_, gpu_with_fallback, _) = opening_2l_backend_values();

        assert_eq!(
            product_contract_json_assert::string_field(&gpu_with_fallback, "backend_requested"),
            "gpu"
        );
        assert_eq!(
            product_contract_json_assert::string_field(&gpu_with_fallback, "backend_selected"),
            "cpu"
        );
        assert!(product_contract_json_assert::bool_field(
            &gpu_with_fallback,
            "backend_fallback_used"
        ));
        assert_eq!(
            product_contract_json_assert::string_field(
                &gpu_with_fallback,
                "backend_fallback_reason"
            ),
            "gpu_kernel_unavailable"
        );
    }
}

mod case_product_gpu_backend_report_includes_trust_state {
    use super::*;

    #[test]
    fn product_gpu_backend_report_includes_trust_state() {
        let (_, gpu_with_fallback, _) = opening_2l_backend_values();

        assert_eq!(
            product_contract_json_assert::string_field(&gpu_with_fallback, "gpu_trust_state"),
            "fallback-used"
        );
    }
}

mod case_library_route_product_e2e_scenario_simple_4l_matches_golden {
    use super::*;

    #[test]
    fn library_route_product_e2e_scenario_simple_4l_matches_golden() {
        let fixture = workspace_path("tests/fixtures/pc/scenario_simple_4l.json");
        let stdout = json_output(&[
            "--format",
            "json",
            "pc-scenario",
            "--fixture",
            &fixture,
            "--verify-expected",
        ]);
        assert_markers(
            "scenario simple 4L",
            &output_marker_text(&stdout),
            include_str!("../../../tests/golden/pc/scenario_simple_4l.json"),
        );
    }
}

mod case_library_route_product_e2e_unsupported_180_reports_capability_reason {
    use super::*;

    #[test]
    fn library_route_product_e2e_unsupported_180_reports_capability_reason() {
        let fixture = workspace_path("tests/fixtures/pc/requires_180_unsupported.json");
        let stdout = json_output(&[
            "--format",
            "json",
            "pc-scenario",
            "--fixture",
            &fixture,
            "--verify-expected",
        ]);
        assert_markers(
            "unsupported 180",
            &output_marker_text(&stdout),
            include_str!("../../../tests/golden/pc/requires_180_unsupported.json"),
        );
    }
}

mod case_library_route_product_e2e_continuation_matches_golden {
    use super::*;

    #[test]
    fn library_route_product_e2e_continuation_matches_golden() {
        let stdout = json_output(&[
            "--format",
            "json",
            "pc",
            "--lines",
            "2",
            "--queue",
            "IIOOOIIOOO",
            "--fixed",
            "--no-hold",
        ]);
        assert_markers(
            "continuation",
            &output_marker_text(&stdout),
            include_str!("../../../tests/golden/continuation/next_pc_available.json"),
        );
    }
}

mod case_library_route_product_e2e_coverage_overlap_fixture_matches_golden {
    use super::*;

    #[test]
    fn library_route_product_e2e_coverage_overlap_fixture_matches_golden() {
        let fixture =
            include_str!("../../../tests/fixtures/coverage/overlap_two_variants_one_pattern.json");
        assert_markers(
            "coverage overlap",
            &fixture_marker_text(fixture, &[]),
            include_str!("../../../tests/golden/coverage/overlap_union_probability.json"),
        );
        product_contract_json_assert::assert_coverage_overlap_json(
            &product_contract_json_assert::json_from_fixture(fixture),
        );
    }
}

mod case_library_route_product_e2e_setup_family_probability_matches_golden {
    use super::*;

    #[test]
    fn library_route_product_e2e_setup_family_probability_matches_golden() {
        let fixture = include_str!("../../../tests/fixtures/setup/simple_family_union.json");
        assert_markers(
            "setup family probability",
            &fixture_marker_text(fixture, &["PatternBitSet OR union"]),
            include_str!("../../../tests/golden/setup/simple_family_probability.json"),
        );
        product_contract_json_assert::assert_setup_family_json(
            &product_contract_json_assert::json_from_fixture(fixture),
        );
    }
}

mod case_library_route_max_score_materializes_profile_specific_nonzero_matrix {
    use super::*;

    #[test]
    fn library_route_max_score_materializes_profile_specific_nonzero_matrix() {
        let json = json_value(&[
            "--format",
            "json",
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
        ]);

        assert_eq!(
            product_contract_json_assert::string_field(&json, "route"),
            "search-problem-core-executor"
        );
        assert_eq!(
            product_contract_json_assert::string_field(&json, "score_matrix_profile_id"),
            "jstris-ultra-pc-t-spins"
        );
        assert!(product_contract_json_assert::bool_field(
            &json,
            "score_matrix_materialized"
        ));
        assert!(product_contract_json_assert::number_field(&json, "score_matrix_cell_count") > 0.0);
        assert!(product_contract_json_assert::number_field(&json, "score_best_score") > 0.0);
    }
}

mod case_pc_command_uses_search_problem_core_executor {
    use super::*;

    #[test]
    fn pc_command_uses_search_problem_core_executor() {
        let stdout = product_fixture_stdout("tests/fixtures/product/pc_2l_fixed_queue.json");
        let json = product_contract_json_assert::json_from_stdout(&stdout);

        assert_markers(
            "MVP1 pc command",
            &output_marker_text(&stdout),
            include_str!("../../../tests/golden/product/pc_2l_fixed_queue.json"),
        );
        assert_eq!(
            product_contract_json_assert::string_field(&json, "route"),
            "search-problem-core-executor"
        );
        assert_eq!(
            product_contract_json_assert::string_field(&json, "packing_backend"),
            "cpu"
        );
        assert_eq!(
            product_contract_json_assert::string_field(&json, "buildup_backend_owner"),
            "cpu"
        );
        assert_eq!(
            product_contract_json_assert::string_field(&json, "coverage_reducer"),
            "pattern-bitset-union"
        );
        assert!(!product_contract_json_assert::bool_field(
            &json,
            "packing_candidate_is_solution"
        ));
    }
}

mod case_setup_command_assembles_exact_residue_request {
    use clearra_setup_search::query::SetupCycleResetBorrowPolicy;

    #[test]
    fn setup_command_assembles_exact_residue_request() {
        let query = crate::assemble::SetupQueryAssembler::assemble(&crate::args::SetupArgs::new(
            "I,T,O", true,
        ))
        .expect("setup residue request");

        assert_eq!(query.residue().remaining_count(), 3);
        assert_eq!(query.residue().cycle(), Some(7));
        assert_eq!(
            query.cycle_reset_borrow_policy(),
            SetupCycleResetBorrowPolicy::AllowPostCyclePieceUse
        );
    }
}

mod case_continue_token_roundtrip_compiles_to_search_problem {
    use super::*;

    #[test]
    fn continue_token_roundtrip_compiles_to_search_problem() {
        let stdout = continue_fixture_stdout("tests/fixtures/product/continue_token_basic.json");
        let json = product_contract_json_assert::json_from_stdout(&stdout);

        assert_markers(
            "MVP1 continue command",
            &output_marker_text(&stdout),
            include_str!("../../../tests/golden/product/continue_token_basic.json"),
        );
        assert_eq!(
            product_contract_json_assert::string_field(&json, "continuation_kind"),
            "opening"
        );
        assert_eq!(
            product_contract_json_assert::string_field(&json, "route"),
            "search-problem-core-executor"
        );
        assert_eq!(
            product_contract_json_assert::string_field(&json, "compiled_goal"),
            "clear-to-empty"
        );
    }
}

mod case_rules_command_uses_app_response_route {
    use super::*;

    #[test]
    fn rules_command_uses_app_response_route() {
        let stdout = product_fixture_stdout("tests/fixtures/product/rules_list.json");
        let json = product_contract_json_assert::json_from_stdout(&stdout);

        assert_markers(
            "MVP1 rules command",
            &output_marker_text(&stdout),
            include_str!("../../../tests/golden/product/rules_list.json"),
        );
        assert_eq!(
            product_contract_json_assert::string_field(&json, "kind"),
            "rules"
        );
        assert_eq!(
            product_contract_json_assert::string_field(&json, "action"),
            "list"
        );
    }
}

mod case_scoring_command_uses_app_response_route {
    use super::*;

    #[test]
    fn scoring_command_uses_app_response_route() {
        let stdout = product_fixture_stdout("tests/fixtures/product/scoring_list.json");
        let json = product_contract_json_assert::json_from_stdout(&stdout);

        assert_markers(
            "MVP1 scoring command",
            &output_marker_text(&stdout),
            include_str!("../../../tests/golden/product/scoring_list.json"),
        );
        assert_eq!(
            product_contract_json_assert::string_field(&json, "kind"),
            "scoring"
        );
        assert_eq!(
            product_contract_json_assert::string_field(&json, "action"),
            "list"
        );
    }
}

mod case_convert_command_uses_app_response_route {
    use super::*;

    #[test]
    fn convert_command_uses_app_response_route() {
        let stdout = product_fixture_stdout("tests/fixtures/product/convert_fumen_like_json.json");
        let json = product_contract_json_assert::json_from_stdout(&stdout);

        assert_markers(
            "MVP1 convert command",
            &output_marker_text(&stdout),
            include_str!("../../../tests/golden/product/convert_fumen_like_json.json"),
        );
        assert_eq!(
            product_contract_json_assert::string_field(&json, "kind"),
            "convert"
        );
        assert_eq!(
            product_contract_json_assert::string_field(&json, "from"),
            "fumen-like"
        );
        assert_eq!(
            product_contract_json_assert::number_field(&json, "page_count"),
            1.0
        );
    }
}

mod case_verify_command_uses_app_response_route {
    use super::*;

    #[test]
    fn verify_command_uses_app_response_route() {
        let fixture = read_workspace_json("tests/fixtures/product/verify_all.json");
        let args = fixture_command(&fixture, "command");
        let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
        let output = cli_output(&refs);

        assert_markers(
            "MVP1 verify command",
            &output_marker_text(output.stdout()),
            include_str!("../../../tests/golden/product/verify_all.json"),
        );
        assert_eq!(output.exit_code(), ExitCode::Success, "{}", output.stderr());
        assert!(output.stderr().is_empty());
    }
}

mod case_verify_kicks_command_uses_app_response_route {
    use super::*;

    #[test]
    fn verify_kicks_command_uses_app_response_route() {
        let stdout = product_fixture_stdout("tests/fixtures/product/verify_kicks.json");
        let json = product_contract_json_assert::json_from_stdout(&stdout);

        assert_markers(
            "MVP1 verify kicks command",
            &output_marker_text(&stdout),
            include_str!("../../../tests/golden/product/verify_kicks.json"),
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
}
