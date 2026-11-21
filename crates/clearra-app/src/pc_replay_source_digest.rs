//! Versioned identity of an immutable, query-bound replay source, not a hash of
//! its exponentially large trace stream. Counting/proof and ownership remain
//! with the replay session; a matching digest does not admit unvalidated input.

use clearra_core_domain::piece::{
    piece_kind::PieceKind, standard_bag_ordinals::StandardBagOrdinals,
};
use clearra_problem::SearchProblem;
use clearra_replay::{
    ExactScoringExecutionBatch, ExactScoringExecutionGraph, FullHeightExecutionBatch,
    RotationRequest, ScoringExecutionEdge, SpinCoverageExecutionGraph,
};
use sha2::{Digest, Sha256};

const INVALID_SOURCE: &str = "complete_replay_source_digest_invalid";
const BAG_LENGTH: usize = PieceKind::STANDARD_TETROMINOES.len();
const MAX_TAIL_LENGTH: usize = 4;
const MAX_TAIL_COUNT: usize = 7 * 6 * 5 * 4;

/// The caller has already established the typed result/problem authority.
/// This additional check binds the projection and ordered immutable storage.
/// No trace, temporary String, sequence cache or count-sized scratch is built.
/// The returned state is extended with the exact ordered manifest by its owner.
pub(crate) fn pc_replay_source_hasher(
    problem: &SearchProblem,
    batches: &[ExactScoringExecutionBatch],
) -> Result<Sha256, &'static str> {
    let mut hash = Sha256::new();
    // Real supply changes canonical rank and witness bytes. Do not reuse a
    // source/page identity minted for the old synthetic cursor/hold projection.
    hash.update(b"clearra.pc-replay-source.v3\0");
    bytes(
        &mut hash,
        b"canonical-projection:trk1/request-bound-cursor-hold/v2",
    )?;
    bytes(
        &mut hash,
        b"count:distinct-visible-language/lexical-rank/v1",
    )?;
    hash_problem_context(&mut hash, problem, false)?;

    bytes(&mut hash, b"ordered-batches")?;
    count(&mut hash, batches.len())?;
    let supply = problem.supply();
    let expected_ordinals = problem
        .piece_source()
        .materialized_universe()
        .and_then(|universe| universe.complete_standard_bag_ordinals());
    for (index, batch) in batches.iter().enumerate() {
        if !batch.complete()
            || batch.initial_occupied() != problem.initial_board().occupied_mask()
            || batch.initial_cursor() != problem.initial_hold().cursor()
            || batch.initial_hold() != problem.initial_hold().hold_piece()
            || batch.hold_enabled() != supply.hold_enabled()
            || batch.projects_unplaced_lookahead() != supply.projects_unplaced_lookahead()
            || batch.projects_standard_bag_lookahead() != supply.projects_standard_bag_lookahead()
            || batch.layout().width() != problem.initial_board().width()
            || batch.layout().height() != problem.visible_height()
            || batch
                .standard_bag_ordinals()
                .is_some_and(|ordinals| expected_ordinals != Some(ordinals))
            || (index != 0 && !same_compact_patterns(&batches[0], batch))
        {
            return Err(INVALID_SOURCE);
        }
        hash_batch(&mut hash, batch)?;
    }
    bytes(&mut hash, b"ordered-exact-manifest")?;
    Ok(hash)
}

fn same_compact_patterns(
    left: &ExactScoringExecutionBatch,
    right: &ExactScoringExecutionBatch,
) -> bool {
    if left.pattern_count() != right.pattern_count() {
        return false;
    }
    if let (Some(left), Some(right)) = (left.standard_bag_ordinals(), right.standard_bag_ordinals())
    {
        return left == right;
    }
    if let (Some(left), Some(right)) = (left.dense_patterns(), right.dense_patterns()) {
        return left == right;
    }
    // A union may include one source of each representation. Equal counts
    // are insufficient; the exact queue at every original ordinal must agree.
    (0..left.pattern_count()).all(|id| match (left.pattern_at(id), right.pattern_at(id)) {
        (Some(left), Some(right)) => left.as_ref() == right.as_ref(),
        _ => false,
    })
}

