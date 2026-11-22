use clearra_core_domain::{
    execution_cancellation::ExecutionControl,
    piece::{piece_kind::PieceKind, rotation::RotationState},
};
use clearra_coverage::pattern::{pattern_bitset::PatternBitSet, pattern_id::PatternId};
use clearra_supply::{
    execution_automaton::{SupplyObservationIdentity, SupplyProvenanceId},
    pattern_universe::PatternUniverseMaterializer,
    piece_source::PieceSourceId,
    queue::queue_pattern_expression::QueuePatternExpression,
};

use super::super::buildup::{BuildOrderGraph, BuildOrderNodeSpec};
use super::*;

fn universe(expression: &str) -> MaterializedPatternUniverse {
    PatternUniverseMaterializer::queue_pattern_expression(
        &QueuePatternExpression::parse(expression, 0).unwrap(),
        83,
    )
    .unwrap()
}

fn initial(hold_enabled: bool, held: Option<PieceKind>) -> HoldAutomatonState {
    HoldAutomatonState::with_contract(
        PieceSourceId::new(83),
        PieceSourceKind::MaterializedPatternUniverse,
        0,
        held,
        if hold_enabled {
            HoldPolicy::Allowed
        } else {
            HoldPolicy::Forbidden
        },
        0,
        0,
        SupplyObservationIdentity::full_queue_oracle(),
        SupplyProvenanceId(83),
    )
}

fn language(masks: &[u8]) -> (PieceOrderLanguageCache, u32) {
    let mut specs = Vec::new();
    let mut edges = Vec::new();
    for (depth, &mask) in masks.iter().enumerate() {
        specs.push(BuildOrderNodeSpec {
            edge_start: edges.len() as u32,
            edge_count: mask.count_ones(),
            depth: depth as u8,
            accepting: false,
        });
        for (index, piece) in PieceKind::STANDARD_TETROMINOES.iter().copied().enumerate() {
            if mask & (1 << index) != 0 {
                edges.push(BuildOrderGraph::edge(
                    (depth + 1) as u32,
                    depth as u8,
                    piece,
                    RotationState::Zero,
                    0,
                    0,
                    0,
                ));
            }
        }
    }
    specs.push(BuildOrderNodeSpec {
        edge_start: edges.len() as u32,
        edge_count: 0,
        depth: masks.len() as u8,
        accepting: true,
    });
    let graph = BuildOrderGraph::from_topological_parts(specs, edges, 0, 0, true).unwrap();
    let mut cache = PieceOrderLanguageCache::default();
    let root = cache.canonicalize(&graph).unwrap();
    (cache, root)
}

/// Explicit original-queue oracle: every real draw uses the canonical finite
/// supply automaton. EOF may release only the retained source piece once.
fn finite_oracle(
    queue: &[PieceKind],
    initial: HoldAutomatonState,
    masks: &[u8],
    projected: bool,
) -> bool {
    let automaton = SupplyExecutionAutomaton::sequence();
    let mut states = vec![initial];
    for (depth, &allowed) in masks.iter().enumerate() {
        let mut next = Vec::new();
        for state in states {
            let cursor = usize::from(state.cursor());
            if cursor == queue.len() {
                if projected
                    && depth + 1 == masks.len()
                    && state.hold_policy() != HoldPolicy::Forbidden
                {
                    if state
                        .hold_piece()
                        .is_some_and(|piece| allowed & (1 << (piece_code(piece) - 1)) != 0)
                    {
                        let mut released = state;
                        released.hold_piece = None;
                        released.hold_empty = true;
                        next.push(released);
                    }
                }
                continue;
            }
            for branch in [
                SupplyBranchKind::Current,
                SupplyBranchKind::SwapHeld,
                SupplyBranchKind::StoreCurrent,
            ] {
                let Ok(step) = automaton.transition(
                    state,
                    branch,
                    queue[cursor],
                    queue.get(cursor + 1).copied(),
                ) else {
                    continue;
                };
                if allowed & (1 << (piece_code(step.used_piece) - 1)) != 0
                    && !next.contains(&step.next_state)
                {
                    next.push(step.next_state);
                }
            }
        }
        states = next;
    }
    states.into_iter().any(|state| {
        !projected || usize::from(state.cursor()) == queue.len() && state.hold_piece().is_none()
    })
}

