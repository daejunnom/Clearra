use super::super::first_witness::tests::{canonical_first, language_for_words};
use super::super::{
    BagState, HoldAutomatonState, MaterializedPatternUniverse, PieceKind, PieceOrderLanguageCache,
    ProductMemoMeaning, SharedStandardBagRequest,
};
use super::*;
use crate::backend::wasm_cpu::parallel_coverage::SharedCoverage;
use clearra_coverage::pattern::pattern_bitset::PatternBitSet;
use clearra_supply::{
    execution_automaton::SupplyObservationIdentity, hold_automaton::SupplyProvenanceId,
    piece_source::PieceSourceId, PatternUniverseMaterializer,
};

fn initial(held: Option<PieceKind>) -> HoldAutomatonState {
    HoldAutomatonState::new(
        PieceSourceId::new(0x77),
        0,
        held,
        0,
        BagState::fresh_standard_7_bag().packed_remainder_key(),
        SupplyProvenanceId(0x77),
    )
}

fn owner(
    universe: &MaterializedPatternUniverse,
    initial: HoldAutomatonState,
    hold: bool,
    projected: bool,
    request: &Arc<SharedStandardBagRequest>,
) -> StandardBagCoverage {
    let mut owner =
        StandardBagCoverage::for_universe_with_shared(universe, initial, hold, projected, request)
            .unwrap()
            .unwrap();
    owner.set_boolean_membership_enabled_for_test(true);
    owner.set_shared_saturation_enabled_for_test(true);
    owner
}

