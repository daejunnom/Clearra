//! Functional proof of the typed 7..24L PC bridge. No performance acceptance
//! or product-reducer qualification is inferred from these finite examples.
use clearra_core_domain::{
    board::standard_pc_board::Board256Mask, execution_cancellation::ExecutionControl,
    piece::piece_kind::PieceKind,
};
use clearra_core_executor::WasmBuildProbabilityBackend;
use clearra_pc_graph::request::{
    ExtendedPcScenarioBoard, PcExecutionPolicy, PcQueueInput, PcScenarioQuery, PieceWindow,
    WorkerPolicy,
};
use clearra_problem::{
    BuildProbabilityAggregation, BuildProbabilityFinesseRequest, ExtendedPcSearchContract,
};
use clearra_supply::queue::fixed_sequence::FixedSequence;

fn top_down_t_field(height: u8) -> (Board256Mask, usize) {
    assert_eq!(height % 3, 0);
    let mut holes = Board256Mask::EMPTY;
    for band in 0..u16::from(height / 3) {
        let x = if band % 2 == 0 { 0 } else { 7 };
        for (dx, dy) in [(1, 0), (0, 1), (1, 1), (1, 2)] {
            holes = holes.union(Board256Mask::singleton((band * 3 + dy) * 10 + x + dx).unwrap());
        }
    }
    (
        Board256Mask::all_cells(u16::from(height) * 10)
            .unwrap()
            .without(holes),
        usize::from(height / 3),
    )
}

#[test]
fn twelve_and_twenty_four_line_pc_execute_existing_four_word_inverse_lock_clear() {
    for height in [12_u8, 24] {
        let (base, pieces) = top_down_t_field(height);
        let query = PcScenarioQuery::new_extended(
            ExtendedPcScenarioBoard::standard_10(height, base).unwrap(),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::T; pieces])),
            PieceWindow::new(pieces),
        )
        .with_allow_hold(false)
        .with_min_remaining_queue(0)
        .with_execution_policy(
            PcExecutionPolicy::default().with_worker_policy(WorkerPolicy::Fixed(1)),
        );
        let compiled = ExtendedPcSearchContract::compile(query)
            .unwrap()
            .execution_problem()
            .unwrap();
        let result = WasmBuildProbabilityBackend::execute_with_control(
            compiled.problem(),
            compiled.field(),
            BuildProbabilityAggregation::Buildability,
            BuildProbabilityFinesseRequest::Off,
            &ExecutionControl::default(),
        )
        .unwrap();
        assert_eq!(result.bool_field("count_complete"), Some(true), "{result:?}");
        assert_eq!(
            result.bool_field("probability_complete"),
            Some(true),
            "{result:?}"
        );
        assert_eq!(result.field("coverage_probability"), Some("1"), "{result:?}");
        assert_eq!(result.usize_field("target_piece_count"), Some(pieces));
        assert!(
            result.usize_field("unique_solution_count").unwrap() > 0,
            "{result:?}"
        );
        assert!(!result.normalized_solution_keys().is_empty());
        let words = base.words();
        let identity = format!(
            "ctk2|height={height}|initial={:016x}{:016x}{:016x}{:016x}|placements=",
            words[3], words[2], words[1], words[0]
        );
        assert!(result
            .normalized_solution_keys()
            .iter()
            .all(|key| key.starts_with(&identity)));
        assert_eq!(
            result
                .path_steps()
                .iter()
                .map(|step| usize::from(step.cleared_lines()))
                .sum::<usize>(),
            usize::from(height)
        );
    }
}
