use clearra_core_domain::probability::probability_value::ProbabilityValue;

use crate::{
    matrix::coverage_matrix::TypedCoverageMatrix,
    pattern::{
        pattern_bitset::PatternBitSet, pattern_id::PatternId,
        weighted_pattern_set::WeightedPatternSet,
    },
    reducer::coverage_probability_reducer::CoverageProbabilityReducer,
    row::{coverage_row::CoverageRow, coverage_row_kind::CoverageRowKind},
    universe::{
        pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId,
    },
};

fn weight(value: f64) -> ProbabilityValue {
    ProbabilityValue::new(value).expect("valid probability")
}

fn row_for_kind(
    row_kind: CoverageRowKind,
    candidate_id: usize,
    pattern_count: usize,
    patterns: &[usize],
) -> CoverageRow {
    CoverageRow::new_with_piece_source(
        candidate_id as u64,
        row_kind,
        11,
        PatternUniverseId::new(1),
        PatternWeightModelId::new(7),
        PatternBitSet::from_patterns(pattern_count, patterns.iter().copied().map(PatternId::new))
            .expect("patterns"),
    )
}

fn row(candidate_id: usize, pattern_count: usize, patterns: &[usize]) -> CoverageRow {
    row_for_kind(CoverageRowKind::Pc, candidate_id, pattern_count, patterns)
}

#[test]
fn variant_coverage_is_not_summed_for_duplicate_patterns() {
    let matrix = TypedCoverageMatrix::from_rows(
        CoverageRowKind::Pc,
        PatternUniverseId::new(1),
        PatternWeightModelId::new(7),
        2,
        vec![row(1, 2, &[0]), row(2, 2, &[0])],
    )
    .expect("matrix");
    let weights = WeightedPatternSet::new(vec![weight(0.7), weight(0.3)]).expect("weights");

    let summary =
        CoverageProbabilityReducer::family_probability(&matrix, &weights).expect("summary");

    assert_eq!(summary.row_count(), 2);
    assert_eq!(summary.covered_patterns().count_ones(), 1);
    assert_eq!(summary.probability().get(), 0.7);
}

#[test]
fn coverage_union_does_not_sum_variant_probability() {
    let matrix = TypedCoverageMatrix::from_rows(
        CoverageRowKind::Build,
        PatternUniverseId::new(1),
        PatternWeightModelId::new(7),
        2,
        vec![
            row_for_kind(CoverageRowKind::Build, 1, 2, &[0]),
            row_for_kind(CoverageRowKind::Build, 2, 2, &[0]),
        ],
    )
    .expect("matrix");
    let weights = WeightedPatternSet::new(vec![weight(0.4), weight(0.6)]).expect("weights");

    let summary =
        CoverageProbabilityReducer::family_probability(&matrix, &weights).expect("summary");

    assert_eq!(summary.row_count(), 2);
    assert_eq!(
        summary.covered_patterns().covered_patterns(),
        vec![PatternId::new(0)]
    );
    assert_eq!(summary.probability().get(), 0.4);
}

#[test]
fn variant_probability_not_summed() {
    coverage_union_does_not_sum_variant_probability();
}

#[test]
fn family_probability_uses_or_union_not_row_probability_sum() {
    let matrix = TypedCoverageMatrix::from_rows(
        CoverageRowKind::Pc,
        PatternUniverseId::new(1),
        PatternWeightModelId::new(7),
        4,
        vec![row(1, 4, &[0, 1]), row(2, 4, &[1, 2])],
    )
    .expect("matrix");
    let weights = WeightedPatternSet::uniform(4).expect("uniform weights");

    let summary =
        CoverageProbabilityReducer::family_probability(&matrix, &weights).expect("summary");

    assert_eq!(summary.covered_patterns().covered_patterns().len(), 3);
    assert_eq!(summary.probability().get(), 0.75);
}

#[test]
fn family_probability_uses_pattern_bitset_or() {
    let matrix = TypedCoverageMatrix::from_rows(
        CoverageRowKind::Setup,
        PatternUniverseId::new(1),
        PatternWeightModelId::new(7),
        4,
        vec![
            row_for_kind(CoverageRowKind::Setup, 1, 4, &[0, 1]),
            row_for_kind(CoverageRowKind::Setup, 2, 4, &[1, 2]),
        ],
    )
    .expect("matrix");
    let weights = WeightedPatternSet::uniform(4).expect("uniform weights");

    let summary =
        CoverageProbabilityReducer::family_probability(&matrix, &weights).expect("summary");

    assert_eq!(
        summary.covered_patterns().covered_patterns(),
        vec![PatternId::new(0), PatternId::new(1), PatternId::new(2)]
    );
    assert_eq!(summary.probability().get(), 0.75);
}

#[test]
fn probability_never_exceeds_one() {
    let matrix = TypedCoverageMatrix::from_rows(
        CoverageRowKind::Pc,
        PatternUniverseId::new(1),
        PatternWeightModelId::new(7),
        2,
        vec![row(1, 2, &[0, 1]), row(2, 2, &[0, 1])],
    )
    .expect("matrix");
    let weights = WeightedPatternSet::new(vec![weight(0.6), weight(0.4)]).expect("weights");

    let summary =
        CoverageProbabilityReducer::family_probability(&matrix, &weights).expect("summary");

    assert_eq!(summary.covered_patterns().count_ones(), 2);
    assert_eq!(summary.probability().get(), 1.0);
    assert!(summary.probability().get() <= 1.0);
}
