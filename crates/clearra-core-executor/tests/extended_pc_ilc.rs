//! A one-placement smoke of the existing four-word ILC/BuildUp engine.
//! Full-height PC input/area/identity contracts are checked separately by
//! `extended_pc_execution`. This smoke is not a complete 24L PC enumeration,
//! performance gate, or qualification of every public PC reducer.
use clearra_core_domain::{
    board::standard_pc_board::Board256Mask, execution_cancellation::ExecutionControl,
    piece::piece_kind::PieceKind,
};
use clearra_core_executor::WasmBuildProbabilityBackend;
use clearra_pc_graph::request::{
    PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow, WorkerPolicy,
};
use clearra_problem::{
    BuildProbabilityAggregation, BuildProbabilityField, BuildProbabilityFinesseRequest,
    ProblemCompiler,
};
use clearra_supply::queue::fixed_sequence::FixedSequence;

#[test]
fn one_placement_in_a_twenty_four_line_field_uses_all_words_in_existing_ilc() {
    let height = 24_u8;
    // One support cell below a vertical I, plus an unrelated uppermost cell.
    // Neither row is full, so initial line clear cannot remove the high words.
    // The I has an independent rotate-left-drop witness onto its support.
    let base = Board256Mask::singleton(19 * 10)
        .unwrap()
        .union(Board256Mask::singleton(23 * 10 + 9).unwrap());
    let mut target = Board256Mask::EMPTY;
    for row in 20..24 {
        target = target.union(Board256Mask::singleton(row * 10).unwrap());
    }
    assert_ne!(base.words()[3], 0);
    assert_ne!(target.words()[3], 0);
    let field =
        BuildProbabilityField::from_words_preserving_height(height, base.words(), target.words())
            .unwrap();
    assert_eq!(field.height(), 24);
    assert_eq!(field.target_piece_count(), 1);
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10_from_words(u16::from(height), base.words()).unwrap(),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
        PieceWindow::new(1),
    )
    .with_exact_pieces(Some(1))
    .with_allow_hold(false)
    .with_min_remaining_queue(0)
    .with_execution_policy(PcExecutionPolicy::default().with_worker_policy(WorkerPolicy::Fixed(1)));
    let problem = ProblemCompiler::compile_scenario_pc(&query).unwrap();
    assert_eq!(problem.initial_board().occupied_words(), base.words());
    let result = WasmBuildProbabilityBackend::execute_with_control(
        &problem,
        field,
        BuildProbabilityAggregation::Buildability,
        BuildProbabilityFinesseRequest::Off,
        &ExecutionControl::default(),
    )
    .unwrap();
    assert_eq!(
        result.bool_field("count_complete"),
        Some(true),
        "{result:?}"
    );
    assert_eq!(
        result.bool_field("probability_complete"),
        Some(true),
        "{result:?}"
    );
    assert_eq!(
        result.field("coverage_probability"),
        Some("1"),
        "{result:?}"
    );
    assert_eq!(result.usize_field("target_piece_count"), Some(1));
    assert_eq!(result.usize_field("unique_solution_count"), Some(1));
    assert_eq!(result.normalized_solution_keys().len(), 1);
    let words = base.words();
    let identity = format!(
        "ctk2|height={height}|initial={:016x}{:016x}{:016x}{:016x}|placements=",
        words[3], words[2], words[1], words[0]
    );
    assert!(result.normalized_solution_keys()[0].starts_with(&identity));
    assert_eq!(result.path_steps().len(), 1);
    assert_eq!(result.path_steps()[0].cleared_lines(), 0);
}
