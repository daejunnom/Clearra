use clearra_coverage::{
    matrix::coverage_matrix::{CoverageMatrixError, TypedCoverageMatrix},
    pattern::{
        pattern_bitset::{PatternBitSet, PatternBitSetError},
        pattern_id::PatternId,
    },
    row::{coverage_row::CoverageRow, coverage_row_kind::CoverageRowKind},
    universe::{
        pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId,
    },
};

use super::GpuWorkerBuildUpMode;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GpuWorkerBuildVariantCoverageInput {
    pub candidate_id: u64,
    pub coverage_pattern_id: usize,
    pub pattern_verified_by_buildup_or_intersection: bool,
}

impl GpuWorkerBuildVariantCoverageInput {
    pub const fn pattern_specific_buildup(candidate_id: u64, coverage_pattern_id: usize) -> Self {
        Self {
            candidate_id,
            coverage_pattern_id,
            pattern_verified_by_buildup_or_intersection: true,
        }
    }
}
impl GpuWorkerBuildVariantCoverageInput {
    pub const fn unverified_for_test(candidate_id: u64, coverage_pattern_id: usize) -> Self {
        Self {
            candidate_id,
            coverage_pattern_id,
            pattern_verified_by_buildup_or_intersection: false,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GpuWorkerCoverageBridgeError {
    VerifyFirstCannotSourceCoverage,
    CountVariantsCannotSourceCoverage,
    UnverifiedPatternCannotSourceCoverage {
        candidate_id: u64,
        coverage_pattern_id: usize,
    },
    Pattern(PatternBitSetError),
    Matrix(CoverageMatrixError),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GpuWorkerCoverageBridgeReport {
    row_count: usize,
    pattern_universe_id: PatternUniverseId,
    pattern_weight_model_id: PatternWeightModelId,
    from_enumerate_variants: bool,
    verify_first_rejected: bool,
}

impl GpuWorkerCoverageBridgeReport {
    pub const fn row_count(self) -> usize {
        self.row_count
    }
}
impl GpuWorkerCoverageBridgeReport {
    pub const fn pattern_universe_id(self) -> PatternUniverseId {
        self.pattern_universe_id
    }
}
impl GpuWorkerCoverageBridgeReport {
    pub const fn pattern_weight_model_id(self) -> PatternWeightModelId {
        self.pattern_weight_model_id
    }
}
impl GpuWorkerCoverageBridgeReport {
    pub const fn from_enumerate_variants(self) -> bool {
        self.from_enumerate_variants
    }
}
impl GpuWorkerCoverageBridgeReport {
    pub const fn verify_first_rejected(self) -> bool {
        self.verify_first_rejected
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GpuWorkerCoverageBridge;

impl GpuWorkerCoverageBridge {
    pub fn matrix_from_enumerated_build_variants(
        mode: GpuWorkerBuildUpMode,
        piece_source_id: u64,
        pattern_universe_id: PatternUniverseId,
        pattern_weight_model_id: PatternWeightModelId,
        pattern_count: usize,
        variants: &[GpuWorkerBuildVariantCoverageInput],
    ) -> Result<(TypedCoverageMatrix, GpuWorkerCoverageBridgeReport), GpuWorkerCoverageBridgeError>
    {
        match mode {
            GpuWorkerBuildUpMode::VerifyFirst => {
                return Err(GpuWorkerCoverageBridgeError::VerifyFirstCannotSourceCoverage);
            }
            GpuWorkerBuildUpMode::CountVariants => {
                return Err(GpuWorkerCoverageBridgeError::CountVariantsCannotSourceCoverage);
            }
            GpuWorkerBuildUpMode::EnumerateVariants => {}
        }

        let mut rows = Vec::with_capacity(variants.len());
        for variant in variants {
            if !variant.pattern_verified_by_buildup_or_intersection {
                return Err(
                    GpuWorkerCoverageBridgeError::UnverifiedPatternCannotSourceCoverage {
                        candidate_id: variant.candidate_id,
                        coverage_pattern_id: variant.coverage_pattern_id,
                    },
                );
            }
            let bitset = PatternBitSet::from_patterns(
                pattern_count,
                [PatternId::new(variant.coverage_pattern_id)],
            )
            .map_err(GpuWorkerCoverageBridgeError::Pattern)?;
            rows.push(CoverageRow::new_with_piece_source(
                variant.candidate_id,
                CoverageRowKind::Build,
                piece_source_id,
                pattern_universe_id,
                pattern_weight_model_id,
                bitset,
            ));
        }

        let matrix = TypedCoverageMatrix::from_rows(
            CoverageRowKind::Build,
            pattern_universe_id,
            pattern_weight_model_id,
            pattern_count,
            rows,
        )
        .map_err(GpuWorkerCoverageBridgeError::Matrix)?;

        Ok((
            matrix,
            GpuWorkerCoverageBridgeReport {
                row_count: variants.len(),
                pattern_universe_id,
                pattern_weight_model_id,
                from_enumerate_variants: true,
                verify_first_rejected: false,
            },
        ))
    }
}
