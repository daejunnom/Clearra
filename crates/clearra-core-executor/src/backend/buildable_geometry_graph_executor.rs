use clearra_core_domain::{
    execution_cancellation::ExecutionCancellationToken,
    pruning::PruningEvidencePolicy,
    resource::{ResourceReport, ResourceTruncationReason},
};
use clearra_core_ffi::{
    CPackingProblem, CoreCNative, NativeGeometryPathConsumer, NativeGeometryPathSinkError,
    NativeGeometrySolutionTask,
};
use clearra_coverage::pattern::pattern_bitset::PatternBitSet;
use clearra_problem::SearchProblem;

use crate::{
    buildup::buildup_native_bridge::uses_standard_bag_automaton,
    packing::PackingRunnerError,
    performance::{ExecutorSearchStage, SearchStageSpan},
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
    let catalog_span = SearchStageSpan::begin(ExecutorSearchStage::PackingCpuCatalogCompile);
    let catalog = CoreCNative::compile_geometry_catalog_with_cancellation(problem, cancellation)
        .map_err(PackingRunnerError::Native)?;
    catalog_span.finish(catalog.view().skeleton_cell_masks().len() as u64);
    let catalog_bytes = catalog.resident_bytes();
    let graph_span = SearchStageSpan::begin(ExecutorSearchStage::PackingCpuGeometryGraph);
    let geometry_search = CoreCNative::search_geometry_solution_graph(
        &catalog,
        problem,
        cancellation,
        PruningEvidencePolicy::BestEffort,
    )
    .map_err(PackingRunnerError::Native)?;
    graph_span.finish(geometry_search.graph.node_count() as u64);
    let desired_task_count = requested_worker_count
        .max(1)
        .saturating_mul(TASKS_PER_WORKER);
    let task_span = SearchStageSpan::begin(ExecutorSearchStage::PackingCpuTaskSplit);
    let (tasks, task_split_scratch_bytes) = geometry_search
        .graph
        .split_tasks(desired_task_count)
        .map_err(PackingRunnerError::Native)?;
    task_span.finish(tasks.len() as u64);
    let worker_count = requested_worker_count.max(1).min(tasks.len().max(1));
    let graph_bytes = geometry_search.graph.resident_bytes();
    let task_bytes = tasks
        .capacity()
        .saturating_mul(std::mem::size_of::<NativeGeometrySolutionTask>());
    let base_resident_bytes = catalog_bytes
        .saturating_add(graph_bytes)
        .saturating_add(task_bytes);
    let task_split_peak_bytes = base_resident_bytes.saturating_add(task_split_scratch_bytes);
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
        base_resident_bytes,
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

    let mut resource_report = catalog.compile_resource_report().clone();
    merge_sequential_stage_metrics(&mut resource_report, &geometry_search.resource_report);
    resource_report.observe_cpu_bytes(task_split_peak_bytes);
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
    let peak_cpu_bytes = resource_report.peak_cpu_bytes.max(
        base_resident_bytes
            .saturating_add(reduction.workspace_bytes)
            .saturating_add(reduction.reducer_bytes),
    );
    resource_report.observe_cpu_bytes(peak_cpu_bytes);
    resource_report.observe_build_worker_backlog(0);
    match reduction.truncation {
        1 => resource_report.mark_truncated(ResourceTruncationReason::CandidateBudgetExceeded),
        2 => resource_report.mark_truncated(ResourceTruncationReason::MemoryExceeded),
        _ => {}
    }
    if problem.budget.has_max_memory_mib != 0 {
        let max_memory_bytes = (problem.budget.max_memory_mib as usize)
            .checked_mul(1024 * 1024)
            .unwrap_or(usize::MAX);
        if peak_cpu_bytes > max_memory_bytes {
            resource_report.mark_truncated(ResourceTruncationReason::MemoryExceeded);
        }
    }
    debug_assert!(reduction.buildable_count >= reduction.candidates.len());

    Ok(PackingBackendOutcome::exact(
        backend,
        reduction.candidates,
        resource_report,
        BackendTrustReport::cpu_exact(),
    )
    .with_workers_used(reduction.worker_count)
    .with_geometry_catalog(catalog)
    .with_pruning_ledger(pruning_ledger)
    .with_buildability_preverified())
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
