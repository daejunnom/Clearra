use super::*;
use crate::ExactScoringExecutionMaterializer;
use clearra_core_domain::{
    board::board_size::BoardSize,
    execution_cancellation::ExecutionCancellationToken,
    piece::{piece_kind::PieceKind, rotation::RotationState},
    solution::normalized_tiling_solution::{PiecePlacementMask, StandardBoard64TilingIdentity},
};
use clearra_geometry::layout::board64_layout::Board64Layout;
use clearra_replay::{ExactScoringExecutionGraph, ScoringExecutionNode, ScoringLockEvidence};

fn graph(paths: &[&[i8]], pieces: usize) -> ExactScoringExecutionGraph {
    let width = pieces * 2;
    let masks = (0..pieces).map(|i| {
        PiecePlacementMask::new(PieceKind::O, (3u64 << (i * 2)) | (3u64 << (width + i * 2)))
    });
    let identity = StandardBoard64TilingIdentity::from_placements(0, masks).unwrap();
    let mut nodes = vec![ScoringExecutionNode::new(0, paths.len() as u32, false)];
    let mut edges = Vec::new();
    for (path_index, path) in paths.iter().enumerate() {
        assert_eq!(path.len(), pieces);
        let first = 1 + path_index * pieces;
        edges.push(edge(first as u32, path[0], 0));
    }
    for (path_index, path) in paths.iter().enumerate() {
        let first = 1 + path_index * pieces;
        for depth in 1..pieces {
            nodes.push(ScoringExecutionNode::new(edges.len() as u32, 1, false));
            edges.push(edge(
                (first + depth) as u32,
                path[depth],
                if depth + 1 == pieces { 2 } else { 0 },
            ));
        }
        nodes.push(ScoringExecutionNode::new(edges.len() as u32, 0, true));
    }
    ExactScoringExecutionGraph::new(1, identity, 0, nodes, edges)
}
fn edge(to: u32, x: i8, clears: u8) -> ScoringExecutionEdge {
    ScoringExecutionEdge::new(
        to,
        0,
        PieceKind::O,
        RotationState::Zero,
        x,
        0,
        clears,
        0,
        0,
        ScoringLockEvidence::no_rotation(RotationState::Zero),
    )
}
fn batch(
    graphs: Vec<ExactScoringExecutionGraph>,
    pieces: usize,
    hold: bool,
) -> ExactScoringExecutionBatch {
    ExactScoringExecutionBatch::new(
        Board64Layout::new(BoardSize::new((pieces * 2) as u16, 2).unwrap()).unwrap(),
        0,
        vec![vec![PieceKind::O; pieces]],
        0,
        None,
        hold,
        false,
        false,
        1,
        1,
        graphs,
        true,
    )
}
fn limits(cap: usize) -> Limits {
    Limits::new(cap, 256, 64 * 1024 * 1024)
}
fn complete(session: &mut ExactReplayLanguageSession) -> Result<(), Error> {
    for _ in 0..100_000 {
        if session.advance(1, &ExecutionControl::default(), &mut |_| Ok(()))? {
            return Ok(());
        }
    }
    panic!("bounded fixture did not finish");
}
fn all(session: &ExactReplayLanguageSession) -> Vec<String> {
    (0..session.count().unwrap())
        .map(|rank| {
            session
                .select(rank, &ExecutionControl::default(), &mut |_| Ok(()))
                .unwrap()
                .trace_identity()
                .to_owned()
        })
        .collect()
}
fn oracle(batches: &[ExactScoringExecutionBatch]) -> Vec<String> {
    let mut keys = Vec::new();
    for batch in batches {
        for g in 0..batch.graphs().len() {
            let (result, _) =
                ExactScoringExecutionMaterializer::materialize_complete_replay_cell_with_limits(
                    batch,
                    g,
                    0,
                    &ExecutionControl::default(),
                    limits(100_000),
                )
                .unwrap();
            for aggregate in result.aggregates() {
                keys.extend(
                    aggregate
                        .executions()
                        .iter()
                        .map(|e| e.trace_identity().to_owned()),
                );
            }
        }
    }
    keys.sort();
    keys.dedup();
    keys
}
fn session(batches: Arc<[ExactScoringExecutionBatch]>, cap: usize) -> ExactReplayLanguageSession {
    let locations = batches
        .iter()
        .enumerate()
        .flat_map(|(b, batch)| {
            (0..batch.graphs().len()).map(move |g| ExactReplayGraphLocation { batch: b, graph: g })
        })
        .collect();
    ExactReplayLanguageSession::new(batches, locations, 0, limits(cap), &mut |_| Ok(())).unwrap()
}

