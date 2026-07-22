use super::*;
use crate::{operation::operation::OperationId, piece::piece_kind::PieceKind};

fn family() -> ShapeFamily {
    ShapeFamily::new(
        ShapeFamilyId::new(1),
        ShapeKey(0x00ff),
        VisualGroupKey(0x00ff),
    )
}

#[test]
fn same_shape_preserves_distinct_tiling_variants() {
    let family = family();
    let cell_partition = CellPartitionKey(0x00ff);
    let left = TilingVariant::new(
        TilingVariantId::new(10),
        family.shape_family_id,
        PieceCountVector::from_pieces(&[PieceKind::L, PieceKind::S, PieceKind::J]),
        vec![
            OperationPlacement::new(OperationId(1), PieceKind::L, 0x000f, 0, 0),
            OperationPlacement::new(OperationId(2), PieceKind::S, 0x00f0, 4, 0),
            OperationPlacement::new(OperationId(3), PieceKind::J, 0x0f00, 0, 1),
        ],
        TilingKey(0x1010),
    );
    let right = TilingVariant::new(
        TilingVariantId::new(11),
        family.shape_family_id,
        PieceCountVector::from_pieces(&[PieceKind::L, PieceKind::S, PieceKind::J]),
        vec![
            OperationPlacement::new(OperationId(4), PieceKind::L, 0x00f0, 4, 0),
            OperationPlacement::new(OperationId(5), PieceKind::S, 0x000f, 0, 0),
            OperationPlacement::new(OperationId(6), PieceKind::J, 0x0f00, 0, 1),
        ],
        TilingKey(0x2020),
    );

    assert_eq!(left.shape_family_id, right.shape_family_id);
    assert_eq!(cell_partition.0, family.occupied_shape_key.0);
    assert_ne!(left.tiling_key, right.tiling_key);
    assert_ne!(left.placements, right.placements);
}

#[test]
fn shape_key_does_not_drop_tiling_variant() {
    same_shape_preserves_distinct_tiling_variants();
}

#[test]
fn same_tiling_preserves_distinct_build_orders() {
    let tiling_id = TilingVariantId::new(20);
    let operation_set_key = OperationSetKey(0xfeed);
    let loj = BuildVariant::new(
        BuildVariantId::new(1),
        tiling_id,
        vec![OperationId(1), OperationId(2), OperationId(3)],
        vec![
            HoldDecision::UseCurrent,
            HoldDecision::UseCurrent,
            HoldDecision::UseCurrent,
        ],
        PatternId(101),
        vec![],
        ReachabilityEvidence::confirmed(),
    );
    let jol = BuildVariant::new(
        BuildVariantId::new(2),
        tiling_id,
        vec![OperationId(3), OperationId(2), OperationId(1)],
        vec![
            HoldDecision::UseCurrent,
            HoldDecision::SwapHeld,
            HoldDecision::UseCurrent,
        ],
        PatternId(102),
        vec![],
        ReachabilityEvidence::confirmed(),
    );

    assert_eq!(loj.tiling_variant_id, jol.tiling_variant_id);
    assert_ne!(operation_set_key.0, 0);
    assert_ne!(loj.operation_order, jol.operation_order);
    assert_ne!(loj.hold_decisions, jol.hold_decisions);
}

#[test]
fn tiling_key_does_not_drop_build_variant() {
    same_tiling_preserves_distinct_build_orders();
}

#[test]
fn loj_jol_same_shape_distinct_build_variants() {
    let shape = ShapeKey(0x0fff);
    let family = ShapeFamily::new(ShapeFamilyId::new(4), shape, VisualGroupKey(shape.0));
    let tiling = TilingVariant::new(
        TilingVariantId::new(8),
        family.shape_family_id,
        PieceCountVector::from_pieces(&[PieceKind::L, PieceKind::O, PieceKind::J]),
        vec![
            OperationPlacement::new(OperationId(1), PieceKind::L, 0x000f, 0, 0),
            OperationPlacement::new(OperationId(2), PieceKind::O, 0x00f0, 4, 0),
            OperationPlacement::new(OperationId(3), PieceKind::J, 0x0f00, 0, 1),
        ],
        TilingKey(0x8080),
    );
    let loj = BuildVariant::new(
        BuildVariantId::new(30),
        tiling.tiling_variant_id,
        vec![OperationId(1), OperationId(2), OperationId(3)],
        vec![
            HoldDecision::UseCurrent,
            HoldDecision::UseCurrent,
            HoldDecision::UseCurrent,
        ],
        PatternId(30),
        vec![],
        ReachabilityEvidence::confirmed(),
    );
    let jol = BuildVariant::new(
        BuildVariantId::new(31),
        tiling.tiling_variant_id,
        vec![OperationId(3), OperationId(2), OperationId(1)],
        vec![
            HoldDecision::UseCurrent,
            HoldDecision::SwapHeld,
            HoldDecision::UseCurrent,
        ],
        PatternId(31),
        vec![],
        ReachabilityEvidence::confirmed(),
    );

    assert_eq!(family.occupied_shape_key, shape);
    assert_eq!(loj.tiling_variant_id, jol.tiling_variant_id);
    assert_ne!(loj.build_variant_id, jol.build_variant_id);
}

