use super::*;

#[test]
fn coverage_row_view_matches_c_layout_shape() {
    assert_eq!(core::mem::size_of::<CPatternBitSet>(), 152);
    assert_eq!(core::mem::size_of::<CCoverageRowView>(), 192);
    assert_eq!(core::mem::size_of::<CCoverageOverlapReport>(), 8);
    assert_eq!(C_SCORE_MATRIX_CAPACITY_EXCEEDED, 7);
    assert_eq!(C_SPIN_COVERAGE_CAPACITY_EXCEEDED, 8);
    assert_eq!(C_COVERAGE_PIECE_SOURCE_MISMATCH, 9);
}

#[test]
fn single_pattern_sets_expected_word() {
    let view = CCoverageRowView::single_pattern(55, 128, 65).expect("view");

    assert_eq!(view.candidate_id, 55);
    assert_eq!(view.piece_source_id, 0);
    assert_eq!(view.row_kind, C_COVERAGE_ROW_KIND_BUILD);
    assert_eq!(view.pattern_universe_id, 0);
    assert_eq!(view.pattern_weight_model_id, 0);
    assert_eq!(view.patterns.pattern_universe_id, 0);
    assert_eq!(view.patterns.pattern_weight_model_id, 0);
    assert_eq!(view.patterns.word_count, 2);
    assert_eq!(view.patterns.words[1], 2);
}

#[test]
fn single_pattern_with_identity_sets_row_and_bitset_identity() {
    let view = CCoverageRowView::single_pattern_with_identity(55, 42, 7, 9, 128, 65).expect("view");

    assert_eq!(view.candidate_id, 55);
    assert_eq!(view.piece_source_id, 0);
    assert_eq!(view.row_kind, 42);
    assert_eq!(view.pattern_universe_id, 7);
    assert_eq!(view.pattern_weight_model_id, 9);
    assert_eq!(view.patterns.pattern_universe_id, 7);
    assert_eq!(view.patterns.pattern_weight_model_id, 9);
    assert_eq!(view.patterns.word_count, 2);
    assert_eq!(view.patterns.words[1], 2);
}

#[test]
fn single_pattern_with_identity_rejects_default_zero_identity() {
    assert!(CCoverageRowView::single_pattern_with_identity(
        55,
        C_COVERAGE_ROW_KIND_BUILD,
        0,
        9,
        8,
        3
    )
    .is_none());
    assert!(CCoverageRowView::single_pattern_with_identity(
        55,
        C_COVERAGE_ROW_KIND_BUILD,
        7,
        0,
        8,
        3
    )
    .is_none());
}

#[test]
fn single_pattern_with_identity_preserves_piece_source_id() {
    let view =
        CCoverageRowView::single_pattern_with_identity_and_piece_source(55, 123, 42, 7, 9, 128, 65)
            .expect("view");

    assert_eq!(view.candidate_id, 55);
    assert_eq!(view.piece_source_id, 123);
    assert_eq!(view.pattern_universe_id, 7);
    assert_eq!(view.pattern_weight_model_id, 9);
    assert_eq!(view.patterns.words[1], 2);
}

#[test]
fn c_coverage_row_view_can_be_read_by_rust_coverage_layer() {
    let view = CCoverageRowView::single_pattern(77, 8, 3).expect("view");

    let row = clearra_coverage::matrix::coverage_row_bridge::coverage_row_from_raw_words_with_identity_and_piece_source(
        view.candidate_id,
        clearra_coverage::row::coverage_row_kind::CoverageRowKind::Build,
        11,
        7,
        9,
        view.patterns.pattern_count as usize,
        view.patterns.word_count as usize,
        &view.patterns.words,
    )
    .expect("coverage row");

    assert_eq!(row.candidate_id(), 77);
    assert!(row
        .coverage_bits()
        .contains(clearra_coverage::pattern::pattern_id::PatternId::new(3)));
}

