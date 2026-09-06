#![cfg(feature = "webgpu-search")]

use std::sync::Arc;

use clearra_core_domain::execution_cancellation::ExecutionCancellationToken;
#[cfg(test)]
use clearra_core_domain::resource::ResourceReport;
use clearra_core_ffi::{
    problem::{
        CPieceMultisetWindow, C_PIECE_I, C_PIECE_J, C_PIECE_L, C_PIECE_O, C_PIECE_S, C_PIECE_T,
        C_PIECE_Z,
    },
    CPackingProblem, CoreCNative, NativeGeometryCatalog, NativePruningLedger,
};
#[cfg(test)]
use clearra_core_ffi::{
    NativeCandidateReducer, NativePackingCandidateConsumer, NativePackingCandidateContext,
    NativePackingCandidateSinkError,
};
use clearra_coverage::pattern::pattern_bitset::PatternBitSet;
use clearra_pc_graph::request::{GpuDeviceSelection, PcExecutionPolicy};
use clearra_problem::SearchProblem;
use clearra_webgpu::{
    WebGpuAdapterSelection, WebGpuAdapterSummary, WebGpuExactCoverCatalog,
    WebGpuGeometryCatalogIdentity, WebGpuGeometryExactCoverBatch, WebGpuGeometryExactCoverOutcome,
    WebGpuGeometryExactCoverSession, WebGpuGeometryExactCoverSessionOutcome,
    WebGpuGeometryPathStreamError, WebGpuGeometrySolutionGraph,
};

#[cfg(all(test, feature = "native-c-core"))]
use clearra_webgpu::{enumerate_adapter_summaries, WebGpuGeometryExactCoverBackend};

use crate::packing::PackingRunnerError;
use crate::performance::{ExecutorSearchStage, SearchStageSpan};

use super::{
    buildable_geometry_task_reducer::{
        reduce_buildable_geometry_paths, BuildableGeometryPathError,
    },
    search_backend_warmup::connect_webgpu_session,
    BackendTrustReport, GpuDeviceSummary, GpuExecutionFailure, GpuExecutionFailureStage,
    GpuPartialResultDisposition, PackingBackendOutcome, SearchBackendFallbackReason,
    SelectedSearchBackend,
};

