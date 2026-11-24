//! Original-order compact PC windows with one aggregate resource authority.
//! The geometry owner applies the request's finite limits once; independent
//! verification keeps its source ordinals and existing language producers.

use super::*;
use crate::resource::SharedExecutionMemoryBudget;
use clearra_core_domain::parallel_work::{try_for_each_chunk, ParallelWorkError};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Mutex,
};

#[derive(Default)]
struct Worker {
    buildup: BuildUpWorkspace,
    evaluator: CoverageProductEvaluator,
    candidates: usize,
    output_bytes: u128,
}

impl Worker {
    fn retained_bytes(&self) -> Option<u128> {
        (self.buildup.retained_bytes() as u128).checked_add(self.evaluator.retained_bytes() as u128)
    }
}

fn output_bytes(result: &CandidateBuildResult) -> Option<u128> {
    (result
        .covered_patterns
        .as_ref()
        .map_or(0, PatternBitSet::retained_bytes) as u128)
        .checked_add(
            (result.representative_path.capacity() as u128)
                .checked_mul(core::mem::size_of::<CorePathStep>() as u128)?,
        )
}

impl WasmExactSearchSession {
    pub(super) fn execute_candidate_windows(
        &mut self,
        requested_workers: usize,
        control: &ExecutionControl,
    ) -> Result<Option<CoreExecutionResult>, WasmExactSearchError> {
        let control = control
            .clone()
            .with_parallelism_if_unbound(requested_workers);
        let width = control.parallelism();
        if width <= 1 {
            return Ok(None);
        }
        if self.finished || self.packing_candidate_count != 0 || control.partition().count() != 1 {
            return Err(contract_error());
        }
        self.execution_control = control.clone();
        self.geometry.retain_pattern_memberships_for_terminal();
        let result = self.run_candidate_windows(width, &control);
        self.parallel_coexisting_bytes = 0;
        result.map(Some)
    }

