use super::super::{
    BagState, HoldAutomatonState, HoldPolicy, MaterializedPatternUniverse, PieceSourceKind,
    SupplyExecutionAutomaton, SupplyExecutionState, ACCEPT,
};
use super::*;
use crate::backend::wasm_cpu::buildup::{BuildOrderGraph, BuildOrderNodeSpec};
use clearra_core_domain::piece::rotation::RotationState;
use clearra_supply::{
    execution_automaton::SupplyObservationIdentity, hold_automaton::SupplyProvenanceId,
    piece_source::PieceSourceId, PatternUniverseMaterializer,
};

const KINDS: [PieceKind; 7] = PieceKind::STANDARD_TETROMINOES;

pub(in super::super) fn fixture(
    draws: usize,
    limit: usize,
    hold_enabled: bool,
    held: Option<PieceKind>,
    projected: bool,
) -> (MaterializedPatternUniverse, StandardBagCoverage) {
    let universe = PatternUniverseMaterializer::standard_7_bag(draws, limit, 0x77).unwrap();
    let initial = HoldAutomatonState::new(
        PieceSourceId::new(0x77),
        0,
        held,
        0,
        BagState::fresh_standard_7_bag().packed_remainder_key(),
        SupplyProvenanceId(0x77),
    );
    let coverage = StandardBagCoverage::for_universe(&universe, initial, hold_enabled, projected)
        .unwrap()
        .unwrap();
    (universe, coverage)
}

// Separate same-kind root edges deliberately retain distinct suffixes. The
// independent oracle below consumes the literal words, not this graph or memo.
pub(in super::super) fn language_for_words(words: &[Vec<PieceKind>]) -> BuildOrderGraph {
    assert!(!words.is_empty());
    let depth = words[0].len();
    assert!(words.iter().all(|word| word.len() == depth));
    if depth == 0 {
        return BuildOrderGraph::from_topological_parts(
            vec![BuildOrderNodeSpec {
                edge_start: 0,
                edge_count: 0,
                depth: 0,
                accepting: true,
            }],
            vec![],
            0,
            0,
            true,
        )
        .unwrap();
    }
    let mut specs = vec![BuildOrderNodeSpec {
        edge_start: 0,
        edge_count: words.len() as u32,
        depth: 0,
        accepting: false,
    }];
    let mut edges: Vec<_> = words
        .iter()
        .enumerate()
        .map(|(index, word)| {
            BuildOrderGraph::edge(
                (1 + index * depth) as u32,
                0,
                word[0],
                RotationState::Zero,
                0,
                0,
                0,
            )
        })
        .collect();
    for (index, word) in words.iter().enumerate() {
        for placed in 1..=depth {
            specs.push(BuildOrderNodeSpec {
                edge_start: edges.len() as u32,
                edge_count: u32::from(placed < depth),
                depth: placed as u8,
                accepting: placed == depth,
            });
            if placed < depth {
                edges.push(BuildOrderGraph::edge(
                    (1 + index * depth + placed) as u32,
                    placed as u8,
                    word[placed],
                    RotationState::Zero,
                    0,
                    0,
                    0,
                ));
            }
        }
    }
    BuildOrderGraph::from_topological_parts(specs, edges, 0, 0, true).unwrap()
}

fn canonical_initial(hold_enabled: bool, held: Option<PieceKind>) -> SupplyExecutionState {
    SupplyExecutionState::with_contract(
        PieceSourceId::new(0x77),
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
        SupplyProvenanceId(0x77),
    )
}

fn original_or_determined_piece(
    queue: &[PieceKind],
    cursor: usize,
    projected: bool,
) -> Option<PieceKind> {
    queue.get(cursor).copied().or_else(|| {
        (projected && cursor == queue.len() && queue.len() % 7 == 6)
            .then(|| {
                KINDS
                    .into_iter()
                    .find(|piece| !queue[queue.len() - 6..].contains(piece))
            })
            .flatten()
    })
}