#[test]
fn shared_saturation_preserves_partial_evidence_recycles_boolean_and_releases_leases() {
    use PieceKind::{I, O};
    let control = ExecutionControl::default();
    let universe = PatternUniverseMaterializer::standard_7_bag(3, 0, 0x77).unwrap();
    let request = Arc::new(SharedStandardBagRequest::default());
    let mut publisher = owner(&universe, initial(None), false, false, &request);
    let mut receiver = owner(&universe, initial(None), false, false, &request);
    let shared =
        SharedCoverage::for_standard_bag_request(universe.pattern_count(), Arc::clone(&request));
    let mut language = PieceOrderLanguageCache::default();
    let i = language
        .canonicalize(&language_for_words(&[vec![I]]))
        .unwrap();
    let o = language
        .canonicalize(&language_for_words(&[vec![O]]))
        .unwrap();
    let negative = language
        .canonicalize(&language_for_words(&[vec![I, I]]))
        .unwrap();
    let all = language
        .canonicalize(&language_for_words(
            &PieceKind::STANDARD_TETROMINOES
                .into_iter()
                .map(|piece| vec![piece])
                .collect::<Vec<_>>(),
        ))
        .unwrap();

    let first = receiver.cover_language(&language, i, &control).unwrap();
    let second = receiver.cover_language(&language, o, &control).unwrap();
    let mut expected = receiver.materialize_root(first.root).unwrap();
    expected
        .union_with(&receiver.materialize_root(second.root).unwrap())
        .unwrap();
    receiver.merge_global(first.root).unwrap();
    receiver.merge_global(second.root).unwrap();
    assert!(receiver.nodes.capacity() > 0);
    assert!(receiver.product_memo.capacity() > 0);
    assert!(receiver.union_memo.capacity() > 0);
    assert!(!receiver.global_is_complete());
    assert!(shared.standard_bag_complete_proof().is_none());

    // This publisher actually computes full original coverage from the union
    // of all seven one-piece languages; no production completion flag is set.
    let full = publisher.cover_language(&language, all, &control).unwrap();
    assert_eq!(full.covered_pattern_count, universe.pattern_count());
    publisher.merge_global(full.root).unwrap();
    let (count, words) = publisher.flushed_global_words().unwrap();
    shared
        .union_original_words_for_request(Some(&request), count, words)
        .unwrap();
    let proof = shared.standard_bag_complete_proof().unwrap();
    assert!(receiver
        .adopt_shared_boolean_saturation(&proof, &control)
        .unwrap());
    assert!(!receiver
        .adopt_shared_boolean_saturation(&proof, &control)
        .unwrap());
    assert!(!receiver.global_is_complete());
    assert!(!receiver.can_use_first_pattern_witness());
    assert!(receiver.can_use_boolean_pattern_membership());
    assert_eq!(receiver.materialize_global().unwrap(), expected);
    assert_eq!(
        receiver.product_memo_meaning,
        ProductMemoMeaning::BooleanMembership
    );
    assert_eq!(receiver.nodes.capacity(), 0);
    assert_eq!(receiver.node_summaries.capacity(), 0);
    assert_eq!(receiver.node_levels.capacity(), 0);
    assert_eq!(receiver.node_valid_masks.capacity(), 0);
    assert_eq!(receiver.bucket_heads.capacity(), 0);
    assert_eq!(receiver.product_memo.capacity(), 0);
    assert_eq!(receiver.product_memo.directory_retained_bytes(), 0);
    assert_eq!(receiver.union_memo.capacity(), 0);
    assert_eq!(receiver.memo_accounting().shared_saturation_adoptions, 1);
    assert_eq!(receiver.memo_accounting().shared_retire_transitions, 1);
    assert!(!receiver.needs_early_shared_publication());
    assert!(matches!(
        receiver.first_pattern_witness(&language, i, &control),
        Err(WasmExactSearchError::InvalidProblem(
            "wasm_standard_bag_first_witness_scope_invalid"
        ))
    ));
    for result in [
        receiver.cover_language(&language, i, &control).map(|_| ()),
        receiver.materialize_root(first.root).map(|_| ()),
        receiver.merge_global(first.root),
        receiver.capture_p7p4_root(first.root).map(|_| ()),
    ] {
        assert!(result.is_err());
    }

    assert!(receiver
        .contains_pattern_language(&language, i, &control)
        .unwrap());
    assert!(!receiver
        .contains_pattern_language(&language, negative, &control)
        .unwrap());
    let before = receiver.product_work_counts();
    assert!(!receiver
        .contains_pattern_language(&language, negative, &control)
        .unwrap());
    assert_eq!(receiver.product_work_counts(), before);
    receiver.flush_and_recycle_local_cache().unwrap();
    language.clear_retain_capacity();
    let recycled = language
        .canonicalize(&language_for_words(&[vec![I, I]]))
        .unwrap();
    assert!(!receiver
        .contains_pattern_language(&language, recycled, &control)
        .unwrap());
    assert!(receiver.has_shared_boolean_saturation());
    assert!(!receiver.global_is_complete());
    assert_eq!(receiver.materialize_global().unwrap(), expected);
    let terminal =
        SharedCoverage::for_standard_bag_request(universe.pattern_count(), Arc::clone(&request));
    let (count, words) = receiver.flushed_global_words().unwrap();
    terminal
        .union_original_words_for_request(Some(&request), count, words)
        .unwrap();
    assert_eq!(terminal.to_bitset().unwrap(), expected);
    assert!(terminal.standard_bag_complete_proof().is_none());

    let tables = Arc::downgrade(&receiver.tables);
    let request_weak = Arc::downgrade(&request);
    drop(publisher);
    drop(request);
    drop(shared);
    drop(terminal);
    assert!(request_weak.upgrade().is_none());
    assert!(tables.upgrade().is_some());
    drop(receiver);
    assert!(
        tables.upgrade().is_none(),
        "the Boolean authority cannot leak its final table lease"
    );
}

