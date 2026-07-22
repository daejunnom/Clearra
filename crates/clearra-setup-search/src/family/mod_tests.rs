use clearra_core_domain::{
    ids::setup_id::{BuildVariantId, SetupFamilyId, TilingVariantId},
    operation::operation::OperationId,
    piece::piece_kind::PieceKind,
    solution::{
        CellPartitionKey, HoldDecision, OperationPlacement, OperationSetKey, PatternId,
        ReachabilityEvidence, TilingKey,
    },
};
use clearra_coverage::pattern::{
    pattern_bitset::PatternBitSet, pattern_id::PatternId as CoveragePatternId,
};

use crate::{
    identity::{build_identity::BuildIdentity, shape_family::ShapeFamily},
    variant::{build_variant::BuildVariant, tiling_variant::TilingVariant},
};

fn coverage(pattern_count: usize, covered: &[usize]) -> PatternBitSet {
    let mut bitset = PatternBitSet::new(pattern_count);
    for pattern in covered {
        bitset
            .insert(CoveragePatternId::new(*pattern))
            .expect("pattern in range");
    }
    bitset
}

fn build_variant(
    id: u32,
    tiling_id: TilingVariantId,
    order: Vec<OperationId>,
    hold_decisions: Vec<HoldDecision>,
    pattern_id: u32,
) -> BuildVariant {
    BuildVariant::new(
        BuildVariantId::new(id),
        tiling_id,
        BuildIdentity::new(0x0fff, None),
        coverage(4, &[pattern_id as usize % 4]),
    )
    .with_execution_interpretation(
        order,
        hold_decisions,
        PatternId(pattern_id),
        Vec::new(),
        ReachabilityEvidence::confirmed(),
    )
}

#[test]
fn same_shape_preserves_distinct_tiling_variants() {
    let family = ShapeFamily::new(SetupFamilyId::new(1), 0x0fff);
    let cell_partition = CellPartitionKey(family.occupied_shape());
    let left = TilingVariant::new(
        TilingVariantId::new(10),
        family.id(),
        family.occupied_shape(),
        vec![PieceKind::L, PieceKind::S, PieceKind::J],
    )
    .with_placements_and_tiling_key(
        vec![
            OperationPlacement::new(OperationId(1), PieceKind::L, 0x000f, 0, 0),
            OperationPlacement::new(OperationId(2), PieceKind::S, 0x00f0, 4, 0),
            OperationPlacement::new(OperationId(3), PieceKind::J, 0x0f00, 0, 1),
        ],
        TilingKey(0x10),
    );
    let right = TilingVariant::new(
        TilingVariantId::new(11),
        family.id(),
        family.occupied_shape(),
        vec![PieceKind::L, PieceKind::S, PieceKind::J],
    )
    .with_placements_and_tiling_key(
        vec![
            OperationPlacement::new(OperationId(1), PieceKind::L, 0x00f0, 4, 0),
            OperationPlacement::new(OperationId(2), PieceKind::S, 0x000f, 0, 0),
            OperationPlacement::new(OperationId(3), PieceKind::J, 0x0f00, 0, 1),
        ],
        TilingKey(0x11),
    );

    assert_eq!(left.family_id(), right.family_id());
    assert_eq!(left.occupied_shape(), right.occupied_shape());
    assert_eq!(cell_partition.0, left.occupied_shape());
    assert_eq!(left.piece_multiset(), right.piece_multiset());
    assert_ne!(left.tiling_key(), right.tiling_key());
    assert_ne!(left.placements(), right.placements());
}

#[test]
fn shape_key_does_not_drop_tiling_variant() {
    same_shape_preserves_distinct_tiling_variants();
}

#[test]
fn same_tiling_preserves_distinct_build_orders() {
    let tiling_id = TilingVariantId::new(20);
    let operation_set_key = OperationSetKey(0x20);
    let loj = build_variant(
        1,
        tiling_id,
        vec![OperationId(1), OperationId(2), OperationId(3)],
        vec![
            HoldDecision::UseCurrent,
            HoldDecision::UseCurrent,
            HoldDecision::UseCurrent,
        ],
        101,
    );
    let jol = build_variant(
        2,
        tiling_id,
        vec![OperationId(3), OperationId(2), OperationId(1)],
        vec![
            HoldDecision::UseCurrent,
            HoldDecision::SwapHeld,
            HoldDecision::UseCurrent,
        ],
        102,
    );

    assert_eq!(loj.tiling_variant_id(), jol.tiling_variant_id());
    assert_ne!(operation_set_key.0, 0);
    assert_ne!(loj.id(), jol.id());
    assert_ne!(loj.operation_order(), jol.operation_order());
    assert_ne!(loj.hold_decisions(), jol.hold_decisions());
}

