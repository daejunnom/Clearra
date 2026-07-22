use crate::{error::CliErrorCode, exit::ExitCode};
use clearra_validation::diagnostic::diagnostic_code::DiagnosticCode;

use super::*;

#[test]
fn pc_command_validates_supported_query() {
    let output = PcCommand::run(supported_pc_args(), RenderFormat::TextVerbose);

    assert_eq!(output.exit_code(), ExitCode::Success);
    assert!(output.stdout().contains("status: searched"));
    assert!(output.stdout().contains("lines: 2"));
    assert!(output.stdout().contains("board_profile: standard-10"));
    assert!(output
        .stdout()
        .contains("piece_set_profile: standard-tetrominoes"));
    assert!(output.stdout().contains("bag_profile: standard-7-bag"));
    assert!(output.stdout().contains("rule_profile: srs-plus"));
    assert!(output
        .stdout()
        .contains("effective_kick_model: srs-plus-180"));
    assert!(!output.stdout().contains("rule_extension_reason"));
    assert!(output.stdout().contains("queue_mode: fixed"));
    assert!(output.stdout().contains("supply_pattern_count: 1"));
    assert!(output
        .stdout()
        .contains("supply_probability_model: fixed-sequence"));
    assert!(output
        .stdout()
        .contains("supply_probability_complete: true"));
    assert!(output
        .stdout()
        .contains("supply_expansion_truncated: false"));
    assert!(output.stdout().contains("supply_boundary_candidates: 1"));
    assert!(output.stdout().contains("hold_enabled: false"));
    assert!(output.stdout().contains("two_line_capable: false"));
    assert!(output
        .stdout()
        .contains("two_line_fast_path_available: false"));
    assert!(output
        .stdout()
        .contains("route: search-problem-core-executor"));
    assert!(output
        .stdout()
        .contains("two_line_fallback_reason: unsupported_hold_disabled"));
    assert!(output
        .stdout()
        .contains(&format!("solver_backend: {}", expected_solver_backend())));
    assert!(output
        .stdout()
        .contains("checkpoint_schedule_source: clearra-pc-graph-labels"));
    assert!(output
        .stdout()
        .contains("checkpoint_schedule_partitions: 2"));
    assert!(output.stdout().contains("solution_found: true"));
    assert!(output.stdout().contains("objective: all"));
    assert!(output.stdout().contains("objective_execution: all-traces"));
    assert!(output
        .stdout()
        .contains("objective_search_mode: all-traces"));
    assert!(output.stdout().contains("objective_applied: true"));
    assert!(output.stdout().contains("objective_complete: true"));
    assert!(output.stdout().contains("count_complete: true"));
    assert!(output.stdout().contains("count_truncated_reason: none"));
    assert!(output.stdout().contains("objective_solution_traces: 64"));
    assert!(output.stdout().contains("trace_steps: 5"));
    assert!(output.stdout().contains("trace_available: true"));
    assert!(output.stdout().contains("partitions: 1"));
    assert!(output.stdout().contains("checkpoints: 1"));
}

#[test]
fn pc_command_renders_json_when_selected() {
    let output = PcCommand::run(supported_pc_args(), RenderFormat::Json);

    assert_eq!(output.exit_code(), ExitCode::Success);
    assert!(output.stdout().contains("\"schema_version\":2"));
    assert!(output.stdout().contains("\"kind\":\"pc\""));
    assert!(output.stdout().contains("\"summary\":{"));
    assert!(output.stdout().contains("\"contract\":{\"command\""));
    assert!(output.stdout().contains("\"pc\":{\"search\""));
    assert!(output.stdout().contains("\"status\":\"searched\""));
    assert!(output.stdout().contains("\"lines\":2"));
    assert!(output
        .stdout()
        .contains("\"route\":\"search-problem-core-executor\""));
    assert!(output.stdout().contains("\"two_line_capable\":false"));
    assert!(output
        .stdout()
        .contains("\"two_line_fast_path_available\":false"));
    assert!(output.stdout().contains("\"pattern_count\":1"));
    assert!(output
        .stdout()
        .contains("\"probability_model\":\"fixed-sequence\""));
    assert!(output.stdout().contains("\"probability_complete\":true"));
    assert!(output.stdout().contains("\"expansion_truncated\":false"));
    assert!(output
        .stdout()
        .contains("\"two_line_fallback_reason\":\"unsupported_hold_disabled\""));
    assert!(output.stdout().contains(&format!(
        "\"solver_backend\":\"{}\"",
        expected_solver_backend()
    )));
    assert!(output.stdout().contains("\"objective\":\"all\""));
    assert!(output
        .stdout()
        .contains("\"effective_kick_model\":\"srs-plus-180\""));
    assert!(!output.stdout().contains("\"rule_extension_reason\""));
    assert!(output
        .stdout()
        .contains("\"objective_execution\":\"all-traces\""));
    assert!(output
        .stdout()
        .contains("\"objective_search_mode\":\"all-traces\""));
    assert!(output.stdout().contains("\"applied\":true"));
    assert!(output.stdout().contains("\"solution_traces\":64"));
    assert!(output.stdout().contains("\"trace_steps\":5"));
    assert!(output.stdout().contains("\"trace_available\":true"));
    assert!(output
        .stdout()
        .contains("\"board_profile\":\"standard-10\""));
}

