use clearra_core_domain::{
    board::board_size::BoardSize, pc::pc_target::PcTarget, piece::piece_kind::PieceKind,
};
use clearra_setup_search::query::{
    SetupHoldPolicy, SetupLimits, SetupProbabilityFilter, SetupSearchQuery,
};

use crate::diagnostic::diagnostic_code::{DiagnosticCode, DiagnosticSeverity};

use super::SetupQueryValidator;

#[test]
fn setup_query_validator_accepts_one_explicitly_selected_hold_duplicate() {
    let query = SetupSearchQuery::default()
        .with_remaining_pieces(vec![
            PieceKind::I,
            PieceKind::I,
            PieceKind::O,
            PieceKind::T,
            PieceKind::S,
            PieceKind::Z,
            PieceKind::J,
        ])
        .with_hold_policy(SetupHoldPolicy::EnabledWithPiece(PieceKind::I));

    let report = SetupQueryValidator::validate(&query);

    assert!(!report.has_errors());
    assert!(report.contains_code(DiagnosticCode::ISetupQueryMvpSupported));
}

#[test]
fn setup_query_validator_rejects_an_unselected_duplicate() {
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
fn setup_query_validator_rejects_a_selected_hold_missing_from_inventory() {
    let query = SetupSearchQuery::default()
        .with_remaining_pieces(vec![PieceKind::I, PieceKind::O, PieceKind::T])
        .with_hold_policy(SetupHoldPolicy::EnabledWithPiece(PieceKind::S));

    assert!(SetupQueryValidator::validate(&query).has_errors());
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
fn queue_based_setup_accepts_exact_next_cycle_inventory_with_one_hold_duplicate() {
    let query = SetupSearchQuery::default()
        .with_remaining_pieces(vec![PieceKind::T, PieceKind::I])
        .with_next_cycle_remaining_pieces(vec![
            PieceKind::O,
            PieceKind::O,
            PieceKind::S,
            PieceKind::I,
            PieceKind::T,
            PieceKind::Z,
        ]);

    let report = SetupQueryValidator::validate(&query);

    assert!(!report.has_errors());
}

#[test]
fn queue_based_setup_rejects_the_wrong_next_cycle_inventory_count() {
    let query = SetupSearchQuery::default()
        .with_remaining_pieces(vec![PieceKind::T, PieceKind::I])
        .with_next_cycle_remaining_pieces(vec![
            PieceKind::O,
            PieceKind::S,
            PieceKind::I,
            PieceKind::T,
            PieceKind::Z,
        ]);

    let report = SetupQueryValidator::validate(&query);

    assert!(report.has_errors());
}

#[test]
fn queue_based_setup_rejects_two_duplicated_next_cycle_piece_kinds() {
    let query = SetupSearchQuery::default()
        .with_remaining_pieces(vec![PieceKind::T, PieceKind::I])
        .with_next_cycle_remaining_pieces(vec![
            PieceKind::O,
            PieceKind::O,
            PieceKind::S,
            PieceKind::S,
            PieceKind::I,
            PieceKind::T,
        ]);

    let report = SetupQueryValidator::validate(&query);

    assert!(report.has_errors());
    assert!(report.contains_code(DiagnosticCode::ESetupQueryInvalid));
}
