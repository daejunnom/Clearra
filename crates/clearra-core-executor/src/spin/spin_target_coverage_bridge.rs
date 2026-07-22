use clearra_core_ffi::CBuildVariantView;
use clearra_coverage::{
    row::{coverage_row_kind::CoverageRowKind, SpinCoverageRow},
    universe::{PatternUniverseId, PatternWeightModelId},
};
use clearra_scoring::spin::SpinTargetId;

use crate::spin::spin_target_runner_error::SpinTargetRunnerError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpinTargetCoverageBridgeError {
    MissingPieceSourceIdentity,
    CoveragePatternIdOutOfRange {
        pattern_id: usize,
        pattern_count: usize,
    },
    RowKindNotSpinTarget,
}

impl From<SpinTargetCoverageBridgeError> for SpinTargetRunnerError {
    fn from(error: SpinTargetCoverageBridgeError) -> Self {
        Self::CoverageBridge(error)
    }
}

pub struct SpinTargetCoverageBridge;

impl SpinTargetCoverageBridge {
    pub fn row_from_build_variant(
        spin_target_id: &SpinTargetId,
        variant: &CBuildVariantView,
        piece_source_id: u64,
        pattern_count: usize,
        pattern_universe_id: PatternUniverseId,
        pattern_weight_model_id: PatternWeightModelId,
    ) -> Result<SpinCoverageRow, SpinTargetCoverageBridgeError> {
        if piece_source_id == 0 {
            return Err(SpinTargetCoverageBridgeError::MissingPieceSourceIdentity);
        }
        let pattern_id = variant.coverage_pattern_id() as usize;
        if pattern_id >= pattern_count {
            return Err(SpinTargetCoverageBridgeError::CoveragePatternIdOutOfRange {
                pattern_id,
                pattern_count,
            });
        }

        let row = SpinCoverageRow::new(
            variant.candidate_id(),
            piece_source_id,
            spin_target_id.clone(),
            pattern_universe_id,
            pattern_weight_model_id,
            clearra_coverage::pattern::pattern_bitset::PatternBitSet::from_patterns(
                pattern_count,
                [clearra_coverage::pattern::pattern_id::PatternId::new(
                    pattern_id,
                )],
            )
            .expect("coverage pattern id checked against pattern_count"),
        );

        if !matches!(
            row.row().row_kind(),
            CoverageRowKind::SpinTarget(row_target) if row_target == spin_target_id
        ) {
            return Err(SpinTargetCoverageBridgeError::RowKindNotSpinTarget);
        }

        Ok(row)
    }
}
