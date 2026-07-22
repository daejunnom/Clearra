use crate::args::PercentQueueMode;
use crate::exit::ExitCode;

use super::*;

#[test]
fn percent_command_expands_observed_queue_probability() {
    let output = PercentCommand::run(
        &PercentArgs::new("IOT")
            .with_mode(PercentQueueMode::Observed)
            .with_minimum_len(Some(5))
            .with_max_patterns(16),
        RenderFormat::Text,
    );

    assert_eq!(output.exit_code(), ExitCode::Success);
    assert!(output.stdout().contains("kind: percent"));
    assert!(output.stdout().contains("queue_mode: observed"));
    assert!(output.stdout().contains("materialized_probability_mass:"));
}

#[test]
fn percent_command_reports_bag_aligned_pattern_as_certain_single_pattern() {
    let output = PercentCommand::run(
        &PercentArgs::new("IOT").with_mode(PercentQueueMode::BagAligned),
        RenderFormat::Text,
    );

    assert_eq!(output.exit_code(), ExitCode::Success);
    assert!(output.stdout().contains("queue_mode: bag-aligned"));
    assert!(output.stdout().contains("materialized_probability_mass: 1"));
}
