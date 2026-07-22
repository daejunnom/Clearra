use clearra_core_domain::{ids::SpinTargetId, probability::probability_value::ProbabilityValue};

use crate::{
    matrix::{
        coverage_matrix::{CoverageMatrix, CoverageMatrixError, TypedCoverageMatrix},
        coverage_row::CoverageRow as UntypedCoverageRow,
        score_cell_matrix::ScoreCellMatrix,
        spin_coverage_matrix::SpinCoverageMatrix,
    },
    pattern::{
        pattern_bitset::PatternBitSet, pattern_id::PatternId,
        weighted_pattern_set::WeightedPatternSet,
    },
    probability::union_probability::union_probability,
    row::{
        coverage_row::CoverageRow,
        coverage_row_kind::{CoverageRowKind, ScoreObjectiveCellId},
        score_cell_row::ScoreCellRow,
        spin_coverage_row::SpinCoverageRow,
    },
    universe::{
        pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId,
    },
};

fn universe(value: u64) -> PatternUniverseId {
    PatternUniverseId::new(value)
}

fn weight_model(value: u64) -> PatternWeightModelId {
    PatternWeightModelId::new(value)
}

fn bitset(pattern_count: usize, patterns: &[usize]) -> PatternBitSet {
    PatternBitSet::from_patterns(pattern_count, patterns.iter().copied().map(PatternId::new))
        .expect("valid test pattern bitset")
}

fn probability(value: f64) -> ProbabilityValue {
    ProbabilityValue::new(value).expect("valid probability")
}

mod case_variant_order_does_not_change_union_probability {
    use super::*;

    #[test]
    fn variant_order_does_not_change_union_probability() {
        let weights = WeightedPatternSet::uniform(4).expect("uniform weights");
        let row_a = UntypedCoverageRow::new(10, bitset(4, &[0, 1]));
        let row_b = UntypedCoverageRow::new(20, bitset(4, &[1, 2]));
        let matrix_ab =
            CoverageMatrix::from_rows(4, vec![row_a.clone(), row_b.clone()]).expect("matrix AB");
        let matrix_ba = CoverageMatrix::from_rows(4, vec![row_b, row_a]).expect("matrix BA");

        let probability_ab = union_probability(&matrix_ab.union_all(), &weights).expect("AB");
        let probability_ba = union_probability(&matrix_ba.union_all(), &weights).expect("BA");

        assert_eq!(probability_ab, probability_ba);
        assert_eq!(probability_ab.get(), 0.75);
    }
}

mod case_duplicate_variant_patterns_do_not_exceed_one_hundred_percent {
    use super::*;

    #[test]
    fn duplicate_variant_patterns_do_not_exceed_one_hundred_percent() {
        let weights = WeightedPatternSet::uniform(2).expect("uniform weights");
        let row_a = UntypedCoverageRow::new(1, bitset(2, &[0, 1]));
        let row_b = UntypedCoverageRow::new(2, bitset(2, &[0, 1]));
        let matrix = CoverageMatrix::from_rows(2, vec![row_a, row_b]).expect("matrix");

        let probability = union_probability(&matrix.union_all(), &weights).expect("probability");

        assert_eq!(probability.get(), 1.0);
    }
}

mod case_product_acceptance_overlap_two_variants_one_pattern_uses_union_probability {
    use super::*;

    #[test]
    fn product_acceptance_overlap_two_variants_one_pattern_uses_union_probability() {
        assert!(include_str!(
            "../../../tests/fixtures/coverage/overlap_two_variants_one_pattern.json"
        )
        .contains("overlap_two_variants_one_pattern"));
        assert!(
            include_str!("../../../tests/golden/coverage/overlap_union_probability.json")
                .contains("variant_probability_sum=forbidden")
        );

        let mut matrix = TypedCoverageMatrix::new(
            CoverageRowKind::Build,
            universe(1001),
            weight_model(2001),
            2,
        );
        matrix
            .push(CoverageRow::new_with_piece_source(
                10,
                CoverageRowKind::Build,
                11,
                universe(1001),
                weight_model(2001),
                bitset(2, &[0]),
            ))
            .expect("first row");
        matrix
            .push(CoverageRow::new_with_piece_source(
                11,
                CoverageRowKind::Build,
                11,
                universe(1001),
                weight_model(2001),
                bitset(2, &[0]),
            ))
            .expect("second row");

        let union = matrix.union_all();
        let weights =
            WeightedPatternSet::new(vec![probability(0.4), probability(0.6)]).expect("weights");
        let probability = union_probability(&union, &weights).expect("union probability");

        assert_eq!(matrix.rows().len(), 2);
        assert_eq!(union.count_ones(), 1);
        assert_eq!(union.covered_patterns(), vec![PatternId::new(0)]);
        assert_eq!(probability.get(), 0.4);
    }
}

mod case_coverage_matrix_rejects_mixed_pattern_universe {
    use super::*;