#[test]
fn pc_command_routes_non_two_line_targets_to_generic_search() {
    let output = PcCommand::run(
        PcArgs::new(4).with_queue("I,O,T", true),
        RenderFormat::TextVerbose,
    );

    assert_eq!(output.exit_code(), ExitCode::Success);
    assert!(output
        .stdout()
        .contains("route: search-problem-core-executor"));
    assert!(output
        .stdout()
        .contains(&format!("solver_backend: {}", expected_solver_backend())));
    assert!(output
        .stdout()
        .contains("checkpoint_schedule_partitions: 4|2+2"));
    assert!(output
        .stdout()
        .contains("checkpoint_schedule_checkpoint_count: 3"));
    assert!(output.stdout().contains("count_requested: true"));
    assert!(output.stdout().contains("count_complete: true"));
    assert!(output.stdout().contains("count_truncated_reason: none"));
    assert!(output
        .stdout()
        .contains("two_line_fallback_reason: unsupported_target_lines"));
    assert!(output.stdout().contains("partitions: 2"));
}

#[test]
fn pc_command_reports_unsupported_target() {
    let output = PcCommand::run(PcArgs::new(8), RenderFormat::Text);

    assert_eq!(output.exit_code(), ExitCode::ValidationFailed);
    assert!(output
        .stderr()
        .contains(CliErrorCode::PcTargetUnsupportedMvp.as_str()));
}

#[test]
fn pc_command_routes_disabled_hold_to_generic_search() {
    let output = PcCommand::run(
        PcArgs::new(2)
            .with_queue("IOT", true)
            .with_hold_enabled(false)
            .with_objective("unique"),
        RenderFormat::TextVerbose,
    );

    assert_eq!(output.exit_code(), ExitCode::Success);
    assert!(output.stdout().contains("queue_mode: fixed"));
    assert!(output.stdout().contains("hold_enabled: false"));
    assert!(output.stdout().contains("objective: unique"));
    assert!(output
        .stdout()
        .contains("objective_execution: unique-canonical-traces"));
    assert!(output
        .stdout()
        .contains("objective_search_mode: unique-canonical-traces"));
    assert!(output.stdout().contains("objective_applied: true"));
    assert!(output
        .stdout()
        .contains("route: search-problem-core-executor"));
    assert!(output.stdout().contains("two_line_capable: false"));
    assert!(output
        .stdout()
        .contains("two_line_fast_path_available: false"));
    assert!(output
        .stdout()
        .contains("two_line_fallback_reason: unsupported_hold_disabled"));
}

#[test]
fn pc_command_routes_extension_rule_to_validation_error_before_search() {
    let output = PcCommand::run(
        PcArgs::new(2).with_rule(Some("srs-x".to_owned())),
        RenderFormat::Text,
    );

    assert_eq!(output.exit_code(), ExitCode::ValidationFailed);
    assert!(output
        .stderr()
        .contains(DiagnosticCode::ERuleUnsupportedMvp.as_str()));
    assert!(output
        .stderr()
        .contains("srs_x_profile_requires_imported_kick_table"));
}

#[test]
fn pc_command_accepts_verified_kick_profile_override() {
    let import_json =
        clearra_rules::kicks::KickImport::to_json(&clearra_rules::kicks::NoKick::profile())
            .expect("no-kick json");
    let output = PcCommand::run(
        PcArgs::new(2)
            .with_queue("IIIII", true)
            .with_hold_enabled(false)
            .with_rule(Some("no-kick".to_owned()))
            .with_kick_profile_json(Some(import_json)),
        RenderFormat::Text,
    );

    assert_eq!(output.exit_code(), ExitCode::Success);
    assert!(output.stdout().contains("rule_profile: no-kick"));
}

fn supported_pc_args() -> PcArgs {
    PcArgs::new(2)
        .with_queue("IJLOO", true)
        .with_hold_enabled(false)
}

fn expected_solver_backend() -> &'static str {
    "core-c-cpu-packing-cpu-buildup"
}
