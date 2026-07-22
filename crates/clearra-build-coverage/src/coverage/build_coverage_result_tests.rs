use clearra_coverage::pattern::{
    pattern_bitset::PatternBitSet, pattern_id::PatternId, weighted_pattern_set::WeightedPatternSet,
};
use clearra_coverage::universe::{
    pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId,
};

use crate::coverage::{
    build_coverage_matrix::BuildCoverageMatrix, build_union_coverage::BuildUnionCoverage,
};

use super::*;

#[test]
fn build_coverage_probability_uses_union() {
    let matrix = BuildCoverageMatrix::from_assignment_coverages(
        11,
        PatternUniverseId::new(1),
        PatternWeightModelId::new(7),
        2,
        vec![
            (
                0,
                PatternBitSet::from_patterns(2, [PatternId::new(0), PatternId::new(1)])
                    .expect("coverage 0"),
            ),
            (
                1,
                PatternBitSet::from_patterns(2, [PatternId::new(0), PatternId::new(1)])
                    .expect("coverage 1"),
            ),
        ],
    )
    .expect("matrix");
    let union = BuildUnionCoverage::from_matrix(matrix.matrix());
    let weights = WeightedPatternSet::uniform(2).expect("weights");

    let result = BuildCoverageResult::from_union(union, &weights).expect("result");

    assert_eq!(result.probability().get(), 1.0);
}

#[test]
fn build_coverage_result_uses_union_probability() {
    let matrix = BuildCoverageMatrix::from_assignment_coverages(
        11,
        PatternUniverseId::new(1),
        PatternWeightModelId::new(7),
        4,
        vec![
            (
                0,
                PatternBitSet::from_patterns(4, [PatternId::new(0), PatternId::new(1)])
                    .expect("coverage 0"),
            ),
            (
                1,
                PatternBitSet::from_patterns(4, [PatternId::new(1), PatternId::new(2)])
                    .expect("coverage 1"),
            ),
        ],
    )
    .expect("matrix");
    let union = BuildUnionCoverage::from_matrix(matrix.matrix());
    let weights = WeightedPatternSet::uniform(4).expect("weights");

    let result = BuildCoverageResult::from_union(union, &weights).expect("result");

    assert_eq!(result.union_coverage().covered_patterns().count_ones(), 3);
    assert_eq!(result.probability().get(), 0.75);
}

#[test]
fn build_coverage_uses_union_probability() {
    let matrix = BuildCoverageMatrix::from_assignment_coverages(
        11,
        PatternUniverseId::new(1),
        PatternWeightModelId::new(7),
        3,
        vec![
            (
                0,
                PatternBitSet::from_patterns(3, [PatternId::new(0), PatternId::new(1)])
                    .expect("coverage 0"),
            ),
            (
                1,
                PatternBitSet::from_patterns(3, [PatternId::new(1), PatternId::new(2)])
                    .expect("coverage 1"),
            ),
        ],
    )
    .expect("matrix");
    let union = BuildUnionCoverage::from_matrix(matrix.matrix());
    let weights = WeightedPatternSet::uniform(3).expect("weights");

    let result = BuildCoverageResult::from_union(union, &weights).expect("result");

    assert_eq!(result.union_coverage().covered_patterns().count_ones(), 3);
    assert_eq!(result.probability().get(), 1.0);
}