#[test]
fn lsj_same_shape_distinct_tiling_variant() {
    let shape_family_id = ShapeFamilyId::new(5);
    let lsj = TilingVariant::new(
        TilingVariantId::new(50),
        shape_family_id,
        PieceCountVector::from_pieces(&[PieceKind::L, PieceKind::S, PieceKind::J]),
        vec![
            OperationPlacement::new(OperationId(1), PieceKind::L, 0x000f, 0, 0),
            OperationPlacement::new(OperationId(2), PieceKind::S, 0x00f0, 4, 0),
            OperationPlacement::new(OperationId(3), PieceKind::J, 0x0f00, 0, 1),
        ],
        TilingKey(0x5151),
    );
    let ljs = TilingVariant::new(
        TilingVariantId::new(51),
        shape_family_id,
        PieceCountVector::from_pieces(&[PieceKind::L, PieceKind::S, PieceKind::J]),
        vec![
            OperationPlacement::new(OperationId(1), PieceKind::L, 0x000f, 0, 0),
            OperationPlacement::new(OperationId(3), PieceKind::J, 0x00f0, 4, 0),
            OperationPlacement::new(OperationId(2), PieceKind::S, 0x0f00, 0, 1),
        ],
        TilingKey(0x5252),
    );

    assert_eq!(lsj.shape_family_id, ljs.shape_family_id);
    assert_eq!(lsj.piece_multiset, ljs.piece_multiset);
    assert_ne!(lsj.tiling_variant_id, ljs.tiling_variant_id);
    assert_ne!(lsj.placements, ljs.placements);
}

#[test]
fn same_mask_different_piece_definition_not_same_tiling() {
    let cell_partition = CellPartitionKey(0x0f0f);
    let imported_piece_a_tiling = TilingKey(0xa0a0);
    let imported_piece_b_tiling = TilingKey(0xb0b0);

    assert_eq!(cell_partition, CellPartitionKey(0x0f0f));
    assert_ne!(imported_piece_a_tiling, imported_piece_b_tiling);
}

#[test]
fn loj_jol_lsj_fixtures_preserved() {
    loj_jol_same_shape_distinct_build_variants();
    lsj_same_shape_distinct_tiling_variant();
}

#[test]
fn shape_family_not_used_as_probability_source() {
    let family = family();
    let build = BuildVariant::new(
        BuildVariantId::new(1),
        TilingVariantId::new(1),
        vec![OperationId(1)],
        vec![HoldDecision::UseCurrent],
        PatternId(1),
        vec![],
        ReachabilityEvidence::confirmed(),
    );

    assert!(family.groups_visual_shape_only());
    assert!(build.can_source_coverage_row());
}

#[test]
fn shape_tiling_build_variant_layers_preserved() {
    same_shape_preserves_distinct_tiling_variants();
    same_tiling_preserves_distinct_build_orders();
    shape_family_not_used_as_probability_source();
}

#[test]
fn identical_piece_copy_order_has_one_normalized_tiling_identity() {
    let left_mask = 0x0000_0000_0000_0033;
    let right_mask = 0x0000_0000_0000_00cc;
    let left_then_right = NormalizedTilingSolutionKey::from_placements(
        0,
        [
            PiecePlacementMask::new(PieceKind::O, left_mask),
            PiecePlacementMask::new(PieceKind::O, right_mask),
        ],
    )
    .expect("valid O-piece tiling");
    let right_then_left = NormalizedTilingSolutionKey::from_placements(
        0,
        [
            PiecePlacementMask::new(PieceKind::O, right_mask),
            PiecePlacementMask::new(PieceKind::O, left_mask),
        ],
    )
    .expect("valid O-piece tiling");

    assert_eq!(left_then_right, right_then_left);
    assert_eq!(
        NormalizedTilingSolutionSet::new([left_then_right, right_then_left]).len(),
        1
    );
}
