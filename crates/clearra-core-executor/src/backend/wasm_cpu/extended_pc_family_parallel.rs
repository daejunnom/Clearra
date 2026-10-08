//! Native ordinary PC execution over the parent's exact four-word family.
//! One request lease covers N-1 pool jobs and the caller. Immutable catalog,
//! input and Geometry indexes are shared; BuildUp/language/reducer workspaces
//! are private. No worker relabels a Build result or grants typed PC authority.
use std::{
    mem::size_of,
    panic::{catch_unwind, AssertUnwindSafe},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc, Mutex,
    },
};

use crate::{cpu_worker_pool, resource::ExecutionMemoryBound};

use super::super::extended_geometry::ExtendedParallelGeometryBranch;
use super::{
    mix_digest, Arc, BuildProbabilityAdvance, CoreExecutionResult, CoverageProductEvaluator,
    ExecutionControl, ExtendedBuildOrderWorkspace, ExtendedBuildProbabilitySession,
    ExtendedFamilyPurpose, ExtendedGeometryAdvance, ExtendedGeometrySearch, HashMap, HashSet,
    PatternBitSet, WasmExactSearchError,
};

struct DigestSegment {
    first_ordinal: u128,
    values: Vec<u64>,
}

struct WorkerResult {
    engine: ExtendedBuildProbabilitySession,
    digests: Vec<DigestSegment>,
    private_peak_bytes: u128,
}

impl WorkerResult {
    fn checked_retained_bytes(&self) -> Option<u128> {
        let mut bytes = self.engine.checked_retained_bytes()?.checked_add(
            (self.digests.capacity() as u128).checked_mul(size_of::<DigestSegment>() as u128)?,
        )?;
        for segment in &self.digests {
            bytes = bytes.checked_add(
                (segment.values.capacity() as u128).checked_mul(size_of::<u64>() as u128)?,
            )?;
        }
        Some(bytes)
    }

    fn digest_retained_bytes(&self) -> Result<u128, WasmExactSearchError> {
        self.checked_retained_bytes()
            .and_then(|bytes| bytes.checked_sub(self.engine.checked_retained_bytes()?))
            .ok_or_else(projection_error)
    }
}

impl ExtendedBuildProbabilitySession {
    pub(in crate::backend::wasm_cpu) fn requested_pc_workers(&self) -> usize {
        self.problem.backend_policy().workers()
    }

