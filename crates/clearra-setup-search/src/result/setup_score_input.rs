use clearra_core_domain::ids::setup_id::{BuildVariantId, SetupFamilyId, TilingVariantId};
use clearra_coverage::pattern::pattern_bitset::PatternBitSet;

use crate::{evaluate::PostPcEvaluation, variant::build_variant::BuildVariant};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SetupBuildScoreInput {
    family_id: SetupFamilyId,
    tiling_variant_id: TilingVariantId,
    build_variant_id: BuildVariantId,
    coverage: PatternBitSet,
    post_pc: PostPcEvaluation,
}

impl SetupBuildScoreInput {
    pub fn new(
        family_id: SetupFamilyId,
        tiling_variant_id: TilingVariantId,
        build_variant_id: BuildVariantId,
        coverage: PatternBitSet,
        post_pc: PostPcEvaluation,
    ) -> Self {
        Self {
            family_id,
            tiling_variant_id,
            build_variant_id,
            coverage,
            post_pc,
        }
    }
}
impl SetupBuildScoreInput {
    pub fn from_build_variant(
        family_id: SetupFamilyId,
        variant: &BuildVariant,
        post_pc: PostPcEvaluation,
    ) -> Self {
        Self::new(
            family_id,
            variant.tiling_variant_id(),
            variant.id(),
            variant.coverage().clone(),
            post_pc,
        )
    }
}
impl SetupBuildScoreInput {
    pub fn family_id(&self) -> SetupFamilyId {
        self.family_id
    }
}
impl SetupBuildScoreInput {
    pub fn tiling_variant_id(&self) -> TilingVariantId {
        self.tiling_variant_id
    }
}
impl SetupBuildScoreInput {
    pub fn build_variant_id(&self) -> BuildVariantId {
        self.build_variant_id
    }
}
impl SetupBuildScoreInput {
    pub fn coverage(&self) -> &PatternBitSet {
        &self.coverage
    }
}
impl SetupBuildScoreInput {
    pub fn post_pc(&self) -> &PostPcEvaluation {
        &self.post_pc
    }
}
