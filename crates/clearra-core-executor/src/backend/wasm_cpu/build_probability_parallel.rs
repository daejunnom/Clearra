//! Native compact fixed-field verification. Geometry emits a bounded source
//! window; independent candidate checks reuse N private workspaces and the
//! same borrowed immutable problem/catalog. Results reduce in source order.
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Mutex,
};

use crate::resource::SharedExecutionMemoryBudget;
use clearra_core_domain::parallel_work::{try_for_each_chunk, ParallelWorkError};

use super::{
    verify_candidate_for_completion, verify_candidate_for_completion_with_finesse, BuildCompletion,
    BuildProbabilityAdvance, BuildUpWorkspace, CandidateBuildResult, CandidateWitnessMode,
    CompactBuildProbabilitySession, CoverageProductEvaluator, ExecutionControl, GeometryAdvance,
    GeometryCandidate, PatternBitSet, WasmExactSearchError,
};

#[derive(Default)]
struct Worker {
    buildup: BuildUpWorkspace,
    evaluator: CoverageProductEvaluator,
    candidates: usize,
    results: u128,
}

impl Worker {
    fn retained_bytes(&self) -> Option<u128> {
        (self.buildup.retained_bytes() as u128).checked_add(self.evaluator.retained_bytes() as u128)
    }
}

fn result_nested_bytes(result: &CandidateBuildResult) -> Option<u128> {
    let mut bytes = result
        .covered_patterns
        .as_ref()
        .map_or(0, PatternBitSet::retained_bytes) as u128;
    bytes = bytes.checked_add(
        (result.representative_path.capacity() as u128)
            .checked_mul(core::mem::size_of::<crate::CorePathStep>() as u128)?,
    )?;
    if let Some(language) = &result.finesse_language {
        bytes =
            bytes
                .checked_add((language.nodes.capacity() as u128).checked_mul(
                    core::mem::size_of::<super::super::buildup::PreparedFinesseNode>() as u128,
                )?)?
                .checked_add((language.edges.capacity() as u128).checked_mul(
                    core::mem::size_of::<super::super::buildup::PreparedFinesseEdge>() as u128,
                )?)?;
    }
    Some(bytes)
}

impl CompactBuildProbabilitySession {
    pub(super) fn execute_native_parallel(
        &mut self,
        control: &ExecutionControl,
    ) -> Result<Option<BuildProbabilityAdvance>, WasmExactSearchError> {
        let width = if control.has_parallelism_binding() {
            control.parallelism()
        } else {
            self.problem.backend_policy().workers()
        };
        if width <= 1
            || self.candidate_count != 0
            || self.trivial_target
            || self.finished
            || (self.aggregation.is_tiling_only() && !self.finesse_requested)
        {
            return Ok(None);
        }
        let control = control.clone().with_parallelism_if_unbound(width);
        self.execution_control = control.clone();
        let original_coexisting = self.coexisting_retained_bytes;
        let result = self.execute_native_parallel_inner(width, &control, original_coexisting);
        self.coexisting_retained_bytes = original_coexisting;
        result.map(Some)
    }

