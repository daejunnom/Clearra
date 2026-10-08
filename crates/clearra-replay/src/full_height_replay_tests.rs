use super::*;
use crate::{
    trace::solution_trace_builder::SolutionTraceBuilder, FullHeightExecutionBatch,
    FullHeightExecutionBatchError, ScoringExecutionNode, ScoringLockEvidence,
    SpinCoverageExecutionBatch, SpinCoverageExecutionGraph,
};
use clearra_geometry::layout::board64_layout::Board64Layout;

fn mask(cells: impl IntoIterator<Item = u16>) -> Board256Mask {
    cells.into_iter().fold(Board256Mask::EMPTY, |mask, cell| {
        mask.union(Board256Mask::singleton(cell).unwrap())
    })
}

fn edge(
    operation: u8,
    piece: PieceKind,
    rotation: RotationState,
    x: i8,
    y: i8,
    cleared: u8,
    pc: bool,
) -> ScoringExecutionEdge {
    ScoringExecutionEdge::new(
        u32::from(operation) + 1,
        operation,
        piece,
        rotation,
        x,
        y,
        cleared,
        0,
        0,
        ScoringLockEvidence::no_rotation(rotation),
    )
    .with_perfect_clear(pc)
}

#[test]
fn full_height_projection_preserves_word_crossings_and_top_row_clears() {
    for height in [7, 8, 12, 24] {
        let holes = mask([
            u16::from(height - 2) * 10 + 8,
            u16::from(height - 2) * 10 + 9,
            u16::from(height - 1) * 10 + 8,
            u16::from(height - 1) * 10 + 9,
        ]);
        let top_rows = Board256Mask::row(10, u16::from(height), u16::from(height - 2))
            .unwrap()
            .union(Board256Mask::row(10, u16::from(height), u16::from(height - 1)).unwrap());
        let initial = top_rows.without(holes).union(mask([1, 41]));
        let projected = FullHeightReplayProjector::project_lock(
            height,
            initial,
            PieceKind::O,
            RotationState::Zero,
            8,
            i32::from(height - 2),
        )
        .unwrap();
        assert_eq!(projected.placement(), holes);
        assert_eq!(
            projected.cleared_row_mask(),
            (1 << (height - 2)) | (1 << (height - 1))
        );
        assert_eq!(projected.after_line_clear(), mask([1, 41]));
        assert!(!projected.perfect_clear());
        assert_eq!(projected.cleared_lines(), 2);
    }
    // Row 6 straddles words 0/1. Clearing it shifts a high cell to row 5,
    // without shifting another low-word cell or discarding the high word.
    let initial = Board256Mask::row(10, 24, 6)
        .unwrap()
        .without(mask(64..68))
        .union(mask([3, 70, 239]));
    let projected = FullHeightReplayProjector::project_lock(
        24,
        initial,
        PieceKind::I,
        RotationState::Zero,
        4,
        6,
    )
    .unwrap();
    assert_eq!(projected.placement(), mask(64..68));
    assert_eq!(projected.cleared_row_mask(), 1 << 6);
    assert_eq!(projected.after_line_clear(), mask([3, 60, 229]));

    let initial = Board256Mask::row(10, 24, 1)
        .unwrap()
        .union(Board256Mask::row(10, 24, 3).unwrap())
        .without(mask([19, 39]))
        .union(mask([239]));
    let projected = FullHeightReplayProjector::project_lock(
        24,
        initial,
        PieceKind::I,
        RotationState::Right,
        9,
        0,
    )
    .unwrap();
    assert_eq!(projected.cleared_row_mask(), (1 << 1) | (1 << 3));
    assert_eq!(projected.after_line_clear(), mask([9, 19, 219]));
}

