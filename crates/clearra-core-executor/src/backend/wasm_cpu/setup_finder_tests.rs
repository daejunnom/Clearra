use clearra_core_domain::{
    execution_cancellation::{ExecutionCancellationToken, ExecutionControl},
    piece::piece_kind::PieceKind,
    probability::probability_value::ProbabilityValue,
};
use clearra_coverage::universe::{
    pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId,
};
use clearra_problem::{
    compile_setup_search_conditions, SetupCandidatePriority, SetupLengthPreference, SetupLimits,
    SetupPathDetail, SetupSearchQuery,
};
use clearra_supply::pattern_universe::{MaterializedPatternUniverse, PatternPiecePositionIndex};

use super::{
    compare_setup_candidates, compile_setup_pattern_index, include_setup_depth_range,
    merge_exact_state_coverage, piece_index, prefers_setup_representative_depth,
    retain_best_setup_state_per_board, setup_build_progress_phase, setup_supply_transitions,
    terminal_supply_target_word, SetupShape, SetupSupplyStateLayout, WasmSetupSearchAdvance,
    WasmSetupSearchSession,
};

#[test]
fn serial_setup_progress_maps_graph_passes_to_stable_ui_phases() {
    assert_eq!(setup_build_progress_phase(0), ("setup-geometry", 1));
    assert_eq!(setup_build_progress_phase(1), ("setup-graph", 2));
    assert_eq!(setup_build_progress_phase(2), ("setup-graph", 2));
    assert_eq!(setup_build_progress_phase(3), ("setup-graph", 2));
}

#[test]
fn guaranteed_leading_residue_piece_covers_the_entire_pattern_word() {
    let universe = MaterializedPatternUniverse::from_sequences(
        PatternUniverseId::new(1),
        PatternWeightModelId::new(1),
        vec![
            vec![PieceKind::I, PieceKind::O],
            vec![PieceKind::I, PieceKind::T],
        ],
        vec![
            ProbabilityValue::new(0.4).expect("weight"),
            ProbabilityValue::new(0.6).expect("weight"),
        ],
        2,
        true,
        None,
    )
    .expect("pattern universe");
    let pattern_index = PatternPiecePositionIndex::compile(&universe).expect("pattern index");
    let active = pattern_index.active_word(0);
    let transitions = setup_supply_transitions(
        &pattern_index,
        0,
        true,
        false,
        false,
        0,
        piece_index(PieceKind::I) as u8 + 1,
        0,
        0,
        active,
        0,
        false,
    );
    let covered = transitions
        .iter()
        .fold(0_u64, |coverage, transition| coverage | transition.mask);

    assert_eq!(active, 0b11);
    assert_eq!(covered, active);
}

