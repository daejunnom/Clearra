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
    // Four exact solved fields; the result retains one representative trace.
    assert!(output.stdout().contains("total_solution_count: 4"));
    assert!(output.stdout().contains("retained_trace_count: 1"));
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
