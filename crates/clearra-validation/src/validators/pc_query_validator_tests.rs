use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_objectives::policy::objective_policy::ObjectivePolicy;
use clearra_pc_graph::request::{
    OpeningPcSearchQuery, PcExecutionPolicy, PcHoldPolicy, PcQueueInput, PcScenarioBoard,
    PieceWindow, RequestedSearchBackend, VISIBLE_SEVEN_MINIMUM_COVER_ERROR_CODE,
};
use clearra_rules::{
    kicks::{KickTableProfile, KickTableProfileId, SrsKicks, VerifiedKickTableProfile},
    profile::rule_profile::RuleProfileId,
};
use clearra_supply::queue::{
    bag_aligned_pattern::BagAlignedPattern, fixed_sequence::FixedSequence,
    queue_observation_policy::QueueObservationPolicy,
};

use crate::diagnostic::diagnostic_code::DiagnosticCode;

use super::*;

#[test]
fn two_four_six_line_targets_are_supported() {
    for target in [
        PcTarget::two_lines(),
        PcTarget::four_lines(),
        PcTarget::six_lines(),
    ] {
        let report = validate_pc_target(target);

        assert!(!report.has_errors());
        assert!(report.contains_code(DiagnosticCode::IPcTargetMvpSupported));
    }
}

#[test]
fn eight_line_target_is_outside_mvp() {
    let target = PcTarget::new(8).expect("valid target but outside MVP");
    let report = validate_pc_target(target);

    assert!(report.has_errors());
    assert!(report.contains_code(DiagnosticCode::EPcTargetUnsupportedMvp));
}

#[test]
fn validates_opening_pc_query_contract() {
    let query = OpeningPcSearchQuery::new(PcTarget::two_lines())
        .with_queue(PcQueueInput::fixed_sequence(FixedSequence::new(vec![
            PieceKind::I,
            PieceKind::O,
            PieceKind::T,
            PieceKind::S,
            PieceKind::Z,
            PieceKind::J,
            PieceKind::L,
        ])))
        .with_hold_policy(PcHoldPolicy::Disabled);

    let report = validate_opening_pc_search_query(&query);

    assert!(!report.has_errors());
    assert!(report.contains_code(DiagnosticCode::IPcQueryMvpSupported));
    assert!(report.contains_code(DiagnosticCode::ISupplyFixedSequenceAccepted));
}

#[test]
fn opening_pc_query_validation_rejects_bag_aligned_pattern_contract_errors() {
    let query = OpeningPcSearchQuery::new(PcTarget::two_lines()).with_queue(
        PcQueueInput::bag_aligned_pattern(BagAlignedPattern::new(vec![
            PieceKind::I,
            PieceKind::O,
            PieceKind::I,
        ])),
    );

    let report = validate_opening_pc_search_query(&query);

    assert!(report.has_errors());
    assert!(report.contains_code(DiagnosticCode::ESupplyInvalidDuplicate));
}

#[test]
fn opening_and_scenario_validation_reject_visible_seven_minimum_cover() {
    let opening = OpeningPcSearchQuery::new(PcTarget::four_lines())
        .with_queue_observation_policy(QueueObservationPolicy::VisibleSeven)
        .with_objective(ObjectivePolicy::minimum_cover());
    let scenario = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(4, 0),
        PcQueueInput::default(),
        PieceWindow::new(10),
    )
    .with_queue_observation_policy(QueueObservationPolicy::VisibleSeven)
    .with_objective(ObjectivePolicy::minimum_cover());

    for report in [
        validate_opening_pc_search_query(&opening),
        validate_pc_scenario_query(&scenario),
    ] {
        assert!(report.has_errors());
        assert!(report.contains_code(DiagnosticCode::EPcQueryInvalid));
        assert!(report.diagnostics().iter().any(|diagnostic| diagnostic
            .evidence()
            .iter()
            .any(|evidence| evidence.key() == "reason"
                && evidence.value() == VISIBLE_SEVEN_MINIMUM_COVER_ERROR_CODE)));
    }
}

#[test]
fn validates_pc_scenario_query_without_target_lines() {
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(2, 0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![
            PieceKind::I,
            PieceKind::O,
            PieceKind::T,
        ])),
        PieceWindow::new(3),
    );

    let report = validate_pc_scenario_query(&query);

    assert!(!report.has_errors());
    assert!(report.contains_code(DiagnosticCode::IPcQueryMvpSupported));
}

