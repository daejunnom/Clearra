use clearra_core_domain::board::board_size::BoardSize;
use clearra_pc_graph::request::{PcCompletionGoal, PcQueueInput, PcScenarioQuery};
use clearra_rules::profile::rule_capability::RuleCapability;

use crate::{
    diagnostic::{
        diagnostic::Diagnostic, diagnostic_code::DiagnosticCode,
        diagnostic_report::DiagnosticReport, suggested_next_step::SuggestedNextStep,
    },
    evidence::{evidence_location::EvidenceLocation, validation_evidence::ValidationEvidence},
    validators::{
        objective_validator::validate_objective_kind,
        pc_execution_policy_validator::validate_pc_execution_policy,
        pc_execution_policy_validator::PcBackendCompatibilityContext,
        pc_query_validator::validate_observation_objective_contract,
        piece_set_validator::validate_piece_set_profile,
        rule_validator::validate_rule_profile_with_verified_kick_profile,
        supply_validator::{
            validate_bag_aligned_pattern, validate_fixed_sequence, validate_observed_queue,
        },
    },
};

pub fn validate_pc_scenario_query(query: &PcScenarioQuery) -> DiagnosticReport {
    let mut report = DiagnosticReport::new();
    validate_scenario_board(query, &mut report);
    validate_scenario_queue(query, &mut report);
    validate_scenario_piece_window(query, &mut report);
    validate_scenario_constraints(query, &mut report);
    validate_scenario_goal(query, &mut report);
    report.append(validate_piece_set_profile(query.piece_set()));
    report.append(validate_rule_profile_with_verified_kick_profile(
        query.rule(),
        query.verified_kick_profile(),
    ));
    report.append(validate_objective_kind(query.objective().kind()));
    report.append(validate_observation_objective_contract(
        query.queue_observation_policy(),
        query.objective().kind(),
    ));
    report.append(validate_pc_execution_policy(
        query.execution_policy(),
        PcBackendCompatibilityContext::scenario(query.count_policy()),
        "pc.scenario.execution",
    ));
    validate_scenario_bag_contract(query, &mut report);

    if !report.has_errors() {
        report.push(scenario_supported_diagnostic(query));
    }

    report
}

fn validate_scenario_board(query: &PcScenarioQuery, report: &mut DiagnosticReport) {
    let board = query.initial_board();
    match BoardSize::new(board.width(), board.visible_height()) {
        Ok(size) if size.area() <= 64 => {
            let layout_mask = if size.area() == 64 {
                u64::MAX
            } else {
                (1_u64 << size.area()) - 1
            };
            if board.occupied_mask() & !layout_mask != 0 {
                report.push(invalid_pc_query(
                    "pc.scenario.initial_board",
                    "PC scenario occupied mask must fit inside the scenario board",
                    "scenario_board_mask_outside_layout",
                ));
            }
        }
        _ => report.push(invalid_pc_query(
            "pc.scenario.initial_board",
            "PC scenario board must fit in the MVP1 Board64 layout",
            "scenario_board_unsupported",
        )),
    }
}

fn validate_scenario_queue(query: &PcScenarioQuery, report: &mut DiagnosticReport) {
    match query.remaining_queue() {
        PcQueueInput::FixedSequence(sequence) => report.append(validate_fixed_sequence(sequence)),
        PcQueueInput::BagAlignedPattern(pattern) => {
            report.append(validate_bag_aligned_pattern(pattern))
        }
        PcQueueInput::PatternExpression(_) => {}
        PcQueueInput::Standard7Bag => {}
        PcQueueInput::Observed(queue) => report.append(validate_observed_queue(queue)),
    }
}

fn validate_scenario_piece_window(query: &PcScenarioQuery, report: &mut DiagnosticReport) {
    let max_pieces = query.piece_window().max_pieces();
    if max_pieces == 0 {
        report.push(invalid_pc_query(
            "pc.scenario.piece_window",
            "PC scenario piece window must include at least one piece",
            "scenario_piece_window_empty",
        ));
    }
    if matches!(
        query.remaining_queue(),
        PcQueueInput::FixedSequence(_)
            | PcQueueInput::BagAlignedPattern(_)
            | PcQueueInput::PatternExpression(_)
    ) && max_pieces > query.remaining_queue().len()
    {
        report.push(invalid_pc_query(
            "pc.scenario.piece_window",
            "PC scenario piece window cannot exceed the remaining queue length",
            "scenario_piece_window_exceeds_queue",
        ));
    }
}