    fn execute_native_parallel_inner(
        &mut self,
        width: usize,
        control: &ExecutionControl,
        original_coexisting: u128,
    ) -> Result<BuildProbabilityAdvance, WasmExactSearchError> {
        let wave_width = width.checked_mul(16).ok_or_else(contract_error)?;
        let fixed = (width as u128)
            .checked_mul((core::mem::size_of::<Mutex<Worker>>() + 2048) as u128)
            .and_then(|bytes| {
                bytes.checked_add(
                    (wave_width as u128)
                        * (core::mem::size_of::<(GeometryCandidate, u64)>()
                            + core::mem::size_of::<Mutex<Option<CandidateBuildResult>>>())
                            as u128,
                )
            })
            .ok_or_else(contract_error)?;
        self.ensure_memory_bound(fixed)?;
        let mut workers = Vec::new();
        workers
            .try_reserve_exact(width)
            .map_err(|_| storage_error())?;
        workers.resize_with(width, || Mutex::new(Worker::default()));
        let mut candidates = Vec::new();
        candidates
            .try_reserve_exact(wave_width)
            .map_err(|_| storage_error())?;
        let mut finished = false;
        while !finished {
            if control.is_cancelled() {
                return Ok(BuildProbabilityAdvance::Cancelled);
            }
            candidates.clear();
            while candidates.len() < wave_width {
                if control.is_cancelled() {
                    return Ok(BuildProbabilityAdvance::Cancelled);
                }
                match self.geometry.advance(&self.catalog) {
                    GeometryAdvance::Pending => {}
                    GeometryAdvance::Candidate(candidate) => {
                        if !self.problem.allows_solution_identity(&candidate.identity) {
                            continue;
                        }
                        let cap = self.problem.backend_request().max_candidates();
                        if cap != 0 && self.candidate_count >= cap {
                            self.truncated_reason = Some("candidate_budget_exceeded");
                            self.count_complete = false;
                            finished = true;
                            break;
                        }
                        let ordinal =
                            u64::try_from(self.candidate_count).map_err(|_| contract_error())?;
                        self.candidate_count = self
                            .candidate_count
                            .checked_add(1)
                            .ok_or_else(contract_error)?;
                        self.candidate_digest = super::super::mix_digest(
                            self.candidate_digest,
                            candidate.identity.bucket_hash(),
                        );
                        if self.aggregation.is_tiling_only() && !self.finesse_requested {
                            self.buildable_tilings
                                .try_reserve(1)
                                .map_err(|_| storage_error())?;
                            self.buildable_tilings.insert(candidate.identity);
                        } else {
                            candidates.push((candidate, ordinal));
                        }
                    }
                    GeometryAdvance::Complete => {
                        finished = true;
                        break;
                    }
                    GeometryAdvance::ResourceIncomplete(reason) => {
                        self.truncated_reason = Some(reason);
                        self.count_complete = false;
                        finished = true;
                        break;
                    }
                }
            }
            if candidates.is_empty() {
                continue;
            }
            let base = self
                .checked_retained_bytes()
                .and_then(|bytes| bytes.checked_add(original_coexisting))
                .and_then(|bytes| bytes.checked_add(fixed))
                .ok_or_else(contract_error)?;
            let ledger = SharedExecutionMemoryBudget::new(self.memory_bound, base, width)
                .map_err(WasmExactSearchError::resource_admission)?;
            for (index, worker) in workers.iter().enumerate() {
                let mut worker = worker.lock().map_err(|_| contract_error())?;
                worker.results = 0;
                ledger
                    .owner(index)
                    .ensure(worker.retained_bytes().ok_or_else(contract_error)?)
                    .map_err(WasmExactSearchError::resource_admission)?;
            }
            let slots: Vec<_> = (0..candidates.len()).map(|_| Mutex::new(None)).collect();
            let next_worker = AtomicUsize::new(0);
            try_for_each_chunk(
                control,
                candidates.len(),
                1,
                || {
                    let index = next_worker.fetch_add(1, Ordering::Relaxed);
                    if index >= width {
                        Err(contract_error())
                    } else {
                        Ok(index)
                    }
                },
                |worker_index, range| {
                    let mut worker = workers[*worker_index]
                        .lock()
                        .map_err(|_| contract_error())?;
                    let owner = ledger.owner(*worker_index);
                    for index in range {
                        let candidate = &candidates[index].0;
                        let projection =
                            super::super::buildup::checked_candidate_verification_peak_upper_bound(
                                &self.problem,
                                &self.catalog,
                                candidate,
                                true,
                            )
                            .ok_or_else(contract_error)?;
                        owner
                            .ensure(
                                worker
                                    .retained_bytes()
                                    .and_then(|bytes| bytes.checked_add(worker.results))
                                    .and_then(|bytes| bytes.checked_add(projection))
                                    .ok_or_else(contract_error)?,
                            )
                            .map_err(WasmExactSearchError::resource_admission)?;
                        let target = self
                            .shared_supply_catalog
                            .targets
                            .targets()
                            .get(candidate.target_index as usize)
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
                            self.solution_coverage.is_some(),
                            false,
                        );
                        let Worker {
                            buildup, evaluator, ..
                        } = &mut *worker;
                        let mut result = if self.finesse_requested {
                            verify_candidate_for_completion_with_finesse(
                                &self.problem,
                                &self.catalog,
                                candidate,
                                target,
                                buildup,
                                evaluator,
                                witness,
                                self.representative_path.is_empty(),
                                0,
                                BuildCompletion::ExactBoardAfterLineClears(self.final_board),
                                self.aggregation.requests_spin_coverage(),
                                control,
                            )
                        } else {
                            verify_candidate_for_completion(
                                &self.problem,
                                &self.catalog,
                                candidate,
                                target,
                                buildup,
                                evaluator,
                                witness,
                                self.representative_path.is_empty(),
                                0,
                                BuildCompletion::ExactBoardAfterLineClears(self.final_board),
                                control,
                            )
                        }?;
                        // Symbolic IDs are workspace-local. Resolve required rows
                        // here, and retain aggregate language in its true owner.
                        if let Some(root) = result.symbolic_coverage_root.take() {
                            if self.solution_coverage.is_some() {
                                let materialized =
                                    worker.buildup.materialize_standard_bag_root(root)?;
                                if let Some(bits) = result.covered_patterns.as_mut() {
                                    bits.union_with(&materialized)
                                        .map_err(|_| contract_error())?;
                                } else {
                                    result.covered_patterns = Some(materialized);
                                }
                            }
                            worker.buildup.merge_standard_bag_coverage(root)?;
                        }
                        worker.candidates += 1;
                        worker.results = worker
                            .results
                            .checked_add(result_nested_bytes(&result).ok_or_else(contract_error)?)
                            .ok_or_else(contract_error)?;
                        owner
                            .ensure(
                                worker
                                    .retained_bytes()
                                    .and_then(|bytes| bytes.checked_add(worker.results))
                                    .ok_or_else(contract_error)?,
                            )
                            .map_err(WasmExactSearchError::resource_admission)?;
                        *slots[index].lock().map_err(|_| contract_error())? = Some(result);
                    }
                    Ok(())
                },
            )
            .map_err(map_parallel_error)?;
            self.native_parallel_peak_bytes = self
                .native_parallel_peak_bytes
                .max(ledger.admitted_peak_bytes());
            let private = workers.iter().try_fold(0_u128, |bytes, worker| {
                let worker = worker.lock().map_err(|_| contract_error())?;
                bytes
                    .checked_add(worker.retained_bytes().ok_or_else(contract_error)?)
                    .and_then(|bytes| bytes.checked_add(worker.results))
                    .ok_or_else(contract_error)
            })?;
            self.coexisting_retained_bytes = original_coexisting
                .checked_add(fixed)
                .and_then(|bytes| bytes.checked_add(private))
                .ok_or_else(contract_error)?;
            for (index, slot) in slots.into_iter().enumerate() {
                let result = slot
                    .into_inner()
                    .map_err(|_| contract_error())?
                    .ok_or_else(contract_error)?;
                let (candidate, ordinal) = candidates[index];
                if self.aggregation.is_tiling_only() {
                    self.buildable_tilings
                        .try_reserve(1)
                        .map_err(|_| storage_error())?;
                    self.buildable_tilings.insert(candidate.identity);
                }
                self.apply_candidate_result(candidate.identity, Some(ordinal), result)?;
                self.ensure_memory_bound(0)?;
            }
            self.coexisting_retained_bytes = original_coexisting;
            control.report_progress("build-verification", self.total_build_nodes as u64, None);
        }
        let private = workers.iter().try_fold(0_u128, |bytes, worker| {
            bytes
                .checked_add(
                    worker
                        .lock()
                        .map_err(|_| contract_error())?
                        .retained_bytes()
                        .ok_or_else(contract_error)?,
                )
                .ok_or_else(contract_error)
        })?;
        self.coexisting_retained_bytes = original_coexisting
            .checked_add(fixed)
            .and_then(|bytes| bytes.checked_add(private))
            .ok_or_else(contract_error)?;
        self.parallel_active_workers = 0;
        self.parallel_minimum_worker_candidates = 0;
        for worker in &workers {
            let mut worker = worker.lock().map_err(|_| contract_error())?;
            self.ensure_symbolic_coverage_finalization_bound()?;
            if let Some(bits) = worker.buildup.materialize_standard_bag_coverage()? {
                self.covered_patterns
                    .union_with(&bits)
                    .map_err(|_| contract_error())?;
            }
            self.parallel_active_workers += usize::from(worker.candidates != 0);
            if worker.candidates != 0 {
                self.parallel_minimum_worker_candidates =
                    if self.parallel_minimum_worker_candidates == 0 {
                        worker.candidates
                    } else {
                        self.parallel_minimum_worker_candidates
                            .min(worker.candidates)
                    };
            }
            self.parallel_maximum_worker_candidates = self
                .parallel_maximum_worker_candidates
                .max(worker.candidates);
        }
        self.workers_used = width;
        self.parallel_decision_reason = "native-shared-build-candidate-windows";
        drop(workers);
        drop(candidates);
        self.coexisting_retained_bytes = original_coexisting;
        self.complete()
    }
}

fn map_parallel_error(error: ParallelWorkError<WasmExactSearchError>) -> WasmExactSearchError {
    match error {
        ParallelWorkError::Worker(error) => error,
        ParallelWorkError::Cancelled => WasmExactSearchError::Cancelled,
        ParallelWorkError::ThreadSpawnFailed => {
            WasmExactSearchError::InvalidProblem("wasm_build_probability_worker_spawn_failed")
        }
        ParallelWorkError::WorkerPanicked => {
            WasmExactSearchError::InvalidProblem("wasm_build_probability_worker_panicked")
        }
    }
}
fn contract_error() -> WasmExactSearchError {
    WasmExactSearchError::InvalidProblem("wasm_build_probability_parallel_contract_invalid")
}
fn storage_error() -> WasmExactSearchError {
    WasmExactSearchError::InvalidProblem("wasm_build_probability_parallel_storage_unavailable")
}