    #[test]
    fn coverage_matrix_rejects_mixed_pattern_universe() {
        let mut matrix =
            TypedCoverageMatrix::new(CoverageRowKind::Pc, universe(1), weight_model(7), 4);
        let row = CoverageRow::new_with_piece_source(
            10,
            CoverageRowKind::Pc,
            11,
            universe(2),
            weight_model(7),
            bitset(4, &[0]),
        );

        let result = matrix.push(row);

        assert_eq!(
            result,
            Err(CoverageMatrixError::PatternUniverseIdMismatch {
                expected: universe(1),
                actual: universe(2)
            })
        );
    }
}

mod case_coverage_row_rejects_universe_mismatch {
    use super::*;

    #[test]
    fn coverage_row_rejects_universe_mismatch() {
        let mut matrix =
            TypedCoverageMatrix::new(CoverageRowKind::Pc, universe(1), weight_model(7), 4);
        let row = CoverageRow::new_with_piece_source(
            10,
            CoverageRowKind::Pc,
            11,
            universe(2),
            weight_model(7),
            bitset(4, &[0]),
        );

        assert_eq!(
            matrix.push(row),
            Err(CoverageMatrixError::PatternUniverseIdMismatch {
                expected: universe(1),
                actual: universe(2)
            })
        );
    }
}

mod case_coverage_matrix_rejects_mixed_weight_model {
    use super::*;

    #[test]
    fn coverage_matrix_rejects_mixed_weight_model() {
        let mut matrix =
            TypedCoverageMatrix::new(CoverageRowKind::Build, universe(1), weight_model(7), 4);
        let row = CoverageRow::new_with_piece_source(
            10,
            CoverageRowKind::Build,
            11,
            universe(1),
            weight_model(8),
            bitset(4, &[0]),
        );

        let result = matrix.push(row);

        assert_eq!(
            result,
            Err(CoverageMatrixError::PatternWeightModelIdMismatch {
                expected: weight_model(7),
                actual: weight_model(8)
            })
        );
    }
}

mod case_coverage_row_rejects_weight_model_mismatch {
    use super::*;

    #[test]
    fn coverage_row_rejects_weight_model_mismatch() {
        let mut matrix =
            TypedCoverageMatrix::new(CoverageRowKind::Build, universe(1), weight_model(7), 4);
        let row = CoverageRow::new_with_piece_source(
            10,
            CoverageRowKind::Build,
            11,
            universe(1),
            weight_model(8),
            bitset(4, &[0]),
        );

        assert_eq!(
            matrix.push(row),
            Err(CoverageMatrixError::PatternWeightModelIdMismatch {
                expected: weight_model(7),
                actual: weight_model(8)
            })
        );
    }
}

mod case_coverage_row_rejects_piece_source_mismatch {
    use super::*;

    #[test]
    pub(crate) fn coverage_row_rejects_piece_source_mismatch() {
        let mut matrix = TypedCoverageMatrix::new_with_piece_source(
            CoverageRowKind::Build,
            11,
            universe(1),
            weight_model(7),
            4,
        );
        let row = CoverageRow::new_with_piece_source(
            10,
            CoverageRowKind::Build,
            12,
            universe(1),
            weight_model(7),
            bitset(4, &[0]),
        );

        assert_eq!(
            matrix.push(row),
            Err(CoverageMatrixError::PieceSourceIdMismatch {
                expected: 11,
                actual: 12
            })
        );
    }
}
pub(crate) use case_coverage_row_rejects_piece_source_mismatch::coverage_row_rejects_piece_source_mismatch;

mod case_coverage_union_rejects_piece_source_mismatch {
    use super::*;

    #[test]
    fn coverage_union_rejects_piece_source_mismatch() {
        coverage_row_rejects_piece_source_mismatch();
    }
}

mod case_coverage_matrix_latches_nonzero_piece_source_identity {
    use super::*;

    #[test]
    fn coverage_matrix_latches_nonzero_piece_source_identity() {
        let mut matrix =
            TypedCoverageMatrix::new(CoverageRowKind::Build, universe(1), weight_model(7), 4);
        matrix
            .push(CoverageRow::new_with_piece_source(
                10,
                CoverageRowKind::Build,
                11,
                universe(1),
                weight_model(7),
                bitset(4, &[0]),
            ))
            .expect("first row establishes piece source");

        let result = matrix.push(CoverageRow::new_with_piece_source(
            11,
            CoverageRowKind::Build,
            12,
            universe(1),
            weight_model(7),
            bitset(4, &[1]),
        ));

        assert_eq!(matrix.piece_source_id(), Some(11));
        assert_eq!(
            result,
            Err(CoverageMatrixError::PieceSourceIdMismatch {
                expected: 11,
                actual: 12
            })
        );
    }
}

mod case_coverage_row_rejects_row_kind_mismatch {
    use super::*;

    #[test]
    fn coverage_row_rejects_row_kind_mismatch() {
        let mut matrix =
            TypedCoverageMatrix::new(CoverageRowKind::Build, universe(1), weight_model(7), 4);
        let row = CoverageRow::new_with_piece_source(
            10,
            CoverageRowKind::Setup,
            11,
            universe(1),
            weight_model(7),
            bitset(4, &[0]),
        );

        assert_eq!(
            matrix.push(row),
            Err(CoverageMatrixError::RowKindMismatch {
                expected: CoverageRowKind::Build,
                actual: CoverageRowKind::Setup
            })
        );
    }
}