#[test]
fn deterministic_nondeterministic_and_cross_location_union_match_exhaustive() {
    for hold in [false, true] {
        // Equal first label -> different hidden states, overlapping suffix, and
        // a disjoint suffix. Choosing one hidden successor loses a valid word.
        let source: Arc<[ExactScoringExecutionBatch]> = vec![batch(
            vec![
                graph(&[&[0, 2, 4], &[0, 4, 2], &[0, 2, 4]], 3),
                graph(&[&[0, 2, 4], &[2, 0, 4]], 3),
            ],
            3,
            hold,
        )]
        .into();
        let expected = oracle(&source);
        let mut counted = session(Arc::clone(&source), 100_000);
        complete(&mut counted).unwrap();
        assert_eq!(counted.count(), Some(expected.len()));
        assert_eq!(all(&counted), expected);
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
fn raw_caps_are_not_redefined_as_distinct_caps() {
    let source: Arc<[ExactScoringExecutionBatch]> =
        vec![batch(vec![graph(&[&[0, 2], &[0, 2]], 2)], 2, false)].into();
    let mut rejected = session(Arc::clone(&source), 1);
    assert_eq!(
        complete(&mut rejected),
        Err(Error::ExecutionLimitExceeded { max_executions: 1 })
    );
    assert_eq!(rejected.count(), None);
    let mut accepted = session(source, 2);
    complete(&mut accepted).unwrap();
    assert_eq!(accepted.count(), Some(1));
    let source: Arc<[ExactScoringExecutionBatch]> = vec![batch(
        vec![graph(&[&[0, 2]], 2), graph(&[&[0, 2], &[0, 2]], 2)],
        2,
        false,
    )]
    .into();
    let mut rejected = session(source, 2);
    assert_eq!(
        complete(&mut rejected),
        Err(Error::ExecutionLimitExceeded { max_executions: 2 })
    );
}

#[test]
fn malformed_unselected_transition_and_empty_pc_fail_closed() {
    for paths in [
        vec![&[0, 2][..], &[0, 0][..]],
        vec![&[0, 2][..], &[0, 3][..]],
    ] {
        let mut counted = session(vec![batch(vec![graph(&paths, 2)], 2, false)].into(), 100);
        assert_eq!(complete(&mut counted), Err(Error::InvalidEvidence));
        assert_eq!(counted.count(), None);
    }
    let identity = graph(&[&[0, 2]], 2).identity();
    for graph in [
        ExactScoringExecutionGraph::new(
            1,
            identity,
            0,
            vec![ScoringExecutionNode::new(0, 0, true)],
            vec![],
        ),
        ExactScoringExecutionGraph::new(
            1,
            identity,
            0,
            vec![ScoringExecutionNode::new(0, 1, false)],
            vec![edge(0, 0, 0)],
        ),
        ExactScoringExecutionGraph::new(
            1,
            identity,
            0,
            vec![ScoringExecutionNode::new(u32::MAX, 1, false)],
            vec![],
        ),
    ] {
        let mut counted = session(vec![batch(vec![graph], 2, false)].into(), 100);
        assert_eq!(complete(&mut counted), Err(Error::InvalidEvidence));
    }
}

#[test]
fn cancellation_and_every_guard_rejection_never_publish_a_count() {
    let source: Arc<[ExactScoringExecutionBatch]> =
        vec![batch(vec![graph(&[&[0, 2], &[2, 0]], 2)], 2, false)].into();
    let mut observed = session(Arc::clone(&source), 100);
    let mut checkpoints = 0;
    while !observed
        .advance(1, &ExecutionControl::default(), &mut |_| {
            checkpoints += 1;
            Ok(())
        })
        .unwrap()
    {}
    for reject in 0..checkpoints {
        let mut counted = session(Arc::clone(&source), 100);
        let mut calls = 0;
        loop {
            match counted.advance(1, &ExecutionControl::default(), &mut |peak| {
                let fail = calls == reject;
                calls += 1;
                if fail {
                    Err(Error::MemoryLimitExceeded {
                        required_memory_bytes: peak,
                        max_memory_bytes: peak.saturating_sub(1),
                    })
                } else {
                    Ok(())
                }
            }) {
                Err(Error::MemoryLimitExceeded { .. }) => break,
                Ok(false) => (),
                other => panic!("guard bypass {reject}: {other:?}"),
            }
        }
        assert_eq!(counted.count(), None);
    }
    let token = ExecutionCancellationToken::new();
    token.handle().cancel();
    let mut counted = session(source, 100);
    assert_eq!(
        counted.advance(1, &ExecutionControl::new(token), &mut |_| Ok(())),
        Err(Error::Cancelled)
    );
    assert_eq!(counted.count(), None);
}

#[test]
fn canonical_step_labels_keep_decimal_lexical_order() {
    let a = Label::new(10, edge(1, 0, 0), HoldDecision::None, 3).unwrap();
    let b = Label::new(2, edge(1, 0, 0), HoldDecision::None, 3).unwrap();
    assert!(a < b, "text i10 precedes text i2, not numeric tuple order");
    assert!(replay_projection(usize::MAX, usize::MAX).is_err());
}

#[test]
fn select_guard_and_cancel_preserve_completed_count_and_next_page() {
    let mut counted = session(
        vec![batch(vec![graph(&[&[0, 2], &[2, 0]], 2)], 2, false)].into(),
        100,
    );
    complete(&mut counted).unwrap();
    let expected = all(&counted);
    let mut peak = 0;
    counted
        .select(0, &ExecutionControl::default(), &mut |bytes| {
            peak = peak.max(bytes);
            Ok(())
        })
        .unwrap();
    assert!(matches!(
        counted.select(0, &ExecutionControl::default(), &mut |bytes| {
            if bytes >= peak {
                Err(Error::MemoryLimitExceeded {
                    required_memory_bytes: bytes,
                    max_memory_bytes: peak - 1,
                })
            } else {
                Ok(())
            }
        }),
        Err(Error::MemoryLimitExceeded { .. })
    ));
    assert_eq!(all(&counted), expected);
    let token = ExecutionCancellationToken::new();
    token.handle().cancel();
    assert_eq!(
        counted.select(0, &ExecutionControl::new(token), &mut |_| Ok(())),
        Err(Error::Cancelled)
    );
    assert_eq!(all(&counted), expected);
}

#[test]
fn merged_suffix_keeps_distinct_prefix_multiplicity() {
    let identity = graph(&[&[0, 2]], 2).identity();
    let graph = ExactScoringExecutionGraph::new(
        1,
        identity,
        0,
        vec![
            ScoringExecutionNode::new(0, 2, false),
            ScoringExecutionNode::new(2, 1, false),
            ScoringExecutionNode::new(3, 1, false),
            ScoringExecutionNode::new(4, 0, true),
        ],
        vec![edge(1, 0, 0), edge(2, 2, 0), edge(3, 2, 2), edge(3, 0, 2)],
    );
    let source: Arc<[ExactScoringExecutionBatch]> = vec![batch(vec![graph], 2, false)].into();
    let expected = oracle(&source);
    let mut counted = session(source, 100);
    complete(&mut counted).unwrap();
    assert_eq!(counted.count(), Some(2));
    assert!(
        counted.fast,
        "unique visible successors certify the direct DAG route"
    );
    assert_eq!(all(&counted), expected);
}

#[test]
fn duplicate_same_destination_is_one_word_but_two_raw_paths() {
    let identity = graph(&[&[0, 2]], 2).identity();
    let graph = ExactScoringExecutionGraph::new(
        1,
        identity,
        0,
        vec![
            ScoringExecutionNode::new(0, 2, false),
            ScoringExecutionNode::new(2, 1, false),
            ScoringExecutionNode::new(3, 0, true),
        ],
        vec![edge(1, 0, 0), edge(1, 0, 0), edge(2, 2, 2)],
    );
    let source: Arc<[ExactScoringExecutionBatch]> = vec![batch(vec![graph], 2, false)].into();
    let mut counted = session(Arc::clone(&source), 2);
    complete(&mut counted).unwrap();
    assert!(!counted.fast);
    assert_eq!(counted.count(), Some(1));
    assert_eq!(all(&counted), oracle(&source));
    for (id, node) in counted.nfa.iter().enumerate() {
        assert!(node.edges.iter().all(|e| e.destination > id));
    }
    for (id, node) in counted.dfa.iter().enumerate() {
        assert!(node.edges.iter().all(|e| e.destination > id));
    }
}

#[test]
fn separate_pattern_ids_are_not_deduplicated_together() {
    let original = batch(vec![graph(&[&[0, 2], &[2, 0]], 2)], 2, false);
    let source: Arc<[ExactScoringExecutionBatch]> = vec![ExactScoringExecutionBatch::new(
        original.layout(),
        0,
        vec![vec![PieceKind::O; 2], vec![PieceKind::O; 2]],
        0,
        None,
        false,
        false,
        false,
        1,
        1,
        original.graphs().to_vec(),
        true,
    )]
    .into();
    let mut first = session(Arc::clone(&source), 100);
    complete(&mut first).unwrap();
    let mut second = ExactReplayLanguageSession::new(
        source,
        vec![ExactReplayGraphLocation { batch: 0, graph: 0 }],
        1,
        limits(100),
        &mut |_| Ok(()),
    )
    .unwrap();
    complete(&mut second).unwrap();
    assert_eq!(first.count(), second.count());
    assert_eq!(
        first
            .select(0, &ExecutionControl::default(), &mut |_| Ok(()))
            .unwrap()
            .pattern_id(),
        0
    );
    assert_eq!(
        second
            .select(0, &ExecutionControl::default(), &mut |_| Ok(()))
            .unwrap()
            .pattern_id(),
        1
    );
}

#[test]
fn actual_supply_and_synthetic_visible_hold_projection_match_all_hold_variants() {
    let original = batch(vec![graph(&[&[0, 2], &[2, 0]], 2)], 2, true);
    for (queue_len, lookahead, bag) in [(3, false, false), (2, true, false), (2, true, true)] {
        let source: Arc<[ExactScoringExecutionBatch]> = vec![ExactScoringExecutionBatch::new(
            original.layout(),
            0,
            vec![vec![PieceKind::O; queue_len]],
            0,
            None,
            true,
            lookahead,
            bag,
            1,
            1,
            original.graphs().to_vec(),
            true,
        )]
        .into();
        let expected = oracle(&source);
        let mut counted = session(source, 100);
        complete(&mut counted).unwrap();
        let actual = all(&counted);
        assert_eq!(actual, expected);
        assert!(actual.iter().all(|key| key.contains("ihnoneohnone")));
        if lookahead {
            assert!(actual.iter().any(|key| key.contains("terminalO")));
        } else {
            assert!(actual.iter().any(|key| key.contains("swapOO")));
            assert!(actual.iter().any(|key| key.contains("storeOO")));
        }
    }
}

#[test]
fn raw_count_overflow_is_checked_without_enumerating_exponentially_many_paths() {
    let depth = usize::BITS as usize;
    let identity = StandardBoard64TilingIdentity::from_placements(
        0,
        [PiecePlacementMask::new(PieceKind::O, 15)],
    )
    .unwrap();
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    for step in 0..depth {
        nodes.push(ScoringExecutionNode::new(edges.len() as u32, 2, false));
        edges.push(edge((step + 1) as u32, 0, 2));
        edges.push(edge((step + 1) as u32, 0, 2));
    }
    nodes.push(ScoringExecutionNode::new(edges.len() as u32, 0, true));
    let graph = ExactScoringExecutionGraph::new(1, identity, 0, nodes, edges);
    let source: Arc<[ExactScoringExecutionBatch]> = vec![ExactScoringExecutionBatch::new(
        Board64Layout::new(BoardSize::new(2, 2).unwrap()).unwrap(),
        0,
        vec![vec![PieceKind::O; depth]],
        0,
        None,
        false,
        false,
        false,
        1,
        1,
        vec![graph],
        true,
    )]
    .into();
    let mut counted = session(source, usize::MAX);
    assert_eq!(complete(&mut counted), Err(Error::ProjectionOverflow));
    assert_eq!(counted.count(), None);
}