#[test]
fn c_pattern_bitset_words_do_not_escape_scope() {
    let mut bitset = CPatternBitSet::single(128, 65).expect("bitset");
    bitset.pattern_universe_id = 7;
    bitset.pattern_weight_model_id = 9;

    let snapshot = bitset.owned_snapshot().expect("owned snapshot");
    bitset.words[1] = 0;

    assert_eq!(bitset.words[1], 0);
    assert_eq!(snapshot.pattern_universe_id(), 7);
    assert_eq!(snapshot.pattern_weight_model_id(), 9);
    assert_eq!(snapshot.pattern_count(), 128);
    assert_eq!(snapshot.words(), &[0, 2]);
}

#[test]
fn ffi_pattern_bitset_pointer_escape_is_blocked_by_owned_snapshot() {
    let bitset = CPatternBitSet::single(128, 65).expect("bitset");
    let snapshot = bitset.owned_snapshot().expect("owned snapshot");

    assert_ne!(snapshot.words().as_ptr(), bitset.words.as_ptr());
    assert_eq!(snapshot.words(), &[0, 2]);
}

#[test]
fn c_coverage_capacity_and_rust_pattern_limit_are_aligned() {
    use clearra_coverage::universe::CoveragePatternBudget;

    assert_eq!(
        CoveragePatternBudget::c_bridge_default().max_pattern_count(),
        Some(C_COVERAGE_MAX_PATTERNS)
    );
    assert_eq!(
        CoveragePatternBudget::product_unbounded().check(C_COVERAGE_MAX_PATTERNS + 1),
        Ok(())
    );
}

#[test]
fn coverage_row_view_uses_build_variant_identity_and_pattern() {
    let variant = CNativeBuildVariantView {
        candidate_id: 0x1234,
        canonical_operation_set_id: 0x1234,
        operation_set_hash: 0x55,
        coverage_pattern_id: 5,
        ..Default::default()
    };

    let view = CCoverageRowView::product_from_build_variant_with_identity(
        &variant,
        123,
        C_COVERAGE_ROW_KIND_BUILD,
        7,
        9,
        8,
    )
    .expect("view");

    assert_eq!(view.candidate_id, 0x1234);
    assert_eq!(view.piece_source_id, 123);
    assert_eq!(view.coverage_pattern_id, 5);
    assert_eq!(view.pattern_universe_id, 7);
    assert_eq!(view.pattern_weight_model_id, 9);
    assert_eq!(view.patterns.pattern_universe_id, 7);
    assert_eq!(view.patterns.pattern_weight_model_id, 9);
    assert_eq!(view.patterns.words[0], 1_u64 << 5);
}

#[test]
fn coverage_row_identity_roundtrips_from_native_build_variant() {
    let variant = CNativeBuildVariantView {
        candidate_id: 0xabc_def,
        canonical_operation_set_id: 0xabc_def,
        operation_set_hash: 0x1234,
        coverage_pattern_id: 6,
        ..Default::default()
    };
    let view = CCoverageRowView::product_from_build_variant_with_identity(
        &variant,
        123,
        C_COVERAGE_ROW_KIND_BUILD,
        77,
        99,
        16,
    )
    .expect("native row view");

    let row = clearra_coverage::matrix::coverage_row_bridge::coverage_row_from_raw_words_with_identity_and_piece_source(
        view.candidate_id,
        clearra_coverage::row::CoverageRowKind::Build,
        view.piece_source_id,
        view.pattern_universe_id,
        view.pattern_weight_model_id,
        view.patterns.pattern_count as usize,
        view.patterns.word_count as usize,
        &view.patterns.words,
    )
    .expect("typed coverage row");

    assert_eq!(row.candidate_id(), 0xabc_def);
    assert_eq!(row.piece_source_id(), 123);
    assert_eq!(
        row.pattern_universe_id(),
        clearra_coverage::universe::pattern_universe_id::PatternUniverseId::new(77)
    );
    assert_eq!(
        row.pattern_weight_model_id(),
        clearra_coverage::universe::pattern_weight_model_id::PatternWeightModelId::new(99)
    );
    assert!(row
        .coverage_bits()
        .contains(clearra_coverage::pattern::pattern_id::PatternId::new(6)));
}
