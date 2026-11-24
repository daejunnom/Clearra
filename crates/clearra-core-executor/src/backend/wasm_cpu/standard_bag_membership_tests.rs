use super::super::first_witness::tests::{canonical_first, fixture, language_for_words};
use super::super::{parse_boolean_membership, ACCEPT};
use super::*;

#[test]
fn boolean_negative_roots_follow_language_owner_and_recycled_ids() {
    use PieceKind::{I, O};
    let control = ExecutionControl::default();
    let (universe, mut coverage) = fixture(3, 13, false, None, false);
    coverage.merge_global(ACCEPT).unwrap();
    let mut language = PieceOrderLanguageCache::default();
    let outside = language
        .canonicalize(&language_for_words(&[vec![O]]))
        .unwrap();
    assert_eq!(
        canonical_first(&universe, &[vec![O]], false, None, false),
        None
    );
    assert!(!coverage
        .contains_pattern_language(&language, outside, &control)
        .unwrap());
    assert!(coverage.boolean_membership.failed_roots.contains(&outside));
    assert_eq!(coverage.boolean_membership.failed_roots.len(), 1);
    let after_cold = coverage.product_work_counts();
    language.clear_coverage_caches();
    assert!(!coverage
        .contains_pattern_language(&language, outside, &control)
        .unwrap());
    assert_eq!(coverage.product_work_counts(), after_cold);
    assert_eq!(coverage.boolean_membership.negative_hits, 1);

    // A second language object deliberately assigns the same root number to
    // a positive word. Numeric IDs and unchanged capacities are not owners.
    let mut other = PieceOrderLanguageCache::default();
    let inside = other.canonicalize(&language_for_words(&[vec![I]])).unwrap();
    assert_eq!(inside, outside);
    assert_eq!(
        canonical_first(&universe, &[vec![I]], false, None, false),
        Some(0)
    );
    assert!(coverage
        .contains_pattern_language(&other, inside, &control)
        .unwrap());
    assert!(coverage.boolean_membership.failed_roots.is_empty());
    assert!(Arc::ptr_eq(
        coverage
            .boolean_membership
            .language_identity
            .as_ref()
            .unwrap(),
        other.arena_identity(),
    ));

    assert!(!coverage
        .contains_pattern_language(&language, outside, &control)
        .unwrap());
    let old_identity = Arc::clone(language.arena_identity());
    language.clear_retain_capacity();
    assert!(!Arc::ptr_eq(&old_identity, language.arena_identity()));
    let reused = language
        .canonicalize(&language_for_words(&[vec![I]]))
        .unwrap();
    assert_eq!(reused, outside);
    assert!(coverage
        .contains_pattern_language(&language, reused, &control)
        .unwrap());
    assert!(coverage.boolean_membership.failed_roots.is_empty());
    assert_eq!(coverage.boolean_membership.negative_hits, 1);
    assert_eq!(coverage.product_memo.len(), 0);
    assert_eq!(coverage.union_memo.len(), 0);
}

#[test]
fn boolean_retires_full_and_minimum_allocations_without_losing_global_coverage() {
    use PieceKind::{I, O};
    let control = ExecutionControl::default();
    let (universe, mut coverage) = fixture(3, 0, false, None, false);
    let mut language = PieceOrderLanguageCache::default();
    let i = language
        .canonicalize(&language_for_words(&[vec![I]]))
        .unwrap();
    let o = language
        .canonicalize(&language_for_words(&[vec![O]]))
        .unwrap();
    let impossible = language
        .canonicalize(&language_for_words(&[vec![I, I]]))
        .unwrap();
    let first = coverage.cover_language(&language, i, &control).unwrap();
    let second = coverage.cover_language(&language, o, &control).unwrap();
    coverage.merge_global(first.root).unwrap();
    coverage.merge_global(second.root).unwrap();
    assert!(coverage.nodes.capacity() > 0);
    assert!(coverage.product_memo.capacity() > 0);
    assert!(coverage.union_memo.capacity() > 0);
    assert!(matches!(
        coverage.contains_pattern_language(&language, i, &control),
        Err(WasmExactSearchError::InvalidProblem(
            "wasm_standard_bag_membership_scope_invalid"
        )),
    ));
    assert_eq!(
        coverage.product_memo_meaning,
        ProductMemoMeaning::CoverageRoot
    );

    coverage.merge_global(ACCEPT).unwrap();
    let original_words = coverage.materialize_global().unwrap().to_owned_words();
    assert!(coverage
        .contains_pattern_language(&language, i, &control)
        .unwrap());
    assert_eq!(
        coverage.product_memo_meaning,
        ProductMemoMeaning::BooleanMembership
    );
    assert_eq!(coverage.product_memo.capacity(), 0);
    assert_eq!(coverage.product_memo.directory_retained_bytes(), 0);
    assert_eq!(coverage.nodes.capacity(), 0);
    assert_eq!(coverage.node_summaries.capacity(), 0);
    assert_eq!(coverage.node_levels.capacity(), 0);
    assert_eq!(coverage.node_valid_masks.capacity(), 0);
    assert_eq!(coverage.bucket_heads.capacity(), 0);
    assert_eq!(coverage.union_memo.capacity(), 0);
    assert!(
        coverage.boolean_membership.failed_roots.is_empty(),
        "partial success is not a failure or full coverage cache entry"
    );
    assert!(!coverage
        .contains_pattern_language(&language, impossible, &control)
        .unwrap());
    assert_eq!(coverage.boolean_membership.failed_roots.len(), 1);
    assert!(coverage.boolean_membership.failed_roots.capacity() > 0);
    for result in [
        coverage.cover_language(&language, i, &control).map(|_| ()),
        coverage.materialize_root(first.root).map(|_| ()),
        coverage.merge_global(first.root),
        coverage.capture_p7p4_root(first.root).map(|_| ()),
    ] {
        assert!(matches!(
            result,
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_standard_bag_coverage_root_after_membership_transition"
            ))
        ));
    }

    // An explicit later representative request gets the real minimum, never
    // the first successful Boolean traversal or its root-number cache value.
    assert_eq!(
        coverage
            .first_pattern_witness(&language, o, &control)
            .unwrap(),
        Some(30)
    );
    assert_eq!(
        coverage.product_memo_meaning,
        ProductMemoMeaning::MinimumOriginalOrdinal
    );
    assert_eq!(coverage.boolean_membership.failed_roots.capacity(), 0);
    assert!(coverage.boolean_membership.language_identity.is_none());
    assert!(coverage.product_memo.capacity() > 0);
    assert!(!coverage
        .contains_pattern_language(&language, impossible, &control)
        .unwrap());
    assert_eq!(coverage.product_memo.capacity(), 0);
    assert_eq!(coverage.boolean_membership.mode_transitions, 2);
    coverage.flush_and_recycle_local_cache().unwrap();
    assert_eq!(
        coverage.product_memo_meaning,
        ProductMemoMeaning::BooleanMembership
    );
    assert!(coverage.boolean_membership.failed_roots.is_empty());
    assert!(coverage.boolean_membership.language_identity.is_none());
    assert!(!coverage
        .contains_pattern_language(&language, impossible, &control)
        .unwrap());
    assert_eq!(
        coverage.materialize_global().unwrap().to_owned_words(),
        original_words
    );
    assert_eq!(
        coverage.materialize_global().unwrap().count_ones() as usize,
        universe.pattern_count()
    );
}

