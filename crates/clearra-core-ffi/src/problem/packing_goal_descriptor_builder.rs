use super::{
    packing_board_descriptor_builder::low_mask, CBoardDescriptor, FfiProblemError, C_COUNT_ALL,
    C_COUNT_FIRST_SOLUTION, C_COUNT_UNIQUE, C_GOAL_CLEAR_TO_EMPTY, C_OBJECTIVE_ALL,
    C_OBJECTIVE_MIN_COVER, C_OBJECTIVE_UNIQUE,
};
use clearra_core_domain::objective::objective_kind::ObjectiveKind;
use clearra_pc_graph::request::{PcCompletionGoal, PcCountPolicy};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PackingGoalMasks {
    pub(crate) goal_region_mask: u64,
    pub(crate) required_fill_mask: u64,
    pub(crate) forbidden_mask: u64,
}

pub(crate) fn packing_goal_masks(
    board: &CBoardDescriptor,
) -> Result<PackingGoalMasks, FfiProblemError> {
    // Use the admitted Board64 extent and occupancy; do not independently
    // lower the original query for the required-fill mask.
    let goal_cells = board.cell_count as usize;
    if goal_cells == 0 || goal_cells > 64 {
        return Err(FfiProblemError::BudgetTooLarge {
            field: "goal_region_cells",
            value: goal_cells,
        });
    }

    let goal_region_mask = low_mask(goal_cells);
    Ok(PackingGoalMasks {
        goal_region_mask,
        required_fill_mask: goal_region_mask & !board.initial_mask,
        forbidden_mask: 0,
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
        // The C packing layer only consumes the geometry uniqueness contract.
        // BuildUp bypass remains owned by the Rust/WASM executor.
        ObjectiveKind::Tiling => C_OBJECTIVE_UNIQUE,
    }
}