    fn run_candidate_windows(
        &mut self,
        width: usize,
        control: &ExecutionControl,
    ) -> Result<CoreExecutionResult, WasmExactSearchError> {
        let window = width.checked_mul(16).ok_or_else(contract_error)?;
        let fixed = (width as u128)
            .checked_mul((core::mem::size_of::<Mutex<Worker>>() + 2048) as u128)
            .and_then(|bytes| {
                bytes.checked_add((window as u128).checked_mul(
                    (core::mem::size_of::<(GeometryCandidate, u64)>()
                        + core::mem::size_of::<Mutex<Option<CandidateBuildResult>>>())
                        as u128,
                )?)
            })
            .ok_or_else(contract_error)?;
        self.ensure_session_memory_bound(fixed)?;
        let workers: Vec<_> = (0..width).map(|_| Mutex::new(Worker::default())).collect();
        let mut candidates = Vec::new();
        candidates
            .try_reserve_exact(window)
            .map_err(|_| contract_error())?;
        let mut finished = false;
        while !finished {
            candidates.clear();
            self.parallel_coexisting_bytes = fixed
                .checked_add(worker_bytes(&workers)?)
                .ok_or_else(contract_error)?;
            while candidates.len() < window {
                if control.is_cancelled() {
                    return Err(WasmExactSearchError::Cancelled);
                }
                if self.problem.budget().max_nodes() != 0
                    && self.geometry.expanded_nodes() >= self.problem.budget().max_nodes()
                {
                    self.mark_truncated("frontier_budget_exceeded");
                    finished = true;
                    break;
                }
                let frontier_limit = self.problem.backend_request().max_frontier_states();
                if frontier_limit != 0 && self.geometry.peak_frontier() > frontier_limit {
                    self.mark_truncated("frontier_budget_exceeded");
                    finished = true;
                    break;
                }
                if self.problem.backend_request().max_candidates() != 0
                    && self.packing_candidate_count
                        >= self.problem.backend_request().max_candidates()
                {
                    self.mark_truncated("candidate_budget_exceeded");
                    finished = true;
                    break;
                }
                let advance = if let Some(limit) = self.checked_geometry_retained_limit(0)? {
                    self.geometry
                        .advance_with_retained_limit(&self.catalog, limit)
                } else {
                    self.geometry.advance(&self.catalog)
                };
                self.ensure_session_memory_bound(0)?;
                match advance {
                    GeometryAdvance::Pending => {}
                    GeometryAdvance::Complete => {
                        finished = true;
                        break;
                    }
                    GeometryAdvance::ResourceIncomplete(reason) => {
                        self.mark_truncated(reason);
                        finished = true;
                        break;
                    }
                    GeometryAdvance::Candidate(candidate) => {
                        if !self.problem.allows_solution_identity(&candidate.identity) {
                            continue;
                        }
                        let ordinal = self.packing_candidate_count as u64;
                        self.packing_candidate_count = self
                            .packing_candidate_count
                            .checked_add(1)
                            .ok_or_else(contract_error)?;
                        self.packing_candidate_digest = if self.order_independent_candidate_digest {
                            mix_order_independent_candidate_digest(
                                self.packing_candidate_digest,
                                candidate.identity.bucket_hash(),
                            )
                        } else {
                            mix_digest(
                                self.packing_candidate_digest,
                                candidate.identity.bucket_hash(),
                            )
                        };
                        if candidate.physical_prefix_impossible() {
                            self.realization_feasibility_rejected_candidates += 1;
                        } else {
                            candidates.push((candidate, ordinal));
                        }
                    }
                }
            }
            if candidates.is_empty() {
                continue;
            }
            self.parallel_coexisting_bytes = fixed;
            let base = self
                .checked_allocation_guard_retained_bytes()
                .ok_or_else(contract_error)?;
            let ledger = SharedExecutionMemoryBudget::new(
                self._execution_admission.memory_bound(),
                base,
                width,
            )
            .map_err(WasmExactSearchError::resource_admission)?;
            for (index, worker) in workers.iter().enumerate() {
                let mut worker = worker.lock().map_err(|_| contract_error())?;
                worker.output_bytes = 0;
                ledger
                    .owner(index)
                    .ensure(worker.retained_bytes().ok_or_else(contract_error)?)
                    .map_err(WasmExactSearchError::resource_admission)?;
            }
            let slots: Vec<_> = (0..candidates.len()).map(|_| Mutex::new(None)).collect();
            let next_worker = AtomicUsize::new(0);
            let exact_rows =
                self.solution_coverage.is_some() || self.pc_chance_coverage_evidence_available;
            #[cfg(any(test, feature = "local-search-ab"))]
            let exact_rows = exact_rows || self.solution_coverage_capture.is_some();
            try_for_each_chunk(
                control,
                candidates.len(),
                1,
                || Ok(next_worker.fetch_add(1, Ordering::Relaxed)),
                |worker_index, range| {
                    let mut worker = workers
                        .get(*worker_index)
                        .ok_or_else(contract_error)?
                        .lock()
                        .map_err(|_| contract_error())?;
                    let owner = ledger.owner(*worker_index);
                    for index in range {
                        let candidate = &candidates[index].0;
                        let projection = checked_candidate_verification_peak_upper_bound(
                            &self.problem,
                            &self.catalog,
                            candidate,
                            self.problem.output_policy().retains_representative_trace(),
                        )
                        .ok_or_else(contract_error)?;
                        owner
                            .ensure(
                                worker
                                    .retained_bytes()
                                    .and_then(|bytes| bytes.checked_add(worker.output_bytes))
                                    .and_then(|bytes| bytes.checked_add(projection))
                                    .ok_or_else(contract_error)?,
                            )
                            .map_err(WasmExactSearchError::resource_admission)?;
                        let target = self
                            .geometry
                            .target(candidate.target_index)
                            .ok_or_else(contract_error)?;
                        let witness = CandidateWitnessMode::for_candidate(
                            &self.problem,
                            target,
                            || {
                                worker.buildup.standard_bag_coverage_complete()
                                    || self
                                        .covered_patterns
                                        .is_superset(target.possible_patterns.as_ref())
                                        .unwrap_or(false)
                            },
                            exact_rows,
                            worker.buildup.can_use_first_pattern_witness(),
                        );
                        let Worker {
                            buildup, evaluator, ..
                        } = &mut *worker;
                        let mut result = verify_candidate(
                            &self.problem,
                            &self.catalog,
                            candidate,
                            target,
                            buildup,
                            evaluator,
                            witness,
                            self.problem.output_policy().retains_representative_trace(),
                            0,
                            control,
                        )?;
                        if let Some(root) = result.symbolic_coverage_root.take() {
                            if exact_rows {
                                let bits = worker.buildup.materialize_standard_bag_root(root)?;
                                if let Some(covered) = result.covered_patterns.as_mut() {
                                    covered.union_with(&bits).map_err(|_| contract_error())?;
                                } else {
                                    result.covered_patterns = Some(bits);
                                }
                                result.symbolic_covered_pattern_count = 0;
                            }
                            worker.buildup.merge_standard_bag_coverage(root)?;
                        }
                        if let Some(root) = result.observation_language_root.take() {
                            worker.buildup.merge_observation_language(root)?;
                        }
                        worker.candidates += 1;
                        worker.output_bytes = worker
                            .output_bytes
                            .checked_add(output_bytes(&result).ok_or_else(contract_error)?)
                            .ok_or_else(contract_error)?;
                        owner
                            .ensure(
                                worker
                                    .retained_bytes()
                                    .and_then(|bytes| bytes.checked_add(worker.output_bytes))
                                    .ok_or_else(contract_error)?,
                            )
                            .map_err(WasmExactSearchError::resource_admission)?;
                        *slots[index].lock().map_err(|_| contract_error())? = Some(result);
                    }
                    Ok(())
                },
            )
            .map_err(parallel_error)?;
            self.peak_cpu_bytes = self
                .peak_cpu_bytes
                .max(usize::try_from(ledger.admitted_peak_bytes()).unwrap_or(usize::MAX));
            let private = worker_bytes(&workers)?;
            let mut remaining = workers
                .iter()
                .try_fold(0_u128, |bytes, worker| {
                    bytes.checked_add(worker.lock().ok()?.output_bytes)
                })
                .ok_or_else(contract_error)?;
            for (index, slot) in slots.into_iter().enumerate() {
                let result = slot
                    .into_inner()
                    .map_err(|_| contract_error())?
                    .ok_or_else(contract_error)?;
                remaining = remaining
                    .checked_sub(output_bytes(&result).ok_or_else(contract_error)?)
                    .ok_or_else(contract_error)?;
                self.parallel_coexisting_bytes = fixed
                    .checked_add(private)
                    .and_then(|bytes| bytes.checked_add(remaining))
                    .ok_or_else(contract_error)?;
                let (candidate, ordinal) = candidates[index];
                if let Some(advance) =
                    self.apply_verified_candidate(candidate, ordinal, result, 0)?
                {
                    return match advance {
                        ExactSearchAdvance::Completed(result) => Ok(result),
                        ExactSearchAdvance::Cancelled => Err(WasmExactSearchError::Cancelled),
                        ExactSearchAdvance::Pending => Err(contract_error()),
                    };
                }
            }
        }
        // Each worker retained the same symbolic aggregate used by serial
        // Unique. Materialize that aggregate once, never a dense C×P row set.
        self.materialize_candidate_worker_coverage(&workers, fixed, control)?;
        self.parallel_coexisting_bytes = fixed
            .checked_add(worker_bytes(&workers)?)
            .ok_or_else(contract_error)?;
        self.parallel_active_workers = 0;
        for worker in &workers {
            let worker = worker.lock().map_err(|_| contract_error())?;
            if worker.candidates != 0 {
                self.parallel_active_workers += 1;
                self.parallel_maximum_worker_candidates = self
                    .parallel_maximum_worker_candidates
                    .max(worker.candidates);
                self.parallel_minimum_worker_candidates =
                    if self.parallel_minimum_worker_candidates == 0 {
                        worker.candidates
                    } else {
                        self.parallel_minimum_worker_candidates
                            .min(worker.candidates)
                    };
            }
            let future = self
                .buildup_workspace
                .checked_observation_import_future_bytes(&worker.buildup)
                .ok_or_else(contract_error)?;
            self.ensure_session_memory_bound(future)?;
            self.buildup_workspace
                .import_observation_languages(&worker.buildup)?;
        }
        drop(workers);
        self.parallel_coexisting_bytes = 0;
        self.workers_used = width;
        self.parallel_decision_reason = "parallel-original-candidate-windows";
        match self.complete()? {
            ExactSearchAdvance::Completed(result) => Ok(result),
            ExactSearchAdvance::Cancelled => Err(WasmExactSearchError::Cancelled),
            ExactSearchAdvance::Pending => Err(contract_error()),
        }
    }