fn hash_problem_context(
    hash: &mut Sha256,
    problem: &SearchProblem,
    full_height: bool,
) -> Result<(), &'static str> {
    bytes(hash, problem.problem_id().as_str().as_bytes())?;
    bytes(hash, problem.board_profile().id().as_str().as_bytes())?;
    if full_height {
        for word in problem.initial_board().occupied_words() {
            hash.update(word.to_le_bytes());
        }
    } else {
        hash.update(problem.initial_board().occupied_mask().to_le_bytes());
    }
    hash.update(problem.visible_height().to_le_bytes());
    hash.update(problem.search_height().to_le_bytes());
    hash.update(problem.initial_hold().cursor().to_le_bytes());
    hash.update([optional_piece(problem.initial_hold().hold_piece())]);
    let supply = problem.supply();
    hash.update([
        u8::from(supply.hold_enabled()),
        u8::from(supply.projects_unplaced_lookahead()),
        u8::from(supply.projects_standard_bag_lookahead()),
    ]);
    count(hash, supply.source_sequence_length())?;
    bytes(hash, supply.queue_mode().as_bytes())?;
    bytes(hash, supply.supply_window_resolution().as_bytes())?;
    hash.update(problem.piece_source().id().get().to_le_bytes());
    bytes(hash, problem.rule_profile_value().id().as_str().as_bytes())?;
    let kicks = problem.kick_profile();
    bytes(hash, kicks.profile_id().as_str().as_bytes())?;
    bytes(hash, kicks.source_rule().as_str().as_bytes())?;
    hash.update([u8::from(kicks.verified()), u8::from(kicks.supports_180())]);
    count(hash, kicks.transition_count())?;
    let spawn = problem.spawn_profile();
    bytes(hash, spawn.id().as_str().as_bytes())?;
    hash.update(spawn.x().to_le_bytes());
    hash.update(spawn.y().to_le_bytes());

    Ok(())
}

/// Distinct identity schema for four-word physical replay. The compact v3
/// byte stream stays unchanged and cannot alias this source's trk2/ranks.
pub(crate) fn full_height_pc_replay_source_hasher(
    problem: &SearchProblem,
    batches: &[FullHeightExecutionBatch],
) -> Result<Sha256, &'static str> {
    if !(7..=24).contains(&problem.visible_height()) {
        return Err(INVALID_SOURCE);
    }
    let mut hash = Sha256::new();
    hash.update(b"clearra.pc-replay-source.v4/full-height\0");
    bytes(
        &mut hash,
        b"canonical-projection:trk2/four-word/request-bound-cursor-hold/v1",
    )?;
    bytes(
        &mut hash,
        b"count:distinct-visible-language/lexical-rank/v1",
    )?;
    hash_problem_context(&mut hash, problem, true)?;
    bytes(&mut hash, b"ordered-batches")?;
    count(&mut hash, batches.len())?;
    let supply = problem.supply();
    for batch in batches {
        let execution = batch.execution();
        if !execution.complete()
            || batch.initial().words() != problem.initial_board().occupied_words()
            || u16::from(batch.height()) != problem.visible_height()
            || execution.initial_cursor() != problem.initial_hold().cursor()
            || execution.initial_hold() != problem.initial_hold().hold_piece()
            || execution.hold_enabled() != supply.hold_enabled()
            || execution.projects_unplaced_lookahead() != supply.projects_unplaced_lookahead()
            || execution.projects_standard_bag_lookahead()
                != supply.projects_standard_bag_lookahead()
            || batches
                .first()
                .is_some_and(|first| execution.patterns() != first.execution().patterns())
        {
            return Err(INVALID_SOURCE);
        }
        hash.update([batch.height()]);
        for word in batch.initial().words() {
            hash.update(word.to_le_bytes());
        }
        hash.update(execution.initial_cursor().to_le_bytes());
        hash.update([
            optional_piece(execution.initial_hold()),
            u8::from(execution.hold_enabled()),
            u8::from(execution.projects_unplaced_lookahead()),
            u8::from(execution.projects_standard_bag_lookahead()),
            u8::from(execution.complete()),
        ]);
        hash.update(execution.kick_table_id().to_le_bytes());
        hash.update(execution.rule_profile_id().to_le_bytes());
        count(&mut hash, execution.patterns().len())?;
        for pattern in execution.patterns() {
            count(&mut hash, pattern.len())?;
            for piece in pattern {
                hash.update([piece.as_ascii() as u8]);
            }
        }
        count(&mut hash, execution.graphs().len())?;
        for graph in execution.graphs() {
            hash_full_height_graph(&mut hash, graph)?;
        }
    }
    bytes(&mut hash, b"ordered-exact-manifest")?;
    Ok(hash)
}

