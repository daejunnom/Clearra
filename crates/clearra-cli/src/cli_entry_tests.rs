use std::path::PathBuf;

use crate::{error::CliErrorCode, exit::ExitCode};

use clearra_output::fumen_like::{FumenLikeReader, FumenLikeTrace, FumenLikeWriter};

use super::*;

fn fixture_path(relative_path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("workspace root")
        .join(relative_path)
}

fn unique_suffix() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos()
}

fn expected_solver_backend() -> &'static str {
    "core-c-cpu-packing-cpu-buildup"
}

mod case_run_with_args_routes_pc_command_through_validation {
    use super::*;

    #[test]
    fn run_with_args_routes_pc_command_through_validation() {
        let output = run_with_args([
            "clearra",
            "pc",
            "--lines",
            "2",
            "--queue",
            "IJLOO",
            "--fixed",
            "--no-hold",
        ]);

        assert_eq!(output.exit_code(), ExitCode::Success);
        assert!(output.stdout().contains("lines: 2"));
        assert!(output.stdout().contains("queue_len: 5"));
        assert!(!output.stdout().contains("executor_flow"));
        assert!(!output.stdout().contains("compact_problem_descriptor"));
        assert!(!output.stdout().contains("gpu_backend_scope"));
        assert!(!output.stdout().contains("hybrid_scheduler"));
        assert!(!output.stdout().contains("score_event_basis"));
        assert!(!output.stdout().contains("coverage_row_view"));
        assert!(output.stdout().lines().count() <= 25);
    }
}

mod case_ux_smoke_rejects_internal_fields_in_default_text {
    use super::*;

    #[test]
    fn ux_smoke_rejects_internal_fields_in_default_text() {
        let output = run_with_args([
            "clearra",
            "pc",
            "--lines",
            "2",
            "--queue",
            "IJLOO",
            "--fixed",
            "--no-hold",
        ]);

        assert_eq!(output.exit_code(), ExitCode::Success);
        for internal_field in [
            "executor_flow",
            "compact_problem_descriptor",
            "gpu_backend_scope",
            "hybrid_scheduler",
            "score_event_basis",
            "coverage_row_view",
            "backend_report",
            "raw_coverage_export_path",
        ] {
            assert!(
                !output.stdout().contains(internal_field),
                "default text output leaked {internal_field}"
            );
        }
    }
}

mod case_run_with_args_selects_json_output_format {
    use super::*;

    #[test]
    fn run_with_args_selects_json_output_format() {
        let output = run_with_args([
            "clearra",
            "--format",
            "json",
            "pc",
            "--lines",
            "2",
            "--queue",
            "IJLOO",
            "--fixed",
            "--no-hold",
        ]);

        assert_eq!(output.exit_code(), ExitCode::Success);
        assert!(output.stdout().contains("\"schema_version\":2"));
        assert!(output.stdout().contains("\"kind\":\"pc\""));
        assert!(output.stdout().contains("\"summary\":{"));
        assert!(output.stdout().contains("\"contract\":{\"command\""));
        assert!(output.stdout().contains("\"lines\":2"));
        assert!(output.stdout().contains("\"queue_mode\":\"fixed\""));
        assert!(output.stdout().contains("\"executor_flow\""));
    }
}

mod case_failed_queue_json_quotes_not_calculated_solution_metadata {
    use super::*;

    fn run_json(args: &[&str]) -> serde_json::Value {
        let output = run_with_args(args.iter().copied());
        assert_eq!(output.exit_code(), ExitCode::Success, "{args:?}");
        serde_json::from_str(output.stdout())
            .unwrap_or_else(|error| panic!("{args:?} did not produce valid JSON: {error}"))
    }

    #[test]
    fn failed_queue_json_quotes_not_calculated_solution_metadata() {
        let value = run_json(&[
            "clearra",
            "--format",
            "json",
            "failed-queue",
            "--lines",
            "2",
            "--patterns",
            "P5",
            "--backend",
            "cpu",
            "--failed-count",
            "7",
        ]);

        assert_eq!(value["summary"]["unique_solution_count"], "not-calculated");
        assert_eq!(
            value["summary"]["normalized_unique_solution_count"],
            "not-calculated"
        );
    }

