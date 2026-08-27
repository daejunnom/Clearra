use clearra_core_domain::execution_cancellation::ExecutionCancellationToken;
use clearra_core_domain::resource::{ResourceReport, ResourceTruncationReason};
#[cfg(all(test, feature = "native-c-core"))]
use clearra_core_ffi::PackingCandidateBatch;
use clearra_core_ffi::{
    CPackingCandidate, CPackingProblem, CoreCNative, NativeCandidateReducer, NativeCoreError,
    NativeGeometryCatalog, NativePackingCandidateConsumer, NativePackingCandidateContext,
    NativePackingCandidateSinkError, NativePackingOutcome, NativePackingStreamOutcome,
};
use clearra_pc_graph::request::PcExecutionPolicy;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};

use crate::{execution_worker_limit, packing::PackingRunnerError};

use super::{
    BackendTrustReport, PackingBackendOutcome, SearchBackendExecutor, SelectedSearchBackend,
};

#[cfg(not(all(feature = "webgpu-search", feature = "native-c-core")))]
use super::{GpuExecutionFailure, GpuExecutionFailureStage, SearchBackendFallbackReason};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeCpuPackingExecutor {
    actual_backend: SelectedSearchBackend,
}

impl NativeCpuPackingExecutor {
    pub(crate) const fn new(actual_backend: SelectedSearchBackend) -> Self {
        Self { actual_backend }
    }
}

