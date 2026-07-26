use clearra_core_domain::{
    execution_cancellation::{ExecutionCancellationToken, ExecutionControl},
    piece::piece_kind::PieceKind,
};
use clearra_problem::{
    compile_setup_search_conditions, SetupCandidatePriority, SetupLengthPreference, SetupLimits,
    SetupSearchQuery,
};

use super::{
    compare_setup_candidates, compile_setup_pattern_index, include_setup_depth_range,
    merge_exact_state_coverage, piece_index, prefers_setup_representative_depth,
    terminal_supply_target_word, SetupSupplyStateLayout, WasmSetupSearchAdvance,
    WasmSetupSearchSession,
};

#[test]
fn queue_based_supply_layout_has_no_terminal_inventory_consumption_dimension() {
    let layout = SetupSupplyStateLayout::new();
    let state = layout.encode(7, 1, 3);

    assert_eq!(layout.decode(state), (7, 1, 3));
    assert_eq!(
        layout.state_capacity(2),
        Some(2 * super::EXTRA_DRAW_STATE_COUNT * super::HOLD_STATE_COUNT)
    );
}

#[test]
fn queue_based_setup_depth_keeps_every_partial_path() {
    let mut min_depth = u8::MAX;
    let mut max_depth = 0;

    for depth in [3, 4, 6] {
        include_setup_depth_range(&mut min_depth, &mut max_depth, depth);
    }

    assert_eq!((min_depth, max_depth), (3, 6));
}

#[test]
fn queue_based_terminal_inventory_matches_hold_plus_exact_bag_suffix() {
    let oracle =
        SetupSearchQuery::default().with_remaining_pieces(vec![PieceKind::T, PieceKind::I]);
    let condition = compile_setup_search_conditions(&oracle)
        .expect("oracle condition")
        .remove(0);
    let pattern_index = compile_setup_pattern_index(&condition).expect("pattern index");
    let queue_based_condition =
        compile_setup_search_conditions(&oracle.with_next_cycle_remaining_pieces(vec![
            PieceKind::O,
            PieceKind::O,
            PieceKind::S,
            PieceKind::I,
            PieceKind::T,
            PieceKind::Z,
        ]))
        .expect("queue-based condition")
        .remove(0);
    let target = queue_based_condition
        .terminal_supply_target()
        .expect("terminal target");
    let filtered_index =
        compile_setup_pattern_index(&queue_based_condition).expect("filtered pattern index");
    assert_eq!(
        filtered_index.global_pattern_count(),
        pattern_index.global_pattern_count()
    );
    assert!(filtered_index.local_pattern_count() < filtered_index.global_pattern_count());
    let hold_o = piece_index(PieceKind::O) as u8 + 1;
    let piece_j = piece_index(PieceKind::J) as u8 + 1;
    let piece_l = piece_index(PieceKind::L) as u8 + 1;

    let mut matched = 0_u32;
    for word_index in 0..pattern_index.word_count() {
        let active = pattern_index.active_word(word_index);
        let expected = active
            & ((pattern_index.piece_word(9, piece_j, word_index)
                & pattern_index.piece_word(10, piece_l, word_index))
                | (pattern_index.piece_word(9, piece_l, word_index)
                    & pattern_index.piece_word(10, piece_j, word_index)));
        let actual = terminal_supply_target_word(
            &pattern_index,
            target,
            0,
            10,
            1,
            hold_o,
            word_index,
            active,
        );

        assert_eq!(actual, expected);
        assert_eq!(
            terminal_supply_target_word(&pattern_index, target, 0, 10, 0, 0, word_index, active,),
            0
        );
        matched += actual.count_ones();
    }
    assert!(matched > 0);
}

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
        compare_setup_candidates(
            SetupCandidatePriority::All,
            SetupLengthPreference::Auto,
            0.8,
            0.4,
            3,
            3,
            1,
            0.7,
            0.5,
            3,
            3,
            2,
        ),
        Ordering::Greater
    );
    assert_eq!(
        compare_setup_candidates(
            SetupCandidatePriority::BuildProbabilityFirst,
            SetupLengthPreference::Auto,
            0.8,
            0.4,
            3,
            3,
            2,
            0.7,
            0.63,
            3,
            8,
            1,
        ),
        Ordering::Less
    );
    assert_eq!(
        compare_setup_candidates(
            SetupCandidatePriority::BuildProbabilityFirst,
            SetupLengthPreference::Auto,
            0.8,
            0.4,
            3,
            6,
            1,
            0.8,
            0.72,
            3,
            4,
            2,
        ),
        Ordering::Less
    );
    assert_eq!(
        compare_setup_candidates(
            SetupCandidatePriority::All,
            SetupLengthPreference::Auto,
            0.8,
            0.4,
            3,
            6,
            1,
            0.8,
            0.4,
            3,
            4,
            2,
        ),
        Ordering::Less
    );
    assert_eq!(
        compare_setup_candidates(
            SetupCandidatePriority::All,
            SetupLengthPreference::Shorter,
            0.8,
            0.4,
            2,
            6,
            1,
            0.8,
            0.4,
            4,
            4,
            2,
        ),
        Ordering::Less
    );
    assert_eq!(
        compare_setup_candidates(
            SetupCandidatePriority::PcProbabilityFirst,
            SetupLengthPreference::Auto,
            0.7,
            0.63,
            3,
            8,
            2,
            0.8,
            0.64,
            3,
            3,
            1,
        ),
        Ordering::Less
    );
    assert_eq!(
        compare_setup_candidates(
            SetupCandidatePriority::PcProbabilityFirst,
            SetupLengthPreference::Auto,
            0.9,
            0.45,
            2,
            8,
            1,
            0.8,
            0.4,
            4,
            4,
            2,
        ),
        Ordering::Less
    );
}

#[test]
fn setup_candidate_priority_selects_a_representative_with_matching_lock_advantage() {
    assert!(prefers_setup_representative_depth(
        SetupCandidatePriority::BuildProbabilityFirst,
        SetupLengthPreference::Auto,
        6,
        4,
    ));
    assert!(!prefers_setup_representative_depth(
        SetupCandidatePriority::BuildProbabilityFirst,
        SetupLengthPreference::Auto,
        3,
        4,
    ));
    assert!(prefers_setup_representative_depth(
        SetupCandidatePriority::PcProbabilityFirst,
        SetupLengthPreference::Auto,
        3,
        4,
    ));
    assert!(prefers_setup_representative_depth(
        SetupCandidatePriority::All,
        SetupLengthPreference::Auto,
        6,
        4,
    ));
    assert!(prefers_setup_representative_depth(
        SetupCandidatePriority::PcProbabilityFirst,
        SetupLengthPreference::Longer,
        6,
        4,
    ));
    assert!(prefers_setup_representative_depth(
        SetupCandidatePriority::BuildProbabilityFirst,
        SetupLengthPreference::Shorter,
        3,
        4,
    ));
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
    assert_eq!(report.hold_conditions().len(), 1);
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