pub(crate) fn execute_webgpu_buildable_unique(
    search_problem: &SearchProblem,
    source_pattern_bits: Option<&PatternBitSet>,
    problem: &CPackingProblem,
    policy: &PcExecutionPolicy,
    cancellation: &ExecutionCancellationToken,
    actual_backend: SelectedSearchBackend,
    host_workers: usize,
) -> Result<PackingBackendOutcome, PackingRunnerError> {
    let gpu = execute_webgpu_geometry(problem, policy, cancellation, host_workers)?;
    let graph_bytes = gpu
        .graphs
        .iter()
        .map(WebGpuGeometrySolutionGraph::resident_bytes)
        .fold(0usize, usize::saturating_add);
    let base_resident_bytes = gpu.catalog.resident_bytes().saturating_add(graph_bytes);
    let reduction = reduce_buildable_geometry_paths(
        search_problem,
        source_pattern_bits,
        problem,
        &gpu.catalog,
        cancellation,
        host_workers,
        base_resident_bytes,
        |worker_index, worker_count, consumer| {
            for graph in &gpu.graphs {
                if consumer.should_stop() {
                    break;
                }
                let mut sink = |row_ids: &[u32]| consumer.consume_row_ids(row_ids);
                match graph.stream_partition_paths(worker_index, worker_count, &mut sink) {
                    Ok(_) => {}
                    Err(WebGpuGeometryPathStreamError::Consumer(
                        BuildableGeometryPathError::Stopped,
                    )) if consumer.was_truncated() || cancellation.is_cancelled() => break,
                    Err(WebGpuGeometryPathStreamError::Consumer(
                        BuildableGeometryPathError::Invalid,
                    )) => break,
                    Err(WebGpuGeometryPathStreamError::Consumer(
                        BuildableGeometryPathError::Stopped,
                    )) => break,
                    Err(WebGpuGeometryPathStreamError::InvalidGraph { .. }) => {
                        return Err(PackingRunnerError::GpuExecution(
                            GpuExecutionFailure::trust_mismatch(
                                GpuExecutionFailureStage::HostReduction,
                            ),
                        ));
                    }
                }
            }
            Ok(())
        },
    )?;
    let mut resource_report = *gpu.catalog.compile_resource_report();
    resource_report.observe_candidate_rows(reduction.generated_count);
    let peak_cpu_bytes = base_resident_bytes
        .saturating_add(reduction.workspace_bytes)
        .saturating_add(reduction.reducer_bytes)
        .max(gpu.peak_host_reduce_bytes);
    resource_report.observe_cpu_bytes(peak_cpu_bytes);
    resource_report.observe_gpu_bytes(gpu.peak_gpu_bytes as usize);
    match reduction.truncation {
        1 => resource_report.mark_truncated(
            clearra_core_domain::resource::ResourceTruncationReason::CandidateBudgetExceeded,
        ),
        2 => resource_report.mark_truncated(
            clearra_core_domain::resource::ResourceTruncationReason::MemoryExceeded,
        ),
        _ => {}
    }
    if problem.budget.has_max_memory_mib != 0 {
        let max_memory_bytes = (problem.budget.max_memory_mib as usize).saturating_mul(1024 * 1024);
        if peak_cpu_bytes > max_memory_bytes {
            resource_report.mark_truncated(
                clearra_core_domain::resource::ResourceTruncationReason::MemoryExceeded,
            );
        }
    }
    debug_assert!(reduction.generated_count >= reduction.buildable_count);
    debug_assert!(reduction.buildable_count >= reduction.candidates.len());
    let gpu_device = GpuDeviceSummary::from_execution(
        policy.gpu_device(),
        gpu.adapter.index(),
        gpu.adapter.name().to_owned(),
        gpu.adapter.device_type().as_str().to_owned(),
        gpu.adapter.backend().to_owned(),
        gpu.adapter.vendor(),
        gpu.adapter.device(),
    );
    let mut pruning_ledgers = vec![gpu.catalog.pruning_ledger().clone()];
    if let Some(buildability_ledger) = reduction.pruning_ledger {
        pruning_ledgers.push(buildability_ledger);
    }
    let pruning_ledger = NativePruningLedger::merge_partition_reports(pruning_ledgers)
        .map_err(clearra_core_ffi::NativeCoreError::InvalidPruningLedger)
        .map_err(PackingRunnerError::Native)?;
    Ok(PackingBackendOutcome::buildability_prefiltered_exact(
        actual_backend,
        reduction.candidates,
        resource_report,
        BackendTrustReport::gpu_cpu_confirmed(false),
    )
    .with_workers_used(reduction.worker_count)
    .with_gpu_device(gpu_device)
    .with_geometry_catalog(gpu.catalog)
    .with_pruning_ledger(pruning_ledger))
}

#[derive(Debug)]
struct NativeWebGpuExactCoverCatalog {
    native: NativeGeometryCatalog,
}

impl WebGpuExactCoverCatalog for NativeWebGpuExactCoverCatalog {
    fn identity(&self) -> WebGpuGeometryCatalogIdentity {
        let identity = self.native.identity();
        WebGpuGeometryCatalogIdentity::from_words([
            identity.board_layout_id,
            identity.compact_universe_digest,
            identity.target_geometry_digest,
            identity.piece_catalog_id,
            identity.skeleton_projection_version,
            identity.rule_capability_id,
            identity.realization_table_digest,
            identity.support_table_digest,
        ])
    }

    fn skeleton_cell_masks(&self) -> &[u64] {
        self.native.view().skeleton_cell_masks()
    }

    fn skeleton_piece_kinds(&self) -> &[u32] {
        self.native.view().skeleton_piece_kinds()
    }

    fn cell_support_offsets(&self) -> &[u32] {
        self.native.view().cell_support_offsets()
    }

    fn cell_support_row_ids(&self) -> &[u32] {
        self.native.view().cell_support_row_ids()
    }
}

