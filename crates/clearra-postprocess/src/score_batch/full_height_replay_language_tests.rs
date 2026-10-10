//! Bounded, deliberately branching input DAGs are compared with an exhaustive
//! selected-path oracle. These are language tests, not PC family qualification.
use super::*;
use clearra_core_domain::{
    board::standard_pc_board::Board256Mask,
    execution_cancellation::ExecutionCancellationToken,
    piece::{piece_kind::PieceKind, rotation::RotationState},
};
use clearra_replay::{
    FullHeightReplayProjector, FullHeightReplayTrace, ScoringExecutionNode, ScoringLockEvidence,
    SpinCoverageExecutionBatch, SpinCoverageExecutionGraph,
};
use std::collections::BTreeSet;

fn hex(mask: Board256Mask) -> String {
    let [a, b, c, d] = mask.words();
    format!("{d:016x}{c:016x}{b:016x}{a:016x}")
}

fn fixture(
    height: u8,
    orders: &[Vec<usize>],
) -> (FullHeightExecutionBatch, Vec<Vec<ScoringExecutionEdge>>) {
    let count = usize::from(height.div_ceil(4));
    let groups: Vec<_> = (0..count)
        .map(|index| {
            let start = (index * 4).min(usize::from(height) - 4) as u8;
            let mask = (start..start + 4).fold(Board256Mask::EMPTY, |board, row| {
                board.union(Board256Mask::singleton(u16::from(row) * 10 + index as u16).unwrap())
            });
            (start, index as i8, mask)
        })
        .collect();
    let holes = groups
        .iter()
        .fold(Board256Mask::EMPTY, |board, &(_, _, mask)| {
            board.union(mask)
        });
    let initial = Board256Mask::all_cells(u16::from(height) * 10)
        .unwrap()
        .without(holes);
    let mut sorted_masks: Vec<_> = groups.iter().map(|group| group.2).collect();
    sorted_masks.sort();
    let key = format!(
        "ctk2|height={height}|initial={}|placements={}",
        hex(initial),
        sorted_masks
            .iter()
            .map(|&mask| format!("I:{}", hex(mask)))
            .collect::<Vec<_>>()
            .join(",")
    );
    let mut paths = Vec::new();
    for (path_index, order) in orders.iter().enumerate() {
        assert_eq!(order.len(), count);
        let mut original_rows: Vec<_> = (0..height).collect();
        let mut board = initial;
        let mut path = Vec::new();
        for (depth, &index) in order.iter().enumerate() {
            let (start, column, mask) = groups[index];
            let y = original_rows.iter().position(|&row| row == start).unwrap() as i8;
            let transition = FullHeightReplayProjector::project_lock(
                height,
                board,
                PieceKind::I,
                RotationState::Right,
                i32::from(column),
                i32::from(y),
            )
            .unwrap();
            let operation = sorted_masks
                .iter()
                .position(|&candidate| candidate == mask)
                .unwrap() as u8;
            let to = (1 + path_index * count + depth) as u32;
            path.push(
                ScoringExecutionEdge::new(
                    to,
                    operation,
                    PieceKind::I,
                    RotationState::Right,
                    column,
                    y,
                    transition.cleared_lines(),
                    0,
                    0,
                    ScoringLockEvidence::no_rotation(RotationState::Right),
                )
                .with_perfect_clear(transition.perfect_clear()),
            );
            original_rows = original_rows
                .into_iter()
                .enumerate()
                .filter_map(|(row, original)| {
                    (transition.cleared_row_mask() & (1 << row) == 0).then_some(original)
                })
                .collect();
            board = transition.after_line_clear();
        }
        assert!(board.is_empty());
        paths.push(path);
    }
    let mut nodes = vec![ScoringExecutionNode::new(0, paths.len() as u32, false)];
    let mut edges: Vec<_> = paths.iter().map(|path| path[0]).collect();
    for path in &paths {
        for &edge in &path[1..] {
            nodes.push(ScoringExecutionNode::new(edges.len() as u32, 1, false));
            edges.push(edge);
        }
        nodes.push(ScoringExecutionNode::new(edges.len() as u32, 0, true));
    }
    let graph = SpinCoverageExecutionGraph::new(1, key, 0, nodes, edges);
    let batch = FullHeightExecutionBatch::from_spin_coverage(
        height,
        initial,
        SpinCoverageExecutionBatch::new(
            vec![vec![PieceKind::I; count]],
            0,
            None,
            false,
            false,
            false,
            101,
            103,
            vec![graph],
            true,
        ),
    )
    .unwrap();
    (batch, paths)
}

