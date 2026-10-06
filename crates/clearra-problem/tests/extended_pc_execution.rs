use clearra_core_domain::{
    board::standard_pc_board::Board256Mask, pc::pc_target::PcTarget, piece::piece_kind::PieceKind,
};
use clearra_pc_graph::request::{
    ExtendedPcScenarioBoard, OpeningPcSearchQuery, PcCountPolicy, PcExecutionPolicy, PcHoldPolicy,
    PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow, WorkerPolicy,
};
use clearra_problem::{
    ExtendedPcSearchContract, ExtendedPcSearchContractError, FiniteScenarioPcCompileBudget,
    ProblemCompiler, SearchOutputPolicy, SearchProblemPreset,
};
use clearra_supply::queue::fixed_sequence::FixedSequence;

fn words_with_two_open_columns(height: u8) -> [u64; 4] {
    let mut words = [0_u64; 4];
    for y in 0..u16::from(height) {
        for x in 0..8 {
            let bit = y * 10 + x;
            words[usize::from(bit / 64)] |= 1_u64 << (bit % 64);
        }
    }
    words
}

#[test]
fn opening_compiler_preserves_even_targets_and_spawn_height_without_enumeration() {
    for lines in (2..=24).step_by(2) {
        let pieces = usize::from(lines) * 10 / 4;
        let query = OpeningPcSearchQuery::new(PcTarget::new(lines).unwrap())
            .with_queue(PcQueueInput::fixed_sequence(FixedSequence::new(vec![
                PieceKind::I;
                pieces
            ])))
            .with_hold_policy(PcHoldPolicy::Disabled);
        for problem in [
            ProblemCompiler::compile_opening_pc(&query).unwrap(),
            ProblemCompiler::compile_opening_pc_tiling(&query).unwrap(),
        ] {
            assert_eq!(problem.preset(), SearchProblemPreset::OpeningPc);
            assert_eq!(problem.initial_board().occupied_words(), [0; 4]);
            assert_eq!(problem.visible_height(), u16::from(lines));
            assert_eq!(problem.search_height(), u16::from(lines).max(20));
            assert_eq!(problem.exact_pieces(), Some(pieces));
            assert_eq!(problem.piece_window().max_pieces(), pieces);
            assert_eq!(problem.labels().last().unwrap(), &format!("{lines}L"));
            assert_eq!(problem.labels().len(), usize::from(lines / 2));
            let schedule = problem
                .checkpoint_schedule()
                .expect("Opening metadata is never silently dropped");
            assert_eq!(schedule.target().lines(), lines);
            assert_eq!(schedule.partitions().len(), 1 << (lines / 2 - 1));
            if lines > 6 {
                assert!(problem.initial_occupancy().is_none());
            } else {
                assert_eq!(problem.initial_occupancy().unwrap().height, lines);
            }
        }
        let tiling = ProblemCompiler::compile_opening_pc_tiling(&query).unwrap();
        assert_eq!(tiling.output_policy(), SearchOutputPolicy::TilingOnly);
    }
}

#[test]
fn pc_execution_compiler_binds_all_words_and_does_not_shrink_target_height() {
    for height in [8_u8, 12, 14, 24] {
        let words = words_with_two_open_columns(height);
        let pieces = usize::from(height / 2);
        let query = PcScenarioQuery::new_extended(
            ExtendedPcScenarioBoard::standard_10_from_words(height, words).unwrap(),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::O; pieces])),
            PieceWindow::new(pieces),
        )
        .with_allow_hold(false)
        .with_min_remaining_queue(0);
        let execution = ExtendedPcSearchContract::compile(query)
            .unwrap()
            .execution_problem()
            .unwrap();
        assert_eq!(execution.field().height(), height);
        assert_eq!(execution.field().base_words(), words);
        assert_eq!(execution.problem().initial_board().occupied_words(), words);
        assert_eq!(execution.problem().visible_height(), u16::from(height));
        assert_eq!(execution.problem().exact_pieces(), Some(pieces));
        assert_eq!(execution.field().target_piece_count(), pieces);
        assert_eq!(
            execution.field().target_board(),
            Board256Mask::all_cells(u16::from(height) * 10).unwrap()
        );
        assert!(execution.problem().problem_id().as_str().contains(&format!(
            "{:016x}{:016x}{:016x}{:016x}",
            words[3], words[2], words[1], words[0]
        )));
    }
}

