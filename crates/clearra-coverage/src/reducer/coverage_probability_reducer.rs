use clearra_core_domain::probability::probability_value::ProbabilityValue;

use crate::{
    matrix::coverage_matrix::TypedCoverageMatrix,
    pattern::{pattern_bitset::PatternBitSet, weighted_pattern_set::WeightedPatternSet},
    probability::union_probability::{union_probability, UnionProbabilityError},
};

#[derive(Clone, Debug, PartialEq)]
pub struct CoverageProbabilitySummary {
    row_count: usize,
    covered_patterns: PatternBitSet,
    probability: ProbabilityValue,
}

impl CoverageProbabilitySummary {
    pub fn new(
        row_count: usize,
        covered_patterns: PatternBitSet,
        probability: ProbabilityValue,
    ) -> Self {
        Self {
            row_count,
            covered_patterns,
            probability,
        }
    }
}
impl CoverageProbabilitySummary {
    pub fn row_count(&self) -> usize {
        self.row_count
    }
}
impl CoverageProbabilitySummary {
    pub fn covered_patterns(&self) -> &PatternBitSet {
        &self.covered_patterns
    }
}
impl CoverageProbabilitySummary {
    pub fn probability(&self) -> ProbabilityValue {
        self.probability
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CoverageProbabilityReducerError {
    PatternUniverseMismatch {
        matrix_pattern_count: usize,
        weight_count: usize,
    },
    RowPatternUniverseMismatch {
        expected: usize,
        actual: usize,
    },
    Probability(UnionProbabilityError),
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CoverageProbabilityReducer;

impl CoverageProbabilityReducer {
    pub fn family_probability(
        matrix: &TypedCoverageMatrix,
        weights: &WeightedPatternSet,
    ) -> Result<CoverageProbabilitySummary, CoverageProbabilityReducerError> {
        if matrix.pattern_count() != weights.len() {
            return Err(CoverageProbabilityReducerError::PatternUniverseMismatch {
                matrix_pattern_count: matrix.pattern_count(),
                weight_count: weights.len(),
            });
        }

        Self::family_probability_from_pattern_sets(
            matrix.pattern_count(),
            matrix.rows().iter().map(|row| row.coverage_bits()),
            weights,
        )
    }

    pub fn family_probability_from_pattern_sets<'a>(
        pattern_count: usize,
        pattern_sets: impl IntoIterator<Item = &'a PatternBitSet>,
        weights: &WeightedPatternSet,
    ) -> Result<CoverageProbabilitySummary, CoverageProbabilityReducerError> {
        if pattern_count != weights.len() {
            return Err(CoverageProbabilityReducerError::PatternUniverseMismatch {
                matrix_pattern_count: pattern_count,
                weight_count: weights.len(),
            });
        }
        let mut covered_patterns = PatternBitSet::new(pattern_count);
        let mut row_count = 0_usize;
        for patterns in pattern_sets {
            if patterns.pattern_count() != pattern_count {
                return Err(
                    CoverageProbabilityReducerError::RowPatternUniverseMismatch {
                        expected: pattern_count,
                        actual: patterns.pattern_count(),
                    },
                );
            }
            covered_patterns
                .union_with(patterns)
                .expect("pattern count was checked before union");
            row_count = row_count.saturating_add(1);
        }
        let probability = union_probability(&covered_patterns, weights)
            .map_err(CoverageProbabilityReducerError::Probability)?;
        Ok(CoverageProbabilitySummary::new(
            row_count,
            covered_patterns,
            probability,
        ))
    }
}

#[cfg(test)]
#[path = "coverage_probability_reducer_tests.rs"]
mod tests;