    #[test]
    fn percent_json_quotes_not_calculated_solution_count() {
        let value = run_json(&[
            "clearra",
            "--format",
            "json",
            "percent",
            "--queue",
            "IOT",
            "--observed",
            "--min-len",
            "5",
            "--max-patterns",
            "16",
        ]);

        assert_eq!(value["summary"]["unique_solution_count"], "not-calculated");
    }

    #[test]
    fn pc_tiling_json_quotes_not_calculated_probability_for_both_entry_points() {
        for objective in ["--tiling-only", "--objective"] {
            let mut args = vec![
                "clearra",
                "--format",
                "json",
                "pc",
                "--lines",
                "2",
                "--queue",
                "IJLOO",
                "--fixed",
                "--backend",
                "cpu",
                "--workers",
                "1",
                "--no-hold",
                objective,
            ];
            if objective == "--objective" {
                args.push("tiling");
            }
            let value = run_json(&args);

            assert_eq!(
                value["summary"]["coverage_probability"], "not-calculated",
                "{objective}"
            );
            assert_eq!(
                value["summary"]["probability_calculated"], false,
                "{objective}"
            );
            assert_eq!(
                value["summary"]["probability_complete"], false,
                "{objective}"
            );
            assert_eq!(
                value["summary"]["supply_probability_complete"], false,
                "{objective}"
            );
            assert_eq!(
                value["summary"]["resource_probability_complete"], false,
                "{objective}"
            );
            assert_eq!(value["resource_report"]["truncated"], false, "{objective}");
            assert_eq!(
                value["resource_report"]["truncation_reason"],
                serde_json::Value::Null,
                "{objective}"
            );
        }
    }

    #[cfg(feature = "wasm-cpu-runtime")]
    #[test]
    fn build_probability_tiling_json_quotes_not_calculated_probability() {
        let value = run_json(&[
            "clearra",
            "--format",
            "json",
            "build-probability",
            "--base-mask",
            "0x0",
            "--target-mask",
            "0xf",
            "--height",
            "4",
            "--queue",
            "I",
            "--no-hold",
            "--no-mirror",
            "--tiling-only",
            "--backend",
            "cpu",
            "--workers",
            "1",
        ]);

        assert_eq!(value["summary"]["coverage_probability"], "not-calculated");
    }

    #[cfg(feature = "wasm-cpu-runtime")]
    #[test]
    fn wasm_pc_json_preserves_builtin_srs_x_rule_and_kick_identity() {
        let value = run_json(&[
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
            "--rule",
            "srs-x",
            "--backend",
            "cpu",
            "--workers",
            "1",
        ]);

        assert_eq!(value["summary"]["rule_profile"], "srs-x");
        assert_eq!(value["summary"]["kick_profile"], "srs-x");
        assert_eq!(value["summary"]["effective_kick_model"], "srs-x");
        assert_eq!(value["summary"]["verified_kick_profile"], true);
        assert_eq!(value["summary"]["kick_profile_transition_count"], 80);
    }

    #[cfg(not(feature = "wasm-cpu-runtime"))]
    #[test]
    fn build_probability_native_only_harness_fails_closed_without_a_connected_backend() {
        let output = run_with_args([
            "clearra",
            "--format",
            "json",
            "build-probability",
            "--base-mask",
            "0x0",
            "--target-mask",
            "0xf",
            "--height",
            "4",
            "--queue",
            "I",
            "--no-hold",
            "--no-mirror",
            "--tiling-only",
            "--backend",
            "cpu",
            "--workers",
            "1",
        ]);

        assert_eq!(output.exit_code(), ExitCode::Unsupported);
        assert!(output
            .stderr()
            .contains(CliErrorCode::ProductRuntimeUnsupported.as_str()));
        assert!(output
            .stderr()
            .contains("native_build_probability_backend_not_connected"));
    }
}

mod case_run_with_args_verbose_text_contains_executor_flow {
    use super::*;

    #[test]
    fn run_with_args_verbose_text_contains_executor_flow() {
        let output = run_with_args([
            "clearra",
            "--verbose",
            "pc",
            "--lines",
            "2",
            "--queue",
            "IJLOO",
            "--fixed",
            "--no-hold",
        ]);

        assert_eq!(output.exit_code(), ExitCode::Success);
        assert!(output.stdout().contains("executor_flow:"));
        assert!(output
            .stdout()
            .contains("route: search-problem-core-executor"));
    }
}

