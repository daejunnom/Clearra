use super::*;
use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_replay::{
    ScoringExecutionNode, ScoringLockEvidence, SpinCoverageExecutionBatch,
    SpinCoverageExecutionGraph,
};

fn edge(operation: u8, cleared: u8, x: i8) -> ScoringExecutionEdge {
    ScoringExecutionEdge::new(
        u32::from(operation) + 1,
        operation,
        PieceKind::I,
        RotationState::Right,
        x,
        0,
        cleared,
        0,
        0,
        ScoringLockEvidence::no_rotation(RotationState::Right),
    )
    .with_perfect_clear(operation == 1)
}

fn batch(graph_ids: &[u64], bad_clear: bool, complete: bool) -> FullHeightExecutionBatch {
    let mask = |start: u16, column: u16| {
        (start..start + 4).fold(Board256Mask::EMPTY, |mask, row| {
            mask.union(Board256Mask::singleton(row * 10 + column).unwrap())
        })
    };
    let first = mask(0, 0);
    let second = mask(4, 1);
    let initial = Board256Mask::all_cells(80)
        .unwrap()
        .without(first.union(second));
    let hex = |mask: Board256Mask| {
        let [a, b, c, d] = mask.words();
        format!("{d:016x}{c:016x}{b:016x}{a:016x}")
    };
    let key = format!(
        "ctk2|height=8|initial={}|placements=I:{},I:{}",
        hex(initial),
        hex(first),
        hex(second)
    );
    let graphs = graph_ids
        .iter()
        .map(|&id| {
            SpinCoverageExecutionGraph::new(
                id,
                key.clone(),
                0,
                vec![
                    ScoringExecutionNode::new(0, 1, false),
                    ScoringExecutionNode::new(1, 1, false),
                    ScoringExecutionNode::new(2, 0, true),
                ],
                vec![edge(0, if bad_clear { 3 } else { 4 }, 0), edge(1, 4, 1)],
            )
        })
        .collect();
    FullHeightExecutionBatch::from_spin_coverage(
        8,
        initial,
        SpinCoverageExecutionBatch::new(
            vec![vec![PieceKind::I; 2]],
            0,
            None,
            false,
            false,
            false,
            101,
            103,
            graphs,
            complete,
        ),
    )
    .unwrap()
}

fn materialize(
    batch: &FullHeightExecutionBatch,
) -> Result<FullHeightScoreCellMaterialization, FullHeightScoreCellError> {
    FullHeightScoreCellMaterializer::materialize_with_memory_limit(
        batch,
        ScoreObjectivePolicy::summary(),
        &ExecutionControl::default(),
        4096,
        4 * 1024 * 1024,
    )
}

#[test]
fn four_word_score_cells_use_actual_clears_and_the_unchanged_common_score_matrix() {
    let batch = batch(&[1], false, true);
    assert_ne!(
        batch.initial().words()[1],
        0,
        "high word participates in the replay"
    );
    let cells = materialize(&batch).unwrap();
    assert!(cells.complete());
    assert_eq!(cells.cells().len(), 1);
    let policy = ScoreObjectivePolicy::summary();
    let (profile, _) = crate::score_profile_with_memory_guard(policy, 0, 1024 * 1024).unwrap();
    let mut expected =
        ScoreModelEvaluator::initial_state(ScoreEvaluationPolicy::tetrio_pc(policy.initial_b2b()));
    for index in 0..2 {
        expected = ScoreModelEvaluator::evaluate_classified_lock(
            &profile,
            expected,
            index,
            4,
            index == 1,
            None,
        );
    }
    assert_eq!(
        (cells.cells()[0].score(), cells.cells()[0].attack()),
        (expected.score(), expected.attack())
    );
    assert!(cells.admitted_peak_bytes() > cells.checked_retained_bytes().unwrap() + 4096);
    let matrix =
        super::super::ScoreMatrix::from_materialized_cells(cells.into_cells(), &profile, 1, true);
    assert!(matrix.complete());
}

#[test]
fn multiple_physical_graphs_for_one_colored_family_do_not_duplicate_score_cells() {
    let single = materialize(&batch(&[1], false, true)).unwrap();
    let repeated = materialize(&batch(&[1, 1], false, true)).unwrap();
    assert_eq!(single.cells(), repeated.cells());
    let projection = FullHeightScoreCellMaterializer::checked_memory_projection(
        &batch(&[1, 1], false, true),
        ScoreObjectivePolicy::summary(),
    )
    .unwrap();
    assert_eq!(projection.candidate_count, 1);
    assert_eq!(projection.cell_capacity, 1);
}

#[test]
fn public_batches_cannot_relabel_canonical_candidate_indices_or_clear_evidence() {
    assert_eq!(
        materialize(&batch(&[2], false, true)).unwrap_err(),
        FullHeightScoreCellError::InvalidEvidence
    );
    assert_eq!(
        materialize(&batch(&[1, 2], false, true)).unwrap_err(),
        FullHeightScoreCellError::InvalidEvidence
    );
    assert_eq!(
        materialize(&batch(&[1], true, true)).unwrap_err(),
        FullHeightScoreCellError::InvalidEvidence
    );
}

#[test]
fn incomplete_execution_input_does_not_gain_complete_materialization_authority() {
    let cells = materialize(&batch(&[1], false, false)).unwrap();
    assert!(!cells.complete());
    assert_eq!(cells.cells().len(), 1);
}

#[test]
fn four_word_materialization_admits_memory_before_allocating_and_cancellation_returns_no_cells() {
    let batch = batch(&[1], false, true);
    let policy = ScoreObjectivePolicy::summary();
    let projection =
        FullHeightScoreCellMaterializer::checked_memory_projection(&batch, policy).unwrap();
    let required = projection.required_peak_bytes + 4096;
    assert_eq!(
        FullHeightScoreCellMaterializer::materialize_with_memory_limit(
            &batch,
            policy,
            &ExecutionControl::default(),
            4096,
            required - 1,
        )
        .unwrap_err(),
        FullHeightScoreCellError::MemoryLimitExceeded {
            required_memory_bytes: required,
            max_memory_bytes: required - 1,
        }
    );
    let control = ExecutionControl::default();
    control.cancellation.handle().cancel();
    assert_eq!(
        FullHeightScoreCellMaterializer::materialize_with_memory_limit(
            &batch, policy, &control, 0, 0,
        )
        .unwrap_err(),
        FullHeightScoreCellError::Cancelled
    );
}