fn session(
    batches: Arc<[FullHeightExecutionBatch]>,
    pattern: usize,
    limits: Limits,
) -> FullHeightReplayLanguageSession {
    let locations = batches
        .iter()
        .enumerate()
        .flat_map(|(batch, source)| {
            (0..source.execution().graphs().len())
                .map(move |graph| ExactReplayGraphLocation { batch, graph })
        })
        .collect();
    FullHeightReplayLanguageSession::new(batches, locations, pattern, limits, &mut |_| Ok(()))
        .unwrap()
}
fn limits() -> Limits {
    Limits::new(100_000, 60, 16 * 1024 * 1024)
}
fn complete(session: &mut FullHeightReplayLanguageSession) -> Result<(), Error> {
    for _ in 0..100_000 {
        if session.advance(1, &ExecutionControl::default(), &mut |_| Ok(()))? {
            return Ok(());
        }
    }
    panic!("bounded language fixture did not finish");
}
fn all(session: &FullHeightReplayLanguageSession) -> Vec<String> {
    (0..session.count().unwrap())
        .map(|rank| {
            let member = session
                .select(rank, &ExecutionControl::default(), &mut |_| Ok(()))
                .unwrap();
            assert!(member.replay_trace().final_board().is_empty());
            member.trace_identity().to_owned()
        })
        .collect()
}
fn oracle(batch: &FullHeightExecutionBatch, paths: &[Vec<ScoringExecutionEdge>]) -> Vec<String> {
    let mut identities = BTreeSet::new();
    for path in paths {
        let path: Vec<_> = path
            .iter()
            .map(|&edge| (edge, HoldDecision::None))
            .collect();
        let trace = FullHeightReplayTrace::from_selected_path(
            batch.height(),
            batch.initial(),
            0,
            None,
            &path,
            &ExecutionControl::default(),
            |_| Ok::<_, Error>(()),
        )
        .unwrap();
        let mut identity = String::new();
        trace.write_canonical_key(&mut identity).unwrap();
        assert_eq!(
            trace.checked_canonical_key_requested_bytes(),
            Some(identity.len() as u128)
        );
        identities.insert(identity);
    }
    identities.into_iter().collect()
}
fn reheader(
    batch: &FullHeightExecutionBatch,
    execution: SpinCoverageExecutionBatch,
) -> FullHeightExecutionBatch {
    FullHeightExecutionBatch::from_spin_coverage(batch.height(), batch.initial(), execution)
        .unwrap()
}

#[test]
fn high_rows_and_all_four_words_match_exhaustive_keys_without_a_low_word_conversion() {
    for height in [7_u8, 8, 12, 24] {
        let count = usize::from(height.div_ceil(4));
        let orders = vec![
            (0..count).collect(),
            (0..count).rev().collect(),
            (0..count).collect(),
        ];
        let (batch, paths) = fixture(height, &orders);
        assert_ne!(batch.initial().words()[1], 0);
        if height == 24 {
            assert_ne!(batch.initial().words()[3], 0);
        }
        let expected = oracle(&batch, &paths);
        assert_eq!(expected.len(), 2);
        let mut counted = session(vec![batch].into(), 0, limits());
        complete(&mut counted).unwrap();
        assert_eq!(counted.count(), Some(expected.len()));
        assert_eq!(all(&counted), expected);
        assert!(
            !counted.inner.fast,
            "duplicate visible labels must take subset union"
        );
        assert!(counted
            .select(
                expected.len(),
                &ExecutionControl::default(),
                &mut |_| Ok(())
            )
            .is_err());
    }
}

