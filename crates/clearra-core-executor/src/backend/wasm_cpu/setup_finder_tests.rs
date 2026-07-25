use clearra_core_domain::{
    execution_cancellation::{ExecutionCancellationToken, ExecutionControl},
    piece::piece_kind::PieceKind,
};
use clearra_problem::{SetupCandidatePriority, SetupLimits, SetupSearchQuery};

use super::{
    compare_setup_candidates, merge_exact_state_coverage, WasmSetupSearchAdvance,
    WasmSetupSearchSession,
};

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
fn setup_candidate_priority_uses_the_requested_lexicographic_tie_break() {
    use std::cmp::Ordering;

    assert_eq!(
        compare_setup_candidates(SetupCandidatePriority::All, 0.8, 0.4, 3, 1, 0.7, 0.5, 3, 2,),
        Ordering::Greater
    );
    assert_eq!(
        compare_setup_candidates(
            SetupCandidatePriority::BuildProbabilityFirst,
            0.8,
            0.4,
            3,
            2,
            0.7,
            0.63,
            3,
            1,
        ),
        Ordering::Less
    );
    assert_eq!(
        compare_setup_candidates(
            SetupCandidatePriority::BuildProbabilityFirst,
            0.8,
            0.64,
            3,
            1,
            0.8,
            0.72,
            3,
            2,
        ),
        Ordering::Greater
    );
    assert_eq!(
        compare_setup_candidates(
            SetupCandidatePriority::PcProbabilityFirst,
            0.7,
            0.63,
            3,
            2,
            0.8,
            0.64,
            3,
            1,
        ),
        Ordering::Less
    );
    assert_eq!(
        compare_setup_candidates(
            SetupCandidatePriority::PcProbabilityFirst,
            0.9,
            0.45,
            3,
            1,
            0.8,
            0.4,
            3,
            2,
        ),
        Ordering::Less
    );
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

#[test]
#[ignore = "full IOTS empty-4L exact acceptance; run in the release acceptance suite"]
fn iots_three_lock_setup_matches_fresh_pc_continuation_coverage() {
    const TARGET_SETUP_BOARD: u64 = 0x0040_11c4_f9;

    let query = SetupSearchQuery::default().with_remaining_pieces(vec![
        PieceKind::I,
        PieceKind::O,
        PieceKind::T,
        PieceKind::S,
    ]);
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
    let empty_hold = report
        .hold_conditions()
        .iter()
        .find(|condition| condition.initial_hold().is_none())
        .expect("empty-hold condition");
    let candidate = empty_hold
        .candidates()
        .iter()
        .find(|candidate| candidate.board_mask() == TARGET_SETUP_BOARD)
        .expect("known three-lock IOTS setup");

    assert_eq!(empty_hold.pattern_count(), 120_960);
    assert_eq!(candidate.min_locks(), 3);
    assert_eq!(candidate.build_covered_patterns(), 120_960);
    assert_eq!(
        candidate.joint_covered_patterns(),
        candidate.build_covered_patterns()
    );
    assert_eq!(candidate.conditional_pc_probability(), "1.0");
}
