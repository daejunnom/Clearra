use super::*;

#[test]
fn union_uses_or_semantics() {
    let left = PatternBitSet::from_patterns(4, [PatternId::new(0), PatternId::new(2)])
        .expect("valid left");
    let right = PatternBitSet::from_patterns(4, [PatternId::new(2), PatternId::new(3)])
        .expect("valid right");

    let union = left.union(&right).expect("matching pattern universe");

    assert_eq!(
        union.covered_patterns(),
        vec![PatternId::new(0), PatternId::new(2), PatternId::new(3)]
    );
}

#[test]
fn rejects_out_of_range_patterns() {
    let mut bitset = PatternBitSet::new(2);

    assert_eq!(
        bitset.insert(PatternId::new(2)),
        Err(PatternBitSetError::PatternOutOfRange {
            index: 2,
            pattern_count: 2
        })
    );
}

#[test]
fn union_rejects_different_pattern_universes() {
    let left = PatternBitSet::from_patterns(2, [PatternId::new(0)]).expect("left");
    let right = PatternBitSet::from_patterns(3, [PatternId::new(2)]).expect("right");

    assert_eq!(
        left.union(&right),
        Err(PatternBitSetError::PatternUniverseMismatch { left: 2, right: 3 })
    );
}

#[test]
fn union_with_rejects_different_pattern_universes_without_mutating() {
    let mut left = PatternBitSet::from_patterns(2, [PatternId::new(0)]).expect("left");
    let before = left.clone();
    let right = PatternBitSet::from_patterns(3, [PatternId::new(2)]).expect("right");

    assert_eq!(
        left.union_with(&right),
        Err(PatternBitSetError::PatternUniverseMismatch { left: 2, right: 3 })
    );
    assert_eq!(left, before);
}

#[test]
fn pattern_bitset_dynamic_word_allocation_budget_is_checked() {
    assert_eq!(
        PatternBitSet::new_with_word_budget(129, 2),
        Err(PatternBitSetError::WordCapacityExceeded {
            word_count: 3,
            word_limit: 2
        })
    );
    assert_eq!(
        PatternBitSet::new_with_word_budget(128, 2)
            .expect("fits budget")
            .word_count(),
        2
    );
}

#[test]
fn pattern_bitset_dynamic_word_allocation_scope_is_enforced() {
    assert_eq!(
        PatternBitSet::new_with_word_budget(192, 2),
        Err(PatternBitSetError::WordCapacityExceeded {
            word_count: 3,
            word_limit: 2
        })
    );
    assert_eq!(
        PatternBitSet::new_with_word_budget(192, 3)
            .expect("fits scoped dynamic allocation budget")
            .word_count(),
        3
    );
}

#[test]
fn is_superset_reports_matching_universe_result() {
    let covered =
        PatternBitSet::from_patterns(4, [PatternId::new(0), PatternId::new(2)]).expect("left");
    let required = PatternBitSet::from_patterns(4, [PatternId::new(2)]).expect("right");
    let missing = PatternBitSet::from_patterns(4, [PatternId::new(3)]).expect("missing");

    assert_eq!(covered.is_superset(&required), Ok(true));
    assert_eq!(covered.is_superset(&missing), Ok(false));
}

#[test]
fn is_superset_rejects_different_pattern_universes() {
    let covered = PatternBitSet::from_patterns(2, [PatternId::new(0)]).expect("covered");
    let required = PatternBitSet::from_patterns(3, [PatternId::new(0)]).expect("required");

    assert_eq!(
        covered.is_superset(&required),
        Err(PatternBitSetError::PatternUniverseMismatch { left: 2, right: 3 })
    );
}
