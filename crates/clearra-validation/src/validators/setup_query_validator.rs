use clearra_setup_search::query::{
    SetupCycleResetBorrowPolicy, SetupHoldPolicy, SetupSearchMode, SetupSearchQuery,
};

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
        validate_max_setup_pieces(query, &mut report);
        if query.search_mode() == SetupSearchMode::QueueBased {
            validate_queue_based_input(query, &mut report);
        }
        validate_next_cycle_remaining_input(query, &mut report);
        validate_hold_policy(query.hold_policy(), &mut report);
        validate_probability_filter(query.probability_filter(), &mut report);
        validate_limits(query.limits(), &mut report);
        if !report.has_errors() {
            report.push(setup_supported_diagnostic(query));
        }
        report
    }
}

fn validate_max_setup_pieces(query: &SetupSearchQuery, report: &mut DiagnosticReport) {
    if !(1..=10).contains(&query.max_setup_pieces()) {
        report.push(invalid_setup_query(
            "setup.max_setup_pieces",
            "maximum setup pieces must be between one and ten",
            "setup_max_piece_count_out_of_range",
        ));
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

    if !residue.has_valid_piece_multiplicity() {
        report.push(invalid_setup_query(
            "setup.remaining_pieces",
            "at most one piece kind may occur twice; no piece kind may occur more than twice",
            "remaining_piece_duplicate_inventory_invalid",
        ));
        return;
    }

    let automatic_initial_hold = residue.duplicate_piece();
    let initial_hold = match query.hold_policy() {
        SetupHoldPolicy::EnabledEmpty => automatic_initial_hold,
        SetupHoldPolicy::EnabledWithPiece(piece) => Some(piece),
        SetupHoldPolicy::Disabled => None,
    };
    let mut queue_remainder = residue.pieces().to_vec();
    if let Some(piece) = initial_hold {
        let Some(index) = queue_remainder.iter().position(|value| *value == piece) else {
            report.push(invalid_setup_query(
                "setup.hold_policy",
                "the selected initial hold must be included in the remaining-piece inventory",
                "initial_hold_piece_missing_from_remaining_inventory",
            ));
            return;
        };
        queue_remainder.remove(index);
    }
    if clearra_core_domain::piece::piece_kind::PieceKind::STANDARD_TETROMINOES
        .into_iter()
        .any(|piece| {
            queue_remainder
                .iter()
                .filter(|value| **value == piece)
                .count()
                > 1
        })
    {
        report.push(invalid_setup_query(
            "setup.remaining_pieces",
            "after the automatic initial-hold piece is separated, queue-remainder pieces must be unique",
            "remaining_piece_duplicate_inventory_invalid",
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
            "queue-based setup search requires observed next-bag pieces",
            "queue_based_setup_requires_fixed_queue",
        ));
        return;
    };
    if queue.is_empty() {
        report.push(invalid_setup_query(
            "setup.queue",
            "queue-based setup search requires at least one observed next-bag piece",
            "queue_based_setup_requires_observed_piece",
        ));
    }
    if queue.len() + query.residue().remaining_count() > 7 {
        report.push(invalid_setup_query(
            "setup.queue",
            "observed queue-based pieces and remaining pieces must fit in one bag",
            "queue_based_setup_observed_piece_count_out_of_range",
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
            "observed queue-based pieces must be distinct",
            "queue_based_setup_observed_piece_duplicate",
        ));
    }
}

fn validate_next_cycle_remaining_input(query: &SetupSearchQuery, report: &mut DiagnosticReport) {
    let Some(pieces) = query.next_cycle_remaining_pieces() else {
        return;
    };
    let expected_count = match query.residue().cycle() {
        Some(1) => 4,
        Some(2) => 1,
        Some(3) => 5,
        Some(4) => 2,
        Some(5) => 6,
        Some(6) => 3,
        Some(7) => 7,
        _ => return,
    };
    if pieces.len() != expected_count {
        report.push(invalid_setup_query(
            "setup.next_cycle_remaining_pieces",
            "next-cycle remaining inventory must match the cycle reached after this PC",
            "next_cycle_remaining_piece_count_out_of_range",
        ));
    }
    let repeated_counts = clearra_core_domain::piece::piece_kind::PieceKind::STANDARD_TETROMINOES
        .into_iter()
        .map(|piece| pieces.iter().filter(|value| **value == piece).count())
        .collect::<Vec<_>>();
    if repeated_counts.iter().any(|count| *count > 2)
        || repeated_counts.iter().filter(|count| **count == 2).count() > 1
    {
        report.push(invalid_setup_query(
            "setup.next_cycle_remaining_pieces",
            "only one next-cycle piece kind may repeat through hold carryover",
            "next_cycle_remaining_piece_duplicate",
        ));
    }
}

pub fn validate_setup_search_query(query: &SetupSearchQuery) -> DiagnosticReport {
    SetupQueryValidator::validate(query)
}

#[cfg(test)]
#[path = "setup_query_validator_tests.rs"]
mod tests;