// Literal finite-source recursion through the public canonical Hold automaton.
// It does not use SourceState, suffix counts, compact transitions, or union/min.
fn canonical_accepts_word(
    queue: &[PieceKind],
    word: &[PieceKind],
    state: SupplyExecutionState,
    projected: bool,
) -> bool {
    let cursor = usize::from(state.cursor);
    if word.is_empty() {
        return !projected
            || (cursor == queue.len() && state.hold_piece.is_none())
            || (cursor == queue.len() + 1 && state.hold_piece.is_some());
    }
    let Some(current) = original_or_determined_piece(queue, cursor, projected) else {
        // Unknown input outside the original finite source can only trigger
        // its final held release. It cannot be output or continue a word.
        return projected
            && cursor == queue.len()
            && state.hold_policy != HoldPolicy::Forbidden
            && word.len() == 1
            && state.hold_piece == Some(word[0]);
    };
    let next = original_or_determined_piece(queue, cursor + 1, projected);
    for action in [
        SupplyBranchKind::Current,
        SupplyBranchKind::SwapHeld,
        SupplyBranchKind::StoreCurrent,
    ] {
        let Ok(step) =
            SupplyExecutionAutomaton::sequence().transition(state, action, current, next)
        else {
            continue;
        };
        if step.used_piece == word[0]
            && canonical_accepts_word(queue, &word[1..], step.next_state, projected)
        {
            return true;
        }
    }
    false
}

pub(in super::super) fn canonical_first(
    universe: &MaterializedPatternUniverse,
    words: &[Vec<PieceKind>],
    hold_enabled: bool,
    held: Option<PieceKind>,
    projected: bool,
) -> Option<u32> {
    let initial = canonical_initial(hold_enabled, held);
    (0..universe.pattern_count())
        .find(|&ordinal| {
            let queue = universe.sequence_at(ordinal);
            words
                .iter()
                .any(|word| canonical_accepts_word(&queue, word, initial, projected))
        })
        .map(|ordinal| ordinal as u32)
}

