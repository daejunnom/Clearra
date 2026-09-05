use clearra_core_domain::{
    execution_cancellation::ExecutionCancellationToken,
    pruning::PruningEvidencePolicy,
    resource::{
        ExecutionAvailability, ExecutionAvailabilityReason, ResourceReport,
        ResourceTruncationReason,
    },
};
use clearra_core_ffi::{
    CPackingProblem, CoreCNative, NativeCoreError, NativeGeometryPathConsumer,
    NativeGeometryPathSinkError, NativeGeometrySolutionTask,
};
use clearra_coverage::pattern::pattern_bitset::PatternBitSet;
use clearra_problem::SearchProblem;

use crate::{
    buildup::buildup_native_bridge::uses_standard_bag_automaton,
    packing::PackingRunnerError,
    performance::{ExecutorSearchStage, SearchStageSpan},
    resource::ExecutionMemoryBound,
};

use super::{
    buildable_geometry_task_reducer::{
        reduce_buildable_geometry_paths, BuildableGeometryPathConsumer, BuildableGeometryPathError,
    },
    BackendTrustReport, PackingBackendOutcome, SelectedSearchBackend,
};

const TASKS_PER_WORKER: usize = 4;

pub(crate) fn execute_cpu_buildable_geometry_graph(
    search_problem: &SearchProblem,
    source_pattern_bits: Option<&PatternBitSet>,
    problem: &CPackingProblem,
    cancellation: &ExecutionCancellationToken,
    backend: SelectedSearchBackend,
    requested_worker_count: usize,
) -> Result<PackingBackendOutcome, PackingRunnerError> {
    if cancellation.is_cancelled() {
        return Err(PackingRunnerError::ExecutionCancelled);
    }
    let memory_bound = engine_memory_bound(search_problem, problem)?;
    let catalog_span = SearchStageSpan::begin(ExecutorSearchStage::PackingCpuCatalogCompile);
    let catalog = CoreCNative::compile_geometry_catalog_with_cancellation(problem, cancellation)
        .map_err(|error| enrich_native_memory_error(search_problem, error))?;
    catalog_span.finish(catalog.view().skeleton_cell_masks().len() as u64);
    let catalog_bytes = catalog.resident_bytes() as u128;
    memory_bound
        .ensure(catalog_bytes, 0)
        .map_err(packing_resource_error)?;
    let graph_span = SearchStageSpan::begin(ExecutorSearchStage::PackingCpuGeometryGraph);
    let geometry_search = CoreCNative::search_geometry_solution_graph(
        &catalog,
        problem,
        cancellation,
        PruningEvidencePolicy::BestEffort,
    )
    .map_err(|error| enrich_native_memory_error(search_problem, error))?;
    graph_span.finish(geometry_search.graph.node_count() as u64);
    let graph_bytes = geometry_search.graph.resident_bytes() as u128;
    let immutable_graph_bytes = catalog_bytes
        .checked_add(graph_bytes)
        .ok_or_else(|| packing_projection_overflow(search_problem))?;
    memory_bound
        .ensure(
            immutable_graph_bytes.max(geometry_search.resource_report.peak_cpu_bytes as u128),
            0,
        )
        .map_err(packing_resource_error)?;
    let desired_task_count = requested_worker_count
        .max(1)
        .checked_mul(TASKS_PER_WORKER)
        .ok_or_else(|| packing_projection_overflow(search_problem))?;
    let task_sizing = geometry_search
        .graph
        .checked_task_split_sizing(desired_task_count)
        .ok_or_else(|| packing_projection_overflow(search_problem))?;
    memory_bound
        .ensure(
            immutable_graph_bytes,
            task_sizing
                .checked_peak_increment_bytes()
                .ok_or_else(|| packing_projection_overflow(search_problem))?,
        )
        .map_err(packing_resource_error)?;
    let task_span = SearchStageSpan::begin(ExecutorSearchStage::PackingCpuTaskSplit);
    let (tasks, task_split_scratch_bytes) = geometry_search
        .graph
        .split_tasks_with_memory_limit(
            desired_task_count,
            immutable_graph_bytes,
            memory_bound.cap_bytes(),
        )
        .map_err(|error| enrich_native_memory_error(search_problem, error))?;
    task_span.finish(tasks.len() as u64);
    let worker_count = requested_worker_count.max(1).min(tasks.len().max(1));
    let task_bytes = (tasks.capacity() as u128)
        .checked_mul(std::mem::size_of::<NativeGeometrySolutionTask>() as u128)
        .ok_or_else(|| packing_projection_overflow(search_problem))?;
    let base_resident_bytes = catalog_bytes
        .checked_add(graph_bytes)
        .and_then(|bytes| bytes.checked_add(task_bytes))
        .ok_or_else(|| packing_projection_overflow(search_problem))?;
    let task_split_peak_bytes = base_resident_bytes
        .checked_add(task_split_scratch_bytes as u128)
        .ok_or_else(|| packing_projection_overflow(search_problem))?;
    memory_bound
        .ensure(task_split_peak_bytes, 0)
        .map_err(packing_resource_error)?;
    let base_resident_bytes_usize = usize::try_from(base_resident_bytes)
        .map_err(|_| packing_projection_overflow(search_problem))?;
    let standard_bag = uses_standard_bag_automaton(search_problem);
    #[cfg(feature = "search-stage-profiling")]
    eprintln!(
        "profile immutable geometry graph | backend={backend:?} | requested_workers={requested_worker_count} | workers={worker_count} | tasks={} | nodes={}",
        tasks.len(),
        geometry_search.graph.node_count(),
    );
    let graph = geometry_search.graph.clone();
    let reduction_span = SearchStageSpan::begin(ExecutorSearchStage::PackingCpuBuildabilityReduce);
    let reduction = reduce_buildable_geometry_paths(
        search_problem,
        source_pattern_bits,
        problem,
        &catalog,
        cancellation,
        worker_count,
        base_resident_bytes_usize,
        move |worker_index, worker_count, consumer| {
            for task in tasks.iter().skip(worker_index).step_by(worker_count) {
                if consumer.should_stop() {
                    break;
                }
                if standard_bag {
                    match consumer.consume_standard_bag_graph_task(&graph, task) {
                        Ok(()) => {}
                        Err(BuildableGeometryPathError::Stopped)
                            if consumer.was_truncated() || cancellation.is_cancelled() =>
                        {
                            break;
                        }
                        Err(
                            BuildableGeometryPathError::Stopped
                            | BuildableGeometryPathError::Invalid,
                        ) => break,
                    }
                } else {
                    let mut adapter = NativeBuildableGeometryPathAdapter { consumer };
                    match graph.stream_task_paths(task, cancellation, &mut adapter) {
                        Ok(_) => {}
                        Err(_) if adapter.consumer.should_stop() => break,
                        Err(error) => return Err(PackingRunnerError::Native(error)),
                    }
                }
            }
            Ok(())
        },
    )?;
    reduction_span.finish(reduction.generated_count as u64);

    let mut resource_report = *catalog.compile_resource_report();
    merge_sequential_stage_metrics(&mut resource_report, &geometry_search.resource_report);
    resource_report.observe_cpu_bytes(
        usize::try_from(task_split_peak_bytes)
            .map_err(|_| packing_projection_overflow(search_problem))?,
    );
    let mut pruning_ledgers = vec![
        catalog.pruning_ledger().clone(),
        geometry_search.pruning_ledger,
    ];
    if let Some(buildability_ledger) = reduction.pruning_ledger {
        pruning_ledgers.push(buildability_ledger);
    }
    let pruning_ledger =
        clearra_core_ffi::NativePruningLedger::merge_partition_reports(pruning_ledgers)
            .map_err(clearra_core_ffi::NativeCoreError::InvalidPruningLedger)
            .map_err(PackingRunnerError::Native)?;

    resource_report.observe_candidate_rows(reduction.generated_count);
    let retained_reduction_bytes = (reduction.workspace_bytes as u128)
        .checked_add(reduction.reducer_bytes as u128)
        .ok_or_else(|| packing_projection_overflow(search_problem))?;
    let observed_peak_bytes = base_resident_bytes
        .checked_add(retained_reduction_bytes)
        .ok_or_else(|| packing_projection_overflow(search_problem))?;
    memory_bound
        .ensure(observed_peak_bytes, 0)
        .map_err(packing_resource_error)?;
    let peak_cpu_bytes = resource_report.peak_cpu_bytes.max(
        usize::try_from(observed_peak_bytes)
            .map_err(|_| packing_projection_overflow(search_problem))?,
    );
    resource_report.observe_cpu_bytes(peak_cpu_bytes);
    resource_report.observe_build_worker_backlog(0);
    match reduction.truncation {
        1 => resource_report.mark_truncated(ResourceTruncationReason::CandidateBudgetExceeded),
        2 => {
            let required_memory_bytes =
                observed_peak_bytes.max(memory_bound.cap_bytes().saturating_add(1));
            return Err(packing_memory_exhausted(
                search_problem,
                required_memory_bytes,
            ));
        }
        _ => {}
    }
    debug_assert!(reduction.buildable_count >= reduction.candidates.len());

    Ok(PackingBackendOutcome::buildability_prefiltered_exact(
        backend,
        reduction.candidates,
        resource_report,
        BackendTrustReport::cpu_exact(),
    )
    .with_workers_used(reduction.worker_count)
    .with_geometry_catalog(catalog)
    .with_pruning_ledger(pruning_ledger))
}