#[cfg(test)]
pub(crate) fn execute_webgpu_packing(
    problem: &CPackingProblem,
    policy: &PcExecutionPolicy,
    cancellation: &ExecutionCancellationToken,
    actual_backend: SelectedSearchBackend,
    host_workers: usize,
) -> Result<PackingBackendOutcome, PackingRunnerError> {
    let gpu = execute_webgpu_geometry(problem, policy, cancellation, host_workers)?;
    let materialize_span = SearchStageSpan::begin(ExecutorSearchStage::PackingGpuCanonicalize);
    let mut reducer = NativeCandidateReducer::new(problem).map_err(|_| {
        PackingRunnerError::CandidateBatch(
            clearra_core_ffi::PackingCandidateBatchError::AllocationFailed,
        )
    })?;
    let (mut resource_report, emitted_count) =
        stream_gpu_graphs(problem, &gpu, cancellation, &mut reducer)?;
    let candidates = reducer.into_candidates();
    materialize_span.finish(emitted_count);
    resource_report.observe_gpu_bytes(gpu.peak_gpu_bytes as usize);
    let gpu_device = GpuDeviceSummary::from_execution(
        policy.gpu_device(),
        gpu.adapter.index(),
        gpu.adapter.name().to_owned(),
        gpu.adapter.device_type().as_str().to_owned(),
        gpu.adapter.backend().to_owned(),
        gpu.adapter.vendor(),
        gpu.adapter.device(),
    );
    Ok(PackingBackendOutcome::raw_geometry_exact(
        actual_backend,
        candidates,
        resource_report,
        BackendTrustReport::gpu_cpu_confirmed(false),
    )
    .with_workers_used(host_workers)
    .with_gpu_device(gpu_device)
    .with_geometry_catalog(gpu.catalog))
}

fn execute_webgpu_geometry(
    problem: &CPackingProblem,
    policy: &PcExecutionPolicy,
    cancellation: &ExecutionCancellationToken,
    host_workers: usize,
) -> Result<ExactGpuResult, PackingRunnerError> {
    if cancellation.is_cancelled() {
        return Err(PackingRunnerError::ExecutionCancelled);
    }
    let adapter_selection = webgpu_adapter_selection(policy.gpu_device());
    #[cfg(not(target_arch = "wasm32"))]
    let (batches, catalog, session) = std::thread::scope(|scope| {
        let connection = scope.spawn(move || connect_exact_webgpu_session(adapter_selection));
        let batch_plan_span = SearchStageSpan::begin(ExecutorSearchStage::PackingGpuBatchPlan);
        let batches = geometry_exact_cover_batches(problem, policy, cancellation);
        batch_plan_span.finish(
            batches
                .as_ref()
                .map_or(0, |(planned, _)| planned.len() as u64),
        );
        let session = match connection.join() {
            Ok(session) => session,
            Err(panic) => std::panic::resume_unwind(panic),
        };
        let (batches, catalog) = batches?;
        Ok::<_, PackingRunnerError>((batches, catalog, session?))
    })?;
    #[cfg(target_arch = "wasm32")]
    let (batches, catalog, session) = {
        let batch_plan_span = SearchStageSpan::begin(ExecutorSearchStage::PackingGpuBatchPlan);
        let (batches, catalog) = geometry_exact_cover_batches(problem, policy, cancellation)?;
        batch_plan_span.finish(batches.len() as u64);
        let session = connect_exact_webgpu_session(adapter_selection)?;
        (batches, catalog, session)
    };
    run_webgpu_batches(&batches, catalog, session, cancellation, host_workers)
}

struct ExactGpuResult {
    catalog: NativeGeometryCatalog,
    graphs: Vec<WebGpuGeometrySolutionGraph>,
    peak_gpu_bytes: u64,
    peak_host_reduce_bytes: usize,
    adapter: WebGpuAdapterSummary,
}