#[test]
fn first_witness_matches_full_roots_and_canonical_finite_hold_enumeration() {
    use PieceKind::{I, J, L, O, T};
    let control = ExecutionControl::default();
    for (draws, limit) in [(2, 0), (3, 0), (3, 13), (6, 43), (7, 43), (8, 43)] {
        let cyclic: Vec<_> = (0..draws).map(|index| KINDS[index % 7]).collect();
        let cases = vec![
            vec![vec![I, L], vec![T, O]],
            vec![vec![I, L], vec![O, T]],
            vec![vec![I, L], vec![I, O]],
            vec![vec![J, I]],
            vec![cyclic],
            vec![vec![I; draws + 1]],
        ];
        let mut language = PieceOrderLanguageCache::default();
        let roots: Vec<_> = cases
            .iter()
            .map(|words| language.canonicalize(&language_for_words(words)).unwrap())
            .collect();
        for (hold_enabled, held) in [
            (false, None),
            (true, None),
            (true, Some(I)),
            (true, Some(T)),
        ] {
            for projected in [false, true] {
                let (universe, mut full) = fixture(draws, limit, hold_enabled, held, projected);
                let (_, mut scalar) = fixture(draws, limit, hold_enabled, held, projected);
                #[cfg(feature = "placement-dependency-pc")]
                let (_, mut membership) = fixture(draws, limit, hold_enabled, held, projected);
                // ACCEPT is the exact universal set in this owner's original
                // domain; it is flushed by the real mode transition below.
                scalar.merge_global(ACCEPT).unwrap();
                #[cfg(feature = "placement-dependency-pc")]
                membership.merge_global(ACCEPT).unwrap();
                for (index, (&root, words)) in roots.iter().zip(&cases).enumerate() {
                    let expected = canonical_first(&universe, words, hold_enabled, held, projected);
                    let baseline = full.cover_language(&language, root, &control).unwrap();
                    assert_eq!(baseline.witness_pattern_id, expected, "full draws={draws} limit={limit} hold={hold_enabled} held={held:?} projection={projected} case={index}");
                    assert_eq!(baseline.covers_any_pattern, expected.is_some());
                    assert_eq!(baseline.covered_pattern_count != 0, expected.is_some());
                    #[cfg(feature = "placement-dependency-pc")]
                    for _ in 0..2 {
                        assert_eq!(membership.contains_pattern_language(&language, root, &control).unwrap(), expected.is_some(), "Boolean draws={draws} limit={limit} hold={hold_enabled} held={held:?} projection={projected} case={index}");
                        assert_eq!(membership.product_memo.len(), 0);
                        assert_eq!(membership.union_memo.len(), 0);
                    }
                    assert_eq!(scalar.first_pattern_witness(&language, root, &control).unwrap(), expected, "scalar draws={draws} limit={limit} hold={hold_enabled} held={held:?} projection={projected} case={index}");
                    assert_eq!(
                        scalar
                            .first_pattern_witness(&language, root, &control)
                            .unwrap(),
                        expected,
                        "memo repeat"
                    );
                    if index == 2 {
                        scalar.flush_and_recycle_local_cache().unwrap();
                        assert_eq!(
                            scalar.product_memo_meaning,
                            ProductMemoMeaning::MinimumOriginalOrdinal
                        );
                        #[cfg(feature = "placement-dependency-pc")]
                        {
                            membership.flush_and_recycle_local_cache().unwrap();
                            assert_eq!(
                                membership.product_memo_meaning,
                                ProductMemoMeaning::BooleanMembership
                            );
                            assert_eq!(membership.memo_accounting().boolean_negative_roots, 0);
                        }
                    }
                }
                assert_eq!(scalar.first_witness_mode_transitions, 1);
                assert_eq!(scalar.first_witness_calls, cases.len() * 2);
                assert!(scalar.global_is_complete());
                assert_eq!(
                    scalar.materialize_global().unwrap().count_ones() as usize,
                    universe.pattern_count()
                );
                #[cfg(feature = "placement-dependency-pc")]
                {
                    assert_eq!(
                        membership
                            .memo_accounting()
                            .boolean_membership_mode_transitions,
                        1
                    );
                    assert_eq!(
                        membership.memo_accounting().boolean_membership_calls,
                        cases.len() * 2
                    );
                    assert_eq!(
                        membership.materialize_global().unwrap().count_ones() as usize,
                        universe.pattern_count()
                    );
                }
            }
        }
    }
}

#[test]
fn first_witness_minimizes_all_hold_and_same_kind_alternatives() {
    use PieceKind::{I, L, O, T};
    let control = ExecutionControl::default();
    for (held, alternatives) in [
        (Some(T), vec![vec![I, L], vec![T, O]]),
        (None, vec![vec![I, L], vec![O, T]]),
        (None, vec![vec![I, L], vec![I, O]]),
    ] {
        let mut language = PieceOrderLanguageCache::default();
        let first_only = language
            .canonicalize(&language_for_words(&alternatives[..1]))
            .unwrap();
        let all = language
            .canonicalize(&language_for_words(&alternatives))
            .unwrap();
        let (_, mut full) = fixture(3, 0, true, held, false);
        assert!(
            full.cover_language(&language, first_only, &control)
                .unwrap()
                .witness_pattern_id
                .unwrap()
                > 0
        );
        let (_, mut scalar) = fixture(3, 0, true, held, false);
        scalar.merge_global(ACCEPT).unwrap();
        assert_eq!(
            scalar
                .first_pattern_witness(&language, all, &control)
                .unwrap(),
            Some(0)
        );
    }
}

