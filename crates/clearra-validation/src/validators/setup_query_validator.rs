use clearra_setup_search::query::{SetupQueueInput, SetupSearchQuery};

use crate::diagnostic::diagnostic_report::DiagnosticReport;

use super::{
    board_validator::validate_board_size,
    pc_query_validator::validate_pc_target,
    piece_set_validator::validate_piece_budget,
    setup_hold_policy_validator::validate_hold_policy,
    setup_limits_validator::validate_limits,
    setup_probability_filter_validator::validate_probability_filter,
    setup_query_diagnostic_builder::setup_supported_diagnostic,
    supply_validator::{
        validate_bag_aligned_pattern, validate_fixed_sequence, validate_observed_queue,
    },
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SetupQueryValidator;

impl SetupQueryValidator {
    pub fn validate(query: &SetupSearchQuery) -> DiagnosticReport {
        let mut report = DiagnosticReport::new();
        report.append(validate_board_size(query.board_size()));
        report.append(validate_pc_target(query.target()));
        report.append(validate_piece_budget(query.piece_budget()));
        report.append(match query.queue() {
            SetupQueueInput::FixedSequence(sequence) => validate_fixed_sequence(sequence),
            SetupQueueInput::BagAlignedPattern(pattern) => validate_bag_aligned_pattern(pattern),
            SetupQueueInput::Observed(queue) => validate_observed_queue(queue),
        });
        validate_hold_policy(query.hold_policy(), &mut report);
        validate_probability_filter(query.probability_filter(), &mut report);
        validate_limits(query.limits(), &mut report);
        if !report.has_errors() {
            report.push(setup_supported_diagnostic(query));
        }
        report
    }
}

pub fn validate_setup_search_query(query: &SetupSearchQuery) -> DiagnosticReport {
    SetupQueryValidator::validate(query)
}

#[cfg(test)]
#[path = "setup_query_validator_tests.rs"]
mod tests;