#[test]
fn full_height_projector_is_differentially_equal_to_unchanged_compact_transition() {
    let pieces = PieceKind::STANDARD_TETROMINOES;
    for height in 1..=6 {
        let layout = Board64Layout::standard_10_by_lines(height).unwrap();
        for piece in pieces {
            for rotation in RotationState::ALL {
                for x in 0..10 {
                    for y in 0..height as i8 {
                        for initial in [0, layout.all_cells_mask() & 0x01a4_1510_089a_18d4] {
                            let edge = edge(0, piece, rotation, x, y, 0, false);
                            let projected = FullHeightReplayProjector::project_lock(
                                height,
                                Board256Mask::from_words([initial, 0, 0, 0]),
                                piece,
                                rotation,
                                i32::from(x),
                                i32::from(y),
                            );
                            let (cleared, pc) = projected.as_ref().map_or((0, false), |value| {
                                (value.cleared_lines(), value.perfect_clear())
                            });
                            let corrected = ScoringExecutionEdge::new(
                                1,
                                0,
                                edge.piece(),
                                edge.rotation(),
                                x,
                                y,
                                cleared,
                                0,
                                0,
                                edge.lock_evidence(),
                            )
                            .with_perfect_clear(pc);
                            let compact = SolutionTraceBuilder::project_scoring_step(
                                layout, initial, corrected,
                            );
                            assert_eq!(
                                projected.as_ref().ok().map(|value| (
                                    value.placement().words()[0],
                                    value.after_line_clear().words()[0]
                                )),
                                compact
                            );
                            if let Ok(value) = projected {
                                assert_eq!(&value.placement().words()[1..], &[0; 3]);
                                assert_eq!(&value.after_line_clear().words()[1..], &[0; 3]);
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn full_height_projection_rejects_bad_layout_collision_and_false_clear_evidence() {
    assert_eq!(
        FullHeightReplayProjector::validate_board(0, Board256Mask::EMPTY),
        Err(FullHeightReplayError::InvalidHeight)
    );
    assert_eq!(
        FullHeightReplayProjector::validate_board(25, Board256Mask::EMPTY),
        Err(FullHeightReplayError::InvalidHeight)
    );
    assert_eq!(
        FullHeightReplayProjector::validate_board(24, mask([240])),
        Err(FullHeightReplayError::InvalidInitialBoard)
    );
    assert_eq!(
        FullHeightReplayProjector::project_lock(
            24,
            mask([239]),
            PieceKind::O,
            RotationState::Zero,
            8,
            22
        ),
        Err(FullHeightReplayError::Collision)
    );
    assert_eq!(
        FullHeightReplayProjector::project_lock(
            24,
            Board256Mask::EMPTY,
            PieceKind::O,
            RotationState::Zero,
            9,
            0
        ),
        Err(FullHeightReplayError::OutOfBounds)
    );
    assert_eq!(
        FullHeightReplayProjector::project_lock(
            24,
            Board256Mask::EMPTY,
            PieceKind::O,
            RotationState::Zero,
            i32::MAX,
            0
        ),
        Err(FullHeightReplayError::OutOfBounds)
    );
    assert_eq!(
        FullHeightReplayProjector::project_scoring_step(
            24,
            Board256Mask::EMPTY,
            edge(0, PieceKind::O, RotationState::Zero, 0, 22, 1, false)
        ),
        Err(FullHeightReplayError::ClearedLinesMismatch)
    );
    assert_eq!(
        FullHeightReplayProjector::project_scoring_step(
            24,
            Board256Mask::EMPTY,
            edge(0, PieceKind::O, RotationState::Zero, 0, 22, 0, true)
        ),
        Err(FullHeightReplayError::PerfectClearMismatch)
    );
}

fn build_trace(
    height: u8,
    initial: Board256Mask,
    path: &[(ScoringExecutionEdge, HoldDecision)],
) -> FullHeightReplayTrace {
    FullHeightReplayTrace::from_selected_path(
        height,
        initial,
        0,
        None,
        path,
        &ExecutionControl::default(),
        |_| Ok::<_, ()>(()),
    )
    .unwrap()
}

#[test]
fn full_height_trace_keeps_a_twenty_four_line_pc_and_entire_supply_chain() {
    let holes = mask((0..24).map(|row| row * 10));
    let initial = Board256Mask::all_cells(240).unwrap().without(holes);
    let path = (0..6)
        .map(|operation| {
            (
                edge(
                    operation,
                    PieceKind::I,
                    RotationState::Right,
                    0,
                    0,
                    4,
                    operation == 5,
                ),
                HoldDecision::None,
            )
        })
        .collect::<Vec<_>>();
    let trace = build_trace(24, initial, &path);
    assert_eq!(trace.steps().len(), 6);
    assert_eq!(trace.initial(), initial);
    assert!(trace.final_board().is_empty());
    assert_eq!(
        trace
            .steps()
            .iter()
            .map(|step| step.transition().cleared_lines() as usize)
            .sum::<usize>(),
        24
    );
    for (index, step) in trace.steps().iter().enumerate() {
        assert_eq!(step.decision().input_cursor(), index);
        assert_eq!(step.decision().output_cursor(), index + 1);
        assert_eq!(step.transition().cleared_row_mask(), 15);
        if index > 0 {
            assert_eq!(
                step.before(),
                trace.steps()[index - 1].transition().after_line_clear()
            );
        }
    }
    assert_eq!(
        trace.checked_nested_retained_bytes(),
        FullHeightReplayTrace::checked_step_buffer_bytes(trace.steps.capacity())
    );
}

#[test]
fn selected_hold_state_is_not_replaced_by_a_synthetic_step_cursor() {
    let path = [
        (
            edge(0, PieceKind::O, RotationState::Zero, 0, 0, 0, false),
            HoldDecision::StoreIncoming {
                stored_piece: PieceKind::I,
                drawn_piece: PieceKind::O,
            },
        ),
        (
            edge(1, PieceKind::I, RotationState::Zero, 3, 0, 0, false),
            HoldDecision::SwapWithHold {
                incoming_piece: PieceKind::T,
                held_piece: PieceKind::I,
            },
        ),
    ];
    let trace = FullHeightReplayTrace::from_selected_path(
        24,
        Board256Mask::EMPTY,
        7,
        None,
        &path,
        &ExecutionControl::default(),
        |_| Ok::<_, ()>(()),
    )
    .unwrap();
    assert_eq!(trace.steps()[0].decision().output_cursor(), 9);
    assert_eq!(trace.steps()[1].decision().input_cursor(), 9);
    assert_eq!(trace.steps()[1].decision().output_cursor(), 10);
    assert_eq!(
        trace.steps()[1].decision().output_hold_piece(),
        Some(PieceKind::T)
    );
    let mut key = String::new();
    trace.write_canonical_key(&mut key).unwrap();
    assert!(key.contains("aIi9o10ihIohTdswapTI"));
}

#[test]
fn full_height_trace_rejects_limits_cancel_supply_mismatch_and_repeated_operations() {
    use FullHeightReplayBuildError::*;
    let path = [(
        edge(0, PieceKind::O, RotationState::Zero, 0, 0, 0, false),
        HoldDecision::None,
    )];
    let limit = FullHeightReplayTrace::checked_step_buffer_bytes(1).unwrap() - 1;
    let error = FullHeightReplayTrace::from_selected_path(
        24,
        Board256Mask::EMPTY,
        0,
        None,
        &path,
        &ExecutionControl::default(),
        |bytes| if bytes > limit { Err("limit") } else { Ok(()) },
    )
    .unwrap_err();
    assert_eq!(error, MemoryGuard("limit"));
    let control = ExecutionControl::default();
    let cancelled = FullHeightReplayTrace::from_selected_path(
        24,
        Board256Mask::EMPTY,
        0,
        None,
        &path,
        &control,
        |_| {
            control.cancellation.handle().cancel();
            Ok::<_, ()>(())
        },
    )
    .unwrap_err();
    assert_eq!(cancelled, Replay(FullHeightReplayError::Cancelled));
    let wrong_hold = [(
        path[0].0,
        HoldDecision::SwapWithHold {
            incoming_piece: PieceKind::I,
            held_piece: PieceKind::O,
        },
    )];
    assert_eq!(
        FullHeightReplayTrace::from_selected_path(
            24,
            Board256Mask::EMPTY,
            0,
            None,
            &wrong_hold,
            &ExecutionControl::default(),
            |_| Ok::<_, ()>(())
        )
        .unwrap_err(),
        Replay(FullHeightReplayError::SupplyTransitionMismatch)
    );
    let duplicate = [
        path[0],
        (
            edge(0, PieceKind::O, RotationState::Zero, 3, 0, 0, false),
            HoldDecision::None,
        ),
    ];
    assert_eq!(
        FullHeightReplayTrace::from_selected_path(
            24,
            Board256Mask::EMPTY,
            0,
            None,
            &duplicate,
            &ExecutionControl::default(),
            |_| Ok::<_, ()>(())
        )
        .unwrap_err(),
        Replay(FullHeightReplayError::DuplicateOperation)
    );
}

#[test]
fn trk2_identity_distinguishes_high_words_initial_board_and_height() {
    let first = build_trace(
        24,
        mask([190]),
        &[(
            edge(0, PieceKind::O, RotationState::Zero, 0, 22, 0, false),
            HoldDecision::None,
        )],
    );
    let second = build_trace(
        24,
        mask([191]),
        &[(
            edge(0, PieceKind::O, RotationState::Zero, 0, 22, 0, false),
            HoldDecision::None,
        )],
    );
    let low = build_trace(
        24,
        mask([190]),
        &[(
            edge(0, PieceKind::O, RotationState::Zero, 0, 0, 0, false),
            HoldDecision::None,
        )],
    );
    let key = |trace: &FullHeightReplayTrace| {
        let mut text = String::new();
        trace.write_canonical_key(&mut text).unwrap();
        text
    };
    assert_ne!(key(&first), key(&second));
    assert_ne!(key(&first), key(&low));
    assert!(key(&first).starts_with("trk2:h24:"));
    assert!(!key(&first).starts_with("trk1:"));
}

fn full_batch(
    nodes: Vec<ScoringExecutionNode>,
    edges: Vec<ScoringExecutionEdge>,
) -> (Board256Mask, SpinCoverageExecutionBatch) {
    let holes = mask([220, 221, 230, 231]);
    let initial = Board256Mask::all_cells(240).unwrap().without(holes);
    let hex = |mask: Board256Mask| {
        let words = mask.words();
        format!(
            "{:016x}{:016x}{:016x}{:016x}",
            words[3], words[2], words[1], words[0]
        )
    };
    let key = format!(
        "ctk2|height=24|initial={}|placements=O:{}",
        hex(initial),
        hex(holes)
    );
    let graphs = vec![SpinCoverageExecutionGraph::new(7, key, 0, nodes, edges)];
    (
        initial,
        SpinCoverageExecutionBatch::new(
            vec![vec![PieceKind::O]],
            0,
            None,
            false,
            false,
            false,
            101,
            103,
            graphs,
            true,
        ),
    )
}

#[test]
fn full_height_batch_moves_existing_graph_storage_and_binds_all_four_initial_words() {
    let (initial, batch) = full_batch(
        vec![
            ScoringExecutionNode::new(0, 1, false),
            ScoringExecutionNode::new(1, 0, true),
        ],
        vec![edge(0, PieceKind::O, RotationState::Zero, 0, 22, 2, false)],
    );
    let graph_pointer = batch.graphs().as_ptr();
    let pattern_pointer = batch.patterns().as_ptr();
    let bytes = batch.checked_nested_retained_bytes();
    let clone_bytes = batch.checked_clone_nested_bytes();
    let bound = FullHeightExecutionBatch::from_spin_coverage(24, initial, batch).unwrap();
    assert_eq!(bound.initial(), initial);
    assert_eq!(bound.execution().graphs().as_ptr(), graph_pointer);
    assert_eq!(bound.execution().patterns().as_ptr(), pattern_pointer);
    assert_eq!(bound.checked_nested_retained_bytes(), bytes);
    assert_eq!(bound.checked_clone_nested_bytes(), clone_bytes);
    assert_eq!(
        bound.checked_clone_peak_bytes(),
        bytes.zip(clone_bytes).map(|(a, b)| a + b)
    );
    let mismatch = initial.without(mask([190]));
    assert_eq!(
        FullHeightExecutionBatch::from_spin_coverage(24, mismatch, bound.into_execution())
            .unwrap_err(),
        FullHeightExecutionBatchError::SnapshotMismatch
    );
}

#[test]
fn full_height_batch_refuses_malformed_spans_cycles_and_incomplete_candidate_partitions() {
    for (nodes, edges) in [
        (vec![ScoringExecutionNode::new(u32::MAX, 1, false)], vec![]),
        (
            vec![ScoringExecutionNode::new(0, 1, false)],
            vec![ScoringExecutionEdge::new(
                0,
                0,
                PieceKind::O,
                RotationState::Zero,
                0,
                22,
                2,
                0,
                0,
                ScoringLockEvidence::no_rotation(RotationState::Zero),
            )],
        ),
        (
            vec![
                ScoringExecutionNode::new(0, 1, true),
                ScoringExecutionNode::new(1, 0, true),
            ],
            vec![edge(0, PieceKind::O, RotationState::Zero, 0, 22, 2, false)],
        ),
    ] {
        let (initial, batch) = full_batch(nodes, edges);
        assert_eq!(
            FullHeightExecutionBatch::from_spin_coverage(24, initial, batch).unwrap_err(),
            FullHeightExecutionBatchError::InvalidGraph
        );
    }
    let batch = SpinCoverageExecutionBatch::new(
        vec![vec![PieceKind::O]],
        0,
        None,
        false,
        false,
        false,
        1,
        2,
        vec![SpinCoverageExecutionGraph::new(
            1,
            format!("ctk2|height=24|initial={}|placements=", "0".repeat(64)),
            0,
            vec![ScoringExecutionNode::new(0, 0, true)],
            vec![],
        )],
        true,
    );
    assert_eq!(
        FullHeightExecutionBatch::from_spin_coverage(24, Board256Mask::EMPTY, batch).unwrap_err(),
        FullHeightExecutionBatchError::InvalidCandidate
    );
}
