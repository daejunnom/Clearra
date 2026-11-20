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

use crate::{
    cpu_worker_pool,
    resource::{ExecutionMemoryOwner, SharedExecutionMemoryBudget},
};

use super::super::extended_geometry::ExtendedParallelGeometryBranch;
use super::{
    mix_digest, Arc, BuildProbabilityAdvance, CoreExecutionResult, ExecutionControl,
    ExtendedBuildProbabilitySession, ExtendedFamilyPurpose, ExtendedGeometryAdvance, HashMap,
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
    #[cfg(test)]
    bounded_rows: Option<BoundedWorkerRows>,
    #[cfg(test)]
    terminal_base: Option<u128>,
}

#[cfg(test)]
const ROW_BLOCK_WIDTH: usize = 16;

// Fixed-width blocks avoid whole-Vec copies during candidate reduction. The
// one-key HashSet is only the immediate producer hand-off; its iterator never
// becomes a terminal work cursor.
#[cfg(test)]
struct WorkerRowBlock {
    next: Option<Box<Self>>,
    keys: [Option<super::ExtendedTilingKey>; ROW_BLOCK_WIDTH],
    used: usize,
}

#[cfg(test)]
#[derive(Default)]
struct BoundedWorkerRows {
    head: Option<Box<WorkerRowBlock>>,
    len: usize,
    retained: u128,
}

#[cfg(test)]
impl BoundedWorkerRows {
    fn growth(&self) -> u128 {
        if self
            .head
            .as_ref()
            .is_none_or(|head| head.used == ROW_BLOCK_WIDTH)
        {
            size_of::<WorkerRowBlock>() as u128
        } else {
            0
        }
    }

    fn nested_bytes(key: &super::ExtendedTilingKey) -> u128 {
        (key.retained_bytes() - size_of::<super::ExtendedTilingKey>()) as u128
    }

    fn push_admitted(&mut self, key: super::ExtendedTilingKey) -> Result<(), WasmExactSearchError> {
        let growth = self.growth();
        let retained = self
            .retained
            .checked_add(growth)
            .and_then(|bytes| bytes.checked_add(Self::nested_bytes(&key)))
            .ok_or_else(projection_error)?;
        let len = self.len.checked_add(1).ok_or_else(projection_error)?;
        if growth != 0 {
            self.head = Some(Box::new(WorkerRowBlock {
                next: self.head.take(),
                keys: std::array::from_fn(|_| None),
                used: 0,
            }));
        }
        let head = self.head.as_mut().ok_or_else(contract_error)?;
        head.keys[head.used] = Some(key);
        head.used += 1;
        self.len = len;
        self.retained = retained;
        Ok(())
    }

    fn last(&self) -> Option<&super::ExtendedTilingKey> {
        let head = self.head.as_ref()?;
        head.keys.get(head.used.checked_sub(1)?)?.as_ref()
    }

    fn pop(&mut self) -> Result<super::ExtendedTilingKey, WasmExactSearchError> {
        let head = self.head.as_mut().ok_or_else(contract_error)?;
        head.used = head.used.checked_sub(1).ok_or_else(contract_error)?;
        let key = head.keys[head.used].take().ok_or_else(contract_error)?;
        self.len = self.len.checked_sub(1).ok_or_else(contract_error)?;
        self.retained = self
            .retained
            .checked_sub(Self::nested_bytes(&key))
            .ok_or_else(projection_error)?;
        if head.used == 0 {
            // This block is empty. Drop just it, not the rest of the chain.
            self.head = head.next.take();
            self.retained = self
                .retained
                .checked_sub(size_of::<WorkerRowBlock>() as u128)
                .ok_or_else(projection_error)?;
        }
        Ok(key)
    }
}

#[cfg(test)]
impl Drop for BoundedWorkerRows {
    fn drop(&mut self) {
        // Cancellation may leave arbitrarily many blocks: never recurse.
        while let Some(mut head) = self.head.take() {
            self.head = head.next.take();
        }
    }
}

impl WorkerResult {
    fn checked_auxiliary_retained_bytes(&self) -> Option<u128> {
        let mut bytes =
            (self.digests.capacity() as u128).checked_mul(size_of::<DigestSegment>() as u128)?;
        for segment in &self.digests {
            bytes = bytes.checked_add(
                (segment.values.capacity() as u128).checked_mul(size_of::<u64>() as u128)?,
            )?;
        }
        #[cfg(test)]
        if let Some(rows) = self.bounded_rows.as_ref() {
            bytes = bytes.checked_add(rows.retained)?;
        }
        Some(bytes)
    }

    fn checked_retained_bytes(&self) -> Option<u128> {
        self.engine
            .checked_retained_bytes()?
            .checked_add(self.checked_auxiliary_retained_bytes()?)
    }

    #[cfg(test)]
    fn take_private_candidate_row(
        &mut self,
        probe: super::ExtendedTilingKey,
    ) -> Result<(), WasmExactSearchError> {
        if self.engine.buildable_tilings.is_empty() {
            return Ok(());
        }
        // The preceding single ordinary Omit call may have produced only
        // the matching exact key. Fail closed on a different producer shape.
        if self.engine.buildable_tilings.len() != 1
            || self.engine.buildable_tilings.capacity() > ROW_BLOCK_WIDTH
        {
            return Err(contract_error());
        }
        let rows = self.bounded_rows.as_mut().ok_or_else(contract_error)?;
        self.engine.ensure_memory_bound(rows.growth())?;
        let row = self
            .engine
            .buildable_tilings
            .take(&probe)
            .ok_or_else(contract_error)?;
        drop(probe);
        rows.push_admitted(row)
    }

    fn memory_owner(&self) -> Result<&ExecutionMemoryOwner, WasmExactSearchError> {
        self.engine
            .private_memory_owner
            .as_ref()
            .ok_or_else(contract_error)
    }

    fn reserve_branch(
        &mut self,
        branch: &ExtendedParallelGeometryBranch,
    ) -> Result<usize, WasmExactSearchError> {
        let count = usize::try_from(branch.candidate_count).map_err(|_| projection_error())?;
        let growth = (count as u128)
            .checked_mul(size_of::<u64>() as u128)
            .and_then(|bytes| {
                bytes.checked_add(
                    (self.digests.len() + 1) as u128 * 2 * size_of::<DigestSegment>() as u128,
                )
            })
            .ok_or_else(projection_error)?;
        let private = self
            .checked_retained_bytes()
            .and_then(|bytes| bytes.checked_add(branch.private_retained_bytes() as u128))
            .and_then(|bytes| bytes.checked_add(growth))
            .ok_or_else(projection_error)?;
        self.memory_owner()?
            .ensure(private)
            .map_err(WasmExactSearchError::resource_admission)?;
        self.digests.try_reserve(1).map_err(|_| storage_error())?;
        let mut values = Vec::new();
        values
            .try_reserve_exact(count)
            .map_err(|_| storage_error())?;
        self.digests.push(DigestSegment {
            first_ordinal: branch.first_ordinal,
            values,
        });
        Ok(self.digests.len() - 1)
    }