    pub(in crate::backend::wasm_cpu) fn execute_pc_family_parallel(
        &mut self,
        requested_workers: usize,
        control: &ExecutionControl,
    ) -> Result<Option<CoreExecutionResult>, WasmExactSearchError> {
        if requested_workers <= 1 {
            return Ok(None);
        }
        if self.purpose != ExtendedFamilyPurpose::Pc
            || self.shared_immutable_owners
            || requested_workers != self.requested_pc_workers()
            || self.finished
            || self.processed_candidate_count != 0
            || control.partition().count() != 1
        {
            return Err(contract_error());
        }
        if self.problem.backend_request().max_nodes() != 0 {
            return Err(WasmExactSearchError::InvalidProblem(
                "extended_pc_family_parallel_node_budget_not_connected",
            ));
        }
        let mut preparation_steps = 0_usize;
        while self.geometry.is_compiling() {
            if control.is_cancelled() {
                return Err(WasmExactSearchError::Cancelled);
            }
            self.ensure_memory_bound(0)?;
            match self.geometry.advance(&self.catalog) {
                ExtendedGeometryAdvance::Pending => {}
                ExtendedGeometryAdvance::ResourceIncomplete(reason) => {
                    return Err(WasmExactSearchError::InvalidProblem(reason));
                }
                _ => return Err(contract_error()),
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
            self.memory_bound,
            live.checked_add(self.coexisting_retained_bytes)
                .ok_or_else(projection_error)?,
        )?
        else {
            // An empty family keeps the normal serial terminal. No useful
            // branch was discarded and no multiworker result is claimed.
            return Ok(None);
        };
        let candidate_count =
            usize::try_from(plan.candidate_count).map_err(|_| projection_error())?;
        let max_candidates = self.problem.backend_request().max_candidates();
        if max_candidates != 0 && candidate_count > max_candidates {
            return Err(WasmExactSearchError::InvalidProblem(
                "candidate_budget_exceeded",
            ));
        }
        let workers = requested_workers.min(plan.branches.len()).max(1);
        let branch_count = plan.branches.len();
        let original_coexisting = self.coexisting_retained_bytes;
        let root_retained = self.checked_retained_bytes().ok_or_else(projection_error)?;
        let fixed = root_retained
            .checked_add(original_coexisting)
            .and_then(|bytes| bytes.checked_add(plan.shared_retained_bytes() as u128))
            .and_then(|bytes| bytes.checked_add(plan.branch_retained_bytes() as u128))
            .and_then(|bytes| {
                bytes.checked_add(
                    (workers as u128)
                        * (2048
                            + size_of::<Self>()
                            + 2 * size_of::<WorkerResult>()
                            + size_of::<ExtendedParallelGeometryBranch>())
                            as u128,
                )
            })
            .ok_or_else(projection_error)?;
        self.memory_bound
            .ensure(fixed, 0)
            .map_err(WasmExactSearchError::resource_admission)?;
        let credit = (self.memory_bound.cap_bytes() - fixed) / workers as u128;
        let worker_bound = self
            .memory_bound
            .with_cap(credit)
            .map_err(WasmExactSearchError::resource_admission)?;

        plan.branches
            .sort_unstable_by_key(|branch| (branch.candidate_count, branch.first_ordinal));
        let mut tasks = Vec::new();
        tasks
            .try_reserve_exact(workers)
            .map_err(|_| storage_error())?;
        // Give every launched worker real initial work even when the caller
        // could otherwise empty a tiny queue before pool jobs begin running.
        for _ in 0..workers {
            let engine = self.new_shared_pc_verifier(worker_bound)?;
            let first = plan.branches.pop().ok_or_else(contract_error)?;
            tasks.push((engine, first));
        }
        let queue = Arc::new(Mutex::new(core::mem::take(&mut plan.branches)));
        let abort = Arc::new(AtomicBool::new(false));
        let progress_nodes = Arc::new(AtomicUsize::new(0));
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
        let (sender, receiver) = mpsc::channel();
        let (caller_engine, caller_first) = tasks.pop().ok_or_else(contract_error)?;
        let mut submitted = 0_usize;
        let mut first_error = None;
        for (engine, first) in tasks {
            let queue = Arc::clone(&queue);
            let abort_worker = Arc::clone(&abort);
            let progress = Arc::clone(&progress_nodes);
            let worker_control = control.clone();
            let sender = sender.clone();
            let queued = cpu_worker_pool::submit_cpu_job(move || {
                let result = caught_worker(
                    engine,
                    first,
                    &queue,
                    &abort_worker,
                    &progress,
                    &worker_control,
                );
                if result.is_err() {
                    abort_worker.store(true, Ordering::Release);
                    worker_control.cancellation.handle().cancel();
                }
                let _ = sender.send(result);
            });
            if queued.is_err() {
                abort.store(true, Ordering::Release);
                control.cancellation.handle().cancel();
                first_error = Some(pool_error());
                break;
            }
            submitted += 1;
        }
        drop(sender);
        let caller = caught_worker(
            caller_engine,
            caller_first,
            &queue,
            &abort,
            &progress_nodes,
            control,
        );
        if caller.is_err() {
            abort.store(true, Ordering::Release);
            control.cancellation.handle().cancel();
        }
        let mut completed = Vec::new();
        completed.try_reserve_exact(workers).map_err(|_| {
            // Allocation was admitted before jobs started. The channel still
            // must be drained if allocation unexpectedly fails.
            abort.store(true, Ordering::Release);
            control.cancellation.handle().cancel();
            for _ in 0..submitted {
                let _ = receiver.recv();
            }
            storage_error()
        })?;
        completed.push(caller);
        // Errors, cancellation and caught panics never release the request
        // lease while a submitted descendant job still owns its workspace.
        for _ in 0..submitted {
            completed.push(receiver.recv().unwrap_or_else(|_| Err(pool_error())));
        }
        let mut successful = Vec::new();
        successful
            .try_reserve_exact(workers)
            .map_err(|_| storage_error())?;
        for result in completed {
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

        let private_bytes = checked_worker_bytes(&successful)?;
        let peak = successful
            .iter()
            .try_fold(fixed, |bytes, worker| {
                bytes.checked_add(worker.private_peak_bytes)
            })
            .ok_or_else(projection_error)?;
        self.native_parallel_peak_bytes = self.native_parallel_peak_bytes.max(peak);
        // FNV is ordered, not a commutative worker digest. Fold the bounded
        // per-branch values in exact serial ordinal order, including rejected
        // BuildUp candidates. Never concatenate worker completion digests.
        let segment_refs_bytes = (branch_count as u128) * size_of::<&DigestSegment>() as u128;
        self.memory_bound
            .ensure(fixed + private_bytes, segment_refs_bytes)
            .map_err(WasmExactSearchError::resource_admission)?;
        let mut segments = Vec::new();
        segments
            .try_reserve_exact(branch_count)
            .map_err(|_| storage_error())?;
        for worker in &successful {
            segments.extend(worker.digests.iter());
        }
        segments.sort_unstable_by_key(|segment| segment.first_ordinal);
        let mut ordinal = 0_u128;
        self.candidate_digest = 0;
        for segment in segments {
            if segment.first_ordinal != ordinal {
                return Err(contract_error());
            }
            for value in &segment.values {
                self.candidate_digest = mix_digest(self.candidate_digest, *value);
            }
            ordinal = ordinal
                .checked_add(segment.values.len() as u128)
                .ok_or_else(projection_error)?;
        }
        if ordinal != plan.candidate_count {
            return Err(contract_error());
        }
        let keys = successful
            .iter()
            .try_fold(0_usize, |count, worker| {
                count.checked_add(worker.engine.buildable_tilings.len())
            })
            .ok_or_else(projection_error)?;
        let coverage_rows = successful
            .iter()
            .try_fold(0_usize, |count, worker| {
                count.checked_add(
                    worker
                        .engine
                        .solution_coverage
                        .as_ref()
                        .map_or(0, HashMap::len),
                )
            })
            .ok_or_else(projection_error)?;
        let execution_graphs = successful
            .iter()
            .try_fold(0_usize, |count, worker| {
                count.checked_add(worker.engine.spin_execution_graphs.len())
            })
            .ok_or_else(projection_error)?;
        // Bucket/control growth coexists with transferred private keys and
        // coverage. The conservative projection does not rely on HashMap's
        // internal load factor or ownership reference count.
        let merge_future = (keys as u128)
            .checked_mul((4 * (size_of::<super::ExtendedTilingKey>() + 1)) as u128)
            .and_then(|bytes| {
                bytes.checked_add(
                    (coverage_rows as u128)
                        * (4 * (size_of::<String>() + size_of::<PatternBitSet>() + 1)) as u128,
                )
            })
            .and_then(|bytes| {
                bytes.checked_add(
                    (execution_graphs as u128)
                        * size_of::<super::SpinCoverageExecutionGraph>() as u128,
                )
            })
            .ok_or_else(projection_error)?;
        let merge_observed = fixed
            .checked_add(private_bytes)
            .ok_or_else(projection_error)?;
        self.memory_bound
            .ensure(merge_observed, merge_future)
            .map_err(WasmExactSearchError::resource_admission)?;
        self.native_parallel_peak_bytes = self
            .native_parallel_peak_bytes
            .max(merge_observed + merge_future);
        self.buildable_tilings
            .try_reserve(keys)
            .map_err(|_| storage_error())?;
        if let Some(rows) = self.solution_coverage.as_mut() {
            rows.try_reserve(coverage_rows)
                .map_err(|_| storage_error())?;
        }
        self.spin_execution_graphs
            .try_reserve_exact(execution_graphs)
            .map_err(|_| storage_error())?;
        self.parallel_minimum_worker_candidates = usize::MAX;
        while let Some(worker) = successful.pop() {
            self.merge_pc_verifier(worker.engine)?;
            // The popped worker's remaining scratch/digest owners drop here.
            drop(worker.digests);
            self.coexisting_retained_bytes = fixed
                .checked_sub(root_retained)
                .and_then(|bytes| bytes.checked_add(checked_worker_bytes(&successful).ok()?))
                .ok_or_else(projection_error)?;
            self.ensure_memory_bound(0)?;
        }
        if self.processed_candidate_count != candidate_count {
            return Err(contract_error());
        }
        self.workers_used = workers;
        self.geometry
            .complete_parallel_enumeration(candidate_count)?;
        self.coexisting_retained_bytes = original_coexisting;
        drop(queue);
        drop(plan);
        self.ensure_memory_bound(0)?;
        control.report_progress("buildup", self.searched_build_nodes as u64, None);
        match self.complete()? {
            BuildProbabilityAdvance::Completed(result) => Ok(Some(result)),
            _ => Err(contract_error()),
        }
    }

    fn new_shared_pc_verifier(
        &self,
        memory_bound: ExecutionMemoryBound,
    ) -> Result<Self, WasmExactSearchError> {
        if self.purpose != ExtendedFamilyPurpose::Pc {
            return Err(contract_error());
        }
        let pattern_count = self
            .problem
            .piece_source()
            .materialized_universe()
            .ok_or_else(contract_error)?
            .pattern_count();
        let verifier = Self {
            problem: Arc::clone(&self.problem),
            purpose: ExtendedFamilyPurpose::Pc,
            aggregation: self.aggregation,
            field: self.field,
            catalog: Arc::clone(&self.catalog),
            shared_immutable_owners: true,
            geometry: ExtendedGeometrySearch::private_verifier_placeholder(),
            build_order_workspace: ExtendedBuildOrderWorkspace::new(
                self.field.width(),
                self.field.height(),
                self.problem.kick_profile().profile_id(),
            ),
            coverage_evaluator: CoverageProductEvaluator::default(),
            covered_patterns: PatternBitSet::new(pattern_count),
            buildable_tilings: HashSet::new(),
            solution_coverage: self.solution_coverage.as_ref().map(|_| HashMap::new()),
            spin_execution_graphs: Vec::new(),
            distributed_solution_keys: HashSet::new(),
            candidate_digest: 0,
            processed_candidate_count: 0,
            searched_build_nodes: 0,
            reachability_states: 0,
            coverage_product_states: 0,
            coverage_product_edge_checks: 0,
            coverage_product_words: 0,
            peak_build_order_nodes: 0,
            total_build_order_nodes: 0,
            peak_build_scratch_bytes: 0,
            witnessed_pattern_count: 0,
            pc_build_variant_count: 0,
            pc_build_variant_count_complete: true,
            representative_path: Vec::new(),
            representative_pattern_id: None,
            representative_rank: None,
            truncated_reason: None,
            supply_projection_complete: self.supply_projection_complete,
            distributed_count_complete: true,
            distributed_probability_complete: true,
            trivial_target: false,
            external_geometry: true,
            workers_used: 1,
            parallel_active_workers: 0,
            parallel_minimum_worker_candidates: 0,
            parallel_maximum_worker_candidates: 0,
            distributed_worker_memory_bytes: 0,
            native_parallel_peak_bytes: 0,
            distributed_execution_constraint_materialized: false,
            finesse_requested: false,
            finesse_languages: Vec::new(),
            memory_bound,
            coexisting_retained_bytes: 0,
            finished: false,
        };
        verifier.ensure_memory_bound(0)?;
        Ok(verifier)
    }

    fn merge_pc_verifier(&mut self, mut worker: Self) -> Result<(), WasmExactSearchError> {
        if worker.purpose != ExtendedFamilyPurpose::Pc
            || !worker.shared_immutable_owners
            || !Arc::ptr_eq(&self.problem, &worker.problem)
            || !Arc::ptr_eq(&self.catalog, &worker.catalog)
            || worker.truncated_reason.is_some()
            || worker.finished
            || (!self.problem.objective().score().requested()
                && !worker.spin_execution_graphs.is_empty())
            || !worker.finesse_languages.is_empty()
        {
            return Err(contract_error());
        }
        self.covered_patterns
            .union_with(&worker.covered_patterns)
            .map_err(|_| contract_error())?;
        self.buildable_tilings
            .extend(worker.buildable_tilings.drain());
        // Transfer concrete lock evidence, never a compact Board64 surrogate
        // or only the worker's representative path. Canonical IDs are rebound
        // once after the complete source family has been merged and sorted.
        self.spin_execution_graphs
            .append(&mut worker.spin_execution_graphs);
        if let Some(rows) = worker.solution_coverage.take() {
            let target = self.solution_coverage.as_mut().ok_or_else(contract_error)?;
            for (key, bits) in rows {
                match target.entry(key) {
                    std::collections::hash_map::Entry::Vacant(entry) => {
                        entry.insert(bits);
                    }
                    std::collections::hash_map::Entry::Occupied(mut entry) => {
                        entry
                            .get_mut()
                            .union_with(&bits)
                            .map_err(|_| contract_error())?;
                    }
                }
            }
        }
        if worker.representative_rank.is_some_and(|rank| {
            self.representative_rank
                .is_none_or(|current| rank < current)
        }) {
            self.representative_path = core::mem::take(&mut worker.representative_path);
            self.representative_rank = worker.representative_rank;
            self.representative_pattern_id = worker.representative_pattern_id;
        }
        let count = worker.processed_candidate_count;
        self.processed_candidate_count = self
            .processed_candidate_count
            .checked_add(count)
            .ok_or_else(projection_error)?;
        self.parallel_active_workers += usize::from(count != 0);
        self.parallel_minimum_worker_candidates =
            self.parallel_minimum_worker_candidates.min(count);
        self.parallel_maximum_worker_candidates =
            self.parallel_maximum_worker_candidates.max(count);
        self.searched_build_nodes = self
            .searched_build_nodes
            .saturating_add(worker.searched_build_nodes);
        self.reachability_states = self
            .reachability_states
            .saturating_add(worker.reachability_states);
        self.coverage_product_states = self
            .coverage_product_states
            .saturating_add(worker.coverage_product_states);
        self.coverage_product_edge_checks = self
            .coverage_product_edge_checks
            .saturating_add(worker.coverage_product_edge_checks);
        self.coverage_product_words = self
            .coverage_product_words
            .saturating_add(worker.coverage_product_words);
        self.peak_build_order_nodes = self
            .peak_build_order_nodes
            .max(worker.peak_build_order_nodes);
        self.total_build_order_nodes = self
            .total_build_order_nodes
            .saturating_add(worker.total_build_order_nodes);
        self.peak_build_scratch_bytes = self
            .peak_build_scratch_bytes
            .max(worker.peak_build_scratch_bytes);
        self.witnessed_pattern_count = self
            .witnessed_pattern_count
            .saturating_add(worker.witnessed_pattern_count);
        let next = self
            .pc_build_variant_count
            .checked_add(worker.pc_build_variant_count);
        self.pc_build_variant_count = next.unwrap_or(u128::MAX);
        self.pc_build_variant_count_complete &=
            next.is_some() && worker.pc_build_variant_count_complete;
        Ok(())
    }
}

fn caught_worker(
    engine: ExtendedBuildProbabilitySession,
    first: ExtendedParallelGeometryBranch,
    queue: &Mutex<Vec<ExtendedParallelGeometryBranch>>,
    abort: &AtomicBool,
    progress: &AtomicUsize,
    control: &ExecutionControl,
) -> Result<WorkerResult, WasmExactSearchError> {
    catch_unwind(AssertUnwindSafe(|| {
        run_worker(engine, first, queue, abort, progress, control)
    }))
    .map_err(|_| WasmExactSearchError::InvalidProblem("extended_pc_family_worker_panicked"))?
}

fn run_worker(
    engine: ExtendedBuildProbabilitySession,
    first: ExtendedParallelGeometryBranch,
    queue: &Mutex<Vec<ExtendedParallelGeometryBranch>>,
    abort: &AtomicBool,
    progress: &AtomicUsize,
    control: &ExecutionControl,
) -> Result<WorkerResult, WasmExactSearchError> {
    let mut result = WorkerResult {
        engine,
        digests: Vec::new(),
        private_peak_bytes: 0,
    };
    let mut assigned = Some(first);
    loop {
        if control.is_cancelled() || abort.load(Ordering::Acquire) {
            return Err(WasmExactSearchError::Cancelled);
        }
        let next = match assigned.take() {
            Some(branch) => Some(branch),
            None => queue.lock().map_err(|_| pool_error())?.pop(),
        };
        let Some(mut branch) = next else {
            break;
        };
        let count = usize::try_from(branch.candidate_count).map_err(|_| projection_error())?;
        let growth = (count as u128)
            .checked_mul(size_of::<u64>() as u128)
            .and_then(|bytes| {
                bytes.checked_add(
                    (result.digests.len() + 1) as u128 * 2 * size_of::<DigestSegment>() as u128,
                )
            })
            .ok_or_else(projection_error)?;
        let owned = result
            .checked_retained_bytes()
            .ok_or_else(projection_error)?;
        let branch_bytes = branch.private_retained_bytes() as u128;
        result
            .engine
            .memory_bound
            .ensure(owned + branch_bytes, growth)
            .map_err(WasmExactSearchError::resource_admission)?;
        result.digests.try_reserve(1).map_err(|_| storage_error())?;
        let mut values = Vec::new();
        values
            .try_reserve_exact(count)
            .map_err(|_| storage_error())?;
        result.digests.push(DigestSegment {
            first_ordinal: branch.first_ordinal,
            values,
        });
        let index = result.digests.len() - 1;
        let before_count = result.engine.processed_candidate_count;
        loop {
            if control.is_cancelled() || abort.load(Ordering::Acquire) {
                return Err(WasmExactSearchError::Cancelled);
            }
            let owned = result
                .checked_retained_bytes()
                .ok_or_else(projection_error)?;
            let Some(candidate) =
                branch.next_candidate(&result.engine.catalog, result.engine.memory_bound, owned)?
            else {
                break;
            };
            let ordinal = branch
                .first_ordinal
                .checked_add(result.digests[index].values.len() as u128)
                .and_then(|ordinal| u64::try_from(ordinal).ok())
                .ok_or_else(projection_error)?;
            let digest =
                super::ExtendedTilingKey::from_candidate(&result.engine.catalog, &candidate)
                    .digest();
            let before_nodes = result.engine.searched_build_nodes;
            result.engine.coexisting_retained_bytes = result
                .digest_retained_bytes()?
                .checked_add(branch.private_retained_bytes() as u128)
                .ok_or_else(projection_error)?;
            result.engine.ensure_memory_bound(0)?;
            result
                .engine
                .process_candidate(candidate, Some(ordinal), control)?;
            if let Some(reason) = result.engine.truncated_reason {
                return Err(WasmExactSearchError::InvalidProblem(reason));
            }
            result.digests[index].values.push(digest);
            result.engine.ensure_memory_bound(0)?;
            let retained = result
                .checked_retained_bytes()
                .ok_or_else(projection_error)?
                .checked_add(branch.private_retained_bytes() as u128)
                .ok_or_else(projection_error)?;
            result.private_peak_bytes = result.private_peak_bytes.max(retained);
            let added = result
                .engine
                .searched_build_nodes
                .saturating_sub(before_nodes);
            let nodes = progress
                .fetch_add(added, Ordering::Relaxed)
                .saturating_add(added);
            control.report_progress("buildup", nodes as u64, None);
        }
        if result.engine.processed_candidate_count - before_count != count {
            return Err(contract_error());
        }
    }
    result.engine.coexisting_retained_bytes = 0;
    Ok(result)
}

fn checked_worker_bytes(workers: &[WorkerResult]) -> Result<u128, WasmExactSearchError> {
    workers
        .iter()
        .try_fold(0_u128, |bytes, worker| {
            bytes.checked_add(worker.checked_retained_bytes()?)
        })
        .ok_or_else(projection_error)
}

fn projection_error() -> WasmExactSearchError {
    WasmExactSearchError::InvalidProblem("extended_pc_family_memory_projection_unavailable")
}

fn contract_error() -> WasmExactSearchError {
    WasmExactSearchError::InvalidProblem("extended_pc_family_parallel_contract_invalid")
}

fn storage_error() -> WasmExactSearchError {
    WasmExactSearchError::InvalidProblem("extended_pc_family_parallel_storage_unavailable")
}

fn pool_error() -> WasmExactSearchError {
    WasmExactSearchError::InvalidProblem("extended_pc_family_worker_pool_unavailable")
}
