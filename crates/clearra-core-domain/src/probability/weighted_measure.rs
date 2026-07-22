use super::probability_value::ProbabilityValue;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WeightedMeasure {
    value: ProbabilityValue,
    weight: ProbabilityValue,
}

impl WeightedMeasure {
    pub fn new(value: ProbabilityValue, weight: ProbabilityValue) -> Self {
        Self { value, weight }
    }
}
impl WeightedMeasure {
    pub fn value(self) -> ProbabilityValue {
        self.value
    }
}
impl WeightedMeasure {
    pub fn weight(self) -> ProbabilityValue {
        self.weight
    }
}
impl WeightedMeasure {
    pub fn contribution(self) -> f64 {
        self.value.get() * self.weight.get()
    }
}
