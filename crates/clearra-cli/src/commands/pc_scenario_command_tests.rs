use std::path::PathBuf;

use clearra_app::{AppCommand, AppRenderModel, AppRequest, AppStatus, ScenarioAppCommand};
use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_pc_graph::request::PcCountPolicy;
use clearra_rules::kicks::{KickImport, NoKick};
use clearra_validation::diagnostic::diagnostic_code::DiagnosticCode;
use serde_json::{json, Value};

use crate::{
    args::pc_scenario_args::PcScenarioArgs,
    assemble::PcScenarioQueryAssembler,
    error::CliErrorCode,
    exit::ExitCode,
    output::{CliOutput, RenderFormat},
};

use super::PcScenarioCommand;

fn fixture_path(relative_path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("workspace root")
        .join(relative_path)
}

fn expected_actual_solution_count() -> usize {
    // With an empty hold and a one-piece queue there is no legal
    // store-current branch: that transition requires a next current piece.
    // The finite terminal projection can release only a piece that was
    // already held when the concrete source ended, so direct use of I is the
    // fixture's sole accepting execution on every backend.
    1
}

fn successful_scenario_json(output: &CliOutput) -> Value {
    assert_eq!(output.exit_code(), ExitCode::Success, "{}", output.stderr());
    let result: Value = serde_json::from_str(output.stdout()).expect("scenario JSON");
    assert_eq!(result["kind"], "pc-scenario");
    result
}

fn assert_one_line_pc(result: &Value) {
    let pc = &result["contract"]["pc"];
    assert_eq!(pc["search"]["solution_found"], true);
    assert_eq!(pc["counting"]["count_complete"], true);
    assert_eq!(
        pc["counts"]["normalized_unique_solution_count"].as_u64(),
        Some(expected_actual_solution_count() as u64),
    );
}

fn assert_no_continuation_or_replay(result: &Value) {
    let pc = &result["contract"]["pc"];
    assert!(pc["continuation"]["token"].is_null());
    assert!(pc["replay"]["token"].is_null());
    let hint = &pc["remaining"]["continue_hint"];
    assert!(hint.is_null() || hint == "none");
}

fn assert_separator_queue(args: &PcScenarioArgs) {
    let assembly = PcScenarioQueryAssembler::assemble(args).expect("scenario assembly");
    let query = assembly.query();
    let queue = query
        .remaining_queue()
        .as_fixed_sequence()
        .expect("fixed queue");
    assert_eq!(queue.pieces(), &[PieceKind::I, PieceKind::O, PieceKind::T]);
    assert_eq!(query.piece_window().max_pieces(), 1);
    assert_eq!(query.exact_pieces(), Some(1));
}

fn write_explicit_one_line_fixture(keep_native_trace_expectation: bool) -> PathBuf {
    let mut fixture: Value =
        serde_json::from_str(include_str!("../../../../tests/fixtures/pc/example.json"))
            .expect("valid historical scenario fixture");
    // The direct API uses the declared target extent. Here the six occupied
    // cells and one I complete exactly one row, not an entire two-row frame.
    fixture["scenario"]["visible_height"] = json!(1);
    if !keep_native_trace_expectation {
        fixture["expected"]["accepted_retained_trace_keys"] = json!([]);
    }
    let path = temp_fixture_path("clearra-pc-scenario-explicit-line");
    std::fs::write(&path, fixture.to_string()).expect("write explicit one-line fixture");
    path
}

fn write_mismatched_fixture() -> PathBuf {
    let path = temp_fixture_path("clearra-pc-scenario");
    std::fs::write(
        &path,
        format!(
            r#"{{
  "name": "mismatched_setup_pc",
  "source": {{
    "site": "clearra",
    "page": "command fixture test",
    "section": "expected mismatch",
    "human_verified": true
  }},
  "scenario": {{
    "board_width": 10,
    "visible_height": 1,
    "initial_board_mask": "0x00000000000003f0",
    "remaining_queue": "I",
    "hold": null,
    "rule": "srs-90",
    "requires_180": false,
    "goal": "clear-to-empty",
    "max_pieces": 1
  }},
  "expected": {{
    "solution_exists": true,
    "expected_total_solution_count": {},
    "accepted_retained_trace_keys": []
  }}
}}"#,
            expected_actual_solution_count() + 1
        ),
    )
    .expect("write mismatched fixture");
    path
}

