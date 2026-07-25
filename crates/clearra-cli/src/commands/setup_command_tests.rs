use crate::{error::CliErrorCode, exit::ExitCode};

use super::*;

#[test]
#[ignore = "full empty-4L exact setup acceptance runs in the release exact suite"]
fn setup_command_returns_exact_setup_finder_report() {
    let output = SetupCommand::run(&SetupArgs::new("IOTSZJL", false), RenderFormat::TextVerbose);

    assert_eq!(output.exit_code(), ExitCode::Success);
    assert!(output.stdout().contains("status: setup-finder-complete"));
    assert!(output
        .stdout()
        .contains("backend_selected: wasm-cpu-setup-family-quotient"));
    assert!(output.stdout().contains("coverage_semantics: oracle"));
    assert!(output.stdout().contains("cycle: 1"));
    assert!(output.stdout().contains("remaining_pieces: IOTSZJL"));
    assert!(output.stdout().contains("post_cycle_borrow_enabled: false"));
    assert!(output.stdout().contains("geometry_family_count:"));
    assert!(output.stdout().contains("partial_build_node_count:"));
    assert!(output.stdout().contains("hold_conditions:"));
}

#[test]
fn setup_command_reports_bad_queue_piece() {
    let output = SetupCommand::run(&SetupArgs::new("IX", false), RenderFormat::Text);

    assert_eq!(output.exit_code(), ExitCode::ValidationFailed);
    assert!(output
        .stderr()
        .contains(CliErrorCode::SetupQueryInvalid.as_str()));
}