#[test]
fn shared_boolean_matches_literal_queue_oracle_across_hold_eof_and_clipped_universes() {
    use PieceKind::{I, J, L, O, T};
    let control = ExecutionControl::default();
    for (draws, limit) in [(2, 0), (3, 13), (6, 43), (7, 43), (8, 43)] {
        let universe = PatternUniverseMaterializer::standard_7_bag(draws, limit, 0x77).unwrap();
        let cyclic = (0..draws)
            .map(|index| PieceKind::STANDARD_TETROMINOES[index % 7])
            .collect::<Vec<_>>();
        let cases = [
            vec![vec![I, L], vec![T, O]],
            vec![vec![I, L], vec![I, O]],
            vec![vec![J, I]],
            vec![cyclic],
            vec![vec![I; draws + 1]],
            vec![vec![O]],
        ];
        for (hold, held) in [
            (false, None),
            (true, None),
            (true, Some(I)),
            (true, Some(T)),
        ] {
            for projected in [false, true] {
                let request = Arc::new(SharedStandardBagRequest::default());
                let mut receiver = owner(&universe, initial(held), hold, projected, &request);
                let shared = SharedCoverage::for_standard_bag_request(
                    universe.pattern_count(),
                    Arc::clone(&request),
                );
                // Fixture publication is the exact full original set. The
                // answer oracle below independently enumerates literal queues.
                shared
                    .union_for_request(
                        Some(&request),
                        &PatternBitSet::all(universe.pattern_count()),
                    )
                    .unwrap();
                receiver
                    .adopt_shared_boolean_saturation(
                        &shared.standard_bag_complete_proof().unwrap(),
                        &control,
                    )
                    .unwrap();
                let mut language = PieceOrderLanguageCache::default();
                for words in &cases {
                    let root = language.canonicalize(&language_for_words(words)).unwrap();
                    let expected =
                        canonical_first(&universe, words, hold, held, projected).is_some();
                    for _ in 0..2 {
                        assert_eq!(receiver.contains_pattern_language(&language, root, &control).unwrap(), expected,
                            "draws={draws} limit={limit} hold={hold} held={held:?} projected={projected} words={words:?}");
                    }
                    receiver.flush_and_recycle_local_cache().unwrap();
                    assert_eq!(
                        receiver
                            .contains_pattern_language(&language, root, &control)
                            .unwrap(),
                        expected
                    );
                }
                assert_eq!(receiver.global_covered_pattern_count, 0);
                assert!(receiver.materialize_global().unwrap().is_empty());
                assert!(!receiver.can_use_first_pattern_witness());
            }
        }
    }
}

#[test]
fn shared_saturation_rejects_foreign_equal_count_and_changed_request_contracts() {
    let control = ExecutionControl::default();
    let universe = PatternUniverseMaterializer::standard_7_bag(3, 13, 0x77).unwrap();
    let request = Arc::new(SharedStandardBagRequest::default());
    let _publisher = owner(&universe, initial(None), false, false, &request);
    let shared = SharedCoverage::for_standard_bag_request(13, Arc::clone(&request));
    shared
        .union_for_request(Some(&request), &PatternBitSet::all(13))
        .unwrap();
    let proof = shared.standard_bag_complete_proof().unwrap();
    let mut observed = initial(None);
    observed.observation = SupplyObservationIdentity::new(observed.observation.policy, 0x88);
    let mut different_provenance = initial(None);
    different_provenance.provenance = SupplyProvenanceId(0x88);
    for (draws, state, hold, projected) in [
        (3, initial(None), false, false), // Same parameters are still another owner.
        (3, initial(None), true, false),
        (3, initial(Some(PieceKind::I)), true, false),
        (3, initial(None), false, true),
        (3, observed, false, false),
        (3, different_provenance, false, false),
        (4, initial(None), false, false), // Equal clipped count, different supply.
    ] {
        let other_universe = PatternUniverseMaterializer::standard_7_bag(draws, 13, 0x77).unwrap();
        let other_request = Arc::new(SharedStandardBagRequest::default());
        let mut other = owner(&other_universe, state, hold, projected, &other_request);
        assert!(matches!(
            other.adopt_shared_boolean_saturation(&proof, &control),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_standard_bag_shared_saturation_request_mismatch"
            ))
        ));
        assert!(!other.has_shared_boolean_saturation());
        assert_eq!(other.product_memo_meaning, ProductMemoMeaning::CoverageRoot);
        assert_eq!(other.memo_accounting().shared_retire_transitions, 0);
    }
    let different_count = PatternUniverseMaterializer::standard_7_bag(3, 12, 0x77).unwrap();
    let other_request = Arc::new(SharedStandardBagRequest::default());
    assert!(owner(
        &different_count,
        initial(None),
        false,
        false,
        &other_request
    )
    .adopt_shared_boolean_saturation(&proof, &control)
    .is_err());
    for (state, hold, projected) in [
        (initial(None), true, false),
        (initial(None), false, true),
        (observed, false, false),
    ] {
        assert!(StandardBagCoverage::for_universe_with_shared(
            &universe, state, hold, projected, &request
        )
        .is_err());
    }
}