#[test]
fn boolean_cancellation_invalid_roots_and_admission_refusal_never_prove_failure() {
    use PieceKind::{I, O};
    let control = ExecutionControl::default();
    let cancelled = ExecutionControl::default();
    cancelled.cancellation.handle().cancel();
    let mut language = PieceOrderLanguageCache::default();
    let negative = language
        .canonicalize(&language_for_words(&[vec![O]]))
        .unwrap();
    let positive = language
        .canonicalize(&language_for_words(&[vec![I]]))
        .unwrap();
    let (_, mut coverage) = fixture(3, 13, false, None, false);
    coverage.merge_global(ACCEPT).unwrap();
    assert!(matches!(
        coverage.contains_pattern_language(&language, negative, &cancelled),
        Err(WasmExactSearchError::Cancelled)
    ));
    assert_eq!(
        coverage.product_memo_meaning,
        ProductMemoMeaning::CoverageRoot
    );
    assert!(coverage.boolean_membership.failed_roots.is_empty());
    assert!(matches!(
        coverage.contains_pattern_language(&language, u32::MAX, &control),
        Err(WasmExactSearchError::InvalidProblem(
            "wasm_piece_language_node_out_of_range"
        ))
    ));
    assert!(coverage.boolean_membership.failed_roots.is_empty());

    // Exercise the same admission-disabled state reached after try_reserve
    // refusal. Its negative result still comes from a new exhaustive walk.
    coverage
        .begin_boolean_membership_query(&language, negative, &control)
        .unwrap();
    coverage.boolean_membership.admission_disabled = true;
    for _ in 0..2 {
        let before = coverage.product_work_counts();
        assert!(!coverage
            .contains_pattern_language(&language, negative, &control)
            .unwrap());
        assert!(coverage.product_work_counts().0 > before.0);
        assert!(coverage.boolean_membership.failed_roots.is_empty());
        assert!(coverage
            .contains_pattern_language(&language, positive, &control)
            .unwrap());
    }
    assert!(matches!(
        coverage.contains_pattern_language(&language, negative, &cancelled),
        Err(WasmExactSearchError::Cancelled)
    ));
    assert!(coverage.boolean_membership.failed_roots.is_empty());
    coverage.boolean_membership.admission_disabled = false;
    assert!(!coverage
        .contains_pattern_language(&language, negative, &control)
        .unwrap());
    assert_eq!(coverage.boolean_membership.failed_roots.len(), 1);
    assert_eq!(coverage.boolean_membership.negative_hits, 0);
    assert!(!coverage
        .contains_pattern_language(&language, negative, &control)
        .unwrap());
    assert_eq!(coverage.boolean_membership.negative_hits, 1);
}

#[test]
fn boolean_membership_opt_out_is_strict_and_defaults_to_product_behavior() {
    assert_eq!(
        parse_boolean_membership(Err(std::env::VarError::NotPresent)),
        Ok(true)
    );
    assert_eq!(parse_boolean_membership(Ok("1".into())), Ok(true));
    assert_eq!(parse_boolean_membership(Ok("0".into())), Ok(false));
    for invalid in ["", "true", "false", "01", " 1", "0\n", "2", "-1"] {
        assert_eq!(
            parse_boolean_membership(Ok(invalid.into())),
            Err("wasm_standard_bag_boolean_membership_mode_invalid")
        );
    }
    assert_eq!(
        parse_boolean_membership(Err(std::env::VarError::NotUnicode("invalid".into()))),
        Err("wasm_standard_bag_boolean_membership_mode_invalid")
    );
}
