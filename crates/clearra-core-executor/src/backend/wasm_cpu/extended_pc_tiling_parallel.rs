//! Native exact Tiling runner. The request's one compute lease covers N-1 pool
//! jobs plus the caller. All jobs are joined before terminal publication.
use std::{
    panic::{catch_unwind, AssertUnwindSafe},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Mutex,
    },
};

use crate::{cpu_worker_pool, resource::ExecutionMemoryBound};

use super::super::extended_geometry::ExtendedParallelGeometryBranch;
use super::{
    size_of, Arc, CoreExecutionResult, ExactSearchAdvance, ExecutionControl,
    ExtendedGeometryAdvance, ExtendedInverseCatalog, ExtendedPcTilingSession, ExtendedTilingKey,
    WasmExactSearchError,
};

struct WorkerResult {
    keys: Vec<String>,
    key_heap_bytes: u128,
    candidate_count: usize,
    private_peak_bytes: u128,
}

impl WorkerResult {
    fn retained_bytes(&self) -> u128 {
        self.keys.capacity() as u128 * size_of::<String>() as u128 + self.key_heap_bytes
    }
}

impl ExtendedPcTilingSession {
    pub fn execute_parallel_if_worthwhile(
        &mut self,
        requested_workers: usize,
        control: &ExecutionControl,
    ) -> Result<Option<CoreExecutionResult>, WasmExactSearchError> {
        if requested_workers <= 1 {
            return Ok(None);
        }
        if self.finished || self.geometry.candidate_count() != 0 {
            return Err(WasmExactSearchError::InvalidProblem(
                "extended_parallel_tiling_already_consumed",
            ));
        }
        let mut preparation_steps = 0_usize;
        while self.geometry.is_compiling() {
            if control.is_cancelled() {
                return Err(WasmExactSearchError::Cancelled);
            }
            self.ensure_memory(0)?;
            let max_nodes = self.problem.backend_request().max_nodes();
            if max_nodes != 0 && self.geometry.expanded_nodes() >= max_nodes {
                return Err(WasmExactSearchError::InvalidProblem("node_budget_exceeded"));
            }
            match self.geometry.advance(&self.catalog) {
                ExtendedGeometryAdvance::Pending => {}
                ExtendedGeometryAdvance::ResourceIncomplete(reason) => {
                    return Err(WasmExactSearchError::InvalidProblem(reason));
                }
                _ => {
                    return Err(WasmExactSearchError::InvalidProblem(
                        "extended_parallel_tiling_prepare_invalid",
                    ))
                }
            }
            preparation_steps = preparation_steps.wrapping_add(1);
            if preparation_steps & 1023 == 0 {
                control.report_progress("geometry", self.geometry.expanded_nodes() as u64, None);
            }
        }
        if control.is_cancelled() {
            return Err(WasmExactSearchError::Cancelled);
        }
        control.report_progress("geometry", self.geometry.expanded_nodes() as u64, None);
        let live = self.checked_retained_bytes().ok_or_else(projection_error)?;
        let Some(mut plan) = self.geometry.take_parallel_plan(
            requested_workers.saturating_mul(4),
            self.execution_admission.memory_bound(),
            live,
        )?
        else {
            return Ok(None);
        };
        let max_candidates = self.problem.backend_request().max_candidates();
        if max_candidates != 0 && plan.candidate_count > max_candidates as u128 {
            return Err(WasmExactSearchError::InvalidProblem(
                "candidate_budget_exceeded",
            ));
        }
        // Clamp only to real disjoint branches, never to a piece-count or
        // arbitrary topology heuristic. The request still records its width.
        let workers = requested_workers.min(plan.branches.len()).max(1);
        let shared = plan.shared_retained_bytes() as u128;
        let branch_bytes = plan.branch_retained_bytes() as u128;
        let fixed = self
            .checked_retained_bytes()
            .ok_or_else(projection_error)?
            .checked_add(shared)
            .and_then(|bytes| bytes.checked_add(branch_bytes))
            .and_then(|bytes| {
                bytes.checked_add(
                    (workers as u128)
                        * (2048
                            + size_of::<WorkerResult>()
                            + size_of::<ExtendedParallelGeometryBranch>())
                            as u128,
                )
            })
            .ok_or_else(projection_error)?;
        self.execution_admission
            .ensure_memory_bound(fixed, 0)
            .map_err(WasmExactSearchError::resource_admission)?;
        let credit = (self.execution_admission.memory_cap_bytes() - fixed) / workers as u128;
        let worker_bound = self
            .execution_admission
            .memory_bound()
            .with_cap(credit)
            .map_err(WasmExactSearchError::resource_admission)?;

        // Large branches are offered first. Canonical ordinal intervals remain
        // attached to every branch; the public store sorts full identities.
        plan.branches
            .sort_unstable_by_key(|branch| (branch.candidate_count, branch.first_ordinal));
        let queue = Arc::new(Mutex::new(core::mem::take(&mut plan.branches)));
        let abort = Arc::new(AtomicBool::new(false));
        let (sender, receiver) = mpsc::channel();
        if workers > 1 {
            let pool = if self.problem.backend_policy().cpu_warmup() {
                cpu_worker_pool::prewarm_cpu_workers(workers)
            } else {
                cpu_worker_pool::ensure_cpu_workers(workers)
            }
            .map_err(|_| pool_error())?;
            if pool.total_workers() != workers {
                return Err(pool_error());
            }
        }
        let mut submitted = 0_usize;
        let mut first_error = None;
        for _ in 0..workers.saturating_sub(1) {
            let queue = Arc::clone(&queue);
            let catalog = Arc::clone(&self.catalog);
            let abort_worker = Arc::clone(&abort);
            let worker_control = control.clone();
            let sender = sender.clone();
            let queued = cpu_worker_pool::submit_cpu_job(move || {
                let result = catch_unwind(AssertUnwindSafe(|| {
                    run_worker(
                        &queue,
                        &catalog,
                        &abort_worker,
                        &worker_control,
                        worker_bound,
                    )
                }))
                .map_err(|_| {
                    WasmExactSearchError::InvalidProblem("extended_parallel_tiling_worker_panicked")
                })
                .and_then(|result| result);
                if result.is_err() {
                    abort_worker.store(true, Ordering::Release);
                }
                let _ = sender.send(result);
            });
            if queued.is_err() {
                abort.store(true, Ordering::Release);
                first_error = Some(pool_error());
                break;
            }
            submitted += 1;
        }
        drop(sender);
        let caller = catch_unwind(AssertUnwindSafe(|| {
            run_worker(&queue, &self.catalog, &abort, control, worker_bound)
        }))
        .map_err(|_| {
            WasmExactSearchError::InvalidProblem("extended_parallel_tiling_worker_panicked")
        })
        .and_then(|result| result);
        if caller.is_err() {
            abort.store(true, Ordering::Release);
        }
        let mut results = Vec::with_capacity(workers);
        results.push(caller);
        // Even submit/panic/cancellation errors must drain every owned job.
        // A returned error must not release the compute lease under a live job.
        for _ in 0..submitted {
            results.push(receiver.recv().unwrap_or_else(|_| Err(pool_error())));
        }
        let mut successful = Vec::with_capacity(workers);
        for result in results {
            match result {
                Ok(result) => successful.push(result),
                Err(error) => {
                    if first_error.is_none()
                        || matches!(first_error, Some(WasmExactSearchError::Cancelled))
                    {
                        first_error = Some(error);
                    }
                }
            }
        }
        if let Some(error) = first_error {
            return Err(error);
        }
        if control.is_cancelled() {
            return Err(WasmExactSearchError::Cancelled);
        }
        let count = successful
            .iter()
            .try_fold(0_usize, |total, worker| {
                total.checked_add(worker.candidate_count)
            })
            .ok_or_else(projection_error)?;
        if count as u128 != plan.candidate_count {
            return Err(WasmExactSearchError::InvalidProblem(
                "extended_parallel_tiling_count_mismatch",
            ));
        }
        let private_bytes = successful
            .iter()
            .map(WorkerResult::retained_bytes)
            .sum::<u128>();
        let peak = fixed
            + successful
                .iter()
                .map(|worker| worker.private_peak_bytes)
                .sum::<u128>();
        self.parallel_peak_bytes = self.parallel_peak_bytes.max(peak);
        let growth = count as u128 * size_of::<String>() as u128;
        self.execution_admission
            .ensure_memory_bound(fixed + private_bytes, growth)
            .map_err(WasmExactSearchError::resource_admission)?;
        self.keys.try_reserve_exact(count).map_err(|_| {
            WasmExactSearchError::InvalidProblem("extended_pc_tiling_storage_unavailable")
        })?;
        for worker in &mut successful {
            self.keys.append(&mut worker.keys);
        }
        self.workers_used = workers;
        self.geometry.complete_parallel_enumeration(count)?;
        drop(successful);
        drop(queue);
        drop(plan);
        self.ensure_memory(0)?;
        match self.finish()? {
            ExactSearchAdvance::Completed(result) => Ok(Some(result)),
            _ => Err(WasmExactSearchError::InvalidProblem(
                "extended_parallel_tiling_terminal_invalid",
            )),
        }
    }
}