// `ResourceReport` is the typed authority contract; callers immediately map it into
// the boxed public runner error, so this local composition keeps the domain type intact.
#[allow(clippy::result_large_err)]
fn engine_memory_bound(
    search_problem: &SearchProblem,
    problem: &CPackingProblem,
) -> Result<ExecutionMemoryBound, PackingRunnerError> {
    let max_memory_bytes = if problem.budget.has_max_memory_mib == 0 {
        u128::MAX
    } else {
        u128::from(problem.budget.max_memory_mib) * 1024 * 1024
    };
    ExecutionMemoryBound::unbounded_for_problem(search_problem)
        .and_then(|bound| bound.with_cap(max_memory_bytes))
        .map_err(packing_resource_error)
}

fn packing_resource_error(resource_report: ResourceReport) -> PackingRunnerError {
    PackingRunnerError::Native(NativeCoreError::packing_incomplete(6, resource_report))
}

fn packing_resource_report(
    search_problem: &SearchProblem,
    reason: ExecutionAvailabilityReason,
    required_memory_bytes: u128,
) -> ResourceReport {
    let Some(universe) = search_problem.piece_source().materialized_universe() else {
        return ResourceReport::admission_failure(ExecutionAvailability::unavailable(
            ExecutionAvailabilityReason::CapabilityUnavailable,
        ));
    };
    let descriptor_pattern_count = universe.total_possible_pattern_count();
    let dense_pattern_count = universe.pattern_count() as u128;
    let required_dense_bytes = dense_pattern_count
        .checked_add(63)
        .and_then(|count| count.checked_div(64))
        .and_then(|words| words.checked_mul(core::mem::size_of::<u64>() as u128))
        .unwrap_or(u128::MAX);
    let availability = match reason {
        ExecutionAvailabilityReason::MemoryBudgetExceeded => {
            ExecutionAvailability::exhausted(reason)
        }
        _ => ExecutionAvailability::unavailable(reason),
    }
    .with_pattern_evidence(
        descriptor_pattern_count,
        dense_pattern_count,
        required_dense_bytes,
    )
    .with_required_memory_bytes(required_memory_bytes);
    ResourceReport::admission_failure(availability)
}