fn validate_scenario_constraints(query: &PcScenarioQuery, report: &mut DiagnosticReport) {
    if let Some(exact_pieces) = query.exact_pieces() {
        if exact_pieces > query.piece_window().max_pieces() {
            report.push(invalid_pc_query(
                "pc.scenario.exact_pieces",
                "PC scenario exact piece count cannot exceed the piece window",
                "scenario_exact_pieces_exceeds_window",
            ));
        }
    }

    if matches!(
        query.remaining_queue(),
        PcQueueInput::FixedSequence(_)
            | PcQueueInput::BagAlignedPattern(_)
            | PcQueueInput::PatternExpression(_)
    ) && query.min_remaining_queue() > query.remaining_queue().len()
    {
        report.push(invalid_pc_query(
            "pc.scenario.min_remaining_queue",
            "PC scenario minimum remaining queue cannot exceed the remaining queue length",
            "scenario_min_remaining_queue_exceeds_queue",
        ));
    }

    if !query.allow_hold() && query.hold_state().piece().is_some() {
        report.push(invalid_pc_query(
            "pc.scenario.allow_hold",
            "PC scenario cannot start with a held piece when hold is disabled",
            "scenario_hold_piece_with_hold_disabled",
        ));
    }

    let supports_180 = query.verified_kick_profile().map_or_else(
        || RuleCapability::from_rule(query.rule()).supports_180(),
        |profile| profile.profile().supports_180(),
    );
    if query.requires_180() && !supports_180 {
        report.push(invalid_pc_query(
            "pc.scenario.requires_180",
            "PC scenario requires 180 rotation but the selected rule profile does not support 180 kicks",
            "scenario_requires_180_unsupported",
        ));
    }
}

fn validate_scenario_goal(query: &PcScenarioQuery, report: &mut DiagnosticReport) {
    match query.completion_goal() {
        PcCompletionGoal::ClearToEmpty => {}
    }
    if query.initial_board().visible_height() == 0 {
        report.push(invalid_pc_query(
            "pc.scenario.completion_goal",
            "PC scenario completion requires a non-empty visible board height",
            "scenario_empty_completion_height",
        ));
    }
}

fn validate_scenario_bag_contract(query: &PcScenarioQuery, report: &mut DiagnosticReport) {
    if query.bag().piece_set_id() != query.piece_set().id() {
        report.push(invalid_pc_query(
            "pc.scenario.bag",
            "PC scenario bag profile must use the selected piece set profile",
            "scenario_bag_piece_set_mismatch",
        ));
    }
}

fn scenario_supported_diagnostic(query: &PcScenarioQuery) -> Diagnostic {
    Diagnostic::new(
        DiagnosticCode::IPcQueryMvpSupported,
        "PC scenario query is valid for MVP1 setup completion search",
    )
    .with_location(EvidenceLocation::new("pc.scenario"))
    .with_evidence(ValidationEvidence::new("query_kind", "scenario"))
    .with_evidence(ValidationEvidence::new(
        "board_width",
        query.initial_board().width().to_string(),
    ))
    .with_evidence(ValidationEvidence::new(
        "board_height",
        query.initial_board().visible_height().to_string(),
    ))
    .with_evidence(ValidationEvidence::new(
        "piece_window",
        query.piece_window().max_pieces().to_string(),
    ))
    .with_evidence(ValidationEvidence::new(
        "completion_goal",
        query.completion_goal().as_str(),
    ))
    .with_evidence(ValidationEvidence::new(
        "verified_kick_profile",
        query.verified_kick_profile().is_some().to_string(),
    ))
    .with_evidence(ValidationEvidence::new(
        "requested_backend",
        query.execution_policy().requested_backend().as_str(),
    ))
    .with_evidence(ValidationEvidence::new(
        "execution_workers",
        query.execution_policy().workers().to_string(),
    ))
}

fn invalid_pc_query(
    location: &'static str,
    message: &'static str,
    reason: &'static str,
) -> Diagnostic {
    Diagnostic::new(DiagnosticCode::EPcQueryInvalid, message)
        .with_location(EvidenceLocation::new(location))
        .with_evidence(ValidationEvidence::new("reason", reason))
        .with_suggested_next_step(SuggestedNextStep::new(
            "Use the standard MVP1 PC defaults: standard 10 board, standard pieces, and standard 7-bag.",
        ))
}
