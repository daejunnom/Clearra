use crate::{
    problem::{CPieceMultisetWindow, C_QUEUE_VIEW_CAPACITY},
    supply::supply_descriptor_compiler::piece_code,
};
use clearra_problem::SearchProblem;
use clearra_supply::{PackingMultisetFamily, PieceMultisetKey};

use super::{
    packing_backend_descriptor_builder::backend_descriptor,
    packing_board_descriptor_builder::board_descriptor,
    packing_budget_descriptor_builder::budget_descriptor,
    packing_checkpoint_descriptor_builder::{checkpoint_spec, problem_kind},
    packing_goal_descriptor_builder::{
        count_policy_code, goal_code, objective_code, packing_goal_masks,
    },
    packing_problem_builder_error::to_u16,
    packing_rule_descriptor_builder::rule_descriptor,
    packing_supply_descriptor_builder::supply_descriptor,
    CPackingProblem, FfiProblemError, C_PIECE_MULTISET_FAMILY_CAPACITY,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CPackingProblemBuilder;

impl CPackingProblemBuilder {
    pub fn from_search_problem(
        problem: &SearchProblem,
    ) -> Result<CPackingProblem, FfiProblemError> {
        Self::build(problem, None)
    }

    pub fn from_search_problem_with_piece_multiset(
        problem: &SearchProblem,
        piece_multiset: PieceMultisetKey,
    ) -> Result<CPackingProblem, FfiProblemError> {
        Self::build(problem, Some(piece_multiset))
    }

    pub fn from_search_problem_with_piece_multiset_family(
        problem: &SearchProblem,
        family: &PackingMultisetFamily,
    ) -> Result<CPackingProblem, FfiProblemError> {
        if family.len() > C_PIECE_MULTISET_FAMILY_CAPACITY {
            return Err(FfiProblemError::PieceMultisetFamilyTooLarge {
                member_count: family.len(),
                capacity: C_PIECE_MULTISET_FAMILY_CAPACITY,
            });
        }
        let mut compact = Self::build(problem, Some(family.envelope()))?;
        compact.piece_multiset_family.count = family.len() as u16;
        compact.piece_multiset_family.complete = 1;
        for (index, group) in family.groups().iter().enumerate() {
            let mut member = piece_multiset_window(problem, group.key());
            member.exact_count = member.total_count;
            compact.piece_multiset_family.members[index] = member;
        }
        Ok(compact)
    }

    fn build(
        problem: &SearchProblem,
        piece_multiset: Option<PieceMultisetKey>,
    ) -> Result<CPackingProblem, FfiProblemError> {
        let supply = supply_descriptor(problem)?;
        let piece_source = supply.piece_source();
        let max_pieces = to_u16(problem.piece_window().max_pieces(), |value| {
            FfiProblemError::PieceWindowTooLarge { max_pieces: value }
        })?;
        let exact_pieces = match problem.exact_pieces() {
            Some(value) => to_u16(value, |exact_pieces| FfiProblemError::ExactPiecesTooLarge {
                exact_pieces,
            })?,
            None => 0,
        };
        let board = board_descriptor(problem)?;
        let goal_masks = packing_goal_masks(problem)?;
        let pattern = materialized_pattern(problem, 0)?;
        let piece_multiset_window = piece_multiset
            .map(|key| piece_multiset_window(problem, key))
            .unwrap_or_else(|| supply.piece_multiset_window());

        Ok(CPackingProblem {
            problem_kind: problem_kind(problem.preset()),
            max_pieces,
            flags: 0,
            board,
            goal_region_mask: goal_masks.goal_region_mask,
            required_fill_mask: goal_masks.required_fill_mask,
            forbidden_mask: goal_masks.forbidden_mask,
            exact_pieces,
            reserved_goal: 0,
            piece_window: supply.piece_window(),
            piece_multiset_window,
            piece_multiset_family: Default::default(),
            piece_source,
            piece_source_pattern_pieces: pattern.pieces,
            piece_source_pattern_len: pattern.len,
            piece_source_pattern_complete: 1,
            piece_source_pattern_reserved: 0,
            piece_source_pattern_truncation_reason: 0,
            piece_source_pattern_id: 0,
            rule: rule_descriptor(problem)?,
            budget: budget_descriptor(problem)?,
            backend: backend_descriptor(problem.backend_request())?,
            checkpoint: checkpoint_spec(problem),
            goal: goal_code(problem.goal()),
            count_policy: count_policy_code(problem.count_policy()),
            objective: objective_code(problem.objective().kind()),
            label_count: problem.labels().len() as u32,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct CMaterializedPattern {
    pub pieces: [u8; C_QUEUE_VIEW_CAPACITY],
    pub len: u16,
}

pub(super) fn materialized_pattern(
    problem: &SearchProblem,
    pattern_id: u32,
) -> Result<CMaterializedPattern, FfiProblemError> {
    let universe = problem
        .piece_source()
        .materialized_universe()
        .ok_or(FfiProblemError::InvalidSupplyDescriptor)?;
    let pattern_index = pattern_id as usize;
    if pattern_index >= universe.pattern_count() {
        return Err(FfiProblemError::PatternIdOutOfRange {
            pattern_id: pattern_index,
            pattern_count: universe.pattern_count(),
        });
    }
    let sequence = universe.sequence_at(pattern_index);
    if sequence.len() > C_QUEUE_VIEW_CAPACITY {
        return Err(FfiProblemError::PatternSequenceTooLong {
            len: sequence.len(),
            capacity: C_QUEUE_VIEW_CAPACITY,
        });
    }
    let mut pieces = [0; C_QUEUE_VIEW_CAPACITY];
    for (index, piece) in sequence.iter().enumerate() {
        pieces[index] = piece_code(*piece);
    }
    Ok(CMaterializedPattern {
        pieces,
        len: sequence.len() as u16,
    })
}

fn piece_multiset_window(
    problem: &SearchProblem,
    piece_multiset: PieceMultisetKey,
) -> CPieceMultisetWindow {
    let mut window = CPieceMultisetWindow::default();
    window.total_count = piece_multiset.total_count();
    window.exact_count = problem
        .exact_pieces()
        .and_then(|count| u8::try_from(count).ok())
        .unwrap_or(0);
    for piece in clearra_core_domain::piece::piece_kind::PieceKind::STANDARD_TETROMINOES {
        window.counts[usize::from(piece_code(piece))] = piece_multiset.count(piece);
    }
    window
}

#[cfg(test)]
#[path = "packing_problem_builder_tests.rs"]
mod tests;