#[test]
fn pc_execution_compiler_rejects_a_piece_count_that_ignores_high_words() {
    let board =
        ExtendedPcScenarioBoard::standard_10_from_words(24, words_with_two_open_columns(24))
            .unwrap();
    let query = PcScenarioQuery::new_extended(
        board,
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::O; 12])),
        PieceWindow::new(12),
    )
    .with_allow_hold(false)
    .with_min_remaining_queue(0)
    .with_exact_pieces(Some(1));
    assert!(matches!(
        ExtendedPcSearchContract::compile(query)
            .unwrap()
            .execution_problem(),
        Err(ExtendedPcSearchContractError::ExactPieceCountMismatch {
            requested: 1,
            required_pieces: 12
        })
    ));
}

#[test]
fn pc_execution_compiler_rejects_a_too_short_window_before_geometry() {
    let board =
        ExtendedPcScenarioBoard::standard_10_from_words(24, words_with_two_open_columns(24))
            .unwrap();
    let query = PcScenarioQuery::new_extended(
        board,
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::O; 11])),
        PieceWindow::new(11),
    );
    assert!(matches!(
        ExtendedPcSearchContract::compile(query)
            .unwrap()
            .execution_problem(),
        Err(ExtendedPcSearchContractError::PieceWindowTooShort {
            maximum_pieces: 11,
            required_pieces: 12
        })
    ));
}

#[test]
fn finite_and_ordinary_compilers_bind_the_same_extended_board_identity() {
    let words = words_with_two_open_columns(24);
    let query = PcScenarioQuery::new_extended(
        ExtendedPcScenarioBoard::standard_10_from_words(24, words).unwrap(),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::O; 12])),
        PieceWindow::new(12),
    )
    .with_allow_hold(false);
    let execution = ExtendedPcSearchContract::compile(query)
        .unwrap()
        .execution_problem()
        .unwrap();
    let query = execution.problem().scenario().core_query().clone();
    let budget = FiniteScenarioPcCompileBudget::try_new(16 * 1024 * 1024, 0, 0).unwrap();
    let finite = ProblemCompiler::compile_scenario_pc_finite_build(query, budget).unwrap();
    assert_eq!(
        finite.problem().problem_id(),
        execution.problem().problem_id()
    );
    assert_eq!(finite.problem().initial_board().occupied_words(), words);
    assert!(finite.peak_required_memory_bytes() <= budget.max_memory_bytes());
}

#[test]
fn distinct_high_words_cannot_alias_a_problem_with_the_same_low_word() {
    let first = words_with_two_open_columns(24);
    let mut second = first;
    // Move one initial cell within the uppermost row without changing area,
    // row completeness, the compact low word or any supply/rule policy.
    second[3] ^= (1_u64 << (230 - 192)) | (1_u64 << (238 - 192));
    let compile = |words| {
        let query = PcScenarioQuery::new_extended(
            ExtendedPcScenarioBoard::standard_10_from_words(24, words).unwrap(),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::O; 12])),
            PieceWindow::new(12),
        )
        .with_allow_hold(false);
        ExtendedPcSearchContract::compile(query)
            .unwrap()
            .execution_problem()
            .unwrap()
    };
    let first_problem = compile(first);
    let second_problem = compile(second);
    assert_eq!(
        first_problem.problem().initial_board().occupied_mask(),
        second_problem.problem().initial_board().occupied_mask()
    );
    assert_ne!(
        first_problem.problem().problem_id(),
        second_problem.problem().problem_id()
    );
}

#[test]
fn shared_input_bridge_leaves_every_compact_target_on_the_legacy_contract() {
    for height in 1..=6 {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(0, 0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
            PieceWindow::new(1),
        );
        assert_eq!(
            ExtendedPcSearchContract::compile_standard_query(query, height),
            Err(ExtendedPcSearchContractError::CompactBoardContractRequired)
        );
    }
}

#[test]
fn shared_input_bridge_uses_the_explicit_target_not_the_initial_field_height() {
    for height in [8_u8, 12, 14, 24] {
        let pieces = usize::from(height) * 10 / 4;
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(0, 0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I; pieces])),
            PieceWindow::new(pieces),
        )
        .with_allow_hold(false);
        // This only compiles the input. It never enumerates the huge empty PC.
        let execution = ExtendedPcSearchContract::compile_standard_query(query, height)
            .unwrap()
            .execution_problem()
            .unwrap();
        assert_eq!(execution.field().height(), height);
        assert_eq!(execution.problem().visible_height(), u16::from(height));
        assert_eq!(execution.field().target_piece_count(), pieces);
        assert_eq!(execution.field().base_words(), [0; 4]);
    }
}

