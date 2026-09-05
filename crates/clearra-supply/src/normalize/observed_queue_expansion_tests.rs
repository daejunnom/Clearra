use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_coverage::pattern::pattern_id::PatternId;

use super::*;

#[test]
fn observed_queue_expands_multiple_suffix_patterns_instead_of_deterministic_completion() {
    let queue = ObservedQueue::new(vec![PieceKind::I, PieceKind::O]);

    let expansion = ObservedQueueExpansion::expand(&queue, 4, 64).expect("expansion");

    assert!(expansion.boundary_report().candidates().len() > 1);
    assert!(expansion.pattern_count() > 1);
    assert!(expansion.patterns().iter().any(|pattern| {
        pattern.queue_pattern().pieces() == [PieceKind::I, PieceKind::O, PieceKind::T, PieceKind::S]
    }));
    assert!(expansion.patterns().iter().any(|pattern| {
        pattern.queue_pattern().pieces() == [PieceKind::I, PieceKind::O, PieceKind::T, PieceKind::Z]
    }));
}

#[test]
fn observed_queue_expansion_assigns_uniform_probability_to_generated_patterns() {
    let queue = ObservedQueue::new(vec![PieceKind::I, PieceKind::O]);

    let expansion = ObservedQueueExpansion::expand(&queue, 3, 64).expect("expansion");
    let total_probability = expansion
        .patterns()
        .iter()
        .map(|pattern| pattern.probability().value().get())
        .sum::<f64>();

    assert!(expansion.probability_complete());
    assert!((total_probability - 1.0).abs() < 0.000_000_001);
}

#[test]
fn observed_queue_pattern_set_connects_pattern_bitset_and_weighted_pattern_set() {
    let queue = ObservedQueue::new(vec![PieceKind::I, PieceKind::O]);

    let expansion = ObservedQueueExpansion::expand(&queue, 3, 64).expect("expansion");

    assert_eq!(
        expansion.covered_patterns().pattern_count(),
        expansion.pattern_count()
    );
    assert_eq!(
        expansion.covered_patterns().count_ones(),
        expansion.pattern_count() as u32
    );
    assert_eq!(expansion.weights().len(), expansion.pattern_count());
    for pattern in expansion.patterns() {
        let pattern_id = PatternId::new(pattern.pattern_index());
        assert!(expansion.covered_patterns().contains(pattern_id));
        assert_eq!(
            expansion.weights().weight(pattern_id),
            Some(pattern.probability().value())
        );
    }
    assert_eq!(expansion.materialized_probability_mass().get(), 1.0);
    assert_eq!(
        expansion.total_pattern_count(),
        expansion.pattern_count() as u128
    );
}

#[test]
fn observed_queue_expansion_reports_truncation_when_pattern_limit_is_hit() {
    let queue = ObservedQueue::default();

    let expansion = ObservedQueueExpansion::expand(&queue, 6, 4).expect("expansion");

    assert_eq!(expansion.pattern_count(), 4);
    assert!(expansion.is_truncated());
    assert!(!expansion.probability_complete());
    assert!(expansion.total_pattern_count() > expansion.pattern_count() as u128);
    assert!(expansion.materialized_probability_mass().get() < 1.0);
}

#[test]
fn observed_queue_truncation_keeps_materialized_probability_mass() {
    let queue = ObservedQueue::default();

    let expansion = ObservedQueueExpansion::expand(&queue, 6, 4).expect("expansion");
    let contract = expansion.probability_contract();

    assert!(!contract.probability_complete());
    assert_eq!(
        contract.materialized_probability_mass(),
        expansion.materialized_probability_mass()
    );
    assert!(!contract.renormalized());
    assert_eq!(
        contract.truncation_reason(),
        Some("observed_queue_pattern_limit")
    );
}

#[test]
fn observed_queue_expansion_rejects_impossible_window() {
    let queue = ObservedQueue::new(vec![PieceKind::I, PieceKind::I, PieceKind::I]);

    let result = ObservedQueueExpansion::expand(&queue, 5, 64);

    assert_eq!(
        result,
        Err(ObservedQueueExpansionError::IncompatibleBoundary)
    );
}

#[test]
fn observed_queue_expansion_uses_custom_multiset_bag_profile() {
    let bag_profile = BagProfile::new(
        "double-i-bag",
        vec![
            crate::bag::bag_profile::BagProfileEntry::new(PieceKind::I, 2, 1),
            crate::bag::bag_profile::BagProfileEntry::new(PieceKind::O, 1, 1),
            crate::bag::bag_profile::BagProfileEntry::new(PieceKind::T, 1, 1),
        ],
    )
    .expect("bag profile");
    let queue = ObservedQueue::new(vec![PieceKind::I, PieceKind::I]);

    let expansion = ObservedQueueExpansion::expand_with_bag_profile(&queue, 4, 64, &bag_profile)
        .expect("custom bag expansion");

    assert_eq!(expansion.boundary_report().bag_size(), 4);
    assert!(expansion.patterns().iter().any(|pattern| {
        pattern.queue_pattern().pieces() == [PieceKind::I, PieceKind::I, PieceKind::O, PieceKind::T]
    }));
    assert!(expansion.pattern_count() > 1);
}

#[test]
fn observed_queue_expansion_rejects_custom_multiset_impossible_visible_window() {
    let bag_profile = BagProfile::new(
        "double-i-bag",
        vec![
            crate::bag::bag_profile::BagProfileEntry::new(PieceKind::I, 2, 1),
            crate::bag::bag_profile::BagProfileEntry::new(PieceKind::O, 1, 1),
        ],
    )
    .expect("bag profile");
    let queue = ObservedQueue::new(vec![
        PieceKind::I,
        PieceKind::I,
        PieceKind::I,
        PieceKind::I,
        PieceKind::I,
    ]);

    let result = ObservedQueueExpansion::expand_with_bag_profile(&queue, 5, 64, &bag_profile);

    assert_eq!(
        result,
        Err(ObservedQueueExpansionError::IncompatibleBoundary)
    );
}

#[test]
fn observed_queue_expansion_exposes_pattern_universe_representation_hint() {
    let queue = ObservedQueue::default();

    let expansion = ObservedQueueExpansion::expand(&queue, 8, 4).expect("expansion");

    assert_eq!(
        expansion.pattern_universe_hint(),
        PatternUniverseHint::SparseRecommended
    );
}