fn run_worker(
    queue: &Mutex<Vec<ExtendedParallelGeometryBranch>>,
    catalog: &ExtendedInverseCatalog,
    abort: &AtomicBool,
    control: &ExecutionControl,
    bound: ExecutionMemoryBound,
) -> Result<WorkerResult, WasmExactSearchError> {
    let mut result = WorkerResult {
        keys: Vec::new(),
        key_heap_bytes: 0,
        candidate_count: 0,
        private_peak_bytes: 0,
    };
    loop {
        if control.is_cancelled() || abort.load(Ordering::Acquire) {
            return Err(WasmExactSearchError::Cancelled);
        }
        let Some(mut branch) = queue.lock().map_err(|_| pool_error())?.pop() else {
            break;
        };
        let mut branch_count = 0_u128;
        loop {
            if control.is_cancelled() || abort.load(Ordering::Acquire) {
                return Err(WasmExactSearchError::Cancelled);
            }
            let owned = result.retained_bytes();
            let Some(candidate) = branch.next_candidate(catalog, bound, owned)? else {
                break;
            };
            let retained = owned + branch.private_retained_bytes() as u128;
            let piece_count = candidate.row_ids().len() as u128;
            let vector_growth = if result.keys.len() == result.keys.capacity() {
                result.keys.len().saturating_add(1).saturating_mul(2).max(4) as u128
                    * size_of::<String>() as u128
            } else {
                0
            };
            let future = vector_growth + 128 + piece_count * (64 + 67);
            bound
                .ensure(retained, future)
                .map_err(WasmExactSearchError::resource_admission)?;
            result.private_peak_bytes = result.private_peak_bytes.max(retained + future);
            result.keys.try_reserve(1).map_err(|_| {
                WasmExactSearchError::InvalidProblem("extended_pc_tiling_storage_unavailable")
            })?;
            let tiling = ExtendedTilingKey::from_candidate(catalog, &candidate);
            let key = tiling.canonical_key(catalog.initial_board(), catalog.height());
            result.key_heap_bytes = result
                .key_heap_bytes
                .checked_add(key.capacity() as u128)
                .ok_or_else(projection_error)?;
            result.keys.push(key);
            branch_count += 1;
            result.candidate_count = result
                .candidate_count
                .checked_add(1)
                .ok_or_else(projection_error)?;
            bound
                .ensure(
                    result.retained_bytes(),
                    branch.private_retained_bytes() as u128,
                )
                .map_err(WasmExactSearchError::resource_admission)?;
        }
        if branch_count != branch.candidate_count {
            return Err(WasmExactSearchError::InvalidProblem(
                "extended_parallel_tiling_branch_count_mismatch",
            ));
        }
    }
    Ok(result)
}

fn projection_error() -> WasmExactSearchError {
    WasmExactSearchError::InvalidProblem("extended_pc_tiling_memory_projection_unavailable")
}

fn pool_error() -> WasmExactSearchError {
    WasmExactSearchError::InvalidProblem("extended_parallel_tiling_worker_pool_unavailable")
}
