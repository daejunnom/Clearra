use std::sync::Arc;

use clearra_core_domain::probability::probability_value::{
    ProbabilityValue, ProbabilityValueError,
};

use super::{pattern_bitset::PatternBitSet, pattern_id::PatternId};

#[derive(Clone, Debug, PartialEq)]
pub struct WeightedPatternSet {
    storage: WeightStorage,
}

#[derive(Clone, Debug, PartialEq)]
enum WeightStorage {
    Explicit(Arc<[ProbabilityValue]>),
    Uniform {
        weight: ProbabilityValue,
        count: usize,
    },
}

impl WeightedPatternSet {
    pub fn new(weights: Vec<ProbabilityValue>) -> Result<Self, WeightedPatternSetError> {
        let total: f64 = weights.iter().map(|weight| weight.get()).sum();
        let summation_tolerance = f64::EPSILON * weights.len().max(1) as f64 * 2.0;
        if total > 1.0 + summation_tolerance {
            return Err(WeightedPatternSetError::TotalWeightExceedsOne);
        }
        Ok(Self {
            storage: WeightStorage::Explicit(weights.into()),
        })
    }
}
impl WeightedPatternSet {
    pub fn uniform(pattern_count: usize) -> Result<Self, WeightedPatternSetError> {
        if pattern_count == 0 {
            return Err(WeightedPatternSetError::EmptyPatternUniverse);
        }

        let weight = ProbabilityValue::new(1.0 / pattern_count as f64)
            .map_err(WeightedPatternSetError::WeightOutOfRange)?;
        Self::uniform_with_weight(pattern_count, weight)
    }

    pub fn uniform_with_weight(
        pattern_count: usize,
        weight: ProbabilityValue,
    ) -> Result<Self, WeightedPatternSetError> {
        if pattern_count == 0 {
            return Err(WeightedPatternSetError::EmptyPatternUniverse);
        }
        if weight.get() * pattern_count as f64 > 1.0 + f64::EPSILON {
            return Err(WeightedPatternSetError::TotalWeightExceedsOne);
        }
        Ok(Self {
            storage: WeightStorage::Uniform {
                weight,
                count: pattern_count,
            },
        })
    }
}
impl WeightedPatternSet {
    pub fn len(&self) -> usize {
        match &self.storage {
            WeightStorage::Explicit(weights) => weights.len(),
            WeightStorage::Uniform { count, .. } => *count,
        }
    }
}
impl WeightedPatternSet {
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
impl WeightedPatternSet {
    pub fn weight(&self, pattern: PatternId) -> Option<ProbabilityValue> {
        match &self.storage {
            WeightStorage::Explicit(weights) => weights.get(pattern.index()).copied(),
            WeightStorage::Uniform { weight, count } => {
                (pattern.index() < *count).then_some(*weight)
            }
        }
    }
}
impl WeightedPatternSet {
    pub fn total_weight(&self) -> ProbabilityValue {
        let total = match &self.storage {
            WeightStorage::Explicit(weights) => weights.iter().map(|weight| weight.get()).sum(),
            WeightStorage::Uniform { weight, count } => weight.get() * *count as f64,
        };
        ProbabilityValue::new(total.min(1.0)).expect("constructor keeps total weight <= 1")
    }

    pub fn covered_weight(&self, coverage: &PatternBitSet) -> Option<ProbabilityValue> {
        if coverage.pattern_count() != self.len() {
            return None;
        }
        let total = match &self.storage {
            WeightStorage::Uniform { weight, .. } => {
                weight.get() * f64::from(coverage.count_ones())
            }
            WeightStorage::Explicit(weights) => coverage
                .covered_patterns_before(weights.len())
                .map(|pattern| weights[pattern.index()].get())
                .sum(),
        };
        ProbabilityValue::new(total.min(1.0)).ok()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WeightedPatternSetError {
    EmptyPatternUniverse,
    WeightOutOfRange(ProbabilityValueError),
    TotalWeightExceedsOne,
}

#[cfg(test)]
#[path = "weighted_pattern_set_tests.rs"]
mod tests;
