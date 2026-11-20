use clearra_core_domain::{
    board::board_size::BoardSize,
    piece::piece_kind::PieceKind,
    solution::{
        PiecePlacementMask, StandardBoard64TilingIdentity, STANDARD_BOARD64_TILING_MAX_PLACEMENTS,
    },
};
use clearra_coverage::{
    pattern::pattern_bitset::PatternBitSet,
    universe::{
        pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId,
    },
};

use crate::{
    assignment::slot_assignment::SlotAssignment,
    coverage::build_coverage_matrix::BuildCoverageMatrix,
    query::build_coverage_query::BuildCoverageQuery,
    template::{
        BuildSlotId, SlotCanonicalization, SlotHoldConstraint, SlotOrderConstraint, SlotSymmetry,
        TemplateCanonicalization, TemplateSymmetry,
    },
};

use super::BuildCoverageExecutionError as Error;

/// Selects a physical row frame and the provider's uniform legacy ordinal
/// universe. Callers must resolve this from the request independently of any
/// returned rows. The IDs bind supplied evidence; they do not prove coverage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConcreteCoverageScope {
    board: BoardSize,
    piece_source_id: u64,
    universe: PatternUniverseId,
    uniform_weight_model: PatternWeightModelId,
    // Issued by the producer owner for the exact movement/rules, hold policy,
    // and admitted ordinal mapping of this request. Not a placement hash.
    execution_scope: u64,
}

