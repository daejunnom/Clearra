use clearra_core_domain::{
    piece::piece_kind::PieceKind, probability::probability_value::ProbabilityValue,
};

use super::*;

fn provenance() -> SupplyProvenance {
    SupplyProvenance::standard_7_bag()
}

#[test]
fn piece_source_fixed_queue_roundtrip() {
    let source = PieceSource::fixed_queue(
        FixedSequence::new(vec![PieceKind::I, PieceKind::O]),
        provenance(),
    );

    assert_eq!(source.kind, PieceSourceKind::FixedQueue);
    assert_eq!(
        source.fixed_sequence.as_ref().expect("fixed").pieces(),
        &[PieceKind::I, PieceKind::O]
    );
    assert!(source.complete());
    assert_ne!(source.id.get(), 0);
}

#[test]
fn piece_source_bag_universe_roundtrip() {
    let source = PieceSource::bag_universe(
        BagAlignedPattern::new(vec![PieceKind::I, PieceKind::O, PieceKind::T]),
        provenance(),
    );

    assert_eq!(source.kind, PieceSourceKind::BagUniverse);
    assert_eq!(
        source.bag_universe.as_ref().expect("bag").pattern(),
        &[PieceKind::I, PieceKind::O, PieceKind::T]
    );
}

#[test]
fn piece_source_observed_window_roundtrip() {
    let source = PieceSource::observed_window(
        ObservedQueue::new(vec![PieceKind::T, PieceKind::I]),
        provenance(),
        4,
        1,
    )
    .expect("observed universe");

    assert_eq!(source.kind(), PieceSourceKind::ObservedWindow);
    assert!(!source.complete());
    assert_eq!(
        source.truncation_reason(),
        Some(SupplyTruncationReason::ObservedWindowBudgetExceeded)
    );
}

#[test]
fn piece_source_materialized_universe_roundtrip() {
    let universe = MaterializedPatternUniverse::from_sequences(
        PatternUniverseId::new(42),
        PatternWeightModelId::new(7),
        vec![vec![PieceKind::I], vec![PieceKind::O]],
        vec![
            ProbabilityValue::new(0.5).expect("weight"),
            ProbabilityValue::new(0.5).expect("weight"),
        ],
        2,
        true,
        None,
    )
    .expect("materialized universe");
    let source = PieceSource::materialized_pattern_universe(universe, provenance());

    assert_eq!(source.kind(), PieceSourceKind::MaterializedPatternUniverse);
    assert_eq!(
        source.pattern_universe_id(),
        Some(PatternUniverseId::new(42))
    );
    assert_eq!(
        source.pattern_weight_model_id(),
        Some(PatternWeightModelId::new(7))
    );
    assert_eq!(
        source
            .materialized_universe()
            .expect("materialized")
            .pattern_count(),
        2
    );
}

#[test]
fn piece_source_is_immutable_shared_source() {
    let source = PieceSource::fixed_queue(
        FixedSequence::new(vec![PieceKind::I, PieceKind::O]),
        provenance(),
    );

    assert!(source.piece_source_is_immutable_shared_source());
    assert_eq!(source.kind(), PieceSourceKind::FixedQueue);
    assert_eq!(
        source.fixed_sequence().expect("fixed").pieces(),
        &[PieceKind::I, PieceKind::O]
    );
}

#[test]
fn fixed_pattern_and_observed_source_kinds_have_noncolliding_identities() {
    let identity_for = |kind| {
        piece_source_id(
            kind,
            PieceSetId::STANDARD_TETROMINOES,
            0x55,
            Some(PatternUniverseId::new(0x66)),
            Some(PatternWeightModelId::new(0x77)),
            3,
            0x88,
        )
    };
    let fixed = identity_for(PieceSourceKind::FixedQueue);
    let pattern = identity_for(PieceSourceKind::MaterializedPatternUniverse);
    let observed = identity_for(PieceSourceKind::ObservedWindow);

    assert_ne!(fixed, pattern);
    assert_ne!(fixed, observed);
    assert_ne!(pattern, observed);
}

#[test]
fn unlimited_observed_standard_bag_is_lazy_exact_and_bounded() {
    let source = PieceSource::observed_window(ObservedQueue::default(), provenance(), 11, 0)
        .expect("lazy exact observed universe");
    let universe = source.materialized_universe().expect("universe descriptor");

    assert!(matches!(
        universe.structure(),
        crate::pattern_universe::MaterializedPatternUniverseStructure::ObservedStandard7BagLexicographic {
            sequence_len: 11,
            observed_len: 0,
            boundary_candidate_count: 7,
        }
    ));
    assert!(universe.complete());
    assert_eq!(universe.truncation_reason(), None);
    assert!(universe.pattern_count() > 4_233_600);
    assert!(universe
        .lazy_sequence_storage_retained_bytes()
        .is_some_and(|bytes| bytes <= 512));
    assert_eq!(universe.try_sequence_at(0).expect("first").len(), 11);
    assert_eq!(
        universe
            .try_sequence_at(universe.pattern_count() - 1)
            .expect("last")
            .len(),
        11
    );
    assert!(universe.try_sequence_at(universe.pattern_count()).is_none());
}