mod case_coverage_row_kind_spin_target_uses_same_union_invariant {
    use super::*;

    #[test]
    fn coverage_row_kind_spin_target_uses_same_union_invariant() {
        let spin_target_id = SpinTargetId::new("tsd");
        let mut matrix =
            SpinCoverageMatrix::new(spin_target_id.clone(), universe(10), weight_model(20), 4);

        matrix
            .push(SpinCoverageRow::new(
                1,
                11,
                spin_target_id.clone(),
                universe(10),
                weight_model(20),
                bitset(4, &[0, 1]),
            ))
            .expect("first spin row");
        matrix
            .push(SpinCoverageRow::new(
                2,
                11,
                spin_target_id,
                universe(10),
                weight_model(20),
                bitset(4, &[1, 2]),
            ))
            .expect("second spin row");

        let union = matrix.union_all();
        let weights = WeightedPatternSet::uniform(4).expect("uniform weights");
        let probability = union_probability(&union, &weights).expect("union probability");

        assert_eq!(
            union.covered_patterns(),
            vec![PatternId::new(0), PatternId::new(1), PatternId::new(2)]
        );
        assert_eq!(probability.get(), 0.75);
    }
}

mod case_spin_probability_uses_pattern_bitset_union {
    use super::*;

    #[test]
    fn spin_probability_uses_pattern_bitset_union() {
        let spin_target_id = SpinTargetId::new("tsd");
        let mut matrix =
            SpinCoverageMatrix::new(spin_target_id.clone(), universe(10), weight_model(20), 4);

        matrix
            .push(SpinCoverageRow::new(
                1,
                11,
                spin_target_id.clone(),
                universe(10),
                weight_model(20),
                bitset(4, &[0, 1]),
            ))
            .expect("first spin row");
        matrix
            .push(SpinCoverageRow::new(
                2,
                11,
                spin_target_id,
                universe(10),
                weight_model(20),
                bitset(4, &[1, 2]),
            ))
            .expect("second spin row");

        let weights = WeightedPatternSet::uniform(4).expect("uniform weights");
        let probability =
            union_probability(&matrix.union_all(), &weights).expect("union probability");

        assert_eq!(matrix.union_all().count_ones(), 3);
        assert_eq!(probability.get(), 0.75);
    }
}

mod case_score_cell_matrix_does_not_change_probability_union {
    use super::*;

    #[test]
    fn score_cell_matrix_does_not_change_probability_union() {
        let score_cell_id = ScoreObjectiveCellId::new("back-to-back-tspin");
        let mut matrix =
            ScoreCellMatrix::new(score_cell_id.clone(), universe(11), weight_model(21), 2);

        matrix
            .push(ScoreCellRow::new(
                1,
                11,
                score_cell_id.clone(),
                universe(11),
                weight_model(21),
                bitset(2, &[0]),
            ))
            .expect("first score row");
        matrix
            .push(ScoreCellRow::new(
                2,
                11,
                score_cell_id,
                universe(11),
                weight_model(21),
                bitset(2, &[0]),
            ))
            .expect("second score row");

        let weights =
            WeightedPatternSet::new(vec![probability(0.7), probability(0.3)]).expect("weights");
        let probability =
            union_probability(&matrix.union_all(), &weights).expect("union probability");

        assert_eq!(matrix.union_all().count_ones(), 1);
        assert_eq!(probability.get(), 0.7);
    }
}

mod case_score_does_not_change_coverage_probability {
    use super::*;

    #[test]
    fn score_does_not_change_coverage_probability() {
        let score_cell_id = ScoreObjectiveCellId::new("back-to-back-tspin");
        let mut matrix =
            ScoreCellMatrix::new(score_cell_id.clone(), universe(11), weight_model(21), 2);

        matrix
            .push(ScoreCellRow::new(
                1,
                11,
                score_cell_id.clone(),
                universe(11),
                weight_model(21),
                bitset(2, &[0]),
            ))
            .expect("first score row");
        matrix
            .push(ScoreCellRow::new(
                2,
                11,
                score_cell_id,
                universe(11),
                weight_model(21),
                bitset(2, &[0]),
            ))
            .expect("second score row");

        let weights =
            WeightedPatternSet::new(vec![probability(0.7), probability(0.3)]).expect("weights");
        let probability =
            union_probability(&matrix.union_all(), &weights).expect("union probability");

        assert_eq!(matrix.union_all().count_ones(), 1);
        assert_eq!(probability.get(), 0.7);
    }
}

mod case_pattern_bitset_capacity_error_is_not_silent_truncation {
    use super::*;

    #[test]
    fn pattern_bitset_capacity_error_is_not_silent_truncation() {
        let result = TypedCoverageMatrix::with_capacity_limit(
            CoverageRowKind::Setup,
            universe(1),
            weight_model(1),
            1025,
            1024,
        );

        assert_eq!(
            result,
            Err(CoverageMatrixError::PatternBitSetCapacityExceeded {
                pattern_count: 1025,
                max_pattern_count: 1024
            })
        );
    }
}
