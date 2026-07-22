mod expander {
    use crate::{
        bag::{
            bag_boundary::BagBoundaryReport, bag_probability::BagProbability,
            bag_profile::BagProfile,
        },
        normalize::observed_suffix_enumerator::{
            enumerate_visible_suffixes_with_profile, materialized_probabilities,
            total_visible_suffix_count_with_profile,
        },
        queue::observed_queue::ObservedQueue,
    };

    use super::{
        ObservedQueueExpansion, ObservedQueueExpansionError, ObservedQueuePattern,
        ObservedQueuePatternSet,
    };

    pub(super) fn expand_observed_queue(
        queue: &ObservedQueue,
        minimum_len: usize,
        max_patterns: usize,
        bag_profile: &BagProfile,
    ) -> Result<ObservedQueueExpansion, ObservedQueueExpansionError> {
        if max_patterns == 0 {
            return Err(ObservedQueueExpansionError::ZeroPatternLimit);
        }

        let boundary_report =
            BagBoundaryReport::analyze_observed_window_with_profile(queue.pieces(), bag_profile);
        if !boundary_report.is_compatible() {
            return Err(ObservedQueueExpansionError::IncompatibleBoundary);
        }

        let target_len = minimum_len.max(queue.len());
        let total_pattern_count = total_visible_suffix_count_with_profile(
            queue.pieces(),
            target_len,
            boundary_report.candidates(),
            bag_profile,
        )
        .ok_or(ObservedQueueExpansionError::NoPatterns)?;
        let mut raw_patterns = Vec::new();
        let mut truncated = false;

        for candidate in boundary_report.candidates().iter().copied() {
            let mut prefix = queue.pieces().to_vec();
            enumerate_visible_suffixes_with_profile(
                &mut prefix,
                target_len,
                candidate,
                bag_profile,
                max_patterns,
                &mut raw_patterns,
                &mut truncated,
            );
            if truncated {
                break;
            }
        }

        let probabilities = materialized_probabilities(raw_patterns.len(), total_pattern_count)
            .map_err(ObservedQueueExpansionError::Probability)?;
        let patterns = raw_patterns
            .into_iter()
            .enumerate()
            .zip(probabilities)
            .map(|((index, (boundary, queue_pattern)), probability)| {
                ObservedQueuePattern::new(
                    index,
                    boundary,
                    queue_pattern,
                    BagProbability::new(probability),
                )
            })
            .collect::<Vec<_>>();
        let pattern_set = ObservedQueuePatternSet::new(
            patterns,
            total_pattern_count,
            bag_profile.pattern_universe_hint(target_len),
        )
        .map_err(ObservedQueueExpansionError::PatternSet)?;

        Ok(ObservedQueueExpansion {
            boundary_report,
            pattern_set,
            truncated,
        })
    }
}
mod expansion {
    use clearra_core_domain::probability::probability_value::ProbabilityValue;
    use clearra_coverage::pattern::{
        pattern_bitset::PatternBitSet, weighted_pattern_set::WeightedPatternSet,
    };

    use crate::{
        bag::{
            bag_boundary::BagBoundaryReport,
            bag_profile::{BagProfile, PatternUniverseHint},
        },
        queue::observed_queue::ObservedQueue,
    };

    use super::{
        expander::expand_observed_queue, ObservedQueueExpansionError, ObservedQueuePattern,
        ObservedQueuePatternSet, ObservedQueueProbabilityContract,
    };

    #[derive(Clone, Debug, PartialEq)]
    pub struct ObservedQueueExpansion {
        pub(super) boundary_report: BagBoundaryReport,
        pub(super) pattern_set: ObservedQueuePatternSet,
        pub(super) truncated: bool,
    }