#[cfg(target_pointer_width = "64")]
#[test]
fn native_six_line_observed_universe_constructs_lazily_but_dense_execution_is_not_bounded() {
    let source = PieceSource::observed_window(ObservedQueue::default(), provenance(), 16, 0)
        .expect("64-bit exact six-line descriptor");
    let universe = source.materialized_universe().expect("universe descriptor");
    let dense_pattern_bitset_bytes = universe.pattern_count().div_ceil(64) * 8;

    assert_eq!(universe.total_possible_pattern_count(), 35_384_428_800);
    assert_eq!(universe.pattern_count(), 35_384_428_800_usize);
    assert!(universe.complete());
    assert!(universe
        .lazy_sequence_storage_retained_bytes()
        .is_some_and(|bytes| bytes <= 512));
    assert_eq!(dense_pattern_bitset_bytes, 4_423_053_600);
    assert!(dense_pattern_bitset_bytes > 4 * 1024 * 1024 * 1024_usize);
}

#[cfg(target_pointer_width = "32")]
#[test]
fn wasm32_six_line_observed_universe_fails_closed_at_addressable_pattern_limit() {
    assert_eq!(
        PatternUniverseMaterializer::observed(&ObservedQueue::default(), 16, 0, 0x6_1e),
        Err(PatternUniverseMaterializationError::PatternCountOverflow)
    );
}

#[test]
fn lazy_observed_order_count_and_weights_match_small_complete_eager_reference() {
    let observed = ObservedQueue::new(vec![PieceKind::I, PieceKind::O]);
    let lazy =
        PatternUniverseMaterializer::observed(&observed, 4, 0, 0x1234).expect("lazy universe");
    let explicit =
        PatternUniverseMaterializer::observed(&observed, 4, lazy.pattern_count(), 0x1234)
            .expect("complete eager reference");

    assert_eq!(lazy.pattern_count(), explicit.pattern_count());
    assert_eq!(
        lazy.total_possible_pattern_count(),
        explicit.total_possible_pattern_count()
    );
    assert!(lazy.complete());
    assert!(explicit.complete());
    for index in 0..lazy.pattern_count() {
        assert_eq!(lazy.sequence_at(index), explicit.sequence_at(index));
        assert_eq!(lazy.weight_at(index), explicit.weight_at(index));
    }
    assert_eq!(
        lazy.materialized_probability_mass(),
        explicit.materialized_probability_mass()
    );
}

#[test]
fn explicit_observed_limits_one_and_complete_keep_existing_materialization_contract() {
    let observed = ObservedQueue::default();
    let one =
        PatternUniverseMaterializer::observed(&observed, 4, 1, 0x77).expect("one explicit pattern");
    assert!(matches!(
        one.structure(),
        crate::pattern_universe::MaterializedPatternUniverseStructure::Explicit
    ));
    assert_eq!(one.pattern_count(), 1);
    assert!(!one.complete());
    assert_eq!(
        one.truncation_reason(),
        Some(SupplyTruncationReason::ObservedWindowBudgetExceeded)
    );

    let lazy = PatternUniverseMaterializer::observed(&observed, 4, 0, 0x77)
        .expect("lazy count descriptor");
    let complete = PatternUniverseMaterializer::observed(&observed, 4, lazy.pattern_count(), 0x77)
        .expect("complete explicit universe");
    assert!(matches!(
        complete.structure(),
        crate::pattern_universe::MaterializedPatternUniverseStructure::Explicit
    ));
    assert!(complete.complete());
    assert_eq!(complete.pattern_count(), lazy.pattern_count());
}

#[test]
fn ambiguous_boundaries_preserve_duplicate_pattern_identity_order_weight_and_mass() {
    let observed = ObservedQueue::default();
    let expansion = crate::normalize::observed_queue_expansion::ObservedQueueExpansion::expand(
        &observed,
        1,
        usize::MAX,
    )
    .expect("complete eager boundary reference");
    let lazy = PatternUniverseMaterializer::observed(&observed, 1, 0, 0x9911)
        .expect("lazy boundary language");
    let explicit =
        PatternUniverseMaterializer::observed(&observed, 1, expansion.pattern_count(), 0x9911)
            .expect("complete explicit boundary language");

    assert_eq!(expansion.pattern_count(), 49);
    assert_eq!(lazy.pattern_count(), expansion.pattern_count());
    assert_eq!(lazy.pattern_universe_id(), explicit.pattern_universe_id());
    assert_eq!(
        lazy.pattern_weight_model_id(),
        explicit.pattern_weight_model_id()
    );
    for (index, eager) in expansion.patterns().iter().enumerate() {
        let expected_piece = PieceKind::STANDARD_TETROMINOES[index % 7];
        assert_eq!(eager.pattern_index(), index);
        assert_eq!(eager.boundary_candidate().initial_offset(), index / 7);
        assert_eq!(eager.queue_pattern().pieces(), &[expected_piece]);
        assert_eq!(lazy.sequence_at(index).as_ref(), &[expected_piece]);
        assert_eq!(explicit.sequence_at(index).as_ref(), &[expected_piece]);
        assert_eq!(lazy.weight_at(index), eager.probability().value());
        assert_eq!(explicit.weight_at(index), eager.probability().value());
    }
    for piece in PieceKind::STANDARD_TETROMINOES {
        assert_eq!(
            expansion
                .patterns()
                .iter()
                .filter(|pattern| pattern.queue_pattern().pieces() == [piece])
                .count(),
            7,
            "one boundary-conditioned identity per possible initial offset"
        );
    }
    assert_eq!(
        lazy.materialized_probability_mass(),
        expansion.materialized_probability_mass()
    );
    assert_eq!(
        explicit.materialized_probability_mass(),
        expansion.materialized_probability_mass()
    );
}
