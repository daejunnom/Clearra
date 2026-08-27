use std::path::PathBuf;

use clearra_app::{AppCommand, AppContext, AppRenderModel, AppRequest, ScenarioAppCommand};
use clearra_validation::diagnostic::diagnostic_code::DiagnosticCode;

use crate::{
    args::pc_scenario_args::PcScenarioArgs, assemble::PcScenarioQueryAssembler,
    error::CliErrorCode, exit::ExitCode, output::RenderFormat,
};

use super::PcScenarioCommand;

fn fixture_path(relative_path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("workspace root")
        .join(relative_path)
}

fn expected_count_complete_json() -> &'static str {
    if cfg!(feature = "native-c-core") {
        "\"count_complete\":true"
    } else {
        "\"count_complete\":false"
    }
}

fn expected_actual_solution_count() -> usize {
    // With an empty hold and a one-piece queue there is no legal
    // store-current branch: that transition requires a next current piece.
    // The finite terminal projection can release only a piece that was
    // already held when the concrete source ended, so direct use of I is the
    // fixture's sole accepting execution on every backend.
    1
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
    "visible_height": 2,
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
        format!(
            r#"{{
  "name": "separator_queue_setup_pc",
  "source": {{
    "site": "clearra",
    "page": "command fixture test",
    "section": "queue separators",
    "human_verified": true
  }},
  "scenario": {{
    "board_width": 10,
    "visible_height": 2,
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
  }},
  "expected": {{
    "solution_exists": true,
    "expected_total_solution_count": {},
    "accepted_retained_trace_keys": []
  }}
}}"#,
            expected_actual_solution_count()
        ),
    )
    .expect("write separator queue fixture");
    path
}

fn retained_trace_key_for_example_query() -> String {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let assembly = PcScenarioQueryAssembler::assemble(
        &PcScenarioArgs::new(None)
            .with_field(Some("0x00000000000003f0".to_owned()))
            .with_queue(Some("I".to_owned()))
            .with_rule(Some("srs-90".to_owned()))
            .with_max_pieces(Some(1))
            .with_exact_pieces(Some(1))
            .with_count_policy(Some("count-all".to_owned()))
            .with_retained_trace_limit(Some(1)),
    )
    .expect("example query");
    let response = AppContext::default().run(AppRequest::new(AppCommand::Scenario(
        ScenarioAppCommand::new(assembly.query().clone()),
    )));
    let Some(AppRenderModel::Scenario(result)) = response.render_model() else {
        panic!("scenario app result");
    };
    field_value(&result.summary_fields(), "retained_trace_keys")
        .expect("retained trace keys")
        .to_owned()
}

fn write_fixture_with_accepted_trace_key(accepted_trace_key: &str) -> PathBuf {
    let path = temp_fixture_path("clearra-pc-scenario-accepted-trace");
    std::fs::write(
        &path,
        format!(
            r#"{{
  "name": "accepted_trace_setup_pc",
  "source": {{
    "site": "clearra",
    "page": "command fixture test",
    "section": "accepted trace key",
    "human_verified": true
  }},
  "scenario": {{
    "board_width": 10,
    "visible_height": 2,
    "initial_board_mask": "0x00000000000003f0",
    "remaining_queue": "I",
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
  }},
  "expected": {{
    "solution_exists": true,
    "expected_total_solution_count": {},
    "accepted_retained_trace_keys": ["{accepted_trace_key}"]
  }}
}}"#,
            expected_actual_solution_count()
        ),
    )
    .expect("write accepted trace fixture");
    path
}

fn temp_fixture_path(prefix: &str) -> PathBuf {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir().join(format!("{prefix}-{unique}.json"))
}

fn field_value<'a>(fields: &'a [(String, String)], key: &str) -> Option<&'a str> {
    fields
        .iter()
        .find_map(|(field_key, value)| (field_key == key).then_some(value.as_str()))
}

mod case_pc_scenario_command_runs_fixture_through_validation_and_search {
    use super::*;

