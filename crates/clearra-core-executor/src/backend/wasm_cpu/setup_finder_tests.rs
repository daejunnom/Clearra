use clearra_core_domain::{
    execution_cancellation::{ExecutionCancellationToken, ExecutionControl},
    piece::piece_kind::PieceKind,
};
use clearra_problem::{SetupLimits, SetupSearchQuery};

use super::{merge_exact_state_coverage, WasmSetupSearchAdvance, WasmSetupSearchSession};

#[test]
fn shape_merge_intersects_forward_and_backward_per_exact_state() {
    let mut build = 0;
    let mut joint = 0;

    merge_exact_state_coverage(&mut build, &mut joint, 0b01, 0b10, 0b11);
    merge_exact_state_coverage(&mut build, &mut joint, 0b10, 0b01, 0b11);

    assert_eq!(build, 0b11);
    assert_eq!(joint, 0);

    merge_exact_state_coverage(&mut build, &mut joint, 0b10, 0b10, 0b11);
    assert_eq!(joint, 0b10);
}

#[test]
#[ignore = "full empty-4L exact acceptance; run in the release acceptance suite"]
fn setup_finder_returns_exact_joint_witness_paths() {
    let query = SetupSearchQuery::default()
        .with_remaining_pieces(vec![PieceKind::I, PieceKind::O, PieceKind::T])
        .with_limits(SetupLimits::new(1, 1, 1, 8, 100_000, 1).expect("setup limits"));
    let control = ExecutionControl::new(ExecutionCancellationToken::new());
    let mut session = WasmSetupSearchSession::new(&query).expect("setup session");
    let result = loop {
        match session.advance(8_192, &control).expect("setup advance") {
            WasmSetupSearchAdvance::Pending => {}
            WasmSetupSearchAdvance::Completed(result) => break result,
            WasmSetupSearchAdvance::Cancelled => panic!("setup search was not cancelled"),
        }
    };
    let report = result.setup_finder_report().expect("setup report");

    assert!(report.complete());
    assert_eq!(report.coverage_semantics(), "oracle");
    assert_eq!(report.hold_conditions().len(), 4);
    assert!(report
        .hold_conditions()
        .iter()
        .any(|condition| !condition.candidates().is_empty()));
    for condition in report.hold_conditions() {
        for candidate in condition.candidates() {
            assert!(!candidate.representative_path().is_empty());
            assert!((candidate.min_locks()..=candidate.max_locks())
                .contains(&(candidate.representative_path().len() as u8)));
            assert!(candidate.joint_covered_patterns() > 0);
        }
    }
}
