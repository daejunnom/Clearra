use clearra_core_domain::objective::objective_kind::ObjectiveKind;
use clearra_pc_graph::request::{PcCompletionGoal, PcCountPolicy};
use clearra_problem::SearchProblem;

use super::{
    packing_board_descriptor_builder::{active_packing_rows, low_mask},
    FfiProblemError, C_COUNT_ALL, C_COUNT_FIRST_SOLUTION, C_COUNT_UNIQUE, C_GOAL_CLEAR_TO_EMPTY,
    C_OBJECTIVE_ALL, C_OBJECTIVE_MIN_COVER, C_OBJECTIVE_UNIQUE,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PackingGoalMasks {
    pub(crate) goal_region_mask: u64,
    pub(crate) required_fill_mask: u64,
    pub(crate) forbidden_mask: u64,
}

pub(crate) fn packing_goal_masks(
    problem: &SearchProblem,
) -> Result<PackingGoalMasks, FfiProblemError> {
    let width = usize::from(problem.initial_board().width());
    let visible_height = usize::from(problem.visible_height());
    let visible_cells = width.saturating_mul(visible_height);
    if visible_cells == 0 {
        return Err(FfiProblemError::BudgetTooLarge {
            field: "goal_region_cells",
            value: visible_cells,
        });
    }

    let initial_mask = problem.initial_board().occupied_mask();
    let rows = active_packing_rows(problem, width, visible_height, initial_mask);

    let goal_cells = width.saturating_mul(rows);
    if goal_cells == 0 || goal_cells > 64 {
        return Err(FfiProblemError::BudgetTooLarge {
            field: "goal_region_cells",
            value: goal_cells,
        });
    }

    let active_mask = low_mask(goal_cells);
    let goal_region_mask = low_mask(goal_cells);
    Ok(PackingGoalMasks {
        goal_region_mask,
        required_fill_mask: goal_region_mask & !initial_mask,
        forbidden_mask: active_mask & !goal_region_mask,
    })
}

pub(crate) fn goal_code(goal: PcCompletionGoal) -> u32 {
    match goal {
        PcCompletionGoal::ClearToEmpty => C_GOAL_CLEAR_TO_EMPTY,
    }
}

pub(crate) fn count_policy_code(policy: PcCountPolicy) -> u32 {
    match policy {
        PcCountPolicy::FirstSolution => C_COUNT_FIRST_SOLUTION,
        PcCountPolicy::CountAll => C_COUNT_ALL,
        PcCountPolicy::CountUnique => C_COUNT_UNIQUE,
    }
}

pub(crate) fn objective_code(objective: ObjectiveKind) -> u32 {
    match objective {
        ObjectiveKind::All => C_OBJECTIVE_ALL,
        ObjectiveKind::Unique => C_OBJECTIVE_UNIQUE,
        ObjectiveKind::MinimumCover => C_OBJECTIVE_MIN_COVER,
    }
}