fn packing_projection_overflow(search_problem: &SearchProblem) -> PackingRunnerError {
    packing_resource_error(packing_resource_report(
        search_problem,
        ExecutionAvailabilityReason::PatternCountAddressSpaceExceeded,
        u128::MAX,
    ))
}

fn packing_memory_exhausted(
    search_problem: &SearchProblem,
    required_memory_bytes: u128,
) -> PackingRunnerError {
    packing_resource_error(packing_resource_report(
        search_problem,
        ExecutionAvailabilityReason::MemoryBudgetExceeded,
        required_memory_bytes,
    ))
}

fn enrich_native_memory_error(
    search_problem: &SearchProblem,
    error: NativeCoreError,
) -> PackingRunnerError {
    if let NativeCoreError::PackingIncomplete {
        status,
        resource_report,
    } = error
    {
        let availability = resource_report.execution_availability();
        if availability.reason() == Some(ExecutionAvailabilityReason::MemoryBudgetExceeded)
            && availability.descriptor_pattern_count().is_none()
        {
            return PackingRunnerError::Native(NativeCoreError::packing_incomplete(
                status,
                packing_resource_report(
                    search_problem,
                    ExecutionAvailabilityReason::MemoryBudgetExceeded,
                    availability.required_memory_bytes().unwrap_or(u128::MAX),
                ),
            ));
        }
        return PackingRunnerError::Native(NativeCoreError::PackingIncomplete {
            status,
            resource_report,
        });
    }
    PackingRunnerError::Native(error)
}

fn merge_sequential_stage_metrics(target: &mut ResourceReport, source: &ResourceReport) {
    target.peak_frontier_states = target.peak_frontier_states.max(source.peak_frontier_states);
    target.peak_candidate_rows = target.peak_candidate_rows.max(source.peak_candidate_rows);
    target.peak_hash_buckets = target.peak_hash_buckets.max(source.peak_hash_buckets);
    target.peak_gpu_bytes = target.peak_gpu_bytes.max(source.peak_gpu_bytes);
    target.peak_cpu_bytes = target.peak_cpu_bytes.max(source.peak_cpu_bytes);
    target.build_worker_backlog_peak = target
        .build_worker_backlog_peak
        .max(source.build_worker_backlog_peak);
    target.coverage_rows_emitted = target
        .coverage_rows_emitted
        .saturating_add(source.coverage_rows_emitted);
    if source.truncated {
        target.truncated = true;
        target.truncation_reason = target.truncation_reason.or(source.truncation_reason);
    }
    target.probability_complete &= source.probability_complete;
}

struct NativeBuildableGeometryPathAdapter<'consumer, 'context> {
    consumer: &'consumer mut BuildableGeometryPathConsumer<'context>,
}

impl NativeGeometryPathConsumer for NativeBuildableGeometryPathAdapter<'_, '_> {
    fn consume(&mut self, skeleton_row_ids: &[u32]) -> Result<(), NativeGeometryPathSinkError> {
        match self.consumer.consume_row_ids(skeleton_row_ids) {
            Ok(()) => Ok(()),
            Err(BuildableGeometryPathError::Stopped) if self.consumer.was_truncated() => {
                Err(NativeGeometryPathSinkError::CapacityExceeded)
            }
            Err(BuildableGeometryPathError::Stopped | BuildableGeometryPathError::Invalid) => {
                Err(NativeGeometryPathSinkError::Invalid)
            }
        }
    }
}