fn run_webgpu_batches(
    batches: &[WebGpuGeometryExactCoverBatch],
    catalog: NativeGeometryCatalog,
    mut session: WebGpuGeometryExactCoverSession,
    cancellation: &ExecutionCancellationToken,
    host_workers: usize,
) -> Result<ExactGpuResult, PackingRunnerError> {
    let mut graphs = Vec::new();
    let mut peak_gpu_bytes = 0_u64;
    let mut peak_host_reduce_bytes = 0_usize;
    let adapter = session.adapter().clone();
    let mut family_ranges = vec![(0_usize, batches.len())];
    while let Some((begin, end)) = family_ranges.pop() {
        if cancellation.is_cancelled() {
            return Err(PackingRunnerError::ExecutionCancelled);
        }
        let family = &batches[begin..end];
        let dispatch_span = SearchStageSpan::begin(ExecutorSearchStage::PackingGpuDispatchReadback);
        let outcome = pollster::block_on(session.run_family_with_host_workers_and_control(
            family,
            host_workers,
            &|| cancellation.is_cancelled(),
        ))
        .map_err(|_| {
            PackingRunnerError::GpuExecution(GpuExecutionFailure::invalid_request(
                GpuExecutionFailureStage::BatchPlanning,
            ))
        })?;
        dispatch_span.finish((end - begin) as u64);
        let canonical_span = SearchStageSpan::begin(ExecutorSearchStage::PackingGpuCanonicalize);
        match outcome {
            WebGpuGeometryExactCoverOutcome::Connected(result) => {
                if !result.can_claim_exact() {
                    return Err(PackingRunnerError::GpuExecution(
                        GpuExecutionFailure::trust_mismatch(
                            GpuExecutionFailureStage::HostReduction,
                        ),
                    ));
                }
                record_webgpu_stage_timings(result.timings());
                peak_gpu_bytes = peak_gpu_bytes.max(result.peak_gpu_bytes());
                peak_host_reduce_bytes =
                    peak_host_reduce_bytes.max(result.peak_host_reduce_bytes());
                graphs.push(result.into_solution_graph());
            }
            WebGpuGeometryExactCoverOutcome::Unavailable(_) => {
                return Err(PackingRunnerError::GpuExecution(
                    GpuExecutionFailure::unavailable(
                        GpuExecutionFailureStage::KernelExecution,
                        SearchBackendFallbackReason::GpuKernelUnavailable,
                    ),
                ));
            }
            WebGpuGeometryExactCoverOutcome::Cancelled => {
                return Err(PackingRunnerError::ExecutionCancelled);
            }
            WebGpuGeometryExactCoverOutcome::ResourceIncomplete(_) if end - begin > 1 => {
                canonical_span.finish(graphs.len() as u64);
                let middle = begin + (end - begin) / 2;
                family_ranges.push((middle, end));
                family_ranges.push((begin, middle));
                continue;
            }
            WebGpuGeometryExactCoverOutcome::ResourceIncomplete(_) => {
                return Err(PackingRunnerError::GpuExecution(
                    GpuExecutionFailure::resource_incomplete(
                        GpuExecutionFailureStage::KernelExecution,
                        GpuPartialResultDisposition::RetainedIncomplete,
                    )
                    .expect("WebGPU retained an incomplete frontier"),
                ));
            }
            WebGpuGeometryExactCoverOutcome::RejectedInvalidResult { .. } => {
                return Err(PackingRunnerError::GpuExecution(
                    GpuExecutionFailure::trust_mismatch(GpuExecutionFailureStage::HostReduction),
                ));
            }
            WebGpuGeometryExactCoverOutcome::RejectedTrustMismatch { .. } => {
                return Err(PackingRunnerError::GpuExecution(
                    GpuExecutionFailure::trust_mismatch(GpuExecutionFailureStage::HostReduction),
                ));
            }
        }
        canonical_span.finish(graphs.len() as u64);
    }
    peak_host_reduce_bytes = peak_host_reduce_bytes.max(
        graphs
            .iter()
            .map(WebGpuGeometrySolutionGraph::resident_bytes)
            .fold(0usize, usize::saturating_add),
    );
    let result = ExactGpuResult {
        catalog,
        graphs,
        peak_gpu_bytes,
        peak_host_reduce_bytes,
        adapter,
    };
    session.recycle();
    Ok(result)
}

