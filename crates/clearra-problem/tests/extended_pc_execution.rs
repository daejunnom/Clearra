use clearra_core_domain::{board::standard_pc_board::Board256Mask, piece::piece_kind::PieceKind};
use clearra_pc_graph::request::{
    ExtendedPcScenarioBoard, PcQueueInput, PcScenarioQuery, PieceWindow,
};
use clearra_problem::{
    ExtendedPcSearchContract, ExtendedPcSearchContractError, FiniteScenarioPcCompileBudget,
    ProblemCompiler,
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
        assert!(execution
            .problem()
            .problem_id()
            .as_str()
            .contains(&format!(
                "{:016x}{:016x}{:016x}{:016x}",
                words[3], words[2], words[1], words[0]
            )));
    }
}

#[test]
fn pc_execution_compiler_rejects_a_piece_count_that_ignores_high_words() {
    let board =
        ExtendedPcScenarioBoard::standard_10_from_words(24, words_with_two_open_columns(24)).unwrap();
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
        ExtendedPcScenarioBoard::standard_10_from_words(24, words_with_two_open_columns(24)).unwrap();
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