fn hash_full_height_graph(
    hash: &mut Sha256,
    graph: &SpinCoverageExecutionGraph,
) -> Result<(), &'static str> {
    let nodes = u32::try_from(graph.node_count()).map_err(|_| INVALID_SOURCE)?;
    if graph.root() >= nodes {
        return Err(INVALID_SOURCE);
    }
    hash.update(b"full-height-graph\0");
    hash.update(graph.candidate_id().to_le_bytes());
    bytes(hash, graph.candidate_key().as_bytes())?;
    hash.update(graph.root().to_le_bytes());
    hash.update(nodes.to_le_bytes());
    for index in 0..nodes {
        let node = graph.node(index).ok_or(INVALID_SOURCE)?;
        graph.checked_edges(node).ok_or(INVALID_SOURCE)?;
        hash.update(node.edge_start().to_le_bytes());
        hash.update(node.edge_count().to_le_bytes());
        hash.update([u8::from(node.accepting())]);
    }
    count(hash, graph.all_edges().len())?;
    for edge in graph.all_edges() {
        hash_edge(hash, *edge);
    }
    Ok(())
}

fn count(hash: &mut Sha256, value: usize) -> Result<(), &'static str> {
    hash.update(
        u64::try_from(value)
            .map_err(|_| INVALID_SOURCE)?
            .to_le_bytes(),
    );
    Ok(())
}

fn bytes(hash: &mut Sha256, value: &[u8]) -> Result<(), &'static str> {
    count(hash, value.len())?;
    hash.update(value);
    Ok(())
}

fn optional_piece(piece: Option<PieceKind>) -> u8 {
    piece.map_or(0, |piece| piece.as_ascii() as u8)
}

fn hash_batch(hash: &mut Sha256, batch: &ExactScoringExecutionBatch) -> Result<(), &'static str> {
    hash.update(b"batch\0");
    hash.update(batch.layout().width().to_le_bytes());
    hash.update(batch.layout().height().to_le_bytes());
    hash.update(batch.initial_occupied().to_le_bytes());
    hash.update(batch.initial_cursor().to_le_bytes());
    hash.update([
        optional_piece(batch.initial_hold()),
        u8::from(batch.hold_enabled()),
        u8::from(batch.projects_unplaced_lookahead()),
        u8::from(batch.projects_standard_bag_lookahead()),
        u8::from(batch.complete()),
    ]);
    hash.update(batch.kick_table_id().to_le_bytes());
    hash.update(batch.rule_profile_id().to_le_bytes());
    count(hash, batch.pattern_count())?;
    if let Some(patterns) = batch.dense_patterns() {
        for pattern in patterns {
            count(hash, pattern.len())?;
            for piece in pattern {
                hash.update([piece.as_ascii() as u8]);
            }
        }
    } else {
        hash_standard_bag_rows(hash, batch.standard_bag_ordinals().ok_or(INVALID_SOURCE)?)?;
    }
    count(hash, batch.graphs().len())?;
    for graph in batch.graphs() {
        hash_graph(hash, graph)?;
    }
    Ok(())
}