#[test]
fn equal_prefixes_and_separate_graph_locations_union_suffixes_not_raw_paths() {
    let (batch, paths) = fixture(
        12,
        &[vec![0, 1, 2], vec![0, 2, 1], vec![0, 1, 2], vec![2, 1, 0]],
    );
    let expected = oracle(&batch, &paths);
    assert_eq!(expected.len(), 3);
    let graph = batch.execution().graphs()[0].clone();
    let repeated = reheader(
        &batch,
        SpinCoverageExecutionBatch::new(
            batch.execution().patterns().to_vec(),
            0,
            None,
            false,
            false,
            false,
            101,
            103,
            vec![graph.clone(), graph],
            true,
        ),
    );
    let mut counted = session(vec![batch, repeated].into(), 0, limits());
    complete(&mut counted).unwrap();
    assert_eq!(counted.count(), Some(3));
    assert_eq!(all(&counted), expected);
}

#[test]
fn nondefault_cursor_hold_and_pattern_identity_are_real_supply_evidence() {
    let (original, paths) = fixture(8, &[vec![0, 1]]);
    let mut pattern = vec![PieceKind::O];
    pattern.extend([PieceKind::I; 2]);
    let batch = reheader(
        &original,
        SpinCoverageExecutionBatch::new(
            vec![pattern.clone(), pattern],
            1,
            Some(PieceKind::I),
            true,
            false,
            false,
            101,
            103,
            original.execution().graphs().to_vec(),
            true,
        ),
    );
    let mut expected = BTreeSet::new();
    for decisions in 0..4 {
        let path: Vec<_> = paths[0]
            .iter()
            .enumerate()
            .map(|(index, &edge)| {
                (
                    edge,
                    if decisions & (1 << index) == 0 {
                        HoldDecision::None
                    } else {
                        HoldDecision::SwapWithHold {
                            incoming_piece: PieceKind::I,
                            held_piece: PieceKind::I,
                        }
                    },
                )
            })
            .collect();
        let trace = FullHeightReplayTrace::from_selected_path(
            8,
            batch.initial(),
            1,
            Some(PieceKind::I),
            &path,
            &ExecutionControl::default(),
            |_| Ok::<_, Error>(()),
        )
        .unwrap();
        let mut key = String::new();
        trace.write_canonical_key(&mut key).unwrap();
        expected.insert(key);
    }
    for pattern_id in 0..2 {
        let mut counted = session(vec![batch.clone()].into(), pattern_id, limits());
        complete(&mut counted).unwrap();
        assert_eq!(all(&counted), expected.iter().cloned().collect::<Vec<_>>());
        for rank in 0..4 {
            let member = counted
                .select(rank, &ExecutionControl::default(), &mut |_| Ok(()))
                .unwrap();
            assert_eq!(member.pattern_id(), pattern_id);
            assert_eq!(
                member.replay_trace().steps()[0].decision().input_cursor(),
                1
            );
        }
    }
}

#[test]
fn malformed_clears_duplicate_operations_and_incomplete_batches_publish_no_count() {
    let (batch, paths) = fixture(8, &[vec![0, 1]]);
    let source = batch.execution();
    for failure in 0..3 {
        let mut path = paths[0].clone();
        if failure < 2 {
            let index = if failure == 0 { 0 } else { 1 };
            let old = path[index];
            path[index] = ScoringExecutionEdge::new(
                old.to(),
                if failure == 1 {
                    path[0].operation_index()
                } else {
                    old.operation_index()
                },
                old.piece(),
                old.rotation(),
                old.x(),
                old.y(),
                if failure == 0 { 3 } else { old.cleared_lines() },
                0,
                0,
                ScoringLockEvidence::no_rotation(old.rotation()),
            )
            .with_perfect_clear(old.perfect_clear());
        }
        let graph = SpinCoverageExecutionGraph::new(
            1,
            source.graphs()[0].candidate_key(),
            0,
            vec![
                ScoringExecutionNode::new(0, 1, false),
                ScoringExecutionNode::new(1, 1, false),
                ScoringExecutionNode::new(2, 0, true),
            ],
            path,
        );
        let malformed = reheader(
            &batch,
            SpinCoverageExecutionBatch::new(
                source.patterns().to_vec(),
                0,
                None,
                false,
                false,
                false,
                101,
                103,
                vec![graph],
                failure != 2,
            ),
        );
        let mut counted = session(vec![malformed].into(), 0, limits());
        assert_eq!(complete(&mut counted), Err(Error::InvalidEvidence));
        assert_eq!(counted.count(), None);
    }
}