mod case_run_with_args_selects_korean_help_label_without_translating_contract_keys {
    use super::*;

    #[test]
    fn run_with_args_selects_korean_help_label_without_translating_contract_keys() {
        let output = run_with_args(["clearra", "--lang", "ko", "--help"]);

        assert_eq!(output.exit_code(), ExitCode::Success);
        assert!(output.stdout().contains("Clearra 명령줄"));
        assert!(output.stdout().contains("--lang en|ko"));
        assert!(output.stdout().contains("usage: clearra"));
        assert!(output
            .stdout()
            .contains("finesse search: clearra finesse search"));
        assert!(output
            .stdout()
            .contains("finesse score: clearra finesse score"));
        assert!(output
            .stdout()
            .contains("build-probability finesse: add --finesse inputs"));
    }
}

mod case_run_with_args_routes_pc_queue_hold_and_objective {
    use super::*;

    #[test]
    fn run_with_args_routes_pc_queue_hold_and_objective() {
        let output = run_with_args([
            "clearra",
            "--verbose",
            "pc",
            "--lines",
            "2",
            "--queue",
            "I,I,O,O,O",
            "--fixed",
            "--no-hold",
            "--objective",
            "unique",
        ]);

        assert_eq!(output.exit_code(), ExitCode::Success);
        assert!(output.stdout().contains("queue_mode: fixed"));
        assert!(output.stdout().contains("queue_len: 5"));
        assert!(output.stdout().contains("hold_enabled: false"));
        assert!(output.stdout().contains("objective: unique"));
        assert!(output
            .stdout()
            .contains("route: search-problem-core-executor"));
        assert!(output
            .stdout()
            .contains("two_line_fallback_reason: unsupported_hold_disabled"));
    }
}

mod case_run_with_args_routes_pc_scenario_fixture {
    use super::*;

    #[test]
    fn run_with_args_routes_pc_scenario_fixture() {
        let output = run_with_args([
            "clearra".to_owned(),
            "pc-scenario".to_owned(),
            "--fixture".to_owned(),
            fixture_path("tests/fixtures/pc/example.json")
                .display()
                .to_string(),
            "--verify-expected".to_owned(),
            "--format".to_owned(),
            "json".to_owned(),
        ]);

        assert_eq!(output.exit_code(), ExitCode::Success);
        assert!(output.stdout().contains("\"kind\":\"pc-scenario\""));
        assert!(output.stdout().contains("\"expected_checked\":true"));
        assert!(output.stdout().contains("\"expected_match\":true"));
        assert!(output.stdout().contains("\"exact_pieces\":1"));
        assert!(output.stdout().contains("\"min_remaining_queue\":0"));
        assert!(output.stdout().contains("\"allow_hold\":true"));
        assert!(output.stdout().contains("\"count_policy\":\"count-all\""));
        assert!(output.stdout().contains("\"retained_trace_limit\":1"));
        assert!(output.stdout().contains("\"solution_found\":true"));
        assert!(output.stdout().contains("\"total_solution_count\":1"));
        assert!(output
            .stdout()
            .contains(if cfg!(feature = "native-c-core") {
                "\"count_complete\":true"
            } else {
                "\"count_complete\":false"
            }));
    }
}

mod case_run_with_args_accepts_duplicate_opening_fixed_sequence {
    use super::*;

    #[test]
    fn run_with_args_accepts_duplicate_opening_fixed_sequence() {
        let output = run_with_args([
            "clearra",
            "pc",
            "--lines",
            "2",
            "--queue",
            "IIOOO",
            "--fixed",
            "--no-hold",
            "--format",
            "json",
        ]);

        assert_eq!(output.exit_code(), ExitCode::Success);
        assert!(output.stdout().contains("\"queue_mode\":\"fixed\""));
        assert!(output.stdout().contains("\"queue_len\":5"));
        assert!(output.stdout().contains("\"solution_found\":true"));
    }
}

mod case_run_with_args_selects_command_local_output_format {
    use super::*;

