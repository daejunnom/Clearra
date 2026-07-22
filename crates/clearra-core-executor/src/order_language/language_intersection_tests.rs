use clearra_core_domain::{operation::operation::OperationId, piece::piece_kind::PieceKind};
use clearra_coverage::pattern::pattern_id::PatternId;
use clearra_supply::{
    hold_automaton::{HoldAutomatonState, SupplyProvenanceId},
    piece_source::PieceSourceId,
};

use super::*;
use crate::order_language::{
    build_order_language::{BuildOrderLanguage, CandidateId, OperationSetKey},
    hold_reachable_language::HoldReachableLanguage,
};

fn hold_language(pattern_id: usize, order: Vec<OperationId>) -> HoldReachableLanguage {
    let piece_source_id = PieceSourceId::new(42);
    HoldReachableLanguage::from_orders(
        piece_source_id,
        PatternId::new(pattern_id),
        HoldAutomatonState::new(
            piece_source_id,
            0,
            Some(PieceKind::I),
            0,
            0,
            SupplyProvenanceId(1),
        ),
        vec![order],
        true,
    )
}

#[test]
fn language_intersection_empty_rejects_pattern() {
    let build_orders = BuildOrderLanguage::from_orders(
        CandidateId(1),
        OperationSetKey(10),
        vec![vec![OperationId(1), OperationId(2)]],
    );
    let hold_orders = vec![hold_language(0, vec![OperationId(2), OperationId(1)])];

    let coverage =
        LanguageIntersection::coverage_bits_for_candidate(&build_orders, &hold_orders, 4)
            .expect("coverage");

    assert!(!coverage.contains(PatternId::new(0)));
    assert!(coverage.is_empty());
}

#[test]
fn language_intersection_non_empty_sets_coverage_bit() {
    let build_orders = BuildOrderLanguage::from_orders(
        CandidateId(1),
        OperationSetKey(10),
        vec![
            vec![OperationId(1), OperationId(2)],
            vec![OperationId(2), OperationId(1)],
        ],
    );
    let hold_orders = vec![hold_language(2, vec![OperationId(2), OperationId(1)])];

    let coverage =
        LanguageIntersection::coverage_bits_for_candidate(&build_orders, &hold_orders, 4)
            .expect("coverage");

    assert!(coverage.contains(PatternId::new(2)));
    assert_eq!(coverage.count_ones(), 1);
}

#[test]
fn same_pattern_multiple_variants_counted_once() {
    let build_orders = BuildOrderLanguage::from_orders(
        CandidateId(1),
        OperationSetKey(10),
        vec![vec![OperationId(7), OperationId(8)]],
    );
    let hold_orders = vec![
        hold_language(1, vec![OperationId(7), OperationId(8)]),
        hold_language(1, vec![OperationId(7), OperationId(8)]),
    ];

    let coverage =
        LanguageIntersection::coverage_bits_for_candidate(&build_orders, &hold_orders, 4)
            .expect("coverage");

    assert_eq!(coverage.covered_patterns(), vec![PatternId::new(1)]);
    assert_eq!(coverage.count_ones(), 1);
}
