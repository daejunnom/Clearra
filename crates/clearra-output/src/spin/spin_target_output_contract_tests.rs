use super::*;

#[test]
fn missing_kick_evidence_is_incomplete_not_exact_spin() {
    let contract = SpinTargetOutputContract::missing_kick_evidence("tsd", "kick-sensitive-special");

    assert_eq!(contract.spin_target_id(), "tsd");
    assert_eq!(contract.classifier_id(), "kick-sensitive-special");
    assert_eq!(contract.trace_requirement(), "kick-evidence-required");
    assert_eq!(contract.trace_completeness(), "missing-kick-evidence");
    assert!(!contract.exact());
    assert!(!contract.probability_complete());
    assert_eq!(
        contract.diagnostic_code(),
        Some("W_SPIN_TARGET_PROBABILITY_INCOMPLETE")
    );
}

#[test]
fn spin_probability_uses_pattern_bitset_union() {
    let contract = SpinTargetOutputContract::new(
        "tsd",
        Some("tetrio".to_owned()),
        Some(0.5),
        "build-variant-replay-evidence",
        "t-spins",
        false,
        "full",
        true,
        None,
    );

    assert_eq!(contract.score_profile_id(), Some("tetrio"));
    assert_eq!(contract.target_probability_threshold(), Some(0.5));
    assert_eq!(contract.coverage_reducer(), "PatternBitSet OR union");
}