    fn materialize_candidate_worker_coverage(
        &mut self,
        workers: &[Mutex<Worker>],
        fixed: u128,
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError> {
        self.parallel_coexisting_bytes = fixed;
        let base = self
            .checked_allocation_guard_retained_bytes()
            .ok_or_else(contract_error)?;
        let ledger = SharedExecutionMemoryBudget::new(
            self._execution_admission.memory_bound(),
            base,
            workers.len(),
        )
        .map_err(WasmExactSearchError::resource_admission)?;
        let pattern_count = self.covered_patterns.pattern_count();
        let future = PatternBitSet::checked_all_projection(pattern_count)
            .ok_or_else(contract_error)?
            .constructor_peak_bytes;
        for (index, worker) in workers.iter().enumerate() {
            ledger
                .owner(index)
                .ensure(
                    worker
                        .lock()
                        .map_err(|_| contract_error())?
                        .retained_bytes()
                        .ok_or_else(contract_error)?,
                )
                .map_err(WasmExactSearchError::resource_admission)?;
        }
        let slots: Vec<_> = (0..workers.len()).map(|_| Mutex::new(None)).collect();
        try_for_each_chunk(
            control,
            workers.len(),
            1,
            || Ok(()),
            |_, range| {
                for index in range {
                    let mut worker = workers[index].lock().map_err(|_| contract_error())?;
                    ledger
                        .owner(index)
                        .ensure(
                            worker
                                .retained_bytes()
                                .and_then(|bytes| bytes.checked_add(future))
                                .ok_or_else(contract_error)?,
                        )
                        .map_err(WasmExactSearchError::resource_admission)?;
                    let bits = worker.buildup.materialize_standard_bag_coverage()?;
                    ledger
                        .owner(index)
                        .ensure(
                            worker
                                .retained_bytes()
                                .and_then(|bytes| {
                                    bytes.checked_add(
                                        bits.as_ref().map_or(0, PatternBitSet::retained_bytes)
                                            as u128,
                                    )
                                })
                                .ok_or_else(contract_error)?,
                        )
                        .map_err(WasmExactSearchError::resource_admission)?;
                    *slots[index].lock().map_err(|_| contract_error())? = bits;
                }
                Ok(())
            },
        )
        .map_err(parallel_error)?;
        self.peak_cpu_bytes = self
            .peak_cpu_bytes
            .max(usize::try_from(ledger.admitted_peak_bytes()).unwrap_or(usize::MAX));
        let mut retained_bits = slots
            .iter()
            .try_fold(0_u128, |bytes, slot| {
                bytes.checked_add(
                    slot.lock()
                        .ok()?
                        .as_ref()
                        .map_or(0, PatternBitSet::retained_bytes) as u128,
                )
            })
            .ok_or_else(contract_error)?;
        for slot in slots {
            if let Some(bits) = slot.into_inner().map_err(|_| contract_error())? {
                self.parallel_coexisting_bytes = fixed
                    .checked_add(worker_bytes(workers)?)
                    .and_then(|bytes| bytes.checked_add(retained_bits))
                    .ok_or_else(contract_error)?;
                self.ensure_session_memory_bound(future)?;
                self.covered_patterns
                    .union_with(&bits)
                    .map_err(|_| contract_error())?;
                retained_bits = retained_bits
                    .checked_sub(bits.retained_bytes() as u128)
                    .ok_or_else(contract_error)?;
            }
        }
        Ok(())
    }
}

fn worker_bytes(workers: &[Mutex<Worker>]) -> Result<u128, WasmExactSearchError> {
    workers
        .iter()
        .try_fold(0_u128, |bytes, worker| {
            bytes.checked_add(worker.lock().ok()?.retained_bytes()?)
        })
        .ok_or_else(contract_error)
}

fn parallel_error(error: ParallelWorkError<WasmExactSearchError>) -> WasmExactSearchError {
    match error {
        ParallelWorkError::Worker(error) => error,
        ParallelWorkError::Cancelled => WasmExactSearchError::Cancelled,
        ParallelWorkError::ThreadSpawnFailed => {
            WasmExactSearchError::InvalidProblem("wasm_candidate_worker_spawn_failed")
        }
        ParallelWorkError::WorkerPanicked => {
            WasmExactSearchError::InvalidProblem("wasm_candidate_worker_panicked")
        }
    }
}

fn contract_error() -> WasmExactSearchError {
    WasmExactSearchError::InvalidProblem("wasm_candidate_window_resource_or_source_invalid")
}