    /// Consume one existing candidate unit with the same row identity,
    /// exact BuildUp/supply reducer and private memory accounting in both
    /// the full native loop and a paused cooperative wave.
    fn reduce_next_candidate(
        &mut self,
        branch: &mut ExtendedParallelGeometryBranch,
        digest_index: usize,
        progress: &AtomicUsize,
        control: &ExecutionControl,
    ) -> Result<bool, WasmExactSearchError> {
        let owned = self.checked_retained_bytes().ok_or_else(projection_error)?;
        let Some(candidate) =
            branch.next_candidate_with_owner(&self.engine.catalog, self.memory_owner()?, owned)?
        else {
            return Ok(false);
        };
        let ordinal = branch
            .first_ordinal
            .checked_add(self.digests[digest_index].values.len() as u128)
            .and_then(|ordinal| u64::try_from(ordinal).ok())
            .ok_or_else(projection_error)?;
        let digest_future = (candidate.row_ids().len() as u128)
            .checked_mul(64)
            .and_then(|bytes| bytes.checked_add(128))
            .ok_or_else(projection_error)?;
        self.memory_owner()?
            .ensure(
                owned
                    .checked_add(branch.private_retained_bytes() as u128)
                    .and_then(|bytes| bytes.checked_add(digest_future))
                    .ok_or_else(projection_error)?,
            )
            .map_err(WasmExactSearchError::resource_admission)?;
        let tiling = super::ExtendedTilingKey::from_candidate(&self.engine.catalog, &candidate);
        let digest = tiling.digest();
        #[cfg(not(test))]
        drop(tiling);
        #[cfg(test)]
        let row_probe = self.bounded_rows.is_some().then_some(tiling);
        let before_nodes = self.engine.searched_build_nodes;
        let auxiliary = self
            .checked_auxiliary_retained_bytes()
            .ok_or_else(projection_error)?
            .checked_add(branch.private_retained_bytes() as u128)
            .ok_or_else(projection_error)?;
        #[cfg(test)]
        let auxiliary = auxiliary
            .checked_add(row_probe.as_ref().map_or(0, |key| key.retained_bytes()) as u128)
            .ok_or_else(projection_error)?;
        #[cfg(test)]
        if row_probe.is_some() && !self.engine.buildable_tilings.is_empty() {
            return Err(contract_error());
        }
        self.engine.coexisting_retained_bytes = auxiliary;
        self.engine.ensure_memory_bound(0)?;
        self.engine
            .process_candidate(candidate, Some(ordinal), control)?;
        if let Some(reason) = self.engine.truncated_reason {
            return Err(self.engine.pc_resource_stop_error(reason));
        }
        #[cfg(test)]
        if let Some(key) = row_probe {
            self.take_private_candidate_row(key)?;
            self.engine.coexisting_retained_bytes = self
                .checked_auxiliary_retained_bytes()
                .and_then(|bytes| bytes.checked_add(branch.private_retained_bytes() as u128))
                .ok_or_else(projection_error)?;
        }
        self.digests[digest_index].values.push(digest);
        self.engine.ensure_memory_bound(0)?;
        let retained = self
            .checked_retained_bytes()
            .ok_or_else(projection_error)?
            .checked_add(branch.private_retained_bytes() as u128)
            .ok_or_else(projection_error)?;
        self.private_peak_bytes = self.private_peak_bytes.max(retained);
        let added = self
            .engine
            .searched_build_nodes
            .saturating_sub(before_nodes);
        let nodes = progress
            .fetch_add(added, Ordering::Relaxed)
            .saturating_add(added);
        control.report_progress("buildup", nodes as u64, None);
        Ok(true)
    }