fn hash_standard_bag_rows(
    hash: &mut Sha256,
    ordinals: StandardBagOrdinals,
) -> Result<(), &'static str> {
    let length = ordinals.sequence_len();
    let mut row = [0_u8; 8 + BAG_LENGTH + MAX_TAIL_LENGTH];
    let row = row.get_mut(..8 + length).ok_or(INVALID_SOURCE)?;
    row[..8].copy_from_slice(&(length as u64).to_le_bytes());
    if length <= BAG_LENGTH {
        for rank in 0..ordinals.len() {
            let (pieces, _) = ordinals.sequence_at(rank).ok_or(INVALID_SOURCE)?;
            write_ascii_pieces(&mut row[8..], &pieces);
            hash.update(&*row);
        }
        return Ok(());
    }

    // The complete fresh bag descriptor guarantees head-major mixed-radix
    // order. Decode each of the at most 5,040 heads and 840 tails once. Only
    // the tiny tail ASCII table is live, not millions of decoded queues.
    let head = StandardBagOrdinals::new(BAG_LENGTH).ok_or(INVALID_SOURCE)?;
    let tail_length = length - BAG_LENGTH;
    let tail = StandardBagOrdinals::new(tail_length).ok_or(INVALID_SOURCE)?;
    if head.len().checked_mul(tail.len()) != Some(ordinals.len()) {
        return Err(INVALID_SOURCE);
    }
    let mut tails = [[0_u8; MAX_TAIL_LENGTH]; MAX_TAIL_COUNT];
    let tails = tails.get_mut(..tail.len()).ok_or(INVALID_SOURCE)?;
    for (rank, suffix) in tails.iter_mut().enumerate() {
        let (pieces, _) = tail.sequence_at(rank).ok_or(INVALID_SOURCE)?;
        write_ascii_pieces(&mut suffix[..tail_length], &pieces);
    }
    for rank in 0..head.len() {
        let (pieces, _) = head.sequence_at(rank).ok_or(INVALID_SOURCE)?;
        write_ascii_pieces(&mut row[8..8 + BAG_LENGTH], &pieces);
        for suffix in &*tails {
            row[8 + BAG_LENGTH..].copy_from_slice(&suffix[..tail_length]);
            hash.update(&*row);
        }
    }
    Ok(())
}

fn write_ascii_pieces(destination: &mut [u8], pieces: &[PieceKind]) {
    for (byte, piece) in destination.iter_mut().zip(pieces) {
        *byte = piece.as_ascii() as u8;
    }
}

fn hash_graph(hash: &mut Sha256, graph: &ExactScoringExecutionGraph) -> Result<(), &'static str> {
    let nodes = u32::try_from(graph.node_count()).map_err(|_| INVALID_SOURCE)?;
    if graph.root() >= nodes {
        return Err(INVALID_SOURCE);
    }
    hash.update(b"graph\0");
    hash.update(graph.candidate_id().to_le_bytes());
    let identity = graph.identity();
    hash.update(identity.initial_board_mask().to_le_bytes());
    count(hash, identity.placement_count())?;
    hash.update(identity.packed_piece_codes().to_le_bytes());
    for mask in identity.placement_masks() {
        hash.update(mask.to_le_bytes());
    }
    hash.update(graph.root().to_le_bytes());
    hash.update(nodes.to_le_bytes());
    for index in 0..nodes {
        let node = graph.node(index).ok_or(INVALID_SOURCE)?;
        graph.checked_edges(node).ok_or(INVALID_SOURCE)?;
        hash.update(node.edge_start().to_le_bytes());
        hash.update(node.edge_count().to_le_bytes());
        hash.update([u8::from(node.accepting())]);
    }
    // Bind ALL retained edges, including unreachable/unused storage. Hashing
    // only node spans would omit hidden evidence from the source identity.
    // Semantic reachability/board validation is the counting engine's task.
    count(hash, graph.all_edges().len())?;
    for edge in graph.all_edges() {
        hash_edge(hash, *edge);
    }
    Ok(())
}