#[cfg(test)]
fn stream_gpu_graphs(
    problem: &CPackingProblem,
    gpu: &ExactGpuResult,
    cancellation: &ExecutionCancellationToken,
    consumer: &mut dyn NativePackingCandidateConsumer,
) -> Result<(ResourceReport, u64), PackingRunnerError> {
    let max_candidate_rows = if problem.budget.max_results == 0 {
        usize::MAX
    } else {
        problem.budget.max_results as usize
    };
    let max_total_bytes = if problem.budget.has_max_memory_mib == 0 {
        usize::MAX
    } else {
        (problem.budget.max_memory_mib as usize)
            .saturating_mul(1024)
            .saturating_mul(1024)
    };
    let graph_bytes = gpu
        .graphs
        .iter()
        .map(WebGpuGeometrySolutionGraph::resident_bytes)
        .fold(0usize, usize::saturating_add);
    let engine_resident_bytes = gpu.catalog.resident_bytes().saturating_add(graph_bytes);
    let mut accepted_count = 0usize;
    let mut generated_count = 0usize;
    for graph in &gpu.graphs {
        let mut consume_path = |row_ids: &[u32]| -> Result<(), PackingRunnerError> {
            if cancellation.is_cancelled() {
                return Err(PackingRunnerError::ExecutionCancelled);
            }
            let mut candidate = gpu
                .catalog
                .materialize_row_ids(problem, row_ids)
                .map_err(PackingRunnerError::Native)?;
            generated_count = generated_count.saturating_add(1);
            candidate.candidate_id = generated_count as u64;
            candidate.canonical_operation_set_id = candidate.candidate_id;
            let context = NativePackingCandidateContext {
                accepted_candidate_count: accepted_count,
                engine_resident_bytes,
                max_candidate_rows,
                max_total_bytes,
            };
            if consumer
                .consume(candidate, context)
                .map_err(map_candidate_sink_error)?
            {
                accepted_count = accepted_count.saturating_add(1);
            }
            Ok(())
        };
        graph
            .stream_partition_paths(0, 1, &mut consume_path)
            .map_err(|error| match error {
                WebGpuGeometryPathStreamError::Consumer(error) => error,
                WebGpuGeometryPathStreamError::InvalidGraph { .. } => {
                    PackingRunnerError::GpuExecution(GpuExecutionFailure::trust_mismatch(
                        GpuExecutionFailureStage::HostReduction,
                    ))
                }
            })?;
    }
    let mut report = *gpu.catalog.compile_resource_report();
    report.observe_candidate_rows(generated_count);
    report.observe_cpu_bytes(
        engine_resident_bytes
            .saturating_add(consumer.resident_bytes())
            .max(gpu.peak_host_reduce_bytes),
    );
    Ok((report, generated_count as u64))
}

#[cfg(test)]
fn map_candidate_sink_error(error: NativePackingCandidateSinkError) -> PackingRunnerError {
    match error {
        NativePackingCandidateSinkError::CandidateBudgetExceeded
        | NativePackingCandidateSinkError::MemoryExceeded => PackingRunnerError::GpuExecution(
            GpuExecutionFailure::resource_incomplete(
                GpuExecutionFailureStage::HostReduction,
                GpuPartialResultDisposition::RetainedIncomplete,
            )
            .expect("GPU host reduction retained incomplete data"),
        ),
        NativePackingCandidateSinkError::Invalid => PackingRunnerError::GpuExecution(
            GpuExecutionFailure::trust_mismatch(GpuExecutionFailureStage::HostReduction),
        ),
    }
}

fn connect_exact_webgpu_session(
    adapter_selection: WebGpuAdapterSelection,
) -> Result<WebGpuGeometryExactCoverSession, PackingRunnerError> {
    let connect_span = SearchStageSpan::begin(ExecutorSearchStage::PackingGpuConnect);
    let outcome = connect_webgpu_session(adapter_selection);
    connect_span.finish(1);
    match outcome {
        WebGpuGeometryExactCoverSessionOutcome::Connected(session) => Ok(session),
        WebGpuGeometryExactCoverSessionOutcome::Unavailable(_) => Err(
            PackingRunnerError::GpuExecution(GpuExecutionFailure::unavailable(
                GpuExecutionFailureStage::CapabilityQuery,
                SearchBackendFallbackReason::GpuKernelUnavailable,
            )),
        ),
    }
}

