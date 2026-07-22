use super::*;

#[test]
fn clearra_spin_does_not_depend_on_clearra_scoring() {
    let manifest = include_str!("../Cargo.toml");

    assert!(!manifest.contains("clearra-scoring"));
}

#[test]
fn unknown_spin_not_false_for_pc_pruning() {
    assert_eq!(PredicateResult::Unknown.is_false_for_pc_pruning(), false);
    assert_eq!(
        UnknownSpinPolicy::PreserveUnknown.as_predicate(),
        PredicateResult::Unknown
    );
}
