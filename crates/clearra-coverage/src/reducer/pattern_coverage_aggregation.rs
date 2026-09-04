//! Product-neutral aggregation over one exact pattern universe.
//!
//! PC and Build products may select different candidates and objectives, but
//! the arithmetic below is identical once those products have produced a
//! success coverage bitset. This module owns only that shared arithmetic: the
//! universe/weight identity, OR-union, unique success/failure counts,
//! unconditional mass, a success-conditional denominator, and the three
//! completeness inputs needed to decide whether the summary is authoritative.

use clearra_core_domain::probability::probability_value::{
    ProbabilityValue, ProbabilityValueError,
};

use crate::{
    matrix::coverage_matrix_error::CoverageMatrixError,
    pattern::{
        pattern_bitset::PatternBitSet, pattern_id::PatternId,
        weighted_pattern_set::WeightedPatternSet,
    },
    universe::coverage_universe_guard::CoverageUniverseGuard,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PatternCoverageCompleteness {
    source_universe_complete: bool,
    coverage_rows_complete: bool,
    probability_weights_complete: bool,
}

impl PatternCoverageCompleteness {
    pub const fn new(
        source_universe_complete: bool,
        coverage_rows_complete: bool,
        probability_weights_complete: bool,
    ) -> Self {
        Self {
            source_universe_complete,
            coverage_rows_complete,
            probability_weights_complete,
        }
    }

    pub const fn complete() -> Self {
        Self::new(true, true, true)
    }

    pub const fn source_universe_complete(self) -> bool {
        self.source_universe_complete
    }

    pub const fn coverage_rows_complete(self) -> bool {
        self.coverage_rows_complete
    }

    pub const fn probability_weights_complete(self) -> bool {
        self.probability_weights_complete
    }

    pub const fn is_complete(self) -> bool {
        self.source_universe_complete
            && self.coverage_rows_complete
            && self.probability_weights_complete
    }

    pub const fn availability(self) -> PatternCoverageAvailability {
        if self.is_complete() {
            PatternCoverageAvailability::Available
        } else {
            PatternCoverageAvailability::Incomplete
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PatternCoverageAvailability {
    Available,
    Incomplete,
}

impl PatternCoverageAvailability {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Available => "available",
            Self::Incomplete => "incomplete",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PatternCoverageAggregation {
    authority: CoverageUniverseGuard,
    source_row_count: usize,
    success_coverage: PatternBitSet,
    success_pattern_count: usize,
    failed_pattern_count: usize,
    success_probability: ProbabilityValue,
    failed_probability: ProbabilityValue,
    materialized_probability_mass: ProbabilityValue,
    completeness: PatternCoverageCompleteness,
}

impl PatternCoverageAggregation {
    pub const CONTRACT_ID: &'static str = "pattern-coverage-aggregation.v1";

    pub fn from_pattern_sets<'a>(
        authority: CoverageUniverseGuard,
        pattern_sets: impl IntoIterator<Item = &'a PatternBitSet>,
        weights: &WeightedPatternSet,
        completeness: PatternCoverageCompleteness,
    ) -> Result<Self, PatternCoverageAggregationError> {
        if authority.pattern_count() == 0 {
            return Err(PatternCoverageAggregationError::EmptyPatternUniverse);
        }
        let mut success_coverage = PatternBitSet::new(authority.pattern_count());
        let mut source_row_count = 0_usize;
        for patterns in pattern_sets {
            authority
                .check_bits(patterns)
                .map_err(PatternCoverageAggregationError::Universe)?;
            success_coverage
                .union_with(patterns)
                .map_err(PatternCoverageAggregationError::Coverage)?;
            source_row_count = source_row_count
                .checked_add(1)
                .ok_or(PatternCoverageAggregationError::CountOverflow)?;
        }
        Self::from_success_coverage(
            authority,
            source_row_count,
            &success_coverage,
            weights,
            completeness,
        )
    }

    pub fn from_success_coverage(
        authority: CoverageUniverseGuard,
        source_row_count: usize,
        success_coverage: &PatternBitSet,
        weights: &WeightedPatternSet,
        completeness: PatternCoverageCompleteness,
    ) -> Result<Self, PatternCoverageAggregationError> {
        if authority.pattern_count() == 0 {
            return Err(PatternCoverageAggregationError::EmptyPatternUniverse);
        }
        authority
            .check_bits(success_coverage)
            .map_err(PatternCoverageAggregationError::Universe)?;
        if weights.len() != authority.pattern_count() {
            return Err(
                PatternCoverageAggregationError::PatternWeightCountMismatch {
                    expected: authority.pattern_count(),
                    actual: weights.len(),
                },
            );
        }

        let success_pattern_count = usize::try_from(success_coverage.count_ones())
            .map_err(|_| PatternCoverageAggregationError::CountOverflow)?;
        let failed_pattern_count = authority
            .pattern_count()
            .checked_sub(success_pattern_count)
            .ok_or(PatternCoverageAggregationError::CountOverflow)?;
        let success_probability = weights.covered_weight(success_coverage).ok_or(
            PatternCoverageAggregationError::PatternWeightCountMismatch {
                expected: authority.pattern_count(),
                actual: weights.len(),
            },
        )?;
        let failed_probability = uncovered_probability(success_coverage, weights)?;
        let materialized_probability_mass = weights.total_weight();
        let partitioned = success_probability.get() + failed_probability.get();
        if !partitioned.is_finite()
            || (partitioned - materialized_probability_mass.get()).abs()
                > summation_tolerance(authority.pattern_count())
        {
            return Err(PatternCoverageAggregationError::ProbabilityPartitionMismatch);
        }

        Ok(Self {
            authority,
            source_row_count,
            success_coverage: success_coverage.clone(),
            success_pattern_count,
            failed_pattern_count,
            success_probability,
            failed_probability,
            materialized_probability_mass,
            completeness,
        })
    }

    pub const fn contract_id(&self) -> &'static str {
        Self::CONTRACT_ID
    }

    pub const fn authority(&self) -> CoverageUniverseGuard {
        self.authority
    }

    pub const fn source_row_count(&self) -> usize {
        self.source_row_count
    }

    pub fn success_coverage(&self) -> &PatternBitSet {
        &self.success_coverage
    }

    pub const fn success_pattern_count(&self) -> usize {
        self.success_pattern_count
    }

    pub const fn failed_pattern_count(&self) -> usize {
        self.failed_pattern_count
    }

    pub const fn success_probability(&self) -> ProbabilityValue {
        self.success_probability
    }

    pub const fn failed_probability(&self) -> ProbabilityValue {
        self.failed_probability
    }

    pub const fn materialized_probability_mass(&self) -> ProbabilityValue {
        self.materialized_probability_mass
    }

    pub const fn completeness(&self) -> PatternCoverageCompleteness {
        self.completeness
    }

    pub const fn availability(&self) -> PatternCoverageAvailability {
        self.completeness.availability()
    }

    /// Measures one subset against both shared denominators.
    ///
    /// `unconditional_probability` uses the full materialized pattern
    /// universe. `conditional_probability_given_success` divides the same
    /// numerator by this aggregation's success union. Overlapping candidate
    /// subsets are deliberately independent; callers must not add them.
    pub fn probabilities_for_success_subset(
        &self,
        subset: &PatternBitSet,
        weights: &WeightedPatternSet,
    ) -> Result<PatternCoverageSubsetProbability, PatternCoverageAggregationError> {
        self.authority
            .check_bits(subset)
            .map_err(PatternCoverageAggregationError::Universe)?;
        if weights.len() != self.authority.pattern_count() {
            return Err(
                PatternCoverageAggregationError::PatternWeightCountMismatch {
                    expected: self.authority.pattern_count(),
                    actual: weights.len(),
                },
            );
        }
        if (0..subset.word_count()).any(|word_index| {
            subset.word_at(word_index) & !self.success_coverage.word_at(word_index) != 0
        }) {
            return Err(PatternCoverageAggregationError::ConditionalSubsetOutsideSuccess);
        }
        let unconditional_probability = weights.covered_weight(subset).ok_or(
            PatternCoverageAggregationError::PatternWeightCountMismatch {
                expected: self.authority.pattern_count(),
                actual: weights.len(),
            },
        )?;
        let conditional_probability_given_success =
            success_conditional_probability(unconditional_probability, self.success_probability)?;
        Ok(PatternCoverageSubsetProbability {
            success_pattern_count: usize::try_from(subset.count_ones())
                .map_err(|_| PatternCoverageAggregationError::CountOverflow)?,
            unconditional_probability,
            conditional_probability_given_success,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PatternCoverageSubsetProbability {
    success_pattern_count: usize,
    unconditional_probability: ProbabilityValue,
    conditional_probability_given_success: ProbabilityValue,
}

impl PatternCoverageSubsetProbability {
    pub const fn success_pattern_count(self) -> usize {
        self.success_pattern_count
    }

    pub const fn unconditional_probability(self) -> ProbabilityValue {
        self.unconditional_probability
    }

    pub const fn conditional_probability_given_success(self) -> ProbabilityValue {
        self.conditional_probability_given_success
    }
}

pub fn success_conditional_probability(
    numerator: ProbabilityValue,
    success_denominator: ProbabilityValue,
) -> Result<ProbabilityValue, PatternCoverageAggregationError> {
    if numerator == ProbabilityValue::ZERO {
        return Ok(ProbabilityValue::ZERO);
    }
    if success_denominator == ProbabilityValue::ZERO {
        return Err(PatternCoverageAggregationError::ConditionalDenominatorZero);
    }
    let tolerance = summation_tolerance(1);
    if numerator.get() > success_denominator.get() + tolerance {
        return Err(PatternCoverageAggregationError::ConditionalNumeratorExceedsDenominator);
    }
    let conditional = if numerator.get().to_bits() == success_denominator.get().to_bits() {
        1.0
    } else {
        numerator.get() / success_denominator.get()
    };
    ProbabilityValue::new(conditional)
        .map_err(PatternCoverageAggregationError::ConditionalProbability)
}

fn uncovered_probability(
    success_coverage: &PatternBitSet,
    weights: &WeightedPatternSet,
) -> Result<ProbabilityValue, PatternCoverageAggregationError> {
    let mut failed = 0.0_f64;
    for pattern_index in 0..weights.len() {
        if success_coverage.contains(PatternId::new(pattern_index)) {
            continue;
        }
        failed += weights
            .weight(PatternId::new(pattern_index))
            .ok_or(PatternCoverageAggregationError::MissingPatternWeight { pattern_index })?
            .get();
    }
    ProbabilityValue::new(failed).map_err(PatternCoverageAggregationError::FailedProbability)
}

fn summation_tolerance(term_count: usize) -> f64 {
    f64::EPSILON * term_count.max(1) as f64 * 2.0
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PatternCoverageAggregationError {
    EmptyPatternUniverse,
    Universe(CoverageMatrixError),
    Coverage(crate::pattern::pattern_bitset::PatternBitSetError),
    PatternWeightCountMismatch { expected: usize, actual: usize },
    MissingPatternWeight { pattern_index: usize },
    CountOverflow,
    FailedProbability(ProbabilityValueError),
    ProbabilityPartitionMismatch,
    ConditionalSubsetOutsideSuccess,
    ConditionalDenominatorZero,
    ConditionalNumeratorExceedsDenominator,
    ConditionalProbability(ProbabilityValueError),
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::probability::probability_value::ProbabilityValue;

    use crate::{
        pattern::{
            pattern_bitset::PatternBitSet, pattern_id::PatternId,
            weighted_pattern_set::WeightedPatternSet,
        },
        universe::{
            coverage_universe_guard::CoverageUniverseGuard, pattern_universe_id::PatternUniverseId,
            pattern_weight_model_id::PatternWeightModelId,
        },
    };

    use super::{
        PatternCoverageAggregation, PatternCoverageAggregationError, PatternCoverageAvailability,
        PatternCoverageCompleteness,
    };

    fn authority(pattern_count: usize) -> CoverageUniverseGuard {
        CoverageUniverseGuard::new(
            PatternUniverseId::new(17),
            PatternWeightModelId::new(23),
            pattern_count,
        )
    }

    #[test]
    fn overlapping_rows_are_or_unioned_and_success_failure_are_counted_once() {
        let first = PatternBitSet::from_patterns(4, [PatternId::new(0), PatternId::new(1)])
            .expect("first row");
        let second = PatternBitSet::from_patterns(4, [PatternId::new(1), PatternId::new(2)])
            .expect("second row");
        let weights = WeightedPatternSet::uniform(4).expect("uniform weights");

        let aggregate = PatternCoverageAggregation::from_pattern_sets(
            authority(4),
            [&first, &second],
            &weights,
            PatternCoverageCompleteness::complete(),
        )
        .expect("shared aggregate");

        assert_eq!(aggregate.contract_id(), "pattern-coverage-aggregation.v1");
        assert_eq!(aggregate.source_row_count(), 2);
        assert_eq!(aggregate.success_pattern_count(), 3);
        assert_eq!(aggregate.failed_pattern_count(), 1);
        assert_eq!(aggregate.success_probability().get(), 0.75);
        assert_eq!(aggregate.failed_probability().get(), 0.25);
        assert_eq!(aggregate.materialized_probability_mass().get(), 1.0);
        assert_eq!(
            aggregate.availability(),
            PatternCoverageAvailability::Available
        );
    }

    #[test]
    fn subset_reports_full_universe_and_success_conditional_probabilities() {
        let success = PatternBitSet::from_patterns(
            4,
            [PatternId::new(0), PatternId::new(1), PatternId::new(2)],
        )
        .expect("success union");
        let subset = PatternBitSet::from_patterns(4, [PatternId::new(0), PatternId::new(1)])
            .expect("candidate subset");
        let weights = WeightedPatternSet::uniform(4).expect("uniform weights");
        let aggregate = PatternCoverageAggregation::from_success_coverage(
            authority(4),
            2,
            &success,
            &weights,
            PatternCoverageCompleteness::complete(),
        )
        .expect("shared aggregate");

        let probabilities = aggregate
            .probabilities_for_success_subset(&subset, &weights)
            .expect("subset probabilities");

        assert_eq!(probabilities.success_pattern_count(), 2);
        assert_eq!(probabilities.unconditional_probability().get(), 0.5);
        assert_eq!(
            probabilities.conditional_probability_given_success().get(),
            2.0 / 3.0
        );
    }

    #[test]
    fn identity_shape_subset_and_completeness_fail_closed() {
        let success = PatternBitSet::all(2);
        let foreign_subset =
            PatternBitSet::from_patterns(2, [PatternId::new(0)]).expect("foreign subset");
        let weights = WeightedPatternSet::new(vec![
            ProbabilityValue::new(0.25).unwrap(),
            ProbabilityValue::new(0.75).unwrap(),
        ])
        .expect("weights");
        let incomplete = PatternCoverageCompleteness::new(true, false, true);
        let aggregate = PatternCoverageAggregation::from_success_coverage(
            authority(2),
            1,
            &success,
            &weights,
            incomplete,
        )
        .expect("shape-valid incomplete evidence");
        assert_eq!(
            aggregate.availability(),
            PatternCoverageAvailability::Incomplete
        );

        let wrong_shape = PatternBitSet::all(3);
        assert!(matches!(
            PatternCoverageAggregation::from_success_coverage(
                authority(2),
                1,
                &wrong_shape,
                &weights,
                PatternCoverageCompleteness::complete(),
            ),
            Err(PatternCoverageAggregationError::Universe(_))
        ));

        let only_first = PatternCoverageAggregation::from_success_coverage(
            authority(2),
            1,
            &foreign_subset,
            &weights,
            PatternCoverageCompleteness::complete(),
        )
        .expect("one-pattern success");
        assert!(matches!(
            only_first.probabilities_for_success_subset(&success, &weights),
            Err(PatternCoverageAggregationError::ConditionalSubsetOutsideSuccess)
        ));
    }
}
