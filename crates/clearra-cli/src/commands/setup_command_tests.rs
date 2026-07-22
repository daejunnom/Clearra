use crate::{error::CliErrorCode, exit::ExitCode};

use super::*;

#[test]
fn setup_command_validates_query() {
    let output = SetupCommand::run(&SetupArgs::new("IOTSZJL", true), RenderFormat::TextVerbose);

    assert_eq!(output.exit_code(), ExitCode::Success);
    assert!(output.stdout().contains("status: setup-executed"));
    assert!(output
        .stdout()
        .contains("execution_scope: m20-setup-search-product-path"));
    assert!(output.stdout().contains("problem_preset: setup"));
    assert!(output
        .stdout()
        .contains("enumeration_strategy: shape-family-tiling-build-core-buildup"));
    assert!(output.stdout().contains("compiled_goal: clear-to-empty"));
    assert!(output
        .stdout()
        .contains("post_pc_evaluation_attached: false"));
    assert!(output
        .stdout()
        .contains("setup_foundation_reason: core_packing_buildup_build_variants_attached"));
    assert!(output.stdout().contains("build_variant_source: C BuildUp"));
    assert!(output.stdout().contains(&format!(
        "packing_candidate_count: {}",
        expected_setup_foundation_count()
    )));
    assert!(output.stdout().contains(&format!(
        "core_buildup_variant_count: {}",
        expected_setup_foundation_count()
    )));
    assert!(output.stdout().contains("shape_family_id: setup-family-0"));
    assert!(output.stdout().contains(&format!(
        "tiling_variant_count: {}",
        expected_setup_foundation_count()
    )));
    assert!(output.stdout().contains(&format!(
        "build_variant_count: {}",
        expected_setup_foundation_count()
    )));
    assert!(output.stdout().contains(&format!(
        "covered_pattern_count: {}",
        expected_setup_coverage_count()
    )));
    assert!(output.stdout().contains(&format!(
        "coverage_probability: {}",
        expected_setup_probability()
    )));
    assert!(output.stdout().contains("post_pc_solution_count: 0"));
    assert!(output.stdout().contains("score_basis: none"));
    assert!(output.stdout().contains("backend_report: attached"));
    assert!(output.stdout().contains(
        "raw_coverage_export_path: inline://clearra/setup/raw-coverage/setup-family-0/union"
    ));
    assert!(!output.stdout().contains("condition_summary"));
    assert!(output.stdout().contains("queue_len: 7"));
    assert!(output
        .stdout()
        .contains("score_aggregation_attached: false"));
}

#[test]
fn setup_command_reports_bad_queue_piece() {
    let output = SetupCommand::run(&SetupArgs::new("IX", false), RenderFormat::Text);

    assert_eq!(output.exit_code(), ExitCode::ValidationFailed);
    assert!(output
        .stderr()
        .contains(CliErrorCode::SetupQueryInvalid.as_str()));
}

fn expected_setup_foundation_count() -> usize {
    if cfg!(feature = "native-c-core") {
        0
    } else {
        1
    }
}

fn expected_setup_probability() -> &'static str {
    "0.0"
}

fn expected_setup_coverage_count() -> usize {
    0
}
