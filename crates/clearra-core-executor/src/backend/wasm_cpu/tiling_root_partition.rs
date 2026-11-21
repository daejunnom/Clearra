//! Bounded exact-cover partitions of a skinny distributed multiset frontier.
use clearra_problem::SearchProblem;

use super::super::result::WasmExactSearchSession;
use super::super::{catalog::GeometryCatalog, geometry::GeometrySearch, piece_index};

const MIN_SPLIT_PIECES: u32 = 7;
const ROOT_TASKS_PER_WORKER: usize = 4;

#[derive(Clone, Copy)]
pub(super) struct PlannedRoot {
    pub(super) family_index: u32,
    pub(super) seed_row: Option<u32>,
}

/// Request-wide caps stay on their existing unsplit paths. In particular a
/// new browser task must never reset a shared candidate or node allowance.
pub(super) fn subdivision_width(problem: &SearchProblem) -> usize {
    let backend = problem.backend_request();
    if problem.budget().max_nodes() != 0
        || backend.max_candidates() != 0
        || backend.max_frontier_states() != 0
        || backend.max_memory_mib().is_some()
    {
        return 1;
    }
    problem.backend_policy().workers()
}

pub(super) fn task_bound(roots: usize, workers: usize) -> Option<usize> {
    if workers > 1 {
        Some(roots.max(workers.checked_mul(ROOT_TASKS_PER_WORKER)?))
    } else {
        Some(roots)
    }
}

pub(super) fn partition_roots(
    session: &WasmExactSearchSession,
    root_order: &[u32],
    width: usize,
    preserve_natural_order: bool,
) -> Result<Vec<PlannedRoot>, &'static str> {
    let catalog = session.catalog();
    let should_split = width > root_order.len()
        && width > 1
        && catalog.required_cells().count_ones() / 4 >= MIN_SPLIT_PIECES;
    let bound = if should_split {
        task_bound(root_order.len(), width).ok_or("wasm_root_task_count_overflow")?
    } else {
        root_order.len()
    };
    let mut tasks = Vec::new();
    tasks
        .try_reserve_exact(bound)
        .map_err(|_| "wasm_root_task_storage_unavailable")?;
    let targets = session
        .geometry_targets()
        .ok_or("wasm_root_task_targets_missing")?;
    for (index, family_index) in root_order.iter().copied().enumerate() {
        let target = targets
            .get(family_index as usize)
            .ok_or("wasm_root_task_target_invalid")?;
        let remaining_roots = root_order.len() - index;
        let slots = (bound - tasks.len()) / remaining_roots;
        let branches = if !should_split || slots < 2 {
            None
        } else if preserve_natural_order {
            GeometrySearch::distributed_first_rows(&catalog, target)
        } else {
            subdivision_rows(&catalog, target.key.counts(), slots)
        };
        if let Some(branches) = branches.filter(|rows| (2..=slots).contains(&rows.len())) {
            for row_id in branches {
                tasks.push(PlannedRoot {
                    family_index,
                    seed_row: Some(row_id),
                });
            }
        } else {
            tasks.push(PlannedRoot {
                family_index,
                seed_row: None,
            });
        }
    }
    Ok(tasks)
}

fn subdivision_rows(
    catalog: &GeometryCatalog,
    piece_counts: [u8; 7],
    limit: usize,
) -> Option<Vec<u32>> {
    let required = catalog.required_cells();
    let feasible = |row_id| {
        let row = catalog.skeleton(row_id);
        piece_counts[piece_index(row.piece)] > 0 && row.cells & !required == 0
    };
    // Any empty cell is an exact disjoint boundary: every successful cover
    // contains exactly one catalog row covering it. Select enough useful
    // branches to occupy the pool without generating an unbounded frontier.
    let mut pending = required;
    let mut best = None;
    while pending != 0 {
        let cell = pending.trailing_zeros() as u8;
        pending &= pending - 1;
        let support = catalog.support(cell);
        let count = support
            .iter()
            .copied()
            .filter(|&row| feasible(row))
            .take(limit.saturating_add(1))
            .count();
        if (2..=limit).contains(&count) && best.is_none_or(|(_, prior)| count > prior) {
            best = Some((support, count));
        }
    }
    best.map(|(rows, _)| rows.iter().copied().filter(|&row| feasible(row)).collect())
}