    impl ObservedQueueExpansion {
        pub fn expand(
            queue: &ObservedQueue,
            minimum_len: usize,
            max_patterns: usize,
        ) -> Result<Self, ObservedQueueExpansionError> {
            Self::expand_with_bag_profile(
                queue,
                minimum_len,
                max_patterns,
                &BagProfile::standard_7(),
            )
        }
    }
    impl ObservedQueueExpansion {
        pub fn expand_with_bag_profile(
            queue: &ObservedQueue,
            minimum_len: usize,
            max_patterns: usize,
            bag_profile: &BagProfile,
        ) -> Result<Self, ObservedQueueExpansionError> {
            expand_observed_queue(queue, minimum_len, max_patterns, bag_profile)
        }
    }
    impl ObservedQueueExpansion {
        pub fn boundary_report(&self) -> &BagBoundaryReport {
            &self.boundary_report
        }
    }
    impl ObservedQueueExpansion {
        pub fn pattern_set(&self) -> &ObservedQueuePatternSet {
            &self.pattern_set
        }
    }
    impl ObservedQueueExpansion {
        pub fn patterns(&self) -> &[ObservedQueuePattern] {
            self.pattern_set.patterns()
        }
    }
    impl ObservedQueueExpansion {
        pub fn pattern_count(&self) -> usize {
            self.pattern_set.pattern_count()
        }
    }
    impl ObservedQueueExpansion {
        pub fn total_pattern_count(&self) -> u128 {
            self.pattern_set.total_pattern_count()
        }
    }
    impl ObservedQueueExpansion {
        pub fn covered_patterns(&self) -> &PatternBitSet {
            self.pattern_set.covered_patterns()
        }
    }
    impl ObservedQueueExpansion {
        pub fn weights(&self) -> &WeightedPatternSet {
            self.pattern_set.weights()
        }
    }
    impl ObservedQueueExpansion {
        pub fn materialized_probability_mass(&self) -> ProbabilityValue {
            self.pattern_set.materialized_probability_mass()
        }
    }
    impl ObservedQueueExpansion {
        pub fn pattern_universe_hint(&self) -> PatternUniverseHint {
            self.pattern_set.pattern_universe_hint()
        }
    }
    impl ObservedQueueExpansion {
        pub fn is_truncated(&self) -> bool {
            self.truncated
        }
    }
    impl ObservedQueueExpansion {
        pub fn probability_complete(&self) -> bool {
            !self.truncated
        }
    }
    impl ObservedQueueExpansion {
        pub fn probability_contract(&self) -> ObservedQueueProbabilityContract {
            ObservedQueueProbabilityContract::from_expansion(self)
        }
    }
}
mod expansion_error {
    use clearra_core_domain::probability::probability_value::ProbabilityValueError;

    use super::ObservedQueuePatternSetError;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum ObservedQueueExpansionError {
        ZeroPatternLimit,
        IncompatibleBoundary,
        NoPatterns,
        Probability(ProbabilityValueError),
        PatternSet(ObservedQueuePatternSetError),
    }
}
mod pattern {
    use crate::{
        bag::{bag_boundary::BagBoundaryCandidate, bag_probability::BagProbability},
        queue::queue_pattern::QueuePattern,
    };

    #[derive(Clone, Debug, PartialEq)]
    pub struct ObservedQueuePattern {
        pattern_index: usize,
        boundary_candidate: BagBoundaryCandidate,
        queue_pattern: QueuePattern,
        probability: BagProbability,
    }

    impl ObservedQueuePattern {
        pub fn new(
            pattern_index: usize,
            boundary_candidate: BagBoundaryCandidate,
            queue_pattern: QueuePattern,
            probability: BagProbability,
        ) -> Self {
            Self {
                pattern_index,
                boundary_candidate,
                queue_pattern,
                probability,
            }
        }
    }
    impl ObservedQueuePattern {
        pub fn pattern_index(&self) -> usize {
            self.pattern_index
        }
    }
    impl ObservedQueuePattern {
        pub fn boundary_candidate(&self) -> BagBoundaryCandidate {
            self.boundary_candidate
        }
    }
    impl ObservedQueuePattern {
        pub fn queue_pattern(&self) -> &QueuePattern {
            &self.queue_pattern
        }
    }
    impl ObservedQueuePattern {
        pub fn probability(&self) -> BagProbability {
            self.probability
        }
    }
}
mod pattern_set {
    use clearra_core_domain::probability::probability_value::ProbabilityValue;
    use clearra_coverage::pattern::{
        pattern_bitset::PatternBitSet, pattern_id::PatternId,
        weighted_pattern_set::WeightedPatternSet,
    };

    use crate::bag::bag_profile::PatternUniverseHint;

    use super::{ObservedQueuePattern, ObservedQueuePatternSetError};

    #[derive(Clone, Debug, PartialEq)]
    pub struct ObservedQueuePatternSet {
        patterns: Vec<ObservedQueuePattern>,
        covered_patterns: PatternBitSet,
        weights: WeightedPatternSet,
        total_pattern_count: u128,
        pattern_universe_hint: PatternUniverseHint,
    }