fn record_webgpu_stage_timings(timings: clearra_webgpu::WebGpuGeometryExactCoverTimings) {
    let layer_count = timings.layer_dispatch_count();
    let generated_records = timings.generated_record_count();
    SearchStageSpan::record_elapsed(
        ExecutorSearchStage::PackingGpuHostPrepareSubmit,
        profile_duration(timings.host_prepare_submit_ns()),
        layer_count,
    );
    SearchStageSpan::record_elapsed(
        ExecutorSearchStage::PackingGpuDispatchCounterWait,
        profile_duration(timings.dispatch_counter_wait_ns()),
        layer_count,
    );
    SearchStageSpan::record_elapsed(
        ExecutorSearchStage::PackingGpuPayloadReadback,
        profile_duration(timings.payload_readback_ns()),
        generated_records,
    );
    SearchStageSpan::record_elapsed(
        ExecutorSearchStage::PackingGpuHostReduce,
        profile_duration(timings.exact_host_reduce_ns()),
        generated_records,
    );
    SearchStageSpan::record_elapsed(
        ExecutorSearchStage::PackingGpuTraceEnumeration,
        profile_duration(timings.trace_enumeration_ns()),
        generated_records,
    );
}

fn profile_duration(elapsed_ns: u64) -> Option<std::time::Duration> {
    (elapsed_ns > 0).then(|| std::time::Duration::from_nanos(elapsed_ns))
}

fn webgpu_adapter_selection(selection: &GpuDeviceSelection) -> WebGpuAdapterSelection {
    match selection {
        GpuDeviceSelection::Auto => WebGpuAdapterSelection::Auto,
        GpuDeviceSelection::Index(index) => WebGpuAdapterSelection::Index(*index),
    }
}

fn geometry_exact_cover_batches(
    problem: &CPackingProblem,
    policy: &PcExecutionPolicy,
    cancellation: &ExecutionCancellationToken,
) -> Result<(Vec<WebGpuGeometryExactCoverBatch>, NativeGeometryCatalog), PackingRunnerError> {
    let width = u8::try_from(problem.board.width).map_err(|_| invalid_batch_error())?;
    let height = u8::try_from(if problem.board.search_height == 0 {
        problem.board.visible_height
    } else {
        problem.board.search_height
    })
    .map_err(|_| invalid_batch_error())?;
    let native_catalog =
        CoreCNative::compile_geometry_catalog_with_cancellation(problem, cancellation)
            .map_err(PackingRunnerError::Native)?;
    let gpu_catalog: Arc<dyn WebGpuExactCoverCatalog> = Arc::new(NativeWebGpuExactCoverCatalog {
        native: native_catalog.clone(),
    });
    let family_count = usize::from(problem.piece_multiset_family.count);
    if family_count > problem.piece_multiset_family.members.len() {
        return Err(invalid_batch_error());
    }
    let batch_count = family_count.max(1);
    let window_at = |index: usize| {
        if family_count == 0 {
            problem.piece_multiset_window
        } else {
            problem.piece_multiset_family.members[index]
        }
    };
    let first_window = window_at(0);
    let first = WebGpuGeometryExactCoverBatch::from_catalog(
        width,
        height,
        problem.board.initial_mask,
        problem.goal_region_mask,
        problem.required_fill_mask,
        problem.forbidden_mask,
        piece_counts(first_window),
        gpu_catalog,
        policy.max_frontier_states(),
    )
    .map_err(|_| invalid_batch_error())?;
    let mut batches = Vec::with_capacity(batch_count);
    batches.push(first);
    for index in 1..batch_count {
        let batch = WebGpuGeometryExactCoverBatch::from_shared_geometry(
            &batches[0],
            piece_counts(window_at(index)),
            policy.max_frontier_states(),
        )
        .map_err(|_| invalid_batch_error())?;
        batches.push(batch);
    }
    Ok((batches, native_catalog))
}

#[cfg(test)]
const MAX_CANONICAL_GEOMETRY_OPERATIONS: usize = 15;

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct CanonicalGeometryCandidate {
    operation_count: u8,
    piece_counts: [u8; 7],
    masks: [u64; MAX_CANONICAL_GEOMETRY_OPERATIONS],
}

#[cfg(test)]
fn canonical_cpu_geometry_candidates(
    candidates: &clearra_core_ffi::PackingCandidateBatch,
) -> Option<Vec<CanonicalGeometryCandidate>> {
    let mut canonical = Vec::with_capacity(candidates.len());
    for candidate in candidates.iter() {
        canonical.push(canonical_geometry_candidate(
            candidate.operations[..usize::from(candidate.operation_count)]
                .iter()
                .map(|operation| (operation.piece, operation.mask)),
        )?);
    }
    canonical.sort_unstable();
    canonical.dedup();
    Some(canonical)
}