impl ConcreteCoverageScope {
    pub const fn new_uniform(
        board: BoardSize,
        piece_source_id: u64,
        universe: PatternUniverseId,
        uniform_weight_model: PatternWeightModelId,
        execution_scope: u64,
    ) -> Self {
        Self {
            board,
            piece_source_id,
            universe,
            uniform_weight_model,
            execution_scope,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConcreteCoverageCompleteness {
    /// All original ordinals in scope were accounted for by the provider,
    /// including any that do not admit this concrete assignment.
    Complete,
    Incomplete,
}

/// One provider's coverage for exact piece placements on an initially empty
/// board. A color union, hash or native candidate ID is not a placement key.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConcreteAssignmentCoverageRow {
    identity: StandardBoard64TilingIdentity,
    scope: ConcreteCoverageScope,
    covered_patterns: PatternBitSet,
    completeness: ConcreteCoverageCompleteness,
}

impl ConcreteAssignmentCoverageRow {
    pub fn new(
        identity: StandardBoard64TilingIdentity,
        scope: ConcreteCoverageScope,
        covered_patterns: PatternBitSet,
        completeness: ConcreteCoverageCompleteness,
    ) -> Self {
        Self {
            identity,
            scope,
            covered_patterns,
            completeness,
        }
    }
}

pub(super) struct ConcreteAssignmentTemplate {
    sorted_slots: Vec<(BuildSlotId, u64)>,
}

pub(super) fn validate_verified_query(
    query: &BuildCoverageQuery,
    expected_scope: ConcreteCoverageScope,
) -> Result<ConcreteAssignmentTemplate, Error> {
    let template = query.template();
    let board = template.board_size();
    if expected_scope.board != board
        || expected_scope.piece_source_id == 0
        || expected_scope.universe.get() == 0
        || expected_scope.uniform_weight_model.get() == 0
        || expected_scope.execution_scope == 0
    {
        return Err(Error::InvalidConcreteCoverageScope);
    }
    let limits = query.limits();
    if query.pattern_count() == 0
        || query.pattern_count() > limits.max_patterns()
        || limits.max_assignments() == 0
    {
        return Err(Error::InvalidConcreteCoverageLimits);
    }
    if board.area() > u64::BITS
        || template.slots().is_empty()
        || template.slots().len() > STANDARD_BOARD64_TILING_MAX_PLACEMENTS
        || template.slots().len() != query.domains().len()
        || template.symmetry() != TemplateSymmetry::None
        || template.canonicalization() != TemplateCanonicalization::None
    {
        return Err(Error::InvalidConcreteAssignmentTemplate);
    }

    let mut seen_cells = 0;
    let mut slots = Vec::with_capacity(template.slots().len());
    for slot in template.slots() {
        if slot.cells().len() != 4
            || slot.hold_constraint() != SlotHoldConstraint::Any
            || slot.order_constraint() != SlotOrderConstraint::Any
            || slot.symmetry() != SlotSymmetry::None
            || slot.canonicalization() != SlotCanonicalization::None
        {
            return Err(Error::InvalidConcreteAssignmentTemplate);
        }
        let mut mask = 0;
        for cell in slot.cells() {
            if cell.x() >= board.width() || cell.y() >= board.height() {
                return Err(Error::InvalidConcreteAssignmentTemplate);
            }
            let bit = u64::from(board.width()) * u64::from(cell.y()) + u64::from(cell.x());
            let cell_mask = 1_u64 << bit;
            if seen_cells & cell_mask != 0 {
                return Err(Error::InvalidConcreteAssignmentTemplate);
            }
            seen_cells |= cell_mask;
            mask |= cell_mask;
        }
        slots.push((slot.id(), mask));
    }
    slots.sort_unstable_by_key(|&(slot, _)| slot);
    if slots.windows(2).any(|pair| pair[0].0 == pair[1].0) {
        return Err(Error::InvalidConcreteAssignmentTemplate);
    }
    if query
        .constraints()
        .iter()
        .any(|constraint| template.slot(constraint.slot_id()).is_none())
    {
        return Err(Error::InvalidConcreteAssignmentTemplate);
    }
    for (index, domain) in query.domains().iter().enumerate() {
        let id = domain.slot_id();
        let slot = template
            .slot(id)
            .ok_or(Error::InvalidConcreteAssignmentTemplate)?;
        if domain.is_empty()
            || domain.pieces().len() > PieceKind::STANDARD_TETROMINOES.len()
            || query.domains()[..index]
                .iter()
                .any(|other| other.slot_id() == id)
            || domain
                .pieces()
                .iter()
                .enumerate()
                .any(|(index, piece)| domain.pieces()[..index].contains(piece))
            || !domain
                .pieces()
                .iter()
                .all(|piece| slot.allowed_pieces().contains(piece))
        {
            return Err(Error::InvalidConcreteAssignmentTemplate);
        }
        // The existing DLX solver consumes explicit query constraints, not
        // the template display metadata. Fail closed if its effective domain
        // would admit a piece forbidden by a required template piece.
        if let Some(required) = slot.required_piece() {
            let has_other_allowed_piece = domain.pieces().iter().copied().any(|piece| {
                piece != required
                    && query
                        .constraints()
                        .iter()
                        .filter(|constraint| constraint.slot_id() == id)
                        .all(|constraint| constraint.allows(piece))
            });
            if has_other_allowed_piece {
                return Err(Error::InvalidConcreteAssignmentTemplate);
            }
        }
    }
    Ok(ConcreteAssignmentTemplate {
        sorted_slots: slots,
    })
}

impl ConcreteAssignmentTemplate {
    fn identity(
        &self,
        assignment: &SlotAssignment,
    ) -> Result<StandardBoard64TilingIdentity, Error> {
        if assignment.assigned_slots().len() != self.sorted_slots.len() {
            return Err(Error::InvalidConcreteAssignmentTemplate);
        }
        let mut placements = [None; STANDARD_BOARD64_TILING_MAX_PLACEMENTS];
        for assigned in assignment.assigned_slots() {
            let index = self
                .sorted_slots
                .binary_search_by_key(&assigned.slot_id(), |&(slot, _)| slot)
                .map_err(|_| Error::InvalidConcreteAssignmentTemplate)?;
            let placement = PiecePlacementMask::new(assigned.piece(), self.sorted_slots[index].1);
            if placements[index].replace(placement).is_some() {
                return Err(Error::InvalidConcreteAssignmentTemplate);
            }
        }
        StandardBoard64TilingIdentity::from_placements(0, placements.into_iter().flatten())
            .map_err(|_| Error::InvalidConcreteAssignmentTemplate)
    }
}

pub(super) fn join_verified_rows(
    template: ConcreteAssignmentTemplate,
    assignments: &[SlotAssignment],
    pattern_count: usize,
    expected_scope: ConcreteCoverageScope,
    rows: Vec<ConcreteAssignmentCoverageRow>,
) -> Result<BuildCoverageMatrix, Error> {
    let mut expected = Vec::new();
    expected
        .try_reserve_exact(assignments.len())
        .map_err(|_| Error::ConcreteCoverageStorageUnavailable)?;
    for (index, assignment) in assignments.iter().enumerate() {
        expected.push((template.identity(assignment)?, index));
    }
    expected.sort_unstable_by(|left, right| left.0.cmp(&right.0));
    if expected.windows(2).any(|pair| pair[0].0 == pair[1].0) {
        return Err(Error::AmbiguousConcreteAssignment);
    }

    let mut joined = Vec::new();
    joined
        .try_reserve_exact(rows.len())
        .map_err(|_| Error::ConcreteCoverageStorageUnavailable)?;
    for (row, evidence) in rows.into_iter().enumerate() {
        if evidence.completeness != ConcreteCoverageCompleteness::Complete {
            return Err(Error::IncompleteConcreteCoverageRow { row });
        }
        if evidence.scope != expected_scope {
            return Err(Error::ConcreteCoverageScopeMismatch { row });
        }
        if evidence.covered_patterns.pattern_count() != pattern_count {
            return Err(Error::CoveragePatternCountMismatch {
                expected: pattern_count,
                actual: evidence.covered_patterns.pattern_count(),
            });
        }
        let index = expected
            .binary_search_by(|(identity, _)| identity.cmp(&evidence.identity))
            .map_err(|_| Error::ForeignConcreteCoverageRow { row })?;
        joined.push((expected[index].1, evidence.covered_patterns));
    }
    joined.sort_unstable_by_key(|&(assignment, _)| assignment);
    if let Some(pair) = joined.windows(2).find(|pair| pair[0].0 == pair[1].0) {
        return Err(Error::DuplicateConcreteCoverageRow {
            assignment: pair[0].0,
        });
    }
    for (position, (assignment, _)) in joined.iter().enumerate() {
        if position != *assignment {
            return Err(Error::MissingConcreteCoverageRow {
                assignment: position,
            });
        }
    }
    if joined.len() != assignments.len() {
        return Err(Error::MissingConcreteCoverageRow {
            assignment: joined.len(),
        });
    }

    BuildCoverageMatrix::from_assignment_coverages(
        expected_scope.piece_source_id,
        expected_scope.universe,
        expected_scope.uniform_weight_model,
        pattern_count,
        joined,
    )
    .map_err(Error::from)
}

#[cfg(test)]
#[path = "verified_assignment_coverage_tests.rs"]
mod tests;