#[test]
fn a_profile_or_supply_snapshot_mismatch_cannot_share_one_count() {
    let (batch, _) = fixture(8, &[vec![0, 1]]);
    let source = batch.execution();
    for mismatch in 0..3 {
        let other = reheader(
            &batch,
            SpinCoverageExecutionBatch::new(
                source.patterns().to_vec(),
                if mismatch == 2 { 1 } else { 0 },
                None,
                false,
                false,
                false,
                if mismatch == 0 { 201 } else { 101 },
                if mismatch == 1 { 203 } else { 103 },
                source.graphs().to_vec(),
                true,
            ),
        );
        let mut counted = session(vec![batch.clone(), other].into(), 0, limits());
        assert_eq!(complete(&mut counted), Err(Error::InvalidEvidence));
        assert_eq!(counted.count(), None);
    }
}

#[test]
fn cancelling_or_rejecting_an_admission_never_leaves_a_partial_count_or_member() {
    let (batch, _) = fixture(8, &[vec![0, 1], vec![1, 0]]);
    let mut counted = session(vec![batch.clone()].into(), 0, limits());
    let token = ExecutionCancellationToken::default();
    token.handle().cancel();
    assert_eq!(
        counted.advance(1, &ExecutionControl::new(token), &mut |_| Ok(())),
        Err(Error::Cancelled)
    );
    assert_eq!(counted.count(), None);
    let mut counted = session(vec![batch.clone()].into(), 0, limits());
    assert!(counted
        .advance(1, &ExecutionControl::default(), &mut |_| Err(
            Error::AllocationFailed
        ))
        .is_err());
    assert_eq!(counted.count(), None);
    let mut counted = session(vec![batch].into(), 0, limits());
    complete(&mut counted).unwrap();
    let mut peak = 0;
    let member = counted
        .select(0, &ExecutionControl::default(), &mut |bytes| {
            peak = peak.max(bytes);
            Ok(())
        })
        .unwrap();
    assert!(
        peak > counted.checked_retained_bytes().unwrap()
            + member.checked_nested_retained_bytes().unwrap()
    );
    let rejected = counted.select(0, &ExecutionControl::default(), &mut |bytes| {
        if bytes >= peak {
            Err(Error::MemoryLimitExceeded {
                required_memory_bytes: bytes,
                max_memory_bytes: peak - 1,
            })
        } else {
            Ok(())
        }
    });
    assert!(matches!(rejected, Err(Error::MemoryLimitExceeded { .. })));
    assert_eq!(
        counted.count(),
        Some(2),
        "a rejected selection cannot corrupt the completed source"
    );
    assert_eq!(all(&counted).len(), 2);
}

#[test]
fn the_full_height_language_uses_the_same_exact_execution_and_step_caps() {
    let (batch, _) = fixture(12, &[vec![0, 1, 2], vec![2, 1, 0]]);
    let mut capped = session(
        vec![batch.clone()].into(),
        0,
        Limits::new(1, 60, 16 * 1024 * 1024),
    );
    assert!(matches!(
        complete(&mut capped),
        Err(Error::ExecutionLimitExceeded { .. })
    ));
    assert_eq!(capped.count(), None);
    let mut short = session(vec![batch].into(), 0, Limits::new(100, 2, 16 * 1024 * 1024));
    assert!(matches!(
        complete(&mut short),
        Err(Error::PathStepLimitExceeded { .. })
    ));
    assert_eq!(short.count(), None);
}