#[test]
fn shared_input_bridge_preserves_initial_clear_and_all_nonboard_policies() {
    let height = 24_u8;
    let words = words_with_two_open_columns(height);
    let mut original = words;
    // Complete two middle rows. Normalization must shift high words across
    // word boundaries exactly once, and retain that initial-clear receipt.
    for row in [5_u16, 13] {
        for x in 8..10 {
            let bit = row * 10 + x;
            original[usize::from(bit / 64)] |= 1_u64 << (bit % 64);
        }
    }
    let initial = PcScenarioBoard::standard_10_from_words(24, original).unwrap();
    let frame = initial.to_standard_target_frame(height).unwrap();
    let pieces = frame.required_pieces();
    let query = PcScenarioQuery::new(
        initial,
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::O; pieces])),
        PieceWindow::new(pieces),
    )
    .with_rule(clearra_rules::profile::builtin_rules::srs_x())
    .with_requires_180(true)
    .with_allow_hold(false)
    .with_exact_pieces(Some(pieces))
    .with_count_policy(PcCountPolicy::CountAll)
    .with_retained_trace_limit(3)
    .with_execution_policy(
        PcExecutionPolicy::default().with_worker_policy(WorkerPolicy::Fixed(11)),
    );
    let expected = query
        .clone()
        .map_initial_board(|_| frame.normalized_board().clone());
    let contract = ExtendedPcSearchContract::compile_standard_query(query, height).unwrap();
    let execution = contract.execution_problem().unwrap();
    assert_eq!(execution.target_frame().initial_cleared_rows(), 2);
    assert_eq!(execution.target_frame(), &frame);
    assert_eq!(
        execution.field().base_words(),
        expected.initial_board().occupied_words()
    );
    assert_eq!(
        contract.query().clone().map_initial_board(|board| {
            PcScenarioBoard::standard_10_from_words(
                u16::from(board.visible_height()),
                board.occupied_words(),
            )
            .unwrap()
        }),
        expected
    );
    assert_eq!(
        execution.problem(),
        &ProblemCompiler::compile_scenario_pc(&expected).unwrap()
    );
    // Moving CountAll and 11-worker policies does not claim that their public
    // extended reducers or pool are already connected.
    assert!(!contract.runtime_capability().connected_exact());
}

#[test]
fn shared_input_bridge_cannot_hide_occupancy_above_a_smaller_target() {
    let occupied = Board256Mask::singleton(23 * 10 + 9).unwrap();
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10_from_words(24, occupied.words()).unwrap(),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I; 20])),
        PieceWindow::new(20),
    );
    assert!(matches!(
        ExtendedPcSearchContract::compile_standard_query(query, 8),
        Err(ExtendedPcSearchContractError::TargetFrame(
            clearra_pc_graph::request::PcScenarioTargetFrameError::ExtendedOccupancyAboveTarget {
                target_lines: 8,
                ..
            }
        ))
    ));
}

#[test]
fn shared_input_bridge_allows_a_tall_initial_field_only_after_real_line_clear() {
    let full = Board256Mask::all_cells(240).unwrap();
    let mut holes = Board256Mask::EMPTY;
    for (column, start) in [(0, 17), (1, 20)] {
        for row in start..start + 4 {
            holes = holes.union(Board256Mask::singleton(row * 10 + column).unwrap());
        }
    }
    let initial = full.without(holes);
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10_from_words(24, initial.words()).unwrap(),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I; 2])),
        PieceWindow::new(2),
    )
    .with_allow_hold(false);
    let execution = ExtendedPcSearchContract::compile_standard_query(query, 7)
        .unwrap()
        .execution_problem()
        .unwrap();
    assert_eq!(execution.target_frame().initial_cleared_rows(), 17);
    assert_eq!(execution.field().height(), 7);
    assert_eq!(execution.field().target_piece_count(), 2);
    assert_eq!(execution.problem().visible_height(), 7);
    assert_ne!(execution.field().base_words()[1], 0);
    assert_eq!(execution.field().base_words()[2..], [0; 2]);
}