fn write_separator_queue_fixture() -> PathBuf {
    let path = temp_fixture_path("clearra-pc-scenario-separated-queue");
    std::fs::write(
        &path,
        r#"{
  "name": "separator_queue_setup_pc",
  "source": {
    "site": "clearra",
    "page": "command fixture test",
    "section": "queue separators",
    "human_verified": true
  },
  "scenario": {
    "board_width": 10,
    "visible_height": 1,
    "initial_board_mask": "0x00000000000003f0",
    "remaining_queue": "I, O T",
    "hold": null,
    "rule": "srs-90",
    "requires_180": false,
    "goal": "clear-to-empty",
    "max_pieces": 1,
    "exact_pieces": 1,
    "min_remaining_queue": 0,
    "allow_hold": true,
    "count_policy": "count-all",
    "retained_trace_limit": 1
  },
  "expected": {
    "solution_exists": true,
    "expected_total_solution_count": null,
    "accepted_retained_trace_keys": []
  }
}"#,
    )
    .expect("write separator queue fixture");
    path
}

fn temp_fixture_path(prefix: &str) -> PathBuf {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir().join(format!("{prefix}-{unique}.json"))
}

mod case_pc_scenario_command_runs_fixture_through_validation_and_search {
    use super::*;

    #[test]
    fn pc_scenario_command_runs_fixture_through_validation_and_search() {
        let path = write_explicit_one_line_fixture(false);
        let args = PcScenarioArgs::new(Some(path.display().to_string()));
        let assembly = PcScenarioQueryAssembler::assemble(&args).expect("fixture query");
        let output = PcScenarioCommand::run(&args, RenderFormat::Json);
        std::fs::remove_file(path).expect("remove test fixture");

        let query = assembly.query();
        assert_eq!(query.exact_pieces(), Some(1));
        assert_eq!(query.min_remaining_queue(), 0);
        assert!(query.allow_hold());
        assert_eq!(query.count_policy(), PcCountPolicy::CountAll);
        assert_eq!(query.retained_trace_limit(), 1);

        let result = successful_scenario_json(&output);
        let summary = &result["summary"];
        assert_eq!(summary["fixture_name"], "example_setup_pc");
        assert_eq!(summary["fixture_source_site"], "harddrop");
        assert_eq!(summary["expected_checked"], false);
        assert_one_line_pc(&result);
        let trace = &result["contract"]["pc"]["trace"];
        assert_eq!(trace["retained_trace_count"], 1);
        assert_eq!(trace["trace_steps"], 1);
        assert_no_continuation_or_replay(&result);
    }
}

mod case_pc_scenario_command_verifies_fixture_expected_contract_when_requested {
    use super::*;

    #[test]
    fn pc_scenario_command_verifies_fixture_expected_contract_when_requested() {
        let path = write_explicit_one_line_fixture(false);
        let output = PcScenarioCommand::run(
            &PcScenarioArgs::new(Some(path.display().to_string())).with_verify_expected(true),
            RenderFormat::Json,
        );
        std::fs::remove_file(path).expect("remove test fixture");

        assert_eq!(output.exit_code(), ExitCode::Success);
        assert!(output.stdout().contains("\"expected_checked\":true"));
        assert!(output.stdout().contains("\"expected_match\":true"));
        assert!(output
            .stdout()
            .contains("\"normalized_unique_solution_count\":1"));
        assert!(output.stdout().contains("\"count_complete\":true"));
    }

    #[test]
    fn pc_scenario_command_does_not_claim_legacy_c_trace_key_evidence() {
        // This fixture explicitly asks for a C candidate/variant ordinal,
        // not a portable witness identity. The product must fail verification
        // rather than claim a match against a Rust lock replay.
        let path = write_explicit_one_line_fixture(true);
        let output = PcScenarioCommand::run(
            &PcScenarioArgs::new(Some(path.display().to_string())).with_verify_expected(true),
            RenderFormat::Json,
        );
        std::fs::remove_file(path).expect("remove test fixture");
        assert_eq!(output.exit_code(), ExitCode::ValidationFailed);
        assert!(output
            .stderr()
            .contains(CliErrorCode::PcScenarioExpectedMismatch.as_str()));
        assert!(output.stderr().contains("retained_trace_keys"));
    }
}

