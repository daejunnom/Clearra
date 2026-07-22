use clearra_core_domain::{
    ids::setup_id::{BuildVariantId, SetupFamilyId, TilingVariantId},
    piece::piece_kind::PieceKind,
};
use clearra_coverage::pattern::{
    pattern_bitset::PatternBitSet, pattern_id::PatternId, weighted_pattern_set::WeightedPatternSet,
};

use crate::{
    coverage::{
        setup_coverage_builder::SetupCoverageBuilder, setup_union_coverage::SetupUnionCoverage,
    },
    identity::{build_identity::BuildIdentity, shape_family::ShapeFamily},
    variant::build_variant::BuildVariant,
};

use super::*;

#[test]
fn setup_probability_uses_union_not_variant_sum() {
    let family_id = SetupFamilyId::new(1);
    let variant_a = BuildVariant::new(
        BuildVariantId::new(10),
        TilingVariantId::new(20),
        BuildIdentity::new(0b1111, Some(PieceKind::I)),
        PatternBitSet::from_patterns(2, [PatternId::new(0), PatternId::new(1)])
            .expect("coverage A"),
    );
    let variant_b = BuildVariant::new(
        BuildVariantId::new(11),
        TilingVariantId::new(20),
        BuildIdentity::new(0b1111, Some(PieceKind::I)),
        PatternBitSet::from_patterns(2, [PatternId::new(0), PatternId::new(1)])
            .expect("coverage B"),
    );
    let mut builder = SetupCoverageBuilder::new(ShapeFamily::new(family_id, 0b1111), 2);
    builder.push_variant(&variant_a).expect("variant A");
    builder.push_variant(&variant_b).expect("variant B");
    let matrix = builder.build().expect("matrix");
    let union = SetupUnionCoverage::from_matrix(family_id, &matrix);
    let weights = WeightedPatternSet::uniform(2).expect("weights");

    let probability = SetupProbability::from_union(&union, &weights).expect("probability");

    assert_eq!(probability.probability().get(), 1.0);
}

#[test]
fn setup_probability_uses_pattern_bitset_union() {
    let family_id = SetupFamilyId::new(1);
    let variant_a = BuildVariant::new(
        BuildVariantId::new(10),
        TilingVariantId::new(20),
        BuildIdentity::new(0b1111, None),
        PatternBitSet::from_patterns(3, [PatternId::new(0), PatternId::new(1)])
            .expect("coverage A"),
    );
    let variant_b = BuildVariant::new(
        BuildVariantId::new(11),
        TilingVariantId::new(21),
        BuildIdentity::new(0b1111, None),
        PatternBitSet::from_patterns(3, [PatternId::new(1), PatternId::new(2)])
            .expect("coverage B"),
    );
    let mut builder = SetupCoverageBuilder::new(ShapeFamily::new(family_id, 0b1111), 3);
    builder.push_variant(&variant_a).expect("variant A");
    builder.push_variant(&variant_b).expect("variant B");
    let matrix = builder.build().expect("matrix");
    let union = SetupUnionCoverage::from_matrix(family_id, &matrix);
    let weights = WeightedPatternSet::uniform(3).expect("weights");

    let probability = SetupProbability::from_union(&union, &weights).expect("probability");

    assert_eq!(union.covered_patterns().count_ones(), 3);
    assert_eq!(probability.probability().get(), 1.0);
}

#[test]
fn product_acceptance_simple_family_union_fixture_uses_pattern_union_probability() {
    assert!(
        include_str!("../../../../tests/fixtures/setup/simple_family_union.json")
            .contains("simple_family_union")
    );
    assert!(
        include_str!("../../../../tests/golden/setup/simple_family_probability.json")
            .contains("variant_probability_sum=forbidden")
    );

    let family_id = SetupFamilyId::new(1);
    let variant_a = BuildVariant::new(
        BuildVariantId::new(10),
        TilingVariantId::new(20),
        BuildIdentity::new(0b1111, Some(PieceKind::I)),
        PatternBitSet::from_patterns(4, [PatternId::new(0), PatternId::new(1)])
            .expect("coverage A"),
    );
    let variant_b = BuildVariant::new(
        BuildVariantId::new(11),
        TilingVariantId::new(20),
        BuildIdentity::new(0b1111, Some(PieceKind::I)),
        PatternBitSet::from_patterns(4, [PatternId::new(1), PatternId::new(2)])
            .expect("coverage B"),
    );
    let mut builder = SetupCoverageBuilder::new(ShapeFamily::new(family_id, 0b1111), 4);
    builder.push_variant(&variant_a).expect("variant A");
    builder.push_variant(&variant_b).expect("variant B");
    let matrix = builder.build().expect("matrix");
    let union = SetupUnionCoverage::from_matrix(family_id, &matrix);
    let weights = WeightedPatternSet::uniform(4).expect("weights");

    let probability = SetupProbability::from_union(&union, &weights).expect("probability");

    assert_eq!(union.covered_patterns().count_ones(), 3);
    assert_eq!(probability.family_id(), family_id);
    assert_eq!(probability.probability().get(), 0.75);
}
