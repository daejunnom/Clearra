use std::sync::Arc;

use clearra_core_domain::probability::probability_value::{
    ProbabilityValue, ProbabilityValueError,
};

use super::{pattern_bitset::PatternBitSet, pattern_id::PatternId};

#[derive(Clone, Debug, PartialEq)]
pub struct WeightedPatternSet {
    storage: WeightStorage,
}

#[derive(Clone, Debug)]
enum WeightStorage {
    Explicit(Arc<[ProbabilityValue]>),
    Uniform {
        weight: ProbabilityValue,
        count: usize,
    },
    UniformWithTerminalRemainder {
        weight: ProbabilityValue,
        terminal_weight: ProbabilityValue,
        count: usize,
    },
}

impl PartialEq for WeightStorage {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Explicit(left), Self::Explicit(right)) => left == right,
            (
                Self::Uniform {
                    weight: left_weight,
                    count: left_count,
                },
                Self::Uniform {
                    weight: right_weight,
                    count: right_count,
                },
            ) => left_weight == right_weight && left_count == right_count,
            (
                Self::UniformWithTerminalRemainder {
                    weight: left_weight,
                    terminal_weight: left_terminal_weight,
                    count: left_count,
                },
                Self::UniformWithTerminalRemainder {
                    weight: right_weight,
                    terminal_weight: right_terminal_weight,
                    count: right_count,
                },
            ) => {
                left_weight == right_weight
                    && left_terminal_weight == right_terminal_weight
                    && left_count == right_count
            }
            _ => false,
        }
    }
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

    /// Stores a complete uniform distribution without allocating one weight per
    /// pattern. The supplied weight must be bitwise-identical to `1.0 / count`;
    /// the terminal weight is precomputed in O(1) as
    /// `1.0 - weight * (count - 1)` and shared with the eager representation.
    pub fn uniform_with_terminal_remainder(
        pattern_count: usize,
        weight: ProbabilityValue,
    ) -> Result<Self, WeightedPatternSetError> {
        if pattern_count == 0 {
            return Err(WeightedPatternSetError::EmptyPatternUniverse);
        }
        let canonical_weight = ProbabilityValue::new(1.0 / pattern_count as f64)
            .map_err(WeightedPatternSetError::WeightOutOfRange)?;
        if weight.get().to_bits() != canonical_weight.get().to_bits() {
            return Err(WeightedPatternSetError::NonCanonicalUniformWeight);
        }
        let terminal_weight = canonical_terminal_remainder(canonical_weight, pattern_count)
            .map_err(WeightedPatternSetError::WeightOutOfRange)?;
        Ok(Self {
            storage: WeightStorage::UniformWithTerminalRemainder {
                weight: canonical_weight,
                terminal_weight,
                count: pattern_count,
            },
        })
    }
}
impl WeightedPatternSet {
    pub fn len(&self) -> usize {
        match &self.storage {
            WeightStorage::Explicit(weights) => weights.len(),
            WeightStorage::Uniform { count, .. }
            | WeightStorage::UniformWithTerminalRemainder { count, .. } => *count,
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
            WeightStorage::UniformWithTerminalRemainder {
                weight,
                terminal_weight,
                count,
            } => {
                if pattern.index() >= *count {
                    return None;
                }
                if pattern.index() + 1 == *count {
                    return Some(*terminal_weight);
                }
                Some(*weight)
            }
        }
    }
}
impl WeightedPatternSet {
    pub fn total_weight(&self) -> ProbabilityValue {
        let total = match &self.storage {
            WeightStorage::Explicit(weights) => weights.iter().map(|weight| weight.get()).sum(),
            WeightStorage::Uniform { weight, count } => weight.get() * *count as f64,
            WeightStorage::UniformWithTerminalRemainder { .. } => 1.0,
        };
        ProbabilityValue::new(normalize_total(total, self.len()))
            .expect("constructor keeps total weight <= 1")
    }

    /// Heap payload retained by this canonical weight representation.
    ///
    /// Cloning explicit weights shares the same `Arc<[ProbabilityValue]>`, while
    /// both uniform representations are inline and retain no heap allocation.
    pub fn checked_storage_retained_bytes(&self) -> Option<u128> {
        match &self.storage {
            WeightStorage::Explicit(weights) => (weights.len() as u128)
                .checked_mul(core::mem::size_of::<ProbabilityValue>() as u128),
            WeightStorage::Uniform { .. } | WeightStorage::UniformWithTerminalRemainder { .. } => {
                Some(0)
            }
        }
    }

    pub fn covered_weight(&self, coverage: &PatternBitSet) -> Option<ProbabilityValue> {
        if coverage.pattern_count() != self.len() {
            return None;
        }
        if coverage.count_ones() as usize == self.len() {
            return Some(self.total_weight());
        }
        let total = match &self.storage {
            WeightStorage::Uniform { weight, .. } => {
                weight.get() * f64::from(coverage.count_ones())
            }
            WeightStorage::UniformWithTerminalRemainder {
                weight,
                terminal_weight,
                count,
            } => {
                let covered_count = coverage.count_ones() as usize;
                let includes_terminal = coverage.contains(PatternId::new(*count - 1));
                if includes_terminal {
                    weight.get() * covered_count.saturating_sub(1) as f64 + terminal_weight.get()
                } else {
                    weight.get() * covered_count as f64
                }
            }
            WeightStorage::Explicit(weights) => coverage
                .covered_patterns_before(weights.len())
                .map(|pattern| weights[pattern.index()].get())
                .sum(),
        };
        ProbabilityValue::new(total.min(1.0)).ok()
    }
}

fn canonical_terminal_remainder(
    weight: ProbabilityValue,
    count: usize,
) -> Result<ProbabilityValue, ProbabilityValueError> {
    ProbabilityValue::new(1.0 - weight.get() * count.saturating_sub(1) as f64)
}

fn normalize_total(total: f64, count: usize) -> f64 {
    let summation_tolerance = f64::EPSILON * count.max(1) as f64 * 2.0;
    if (total - 1.0).abs() <= summation_tolerance {
        1.0
    } else {
        total.min(1.0)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WeightedPatternSetError {
    EmptyPatternUniverse,
    WeightOutOfRange(ProbabilityValueError),
    NonCanonicalUniformWeight,
    TotalWeightExceedsOne,
}

#[cfg(test)]
#[path = "weighted_pattern_set_tests.rs"]
mod tests;
