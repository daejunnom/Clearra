use clearra_core_domain::{
    piece::piece_kind::PieceKind, probability::probability_value::ProbabilityValue,
};

use super::*;

fn provenance() -> SupplyProvenance {
    SupplyProvenance::standard_7_bag()
}

#[test]
fn piece_source_fixed_queue_roundtrip() {
    let source = PieceSource::fixed_queue(
        FixedSequence::new(vec![PieceKind::I, PieceKind::O]),
        provenance(),
    );

    assert_eq!(source.kind, PieceSourceKind::FixedQueue);
    assert_eq!(
        source.fixed_sequence.as_ref().expect("fixed").pieces(),
        &[PieceKind::I, PieceKind::O]
    );
    assert!(source.complete());
    assert_ne!(source.id.get(), 0);
}

#[test]
fn piece_source_bag_universe_roundtrip() {
    let source = PieceSource::bag_universe(
        BagAlignedPattern::new(vec![PieceKind::I, PieceKind::O, PieceKind::T]),
        provenance(),
    );

    assert_eq!(source.kind, PieceSourceKind::BagUniverse);
    assert_eq!(
        source.bag_universe.as_ref().expect("bag").pattern(),
        &[PieceKind::I, PieceKind::O, PieceKind::T]
    );
}

#[test]
fn piece_source_observed_window_roundtrip() {
    let source = PieceSource::observed_window(
        ObservedQueue::new(vec![PieceKind::T, PieceKind::I]),
        provenance(),
        4,
        1,
    )
    .expect("observed universe");

    assert_eq!(source.kind(), PieceSourceKind::ObservedWindow);
    assert!(!source.complete());
    assert_eq!(
        source.truncation_reason(),
        Some(SupplyTruncationReason::ObservedWindowBudgetExceeded)
    );
}

#[test]
fn piece_source_materialized_universe_roundtrip() {
    let universe = MaterializedPatternUniverse::from_sequences(
        PatternUniverseId::new(42),
        PatternWeightModelId::new(7),
        vec![vec![PieceKind::I], vec![PieceKind::O]],
        vec![
            ProbabilityValue::new(0.5).expect("weight"),
            ProbabilityValue::new(0.5).expect("weight"),
        ],
        2,
        true,
        None,
    )
    .expect("materialized universe");
    let source = PieceSource::materialized_pattern_universe(universe, provenance());

    assert_eq!(source.kind(), PieceSourceKind::MaterializedPatternUniverse);
    assert_eq!(
        source.pattern_universe_id(),
        Some(PatternUniverseId::new(42))
    );
    assert_eq!(
        source.pattern_weight_model_id(),
        Some(PatternWeightModelId::new(7))
    );
    assert_eq!(
        source
            .materialized_universe()
            .expect("materialized")
            .pattern_count(),
        2
    );
}

#[test]
fn piece_source_is_immutable_shared_source() {
    let source = PieceSource::fixed_queue(
        FixedSequence::new(vec![PieceKind::I, PieceKind::O]),
        provenance(),
    );

    assert!(source.piece_source_is_immutable_shared_source());
    assert_eq!(source.kind(), PieceSourceKind::FixedQueue);
    assert_eq!(
        source.fixed_sequence().expect("fixed").pieces(),
        &[PieceKind::I, PieceKind::O]
    );
}