#[test]
fn first_witness_drops_full_storage_and_rejects_old_roots_without_losing_global_words() {
    use PieceKind::{I, O};
    let control = ExecutionControl::default();
    let mut language = PieceOrderLanguageCache::default();
    let i = language
        .canonicalize(&language_for_words(&[vec![I]]))
        .unwrap();
    let o = language
        .canonicalize(&language_for_words(&[vec![O]]))
        .unwrap();
    let (universe, mut coverage) = fixture(3, 0, false, None, false);
    let old = coverage
        .cover_language(&language, i, &control)
        .unwrap()
        .root;
    let other = coverage
        .cover_language(&language, o, &control)
        .unwrap()
        .root;
    coverage.merge_global(old).unwrap();
    coverage.merge_global(other).unwrap();
    assert!(!coverage.can_use_first_pattern_witness());
    assert!(matches!(
        coverage.first_pattern_witness(&language, i, &control),
        Err(WasmExactSearchError::InvalidProblem(
            "wasm_standard_bag_first_witness_scope_invalid"
        ))
    ));
    assert_eq!(
        coverage.product_memo_meaning,
        ProductMemoMeaning::CoverageRoot
    );
    assert!(coverage.nodes.capacity() > 0);
    assert!(coverage.union_memo.capacity() > 0);
    let before = coverage.retained_bytes();
    let product_capacity = coverage.product_memo.capacity();
    assert!(product_capacity > 0);
    coverage.merge_global(ACCEPT).unwrap();
    assert!(coverage.can_use_first_pattern_witness());
    coverage.enter_first_witness_mode().unwrap();
    assert_eq!(coverage.product_memo.capacity(), 0);
    assert_eq!(coverage.product_memo.directory_retained_bytes(), 0);
    assert_eq!(
        coverage
            .first_pattern_witness(&language, i, &control)
            .unwrap(),
        Some(0)
    );
    assert_eq!(coverage.nodes.capacity(), 0);
    assert_eq!(coverage.node_summaries.capacity(), 0);
    assert_eq!(coverage.node_levels.capacity(), 0);
    assert_eq!(coverage.node_valid_masks.capacity(), 0);
    assert_eq!(coverage.bucket_heads.capacity(), 0);
    assert_eq!(coverage.union_memo.capacity(), 0);
    assert!(coverage.product_memo.capacity() <= product_capacity);
    assert!(coverage.retained_bytes() < before);
    for result in [
        coverage.cover_language(&language, i, &control).map(|_| ()),
        coverage.materialize_root(old).map(|_| ()),
        coverage.merge_global(old),
        coverage.capture_p7p4_root(old).map(|_| ()),
    ] {
        assert!(matches!(
            result,
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_standard_bag_coverage_root_after_membership_transition"
            ))
        ));
    }
    let global = coverage.materialize_global().unwrap();
    assert_eq!(global.count_ones() as usize, universe.pattern_count());
    #[cfg(feature = "parallel")]
    {
        let (count, words) = coverage.flushed_global_words().unwrap();
        assert_eq!(count, universe.pattern_count());
        assert_eq!(words.unwrap(), global.to_owned_words());
    }
    coverage.flush_and_recycle_local_cache().unwrap();
    assert_eq!(coverage.product_memo.len(), 0);
    assert!(coverage.can_use_first_pattern_witness());
    assert_eq!(
        coverage
            .first_pattern_witness(&language, o, &control)
            .unwrap(),
        Some(30)
    );
    assert_eq!(coverage.first_witness_mode_transitions, 1);
}

