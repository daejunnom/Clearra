//! Parallel Finesse terminal work over the admitted immutable source family.

use std::sync::Mutex;

use clearra_core_domain::parallel_work::{try_for_each_chunk, ParallelWorkError};

use super::*;

pub(super) fn work_error(error: ParallelWorkError<WasmExactSearchError>) -> WasmExactSearchError {
    match error {
        ParallelWorkError::Worker(error) => error,
        ParallelWorkError::Cancelled => WasmExactSearchError::Cancelled,
        ParallelWorkError::ThreadSpawnFailed => {
            WasmExactSearchError::InvalidProblem("wasm_finesse_compute_worker_spawn_failed")
        }
        ParallelWorkError::WorkerPanicked => {
            WasmExactSearchError::InvalidProblem("wasm_finesse_compute_worker_panicked")
        }
    }
}

pub(super) fn product_error(
    error: ParallelWorkError<GeometryLanguageError>,
    fallback: &'static str,
) -> WasmExactSearchError {
    work_error(match error {
        ParallelWorkError::Worker(error) => {
            ParallelWorkError::Worker(map_finesse_product_error(error, fallback))
        }
        ParallelWorkError::Cancelled => ParallelWorkError::Cancelled,
        ParallelWorkError::ThreadSpawnFailed => ParallelWorkError::ThreadSpawnFailed,
        ParallelWorkError::WorkerPanicked => ParallelWorkError::WorkerPanicked,
    })
}

#[allow(clippy::too_many_arguments)]
pub(super) fn solution_costs(
    policy: &'static str,
    languages: &[(String, CostedGeometryLanguage)],
    classes: &QueueClassSet,
    fixed_queue: bool,
    initial_hold: Option<PieceKind>,
    hold_enabled: bool,
    terminal_hold_release: bool,
    spawn_profile: SpawnProfile,
    control: &ExecutionControl,
) -> Result<Vec<(String, QueueCostTable)>, WasmExactSearchError> {
    let mut slots = Vec::new();
    slots.try_reserve_exact(languages.len()).map_err(|_| {
        WasmExactSearchError::InvalidProblem("wasm_finesse_solution_cost_storage_unavailable")
    })?;
    slots.extend((0..languages.len()).map(|_| Mutex::new(None)));
    try_for_each_chunk(
        control,
        languages.len(),
        1,
        || Ok(()),
        |_, range| {
            for index in range {
                let evaluator = QueueClassProductEvaluator::new(&languages[index].1)
                    .with_spawn_profile(spawn_profile)
                    .with_hold_enabled(hold_enabled)
                    .with_terminal_hold_release_enabled(terminal_hold_release);
                let costs = finesse_policy_costs(
                    &evaluator,
                    policy,
                    classes,
                    fixed_queue,
                    initial_hold,
                    control,
                    "wasm_finesse_policy_evaluation_failed",
                )?;
                *slots[index]
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(costs);
            }
            Ok(())
        },
    )
    .map_err(work_error)?;
    let mut solutions = Vec::new();
    solutions.try_reserve_exact(languages.len()).map_err(|_| {
        WasmExactSearchError::InvalidProblem("wasm_finesse_solution_cost_storage_unavailable")
    })?;
    for ((key, _), slot) in languages.iter().zip(slots) {
        let costs = slot
            .into_inner()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_finesse_solution_cost_missing",
            ))?;
        solutions.push((key.clone(), costs));
    }
    Ok(solutions)
}

pub(super) fn oracle_union_costs(
    solutions: &[(String, QueueCostTable)],
    class_count: usize,
    control: &ExecutionControl,
) -> Result<QueueCostTable, WasmExactSearchError> {
    let costs = Mutex::new(QueueCostTable::unreachable(class_count).map_err(|error| {
        map_finesse_product_error(error, "wasm_finesse_overall_cost_storage_unavailable")
    })?);
    try_for_each_chunk(
        control,
        class_count,
        64,
        || Ok(()),
        |_, range| {
            let mut local = [None; 64];
            for (_, solution_costs) in solutions {
                for index in range.clone() {
                    if let Some(cost) = solution_costs.get(index).flatten() {
                        let slot = &mut local[index - range.start];
                        *slot = Some(slot.map_or(cost, |previous: u32| previous.min(cost)));
                    }
                }
            }
            let mut costs = costs
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            for index in range.clone() {
                if let Some(cost) = local[index - range.start] {
                    costs.set_min(index, cost).map_err(|error| {
                        map_finesse_product_error(
                            error,
                            "wasm_finesse_overall_policy_evaluation_failed",
                        )
                    })?;
                }
            }
            Ok(())
        },
    )
    .map_err(work_error)?;
    Ok(costs
        .into_inner()
        .unwrap_or_else(std::sync::PoisonError::into_inner))
}

pub(super) fn summarize_solutions(
    solutions: &[(String, QueueCostTable)],
    classes: &QueueClassSet,
    control: &ExecutionControl,
) -> Result<
    (
        Vec<FinesseSolutionAverage>,
        Option<FinesseRepresentativeSelection>,
    ),
    WasmExactSearchError,
> {
    let mut slots = Vec::new();
    slots.try_reserve_exact(solutions.len()).map_err(|_| {
        WasmExactSearchError::InvalidProblem("wasm_finesse_average_storage_unavailable")
    })?;
    slots.extend((0..solutions.len()).map(|_| Mutex::new(None)));
    try_for_each_chunk(
        control,
        solutions.len(),
        1,
        || Ok(()),
        |_, range| {
            for solution_index in range {
                let (key, costs) = &solutions[solution_index];
                let aggregation = aggregate_unique_queue_costs(classes, costs).map_err(|_| {
                    WasmExactSearchError::InvalidProblem("wasm_finesse_solution_aggregation_failed")
                })?;
                let mut representative = None;
                for class_index in 0..classes.classes().len() {
                    if class_index % 64 == 0 {
                        ensure_finesse_not_cancelled(control)?;
                    }
                    let Some(expected_cost) = costs.get(class_index).flatten() else {
                        continue;
                    };
                    let candidate = FinesseRepresentativeSelection {
                        solution_index,
                        class_index,
                        expected_cost,
                    };
                    if representative
                        .is_none_or(|previous| selection_key(candidate) < selection_key(previous))
                    {
                        representative = Some(candidate);
                    }
                }
                let average = FinesseSolutionAverage::new(
                    key,
                    finesse_average_text(aggregation.conditional_mean_inputs),
                    aggregation.complete,
                );
                *slots[solution_index]
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) =
                    Some((average, representative));
            }
            Ok(())
        },
    )
    .map_err(work_error)?;
    let mut averages = Vec::new();
    averages.try_reserve_exact(solutions.len()).map_err(|_| {
        WasmExactSearchError::InvalidProblem("wasm_finesse_average_storage_unavailable")
    })?;
    let mut representative = None;
    for slot in slots {
        let (average, candidate) = slot
            .into_inner()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_finesse_solution_summary_missing",
            ))?;
        averages.push(average);
        if let Some(candidate) = candidate {
            if representative
                .is_none_or(|previous| selection_key(candidate) < selection_key(previous))
            {
                representative = Some(candidate);
            }
        }
    }
    Ok((averages, representative))
}

fn selection_key(selection: FinesseRepresentativeSelection) -> (u32, usize, usize) {
    (
        selection.expected_cost,
        selection.solution_index,
        selection.class_index,
    )
}
