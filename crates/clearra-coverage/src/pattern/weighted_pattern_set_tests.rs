use super::*;

fn weight(value: f64) -> ProbabilityValue {
    ProbabilityValue::new(value).expect("valid probability weight")
}

#[test]
fn rejects_total_weight_above_one() {
    let result = WeightedPatternSet::new(vec![weight(0.6), weight(0.4), weight(f64::EPSILON)]);

    assert_eq!(result, Err(WeightedPatternSetError::TotalWeightExceedsOne));
}

#[test]
fn total_weight_reports_actual_sum_without_clamping() {
    let weights = WeightedPatternSet::new(vec![weight(0.25), weight(0.5)]).expect("weights");

    assert_eq!(weights.total_weight().get(), 0.75);
}