#[test]
fn first_witness_preserves_clipped_universe_failure_admission_and_language_recycle() {
    use PieceKind::{I, O};
    let control = ExecutionControl::default();
    let mut language = PieceOrderLanguageCache::default();
    let outside = language
        .canonicalize(&language_for_words(&[vec![O]]))
        .unwrap();
    let seed = language
        .canonicalize(&language_for_words(&[vec![I]]))
        .unwrap();
    let (universe, mut coverage) = fixture(3, 13, false, None, false);
    let (_, mut untruncated) = fixture(3, 0, false, None, false);
    assert_eq!(
        untruncated
            .cover_language(&language, outside, &control)
            .unwrap()
            .witness_pattern_id,
        Some(30)
    );
    let clipped = coverage
        .cover_language(&language, outside, &control)
        .unwrap();
    assert!(!clipped.covers_any_pattern);
    assert_eq!(clipped.covered_pattern_count, 0);
    assert_eq!(clipped.witness_pattern_id, None);
    assert_eq!(
        canonical_first(&universe, &[vec![O]], false, None, false),
        None
    );
    let seed_root = coverage
        .cover_language(&language, seed, &control)
        .unwrap()
        .root;
    assert_ne!(seed_root, ACCEPT);
    coverage.merge_global(seed_root).unwrap();
    assert!(!coverage.can_use_first_pattern_witness());
    assert_eq!(coverage.materialize_global().unwrap().count_ones(), 13);
    assert!(coverage.can_use_first_pattern_witness());
    assert_eq!(
        coverage
            .first_pattern_witness(&language, outside, &control)
            .unwrap(),
        None
    );
    coverage.flush_and_recycle_local_cache().unwrap();
    language.clear_retain_capacity();
    let inside = language
        .canonicalize(&language_for_words(&[vec![I]]))
        .unwrap();
    assert_eq!(
        inside, outside,
        "the language arena really reused its old node ID"
    );
    coverage.product_memo.fail_admissions_for_test(usize::MAX);
    for _ in 0..2 {
        assert_eq!(
            coverage
                .first_pattern_witness(&language, inside, &control)
                .unwrap(),
            canonical_first(&universe, &[vec![I]], false, None, false)
        );
        assert_eq!(coverage.product_memo.len(), 0);
    }
    assert_eq!(coverage.first_witness_mode_transitions, 1);
}

#[test]
fn first_witness_checks_cancel_and_u32_sentinel_without_truncating() {
    let mut language = PieceOrderLanguageCache::default();
    let root = language
        .canonicalize(&language_for_words(&[vec![PieceKind::I]]))
        .unwrap();
    let (_, mut coverage) = fixture(3, 13, false, None, false);
    coverage.merge_global(ACCEPT).unwrap();
    let cancelled = ExecutionControl::default();
    cancelled.cancellation.handle().cancel();
    assert!(matches!(
        coverage.first_pattern_witness(&language, root, &cancelled),
        Err(WasmExactSearchError::Cancelled)
    ));
    assert_eq!(
        coverage.product_memo_meaning,
        ProductMemoMeaning::CoverageRoot
    );
    coverage
        .first_pattern_witness(&language, root, &ExecutionControl::default())
        .unwrap();
    assert!(matches!(
        coverage.first_pattern_witness(&language, root, &cancelled),
        Err(WasmExactSearchError::Cancelled)
    ));
    coverage.materialized_pattern_count = u32::MAX as usize;
    coverage.global_covered_pattern_count = u32::MAX as usize;
    assert!(coverage.can_use_first_pattern_witness());
    assert_eq!(
        coverage.first_witness_add_offset(u128::from(u32::MAX) - 1, 0),
        u32::MAX - 1
    );
    assert_eq!(
        coverage.first_witness_add_offset(u128::from(u32::MAX), 0),
        NO_WITNESS
    );
    assert_eq!(coverage.first_witness_add_offset(u128::MAX, 1), NO_WITNESS);
    assert_eq!(coverage.first_witness_add_offset(0, NO_WITNESS), NO_WITNESS);
    #[cfg(target_pointer_width = "64")]
    {
        coverage.materialized_pattern_count = u32::MAX as usize + 1;
        coverage.global_covered_pattern_count = coverage.materialized_pattern_count;
        assert!(!coverage.can_use_first_pattern_witness());
        assert!(matches!(
            coverage.first_pattern_witness(&language, root, &ExecutionControl::default()),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_standard_bag_first_witness_scope_invalid"
            ))
        ));
    }
}