    #[test]
    fn pc_scenario_command_runs_fixture_through_validation_and_search() {
        let output = PcScenarioCommand::run(
            &PcScenarioArgs::new(Some(
                fixture_path("tests/fixtures/pc/example.json")
                    .display()
                    .to_string(),
            )),
            RenderFormat::Json,
        );

        assert_eq!(output.exit_code(), ExitCode::Success);
        assert!(output.stdout().contains("\"kind\":\"pc-scenario\""));
        assert!(output
            .stdout()
            .contains("\"fixture_name\":\"example_setup_pc\""));
        assert!(output
            .stdout()
            .contains("\"fixture_source_site\":\"harddrop\""));
        assert!(output.stdout().contains("\"expected_checked\":false"));
        assert!(output.stdout().contains("\"exact_pieces\":1"));
        assert!(output.stdout().contains("\"min_remaining_queue\":0"));
        assert!(output.stdout().contains("\"allow_hold\":true"));
        assert!(output.stdout().contains("\"count_policy\":\"count-all\""));
        assert!(output.stdout().contains("\"retained_trace_limit\":1"));
        assert!(output.stdout().contains("\"solution_found\":true"));
        assert!(output.stdout().contains(&format!(
            "\"total_solution_count\":{}",
            expected_actual_solution_count()
        )));
        assert!(output.stdout().contains(expected_count_complete_json()));
        assert!(output.stdout().contains("\"retained_trace_count\":1"));
        assert!(output
            .stdout()
            .contains("\"continuation_token_version\":\"none\""));
        assert!(output.stdout().contains(
        "\"scenario_replay_token\":\"sr2:w10:v2:m0x00000000000003f0:psstandard-tetrominoes:bgstandard-7-bag:rsrs:hnone:qI:p1:x1:n0:a1:z0:gclear-to-empty:ccount-all:t1:knone:u0:ooracle\""
    ));
        assert!(output.stdout().contains("\"continue_hint\":\"none\""));
        assert!(output
            .stdout()
            .contains("\"replay_hint\":\"clearra continue sr2:"));
    }
}

mod case_pc_scenario_command_verifies_fixture_expected_contract_when_requested {
    use super::*;

    #[test]
    fn pc_scenario_command_verifies_fixture_expected_contract_when_requested() {
        let output = PcScenarioCommand::run(
            &PcScenarioArgs::new(Some(
                fixture_path("tests/fixtures/pc/example.json")
                    .display()
                    .to_string(),
            ))
            .with_verify_expected(true),
            RenderFormat::Json,
        );

        assert_eq!(output.exit_code(), ExitCode::Success);
        assert!(output.stdout().contains("\"expected_checked\":true"));
        assert!(output.stdout().contains("\"expected_match\":true"));
        assert!(output
            .stdout()
            .contains("\"expected_total_solution_count\":\"none\""));
        assert!(output
            .stdout()
            .contains("\"expected_retained_trace_key_count\":1"));
        assert!(output
            .stdout()
            .contains("\"retained_trace_keys_checked\":true"));
        assert!(output
            .stdout()
            .contains("\"retained_trace_keys_match\":true"));
    }
}

mod case_pc_scenario_command_verifies_accepted_retained_trace_keys {
    use super::*;

    #[test]
    fn pc_scenario_command_verifies_accepted_retained_trace_keys() {
        let accepted_key = retained_trace_key_for_example_query();
        let path = write_fixture_with_accepted_trace_key(&accepted_key);
        let output = PcScenarioCommand::run(
            &PcScenarioArgs::new(Some(path.display().to_string())).with_verify_expected(true),
            RenderFormat::Json,
        );

        assert_eq!(output.exit_code(), ExitCode::Success);
        assert!(output.stdout().contains("\"expected_checked\":true"));
        assert!(output
            .stdout()
            .contains("\"expected_retained_trace_key_count\":1"));
        assert!(output
            .stdout()
            .contains("\"retained_trace_keys_checked\":true"));
        assert!(output
            .stdout()
            .contains("\"retained_trace_keys_match\":true"));
        let _ = std::fs::remove_file(path);
    }
}

mod case_pc_scenario_command_rejects_unaccepted_retained_trace_keys {
    use super::*;

