use super::*;

#[test]
fn accepts_closed_unit_interval() {
    assert_eq!(ProbabilityValue::new(0.0), Ok(ProbabilityValue::ZERO));
    assert_eq!(
        ProbabilityValue::new(0.5).map(ProbabilityValue::get),
        Ok(0.5)
    );
    assert_eq!(ProbabilityValue::new(1.0), Ok(ProbabilityValue::ONE));
}

#[test]
fn rejects_values_outside_unit_interval() {
    assert_eq!(
        ProbabilityValue::new(-0.000_001),
        Err(ProbabilityValueError::OutOfRange)
    );
    assert_eq!(
        ProbabilityValue::new(1.000_001),
        Err(ProbabilityValueError::OutOfRange)
    );
}

#[test]
fn rejects_nan_and_infinity() {
    assert_eq!(
        ProbabilityValue::new(f64::NAN),
        Err(ProbabilityValueError::NotFinite)
    );
    assert_eq!(
        ProbabilityValue::new(f64::INFINITY),
        Err(ProbabilityValueError::NotFinite)
    );
    assert_eq!(
        ProbabilityValue::new(f64::NEG_INFINITY),
        Err(ProbabilityValueError::NotFinite)
    );
}
