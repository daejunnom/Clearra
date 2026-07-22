#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct ProbabilityValue(f64);

impl ProbabilityValue {
    pub const ZERO: Self = Self(0.0);
    pub const ONE: Self = Self(1.0);
}
impl ProbabilityValue {
    pub fn new(value: f64) -> Result<Self, ProbabilityValueError> {
        if !value.is_finite() {
            return Err(ProbabilityValueError::NotFinite);
        }
        if !(0.0..=1.0).contains(&value) {
            return Err(ProbabilityValueError::OutOfRange);
        }
        Ok(if value == 0.0 {
            Self::ZERO
        } else {
            Self(value)
        })
    }
}
impl ProbabilityValue {
    pub fn get(self) -> f64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProbabilityValueError {
    NotFinite,
    OutOfRange,
}

#[cfg(test)]
#[path = "probability_value_tests.rs"]
mod tests;