#[test]
fn factorized_coverage_matches_every_original_queue_and_canonical_hold_branch() {
    let control = ExecutionControl::default();
    for expression in ["P1P4", "P2P3", "P1P6", "[LOI]2P5"] {
        let source = universe(expression);
        let draws = source.sequence_len_at(0);
        for (hold_enabled, held) in [(false, None), (true, None), (true, Some(PieceKind::T))] {
            let initial = initial(hold_enabled, held);
            let placed = draws + usize::from(held.is_some());
            let projected = hold_enabled;
            let mut coverage =
                StandardBagCoverage::for_universe(&source, initial, hold_enabled, projected)
                    .unwrap()
                    .expect("complete finite layout supported");
            let mut expected_union = PatternBitSet::new(source.pattern_count());
            for phase in 0..2 {
                let seed = source.sequence_at((phase * 137) % source.pattern_count());
                let masks = (0..placed)
                    .map(|depth| {
                        (1 << (piece_code(seed[depth % draws]) - 1))
                            | (1 << ((depth + 2 + phase) % 7))
                    })
                    .collect::<Vec<_>>();
                let (language, root) = language(&masks);
                let mut expected = PatternBitSet::new(source.pattern_count());
                for ordinal in 0..source.pattern_count() {
                    if finite_oracle(&source.sequence_at(ordinal), initial, &masks, projected) {
                        expected.insert(PatternId::new(ordinal)).unwrap();
                    }
                }
                let result = coverage.cover_language(&language, root, &control).unwrap();
                assert_eq!(
                    result.covered_pattern_count,
                    expected.count_ones() as usize,
                    "{expression} hold={held:?} phase={phase}"
                );
                let first = (0..source.pattern_count())
                    .find(|&ordinal| expected.contains(PatternId::new(ordinal)))
                    .map(|ordinal| ordinal as u32);
                assert_eq!(result.witness_pattern_id, first);
                assert_eq!(result.covers_any_pattern, first.is_some());
                assert_eq!(coverage.materialize_root(result.root).unwrap(), expected);
                expected_union.union_with(&expected).unwrap();
                coverage.merge_global(result.root).unwrap();
                coverage.flush_and_recycle_local_cache().unwrap();
                assert_eq!(coverage.materialize_global().unwrap(), expected_union);
            }
        }
    }
}

#[test]
fn factorized_p1p7p7_rank_boundaries_keep_exact_witnesses_without_expanding_source() {
    let source = universe("P1P7P7");
    let mut coverage =
        StandardBagCoverage::for_universe(&source, initial(false, None), false, false)
            .unwrap()
            .unwrap();
    assert_eq!(coverage.suffix_count(coverage.root_source()), 177_811_200);
    assert!(coverage.tables.retained_bytes() < 100_000);
    let ordinals = [0, 25_401_600 + 5040 + 63, 177_811_199];
    for ordinal in ordinals {
        let masks = source
            .sequence_at(ordinal)
            .iter()
            .map(|piece| 1 << (piece_code(*piece) - 1))
            .collect::<Vec<_>>();
        let (language, root) = language(&masks);
        let result = coverage
            .cover_language(&language, root, &ExecutionControl::default())
            .unwrap();
        assert_eq!(result.covered_pattern_count, 1);
        assert_eq!(result.witness_pattern_id, Some(ordinal as u32));
        coverage.merge_global(result.root).unwrap();
        coverage.flush_and_recycle_local_cache().unwrap();
    }
    let union = coverage.materialize_global().unwrap();
    assert_eq!(union.count_ones(), ordinals.len() as u32);
    for ordinal in ordinals {
        assert!(union.contains(PatternId::new(ordinal)));
    }
}

#[test]
fn factorized_shared_table_identity_compares_full_segment_schedule() {
    let a = universe("P1P4");
    let b = universe("P4P1");
    assert_eq!(a.structure(), b.structure());
    assert_eq!(a.pattern_count(), b.pattern_count());
    let shared = SharedStandardBagRequest::default();
    let first =
        StandardBagCoverage::for_universe_with_shared(&a, initial(true, None), true, true, &shared)
            .unwrap()
            .unwrap();
    let second =
        StandardBagCoverage::for_universe_with_shared(&a, initial(true, None), true, true, &shared)
            .unwrap()
            .unwrap();
    assert!(Arc::ptr_eq(&first.tables, &second.tables));
    assert!(matches!(
        StandardBagCoverage::for_universe_with_shared(&b, initial(true, None), true, true, &shared),
        Err(WasmExactSearchError::InvalidProblem(
            "wasm_standard_bag_shared_request_identity_mismatch"
        ))
    ));
    let prefix = QueuePatternExpression::parse("P1P4", 0).unwrap().prefix(4);
    let hidden = PatternUniverseMaterializer::queue_pattern_expression(&prefix, 83).unwrap();
    assert!(!StandardBagCoverage::supports(&hidden, initial(true, None)));
    let mut bag_identity = initial(true, None);
    bag_identity.source_kind = PieceSourceKind::BagUniverse;
    assert!(!StandardBagCoverage::supports(&a, bag_identity));
}

