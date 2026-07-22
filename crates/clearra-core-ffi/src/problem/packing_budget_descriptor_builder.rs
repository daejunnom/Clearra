use clearra_problem::SearchProblem;

use super::{packing_problem_builder_error::to_u32, CProblemBudget, FfiProblemError};

pub(crate) fn budget_descriptor(
    problem: &SearchProblem,
) -> Result<CProblemBudget, FfiProblemError> {
    let budget = problem.budget();
    let backend = problem.backend_request();
    let max_memory_mib = match backend.max_memory_mib() {
        Some(value) => Some(
            u32::try_from(value).map_err(|_| FfiProblemError::MemoryBudgetTooLarge { value })?,
        ),
        None => None,
    };

    Ok(CProblemBudget {
        max_nodes: budget.max_nodes() as u64,
        max_seconds: to_u32("max_seconds", budget.max_seconds() as usize)?,
        max_results: to_u32("max_results", backend.max_candidates())?,
        max_patterns: to_u32("max_patterns", budget.max_patterns())?,
        max_frontier_states: to_u32("max_frontier_states", backend.max_frontier_states())?,
        max_memory_mib: max_memory_mib.unwrap_or(0),
        has_max_memory_mib: max_memory_mib.is_some() as u8,
        reserved: [0; 7],
    })
}