#[cfg(test)]
fn canonical_geometry_candidate(
    operations: impl IntoIterator<Item = (u8, u64)>,
) -> Option<CanonicalGeometryCandidate> {
    let mut entries = [(0_u8, 0_u64); MAX_CANONICAL_GEOMETRY_OPERATIONS];
    let mut operation_count = 0_usize;
    for operation in operations {
        if operation_count == entries.len() || operation.0 == 0 || operation.0 > 7 {
            return None;
        }
        entries[operation_count] = operation;
        operation_count += 1;
    }
    if operation_count == 0 {
        return None;
    }
    entries[..operation_count].sort_unstable();
    let mut piece_counts = [0_u8; 7];
    let mut masks = [0_u64; MAX_CANONICAL_GEOMETRY_OPERATIONS];
    for (index, (piece, mask)) in entries[..operation_count].iter().copied().enumerate() {
        piece_counts[usize::from(piece - 1)] =
            piece_counts[usize::from(piece - 1)].checked_add(1)?;
        masks[index] = mask;
    }
    Some(CanonicalGeometryCandidate {
        operation_count: u8::try_from(operation_count).ok()?,
        piece_counts,
        masks,
    })
}

fn piece_counts(window: CPieceMultisetWindow) -> [u8; 7] {
    [
        window.counts[usize::from(C_PIECE_I)],
        window.counts[usize::from(C_PIECE_O)],
        window.counts[usize::from(C_PIECE_T)],
        window.counts[usize::from(C_PIECE_S)],
        window.counts[usize::from(C_PIECE_Z)],
        window.counts[usize::from(C_PIECE_J)],
        window.counts[usize::from(C_PIECE_L)],
    ]
}

fn invalid_batch_error() -> PackingRunnerError {
    PackingRunnerError::GpuExecution(GpuExecutionFailure::invalid_request(
        GpuExecutionFailureStage::BatchPlanning,
    ))
}

#[cfg(all(test, feature = "native-c-core"))]
mod tests {
    use clearra_core_domain::{pc::pc_target::PcTarget, piece::piece_kind::PieceKind};
    use clearra_core_ffi::CPackingProblemBuilder;
    use clearra_pc_graph::request::{
        OpeningPcSearchQuery, PcHoldPolicy, PcQueueInput, RequestedSearchBackend,
    };
    use clearra_problem::ProblemCompiler;
    use clearra_supply::queue::fixed_sequence::FixedSequence;

    use super::*;
    use crate::backend::{BackendTrustState, NativeGpuPackingExecutor, SearchBackendExecutor};
    use crate::packing::packing_native_bridge::native_packing_outcome;

    #[test]
    fn gpu_executes_webgpu_with_cpu_sample_confirmation() {
        if !pollster::block_on(WebGpuGeometryExactCoverBackend::adapter_available()) {
            return;
        }

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
            .with_backend(RequestedSearchBackend::Gpu)
            .with_max_frontier_states(65_536);

        let outcome = NativeGpuPackingExecutor
            .execute_packing(&compact, &policy, &ExecutionCancellationToken::new())
            .expect("CPU-confirmed WebGPU packing");

        assert_eq!(outcome.actual_backend, SelectedSearchBackend::Gpu);
        assert!(!outcome.candidates.is_empty());
        assert_eq!(
            outcome.trust_report.state(),
            BackendTrustState::GpuComputedCpuConfirmed
        );
        assert!(outcome.trust_report.cpu_confirmed());
        assert!(!outcome.trust_report.deterministic_reference_matched());
        assert!(outcome.trust_report.can_source_exact_probability());
        assert!(outcome.resource_report.peak_gpu_bytes > 0);
        let adapters =
            pollster::block_on(enumerate_adapter_summaries()).expect("adapter inventory");
        if adapters.iter().any(|adapter| {
            adapter.device_type() == clearra_webgpu::WebGpuAdapterDeviceType::DiscreteGpu
        }) {
            assert_eq!(
                outcome
                    .gpu_device
                    .as_ref()
                    .and_then(GpuDeviceSummary::selected_device_type),
                Some(clearra_webgpu::WebGpuAdapterDeviceType::DiscreteGpu.as_str())
            );
        }
    }