    #[test]
    fn pc_scenario_command_rejects_unaccepted_retained_trace_keys() {
        let path = write_fixture_with_accepted_trace_key("trk1:not-the-retained-key");
        let output = PcScenarioCommand::run(
            &PcScenarioArgs::new(Some(path.display().to_string())).with_verify_expected(true),
            RenderFormat::Json,
        );

        assert_eq!(output.exit_code(), ExitCode::ValidationFailed);
        assert!(output
            .stderr()
            .contains(CliErrorCode::PcScenarioExpectedMismatch.as_str()));
        assert!(output
            .stderr()
            .contains("retained_trace_keys include unaccepted retained keys"));
        let _ = std::fs::remove_file(path);
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
                .with_max_pieces(Some(1))
                .with_exact_pieces(Some(1))
                .with_count_policy(Some("count-all".to_owned()))
                .with_retained_trace_limit(Some(1)),
            RenderFormat::Json,
        );

        assert_eq!(output.exit_code(), ExitCode::Success);
        assert!(output.stdout().contains("\"input_mode\":\"inline\""));
        assert!(output.stdout().contains("\"solution_found\":true"));
        assert!(output.stdout().contains(&format!(
            "\"total_solution_count\":{}",
            expected_actual_solution_count()
        )));
        assert!(output
            .stdout()
            .contains("\"continuation_token_version\":\"none\""));
        assert!(output.stdout().contains(
        "\"scenario_replay_token\":\"sr2:w10:v2:m0x00000000000003f0:psstandard-tetrominoes:bgstandard-7-bag:rsrs-plus:hnone:qI:p1:x1:n0:a1:z0:gclear-to-empty:ccount-all:t1:knone:u0:ooracle\""
    ));
        assert!(output.stdout().contains("\"continue_hint\":\"none\""));
    }
}

mod case_pc_scenario_command_accepts_verified_kick_profile_override {
    use super::*;

    #[test]
    fn pc_scenario_command_accepts_verified_kick_profile_override() {
        let import_json =
            clearra_rules::kicks::KickImport::to_json(&clearra_rules::kicks::NoKick::profile())
                .expect("no-kick json");
        let output = PcScenarioCommand::run(
            &PcScenarioArgs::new(None)
                .with_field(Some("0x00000000000003f0".to_owned()))
                .with_queue(Some("I".to_owned()))
                .with_rule(Some("no-kick".to_owned()))
                .with_kick_profile_json(Some(import_json))
                .with_max_pieces(Some(1))
                .with_exact_pieces(Some(1)),
            RenderFormat::Json,
        );

        assert_eq!(output.exit_code(), ExitCode::Success);
        assert!(output.stdout().contains("\"solution_found\":true"));
        assert!(output
            .stdout()
            .contains("\"search_unsupported_reason\":\"none\""));
    }
}

mod case_pc_scenario_command_reuses_fixed_sequence_parser_for_queue_separators {
    use super::*;

    #[test]
    fn pc_scenario_command_reuses_fixed_sequence_parser_for_queue_separators() {
        let output = PcScenarioCommand::run(
            &PcScenarioArgs::new(None)
                .with_field(Some("0x00000000000003f0".to_owned()))
                .with_queue(Some("I, O T".to_owned()))
                .with_max_pieces(Some(1))
                .with_exact_pieces(Some(1)),
            RenderFormat::Json,
        );

        assert_eq!(output.exit_code(), ExitCode::Success);
        assert!(output.stdout().contains("\"queue_len\":3"));
        assert!(output.stdout().contains("\"piece_window\":1"));
        assert!(output.stdout().contains("\"solution_found\":true"));
    }
}

mod case_pc_scenario_command_fixture_queue_accepts_opening_style_separators {
    use super::*;

    #[test]
    fn pc_scenario_command_fixture_queue_accepts_opening_style_separators() {
        let path = write_separator_queue_fixture();
        let output = PcScenarioCommand::run(
            &PcScenarioArgs::new(Some(path.display().to_string())),
            RenderFormat::Json,
        );

        assert_eq!(output.exit_code(), ExitCode::Success);
        assert!(output.stdout().contains("\"input_mode\":\"fixture\""));
        assert!(output.stdout().contains("\"queue_len\":3"));
        assert!(output.stdout().contains("\"piece_window\":1"));
        assert!(output.stdout().contains("\"solution_found\":true"));
        let _ = std::fs::remove_file(path);
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
        assert!(output.stderr().contains(&format!(
            "total_solution_count expected {} but actual {}",
            expected_actual_solution_count() + 1,
            expected_actual_solution_count()
        )));
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
