use clearra_core_domain::{
    board::board_size::BoardSize, pc::pc_target::PcTarget, piece::piece_kind::PieceKind,
};
use clearra_setup_search::query::{
    SetupHoldPolicy, SetupLimits, SetupProbabilityFilter, SetupSearchQuery,
};

use crate::diagnostic::diagnostic_code::{DiagnosticCode, DiagnosticSeverity};

use super::SetupQueryValidator;

#[test]
fn setup_query_validator_accepts_one_explicit_hold_duplicate() {
    let query = SetupSearchQuery::default().with_remaining_pieces(vec![
        PieceKind::I,
        PieceKind::I,
        PieceKind::O,
        PieceKind::T,
        PieceKind::S,
        PieceKind::Z,
        PieceKind::J,
    ]);

    let report = SetupQueryValidator::validate(&query);

    assert!(!report.has_errors());
    assert!(report.contains_code(DiagnosticCode::ISetupQueryMvpSupported));
}

#[test]
fn setup_query_validator_rejects_multiple_explicit_hold_duplicates() {
    let query = SetupSearchQuery::default().with_remaining_pieces(vec![
        PieceKind::I,
        PieceKind::I,
        PieceKind::O,
        PieceKind::O,
        PieceKind::T,
        PieceKind::S,
        PieceKind::Z,
    ]);

    let report = SetupQueryValidator::validate(&query);

    assert!(report.has_errors());
    assert!(report.diagnostics().iter().any(|diagnostic| {
        diagnostic.code() == DiagnosticCode::ESetupQueryInvalid
            && diagnostic.severity() == DiagnosticSeverity::Error
    }));
}

#[test]
fn setup_query_validator_rejects_outside_mvp_target_and_board() {
    let query = SetupSearchQuery::default();
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

#[test]
fn queue_based_setup_accepts_observed_next_bag_subset() {
    let query = SetupSearchQuery::default()
        .with_remaining_pieces(vec![PieceKind::I, PieceKind::O, PieceKind::T, PieceKind::S])
        .with_queue_based_pieces(vec![PieceKind::Z, PieceKind::J, PieceKind::L]);

    let report = SetupQueryValidator::validate(&query);

    assert!(!report.has_errors());
}

#[test]
fn queue_based_setup_rejects_more_than_seven_combined_observations() {
    let query = SetupSearchQuery::default()
        .with_remaining_pieces(vec![PieceKind::I, PieceKind::O, PieceKind::T, PieceKind::S])
        .with_queue_based_pieces(vec![PieceKind::Z, PieceKind::J, PieceKind::L, PieceKind::O]);

    let report = SetupQueryValidator::validate(&query);

    assert!(report.has_errors());
}

#[test]
fn queue_based_setup_rejects_duplicate_observed_next_bag_piece() {
    let query = SetupSearchQuery::default()
        .with_remaining_pieces(vec![PieceKind::T, PieceKind::I])
        .with_queue_based_pieces(vec![PieceKind::O, PieceKind::O]);

    let report = SetupQueryValidator::validate(&query);

    assert!(report.has_errors());
    assert!(report.contains_code(DiagnosticCode::ESetupQueryInvalid));
}