    #[test]
    fn run_with_args_selects_command_local_output_format() {
        let output = run_with_args([
            "clearra",
            "pc",
            "--format",
            "fumen-like",
            "--lines",
            "2",
            "--queue",
            "IJLOO",
            "--fixed",
            "--no-hold",
        ]);

        assert_eq!(output.exit_code(), ExitCode::Success);
        let decoded = FumenLikeReader::read(output.stdout()).expect("fumen-like output");
        assert_eq!(decoded.pages().len(), 1);
        assert!(decoded.pages()[0].contains("kind=pc"));
        assert!(decoded.pages()[0].contains("status=searched"));
        assert!(decoded.pages()[0].contains("route=search-problem-core-executor"));
        assert!(decoded.pages()[0].contains("supply_pattern_count=1"));
        assert!(decoded.pages()[0].contains("supply_probability_model=fixed-sequence"));
        assert!(decoded.pages()[0].contains("supply_probability_complete=true"));
        assert!(decoded.pages()[0].contains("supply_expansion_truncated=false"));
        assert!(decoded.pages()[0].contains("two_line_fallback_reason=unsupported_hold_disabled"));
        assert!(
            decoded.pages()[0].contains(&format!("solver_backend={}", expected_solver_backend()))
        );
        assert!(decoded.pages()[0].contains("objective_execution=all-traces"));
        assert!(decoded.pages()[0].contains("objective_search_mode=all-traces"));
        assert!(decoded.pages()[0].contains("objective_applied=true"));
        assert!(decoded.pages()[0].contains("checkpoints=1"));
    }
}

mod case_run_with_args_rejects_unknown_output_format {
    use super::*;

    #[test]
    fn run_with_args_rejects_unknown_output_format() {
        let output = run_with_args(["clearra", "pc", "--format", "yaml"]);

        assert_eq!(output.exit_code(), ExitCode::ValidationFailed);
        assert!(output
            .stderr()
            .contains(CliErrorCode::CliOutputFormatUnsupported.as_str()));
    }
}

mod case_run_with_args_routes_setup_command {
    use super::*;

    #[test]
    #[ignore = "full empty-4L exact setup acceptance runs in the release exact suite"]
    fn run_with_args_routes_setup_command() {
        let output = run_with_args(["clearra", "--verbose", "setup", "--remaining", "IOTSZJL"]);

        assert_eq!(output.exit_code(), ExitCode::Success);
        assert!(output.stdout().contains("status: setup-finder-complete"));
        assert!(output
            .stdout()
            .contains("backend_selected: wasm-cpu-setup-family-quotient"));
        assert!(output
            .stdout()
            .contains("coverage_semantics: full-future-oracle"));
        assert!(output.stdout().contains("cycle: 1"));
        assert!(output.stdout().contains("remaining_pieces: IOTSZJL"));
    }
}

mod case_run_with_args_routes_rules_scoring_path_and_percent_commands {
    use super::*;

    #[test]
    fn run_with_args_routes_rules_scoring_path_and_percent_commands() {
        let rules = run_with_args(["clearra", "rules", "inspect", "--profile", "srs"]);
        let scoring = run_with_args(["clearra", "scoring", "inspect", "--profile", "jstris-ultra"]);
        let path = run_with_args([
            "clearra",
            "path",
            "--lines",
            "2",
            "--queue",
            "IIOOO",
            "--fixed",
            "--no-hold",
        ]);
        let percent = run_with_args(["clearra", "percent", "--queue", "IOT", "--min-len", "5"]);

        assert_eq!(rules.exit_code(), ExitCode::Success);
        assert!(rules.stdout().contains("effective_kick_model: srs-90"));
        assert_eq!(scoring.exit_code(), ExitCode::Success);
        assert!(scoring.stdout().contains("score_model: jstris-ultra"));
        assert_eq!(path.exit_code(), ExitCode::Success);
        assert!(path.stdout().contains("kind: path"));
        assert_eq!(percent.exit_code(), ExitCode::Success);
        assert!(percent.stdout().contains("kind: percent"));
    }
}

mod case_run_with_args_routes_pc_scenario_inline_input {
    use super::*;

