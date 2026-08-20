use clearra_problem::SearchProblem;

use crate::packing_problem::{CPackingCandidate, CPackingOperation, C_PACKING_MAX_OPERATIONS};
use crate::supply::SupplyDescriptorCompiler;
use crate::NativeGeometryCatalog;
use crate::PackingCandidateView;

use super::{
    packing_problem_builder::{materialized_pattern, CPackingProblemBuilder},
    CBuildUpOperation, CBuildUpProblem, CPieceMultisetWindow, FfiProblemError,
    C_BUILDUP_MAX_OPERATIONS, C_LINE_CLEAR_POLICY_STANDARD, C_PIECE_I, C_PIECE_L,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CBuildUpProblemBuilder;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CBuildUpProblemTemplate {
    base: CBuildUpProblem,
    patterns: Box<[super::packing_problem_builder::CMaterializedPattern]>,
}

impl CBuildUpProblemTemplate {
    pub fn compile(problem: &SearchProblem) -> Result<Self, FfiProblemError> {
        let base = CBuildUpProblemBuilder::from_search_problem(problem)?;
        let universe = problem
            .piece_source()
            .materialized_universe()
            .ok_or(FfiProblemError::InvalidSupplyDescriptor)?;
        let mut patterns = Vec::new();
        patterns
            .try_reserve_exact(universe.pattern_count())
            .map_err(|_| FfiProblemError::DescriptorStorageAllocationFailed)?;
        for pattern_id in 0..universe.pattern_count() {
            let pattern_id =
                u32::try_from(pattern_id).map_err(|_| FfiProblemError::InvalidSupplyDescriptor)?;
            patterns.push(materialized_pattern(problem, pattern_id)?);
        }
        Ok(Self {
            base,
            patterns: patterns.into_boxed_slice(),
        })
    }

    pub fn compile_for_standard_bag_automaton(
        problem: &SearchProblem,
    ) -> Result<Self, FfiProblemError> {
        Ok(Self {
            base: CBuildUpProblemBuilder::from_search_problem(problem)?,
            patterns: Box::new([]),
        })
    }

    pub const fn new_scratch(&self) -> CBuildUpProblem {
        self.base
    }

    pub fn new_standard_bag_automaton_scratch(&self) -> Result<CBuildUpProblem, FfiProblemError> {
        let mut scratch = self.base;
        configure_standard_bag_automaton(&mut scratch)?;
        Ok(scratch)
    }

    pub fn configure_piece_source_pattern(
        &self,
        scratch: &mut CBuildUpProblem,
        piece_source_pattern_id: u32,
    ) -> Result<(), FfiProblemError> {
        let pattern = self.pattern(piece_source_pattern_id)?;
        configure_concrete_pattern(scratch, pattern, piece_source_pattern_id);
        Ok(())
    }

    pub fn configure_packing_candidate(
        &self,
        scratch: &mut CBuildUpProblem,
        candidate: &CPackingCandidate,
        piece_source_pattern_id: u32,
        coverage_pattern_id: u32,
    ) -> Result<(), FfiProblemError> {
        let pattern = self.pattern(piece_source_pattern_id)?;
        configure_candidate_geometry(
            &self.base,
            scratch,
            CandidateDescriptor {
                candidate_id: candidate.candidate_id,
                canonical_operation_set_id: candidate.canonical_operation_set_id,
                geometry_variant_domains: candidate.geometry_variant_domains,
                operation_count: usize::from(candidate.operation_count),
            },
            candidate.operations.iter().copied(),
            coverage_pattern_id,
        )?;
        configure_concrete_pattern(scratch, pattern, piece_source_pattern_id);
        Ok(())
    }

    pub fn configure_packing_candidate_view(
        &self,
        scratch: &mut CBuildUpProblem,
        candidate: PackingCandidateView<'_>,
        piece_source_pattern_id: u32,
        coverage_pattern_id: u32,
    ) -> Result<(), FfiProblemError> {
        let pattern = self.pattern(piece_source_pattern_id)?;
        configure_candidate_geometry(
            &self.base,
            scratch,
            CandidateDescriptor {
                candidate_id: candidate.candidate_id(),
                canonical_operation_set_id: candidate.canonical_operation_set_id(),
                geometry_variant_domains: candidate.geometry_variant_domains(),
                operation_count: candidate.operation_count(),
            },
            candidate.operations(),
            coverage_pattern_id,
        )?;
        configure_concrete_pattern(scratch, pattern, piece_source_pattern_id);
        Ok(())
    }

    pub fn configure_packing_candidate_with_standard_bag_automaton(
        &self,
        scratch: &mut CBuildUpProblem,
        candidate: &CPackingCandidate,
    ) -> Result<(), FfiProblemError> {
        configure_candidate_geometry(
            &self.base,
            scratch,
            CandidateDescriptor {
                candidate_id: candidate.candidate_id,
                canonical_operation_set_id: candidate.canonical_operation_set_id,
                geometry_variant_domains: candidate.geometry_variant_domains,
                operation_count: usize::from(candidate.operation_count),
            },
            candidate.operations.iter().copied(),
            0,
        )?;
        configure_standard_bag_automaton(scratch)?;
        Ok(())
    }

    pub fn attach_geometry_catalog(
        &self,
        scratch: &mut CBuildUpProblem,
        catalog: &NativeGeometryCatalog,
    ) {
        scratch.geometry_catalog = catalog.raw_pointer() as usize;
    }

    pub fn configure_packing_candidate_view_with_standard_bag_automaton(
        &self,
        scratch: &mut CBuildUpProblem,
        candidate: PackingCandidateView<'_>,
    ) -> Result<(), FfiProblemError> {
        configure_candidate_geometry(
            &self.base,
            scratch,
            CandidateDescriptor {
                candidate_id: candidate.candidate_id(),
                canonical_operation_set_id: candidate.canonical_operation_set_id(),
                geometry_variant_domains: candidate.geometry_variant_domains(),
                operation_count: candidate.operation_count(),
            },
            candidate.operations(),
            0,
        )?;
        configure_standard_bag_automaton(scratch)?;
        Ok(())
    }

    fn pattern(
        &self,
        pattern_id: u32,
    ) -> Result<&super::packing_problem_builder::CMaterializedPattern, FfiProblemError> {
        self.patterns
            .get(pattern_id as usize)
            .ok_or(FfiProblemError::PatternIdOutOfRange {
                pattern_id: pattern_id as usize,
                pattern_count: self.patterns.len(),
            })
    }
}

impl CBuildUpProblemBuilder {
    pub fn from_search_problem(
        problem: &SearchProblem,
    ) -> Result<CBuildUpProblem, FfiProblemError> {
        let packing = CPackingProblemBuilder::from_search_problem(problem)?;
        let supply = SupplyDescriptorCompiler::compile(problem)?;
        Ok(CBuildUpProblem {
            initial_board: packing.board,
            piece_source: supply.piece_source(),
            piece_source_pattern_pieces: packing.piece_source_pattern_pieces,
            piece_source_pattern_len: packing.piece_source_pattern_len,
            piece_source_pattern_complete: packing.piece_source_pattern_complete,
            piece_source_pattern_reserved: 0,
            piece_source_pattern_truncation_reason: packing.piece_source_pattern_truncation_reason,
            piece_source_pattern_id: packing.piece_source_pattern_id,
            initial_hold_automaton: supply.initial_hold_automaton(),
            rule: packing.rule,
            line_clear_policy: C_LINE_CLEAR_POLICY_STANDARD,
            piece_window: packing.piece_window,
            goal: packing.goal,
            packing,
            buildup_flags: if problem.supply().hold_enabled() {
                crate::problem::C_BUILDUP_FLAG_HOLD_ENABLED
            } else {
                0
            },
            source_execution_mode: crate::problem::C_BUILDUP_SOURCE_CONCRETE_PATTERN,
            terminal_projection_policy_version:
                crate::problem::C_BUILDUP_TERMINAL_PROJECTION_POLICY_VERSION,
            terminal_projection_policy: if problem.supply().projects_unplaced_lookahead()
                && !problem.supply().projects_standard_bag_lookahead()
            {
                crate::problem::C_BUILDUP_TERMINAL_PROJECTION_RELEASE_FINITE_HELD
            } else {
                crate::problem::C_BUILDUP_TERMINAL_PROJECTION_DISABLED
            },
            terminal_projection_reserved: 0,
            ..Default::default()
        })
    }
}
impl CBuildUpProblemBuilder {
    pub fn from_packing_candidate(
        problem: &SearchProblem,
        candidate: &CPackingCandidate,
        piece_source_pattern_id: u32,
        coverage_pattern_id: u32,
    ) -> Result<CBuildUpProblem, FfiProblemError> {
        let base = Self::from_search_problem(problem)?;
        let mut buildup = base;
        let pattern = materialized_pattern(problem, piece_source_pattern_id)?;
        configure_candidate_geometry(
            &base,
            &mut buildup,
            CandidateDescriptor {
                candidate_id: candidate.candidate_id,
                canonical_operation_set_id: candidate.canonical_operation_set_id,
                geometry_variant_domains: candidate.geometry_variant_domains,
                operation_count: usize::from(candidate.operation_count),
            },
            candidate.operations.iter().copied(),
            coverage_pattern_id,
        )?;
        configure_concrete_pattern(&mut buildup, &pattern, piece_source_pattern_id);
        Ok(buildup)
    }

    pub fn from_packing_candidate_with_standard_bag_automaton(
        problem: &SearchProblem,
        candidate: &CPackingCandidate,
    ) -> Result<CBuildUpProblem, FfiProblemError> {
        let mut buildup = Self::from_packing_candidate(problem, candidate, 0, 0)?;
        if buildup.piece_source.exact_bag_automaton_supported == 0 {
            return Err(FfiProblemError::InvalidSupplyDescriptor);
        }
        buildup.source_execution_mode = crate::problem::C_BUILDUP_SOURCE_STANDARD_BAG_AUTOMATON;
        Ok(buildup)
    }
}

#[derive(Clone, Copy)]
struct CandidateDescriptor {
    candidate_id: u64,
    canonical_operation_set_id: u64,
    geometry_variant_domains: u16,
    operation_count: usize,
}

#[allow(clippy::too_many_arguments)]
fn configure_candidate_geometry(
    base: &CBuildUpProblem,
    scratch: &mut CBuildUpProblem,
    candidate: CandidateDescriptor,
    operations: impl Iterator<Item = CPackingOperation>,
    coverage_pattern_id: u32,
) -> Result<(), FfiProblemError> {
    let operation_count = candidate.operation_count;
    if operation_count > C_PACKING_MAX_OPERATIONS || operation_count > C_BUILDUP_MAX_OPERATIONS {
        return Err(FfiProblemError::CandidateOperationCountTooLarge { operation_count });
    }
    if operation_count > usize::from(base.piece_window.max_pieces) {
        return Err(FfiProblemError::CandidateOperationCountTooLarge { operation_count });
    }

    let candidate_piece_count = u8::try_from(operation_count)
        .map_err(|_| FfiProblemError::CandidateOperationCountTooLarge { operation_count })?;
    let mut candidate_multiset = CPieceMultisetWindow::default();
    candidate_multiset.total_count = candidate_piece_count;
    candidate_multiset.exact_count = candidate_piece_count;
    for (index, operation) in operations.take(operation_count).enumerate() {
        if !(C_PIECE_I..=C_PIECE_L).contains(&operation.piece) {
            return Err(FfiProblemError::InvalidCandidatePiece {
                piece: operation.piece,
            });
        }
        candidate_multiset.counts[usize::from(operation.piece)] += 1;
        scratch.operation_set.representative_order_hint[index] = index as u16;
        scratch.operation_set.operations[index] = CBuildUpOperation {
            piece: operation.piece,
            rotation: operation.rotation,
            x: operation.x,
            y: operation.y,
            operation_id: operation.operation_id,
            required_deleted_row_mask: operation.required_deleted_row_mask,
            mask: operation.mask,
        };
    }

    scratch.packing.piece_multiset_window = candidate_multiset;
    scratch.operation_set.operation_count = operation_count as u16;
    scratch.operation_set.geometry_variant_domains = candidate.geometry_variant_domains;
    scratch.candidate_id = candidate.candidate_id;
    scratch.canonical_operation_set_id = if candidate.canonical_operation_set_id == 0 {
        candidate.candidate_id
    } else {
        candidate.canonical_operation_set_id
    };
    scratch.coverage_pattern_id = coverage_pattern_id;
    scratch.initial_hold_automaton = base.initial_hold_automaton;
    Ok(())
}

fn configure_concrete_pattern(
    scratch: &mut CBuildUpProblem,
    pattern: &super::packing_problem_builder::CMaterializedPattern,
    piece_source_pattern_id: u32,
) {
    scratch.piece_source_pattern_pieces = pattern.pieces;
    scratch.piece_source_pattern_len = pattern.len;
    scratch.piece_source_pattern_complete = 1;
    scratch.piece_source_pattern_truncation_reason = 0;
    scratch.piece_source_pattern_id = piece_source_pattern_id;
    scratch.packing.piece_source_pattern_pieces = pattern.pieces;
    scratch.packing.piece_source_pattern_len = pattern.len;
    scratch.packing.piece_source_pattern_complete = 1;
    scratch.packing.piece_source_pattern_truncation_reason = 0;
    scratch.packing.piece_source_pattern_id = piece_source_pattern_id;
    scratch.source_execution_mode = crate::problem::C_BUILDUP_SOURCE_CONCRETE_PATTERN;
}

fn configure_standard_bag_automaton(scratch: &mut CBuildUpProblem) -> Result<(), FfiProblemError> {
    if scratch.piece_source.exact_bag_automaton_supported == 0 {
        return Err(FfiProblemError::InvalidSupplyDescriptor);
    }
    scratch.piece_source_pattern_id = 0;
    scratch.packing.piece_source_pattern_id = 0;
    scratch.source_execution_mode = crate::problem::C_BUILDUP_SOURCE_STANDARD_BAG_AUTOMATON;
    scratch.terminal_projection_policy = crate::problem::C_BUILDUP_TERMINAL_PROJECTION_DISABLED;
    Ok(())
}

#[cfg(test)]
#[path = "buildup_problem_builder_tests.rs"]
mod tests;
