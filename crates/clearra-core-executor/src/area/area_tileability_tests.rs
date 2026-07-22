use super::*;

#[test]
fn standard_area4_fast_path_unchanged() {
    let rules = AreaTileabilityRules::standard_tetrominoes();

    assert_eq!(
        rules.rule_kind(),
        AreaTileabilityRuleKind::StandardTetrominoArea4FastPath
    );
    assert!(rules.can_compose_area(8));
    assert!(!rules.can_compose_area(10));
}

#[test]
fn area_multiset_feasibility_uses_piece_area_multiset() {
    let rules = AreaTileabilityRules::new([3, 4]).expect("rules");

    assert_eq!(
        rules.rule_kind(),
        AreaTileabilityRuleKind::ActivePieceAreaMultiset
    );
    assert!(rules.can_compose_area(7));
    assert!(!rules.can_compose_area(8));
}
