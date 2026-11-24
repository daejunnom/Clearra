//! The source family's rich execution graphs are independent until their
//! canonical IDs are assembled. Share the actual problem/catalog and admit
//! each in-flight graph against one ledger rather than multiplying a full
//! family bound by N or rebuilding every graph on the coordinator.
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Mutex,
};

use clearra_core_domain::parallel_work::{try_for_each_chunk, ParallelWorkError};
use clearra_replay::ExactScoringExecutionGraph;

use crate::resource::{ExecutionMemoryBound, ExecutionMemoryOwner, SharedExecutionMemoryBudget};

use super::super::buildup::{exact_scoring_execution_graph_for_completion, BuildCompletion};
use super::{
    exact_scoring_execution_graph_memory_projection, BuildUpWorkspace, ExecutionControl,
    GeometryCatalog, SearchProblem, StandardBoard64TilingIdentity, WasmExactSearchError,
    WasmExactSearchSession,
};

struct GraphWorker {
    workspace: BuildUpWorkspace,
    owner: ExecutionMemoryOwner,
    retained_graphs: u128,
}

impl WasmExactSearchSession {
    pub(super) fn prepare_scoring_graphs_parallel(
        &mut self,
        identities: &[StandardBoard64TilingIdentity],
        graphs: &mut Vec<ExactScoringExecutionGraph>,
        identity_bytes: u128,
        graph_outer_bytes: u128,
    ) -> Result<(bool, u128), WasmExactSearchError> {
        let base = self
            .checked_allocation_guard_retained_bytes()
            .and_then(|bytes| bytes.checked_add(identity_bytes))
            .and_then(|bytes| bytes.checked_add(graph_outer_bytes))
            .ok_or_else(|| self.scoring_memory_projection_overflow())?;
        let control = self
            .execution_control
            .clone()
            .with_parallelism_if_unbound(self.problem.backend_policy().workers());
        let (complete, retained, peak) = fill_graphs_parallel(
            &self.problem,
            &self.catalog,
            identities,
            graphs,
            BuildCompletion::ClearToEmpty,
            &control,
            self._execution_admission.memory_bound(),
            base,
        )?;
        self.peak_cpu_bytes = self
            .peak_cpu_bytes
            .max(usize::try_from(peak).unwrap_or(usize::MAX));
        Ok((complete, retained))
    }
}

#[allow(clippy::too_many_arguments)]
pub(in crate::backend::wasm_cpu) fn fill_graphs_parallel(
    problem: &SearchProblem,
    catalog: &GeometryCatalog,
    identities: &[StandardBoard64TilingIdentity],
    graphs: &mut Vec<ExactScoringExecutionGraph>,
    completion: BuildCompletion,
    control: &ExecutionControl,
    memory_bound: ExecutionMemoryBound,
    base_retained_bytes: u128,
) -> Result<(bool, u128, u128), WasmExactSearchError> {
    let workers = control.parallelism();
    let slots_bytes = (identities.len() as u128)
        .checked_mul(core::mem::size_of::<Mutex<Option<ExactScoringExecutionGraph>>>() as u128)
        .ok_or_else(contract_error)?;
    let worker_bytes = (workers as u128)
        .checked_mul((core::mem::size_of::<GraphWorker>() + 2048) as u128)
        .ok_or_else(contract_error)?;
    let fixed = base_retained_bytes
        .checked_add(slots_bytes)
        .and_then(|bytes| bytes.checked_add(worker_bytes))
        .ok_or_else(contract_error)?;
    let memory = SharedExecutionMemoryBudget::new(memory_bound, fixed, workers)
        .map_err(WasmExactSearchError::resource_admission)?;
    let mut slots = Vec::new();
    slots
        .try_reserve_exact(identities.len())
        .map_err(|_| storage_error())?;
    slots.resize_with(identities.len(), || Mutex::new(None));
    let next_worker = AtomicUsize::new(0);
    try_for_each_chunk(
        &control,
        identities.len(),
        1,
        || {
            let index = next_worker.fetch_add(1, Ordering::Relaxed);
            if index >= workers {
                return Err(contract_error());
            }
            Ok(GraphWorker {
                workspace: BuildUpWorkspace::default(),
                owner: memory.owner(index),
                retained_graphs: 0,
            })
        },
        |worker, range| {
            for index in range {
                if control.is_cancelled() {
                    return Err(WasmExactSearchError::Cancelled);
                }
                let identity = identities[index];
                let projection =
                    exact_scoring_execution_graph_memory_projection(problem, catalog, identity)?;
                let future = worker
                    .retained_graphs
                    .checked_add(worker.workspace.retained_bytes() as u128)
                    .and_then(|bytes| bytes.checked_add(projection.peak_additional_bytes))
                    .ok_or_else(contract_error)?;
                worker
                    .owner
                    .ensure(future)
                    .map_err(WasmExactSearchError::resource_admission)?;
                let candidate_id = u64::try_from(index)
                    .ok()
                    .and_then(|value| value.checked_add(1))
                    .ok_or_else(contract_error)?;
                let graph = exact_scoring_execution_graph_for_completion(
                    problem,
                    catalog,
                    identity,
                    candidate_id,
                    &mut worker.workspace,
                    completion,
                )?;
                if let Some(graph) = graph.as_ref() {
                    let retained = graph
                        .checked_nested_retained_bytes()
                        .ok_or_else(contract_error)?;
                    if retained > projection.retained_graph_nested_bytes {
                        return Err(contract_error());
                    }
                    worker.retained_graphs = worker
                        .retained_graphs
                        .checked_add(retained)
                        .ok_or_else(contract_error)?;
                }
                worker
                    .owner
                    .ensure(
                        worker
                            .retained_graphs
                            .checked_add(worker.workspace.retained_bytes() as u128)
                            .ok_or_else(contract_error)?,
                    )
                    .map_err(WasmExactSearchError::resource_admission)?;
                *slots[index].lock().map_err(|_| contract_error())? = graph;
            }
            Ok(())
        },
    )
    .map_err(|error| match error {
        ParallelWorkError::Worker(error) => error,
        ParallelWorkError::Cancelled => WasmExactSearchError::Cancelled,
        ParallelWorkError::ThreadSpawnFailed => {
            WasmExactSearchError::InvalidProblem("wasm_scoring_graph_worker_spawn_failed")
        }
        ParallelWorkError::WorkerPanicked => {
            WasmExactSearchError::InvalidProblem("wasm_scoring_graph_worker_panicked")
        }
    })?;
    let peak = memory.admitted_peak_bytes();
    let mut complete = true;
    let mut retained_graphs = 0_u128;
    for slot in slots {
        match slot.into_inner().map_err(|_| contract_error())? {
            Some(graph) => {
                retained_graphs = retained_graphs
                    .checked_add(
                        graph
                            .checked_nested_retained_bytes()
                            .ok_or_else(contract_error)?,
                    )
                    .ok_or_else(contract_error)?;
                graphs.push(graph);
            }
            None => complete = false,
        }
    }
    if control.is_cancelled() {
        return Err(WasmExactSearchError::Cancelled);
    }
    Ok((complete, retained_graphs, peak))
}

fn storage_error() -> WasmExactSearchError {
    WasmExactSearchError::InvalidProblem("wasm_scoring_graph_parallel_storage_unavailable")
}

fn contract_error() -> WasmExactSearchError {
    WasmExactSearchError::InvalidProblem("wasm_scoring_graph_parallel_contract_invalid")
}