#[test]
fn factorized_layout_and_cursor_payload_are_charged_at_live_memory_boundaries() {
    let source = universe("P1P4");
    let mut coverage =
        StandardBagCoverage::for_universe(&source, initial(false, None), false, false)
            .unwrap()
            .unwrap();
    let tables = &coverage.tables;
    let layout_bytes = tables.factorized_layout.as_ref().unwrap().retained_bytes();
    assert!(layout_bytes > 0);
    assert_eq!(
        StandardBagCoverage::checked_cursor_transition_retained_bytes(&source),
        Some((tables.cursor_transitions.retained_bytes() + layout_bytes) as u128)
    );
    assert!(
        StandardBagCoverage::checked_creation_future_bytes(&source).unwrap()
            >= tables.retained_bytes() as u128
    );
    coverage.merge_global(ACCEPT).unwrap();
    let outside = 37_u128;
    let retained = coverage.retained_bytes() as u128;
    let words = source.pattern_count().div_ceil(64) as u128 * 8;
    coverage.set_resource_boundary(
        ExecutionMemoryBound::for_test_cap(outside + retained + words - 1),
        outside,
    );
    assert!(coverage.flush_epoch_global().is_err());
    assert_eq!(coverage.epoch_global_root, ACCEPT);
    assert_eq!(coverage.global_words.capacity(), 0);
    coverage.set_resource_boundary(
        ExecutionMemoryBound::for_test_cap(outside + retained + words),
        outside,
    );
    coverage.flush_epoch_global().unwrap();
    assert!(coverage.global_is_complete());
}

#[test]
fn factorized_partial_atom_eof_cannot_place_its_unseen_complement() {
    let source = universe("P1P6");
    let mut coverage = StandardBagCoverage::for_universe(&source, initial(true, None), true, true)
        .unwrap()
        .unwrap();
    let terminal = SourceState {
        depth: 7,
        bag_remainder: 1 << (piece_code(PieceKind::L) - 1),
    };
    coverage
        .tables
        .cursor_transitions
        .validate_source(terminal)
        .unwrap();
    let (missing, missing_root) = language(&[1 << (piece_code(PieceKind::L) - 1)]);
    assert_eq!(
        coverage
            .solve(
                &missing,
                missing_root,
                terminal,
                piece_code(PieceKind::I),
                &ExecutionControl::default()
            )
            .unwrap(),
        REJECT
    );
    let (held, held_root) = language(&[1 << (piece_code(PieceKind::I) - 1)]);
    assert_eq!(
        coverage
            .solve(
                &held,
                held_root,
                terminal,
                piece_code(PieceKind::I),
                &ExecutionControl::default()
            )
            .unwrap(),
        ACCEPT
    );
}

#[test]
fn factorized_full_bag_preserves_existing_standard_bag_ordinal_coverage() {
    let finite = universe("P7");
    let standard = PatternUniverseMaterializer::standard_7_bag(7, 0, 83).unwrap();
    for held in [None, Some(PieceKind::T)] {
        let finite_initial = initial(true, held);
        let mut standard_initial = finite_initial;
        standard_initial.bag_remainder_key =
            BagState::fresh_standard_7_bag().packed_remainder_key();
        let mut a = StandardBagCoverage::for_universe(&finite, finite_initial, true, true)
            .unwrap()
            .unwrap();
        let mut b = StandardBagCoverage::for_universe(&standard, standard_initial, true, true)
            .unwrap()
            .unwrap();
        let masks = (0..7 + usize::from(held.is_some()))
            .map(|depth| (1 << (depth % 7)) | (1 << ((depth + 2) % 7)))
            .collect::<Vec<_>>();
        let (language, root) = language(&masks);
        let ar = a
            .cover_language(&language, root, &ExecutionControl::default())
            .unwrap();
        let br = b
            .cover_language(&language, root, &ExecutionControl::default())
            .unwrap();
        assert_eq!(ar.covered_pattern_count, br.covered_pattern_count);
        assert_eq!(ar.witness_pattern_id, br.witness_pattern_id);
        assert_eq!(
            a.materialize_root(ar.root).unwrap(),
            b.materialize_root(br.root).unwrap()
        );
    }
}