#[test]
fn pc_scenario_fixed_sequence_allows_duplicates_without_bag_offset_zero_contract() {
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(2, 0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![
            PieceKind::I,
            PieceKind::O,
            PieceKind::I,
        ])),
        PieceWindow::new(3),
    );

    let report = validate_pc_scenario_query(&query);

    assert!(!report.has_errors());
    assert!(report.contains_code(DiagnosticCode::ISupplyFixedSequenceAccepted));
    assert!(!report.contains_code(DiagnosticCode::ESupplyInvalidDuplicate));
}

#[test]
fn pc_scenario_query_accepts_observed_queue_with_future_bag_materialization() {
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(2, 0),
        PcQueueInput::observed(clearra_supply::queue::observed_queue::ObservedQueue::new(
            vec![PieceKind::I],
        )),
        PieceWindow::new(2),
    );

    let report = validate_pc_scenario_query(&query);

    assert!(!report.has_errors());
    assert!(report.contains_code(DiagnosticCode::WSupplyAmbiguousObservedWindow));
}

#[test]
fn pc_scenario_query_rejects_invalid_completion_constraints() {
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(4, 0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![
            PieceKind::I,
            PieceKind::O,
            PieceKind::S,
        ])),
        PieceWindow::new(3),
    )
    .with_hold_piece(Some(PieceKind::T))
    .with_exact_pieces(Some(4))
    .with_min_remaining_queue(4)
    .with_allow_hold(false)
    .with_requires_180(true);

    let report = validate_pc_scenario_query(&query);

    assert!(report.has_errors());
    assert!(report.contains_code(DiagnosticCode::EPcQueryInvalid));
}

#[test]
fn opening_pc_query_accepts_verified_imported_kick_profile_override() {
    let verified = verified_srs_x_profile();
    let query =
        OpeningPcSearchQuery::new(PcTarget::two_lines()).with_verified_kick_table_profile(verified);

    let report = validate_opening_pc_search_query(&query);

    assert!(!report.has_errors());
    assert!(report.contains_code(DiagnosticCode::IRuleMvpSupported));
    assert!(report.diagnostics().iter().any(|diagnostic| diagnostic
        .evidence()
        .iter()
        .any(|evidence| evidence.key() == "verified_profile" && evidence.value() == "true")));
}

#[test]
fn pc_scenario_query_accepts_verified_imported_kick_profile_override() {
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(2, 0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
        PieceWindow::new(1),
    )
    .with_verified_kick_table_profile(verified_srs_x_profile());

    let report = validate_pc_scenario_query(&query);

    assert!(!report.has_errors());
    assert!(report.contains_code(DiagnosticCode::IPcQueryMvpSupported));
}

#[test]
fn pc_query_accepts_user_facing_cpu_backend_without_fallback() {
    let query = OpeningPcSearchQuery::new(PcTarget::two_lines())
        .with_objective(ObjectivePolicy::minimum_cover())
        .with_execution_policy(
            PcExecutionPolicy::mvp_default()
                .with_requested_backend(RequestedSearchBackend::Cpu)
                .with_allow_backend_fallback(true),
        );

    let report = validate_opening_pc_search_query(&query);

    assert!(!report.has_errors());
    assert!(!report.contains_code(DiagnosticCode::WPcBackendFallback));
    assert!(report.contains_code(DiagnosticCode::IPcQueryMvpSupported));
}

#[test]
fn pc_scenario_query_defers_gpu_capability_to_runtime_selector() {
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(2, 0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
        PieceWindow::new(1),
    )
    .with_execution_policy(
        PcExecutionPolicy::mvp_default()
            .with_requested_backend(RequestedSearchBackend::Gpu)
            .with_allow_backend_fallback(false),
    );

    let report = validate_pc_scenario_query(&query);

    assert!(!report.has_errors());
    assert!(!report.contains_code(DiagnosticCode::EBackendGpuFeatureDisabled));
    assert!(report.contains_code(DiagnosticCode::IPcQueryMvpSupported));
}

fn verified_srs_x_profile() -> VerifiedKickTableProfile {
    VerifiedKickTableProfile::try_new(KickTableProfile::new(
        KickTableProfileId::Imported,
        RuleProfileId::SrsX,
        SrsKicks::srs_plus_profile().entries().to_vec(),
    ))
    .expect("verified imported SRS-X profile with exact 180 transitions")
}
