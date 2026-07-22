use super::*;

fn verified_variant(pattern_id: u32, candidate_id: u64) -> PatternVerifiedBuildVariant {
    PatternVerifiedBuildVariant::try_new(
        CBuildVariantView::from_native(&CNativeBuildVariantView {
            candidate_id,
            build_variant_id: 1,
            canonical_operation_set_id: candidate_id,
            operation_set_hash: candidate_id,
            coverage_pattern_id: pattern_id,
            queue_cursor: 1,
            ..Default::default()
        })
        .expect("owned variant"),
        PatternCoverageVerification::pattern_specific_buildup(pattern_id),
    )
    .expect("verified variant")
}

#[test]
fn witnessed_pattern_coverage_accumulator_exposes_candidate_identity() {
    let accumulator = WitnessedPatternCoverageAccumulator::new(0xabc, 3);

    assert_eq!(accumulator.candidate_id, 0xabc);
}

#[test]
fn verified_pattern_buildup_sets_coverage_bit_directly() {
    let mut accumulator = WitnessedPatternCoverageAccumulator::new(0xabc, 3);
    accumulator
        .record_verified_variant(&verified_variant(1, 0xabc))
        .expect("verified pattern");

    let coverage = accumulator
        .into_coverage_bits()
        .expect("validated coverage bits");
    assert!(!coverage.contains(PatternId::new(0)));
    assert!(coverage.contains(PatternId::new(1)));
    assert!(!coverage.contains(PatternId::new(2)));
}

#[test]
fn two_verified_patterns_set_two_coverage_bits() {
    let mut accumulator = WitnessedPatternCoverageAccumulator::new(0xabc, 3);
    accumulator
        .record_verified_variant(&verified_variant(0, 0xabc))
        .expect("first verified pattern");
    accumulator
        .record_verified_variant(&verified_variant(1, 0xabc))
        .expect("second verified pattern");

    assert_eq!(
        accumulator
            .into_coverage_bits()
            .expect("validated coverage bits")
            .count_ones(),
        2
    );
}

#[test]
fn duplicate_verified_variants_set_one_pattern_bit() {
    let mut accumulator = WitnessedPatternCoverageAccumulator::new(0xabc, 3);
    accumulator
        .record_verified_variant(&verified_variant(1, 0xabc))
        .expect("first verified variant");
    accumulator
        .record_verified_variant(&verified_variant(1, 0xabc))
        .expect("duplicate verified variant");

    assert_eq!(
        accumulator
            .into_coverage_bits()
            .expect("validated coverage bits")
            .count_ones(),
        1
    );
}
