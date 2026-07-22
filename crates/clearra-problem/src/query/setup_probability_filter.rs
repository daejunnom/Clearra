use clearra_core_domain::probability::probability_value::ProbabilityValue;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SetupProbabilityFilter {
    min_probability: Option<ProbabilityValue>,
    max_probability: Option<ProbabilityValue>,
}

impl SetupProbabilityFilter {
    pub fn new(
        min_probability: Option<ProbabilityValue>,
        max_probability: Option<ProbabilityValue>,
    ) -> Result<Self, SetupProbabilityFilterError> {
        if let (Some(min), Some(max)) = (min_probability, max_probability) {
            if min > max {
                return Err(SetupProbabilityFilterError::MinimumExceedsMaximum);
            }
        }

        Ok(Self {
            min_probability,
            max_probability,
        })
    }
}
impl SetupProbabilityFilter {
    pub fn at_least(min_probability: ProbabilityValue) -> Self {
        Self {
            min_probability: Some(min_probability),
            max_probability: None,
        }
    }
}
impl SetupProbabilityFilter {
    pub fn at_most(max_probability: ProbabilityValue) -> Self {
        Self {
            min_probability: None,
            max_probability: Some(max_probability),
        }
    }
}
impl SetupProbabilityFilter {
    pub fn min_probability(self) -> Option<ProbabilityValue> {
        self.min_probability
    }
}
impl SetupProbabilityFilter {
    pub fn max_probability(self) -> Option<ProbabilityValue> {
        self.max_probability
    }
}
impl SetupProbabilityFilter {
    pub fn accepts(self, probability: ProbabilityValue) -> bool {
        self.min_probability
            .is_none_or(|minimum| probability >= minimum)
            && self
                .max_probability
                .is_none_or(|maximum| probability <= maximum)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SetupProbabilityFilterError {
    MinimumExceedsMaximum,
}