#[test]
fn shared_saturation_disabled_and_cancelled_queries_cannot_retire_or_cache_a_negative() {
    let universe = PatternUniverseMaterializer::standard_7_bag(3, 13, 0x77).unwrap();
    let request = Arc::new(SharedStandardBagRequest::default());
    let mut receiver = owner(&universe, initial(None), false, false, &request);
    let shared = SharedCoverage::for_standard_bag_request(13, Arc::clone(&request));
    shared
        .union_for_request(Some(&request), &PatternBitSet::all(13))
        .unwrap();
    let proof = shared.standard_bag_complete_proof().unwrap();
    let control = ExecutionControl::default();
    for (boolean, enabled) in [(false, true), (true, false)] {
        receiver.set_boolean_membership_enabled_for_test(boolean);
        receiver.set_shared_saturation_enabled_for_test(enabled);
        assert!(!receiver
            .adopt_shared_boolean_saturation(&proof, &control)
            .unwrap());
        assert!(!receiver.needs_early_shared_publication());
        assert_eq!(
            receiver.product_memo_meaning,
            ProductMemoMeaning::CoverageRoot
        );
    }
    receiver.set_boolean_membership_enabled_for_test(true);
    receiver.set_shared_saturation_enabled_for_test(true);
    let cancelled = ExecutionControl::default();
    cancelled.cancellation.handle().cancel();
    assert!(matches!(
        receiver.adopt_shared_boolean_saturation(&proof, &cancelled),
        Err(WasmExactSearchError::Cancelled)
    ));
    assert!(!receiver.has_shared_boolean_saturation());
    assert_eq!(
        receiver.product_memo_meaning,
        ProductMemoMeaning::CoverageRoot
    );
    receiver
        .adopt_shared_boolean_saturation(&proof, &control)
        .unwrap();
    let mut language = PieceOrderLanguageCache::default();
    let negative = language
        .canonicalize(&language_for_words(&[vec![PieceKind::O]]))
        .unwrap();
    assert!(matches!(
        receiver.contains_pattern_language(&language, negative, &cancelled),
        Err(WasmExactSearchError::Cancelled)
    ));
    assert_eq!(receiver.memo_accounting().boolean_negative_roots, 0);
    assert!(receiver.materialize_global().unwrap().is_empty());
}

#[test]
fn shared_saturation_mode_parser_accepts_only_explicit_zero_or_one() {
    assert_eq!(
        parse_shared_saturation(Err(std::env::VarError::NotPresent)),
        Ok(true)
    );
    assert_eq!(parse_shared_saturation(Ok("0".into())), Ok(false));
    assert_eq!(parse_shared_saturation(Ok("1".into())), Ok(true));
    for invalid in ["", "true", "false", "2", " 1", "0 "] {
        assert_eq!(
            parse_shared_saturation(Ok(invalid.into())),
            Err("wasm_standard_bag_shared_saturation_mode_invalid")
        );
    }
}
