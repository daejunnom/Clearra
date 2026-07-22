use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ShapeUnionMask(u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CandidateShapeUnionMask(u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct GpuShapeUnionMask(u64);

#[test]
fn shape_union_mask_is_not_pattern_coverage() {
    let shape_union_mask = ShapeUnionMask(0b11);
    let coverage =
        PatternCoverageBitSet::from_patterns(4, [PatternId::new(1)]).expect("coverage bitset");

    assert_eq!(shape_union_mask, ShapeUnionMask(0b11));
    assert_eq!(
        coverage.as_pattern_bitset().covered_patterns(),
        vec![PatternId::new(1)]
    );
}

#[test]
fn shape_mask_cannot_be_used_as_pattern_coverage() {
    let shape_union_mask = ShapeUnionMask(0b0011);
    let candidate_shape_union_mask = CandidateShapeUnionMask(0b0011);
    let gpu_shape_union_mask = GpuShapeUnionMask(0b0011);
    let pattern_coverage_bits =
        PatternCoverageBitSet::from_patterns(4, [PatternId::new(1)]).expect("coverage bitset");
    let pattern_bitset_union = pattern_coverage_bits.as_pattern_bitset().clone();
    let coverage_probability_bits = pattern_bitset_union.covered_patterns();

    assert_eq!(shape_union_mask, ShapeUnionMask(0b0011));
    assert_eq!(candidate_shape_union_mask, CandidateShapeUnionMask(0b0011));
    assert_eq!(gpu_shape_union_mask, GpuShapeUnionMask(0b0011));
    assert_eq!(coverage_probability_bits, vec![PatternId::new(1)]);
}
