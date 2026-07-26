use clearra_setup_search::query::{SetupCycleResetBorrowPolicy, SetupSearchMode, SetupSearchQuery};

use crate::diagnostic::diagnostic_report::DiagnosticReport;

use super::{
    board_validator::validate_board_size,
    pc_query_validator::validate_pc_target,
    piece_set_validator::validate_piece_budget,
    setup_hold_policy_validator::validate_hold_policy,
    setup_limits_validator::validate_limits,
    setup_probability_filter_validator::validate_probability_filter,
    setup_query_diagnostic_builder::{invalid_setup_query, setup_supported_diagnostic},
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SetupQueryValidator;

impl SetupQueryValidator {
    pub fn validate(query: &SetupSearchQuery) -> DiagnosticReport {
        let mut report = DiagnosticReport::new();
        report.append(validate_board_size(query.board_size()));
        report.append(validate_pc_target(query.target()));
        validate_fixed_setup_target(query, &mut report);
        report.append(validate_piece_budget(query.piece_budget()));
        validate_residue(query, &mut report);
        if query.search_mode() == SetupSearchMode::QueueBased {
            validate_queue_based_input(query, &mut report);
        }
        validate_hold_policy(query.hold_policy(), &mut report);
        validate_probability_filter(query.probability_filter(), &mut report);
        validate_limits(query.limits(), &mut report);
        if !report.has_errors() {
            report.push(setup_supported_diagnostic(query));
        }
        report
    }
}

fn validate_fixed_setup_target(query: &SetupSearchQuery, report: &mut DiagnosticReport) {
    if query.board_size().width() != 10
        || query.board_size().height() != 4
        || query.target().lines() != 4
    {
        report.push(invalid_setup_query(
            "setup.target",
            "setup finder requires the fixed empty 10x4 perfect-clear target",
            "setup_finder_target_is_not_fixed_empty_10x4",
        ));
    }
}

fn validate_residue(query: &SetupSearchQuery, report: &mut DiagnosticReport) {
    let residue = query.residue();
    if residue.cycle().is_none() {
        report.push(invalid_setup_query(
            "setup.remaining_pieces",
            "remaining setup pieces must identify one of the seven PC cycles",
            "remaining_piece_count_does_not_map_to_pc_cycle",
        ));
        return;
    }

    let duplicate_kinds = clearra_core_domain::piece::piece_kind::PieceKind::STANDARD_TETROMINOES
        .into_iter()
        .filter(|piece| {
            residue
                .pieces()
                .iter()
                .filter(|value| **value == *piece)
                .count()
                > 1
        })
        .collect::<Vec<_>>();
    if duplicate_kinds.len() > 1
        || residue.pieces().iter().any(|piece| {
            residue
                .pieces()
                .iter()
                .filter(|value| *value == piece)
                .count()
                > 2
        })
    {
        report.push(invalid_setup_query(
            "setup.remaining_pieces",
            "remaining setup pieces may contain at most one duplicated kind and at most two copies",
            "remaining_piece_multiset_cannot_represent_one_hold_slot",
        ));
    }

    if query.cycle_reset_borrow_policy() == SetupCycleResetBorrowPolicy::AllowPostCyclePieceUse
        && residue.cycle() != Some(7)
    {
        report.push(invalid_setup_query(
            "setup.cycle_reset_borrow",
            "post-cycle piece use is only meaningful for the seventh PC cycle",
            "post_cycle_borrow_requested_outside_cycle_seven",
        ));
    }
}

fn validate_queue_based_input(query: &SetupSearchQuery, report: &mut DiagnosticReport) {
    let Some(queue) = query.queue().as_fixed_sequence() else {
        report.push(invalid_setup_query(
            "setup.queue",
            "queue-based setup search requires an observed subset of the following standard bag",
            "queue_based_setup_requires_fixed_queue",
        ));
        return;
    };
    if queue.is_empty() {
        report.push(invalid_setup_query(
            "setup.queue",
            "queue-based setup search requires at least one observed next-bag piece",
            "queue_based_setup_piece_count_out_of_range",
        ));
    }
    if queue.len() + query.residue().remaining_count() > 7 {
        report.push(invalid_setup_query(
            "setup.queue",
            "remaining setup pieces and observed next-bag pieces may contain at most seven pieces in total",
            "queue_based_setup_combined_piece_count_out_of_range",
        ));
    }
    if clearra_core_domain::piece::piece_kind::PieceKind::STANDARD_TETROMINOES
        .into_iter()
        .any(|piece| {
            queue
                .pieces()
                .iter()
                .filter(|value| **value == piece)
                .count()
                > 1
        })
    {
        report.push(invalid_setup_query(
            "setup.queue",
            "observed queue-based pieces must be a subset of one standard seven-bag",
            "queue_based_setup_observed_piece_duplicate",
        ));
    }
}

pub fn validate_setup_search_query(query: &SetupSearchQuery) -> DiagnosticReport {
    SetupQueryValidator::validate(query)
}

#[cfg(test)]
#[path = "setup_query_validator_tests.rs"]
mod tests;