impl SearchBackendExecutor for NativeCpuPackingExecutor {
    fn execute_packing(
        &self,
        problem: &CPackingProblem,
        _policy: &PcExecutionPolicy,
        cancellation: &ExecutionCancellationToken,
    ) -> Result<PackingBackendOutcome, PackingRunnerError> {
        if problem.piece_multiset_family.count > 1 {
            return NativeParallelPackingExecutor.execute_family_work_items(
                problem,
                _policy,
                cancellation,
                1,
                self.actual_backend,
            );
        }
        let catalog =
            CoreCNative::compile_geometry_catalog_with_cancellation(problem, cancellation)
                .map_err(PackingRunnerError::Native)?;
        let outcome = generate_complete_family(&catalog, problem, cancellation)?;
        Ok(PackingBackendOutcome::raw_geometry_exact(
            self.actual_backend,
            outcome.candidates,
            outcome.resource_report,
            BackendTrustReport::cpu_exact(),
        )
        .with_geometry_catalog(catalog))
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NativeGpuPackingExecutor;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NativeParallelPackingExecutor;

impl SearchBackendExecutor for NativeParallelPackingExecutor {
    fn execute_packing(
        &self,
        problem: &CPackingProblem,
        policy: &PcExecutionPolicy,
        cancellation: &ExecutionCancellationToken,
    ) -> Result<PackingBackendOutcome, PackingRunnerError> {
        let available_workers = policy
            .workers()
            .min(execution_worker_limit::hardware_worker_limit());
        self.execute_with_available_workers(problem, policy, cancellation, available_workers)
    }
}

impl NativeParallelPackingExecutor {
    fn execute_with_available_workers(
        &self,
        problem: &CPackingProblem,
        policy: &PcExecutionPolicy,
        cancellation: &ExecutionCancellationToken,
        available_workers: usize,
    ) -> Result<PackingBackendOutcome, PackingRunnerError> {
        let worker_count = parallel_worker_count(problem, policy, available_workers);
        if worker_count <= 1 && problem.piece_multiset_family.count <= 1 {
            return native_exact_outcome(
                problem,
                cancellation,
                SelectedSearchBackend::CpuParallelGeometryExactCover,
                1,
            );
        }

        self.execute_family_work_items(
            problem,
            policy,
            cancellation,
            worker_count,
            SelectedSearchBackend::CpuParallelGeometryExactCover,
        )
    }

    fn execute_family_work_items(
        &self,
        problem: &CPackingProblem,
        policy: &PcExecutionPolicy,
        cancellation: &ExecutionCancellationToken,
        requested_worker_count: usize,
        actual_backend: SelectedSearchBackend,
    ) -> Result<PackingBackendOutcome, PackingRunnerError> {
        let work_items = parallel_work_items(problem, requested_worker_count);
        let work_item_count = work_items.len();
        let worker_count = requested_worker_count.min(work_item_count).max(1);
        let catalog =
            CoreCNative::compile_geometry_catalog_with_cancellation(problem, cancellation)
                .map_err(PackingRunnerError::Native)?;

        let shared_reducer = Arc::new(Mutex::new(
            NativeCandidateReducer::new(problem).map_err(PackingRunnerError::CandidateBatch)?,
        ));
        let next_shard = AtomicUsize::new(0);
        let shard_results = std::thread::scope(|scope| {
            let work_items = &work_items;
            let catalog = &catalog;
            let shared_reducer = &shared_reducer;
            let mut handles = Vec::with_capacity(worker_count);
            for _ in 0..worker_count {
                let next_shard = &next_shard;
                let reducer = Arc::clone(shared_reducer);
                handles.push(scope.spawn(move || {
                    let mut completed = Vec::new();
                    let mut consumer = SharedCandidateConsumer { reducer };
                    loop {
                        if cancellation.is_cancelled() {
                            return Err(PackingRunnerError::ExecutionCancelled);
                        }
                        let shard_index = next_shard.fetch_add(1, Ordering::Relaxed);
                        if shard_index >= work_item_count {
                            break;
                        }
                        match work_items[shard_index].stream(
                            problem,
                            catalog,
                            cancellation,
                            &mut consumer,
                        ) {
                            Ok(outcome) => completed.push((shard_index, outcome)),
                            Err(error) => return Err(PackingRunnerError::Native(error)),
                        }
                    }
                    Ok(completed)
                }));
            }
            let mut completed = Vec::with_capacity(work_item_count);
            for handle in handles {
                completed.extend(
                    handle
                        .join()
                        .map_err(|_| PackingRunnerError::ParallelWorkerPanicked)??,
                );
            }
            completed.sort_unstable_by_key(|(shard_index, _)| *shard_index);
            Ok::<_, PackingRunnerError>(completed)
        })?;

        if cancellation.is_cancelled() {
            return Err(PackingRunnerError::ExecutionCancelled);
        }

        let reducer = match Arc::try_unwrap(shared_reducer) {
            Ok(reducer) => reducer,
            Err(_) => return Err(PackingRunnerError::ParallelWorkerPanicked),
        };
        let mut candidates = reducer
            .into_inner()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .into_candidates();
        let mut resource_report = catalog.compile_resource_report().clone();
        let shared_catalog_bytes = resource_report.peak_cpu_bytes;
        for (_, outcome) in shard_results {
            let mut worker_report = outcome.resource_report;
            worker_report.peak_cpu_bytes = worker_report
                .peak_cpu_bytes
                .saturating_sub(shared_catalog_bytes);
            merge_parallel_resource_report(&mut resource_report, &worker_report);
        }
        if policy.max_candidates() != 0 && candidates.len() > policy.max_candidates() {
            candidates.truncate(policy.max_candidates());
            resource_report.mark_truncated(ResourceTruncationReason::CandidateBudgetExceeded);
        }

        Ok(PackingBackendOutcome::raw_geometry_exact(
            actual_backend,
            candidates,
            resource_report,
            BackendTrustReport::cpu_exact(),
        )
        .with_workers_used(worker_count)
        .with_geometry_catalog(catalog))
    }
}

fn generate_complete_family(
    catalog: &NativeGeometryCatalog,
    problem: &CPackingProblem,
    cancellation: &ExecutionCancellationToken,
) -> Result<NativePackingOutcome, PackingRunnerError> {
    catalog
        .generate_partition(
            problem,
            0,
            problem.piece_multiset_family.count,
            0,
            1,
            1,
            cancellation,
        )
        .map_err(PackingRunnerError::Native)
}

fn native_exact_outcome(
    problem: &CPackingProblem,
    cancellation: &ExecutionCancellationToken,
    backend: SelectedSearchBackend,
    workers_used: usize,
) -> Result<PackingBackendOutcome, PackingRunnerError> {
    let catalog = CoreCNative::compile_geometry_catalog_with_cancellation(problem, cancellation)
        .map_err(PackingRunnerError::Native)?;
    let outcome = generate_complete_family(&catalog, problem, cancellation)?;
    Ok(PackingBackendOutcome::raw_geometry_exact(
        backend,
        outcome.candidates,
        outcome.resource_report,
        BackendTrustReport::cpu_exact(),
    )
    .with_workers_used(workers_used)
    .with_geometry_catalog(catalog))
}

fn parallel_worker_count(
    problem: &CPackingProblem,
    policy: &PcExecutionPolicy,
    available_workers: usize,
) -> usize {
    let hardware_limit = execution_worker_limit::hardware_worker_limit();
    let mut workers = policy
        .workers()
        .min(hardware_limit)
        .min(available_workers.max(1))
        .min(u16::MAX as usize)
        .max(1);
    if problem.budget.has_max_memory_mib != 0 {
        workers = workers.min(problem.budget.max_memory_mib as usize).max(1);
    }
    workers
}

fn parallel_partition_depth(problem: &CPackingProblem) -> u8 {
    problem.piece_window.max_pieces.clamp(1, 2) as u8
}

#[derive(Clone, Copy)]
enum ParallelPackingWorkItem {
    FamilyRange {
        begin: u16,
        end: u16,
    },
    Prefix {
        partition_index: u16,
        partition_count: u16,
        partition_depth: u8,
    },
}

impl ParallelPackingWorkItem {
    fn stream(
        &self,
        problem: &CPackingProblem,
        catalog: &NativeGeometryCatalog,
        cancellation: &ExecutionCancellationToken,
        consumer: &mut dyn NativePackingCandidateConsumer,
    ) -> Result<NativePackingStreamOutcome, NativeCoreError> {
        match self {
            Self::FamilyRange { begin, end } => {
                catalog.stream_partition(problem, *begin, *end, 0, 1, 1, cancellation, consumer)
            }
            Self::Prefix {
                partition_index,
                partition_count,
                partition_depth,
            } => catalog.stream_partition(
                problem,
                0,
                0,
                *partition_index,
                *partition_count,
                *partition_depth,
                cancellation,
                consumer,
            ),
        }
    }
}

struct SharedCandidateConsumer {
    reducer: Arc<Mutex<NativeCandidateReducer>>,
}

impl NativePackingCandidateConsumer for SharedCandidateConsumer {
    fn consume(
        &mut self,
        candidate: CPackingCandidate,
        mut context: NativePackingCandidateContext,
    ) -> Result<bool, NativePackingCandidateSinkError> {
        let mut reducer = self
            .reducer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        context.accepted_candidate_count = reducer.accepted_candidate_count();
        reducer.consume(candidate, context)
    }

    fn resident_bytes(&self) -> usize {
        self.reducer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .resident_bytes()
    }
}

fn parallel_work_items(
    problem: &CPackingProblem,
    worker_count: usize,
) -> Vec<ParallelPackingWorkItem> {
    let family_count = usize::from(problem.piece_multiset_family.count);
    let work_item_count = if family_count > 1 {
        worker_count.min(family_count)
    } else {
        worker_count
    }
    .max(1);
    if family_count > 1 {
        return (0..work_item_count)
            .map(|work_index| {
                let begin = family_count * work_index / work_item_count;
                let end = family_count * (work_index + 1) / work_item_count;
                ParallelPackingWorkItem::FamilyRange {
                    begin: begin as u16,
                    end: end as u16,
                }
            })
            .collect();
    }

    let partition_count = u16::try_from(work_item_count)
        .expect("parallel worker count is clamped to the C partition width");
    let partition_depth = parallel_partition_depth(problem);
    (0..work_item_count)
        .map(|partition_index| ParallelPackingWorkItem::Prefix {
            partition_index: partition_index as u16,
            partition_count,
            partition_depth,
        })
        .collect()
}

#[cfg(all(test, feature = "native-c-core"))]
mod tests {
    use std::collections::BTreeSet;

    use clearra_core_domain::{pc::pc_target::PcTarget, piece::piece_kind::PieceKind};
    use clearra_core_ffi::{CPackingCandidate, CPackingProblemBuilder};
    use clearra_pc_graph::request::{
        OpeningPcSearchQuery, PcExecutionPolicy, PcHoldPolicy, PcQueueInput, RequestedSearchBackend,
    };
    use clearra_problem::ProblemCompiler;
    use clearra_supply::queue::fixed_sequence::FixedSequence;

    use super::*;

    #[test]
    fn parallel_root_partitions_preserve_exact_candidate_payloads() {
        let query = OpeningPcSearchQuery::new(PcTarget::two_lines())
            .with_queue(PcQueueInput::fixed_sequence(FixedSequence::new(vec![
                PieceKind::I,
                PieceKind::I,
                PieceKind::O,
                PieceKind::O,
                PieceKind::O,
            ])))
            .with_hold_policy(PcHoldPolicy::Disabled);
        let search_problem = ProblemCompiler::compile_opening_pc(&query).expect("problem");
        let compact =
            CPackingProblemBuilder::from_search_problem(&search_problem).expect("compact problem");
        let policy = PcExecutionPolicy::mvp_default()
            .with_backend(RequestedSearchBackend::Cpu)
            .with_workers(2);
        let cancellation = ExecutionCancellationToken::new();

        let serial = NativeCpuPackingExecutor::new(SelectedSearchBackend::CpuGeometryExactCover)
            .execute_packing(&compact, &policy, &cancellation)
            .expect("serial packing");
        let parallel = NativeParallelPackingExecutor
            .execute_with_available_workers(&compact, &policy, &cancellation, 2)
            .expect("parallel packing");

        assert_eq!(
            parallel.workers_used,
            execution_worker_limit::clamp_requested_workers(2, false)
        );
        assert_eq!(
            exact_payloads(&serial.candidates),
            exact_payloads(&parallel.candidates)
        );
        assert_eq!(
            serial.resource_report.probability_complete,
            parallel.resource_report.probability_complete
        );
    }

    #[test]
    fn exact_multiset_family_members_are_grouped_into_disjoint_worker_ranges() {
        let mut problem = CPackingProblem::default();
        problem.piece_multiset_window.exact_count = 10;
        problem.piece_multiset_family.complete = 1;
        problem.piece_multiset_family.count = 3;
        for (member_index, piece_index) in [1_usize, 2, 3].into_iter().enumerate() {
            let member = &mut problem.piece_multiset_family.members[member_index];
            member.counts[piece_index] = 10;
            member.total_count = 10;
            member.exact_count = 10;
        }

        let work_items = parallel_work_items(&problem, 2);

        assert_eq!(work_items.len(), 2);
        let expected_ranges = [0..1, 1..3];
        for (expected_range, work_item) in expected_ranges.into_iter().zip(work_items) {
            let ParallelPackingWorkItem::FamilyRange { begin, end } = work_item else {
                panic!("multiset family must not be lowered to a prefix partition");
            };
            assert_eq!(usize::from(begin), expected_range.start);
            assert_eq!(usize::from(end), expected_range.end);
        }
    }

    fn exact_payloads(candidates: &PackingCandidateBatch) -> BTreeSet<Vec<u64>> {
        candidates.iter().map(candidate_payload).collect()
    }

    fn candidate_payload(candidate: CPackingCandidate) -> Vec<u64> {
        let mut payload = vec![
            candidate.final_board,
            candidate.shape_mask,
            candidate.shape_key,
            candidate.tiling_key,
            candidate.operation_set_key,
            u64::from(candidate.operation_count),
            u64::from(candidate.geometry_variant_domains),
            u64::from(candidate.cleared_lines),
        ];
        for operation in &candidate.operations[..usize::from(candidate.operation_count)] {
            payload.extend([
                u64::from(operation.piece),
                u64::from(operation.rotation),
                u64::from(operation.x as u8),
                u64::from(operation.y as u8),
                u64::from(operation.operation_id),
                u64::from(operation.required_deleted_row_mask),
                operation.mask,
            ]);
        }
        payload
    }
}

pub(crate) fn merge_parallel_resource_report(target: &mut ResourceReport, source: &ResourceReport) {
    if let Some(reason) = source.truncation_reason {
        target.mark_truncated(reason);
    }
    target.peak_frontier_states = target
        .peak_frontier_states
        .saturating_add(source.peak_frontier_states);
    target.peak_candidate_rows = target
        .peak_candidate_rows
        .saturating_add(source.peak_candidate_rows);
    target.peak_hash_buckets = target
        .peak_hash_buckets
        .saturating_add(source.peak_hash_buckets);
    target.peak_gpu_bytes = target.peak_gpu_bytes.saturating_add(source.peak_gpu_bytes);
    target.peak_cpu_bytes = target.peak_cpu_bytes.saturating_add(source.peak_cpu_bytes);
    target.build_worker_backlog_peak = target
        .build_worker_backlog_peak
        .saturating_add(source.build_worker_backlog_peak);
    target.coverage_rows_emitted = target
        .coverage_rows_emitted
        .saturating_add(source.coverage_rows_emitted);
    target.probability_complete &= source.probability_complete;
}

#[cfg(all(feature = "webgpu-search", feature = "native-c-core"))]
impl SearchBackendExecutor for NativeGpuPackingExecutor {
    fn execute_packing(
        &self,
        problem: &CPackingProblem,
        policy: &PcExecutionPolicy,
        cancellation: &ExecutionCancellationToken,
    ) -> Result<PackingBackendOutcome, PackingRunnerError> {
        super::native_webgpu_packing_executor::execute_webgpu_packing(
            problem,
            policy,
            cancellation,
            SelectedSearchBackend::Gpu,
            policy.workers(),
        )
    }
}

#[cfg(not(all(feature = "webgpu-search", feature = "native-c-core")))]
impl SearchBackendExecutor for NativeGpuPackingExecutor {
    fn execute_packing(
        &self,
        _problem: &CPackingProblem,
        _policy: &PcExecutionPolicy,
        _cancellation: &ExecutionCancellationToken,
    ) -> Result<PackingBackendOutcome, PackingRunnerError> {
        Err(PackingRunnerError::GpuExecution(
            GpuExecutionFailure::unavailable(
                GpuExecutionFailureStage::KernelExecution,
                SearchBackendFallbackReason::GpuKernelUnavailable,
            ),
        ))
    }
}
