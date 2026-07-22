use clearra_core_domain::probability::probability_value::{
    ProbabilityValue, ProbabilityValueError,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProbabilityGuardError {
    InvalidProbability(ProbabilityValueError),
}

pub fn guard_probability(value: f64) -> Result<ProbabilityValue, ProbabilityGuardError> {
    ProbabilityValue::new(value).map_err(ProbabilityGuardError::InvalidProbability)
}