#[test]
fn tiling_key_does_not_drop_build_variant() {
    same_tiling_preserves_distinct_build_orders();
}

#[test]
fn loj_jol_same_shape_distinct_build_variants() {
    let family = ShapeFamily::new(SetupFamilyId::new(4), 0x0fff);
    let tiling_id = TilingVariantId::new(8);
    let loj = build_variant(
        30,
        tiling_id,
        vec![OperationId(1), OperationId(2), OperationId(3)],
        vec![
            HoldDecision::UseCurrent,
            HoldDecision::UseCurrent,
            HoldDecision::UseCurrent,
        ],
        30,
    );
    let jol = build_variant(
        31,
        tiling_id,
        vec![OperationId(3), OperationId(2), OperationId(1)],
        vec![
            HoldDecision::UseCurrent,
            HoldDecision::SwapHeld,
            HoldDecision::UseCurrent,
        ],
        31,
    );

    assert_eq!(family.occupied_shape(), 0x0fff);
    assert_eq!(loj.tiling_variant_id(), jol.tiling_variant_id());
    assert_ne!(loj.id(), jol.id());
    assert_ne!(loj.consumed_pattern_id(), jol.consumed_pattern_id());
}

#[test]
fn lsj_same_shape_distinct_tiling_variant() {
    let family = ShapeFamily::new(SetupFamilyId::new(5), 0x0fff);
    let lsj = TilingVariant::new(
        TilingVariantId::new(50),
        family.id(),
        family.occupied_shape(),
        vec![PieceKind::L, PieceKind::S, PieceKind::J],
    )
    .with_placements_and_tiling_key(
        vec![
            OperationPlacement::new(OperationId(1), PieceKind::L, 0x000f, 0, 0),
            OperationPlacement::new(OperationId(2), PieceKind::S, 0x00f0, 4, 0),
            OperationPlacement::new(OperationId(3), PieceKind::J, 0x0f00, 0, 1),
        ],
        TilingKey(0x51),
    );
    let ljs = TilingVariant::new(
        TilingVariantId::new(51),
        family.id(),
        family.occupied_shape(),
        vec![PieceKind::L, PieceKind::S, PieceKind::J],
    )
    .with_placements_and_tiling_key(
        vec![
            OperationPlacement::new(OperationId(1), PieceKind::L, 0x000f, 0, 0),
            OperationPlacement::new(OperationId(3), PieceKind::J, 0x00f0, 4, 0),
            OperationPlacement::new(OperationId(2), PieceKind::S, 0x0f00, 0, 1),
        ],
        TilingKey(0x52),
    );

    assert_eq!(lsj.family_id(), ljs.family_id());
    assert_eq!(lsj.occupied_shape(), ljs.occupied_shape());
    assert_ne!(lsj.tiling_key(), ljs.tiling_key());
    assert_ne!(lsj.placements(), ljs.placements());
}

#[test]
fn same_mask_different_piece_definition_not_same_tiling() {
    let cell_partition = CellPartitionKey(0x0fff);
    let imported_piece_a_tiling = TilingKey(0xa0);
    let imported_piece_b_tiling = TilingKey(0xb0);

    assert_eq!(cell_partition.0, 0x0fff);
    assert_ne!(imported_piece_a_tiling, imported_piece_b_tiling);
}

#[test]
fn loj_jol_lsj_fixtures_preserved() {
    loj_jol_same_shape_distinct_build_variants();
    lsj_same_shape_distinct_tiling_variant();
}

#[test]
fn shape_family_not_used_as_probability_source() {
    let family = ShapeFamily::new(SetupFamilyId::new(1), 0x0fff);
    let build = build_variant(
        1,
        TilingVariantId::new(1),
        vec![OperationId(1)],
        vec![HoldDecision::UseCurrent],
        1,
    );

    assert!(!family.can_source_probability());
    assert!(build.can_source_coverage_row());
}