mod case_pc_scenario_command_accepts_inline_board_and_queue {
    use super::*;

    #[test]
    fn pc_scenario_command_accepts_inline_board_and_queue() {
        let output = PcScenarioCommand::run(
            &PcScenarioArgs::new(None)
                .with_field(Some("0x00000000000003f0".to_owned()))
                .with_queue(Some("I".to_owned()))
                .with_visible_height(Some(1))
                .with_max_pieces(Some(1))
                .with_exact_pieces(Some(1))
                .with_count_policy(Some("count-all".to_owned()))
                .with_retained_trace_limit(Some(1)),
            RenderFormat::Json,
        );

        let result = successful_scenario_json(&output);
        assert_eq!(result["summary"]["input_mode"], "inline");
        assert_one_line_pc(&result);
        assert_no_continuation_or_replay(&result);
    }
}

mod case_pc_scenario_command_accepts_verified_kick_profile_override {
    use super::*;

    #[test]
    fn pc_scenario_command_accepts_verified_kick_profile_override() {
        let kick_profile = NoKick::profile();
        let import_json = KickImport::to_json(&kick_profile).expect("no-kick json");
        let args = PcScenarioArgs::new(None)
            .with_field(Some("0x00000000000003f0".to_owned()))
            .with_queue(Some("I".to_owned()))
            .with_visible_height(Some(1))
            .with_rule(Some("no-kick".to_owned()))
            .with_kick_profile_json(Some(import_json))
            .with_max_pieces(Some(1))
            .with_exact_pieces(Some(1))
            .with_retained_trace_limit(Some(1));
        let output = PcScenarioCommand::run(&args, RenderFormat::Json);
        let result = successful_scenario_json(&output);
        assert_one_line_pc(&result);
        let rule = &result["contract"]["pc"]["rule"];
        assert_eq!(rule["rule_profile"], "no-kick");
        assert_eq!(rule["kick_profile"], kick_profile.id().as_str());
        assert_eq!(rule["verified_kick_profile"], true);

        // The direct CLI summary reports trace counts, not the concrete locks.
        // Inspect the same App route before rendering to verify the witness.
        let assembly = PcScenarioQueryAssembler::assemble(&args).expect("verified profile query");
        let response = crate::cli_routing::product_app_context().run(AppRequest::new(
            AppCommand::Scenario(ScenarioAppCommand::new(assembly.query().clone())),
        ));
        assert_eq!(response.status(), AppStatus::Success);
        let Some(AppRenderModel::Scenario(result)) = response.render_model() else {
            panic!("missing typed scenario result");
        };
        let [step] = result.path_steps() else {
            panic!("expected one retained lock");
        };
        assert_eq!(step.piece(), PieceKind::I);
        assert!(matches!(
            RotationState::from_quarter_turns(step.rotation()),
            Ok(RotationState::Zero | RotationState::Two)
        ));
        assert_eq!(step.cleared_lines(), 1);
    }
}

mod case_pc_scenario_command_reuses_fixed_sequence_parser_for_queue_separators {
    use super::*;

    #[test]
    fn pc_scenario_command_reuses_fixed_sequence_parser_for_queue_separators() {
        let args = PcScenarioArgs::new(None)
            .with_field(Some("0x00000000000003f0".to_owned()))
            .with_queue(Some("I, O T".to_owned()))
            .with_visible_height(Some(1))
            .with_max_pieces(Some(1))
            .with_exact_pieces(Some(1));
        assert_separator_queue(&args);
        let output = PcScenarioCommand::run(&args, RenderFormat::Json);
        let result = successful_scenario_json(&output);
        assert_one_line_pc(&result);
    }
}

mod case_pc_scenario_command_fixture_queue_accepts_opening_style_separators {
    use super::*;