fn hash_edge(hash: &mut Sha256, edge: ScoringExecutionEdge) {
    hash.update(edge.to().to_le_bytes());
    hash.update([
        edge.operation_index(),
        edge.piece().as_ascii() as u8,
        edge.rotation().quarter_turns(),
    ]);
    hash.update(edge.x().to_le_bytes());
    hash.update(edge.y().to_le_bytes());
    hash.update([
        edge.cleared_lines(),
        edge.blocked_t_corners(),
        edge.blocked_t_front_corners(),
        u8::from(edge.perfect_clear()),
    ]);
    let lock = edge.lock_evidence();
    hash.update([
        u8::from(lock.last_action_was_rotation()),
        u8::from(lock.used_kick()),
        u8::from(lock.used_180()),
        lock.from_rotation().quarter_turns(),
        match lock.rotation_request() {
            RotationRequest::None => 0,
            RotationRequest::Clockwise => 1,
            RotationRequest::CounterClockwise => 2,
            RotationRequest::HalfTurn => 3,
        },
        lock.kick_index(),
    ]);
    hash.update(lock.kick_dx().to_le_bytes());
    hash.update(lock.kick_dy().to_le_bytes());
    let (x, y) = lock.predecessor();
    hash.update(x.to_le_bytes());
    hash.update(y.to_le_bytes());
    hash.update([
        u8::from(lock.first_success_confirmed()),
        u8::from(lock.immobile_before_clear()),
    ]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use clearra_core_domain::{
        piece::rotation::RotationState,
        solution::normalized_tiling_solution::StandardBoard64TilingIdentity,
    };
    use clearra_replay::{ScoringExecutionNode, ScoringLockEvidence};

    fn edge(to: u32, operation: u8) -> ScoringExecutionEdge {
        ScoringExecutionEdge::new(
            to,
            operation,
            PieceKind::O,
            RotationState::Zero,
            0,
            0,
            0,
            0,
            0,
            ScoringLockEvidence::no_rotation(RotationState::Zero),
        )
    }

    fn graph(edges: Vec<ScoringExecutionEdge>) -> ExactScoringExecutionGraph {
        ExactScoringExecutionGraph::new(
            7,
            StandardBoard64TilingIdentity::from_placements(0, []).unwrap(),
            0,
            vec![ScoringExecutionNode::new(0, 0, true)],
            edges,
        )
    }

    fn digest(graph: &ExactScoringExecutionGraph) -> [u8; 32] {
        let mut hash = Sha256::new();
        hash_graph(&mut hash, graph).unwrap();
        hash.finalize().into()
    }

    #[test]
    fn replay_source_digest_binds_unused_hidden_edges_and_lock_evidence() {
        let base = graph(vec![edge(0, 0)]);
        assert_ne!(digest(&base), digest(&graph(vec![edge(0, 1)])));
        assert_ne!(digest(&base), digest(&graph(vec![edge(9, 0)])));
        assert_ne!(
            digest(&base),
            digest(&graph(vec![edge(0, 0).with_perfect_clear(true)]))
        );
        let rotated = ScoringExecutionEdge::new(
            0,
            0,
            PieceKind::O,
            RotationState::Zero,
            0,
            0,
            0,
            0,
            0,
            ScoringLockEvidence::rotation(
                RotationState::Left,
                RotationRequest::Clockwise,
                1,
                -1,
                2,
                3,
                4,
            )
            .with_immobile_before_clear(true),
        );
        assert_ne!(digest(&base), digest(&graph(vec![rotated])));
        assert_ne!(digest(&base), digest(&graph(vec![])));
    }

    #[test]
    fn replay_source_digest_ignores_allocation_capacity_but_not_storage_order() {
        let compact = graph(vec![edge(0, 0), edge(0, 1)]);
        let mut spare = Vec::with_capacity(64);
        spare.extend_from_slice(compact.all_edges());
        assert_eq!(digest(&compact), digest(&graph(spare)));
        assert_ne!(
            digest(&compact),
            digest(&graph(vec![edge(0, 1), edge(0, 0)]))
        );
    }

    #[test]
    fn ordinal_row_hash_preserves_historical_bytes_across_the_bag_boundary() {
        fn write_legacy(hash: &mut Sha256, remaining: u8, next: usize, head: &mut [u8; 7]) {
            if next == head.len() {
                for tail in PieceKind::STANDARD_TETROMINOES {
                    count(hash, 8).unwrap();
                    for &piece in &*head {
                        hash.update([piece]);
                    }
                    hash.update([tail.as_ascii() as u8]);
                }
                return;
            }
            for (slot, piece) in PieceKind::STANDARD_TETROMINOES.into_iter().enumerate() {
                let bit = 1 << slot;
                if remaining & bit != 0 {
                    head[next] = piece.as_ascii() as u8;
                    write_legacy(hash, remaining & !bit, next + 1, head);
                }
            }
        }
        let mut historical = Sha256::new();
        write_legacy(&mut historical, 0x7f, 0, &mut [0; 7]);
        let mut ordinal = Sha256::new();
        hash_standard_bag_rows(&mut ordinal, StandardBagOrdinals::new(8).unwrap()).unwrap();
        assert_eq!(ordinal.finalize(), historical.finalize());
    }

    #[test]
    fn replay_source_digest_rejects_invalid_root_and_node_spans() {
        let identity = StandardBoard64TilingIdentity::from_placements(0, []).unwrap();
        for malformed in [
            ExactScoringExecutionGraph::new(
                7,
                identity,
                1,
                vec![ScoringExecutionNode::new(0, 0, true)],
                vec![],
            ),
            ExactScoringExecutionGraph::new(
                7,
                identity,
                0,
                vec![ScoringExecutionNode::new(u32::MAX, 1, true)],
                vec![],
            ),
            ExactScoringExecutionGraph::new(7, identity, 0, vec![], vec![]),
        ] {
            assert!(hash_graph(&mut Sha256::new(), &malformed).is_err());
        }
    }

    #[test]
    fn replay_source_digest_length_prefix_separates_adjacent_strings() {
        let mut left = Sha256::new();
        bytes(&mut left, b"a").unwrap();
        bytes(&mut left, b"bc").unwrap();
        let mut right = Sha256::new();
        bytes(&mut right, b"ab").unwrap();
        bytes(&mut right, b"c").unwrap();
        assert_ne!(left.finalize(), right.finalize());
    }

    #[test]
    fn replay_source_digest_binds_compiled_context_even_for_an_empty_manifest() {
        use clearra_pc_graph::request::{
            PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow,
        };
        use clearra_problem::ProblemCompiler;
        use clearra_supply::queue::fixed_sequence::FixedSequence;

        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(2, 0xf3fcf),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::O])),
            PieceWindow::new(1),
        )
        .with_allow_hold(false)
        .with_exact_pieces(Some(1));
        let problem = ProblemCompiler::compile_scenario_pc(&query).unwrap();
        let changed = ProblemCompiler::compile_scenario_pc(
            &query
                .clone()
                .with_rule(clearra_rules::profile::builtin_rules::srs()),
        )
        .unwrap();
        let batches = [];
        assert_ne!(
            pc_replay_source_hasher(&problem, &batches)
                .unwrap()
                .finalize(),
            pc_replay_source_hasher(&changed, &batches)
                .unwrap()
                .finalize(),
        );
    }
}
