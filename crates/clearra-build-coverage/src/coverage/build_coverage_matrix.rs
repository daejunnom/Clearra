use clearra_coverage::{
    matrix::coverage_matrix::{CoverageMatrixError, TypedCoverageMatrix},
    pattern::pattern_bitset::PatternBitSet,
    row::{coverage_row::CoverageRow, coverage_row_kind::CoverageRowKind},
    universe::{
        pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId,
    },
};

use crate::assignment::slot_assignment::SlotAssignment;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildCoverageMatrix {
    matrix: TypedCoverageMatrix,
}

impl BuildCoverageMatrix {
    pub fn from_assignment_coverages(
        piece_source_id: u64,
        pattern_universe_id: PatternUniverseId,
        pattern_weight_model_id: PatternWeightModelId,
        pattern_count: usize,
        assignment_coverages: Vec<(usize, PatternBitSet)>,
    ) -> Result<Self, BuildCoverageMatrixError> {
        let rows = assignment_coverages
            .into_iter()
            .map(|(candidate_id, coverage)| {
                CoverageRow::new_with_piece_source(
                    candidate_id as u64,
                    CoverageRowKind::Build,
                    piece_source_id,
                    pattern_universe_id,
                    pattern_weight_model_id,
                    coverage,
                )
            })
            .collect::<Vec<_>>();
        Ok(Self {
            matrix: TypedCoverageMatrix::from_rows(
                CoverageRowKind::Build,
                pattern_universe_id,
                pattern_weight_model_id,
                pattern_count,
                rows,
            )?,
        })
    }
}
impl BuildCoverageMatrix {
    pub fn from_assignments_with_coverages(
        piece_source_id: u64,
        pattern_universe_id: PatternUniverseId,
        pattern_weight_model_id: PatternWeightModelId,
        pattern_count: usize,
        assignments: &[SlotAssignment],
        coverages: &[PatternBitSet],
    ) -> Result<Self, BuildCoverageMatrixError> {
        if assignments.len() != coverages.len() {
            return Err(BuildCoverageMatrixError::AssignmentCoverageLengthMismatch {
                assignments: assignments.len(),
                coverages: coverages.len(),
            });
        }

        let assignment_coverages = assignments
            .iter()
            .enumerate()
            .zip(coverages.iter().cloned())
            .map(|((index, _assignment), coverage)| (index, coverage))
            .collect::<Vec<_>>();
        Self::from_assignment_coverages(
            piece_source_id,
            pattern_universe_id,
            pattern_weight_model_id,
            pattern_count,
            assignment_coverages,
        )
    }
}
impl BuildCoverageMatrix {
    pub fn matrix(&self) -> &TypedCoverageMatrix {
        &self.matrix
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BuildCoverageMatrixError {
    AssignmentCoverageLengthMismatch {
        assignments: usize,
        coverages: usize,
    },
    Matrix(CoverageMatrixError),
}

impl From<CoverageMatrixError> for BuildCoverageMatrixError {
    fn from(error: CoverageMatrixError) -> Self {
        Self::Matrix(error)
    }
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::piece::piece_kind::PieceKind;
    use clearra_coverage::pattern::{pattern_bitset::PatternBitSet, pattern_id::PatternId};

    use crate::{
        assignment::slot_assignment::{AssignedSlot, SlotAssignment},
        template::build_slot::BuildSlotId,
    };

    use super::*;

    #[test]
    fn rejects_assignment_coverage_length_mismatch() {
        let slot = BuildSlotId::new(1);
        let assignments = vec![
            SlotAssignment::new(vec![AssignedSlot::new(slot, PieceKind::I)]),
            SlotAssignment::new(vec![AssignedSlot::new(slot, PieceKind::O)]),
            SlotAssignment::new(vec![AssignedSlot::new(slot, PieceKind::T)]),
        ];
        let coverages = vec![
            PatternBitSet::from_patterns(2, [PatternId::new(0)]).expect("coverage 0"),
            PatternBitSet::from_patterns(2, [PatternId::new(1)]).expect("coverage 1"),
        ];

        let result = BuildCoverageMatrix::from_assignments_with_coverages(
            11,
            PatternUniverseId::new(1),
            PatternWeightModelId::new(7),
            2,
            &assignments,
            &coverages,
        );

        assert_eq!(
            result,
            Err(BuildCoverageMatrixError::AssignmentCoverageLengthMismatch {
                assignments: 3,
                coverages: 2
            })
        );
    }
}
