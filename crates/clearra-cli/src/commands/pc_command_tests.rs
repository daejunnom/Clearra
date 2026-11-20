use crate::{error::CliErrorCode, exit::ExitCode};
use serde_json::Value;

use super::*;

fn exact_pc_args(lines: u8, queue: &str) -> PcArgs {
    PcArgs::new(lines)
        .with_queue(queue, true)
        .with_hold_enabled(false)
        .with_backend(Some("cpu".to_owned()))
        .with_workers(Some(1))
}

fn pc_json(args: PcArgs) -> Value {
    let output = PcCommand::run(args, RenderFormat::Json);
    assert_eq!(output.exit_code(), ExitCode::Success, "{}", output.stderr());
    let result: Value = serde_json::from_str(output.stdout()).expect("PC JSON");
    assert_eq!(result["kind"], "pc");
    assert_eq!(result["summary"]["actual_backend"], "wasm-cpu");
    assert_eq!(result["summary"]["count_complete"], true);
    assert_eq!(result["summary"]["probability_complete"], true);
    result
}

#[test]
fn pc_command_validates_supported_query_and_preserves_exact_fields() {
    let result = pc_json(exact_pc_args(2, "IIOOO"));
    let summary = &result["summary"];
    assert_eq!(summary["lines"], 2);
    assert_eq!(summary["rule_profile"], "srs-plus");
    assert_eq!(summary["queue_mode"], "fixed");
    assert_eq!(summary["hold_enabled"], false);
    assert_eq!(summary["covered_pattern_count"], 1);
    assert_eq!(summary["coverage_probability"].as_f64(), Some(1.0));
    assert_eq!(summary["normalized_unique_solution_count"], 4);
    assert_eq!(summary["retained_trace_count"], 1);
    assert_eq!(summary["trace_steps"], 5);
}

#[test]
fn pc_command_renders_json_when_selected() {
    let result = pc_json(exact_pc_args(2, "IIOOO"));
    assert_eq!(result["schema_version"], 2);
    assert!(result["contract"]["pc"]["search"].is_object());
    assert_eq!(result["summary"]["status"], "searched");
}

#[test]
fn pc_command_routes_non_two_line_targets_to_generic_search() {
    let result = pc_json(exact_pc_args(4, "IIOOOIIOOO"));
    assert_eq!(result["summary"]["lines"], 4);
    assert_eq!(result["summary"]["solution_found"], true);
    assert_eq!(result["summary"]["covered_pattern_count"], 1);
    assert_eq!(
        result["summary"]["coverage_probability"].as_f64(),
        Some(1.0)
    );
}

#[test]
fn pc_command_reports_unsupported_target() {
    let output = PcCommand::run(PcArgs::new(26), RenderFormat::Text);
    assert_eq!(output.exit_code(), ExitCode::ValidationFailed);
    assert!(output
        .stderr()
        .contains(CliErrorCode::PcTargetInvalid.as_str()));
}

#[test]
fn pc_command_routes_disabled_hold_to_generic_search() {
    let result = pc_json(exact_pc_args(2, "IIOOO").with_objective("unique"));
    assert_eq!(result["summary"]["objective"], "unique");
    assert_eq!(result["summary"]["hold_enabled"], false);
    assert_eq!(result["summary"]["normalized_unique_solution_count"], 4);
}

#[test]
fn pc_command_routes_builtin_srs_x_through_verified_rust_rules() {
    let result = pc_json(exact_pc_args(2, "IIOOO").with_rule(Some("srs-x".to_owned())));
    let summary = &result["summary"];
    assert_eq!(summary["rule_profile"], "srs-x");
    assert_eq!(summary["effective_kick_model"], "srs-x");
    assert_eq!(summary["verified_kick_profile"], true);
    assert_eq!(summary["kick_profile_transition_count"], 84);
}

#[test]
fn pc_command_accepts_verified_kick_profile_override() {
    let import_json =
        clearra_rules::kicks::KickImport::to_json(&clearra_rules::kicks::NoKick::profile())
            .expect("no-kick json");
    let result = pc_json(
        exact_pc_args(2, "IIIII")
            .with_rule(Some("no-kick".to_owned()))
            .with_kick_profile_json(Some(import_json)),
    );
    assert_eq!(result["summary"]["rule_profile"], "no-kick");
    assert_eq!(result["summary"]["verified_kick_profile"], true);
}
