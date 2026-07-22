use clearra_core_domain::probability::probability_value::ProbabilityValue;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BagProbability {
    value: ProbabilityValue,
}

impl BagProbability {
    pub const CERTAIN: Self = Self {
        value: ProbabilityValue::ONE,
    };

    pub const IMPOSSIBLE: Self = Self {
        value: ProbabilityValue::ZERO,
    };
}
impl BagProbability {
    pub fn new(value: ProbabilityValue) -> Self {
        Self { value }
    }
}
impl BagProbability {
    pub fn uniform(candidate_count: usize) -> Option<Self> {
        if candidate_count == 0 {
            return None;
        }
        ProbabilityValue::new(1.0 / candidate_count as f64)
            .ok()
            .map(Self::new)
    }
}
impl BagProbability {
    pub fn value(self) -> ProbabilityValue {
        self.value
    }
}