    #[test]
    fn run_with_args_routes_pc_scenario_inline_input() {
        let output = run_with_args([
            "clearra",
            "pc-scenario",
            "--field",
            "0x00000000000003f0",
            "--queue",
            "I",
            "--max-pieces",
            "1",
            "--exact-pieces",
            "1",
            "--retained-trace-limit",
            "1",
            "--format",
            "json",
        ]);

        assert_eq!(output.exit_code(), ExitCode::Success);
        assert!(output.stdout().contains("\"input_mode\":\"inline\""));
        assert!(output.stdout().contains("\"solution_found\":true"));
    }
}

mod case_run_with_args_routes_cover_command {
    use super::*;

    #[test]
    fn run_with_args_routes_cover_command() {
        let output = run_with_args(["clearra", "cover", "--template", "basic"]);

        assert_eq!(output.exit_code(), ExitCode::Success);
        assert!(output.stdout().contains("basic"));
    }
}

mod case_run_with_args_routes_cover_native_template_json_export {
    use super::*;

    #[test]
    fn run_with_args_routes_cover_native_template_json_export() {
        let output = run_with_args([
            "clearra",
            "cover",
            "--template",
            "basic",
            "--export-template-json",
        ]);

        assert_eq!(output.exit_code(), ExitCode::Success);
        assert!(output.stdout().contains("\"schema_version\": 2"));
        assert!(output.stdout().contains("\"id\": \"basic\""));
    }
}

mod case_run_with_args_routes_convert_command {
    use super::*;

    #[test]
    fn run_with_args_routes_convert_command() {
        let input =
            FumenLikeWriter::write(&FumenLikeTrace::new(vec!["kind=pc\nlines=2".to_owned()]))
                .expect("test fumen");
        let output = run_with_args(vec![
            "clearra".to_owned(),
            "convert".to_owned(),
            "--from".to_owned(),
            "fumen-like".to_owned(),
            "--to".to_owned(),
            "json".to_owned(),
            "--input".to_owned(),
            input,
        ]);

        assert_eq!(output.exit_code(), ExitCode::Success);
        assert!(output.stdout().contains("\"kind\":\"convert\""));
        assert!(output.stdout().contains("\"page_0\":\"kind=pc\\nlines=2\""));
    }
}

mod case_run_with_args_routes_continue_command_without_prompting {
    use super::*;

    #[test]
    fn run_with_args_routes_continue_command_without_prompting() {
        let output = run_with_args([
            "clearra",
            "continue",
            "pc2:l2:bdstandard-10:psstandard-tetrominoes:bgstandard-7-bag:rsrs-plus:oall:e0:hnone:qIIOOO",
        ]);

        assert_eq!(output.exit_code(), ExitCode::Success);
        assert!(output.stdout().contains("kind: continue"));
        assert!(output.stdout().contains("status: continued-searched"));
        assert!(output.stdout().contains("interactive_prompt: false"));
        assert!(output.stdout().contains("queue_mode: fixed"));
        assert!(output.stdout().contains("queue_len: 5"));
        assert!(output.stdout().contains("rule_profile: srs-plus"));
        assert!(output.stdout().contains("objective: all"));
    }
}

mod case_run_with_args_routes_scenario_continue_token_without_prompting {
    use super::*;

    #[test]
    fn run_with_args_routes_scenario_continue_token_without_prompting() {
        let output = run_with_args([
            "clearra",
            "--verbose",
            "continue",
            "sc2:w10:v2:m0x00000000000003f0:psstandard-tetrominoes:bgstandard-7-bag:rsrs-plus:hnone:qI:p1:x1:n0:a1:z0:gclear-to-empty:ccount-all:t1",
        ]);

        assert_eq!(output.exit_code(), ExitCode::Success);
        assert!(output
            .stdout()
            .contains("status: scenario-continued-searched"));
        assert!(output.stdout().contains("interactive_prompt: false"));
        assert!(output.stdout().contains("exact_pieces: 1"));
        assert!(output.stdout().contains("retained_trace_limit: 1"));
    }
}

mod case_run_with_args_rejects_unsupported_convert_direction {
    use super::*;