    #[test]
    fn pc_scenario_command_fixture_queue_accepts_opening_style_separators() {
        let path = write_separator_queue_fixture();
        let args = PcScenarioArgs::new(Some(path.display().to_string()));
        assert_separator_queue(&args);
        let output = PcScenarioCommand::run(&args, RenderFormat::Json);
        std::fs::remove_file(path).expect("remove test fixture");

        let result = successful_scenario_json(&output);
        assert_eq!(result["summary"]["input_mode"], "fixture");
        assert_one_line_pc(&result);
    }
}

mod case_pc_scenario_command_marks_empty_retained_trace_key_expectation_as_not_checked {
    use super::*;

    #[test]
    fn pc_scenario_command_marks_empty_retained_trace_key_expectation_as_not_checked() {
        let path = write_separator_queue_fixture();
        let output = PcScenarioCommand::run(
            &PcScenarioArgs::new(Some(path.display().to_string())).with_verify_expected(true),
            RenderFormat::Json,
        );

        assert_eq!(output.exit_code(), ExitCode::Success);
        assert!(output.stdout().contains("\"expected_checked\":true"));
        assert!(output.stdout().contains("\"expected_match\":true"));
        assert!(output
            .stdout()
            .contains("\"expected_retained_trace_key_count\":0"));
        assert!(output
            .stdout()
            .contains("\"retained_trace_keys_checked\":false"));
        assert!(output
            .stdout()
            .contains("\"retained_trace_keys_match\":\"none\""));
        let _ = std::fs::remove_file(path);
    }
}

mod case_pc_scenario_command_fails_verify_expected_on_count_mismatch {
    use super::*;

    #[test]
    fn pc_scenario_command_fails_verify_expected_on_count_mismatch() {
        let path = write_mismatched_fixture();
        let output = PcScenarioCommand::run(
            &PcScenarioArgs::new(Some(path.display().to_string())).with_verify_expected(true),
            RenderFormat::Json,
        );

        assert_eq!(output.exit_code(), ExitCode::ValidationFailed);
        assert!(output
            .stderr()
            .contains(CliErrorCode::PcScenarioExpectedMismatch.as_str()));
        assert!(output.stderr().contains("total_solution_count"));
        let _ = std::fs::remove_file(path);
    }
}

mod case_pc_scenario_command_rejects_requires_180_fixture_before_search {
    use super::*;

    #[test]
    fn pc_scenario_command_rejects_requires_180_fixture_before_search() {
        let output = PcScenarioCommand::run(
            &PcScenarioArgs::new(Some(
                fixture_path("tests/fixtures/pc/requires_180_unsupported.json")
                    .display()
                    .to_string(),
            )),
            RenderFormat::Json,
        );

        assert_eq!(output.exit_code(), ExitCode::ValidationFailed);
        assert_eq!(output.stderr(), "");
        assert!(output
            .stdout()
            .contains(DiagnosticCode::EPcQueryInvalid.as_str()));
        assert!(output
            .stdout()
            .contains("selected rule profile does not support 180 kicks"));
    }
}

mod case_pc_scenario_command_treats_expected_unsupported_fixture_as_verified_success {
    use super::*;

    #[test]
    fn pc_scenario_command_treats_expected_unsupported_fixture_as_verified_success() {
        let output = PcScenarioCommand::run(
            &PcScenarioArgs::new(Some(
                fixture_path("tests/fixtures/pc/requires_180_unsupported.json")
                    .display()
                    .to_string(),
            ))
            .with_verify_expected(true),
            RenderFormat::Json,
        );

        assert_eq!(output.exit_code(), ExitCode::Success);
        assert!(output.stdout().contains("\"expected_checked\":true"));
        assert!(output.stdout().contains("\"expected_match\":true"));
        assert!(output.stdout().contains("\"expected_unsupported\":true"));
        assert!(output.stdout().contains("\"actual_unsupported\":true"));
        assert!(output
            .stdout()
            .contains("\"unsupported_stage\":\"validation\""));
        assert!(output
            .stdout()
            .contains("\"actual_unsupported_reason\":\"scenario_requires_180_unsupported\""));
        assert!(output
            .stdout()
            .contains("\"status\":\"scenario-unsupported-expected\""));
        assert!(output.stdout().contains(&format!(
            "\"validation_code\":\"{}\"",
            DiagnosticCode::EPcQueryInvalid.as_str()
        )));
    }
}