    impl ObservedQueuePatternSet {
        pub fn new(
            patterns: Vec<ObservedQueuePattern>,
            total_pattern_count: u128,
            pattern_universe_hint: PatternUniverseHint,
        ) -> Result<Self, ObservedQueuePatternSetError> {
            if patterns.is_empty() {
                return Err(ObservedQueuePatternSetError::EmptyPatternSet);
            }
            if total_pattern_count < patterns.len() as u128 {
                return Err(
                    ObservedQueuePatternSetError::MaterializedPatternsExceedTotal {
                        materialized: patterns.len(),
                        total: total_pattern_count,
                    },
                );
            }

            let mut covered_patterns = PatternBitSet::new(patterns.len());
            let mut weights = Vec::with_capacity(patterns.len());
            for (expected_index, pattern) in patterns.iter().enumerate() {
                if pattern.pattern_index() != expected_index {
                    return Err(ObservedQueuePatternSetError::NonContiguousPatternIndex {
                        expected: expected_index,
                        actual: pattern.pattern_index(),
                    });
                }
                covered_patterns
                    .insert(PatternId::new(pattern.pattern_index()))
                    .map_err(ObservedQueuePatternSetError::PatternBitSet)?;
                weights.push(pattern.probability().value());
            }
            let weights =
                WeightedPatternSet::new(weights).map_err(ObservedQueuePatternSetError::Weights)?;
            Ok(Self {
                patterns,
                covered_patterns,
                weights,
                total_pattern_count,
                pattern_universe_hint,
            })
        }
    }
    impl ObservedQueuePatternSet {
        pub fn patterns(&self) -> &[ObservedQueuePattern] {
            &self.patterns
        }
    }
    impl ObservedQueuePatternSet {
        pub fn pattern_count(&self) -> usize {
            self.patterns.len()
        }
    }
    impl ObservedQueuePatternSet {
        pub fn total_pattern_count(&self) -> u128 {
            self.total_pattern_count
        }
    }
    impl ObservedQueuePatternSet {
        pub fn covered_patterns(&self) -> &PatternBitSet {
            &self.covered_patterns
        }
    }
    impl ObservedQueuePatternSet {
        pub fn weights(&self) -> &WeightedPatternSet {
            &self.weights
        }
    }
    impl ObservedQueuePatternSet {
        pub fn materialized_probability_mass(&self) -> ProbabilityValue {
            self.weights.total_weight()
        }
    }
    impl ObservedQueuePatternSet {
        pub fn pattern_universe_hint(&self) -> PatternUniverseHint {
            self.pattern_universe_hint
        }
    }
}
mod pattern_set_error {
    use clearra_coverage::pattern::{
        pattern_bitset::PatternBitSetError, weighted_pattern_set::WeightedPatternSetError,
    };

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum ObservedQueuePatternSetError {
        EmptyPatternSet,
        MaterializedPatternsExceedTotal { materialized: usize, total: u128 },
        NonContiguousPatternIndex { expected: usize, actual: usize },
        PatternBitSet(PatternBitSetError),
        Weights(WeightedPatternSetError),
    }
}
mod probability_contract {
    use clearra_core_domain::probability::probability_value::ProbabilityValue;

    use super::ObservedQueueExpansion;

    #[derive(Clone, Debug, PartialEq)]
    pub struct ObservedQueueProbabilityContract {
        probability_complete: bool,
        materialized_probability_mass: ProbabilityValue,
        renormalized: bool,
        truncation_reason: Option<&'static str>,
    }

    impl ObservedQueueProbabilityContract {
        pub fn from_expansion(expansion: &ObservedQueueExpansion) -> Self {
            Self {
                probability_complete: expansion.probability_complete(),
                materialized_probability_mass: expansion.materialized_probability_mass(),
                renormalized: false,
                truncation_reason: expansion
                    .is_truncated()
                    .then_some("observed_queue_pattern_limit"),
            }
        }
    }
    impl ObservedQueueProbabilityContract {
        pub fn probability_complete(&self) -> bool {
            self.probability_complete
        }
    }
    impl ObservedQueueProbabilityContract {
        pub fn materialized_probability_mass(&self) -> ProbabilityValue {
            self.materialized_probability_mass
        }
    }
    impl ObservedQueueProbabilityContract {
        pub fn renormalized(&self) -> bool {
            self.renormalized
        }
    }
    impl ObservedQueueProbabilityContract {
        pub fn truncation_reason(&self) -> Option<&'static str> {
            self.truncation_reason
        }
    }
}

pub use expansion::ObservedQueueExpansion;
pub use expansion_error::ObservedQueueExpansionError;
pub use pattern::ObservedQueuePattern;
pub use pattern_set::ObservedQueuePatternSet;
pub use pattern_set_error::ObservedQueuePatternSetError;
pub use probability_contract::ObservedQueueProbabilityContract;

#[cfg(test)]
use crate::{
    bag::bag_profile::{BagProfile, PatternUniverseHint},
    queue::observed_queue::ObservedQueue,
};

#[cfg(test)]
#[path = "observed_queue_expansion_tests.rs"]
mod tests;
