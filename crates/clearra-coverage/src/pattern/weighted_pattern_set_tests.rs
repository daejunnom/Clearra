use super::*;

fn weight(value: f64) -> ProbabilityValue {
    ProbabilityValue::new(value).expect("valid probability weight")
}

#[test]
fn rejects_total_weight_above_one() {
    let result = WeightedPatternSet::new(vec![weight(0.6), weight(0.4), weight(1.0e-12)]);

    assert_eq!(result, Err(WeightedPatternSetError::TotalWeightExceedsOne));
}

#[test]
fn accepts_rounding_noise_within_summation_tolerance() {
    let weights = WeightedPatternSet::new(vec![weight(0.6), weight(0.4), weight(f64::EPSILON)])
        .expect("rounding noise stays within the summation tolerance");

    assert_eq!(weights.total_weight().get(), 1.0);
}

#[test]
fn total_weight_reports_actual_sum_without_clamping() {
    let weights = WeightedPatternSet::new(vec![weight(0.25), weight(0.5)]).expect("weights");

    assert_eq!(weights.total_weight().get(), 0.75);
}

#[test]
fn canonical_terminal_formula_reports_exact_complete_mass_for_explicit_storage() {
    let count = 35;
    let uniform = weight(1.0 / count as f64);
    let mut eager = vec![uniform; count];
    eager[count - 1] = weight(1.0 - uniform.get() * (count - 1) as f64);
    let weights = WeightedPatternSet::new(eager).expect("complete explicit weights");

    assert_eq!(weights.total_weight(), ProbabilityValue::ONE);
}

#[test]
fn corrected_uniform_matches_eager_weights_fieldwise_without_count_sized_storage() {
    let count = 35;
    let uniform = weight(1.0 / count as f64);
    let mut eager = vec![uniform; count];
    eager[count - 1] = weight(1.0 - uniform.get() * (count - 1) as f64);
    let lazy = WeightedPatternSet::uniform_with_terminal_remainder(count, uniform)
        .expect("corrected uniform weights");

    assert_eq!(lazy.len(), eager.len());
    for (index, expected) in eager.into_iter().enumerate() {
        assert_eq!(lazy.weight(PatternId::new(index)), Some(expected));
    }
    assert_eq!(lazy.total_weight(), ProbabilityValue::ONE);
}

#[test]
fn corrected_uniform_covered_weight_replaces_only_the_terminal_remainder() {
    let count = 7;
    let uniform = weight(1.0 / count as f64);
    let weights = WeightedPatternSet::uniform_with_terminal_remainder(count, uniform)
        .expect("corrected uniform weights");
    let coverage =
        PatternBitSet::from_pattern_indices(count, vec![0_u32, 3, 6]).expect("coverage indices");
    let expected = weights.weight(PatternId::new(0)).unwrap().get()
        + weights.weight(PatternId::new(3)).unwrap().get()
        + weights.weight(PatternId::new(6)).unwrap().get();

    assert_eq!(
        weights
            .covered_weight(&coverage)
            .expect("covered weight")
            .get(),
        expected
    );
    assert_eq!(weights.weight(PatternId::new(count)), None);
}

#[test]
fn corrected_uniform_rejects_arbitrary_sub_uniform_weight() {
    assert_eq!(
        WeightedPatternSet::uniform_with_terminal_remainder(5, weight(0.1)),
        Err(WeightedPatternSetError::NonCanonicalUniformWeight)
    );
}

#[test]
fn retained_storage_counts_only_the_shared_explicit_arc_payload() {
    let explicit =
        WeightedPatternSet::new(vec![weight(0.25), weight(0.75)]).expect("explicit weights");
    let explicit_clone = explicit.clone();
    let uniform = WeightedPatternSet::uniform(2).expect("uniform weights");

    assert_eq!(
        explicit.checked_storage_retained_bytes(),
        Some(2 * core::mem::size_of::<ProbabilityValue>() as u128)
    );
    assert_eq!(
        explicit_clone.checked_storage_retained_bytes(),
        explicit.checked_storage_retained_bytes()
    );
    assert_eq!(uniform.checked_storage_retained_bytes(), Some(0));
}

#[cfg(target_pointer_width = "64")]
#[test]
fn corrected_uniform_huge_count_terminal_access_is_constant_time() {
    let count = 35_384_428_800_usize;
    let uniform = weight(1.0 / count as f64);
    let started = std::time::Instant::now();
    let weights = WeightedPatternSet::uniform_with_terminal_remainder(count, uniform)
        .expect("huge corrected uniform");
    let terminal = weights
        .weight(PatternId::new(count - 1))
        .expect("terminal weight");

    assert!(terminal.get() > 0.0);
    assert_eq!(weights.total_weight(), ProbabilityValue::ONE);
    assert!(started.elapsed() < std::time::Duration::from_secs(1));
}
