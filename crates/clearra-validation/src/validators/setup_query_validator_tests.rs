use clearra_core_domain::{
    board::board_size::BoardSize, pc::pc_target::PcTarget, piece::piece_kind::PieceKind,
};
use clearra_setup_search::query::{
    SetupHoldPolicy, SetupLimits, SetupProbabilityFilter, SetupQueueInput, SetupSearchQuery,
};
use clearra_supply::queue::{
    bag_aligned_pattern::BagAlignedPattern, fixed_sequence::FixedSequence,
};

use crate::diagnostic::diagnostic_code::{DiagnosticCode, DiagnosticSeverity};

use super::SetupQueryValidator;

#[test]
fn setup_query_validator_accepts_fixed_sequence_duplicates() {
    let query = SetupSearchQuery::default().with_queue(SetupQueueInput::fixed_sequence(
        FixedSequence::new(vec![PieceKind::I, PieceKind::O, PieceKind::I]),
    ));

    let report = SetupQueryValidator::validate(&query);

    assert!(!report.has_errors());
    assert!(report.contains_code(DiagnosticCode::ISupplyFixedSequenceAccepted));
    assert!(!report.contains_code(DiagnosticCode::ESupplyInvalidDuplicate));
}

#[test]
fn setup_query_validator_rejects_bag_aligned_pattern_duplicates() {
    let query = SetupSearchQuery::default().with_queue(SetupQueueInput::bag_aligned_pattern(
        BagAlignedPattern::new(vec![PieceKind::I, PieceKind::O, PieceKind::I]),
    ));

    let report = SetupQueryValidator::validate(&query);

    assert!(report.has_errors());
    assert!(report.diagnostics().iter().any(|diagnostic| {
        diagnostic.code() == DiagnosticCode::ESupplyInvalidDuplicate
            && diagnostic.severity() == DiagnosticSeverity::Error
    }));
}

#[test]
fn setup_query_validator_rejects_outside_mvp_target_and_board() {
    let query = SetupSearchQuery::default()
        .with_queue(SetupQueueInput::fixed_sequence(FixedSequence::new(vec![
            PieceKind::I,
            PieceKind::O,
            PieceKind::T,
            PieceKind::S,
            PieceKind::Z,
            PieceKind::J,
            PieceKind::L,
        ])))
        .with_limits(Default::default());
    let query = SetupSearchQuery::new(
        BoardSize::new(12, 20).expect("board"),
        PcTarget::new(8).expect("target"),
        query.queue().clone(),
        query.hold_policy(),
        query.piece_budget().clone(),
        query.probability_filter(),
        query.grouping_mode(),
        query.limits(),
    );

    let report = SetupQueryValidator::validate(&query);

    assert!(report.has_errors());
    assert!(report.contains_code(DiagnosticCode::EBoardUnsupportedMvp));
    assert!(report.contains_code(DiagnosticCode::EPcTargetUnsupportedMvp));
}

#[test]
fn setup_query_validator_checks_hold_policy_probability_filter_and_limits() {
    let query = SetupSearchQuery::default()
        .with_hold_policy(SetupHoldPolicy::Disabled)
        .with_probability_filter(SetupProbabilityFilter::default())
        .with_limits(SetupLimits::new(4097, 1024, 1024, 256, 4096, 65).expect("limits"));

    let report = SetupQueryValidator::validate(&query);

    assert!(report.has_errors());
    assert!(report.contains_code(DiagnosticCode::ESetupQueryInvalid));
}

#[test]
fn valid_setup_query_reports_supported_mvp_contract() {
    let query = SetupSearchQuery::default();

    let report = SetupQueryValidator::validate(&query);

    assert!(!report.has_errors());
    assert!(report.contains_code(DiagnosticCode::ISetupQueryMvpSupported));
}
