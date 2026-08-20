use crate::{args::PcArgs, exit::ExitCode};

use super::*;

#[test]
fn path_command_renders_retained_solution_trace_steps() {
    let output = PathCommand::run(
        &PathArgs::new(
            PcArgs::new(2)
                .with_queue("IIOOO", true)
                .with_hold_enabled(false),
        ),
        RenderFormat::TextVerbose,
    );

    assert_eq!(output.exit_code(), ExitCode::Success);
    assert!(output.stdout().contains("kind: path"));
    assert!(output.stdout().contains("status: path-rendered"));
    assert!(output
        .stdout()
        .contains("retained_representative_trace: true"));
    assert!(output.stdout().contains(&format!(
        "total_solution_count: {}",
        expected_path_solution_count()
    )));
    assert!(output.stdout().contains(&format!(
        "retained_trace_count: {}",
        expected_path_retained_trace_count()
    )));
    assert!(output.stdout().contains("score_post_processing: false"));
    assert!(output.stdout().contains("score_accuracy_level: none"));
    assert!(output.stdout().contains("trace_steps:"));
}

#[test]
fn path_command_reports_trace_requirement_when_no_solution_trace_exists() {
    let output = PathCommand::run(
        &PathArgs::new(
            PcArgs::new(2)
                .with_queue("TTTTT", true)
                .with_hold_enabled(false),
        ),
        RenderFormat::Text,
    );

    assert_eq!(output.exit_code(), ExitCode::ValidationFailed);
    assert!(output
        .stderr()
        .contains(CliErrorCode::PathNoSolution.as_str()));
    assert!(output.stderr().contains("sample_trace_available=false"));
}

fn expected_path_solution_count() -> usize {
    if cfg!(feature = "native-c-core") {
        4
    } else {
        1
    }
}

fn expected_path_retained_trace_count() -> usize {
    if cfg!(feature = "native-c-core") {
        4
    } else {
        1
    }
}