#[test]
fn queue_based_supply_layout_has_no_observed_piece_consumption_dimension() {
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
fn visible_board_uses_only_the_best_independently_measured_exact_state() {
    let shapes = vec![
        SetupShape::new(0x3c, 0, 0),
        SetupShape::new(0x3c, 1, 0),
        SetupShape::new(0x0f, 2, 0),
    ];
    let mut sorted_shape_indexes = vec![1, 2, 0];

    retain_best_setup_state_per_board(&mut sorted_shape_indexes, &shapes).expect("board dedupe");

    assert_eq!(sorted_shape_indexes, vec![1, 2]);
}

#[test]
fn next_cycle_terminal_inventory_matches_hold_plus_exact_bag_suffix() {
    let oracle =
        SetupSearchQuery::default().with_remaining_pieces(vec![PieceKind::T, PieceKind::I]);
    let condition = compile_setup_search_conditions(&oracle)
        .expect("oracle condition")
        .remove(0);
    let pattern_index = compile_setup_pattern_index(&condition).expect("pattern index");
    let terminal_inventory_condition =
        compile_setup_search_conditions(&oracle.with_next_cycle_remaining_pieces(vec![
            PieceKind::O,
            PieceKind::O,
            PieceKind::S,
            PieceKind::I,
            PieceKind::T,
            PieceKind::Z,
        ]))
        .expect("terminal inventory condition")
        .remove(0);
    let target = terminal_inventory_condition
        .terminal_supply_target()
        .expect("terminal target");
    let filtered_index =
        compile_setup_pattern_index(&terminal_inventory_condition).expect("filtered pattern index");
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
    assert_eq!(report.coverage_semantics(), "full-future-oracle");
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

#[test]
#[ignore = "full SZ empty-4L symmetry acceptance; run in the release acceptance suite"]
fn mirrored_one_lock_setups_have_identical_coverage_and_tiling_counts() {
    const Z_SETUP_BOARD: u64 = 0xc060;
    const S_SETUP_BOARD: u64 = 0xc018;

    fn run(query: &SetupSearchQuery, control: &ExecutionControl) -> crate::CoreExecutionResult {
        let mut session = WasmSetupSearchSession::new(query).expect("setup session");
        loop {
            match session.advance(8_192, control).expect("setup advance") {
                WasmSetupSearchAdvance::Pending => {}
                WasmSetupSearchAdvance::Completed(result) => break result,
                WasmSetupSearchAdvance::Cancelled => panic!("setup search was not cancelled"),
            }
        }
    }

    let query = SetupSearchQuery::default()
        .with_remaining_pieces(vec![PieceKind::S, PieceKind::Z])
        .with_max_setup_pieces(1)
        .with_tablebase_requested(false);
    let control = ExecutionControl::new(ExecutionCancellationToken::new());
    let result = run(&query, &control);
    let report = result.setup_finder_report().expect("setup report");
    let condition = report
        .hold_conditions()
        .iter()
        .find(|condition| condition.initial_hold().is_none())
        .expect("empty-hold condition");
    let z = condition
        .candidates()
        .iter()
        .find(|candidate| candidate.board_mask() == Z_SETUP_BOARD)
        .expect("Z setup");
    let s = condition
        .candidates()
        .iter()
        .find(|candidate| candidate.board_mask() == S_SETUP_BOARD)
        .expect("S setup");

    assert_eq!(z.build_covered_patterns(), s.build_covered_patterns());
    assert_eq!(z.joint_covered_patterns(), s.joint_covered_patterns());
    assert_eq!(z.joint_probability(), s.joint_probability());

    let z_detail =
        SetupPathDetail::from_setup_id(z.setup_id(), condition.condition_id()).expect("Z detail");
    let s_detail =
        SetupPathDetail::from_setup_id(s.setup_id(), condition.condition_id()).expect("S detail");
    let z_detail_result = run(&query.clone().with_path_detail(z_detail), &control);
    let s_detail_result = run(&query.clone().with_path_detail(s_detail), &control);
    let z_paths = &z_detail_result
        .setup_finder_report()
        .expect("Z detail report")
        .hold_conditions()[0]
        .candidates()[0];
    let s_paths = &s_detail_result
        .setup_finder_report()
        .expect("S detail report")
        .hold_conditions()[0]
        .candidates()[0];

    assert!(z_paths.solution_paths_complete());
    assert!(s_paths.solution_paths_complete());
    assert_eq!(z_paths.solution_path_count(), s_paths.solution_path_count());
}

#[test]
#[ignore = "selected setup detail acceptance; run in the release acceptance suite"]
fn selected_one_lock_detail_enumerates_complete_pc_suffixes() {
    let detail = SetupPathDetail::from_setup_id(
        "setup-000000c060-0000-00000000000000000000000000015d",
        "hold-empty",
    )
    .expect("known Z setup detail");
    let query = SetupSearchQuery::default()
        .with_remaining_pieces(vec![PieceKind::S, PieceKind::Z])
        .with_max_setup_pieces(1)
        .with_tablebase_requested(false)
        .with_path_detail(detail);
    let control = ExecutionControl::new(ExecutionCancellationToken::new());
    let mut session = WasmSetupSearchSession::new(&query).expect("setup detail session");
    let result = loop {
        match session
            .advance(8_192, &control)
            .expect("setup detail advance")
        {
            WasmSetupSearchAdvance::Pending => {}
            WasmSetupSearchAdvance::Completed(result) => break result,
            WasmSetupSearchAdvance::Cancelled => panic!("setup detail search was not cancelled"),
        }
    };
    let candidate = &result
        .setup_finder_report()
        .expect("setup detail report")
        .hold_conditions()[0]
        .candidates()[0];

    assert!(candidate.solution_paths_complete());
    assert!(candidate.solution_path_count() > 1);
}