    fn settle_without_branch(&mut self) -> Result<(), WasmExactSearchError> {
        self.engine.coexisting_retained_bytes = 0;
        self.memory_owner()?
            .ensure(self.checked_retained_bytes().ok_or_else(projection_error)?)
            .map_err(WasmExactSearchError::resource_admission)
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
        let mut preparation_steps = 0_usize;
        while self.geometry.is_compiling() {
            if control.is_cancelled() {
                return Err(WasmExactSearchError::Cancelled);
            }
            self.ensure_memory_bound(0)?;
            match self.advance_geometry_bounded()? {
                ExtendedGeometryAdvance::Pending => {}
                ExtendedGeometryAdvance::ResourceIncomplete(reason) => {
                    return Err(self.pc_resource_stop_error(reason));
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
        let workers = requested_workers;
        let branch_count = plan.branches.len();
        let queued_branch_private = plan
            .branches
            .iter()
            .try_fold(0_u128, |bytes, branch| {
                bytes.checked_add(branch.private_retained_bytes() as u128)
            })
            .ok_or_else(projection_error)?;
        let original_coexisting = self.coexisting_retained_bytes;
        let root_retained = self.checked_retained_bytes().ok_or_else(projection_error)?;
        let fixed = root_retained
            .checked_add(original_coexisting)
            .and_then(|bytes| bytes.checked_add(plan.shared_retained_bytes() as u128))
            .and_then(|bytes| {
                bytes.checked_add(
                    (plan.branch_retained_bytes() as u128).checked_sub(queued_branch_private)?,
                )
            })
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
        let private_memory = SharedExecutionMemoryBudget::new(
            self.memory_bound,
            fixed,
            workers.checked_add(1).ok_or_else(projection_error)?,
        )
        .map_err(WasmExactSearchError::resource_admission)?;
        let queued_memory = private_memory.owner(workers);
        queued_memory
            .ensure(queued_branch_private)
            .map_err(WasmExactSearchError::resource_admission)?;

        plan.branches
            .sort_unstable_by_key(|branch| (branch.candidate_count, branch.first_ordinal));
        let mut tasks = Vec::new();
        tasks
            .try_reserve_exact(workers)
            .map_err(|_| storage_error())?;
        // Preserve the admitted request width even for a one-branch family.
        // Slots without initial work may steal a queued branch or remain idle;
        // `parallel_active_workers` separately reports useful participation.
        for index in 0..workers {
            let engine = self.new_shared_pc_verifier_with_resources(
                self.memory_bound,
                Some(private_memory.owner(index)),
            )?;
            engine.ensure_memory_bound(0)?;
            let first = plan.branches.pop();
            if let Some(branch) = first.as_ref() {
                queued_memory
                    .transfer_retained_to(
                        engine
                            .private_memory_owner
                            .as_ref()
                            .ok_or_else(contract_error)?,
                        branch.private_retained_bytes() as u128,
                    )
                    .map_err(WasmExactSearchError::resource_admission)?;
            }
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
            let queued_memory = queued_memory.clone();
            let worker_control = control.clone();
            let sender = sender.clone();
            let queued = cpu_worker_pool::submit_cpu_job(move || {
                let result = caught_worker(
                    engine,
                    first,
                    &queue,
                    &queued_memory,
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
            &queued_memory,
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
        self.native_parallel_peak_bytes = self
            .native_parallel_peak_bytes
            .max(private_memory.admitted_peak_bytes());
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
            let memory_owner = worker
                .engine
                .private_memory_owner
                .clone()
                .ok_or_else(contract_error)?;
            self.coexisting_retained_bytes = fixed
                .checked_sub(root_retained)
                .and_then(|bytes| bytes.checked_add(worker.checked_retained_bytes()?))
                .and_then(|bytes| bytes.checked_add(checked_worker_bytes(&successful).ok()?))
                .ok_or_else(projection_error)?;
            self.merge_pc_verifier(worker.engine)?;
            // The popped worker's remaining scratch/digest owners drop here.
            drop(worker.digests);
            memory_owner
                .release_after_drop()
                .map_err(WasmExactSearchError::resource_admission)?;
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
        match self.complete_with_control(control)? {
            BuildProbabilityAdvance::Completed(result) => Ok(Some(result)),
            _ => Err(contract_error()),
        }
    }

    fn validate_pc_verifier(&self, worker: &Self) -> Result<(), WasmExactSearchError> {
        if worker.purpose != ExtendedFamilyPurpose::Pc
            || !worker.shared_immutable_owners
            || !Arc::ptr_eq(&self.problem, &worker.problem)
            || !Arc::ptr_eq(&self.catalog, &worker.catalog)
            || worker.truncated_reason.is_some()
            || worker.finished
            || (!self.problem.objective().score().requested()
                && !self.problem.objective().execution_constraints().requested()
                && !self
                    .problem
                    .pc_chance_evidence_policy()
                    .retains_pc_path_v2_evidence()
                && !worker.spin_execution_graphs.is_empty())
            || !worker.finesse_languages.is_empty()
        {
            return Err(contract_error());
        }
        Ok(())
    }

    fn merge_pc_verifier(&mut self, mut worker: Self) -> Result<(), WasmExactSearchError> {
        self.validate_pc_verifier(&worker)?;
        match (self.pc_observation.as_ref(), worker.pc_observation.as_ref()) {
            (Some(target), Some(source)) => {
                let future = target
                    .checked_merge_future_bytes(source)
                    .ok_or_else(projection_error)?;
                self.ensure_memory_bound(future)?;
                self.pc_observation
                    .as_mut()
                    .expect("checked observation owner")
                    .merge_from(source)?;
            }
            (None, None) => {}
            _ => return Err(contract_error()),
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
        self.merge_pc_verifier_counters(&worker)?;
        let next = self
            .pc_build_variant_count
            .checked_add(worker.pc_build_variant_count);
        self.pc_build_variant_count = next.unwrap_or(u128::MAX);
        self.pc_build_variant_count_complete &=
            next.is_some() && worker.pc_build_variant_count_complete;
        Ok(())
    }

    fn merge_pc_verifier_counters(&mut self, worker: &Self) -> Result<(), WasmExactSearchError> {
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
        Ok(())
    }
}

fn caught_worker(
    engine: ExtendedBuildProbabilitySession,
    first: Option<ExtendedParallelGeometryBranch>,
    queue: &Mutex<Vec<ExtendedParallelGeometryBranch>>,
    queued_memory: &ExecutionMemoryOwner,
    abort: &AtomicBool,
    progress: &AtomicUsize,
    control: &ExecutionControl,
) -> Result<WorkerResult, WasmExactSearchError> {
    catch_unwind(AssertUnwindSafe(|| {
        run_worker(
            engine,
            first,
            queue,
            queued_memory,
            abort,
            progress,
            control,
        )
    }))
    .map_err(|_| WasmExactSearchError::InvalidProblem("extended_pc_family_worker_panicked"))?
}

fn run_worker(
    engine: ExtendedBuildProbabilitySession,
    first: Option<ExtendedParallelGeometryBranch>,
    queue: &Mutex<Vec<ExtendedParallelGeometryBranch>>,
    queued_memory: &ExecutionMemoryOwner,
    abort: &AtomicBool,
    progress: &AtomicUsize,
    control: &ExecutionControl,
) -> Result<WorkerResult, WasmExactSearchError> {
    let mut result = WorkerResult {
        engine,
        digests: Vec::new(),
        private_peak_bytes: 0,
        #[cfg(test)]
        bounded_rows: None,
        #[cfg(test)]
        terminal_base: None,
    };
    let mut assigned = first;
    loop {
        if control.is_cancelled() || abort.load(Ordering::Acquire) {
            return Err(WasmExactSearchError::Cancelled);
        }
        let next = match assigned.take() {
            Some(branch) => Some(branch),
            None => take_worker_branch(queue, queued_memory, &result)?,
        };
        let Some(mut branch) = next else {
            break;
        };
        let index = result.reserve_branch(&branch)?;
        let before_count = result.engine.processed_candidate_count;
        loop {
            if control.is_cancelled() || abort.load(Ordering::Acquire) {
                return Err(WasmExactSearchError::Cancelled);
            }
            if !result.reduce_next_candidate(&mut branch, index, progress, control)? {
                break;
            }
        }
        if (result.engine.processed_candidate_count - before_count) as u128
            != branch.candidate_count
        {
            return Err(contract_error());
        }
        drop(branch);
        result.settle_without_branch()?;
    }
    result.settle_without_branch()?;
    Ok(result)
}

fn take_worker_branch(
    queue: &Mutex<Vec<ExtendedParallelGeometryBranch>>,
    queued_memory: &ExecutionMemoryOwner,
    worker: &WorkerResult,
) -> Result<Option<ExtendedParallelGeometryBranch>, WasmExactSearchError> {
    let branch = queue.lock().map_err(|_| pool_error())?.pop();
    if let Some(branch) = branch.as_ref() {
        queued_memory
            .transfer_retained_to(
                worker.memory_owner()?,
                branch.private_retained_bytes() as u128,
            )
            .map_err(WasmExactSearchError::resource_admission)?;
    }
    Ok(branch)
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

// A private foundation: only a conformance owner drives these waves for now.
// The full native path remains the only product entry.
#[cfg(test)]
pub(in crate::backend::wasm_cpu) mod cooperative {
    use super::*;
    use crate::backend::wasm_cpu::extended_geometry::{
        ExtendedGeometrySearch, ExtendedParallelGeometryPlan, ExtendedParallelPreparation,
        ExtendedPreparationStatus,
    };
    use crate::backend::wasm_cpu::extended_pc_search::{
        checked_pc_family_problem_nested_retained_bytes, validate_pc_family_problem,
    };
    use crate::resource::{admit_budget_bound_search_execution, ExecutionAdmission};
    use clearra_core_domain::{
        board::standard_pc_board::Board256Mask, objective::objective_kind::ObjectiveKind,
        resource::ResourceLeaseToken,
    };
    use clearra_pc_graph::request::PcCountPolicy;
    use clearra_problem::{BuildProbabilityField, SearchOutputPolicy, SearchProblem};

    use super::cooperative_canonical::{CanonicalReduction, SourceKey};

    pub(super) struct WorkerCursor {
        result: WorkerResult,
        branch: Option<ExtendedParallelGeometryBranch>,
        digest: Option<(usize, usize)>,
        done: bool,
    }

    impl WorkerCursor {
        pub(super) fn new(
            engine: ExtendedBuildProbabilitySession,
            first: Option<ExtendedParallelGeometryBranch>,
        ) -> Self {
            Self {
                result: WorkerResult {
                    engine,
                    digests: Vec::new(),
                    private_peak_bytes: 0,
                    bounded_rows: None,
                    terminal_base: None,
                },
                branch: first,
                digest: None,
                done: false,
            }
        }

        fn retain_bounded_rows(mut self) -> Self {
            self.result.bounded_rows = Some(BoundedWorkerRows::default());
            self
        }

        fn advance(
            &mut self,
            credit: usize,
            shared: &WaveShared,
            control: &ExecutionControl,
        ) -> Result<usize, WasmExactSearchError> {
            let mut used = 0;
            while used < credit && !self.done {
                if control.is_cancelled() || shared.abort.load(Ordering::Acquire) {
                    return Err(WasmExactSearchError::Cancelled);
                }
                used += 1;
                if self.branch.is_none() {
                    self.branch =
                        take_worker_branch(&shared.queue, &shared.queued_memory, &self.result)?;
                }
                let Some(branch) = self.branch.as_mut() else {
                    self.result.settle_without_branch()?;
                    self.done = true;
                    break;
                };
                let Some((index, before_count)) = self.digest else {
                    self.digest = Some((
                        self.result.reserve_branch(branch)?,
                        self.result.engine.processed_candidate_count,
                    ));
                    continue;
                };
                if !self
                    .result
                    .reduce_next_candidate(branch, index, &shared.progress, control)?
                {
                    let count = self
                        .result
                        .engine
                        .processed_candidate_count
                        .checked_sub(before_count)
                        .ok_or_else(contract_error)?;
                    if count as u128 != branch.candidate_count {
                        return Err(contract_error());
                    }
                    self.branch = None;
                    self.digest = None;
                    self.result.settle_without_branch()?;
                }
            }
            if control.is_cancelled() || shared.abort.load(Ordering::Acquire) {
                return Err(WasmExactSearchError::Cancelled);
            }
            Ok(used)
        }
    }

    #[derive(Clone)]
    struct WaveShared {
        queue: Arc<Mutex<Vec<ExtendedParallelGeometryBranch>>>,
        queued_memory: ExecutionMemoryOwner,
        progress: Arc<AtomicUsize>,
        abort: Arc<AtomicBool>,
    }

    impl WaveShared {
        fn run(
            self,
            mut cursor: WorkerCursor,
            credit: usize,
            control: ExecutionControl,
        ) -> WorkerCompletion {
            let work = catch_unwind(AssertUnwindSafe(|| cursor.advance(credit, &self, &control)))
                .unwrap_or(Err(WasmExactSearchError::InvalidProblem(
                    "extended_pc_family_worker_panicked",
                )));
            if work.is_err() {
                self.abort.store(true, Ordering::Release);
            }
            // Drop queue/control clones before handing the sole live cursor
            // (or an error with no cursor) back to the coordinator.
            drop(self);
            drop(control);
            work.map(|used| (cursor, used))
        }
    }

    type WorkerCompletion = Result<(WorkerCursor, usize), WasmExactSearchError>;
    type TaggedCompletion = (usize, WorkerCompletion);

    struct DrainWave<'a> {
        _admission: &'a ExecutionAdmission,
        abort: Arc<AtomicBool>,
        sender: Option<mpsc::Sender<TaggedCompletion>>,
        receiver: mpsc::Receiver<TaggedCompletion>,
        submitted: usize,
    }

    impl<'a> DrainWave<'a> {
        fn new(admission: &'a ExecutionAdmission, abort: Arc<AtomicBool>) -> Self {
            let (sender, receiver) = mpsc::channel();
            Self {
                _admission: admission,
                abort,
                sender: Some(sender),
                receiver,
                submitted: 0,
            }
        }

        fn receive(&mut self) -> Option<Result<TaggedCompletion, WasmExactSearchError>> {
            if self.submitted == 0 {
                return None;
            }
            self.submitted -= 1;
            Some(self.receiver.recv().map_err(|_| pool_error()))
        }
    }

    impl Drop for DrainWave<'_> {
        fn drop(&mut self) {
            // Every successfully enqueued closure must end before the wave
            // returns, including an early error or coordinator unwind.
            if self.submitted != 0 {
                self.abort.store(true, Ordering::Release);
                self.sender = None;
                while self.receive().is_some() {}
            }
        }
    }

    pub(super) struct WorkerWaves {
        slots: Vec<Option<WorkerCursor>>,
        queue: Arc<Mutex<Vec<ExtendedParallelGeometryBranch>>>,
        queued_memory: ExecutionMemoryOwner,
        progress: Arc<AtomicUsize>,
        lease: ResourceLeaseToken,
        next_slot: usize,
        failed: bool,
    }

    impl WorkerWaves {
        pub(super) fn checked_fixed_backing_bytes(width: usize) -> Option<u128> {
            // Slot backing remains allocated after move-out. Also charge the
            // pool job, the completion channel, and per-wave control owners.
            (width as u128)
                .checked_mul(
                    (size_of::<Option<WorkerCursor>>() + 3 * size_of::<TaggedCompletion>() + 2048)
                        as u128,
                )?
                .checked_add(size_of::<Self>() as u128 + 512)
        }

        /// The coordinator has already admitted all slots and any initially
        /// assigned branch, using the same credits as the direct native run.
        pub(super) fn new(
            admission: &ExecutionAdmission,
            slots: Vec<Option<WorkerCursor>>,
            queue: Vec<ExtendedParallelGeometryBranch>,
            queued_memory: ExecutionMemoryOwner,
        ) -> Result<Self, WasmExactSearchError> {
            let lease = admission.lease_token();
            let width = slots.len();
            if width < 2
                || lease.grant().compute_units as usize != width
                || slots.iter().any(Option::is_none)
            {
                return Err(contract_error());
            }
            let pool = cpu_worker_pool::ensure_cpu_workers(width).map_err(|_| pool_error())?;
            if pool.total_workers() != width {
                return Err(pool_error());
            }
            Ok(Self {
                slots,
                queue: Arc::new(Mutex::new(queue)),
                queued_memory,
                progress: Arc::new(AtomicUsize::new(0)),
                lease,
                next_slot: 0,
                failed: false,
            })
        }

        pub(super) fn advance(
            &mut self,
            admission: &ExecutionAdmission,
            budget: usize,
            control: &ExecutionControl,
        ) -> Result<(bool, usize), WasmExactSearchError> {
            if self.failed
                || admission.lease_token() != self.lease
                || control.partition().count() != 1
            {
                return Err(contract_error());
            }
            if control.is_cancelled() {
                self.failed = true;
                return Err(WasmExactSearchError::Cancelled);
            }
            let live = self
                .slots
                .iter()
                .filter(|slot| slot.as_ref().is_some_and(|cursor| !cursor.done))
                .count();
            let active = live.min(budget);
            if active == 0 {
                return Ok((live == 0, 0));
            }
            // Retain all admitted slots. Low per-call credit gives one
            // unit to a rotating subset; later calls can use the full width.
            let width = self.slots.len();
            let start = self.next_slot;
            self.failed = true;
            let abort = Arc::new(AtomicBool::new(false));
            let mut wave = DrainWave::new(admission, Arc::clone(&abort));
            let shared = WaveShared {
                queue: Arc::clone(&self.queue),
                queued_memory: self.queued_memory.clone(),
                progress: Arc::clone(&self.progress),
                abort: Arc::clone(&abort),
            };
            let mut caller = None;
            let mut assigned = 0;
            for offset in 0..width {
                let index = (start + offset) % width;
                if self.slots[index].as_ref().is_none_or(|cursor| cursor.done) {
                    continue;
                }
                let credit = budget / active + usize::from(assigned < budget % active);
                let cursor = self.slots[index].take().ok_or_else(contract_error)?;
                if caller.is_none() {
                    caller = Some((index, cursor, credit));
                } else {
                    let work = shared.clone();
                    let worker_control = control.clone();
                    let sender = wave.sender.as_ref().ok_or_else(contract_error)?.clone();
                    cpu_worker_pool::submit_cpu_job(move || {
                        let result = work.run(cursor, credit, worker_control);
                        let _ = sender.send((index, result));
                    })
                    .map_err(|_| pool_error())?;
                    wave.submitted += 1;
                }
                assigned += 1;
                self.next_slot = (index + 1) % width;
                if assigned == active {
                    break;
                }
            }
            wave.sender = None;
            let (index, cursor, credit) = caller.ok_or_else(contract_error)?;
            let caller = shared.run(cursor, credit, control.clone());
            let mut used = 0;
            let mut error = None;
            self.accept_completion(Ok((index, caller)), &mut used, &mut error);
            while let Some(result) = wave.receive() {
                self.accept_completion(result, &mut used, &mut error);
            }
            if let Some(error) = error {
                return Err(error);
            }
            if control.is_cancelled() {
                return Err(WasmExactSearchError::Cancelled);
            }
            if used > budget {
                return Err(contract_error());
            }
            self.failed = false;
            let done = self
                .slots
                .iter()
                .all(|slot| slot.as_ref().is_some_and(|cursor| cursor.done));
            Ok((done, used))
        }

        fn accept_completion(
            &mut self,
            result: Result<TaggedCompletion, WasmExactSearchError>,
            used: &mut usize,
            error: &mut Option<WasmExactSearchError>,
        ) {
            let result = result.and_then(|(index, completion)| {
                let (cursor, steps) = completion?;
                let slot = self.slots.get_mut(index).ok_or_else(contract_error)?;
                if slot.is_some() {
                    return Err(contract_error());
                }
                *used = used.checked_add(steps).ok_or_else(projection_error)?;
                *slot = Some(cursor);
                Ok(())
            });
            if let Err(next) = result {
                if error.is_none() || matches!(error, Some(WasmExactSearchError::Cancelled)) {
                    *error = Some(next);
                }
            }
        }

        pub(super) fn into_completed_slots(self) -> Result<CompletedSlots, WasmExactSearchError> {
            if self.failed
                || self
                    .slots
                    .iter()
                    .any(|slot| slot.as_ref().is_none_or(|cursor| !cursor.done))
            {
                return Err(contract_error());
            }
            Ok(CompletedSlots(self.slots.into_iter()))
        }
    }

    pub(super) struct CompletedSlots(std::vec::IntoIter<Option<WorkerCursor>>);

    impl Iterator for CompletedSlots {
        type Item = WorkerResult;

        fn next(&mut self) -> Option<Self::Item> {
            self.0.next().flatten().map(|cursor| cursor.result)
        }
    }

    #[derive(Clone, Copy, Eq, PartialEq)]
    enum DryPhase {
        Prepare,
        AdmitPlan,
        IndexRange,
        AdmitWorkerBacking,
        AdmitSlot,
        StartWaves,
        Wave,
        CollectSlots,
        Bind,
        Fold,
        SettleCanonicalSources,
        AdmitCanonical,
        ImportCanonical,
        FoldCanonical,
        SealVerification,
        SerialHandoff,
        Complete,
        Failed,
    }

    struct Range {
        first: u128,
        count: u128,
        returned: Option<DigestSegment>,
    }

    struct WorkerBinding {
        worker: WorkerResult,
        count: u128,
        representative_bound: bool,
    }

    #[derive(Clone, Copy)]
    struct RepresentativeRank {
        worker_index: usize,
        ordinal: u64,
    }

    #[derive(Default)]
    struct VerificationFold {
        count_all: u128,
        coverage_word: u64,
        representative: Option<RepresentativeRank>,
    }

    impl VerificationFold {
        fn merge_worker(
            &mut self,
            slot: usize,
            binding: &WorkerBinding,
            depth: usize,
        ) -> Result<(), WasmExactSearchError> {
            let worker = &binding.worker;
            let engine = &worker.engine;
            if engine.covered_patterns.pattern_count() != 1
                || !engine.pc_build_variant_count_complete
            {
                return Err(contract_error());
            }
            // Do not populate PatternBitSet's lazy dense cache. This retains
            // the original supplied ordinal, not a successful-queue subset.
            let word = engine.covered_patterns.word_at(0);
            let rows = worker.bounded_rows.as_ref().ok_or_else(contract_error)?;
            let count = engine.pc_build_variant_count;
            let rank = match (
                word,
                engine.representative_rank,
                engine.representative_pattern_id,
            ) {
                (0, None, None)
                    if count == 0 && rows.len == 0 && engine.representative_path.is_empty() =>
                {
                    None
                }
                (1, Some(ordinal), Some(0))
                    if binding.representative_bound
                        && count != 0
                        && rows.len != 0
                        && engine.representative_path.len() == depth =>
                {
                    Some(RepresentativeRank {
                        worker_index: slot,
                        ordinal,
                    })
                }
                _ => return Err(contract_error()),
            };
            self.count_all = self
                .count_all
                .checked_add(count)
                .ok_or_else(contract_error)?;
            self.coverage_word |= word;
            if let Some(rank) = rank {
                if self
                    .representative
                    .is_none_or(|current| rank.ordinal < current.ordinal)
                {
                    self.representative = Some(rank);
                }
            }
            Ok(())
        }
    }

    pub(super) struct VerifiedStats<'a> {
        pub(super) count_all: u128,
        pub(super) coverage_word: u64,
        pub(super) representative: Option<(u64, u32, &'a [crate::CorePathStep])>,
    }

    pub(super) struct CompletedFacts<'a> {
        pub(super) digest: u64,
        pub(super) candidate_count: u128,
        pub(super) workers: &'a [WorkerResult],
        pub(super) keys: &'a [String],
        pub(super) hash: &'a str,
        pub(super) verification: VerifiedStats<'a>,
    }

    pub(in crate::backend::wasm_cpu) struct SerialHandoff {
        pub(in crate::backend::wasm_cpu) engine: ExtendedBuildProbabilitySession,
        pub(in crate::backend::wasm_cpu) admission: ExecutionAdmission,
    }

    pub(in crate::backend::wasm_cpu) struct ParallelTerminal {
        // The published result and its allocation account drop before the
        // exact original admission. It is not a new execution or new lease.
        result: CoreExecutionResult,
        _backing: ExecutionMemoryOwner,
        admission: ExecutionAdmission,
    }

    pub(in crate::backend::wasm_cpu) struct ParallelTerminalRetention {
        _backing: ExecutionMemoryOwner,
        admission: ExecutionAdmission,
    }

    impl ParallelTerminalRetention {
        pub(in crate::backend::wasm_cpu) fn lease_token(&self) -> ResourceLeaseToken {
            self.admission.lease_token()
        }
    }

    impl ParallelTerminal {
        pub(in crate::backend::wasm_cpu) fn into_result_and_retention(
            self,
        ) -> (CoreExecutionResult, ParallelTerminalRetention) {
            let retention = ParallelTerminalRetention {
                _backing: self._backing,
                admission: self.admission,
            };
            (self.result, retention)
        }
    }

    /// Test-private exact owner. Proof facts remain borrowed; publication is
    /// an explicit consuming operation under the same admission.
    pub(in crate::backend::wasm_cpu) struct DryRunOwner {
        // Move-out owners precede the admission in destruction order.
        preparation: Option<ExtendedParallelPreparation>,
        plan: Option<ExtendedParallelGeometryPlan>,
        admitted_memory: Option<Arc<SharedExecutionMemoryBudget>>,
        slots: Vec<Option<WorkerCursor>>,
        waves: Option<WorkerWaves>,
        completed_slots: Option<CompletedSlots>,
        active_worker: Option<WorkerBinding>,
        workers: Vec<WorkerResult>,
        ranges: Vec<Range>,
        canonical: Option<CanonicalReduction>,
        verification: VerificationFold,
        source_rows: usize,
        row_worker: usize,
        phase: DryPhase,
        width: usize,
        candidate_count: u128,
        queued_private: u128,
        bound_candidates: u128,
        folded_candidates: u128,
        fold_range: usize,
        fold_offset: usize,
        digest: u64,
        root: ExtendedBuildProbabilitySession,
        admission: ExecutionAdmission,
    }

    impl DryRunOwner {
        pub(in crate::backend::wasm_cpu) fn is_eligible(
            problem: &SearchProblem,
            field: BuildProbabilityField,
        ) -> bool {
            let width = problem.backend_policy().workers();
            if problem.visible_height() != 7
                || field.height() != 7
                || width < 2
                || problem.objective().kind() != ObjectiveKind::All
                || problem.count_policy() != PcCountPolicy::CountAll
                || !matches!(
                    problem.output_policy(),
                    SearchOutputPolicy::Summary | SearchOutputPolicy::Trace
                )
                || problem
                    .piece_source()
                    .materialized_universe()
                    .is_none_or(|universe| !universe.complete() || universe.pattern_count() != 1)
                || problem
                    .queue_observation_policy()
                    .requires_observation_policy()
                || problem.objective().score().requested()
                || problem.objective().execution_constraints().requested()
                || problem.solution_probability_policy().requested()
                || problem.pc_chance_evidence_policy()
                    != clearra_problem::PcChanceEvidencePolicy::Disabled
                || problem
                    .core_query()
                    .remaining_queue()
                    .as_fixed_sequence()
                    .is_none()
                || validate_pc_family_problem(problem).is_err()
            {
                return false;
            }
            let initial = Board256Mask::from_words(problem.initial_board().occupied_words());
            let Ok(full) = Board256Mask::all_cells(70) else {
                return false;
            };
            let required = full.without(initial);
            let cells = required.count_ones() as usize;
            initial.fits_cell_count(70).unwrap_or(false)
                && field.base_words() == initial.words()
                && field.target_words() == required.words()
                && cells != 0
                && cells.is_multiple_of(4)
                && problem.exact_pieces() == Some(cells / 4)
        }

        pub(super) fn new(
            problem: &SearchProblem,
            field: BuildProbabilityField,
        ) -> Result<Self, WasmExactSearchError> {
            if !Self::is_eligible(problem, field) {
                return Err(contract_error());
            }
            let width = problem.backend_policy().workers();
            let admission = admit_budget_bound_search_execution(problem, width)
                .map_err(WasmExactSearchError::resource_admission)?;
            let external = checked_pc_family_problem_nested_retained_bytes(problem)
                .and_then(|bytes| bytes.checked_add(size_of::<SearchProblem>() as u128))
                .ok_or_else(projection_error)?;
            let root = ExtendedBuildProbabilitySession::new_pc_family(
                problem,
                field,
                admission.memory_bound(),
                external,
            )?;
            let expected_lease = admission.lease_token();
            Self::from_admitted(root, admission, expected_lease)
        }

        /// The caller must retain the original token at engine construction,
        /// not derive an expected token from a later, interchangeable grant.
        /// Reject unsupported type/state before replacing its Geometry owner.
        pub(in crate::backend::wasm_cpu) fn from_admitted(
            mut root: ExtendedBuildProbabilitySession,
            admission: ExecutionAdmission,
            expected_lease: ResourceLeaseToken,
        ) -> Result<Self, WasmExactSearchError> {
            let width = root.problem.backend_policy().workers();
            let lease = admission.lease_token();
            if !Self::is_eligible(&root.problem, root.field)
                || lease != expected_lease
                || lease.grant().compute_units as usize != width
                || root.memory_bound != admission.memory_bound()
                || root.purpose != ExtendedFamilyPurpose::Pc
                || root.shared_immutable_owners
                || root.external_geometry
                || root.trivial_target
                || root.finished
                || root.truncated_reason.is_some()
                || root.processed_candidate_count != 0
                || root.geometry.candidate_count() != 0
                || root.geometry.expanded_nodes() != 0
                || !root.geometry.is_compiling()
                || !root.supply_projection_complete
                || root.solution_coverage.is_some()
                || root.pc_observation.is_some()
            {
                return Err(contract_error());
            }
            let preparation = ExtendedParallelPreparation::new(
                core::mem::replace(
                    &mut root.geometry,
                    ExtendedGeometrySearch::private_verifier_placeholder(),
                ),
                Arc::clone(&root.catalog),
                width.saturating_mul(4),
            )?;
            Ok(Self {
                preparation: Some(preparation),
                plan: None,
                admitted_memory: None,
                slots: Vec::new(),
                waves: None,
                completed_slots: None,
                active_worker: None,
                workers: Vec::new(),
                ranges: Vec::new(),
                canonical: None,
                verification: VerificationFold::default(),
                source_rows: 0,
                row_worker: 0,
                phase: DryPhase::Prepare,
                width,
                candidate_count: 0,
                queued_private: 0,
                bound_candidates: 0,
                folded_candidates: 0,
                fold_range: 0,
                fold_offset: 0,
                digest: 0,
                root,
                admission,
            })
        }

        pub(in crate::backend::wasm_cpu) fn advance(
            &mut self,
            budget: usize,
            control: &ExecutionControl,
        ) -> Result<(bool, usize), WasmExactSearchError> {
            let result = self.advance_inner(budget, control);
            if result.is_err() {
                self.phase = DryPhase::Failed;
            }
            result
        }

        fn advance_inner(
            &mut self,
            budget: usize,
            control: &ExecutionControl,
        ) -> Result<(bool, usize), WasmExactSearchError> {
            if self.phase == DryPhase::Failed || control.partition().count() != 1 {
                return Err(contract_error());
            }
            let mut remaining = budget;
            loop {
                if control.is_cancelled() {
                    return Err(WasmExactSearchError::Cancelled);
                }
                let terminal = matches!(self.phase, DryPhase::Complete | DryPhase::SerialHandoff);
                if terminal || remaining == 0 {
                    return Ok((terminal, budget - remaining));
                }
                match self.phase {
                    DryPhase::Prepare => {
                        let outside = self
                            .root
                            .checked_retained_bytes_with_coexisting_owners()
                            .and_then(|bytes| bytes.checked_add(size_of::<Self>() as u128))
                            .ok_or_else(projection_error)?;
                        let (status, used) = self
                            .preparation
                            .as_mut()
                            .ok_or_else(contract_error)?
                            .advance(
                                self.root.request_node_budget.as_deref(),
                                self.root.memory_bound,
                                outside,
                                remaining,
                                control,
                            )?;
                        remaining = remaining.checked_sub(used).ok_or_else(contract_error)?;
                        match status {
                            ExtendedPreparationStatus::Pending => break,
                            ExtendedPreparationStatus::Cancelled => {
                                return Err(WasmExactSearchError::Cancelled)
                            }
                            ExtendedPreparationStatus::ResourceIncomplete(reason) => {
                                return Err(self.root.pc_resource_stop_error(reason));
                            }
                            ExtendedPreparationStatus::Ready => self.phase = DryPhase::AdmitPlan,
                        }
                    }
                    DryPhase::Wave => {
                        let (done, used) = self
                            .waves
                            .as_mut()
                            .ok_or_else(contract_error)?
                            .advance(&self.admission, remaining, control)?;
                        remaining = remaining.checked_sub(used).ok_or_else(contract_error)?;
                        if !done {
                            break;
                        }
                        self.phase = DryPhase::CollectSlots;
                    }
                    DryPhase::ImportCanonical => {
                        self.import_canonical_step(&mut remaining, control)?;
                    }
                    DryPhase::FoldCanonical => {
                        if self
                            .canonical
                            .as_mut()
                            .ok_or_else(contract_error)?
                            .advance_output(&mut remaining, control)?
                        {
                            self.phase = DryPhase::SealVerification;
                        }
                    }
                    _ => {
                        // Every carrier transfer, branch registration, worker
                        // admission and terminal digest update spends a unit.
                        remaining -= 1;
                        self.advance_owner_step()?;
                    }
                }
            }
            if control.is_cancelled() {
                return Err(WasmExactSearchError::Cancelled);
            }
            Ok((false, budget - remaining))
        }

        fn advance_owner_step(&mut self) -> Result<(), WasmExactSearchError> {
            match self.phase {
                DryPhase::AdmitPlan => self.admit_plan(),
                DryPhase::IndexRange => {
                    let plan = self.plan.as_ref().ok_or_else(contract_error)?;
                    if let Some(branch) = plan.branches.get(self.ranges.len()) {
                        let first = self
                            .ranges
                            .last()
                            .map_or(Some(0), |previous| {
                                previous.first.checked_add(previous.count)
                            })
                            .ok_or_else(projection_error)?;
                        if first != branch.first_ordinal || branch.candidate_count == 0 {
                            return Err(contract_error());
                        }
                        self.ranges.push(Range {
                            first,
                            count: branch.candidate_count,
                            returned: None,
                        });
                        self.queued_private = self
                            .queued_private
                            .checked_add(branch.private_retained_bytes() as u128)
                            .ok_or_else(projection_error)?;
                    } else {
                        self.phase = DryPhase::AdmitWorkerBacking;
                    }
                    Ok(())
                }
                DryPhase::AdmitWorkerBacking => self.admit_worker_backing(),
                DryPhase::AdmitSlot => {
                    let aggregate = self.admitted_memory.as_ref().ok_or_else(contract_error)?;
                    let worker = self.root.new_shared_pc_verifier_with_resources(
                        self.root.memory_bound,
                        Some(aggregate.owner(self.slots.len())),
                    )?;
                    let first = self
                        .plan
                        .as_mut()
                        .ok_or_else(contract_error)?
                        .branches
                        .pop();
                    if let Some(branch) = first.as_ref() {
                        aggregate
                            .owner(self.width)
                            .transfer_retained_to(
                                worker
                                    .private_memory_owner
                                    .as_ref()
                                    .ok_or_else(contract_error)?,
                                branch.private_retained_bytes() as u128,
                            )
                            .map_err(WasmExactSearchError::resource_admission)?;
                    }
                    self.slots
                        .push(Some(WorkerCursor::new(worker, first).retain_bounded_rows()));
                    if self.slots.len() == self.width {
                        self.phase = DryPhase::StartWaves;
                    }
                    Ok(())
                }
                DryPhase::StartWaves => {
                    let queued = self
                        .admitted_memory
                        .as_ref()
                        .ok_or_else(contract_error)?
                        .owner(self.width);
                    let queue = core::mem::take(
                        &mut self.plan.as_mut().ok_or_else(contract_error)?.branches,
                    );
                    self.waves = Some(WorkerWaves::new(
                        &self.admission,
                        core::mem::take(&mut self.slots),
                        queue,
                        queued,
                    )?);
                    self.phase = DryPhase::Wave;
                    Ok(())
                }
                DryPhase::CollectSlots => {
                    self.completed_slots = Some(
                        self.waves
                            .take()
                            .ok_or_else(contract_error)?
                            .into_completed_slots()?,
                    );
                    self.phase = DryPhase::Bind;
                    Ok(())
                }
                DryPhase::Bind => self.bind_step(),
                DryPhase::Fold => self.fold_step(),
                DryPhase::SettleCanonicalSources => self.settle_canonical_source(),
                DryPhase::AdmitCanonical => self.admit_canonical_cursor(),
                DryPhase::SealVerification => self.seal_verification(),
                _ => Err(contract_error()),
            }
        }

        fn admit_plan(&mut self) -> Result<(), WasmExactSearchError> {
            let preparation = self.preparation.take().ok_or_else(contract_error)?;
            let ready_upper = preparation
                .ready_retained_upper_bound()
                .ok_or_else(contract_error)?;
            let (geometry, plan) = preparation.into_result()?;
            self.root.geometry = geometry;
            let Some(plan) = plan else {
                // Geometry never yielded a parallel owner. Preserve its
                // exact serial state and this same request's lease instead of
                // manufacturing a parallel family with zero solutions.
                self.phase = DryPhase::SerialHandoff;
                return Ok(());
            };
            let limit = self.root.problem.backend_request().max_candidates();
            if limit != 0 && plan.candidate_count > limit as u128 {
                return Err(self
                    .root
                    .pc_resource_stop_error("candidate_budget_exceeded"));
            }
            let ranges = plan.branches.len();
            // Forward the preparation's already checked allocation envelope
            // before walking the concrete branch-owner credits incrementally.
            let observed = self
                .root
                .checked_retained_bytes_with_coexisting_owners()
                .and_then(|n| n.checked_add(ready_upper))
                .and_then(|n| n.checked_add(size_of::<Self>() as u128))
                .ok_or_else(projection_error)?;
            self.root
                .memory_bound
                .ensure(observed, (ranges as u128) * size_of::<Range>() as u128)
                .map_err(WasmExactSearchError::resource_admission)?;
            self.ranges
                .try_reserve_exact(ranges)
                .map_err(|_| storage_error())?;
            self.candidate_count = plan.candidate_count;
            self.root.parallel_minimum_worker_candidates = usize::MAX;
            self.plan = Some(plan);
            self.phase = DryPhase::IndexRange;
            Ok(())
        }

        fn admit_worker_backing(&mut self) -> Result<(), WasmExactSearchError> {
            let plan = self.plan.as_ref().ok_or_else(contract_error)?;
            // Count the retained immutable plan once. The count/split
            // scratch is already dropped before granting worker backing.
            let fixed = self
                .root
                .checked_retained_bytes_with_coexisting_owners()
                .and_then(|n| n.checked_add(plan.shared_retained_bytes() as u128))
                .and_then(|n| {
                    n.checked_add(
                        (plan.branches.capacity() as u128)
                            * size_of::<ExtendedParallelGeometryBranch>() as u128,
                    )
                })
                .and_then(|n| n.checked_add(WorkerWaves::checked_fixed_backing_bytes(self.width)?))
                .and_then(|n| {
                    n.checked_add((self.ranges.capacity() as u128) * size_of::<Range>() as u128)
                })
                .and_then(|n| {
                    n.checked_add((self.width as u128) * size_of::<WorkerResult>() as u128)
                })
                .and_then(|n| n.checked_add(size_of::<Self>() as u128))
                .ok_or_else(projection_error)?;
            let admitted = SharedExecutionMemoryBudget::new(
                self.root.memory_bound,
                fixed,
                self.width.checked_add(2).ok_or_else(projection_error)?,
            )
            .map_err(WasmExactSearchError::resource_admission)?;
            admitted
                .owner(self.width)
                .ensure(self.queued_private)
                .map_err(WasmExactSearchError::resource_admission)?;
            self.slots
                .try_reserve_exact(self.width)
                .map_err(|_| storage_error())?;
            self.workers
                .try_reserve_exact(self.width)
                .map_err(|_| storage_error())?;
            self.admitted_memory = Some(admitted);
            self.phase = DryPhase::AdmitSlot;
            Ok(())
        }

        fn bind_step(&mut self) -> Result<(), WasmExactSearchError> {
            if let Some(mut binding) = self.active_worker.take() {
                if let Some(segment) = binding.worker.digests.pop() {
                    let index = self
                        .ranges
                        .binary_search_by_key(&segment.first_ordinal, |range| range.first)
                        .map_err(|_| contract_error())?;
                    let range = &mut self.ranges[index];
                    let n = segment.values.len() as u128;
                    if range.returned.is_some() || range.count != n {
                        return Err(contract_error());
                    }
                    let end = range.first.checked_add(n).ok_or_else(projection_error)?;
                    if binding
                        .worker
                        .engine
                        .representative_rank
                        .is_some_and(|ordinal| (range.first..end).contains(&u128::from(ordinal)))
                    {
                        if binding.representative_bound {
                            return Err(contract_error());
                        }
                        binding.representative_bound = true;
                    }
                    binding.count = binding.count.checked_add(n).ok_or_else(projection_error)?;
                    range.returned = Some(segment);
                    self.active_worker = Some(binding);
                } else {
                    let worker = &binding.worker;
                    if binding.count != worker.engine.processed_candidate_count as u128 {
                        return Err(contract_error());
                    }
                    let rows = worker.bounded_rows.as_ref().ok_or_else(contract_error)?;
                    if rows.len as u128 > binding.count
                        || !worker.engine.buildable_tilings.is_empty()
                        || !worker.engine.supply_projection_complete
                        || !worker.engine.pc_build_variant_count_complete
                    {
                        return Err(contract_error());
                    }
                    self.source_rows = self
                        .source_rows
                        .checked_add(rows.len)
                        .ok_or_else(projection_error)?;
                    self.bound_candidates = self
                        .bound_candidates
                        .checked_add(binding.count)
                        .ok_or_else(projection_error)?;
                    self.verification.merge_worker(
                        self.workers.len(),
                        &binding,
                        self.root
                            .problem
                            .exact_pieces()
                            .ok_or_else(contract_error)?,
                    )?;
                    self.root
                        .merge_pc_verifier_counters(&binding.worker.engine)?;
                    self.workers.push(binding.worker);
                }
            } else if let Some(worker) = self
                .completed_slots
                .as_mut()
                .ok_or_else(contract_error)?
                .next()
            {
                self.root.validate_pc_verifier(&worker.engine)?;
                self.active_worker = Some(WorkerBinding {
                    worker,
                    count: 0,
                    representative_bound: false,
                });
            } else {
                if self.workers.len() != self.width || self.bound_candidates != self.candidate_count
                {
                    return Err(contract_error());
                }
                self.phase = DryPhase::Fold;
            }
            // Segment backing moves, never clones. The original worker slot
            // stays charged until the segments are dropped after ordinal fold.
            Ok(())
        }

        fn fold_step(&mut self) -> Result<(), WasmExactSearchError> {
            if let Some(range) = self.ranges.get_mut(self.fold_range) {
                let segment = range.returned.as_ref().ok_or_else(contract_error)?;
                if let Some(value) = segment.values.get(self.fold_offset) {
                    if range.first.checked_add(self.fold_offset as u128)
                        != Some(self.folded_candidates)
                    {
                        return Err(contract_error());
                    }
                    self.digest = mix_digest(self.digest, *value);
                    self.folded_candidates = self
                        .folded_candidates
                        .checked_add(1)
                        .ok_or_else(projection_error)?;
                    self.fold_offset += 1;
                } else {
                    drop(range.returned.take());
                    self.fold_range += 1;
                    self.fold_offset = 0;
                }
            } else {
                if self.folded_candidates != self.candidate_count {
                    return Err(contract_error());
                }
                self.root.geometry.complete_parallel_enumeration(
                    usize::try_from(self.candidate_count).map_err(|_| projection_error())?,
                )?;
                self.phase = DryPhase::SettleCanonicalSources;
            }
            Ok(())
        }

        fn settle_canonical_source(&mut self) -> Result<(), WasmExactSearchError> {
            let Some(worker) = self.workers.get_mut(self.row_worker) else {
                self.row_worker = 0;
                self.phase = DryPhase::AdmitCanonical;
                return Ok(());
            };
            let rows = worker.bounded_rows.as_ref().ok_or_else(contract_error)?;
            if !worker.digests.is_empty()
                || !worker.engine.buildable_tilings.is_empty()
                || worker.engine.pc_observation.is_some()
                || worker.engine.solution_coverage.is_some()
                || !worker.engine.spin_execution_graphs.is_empty()
                || !worker.engine.finesse_languages.is_empty()
            {
                return Err(contract_error());
            }
            // All pool jobs and range bindings are finished. No later phase
            // calls this worker's solver; retain only proof/witness data. The
            // replacement constructors allocate nothing.
            let empty_workspace = super::super::ExtendedBuildOrderWorkspace::new(
                worker.engine.field.width(),
                worker.engine.field.height(),
                worker.engine.problem.kick_profile().profile_id(),
            );
            drop(core::mem::replace(
                &mut worker.engine.build_order_workspace,
                empty_workspace,
            ));
            drop(core::mem::take(&mut worker.engine.coverage_evaluator));
            drop(core::mem::take(&mut worker.engine.buildable_tilings));
            drop(core::mem::take(&mut worker.digests));
            // One source inventory phase per worker after all digest ranges
            // are gone. Bind validated shared immutable ownership, so there
            // is no need to rewalk the common problem here. The fixed-queue
            // Omit worker has no retained observation, coverage-row, or
            // score/replay graph collections.
            let retained = worker
                .engine
                .checked_non_problem_retained_bytes()
                // This historical peak includes prior local graphs/projection
                // scratch. Those are already dropped; the original active
                // charges and aggregate high-water mark remain intact.
                .and_then(|bytes| bytes.checked_sub(worker.engine.peak_build_scratch_bytes as u128))
                .and_then(|bytes| bytes.checked_add(worker.checked_auxiliary_retained_bytes()?))
                .ok_or_else(projection_error)?;
            let base = retained
                .checked_sub(rows.retained)
                .ok_or_else(projection_error)?;
            worker
                .memory_owner()?
                .ensure(retained)
                .map_err(WasmExactSearchError::resource_admission)?;
            worker.terminal_base = Some(base);
            self.row_worker += 1;
            Ok(())
        }

        fn admit_canonical_cursor(&mut self) -> Result<(), WasmExactSearchError> {
            let aggregate = self.admitted_memory.as_ref().ok_or_else(contract_error)?;
            let owner = aggregate.owner(self.width.checked_add(1).ok_or_else(projection_error)?);
            self.canonical = Some(CanonicalReduction::new_shared(
                self.source_rows,
                self.root.field.height(),
                Board256Mask::from_words(self.root.catalog.initial_board().words()),
                self.root
                    .problem
                    .exact_pieces()
                    .ok_or_else(contract_error)?,
                owner,
            )?);
            self.phase = DryPhase::ImportCanonical;
            Ok(())
        }

        fn import_canonical_step(
            &mut self,
            remaining: &mut usize,
            control: &ExecutionControl,
        ) -> Result<(), WasmExactSearchError> {
            let Some(worker) = self.workers.get_mut(self.row_worker) else {
                *remaining -= 1;
                self.phase = DryPhase::FoldCanonical;
                return Ok(());
            };
            let rows = worker.bounded_rows.as_mut().ok_or_else(contract_error)?;
            if let Some(row) = rows.last() {
                if !self
                    .canonical
                    .as_mut()
                    .ok_or_else(contract_error)?
                    .collect_one(remaining, control, SourceKey::Concrete(row))?
                {
                    return Err(contract_error());
                }
                // The aggregate reserved space for both source and encoded
                // row. Release the worker's old credit only once dropped.
                drop(rows.pop()?);
            } else {
                if rows.len != 0 || rows.retained != 0 {
                    return Err(contract_error());
                }
                *remaining -= 1;
                self.row_worker += 1;
            }
            let retained = worker
                .terminal_base
                .and_then(|base| base.checked_add(rows.retained))
                .ok_or_else(projection_error)?;
            // Only the live row backing can change here. Avoid re-inventory
            // of the source problem or worker scratch for every emitted row.
            worker
                .memory_owner()?
                .ensure(retained)
                .map_err(WasmExactSearchError::resource_admission)
        }

        fn seal_verification(&mut self) -> Result<(), WasmExactSearchError> {
            let (keys, _) = self
                .canonical
                .as_ref()
                .and_then(CanonicalReduction::family)
                .ok_or_else(contract_error)?;
            match self.verification.representative {
                None if keys.is_empty()
                    && self.verification.coverage_word == 0
                    && self.verification.count_all == 0 => {}
                Some(winner)
                    if !keys.is_empty()
                        && self.verification.coverage_word == 1
                        && self.verification.count_all != 0 =>
                {
                    let source = self
                        .workers
                        .get(winner.worker_index)
                        .ok_or_else(contract_error)?;
                    source
                        .engine
                        .validate_pc_representative_physical_chain(true)?;
                }
                _ => return Err(contract_error()),
            }
            self.phase = DryPhase::Complete;
            Ok(())
        }

        pub(super) fn completed(&self) -> Option<CompletedFacts<'_>> {
            if self.phase != DryPhase::Complete {
                return None;
            }
            let (keys, hash) = self.canonical.as_ref()?.family()?;
            let representative = match self.verification.representative {
                Some(winner) => {
                    let worker = self.workers.get(winner.worker_index)?;
                    Some((
                        winner.ordinal,
                        worker.engine.representative_pattern_id?,
                        worker.engine.representative_path.as_slice(),
                    ))
                }
                None => None,
            };
            Some(CompletedFacts {
                digest: self.digest,
                candidate_count: self.candidate_count,
                workers: &self.workers,
                keys,
                hash,
                verification: VerifiedStats {
                    count_all: self.verification.count_all,
                    coverage_word: self.verification.coverage_word,
                    representative,
                },
            })
        }

        pub(in crate::backend::wasm_cpu) fn serial_handoff_pending(
            &self,
        ) -> Option<ResourceLeaseToken> {
            (self.phase == DryPhase::SerialHandoff).then(|| self.admission.lease_token())
        }

        pub(in crate::backend::wasm_cpu) fn parallel_result_pending(
            &self,
        ) -> Option<ResourceLeaseToken> {
            (self.phase == DryPhase::Complete).then(|| self.admission.lease_token())
        }

        pub(in crate::backend::wasm_cpu) fn into_parallel_result(
            mut self,
            control: &ExecutionControl,
        ) -> Result<ParallelTerminal, WasmExactSearchError> {
            if control.is_cancelled() {
                return Err(WasmExactSearchError::Cancelled);
            }
            if self.phase != DryPhase::Complete || control.partition().count() != 1 {
                return Err(contract_error());
            }
            let aggregate = self.admitted_memory.as_ref().ok_or_else(contract_error)?;
            let backing = aggregate.owner(self.width.checked_add(1).ok_or_else(projection_error)?);
            let family = self
                .canonical
                .take()
                .ok_or_else(contract_error)?
                .into_family(control)?;

            if let Some(winner) = self.verification.representative {
                let source = &mut self
                    .workers
                    .get_mut(winner.worker_index)
                    .ok_or_else(contract_error)?
                    .engine;
                if source.covered_patterns.word_at(0) != self.verification.coverage_word
                    || source.representative_rank != Some(winner.ordinal)
                {
                    return Err(contract_error());
                }
                // Exactly one ordinal: the selected witness necessarily
                // covers the already-verified union. Move both backing
                // owners; the worker slot stays charged throughout the
                // terminal join even after its witness moves into the root.
                core::mem::swap(
                    &mut self.root.covered_patterns,
                    &mut source.covered_patterns,
                );
                self.root.representative_path = core::mem::take(&mut source.representative_path);
                self.root.representative_rank = source.representative_rank;
                self.root.representative_pattern_id = source.representative_pattern_id;
            }
            if self.root.covered_patterns.word_at(0) != self.verification.coverage_word {
                return Err(contract_error());
            }
            self.root.pc_build_variant_count = self.verification.count_all;
            self.root.candidate_digest = self.digest;
            self.root.workers_used = self.width;
            self.root.native_parallel_peak_bytes = aggregate.admitted_peak_bytes();
            let result = self
                .root
                .complete_pc_family_with_sealed_keys(family, &backing, control)?;
            Ok(ParallelTerminal {
                result,
                _backing: backing,
                admission: self.admission,
            })
        }

        pub(in crate::backend::wasm_cpu) fn into_serial_handoff(
            self,
            control: &ExecutionControl,
        ) -> Result<SerialHandoff, WasmExactSearchError> {
            if control.is_cancelled() {
                return Err(WasmExactSearchError::Cancelled);
            }
            if self.phase != DryPhase::SerialHandoff || control.partition().count() != 1 {
                return Err(contract_error());
            }
            Ok(SerialHandoff {
                engine: self.root,
                admission: self.admission,
            })
        }

        pub(super) fn canonical_last_row_imported(&self) -> bool {
            self.phase == DryPhase::ImportCanonical
                && self.source_rows != 0
                && self
                    .canonical
                    .as_ref()
                    .is_some_and(|cursor| cursor.imported_rows() == self.source_rows)
        }

        pub(super) fn catalog(&self) -> &Arc<super::super::ExtendedInverseCatalog> {
            &self.root.catalog
        }
    }
}

#[cfg(test)]
#[path = "extended_pc_family_parallel_worker_tests.rs"]
mod worker_tests;

#[cfg(test)]
#[path = "extended_pc_cooperative_canonical.rs"]
mod cooperative_canonical;

#[cfg(test)]
#[path = "extended_pc_cooperative_stats_tests.rs"]
mod cooperative_stats_tests;