    #[test]
    fn gpu_uses_explicit_runtime_adapter_index_and_reports_actual_device() {
        let adapters =
            pollster::block_on(enumerate_adapter_summaries()).expect("adapter inventory");
        let Some(adapter) = adapters
            .iter()
            .find(|adapter| adapter.device_type() != clearra_webgpu::WebGpuAdapterDeviceType::Cpu)
        else {
            return;
        };
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
            .with_backend(RequestedSearchBackend::Gpu)
            .with_gpu_device(GpuDeviceSelection::Index(adapter.index()))
            .with_max_frontier_states(65_536);

        let outcome = NativeGpuPackingExecutor
            .execute_packing(&compact, &policy, &ExecutionCancellationToken::new())
            .expect("explicit adapter execution");
        let reported = outcome.gpu_device.expect("actual GPU device report");

        assert_eq!(reported.requested(), adapter.index().to_string());
        assert_eq!(reported.selected_index(), Some(adapter.index()));
        assert_eq!(reported.selected_name(), Some(adapter.name()));
        assert_eq!(
            reported.selected_device_type(),
            Some(adapter.device_type().as_str())
        );
        assert_eq!(reported.selected_backend(), Some(adapter.backend()));
    }

    #[test]
    fn four_line_target_frame_projections_match_cpu_reference() {
        if !pollster::block_on(WebGpuGeometryExactCoverBackend::adapter_available()) {
            return;
        }

        let query = OpeningPcSearchQuery::new(PcTarget::four_lines())
            .with_queue(PcQueueInput::fixed_sequence(FixedSequence::new(vec![
                PieceKind::I,
                PieceKind::I,
                PieceKind::O,
                PieceKind::O,
                PieceKind::O,
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
            .with_backend(RequestedSearchBackend::Gpu)
            .with_max_frontier_states(65_536);
        let cancellation = ExecutionCancellationToken::new();
        let cpu = native_packing_outcome(&compact, &cancellation)
            .expect("CPU reference call")
            .expect("native CPU reference");
        let session = connect_exact_webgpu_session(WebGpuAdapterSelection::Auto)
            .expect("connected WebGPU session");
        let (batches, catalog) =
            geometry_exact_cover_batches(&compact, &policy, &cancellation).expect("WebGPU batches");
        let host_workers =
            clearra_core_domain::runtime_cpu_capacity::CpuCapacity::current().hard_limit();
        let gpu = run_webgpu_batches(&batches, catalog, session, &cancellation, host_workers)
            .expect("WebGPU packing");
        let mut materializer = NativeCandidateReducer::new(&compact).expect("candidate reducer");
        stream_gpu_graphs(&compact, &gpu, &cancellation, &mut materializer)
            .expect("C canonical GPU materialization");
        let materialized = materializer.into_candidates();
        let cpu_candidates =
            canonical_cpu_geometry_candidates(&cpu.candidates).expect("valid CPU candidates");
        let catalog_view = gpu.catalog.view();
        let mut gpu_candidates = Vec::new();
        for graph in &gpu.graphs {
            graph
                .stream_partition_paths(0, 1, &mut |row_ids| {
                    gpu_candidates.push(
                        canonical_geometry_candidate(row_ids.iter().map(|index| {
                            let index = *index as usize;
                            (
                                catalog_view.skeleton_piece_kinds()[index] as u8,
                                catalog_view.skeleton_cell_masks()[index],
                            )
                        }))
                        .expect("valid GPU geometry path"),
                    );
                    Ok::<(), ()>(())
                })
                .expect("stream GPU geometry graph");
        }
        gpu_candidates.sort_unstable();
        gpu_candidates.dedup();
        assert_eq!(
            cpu_candidates, gpu_candidates,
            "WebGPU and C CPU must preserve the same canonical packing geometry sets"
        );
        assert_eq!(
            exact_candidate_payloads(&cpu.candidates),
            exact_candidate_payloads(&materialized),
            "GPU geometry paths materialized by C must match every exact CPU candidate field"
        );
    }

    fn exact_candidate_payloads(
        candidates: &clearra_core_ffi::PackingCandidateBatch,
    ) -> Vec<Vec<u64>> {
        let mut payloads = candidates
            .iter()
            .map(|candidate| {
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
            })
            .collect::<Vec<_>>();
        payloads.sort_unstable();
        payloads.dedup();
        payloads
    }
}
