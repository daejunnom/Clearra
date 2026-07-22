use super::*;
use clearra_core_domain::{
    piece::piece_kind::PieceKind, probability::probability_value::ProbabilityValue,
};
use clearra_coverage::universe::{
    pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId,
};
use clearra_supply::{
    mixed::supply_provenance::SupplyProvenance,
    piece_source::MaterializedPatternUniverse,
    queue::{
        bag_aligned_pattern::BagAlignedPattern, fixed_sequence::FixedSequence,
        observed_queue::ObservedQueue,
    },
};

fn provenance() -> SupplyProvenance {
    SupplyProvenance::standard_7_bag()
}

#[test]
fn ffi_piece_source_fixed_queue_roundtrip() {
    let source = PieceSource::fixed_queue(
        FixedSequence::new(vec![PieceKind::I, PieceKind::O]),
        provenance(),
    );

    let descriptor = PieceSourceDescriptorCompiler::compile(&source).expect("descriptor");

    assert_eq!(descriptor.piece_source_id, source.id().get());
    assert_eq!(descriptor.source_kind, C_PIECE_SOURCE_FIXED_QUEUE);
    assert_eq!(descriptor.fixed_sequence_len, 2);
    assert_eq!(descriptor.complete, 1);
}

#[test]
fn ffi_piece_source_bag_universe_roundtrip() {
    let source = PieceSource::bag_universe(
        BagAlignedPattern::new(vec![PieceKind::I, PieceKind::O, PieceKind::T]),
        provenance(),
    );

    let descriptor = PieceSourceDescriptorCompiler::compile(&source).expect("descriptor");

    assert_eq!(descriptor.source_kind, C_PIECE_SOURCE_BAG_UNIVERSE);
    assert_eq!(descriptor.fixed_sequence_len, 0);
}

#[test]
fn ffi_piece_source_observed_window_roundtrip() {
    let source = PieceSource::observed_window(
        ObservedQueue::new(vec![PieceKind::T, PieceKind::I]),
        provenance(),
        4,
        1,
    )
    .expect("observed source");

    let descriptor = PieceSourceDescriptorCompiler::compile(&source).expect("descriptor");

    assert_eq!(descriptor.source_kind, C_PIECE_SOURCE_OBSERVED_WINDOW);
    assert_eq!(descriptor.complete, 0);
    assert_eq!(
        descriptor.truncation_reason,
        C_SUPPLY_TRUNCATION_OBSERVED_WINDOW_BUDGET_EXCEEDED
    );
}

#[test]
fn ffi_piece_source_materialized_universe_roundtrip() {
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

    let descriptor = PieceSourceDescriptorCompiler::compile(&source).expect("descriptor");

    assert_eq!(
        descriptor.source_kind,
        C_PIECE_SOURCE_MATERIALIZED_PATTERN_UNIVERSE
    );
    assert_eq!(descriptor.pattern_universe_id, 42);
    assert_eq!(descriptor.pattern_weight_model_id, 7);
    assert_eq!(descriptor.materialized_pattern_count, 2);
}