    #[test]
    fn run_with_args_rejects_unsupported_convert_direction() {
        let output = run_with_args([
            "clearra",
            "convert",
            "--from",
            "json",
            "--to",
            "fumen-like",
            "--input",
            "{}",
        ]);

        assert_eq!(output.exit_code(), ExitCode::Unsupported);
        assert!(output
            .stderr()
            .contains(CliErrorCode::ConvertDirectionUnsupported.as_str()));
    }
}

mod case_run_with_args_routes_verify_command {
    use super::*;

    #[test]
    fn run_with_args_routes_verify_command() {
        let output = run_with_args(["clearra", "verify", "pc"]);

        assert_eq!(output.exit_code(), ExitCode::Success, "{}", output.stderr());
        assert!(output.stdout().contains("kind: pc"));
        assert!(output.stdout().contains("queue_len: 5"));
        assert!(output.stdout().contains("hold_enabled: false"));
    }
}

mod case_run_with_args_routes_verify_kicks_command {
    use super::*;

    #[test]
    fn run_with_args_routes_verify_kicks_command() {
        let output = run_with_args(["clearra", "verify", "kicks"]);

        assert_eq!(output.exit_code(), ExitCode::Success);
        assert!(output.stdout().contains("kind: verify-kicks"));
        assert!(output.stdout().contains("srs_jlstz_transitions: 8"));
        assert!(output
            .stdout()
            .contains("srs_plus_effective_kick_model: srs-plus-180"));
        assert!(output.stdout().contains("srs_plus_180_transitions: 24"));
    }
}

mod case_run_with_args_reports_unknown_command {
    use super::*;

    #[test]
    fn run_with_args_reports_unknown_command() {
        let output = run_with_args(["clearra", "wat"]);

        assert_eq!(output.exit_code(), ExitCode::ValidationFailed);
        assert!(output
            .stderr()
            .contains(CliErrorCode::CliCommandUnknown.as_str()));
    }
}

mod case_run_with_args_reports_unsupported_mvp_command {
    use super::*;

    #[test]
    fn run_with_args_reports_unsupported_mvp_command() {
        for command in ["inspect"] {
            let output = run_with_args(["clearra", command]);

            assert_eq!(output.exit_code(), ExitCode::Unsupported);
            assert!(output
                .stderr()
                .contains(CliErrorCode::CliCommandUnsupported.as_str()));
            assert!(output.stderr().contains(command));
            assert!(output
                .stderr()
                .contains("use rules inspect or scoring inspect for profile inspection"));
        }
    }
}

mod case_run_with_args_reports_missing_option_value {
    use super::*;

    #[test]
    fn run_with_args_reports_missing_option_value() {
        let output = run_with_args(["clearra", "pc", "--lines"]);

        assert_eq!(output.exit_code(), ExitCode::ValidationFailed);
        assert!(output
            .stderr()
            .contains(CliErrorCode::CliMissingValue.as_str()));
    }
}

mod case_run_with_args_redacts_file_input_paths_by_default {
    use super::*;

    #[test]
    fn run_with_args_redacts_file_input_paths_by_default() {
        let path =
            std::env::temp_dir().join(format!("clearra-run-redacted-{}.txt", unique_suffix()));
        let output = run_with_args([
            "clearra".to_owned(),
            "cover".to_owned(),
            "--template-file".to_owned(),
            path.display().to_string(),
        ]);

        assert_eq!(output.exit_code(), ExitCode::ValidationFailed);
        assert!(output.stderr().contains(".../clearra-run-redacted-"));
        assert!(!output
            .stderr()
            .contains(&std::env::temp_dir().display().to_string()));
    }
}

mod case_run_with_args_verbose_paths_exposes_file_input_paths_explicitly {
    use super::*;

    #[test]
    fn run_with_args_verbose_paths_exposes_file_input_paths_explicitly() {
        let path =
            std::env::temp_dir().join(format!("clearra-run-verbose-{}.txt", unique_suffix()));
        let output = run_with_args([
            "clearra".to_owned(),
            "--verbose-paths".to_owned(),
            "cover".to_owned(),
            "--template-file".to_owned(),
            path.display().to_string(),
        ]);

        assert_eq!(output.exit_code(), ExitCode::ValidationFailed);
        assert!(output.stderr().contains(&path.display().to_string()));
    }
}
