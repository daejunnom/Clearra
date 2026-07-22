use clearra_core_domain::{
    ids::setup_id::{BuildVariantId, SetupFamilyId, TilingVariantId},
    piece::piece_kind::PieceKind,
};
use clearra_coverage::pattern::pattern_bitset::PatternBitSet;

use super::*;

#[test]
fn build_variant_requires_core_buildup_proof() {
    let tiling = TilingVariant::new(
        TilingVariantId::new(1),
        SetupFamilyId::new(1),
        0b1111,
        vec![PieceKind::I],
    );
    let coverage = PatternBitSet::from_patterns(
        1,
        [clearra_coverage::pattern::pattern_id::PatternId::new(0)],
    )
    .expect("coverage");

    assert!(BuildVariantEnumerator::from_core_buildup(
        BuildVariantId::new(1),
        &tiling,
        None,
        coverage.clone(),
        BuildUpVariantProof::new(1, 1).with_build_input(0b1111, None, 1),
    )
    .is_some());
    assert!(BuildVariantEnumerator::from_core_buildup(
        BuildVariantId::new(2),
        &tiling,
        None,
        coverage,
        BuildUpVariantProof::new(0, 1).with_build_input(0b1111, None, 1),
    )
    .is_none());
}

#[test]
fn setup_build_variant_generated_through_c_buildup_requires_matching_identity() {
    let tiling = TilingVariant::new(
        TilingVariantId::new(1),
        SetupFamilyId::new(1),
        0b1111,
        vec![PieceKind::I],
    );
    let coverage = PatternBitSet::from_patterns(
        1,
        [clearra_coverage::pattern::pattern_id::PatternId::new(0)],
    )
    .expect("coverage");

    assert!(BuildVariantEnumerator::from_core_buildup(
        BuildVariantId::new(1),
        &tiling,
        Some(PieceKind::T),
        coverage.clone(),
        BuildUpVariantProof::new(1, 1).with_build_input(0b1111, Some(PieceKind::T), 1),
    )
    .is_some());
    assert!(BuildVariantEnumerator::from_core_buildup(
        BuildVariantId::new(2),
        &tiling,
        Some(PieceKind::T),
        coverage.clone(),
        BuildUpVariantProof::new(1, 1).with_build_input(0b1110, Some(PieceKind::T), 1),
    )
    .is_none());
    assert!(BuildVariantEnumerator::from_core_buildup(
        BuildVariantId::new(3),
        &tiling,
        Some(PieceKind::T),
        coverage,
        BuildUpVariantProof::new(1, 1).with_build_input(0b1111, Some(PieceKind::I), 1),
    )
    .is_none());
}
