use std::{
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll, Waker},
};

use clearra_core_domain::{execution_cancellation::ExecutionControl, piece::piece_kind::PieceKind};
use clearra_pc_graph::request::{GpuDeviceSelection, PcExecutionPolicy};
use clearra_problem::SearchProblem;
use clearra_webgpu::{
    WebGpuAdapterSelection, WebGpuExactCoverCatalog, WebGpuGeometryCatalogIdentity,
    WebGpuGeometryExactCoverBackend, WebGpuGeometryExactCoverBatch,
    WebGpuGeometryExactCoverInputError, WebGpuGeometryExactCoverOutcome,
    WebGpuGeometryExactCoverSessionOutcome, WebGpuGeometryPathCursor, WebGpuGeometrySolutionGraph,
};

use crate::backend::{
    GpuExecutionFailure, GpuExecutionFailureStage, GpuFailureDisposition,
    GpuPartialResultDisposition, SearchBackendFallbackReason,
};
use crate::performance::{ExecutorSearchStage, SearchStageSpan};

use super::{
    catalog::GeometryCatalog,
    result::{ExactSearchAdvance, WasmExactSearchSession},
    WasmExactSearchError,
};

pub(super) type GpuRunFuture = Pin<Box<dyn Future<Output = Result<GpuRunResult, GpuRunFailure>>>>;

pub(crate) struct WasmWebGpuSearchSession {
    problem: Arc<SearchProblem>,
    exact: Option<WasmExactSearchSession>,
    phase: WebGpuSearchPhase,
    fallback_allowed: bool,
}

// The active GPU search phase is retained in place for the session lifetime.
#[allow(clippy::large_enum_variant)]
enum WebGpuSearchPhase {
    Preparing {
        selection: WebGpuAdapterSelection,
        warmup_requested: bool,
    },
    Prepared {
        batches: Vec<WebGpuGeometryExactCoverBatch>,
        selection: WebGpuAdapterSelection,
        warmup_requested: bool,
    },
    Dispatching(GpuRunFuture),
    Reducing(GpuReduction),
    CpuFallback(WasmExactSearchSession),
    Finished,
}

pub(super) struct GpuReduction {
    pub(super) graphs: Vec<GpuGraph>,
    pub(super) graph_cursor: usize,
    pub(super) path_cursor: Option<WebGpuGeometryPathCursor>,
    pub(super) adapter_index: u8,
    pub(super) adapter_name: String,
    pub(super) adapter_type: &'static str,
    pub(super) adapter_backend: String,
    pub(super) peak_gpu_bytes: u64,
    pub(super) peak_frontier_states: usize,
    pub(super) shader_hash: String,
    pub(super) shader_version: &'static str,
    pub(super) warmup_performed: bool,
    pub(super) session_reused: bool,
    pub(super) expanded_records: usize,
}

pub(super) struct GpuGraph {
    pub(super) target_begin: usize,
    pub(super) graph: Arc<WebGpuGeometrySolutionGraph>,
}

pub(super) struct GpuRunResult {
    pub(super) reduction: GpuReduction,
}

pub(super) enum GpuRunFailure {
    Cancelled,
    Execution(GpuExecutionFailure),
    TrustMismatch(&'static str),
}

#[derive(Debug)]
struct WasmGpuCatalog {
    identity: WebGpuGeometryCatalogIdentity,
    masks: Box<[u64]>,
    pieces: Box<[u32]>,
    support_offsets: Box<[u32]>,
    support_rows: Box<[u32]>,
    constraint_words: Box<[u32]>,
}

impl WebGpuExactCoverCatalog for WasmGpuCatalog {
    fn identity(&self) -> WebGpuGeometryCatalogIdentity {
        self.identity
    }

    fn skeleton_cell_masks(&self) -> &[u64] {
        &self.masks
    }

    fn skeleton_piece_kinds(&self) -> &[u32] {
        &self.pieces
    }

    fn cell_support_offsets(&self) -> &[u32] {
        &self.support_offsets
    }

    fn cell_support_row_ids(&self) -> &[u32] {
        &self.support_rows
    }

    fn certified_constraint_words(&self) -> &[u32] {
        &self.constraint_words
    }
}

impl WasmWebGpuSearchSession {
    pub fn new(problem: &SearchProblem) -> Result<Self, WasmExactSearchError> {
        let exact = WasmExactSearchSession::new_external_geometry(problem)?;
        Ok(Self {
            problem: Arc::new(problem.clone()),
            exact: Some(exact),
            phase: WebGpuSearchPhase::Preparing {
                selection: adapter_selection(problem.backend_policy().gpu_device()),
                warmup_requested: problem.backend_policy().gpu_warmup(),
            },
            fallback_allowed: problem.backend_policy().allow_backend_fallback(),
        })
    }

    pub fn advance(
        &mut self,
        work_budget: usize,
        control: &ExecutionControl,
    ) -> Result<ExactSearchAdvance, WasmExactSearchError> {
        if control.is_cancelled() {
            return Ok(ExactSearchAdvance::Cancelled);
        }
        loop {
            let phase = std::mem::replace(&mut self.phase, WebGpuSearchPhase::Finished);
            match phase {
                WebGpuSearchPhase::Preparing {
                    selection,
                    warmup_requested,
                } => {
                    let exact = self
                        .exact
                        .as_mut()
                        .ok_or(WasmExactSearchError::InvalidProblem(
                            "webgpu_exact_session_missing",
                        ))?;
                    if !exact.advance_external_geometry_preparation(control)? {
                        self.phase = WebGpuSearchPhase::Preparing {
                            selection,
                            warmup_requested,
                        };
                        return Ok(ExactSearchAdvance::Pending);
                    }
                    let batch_plan_span =
                        SearchStageSpan::begin(ExecutorSearchStage::PackingGpuBatchPlan);
                    let batches = compile_batches(exact, self.problem.backend_policy())?;
                    batch_plan_span.finish(batches.len() as u64);
                    self.phase = WebGpuSearchPhase::Prepared {
                        batches,
                        selection,
                        warmup_requested,
                    };
                    return Ok(ExactSearchAdvance::Pending);
                }
                WebGpuSearchPhase::Prepared {
                    batches,
                    selection,
                    warmup_requested,
                } => {
                    self.phase = WebGpuSearchPhase::Dispatching(Box::pin(run_gpu(
                        batches,
                        selection,
                        warmup_requested,
                        control.clone(),
                    )));
                    return Ok(ExactSearchAdvance::Pending);
                }
                WebGpuSearchPhase::Dispatching(mut future) => match poll_once(&mut future) {
                    Poll::Pending => {
                        self.phase = WebGpuSearchPhase::Dispatching(future);
                        return Ok(ExactSearchAdvance::Pending);
                    }
                    Poll::Ready(Ok(result)) => {
                        self.phase = WebGpuSearchPhase::Reducing(result.reduction);
                    }
                    Poll::Ready(Err(GpuRunFailure::Cancelled)) => {
                        return Ok(ExactSearchAdvance::Cancelled);
                    }
                    Poll::Ready(Err(GpuRunFailure::TrustMismatch(reason))) => {
                        return Err(WasmExactSearchError::InvalidProblem(reason));
                    }
                    Poll::Ready(Err(GpuRunFailure::Execution(failure))) => {
                        let fallback_policy = if self.fallback_allowed {
                            clearra_pc_graph::request::BackendFallbackPolicy::Allow
                        } else {
                            clearra_pc_graph::request::BackendFallbackPolicy::Deny
                        };
                        let resolution = failure.resolve(fallback_policy);
                        if resolution.fallback_used() {
                            // A score-requested exact session leases the full
                            // configured host cap. Release the failed GPU
                            // coordinator lease before admitting its CPU
                            // replacement; otherwise the replacement can only
                            // observe self-inflicted shared-cap contention.
                            self.exact = None;
                            let mut cpu = WasmExactSearchSession::new(&self.problem)?;
                            cpu.mark_cpu_fallback(
                                resolution
                                    .backend_fallback_reason()
                                    .map_or("webgpu_fallback", |reason| reason.as_str()),
                                resolution.class().as_str(),
                                resolution.stage().as_str(),
                                resolution.discarded_partial_gpu_result(),
                                resolution.original_gpu_result_incomplete(),
                            );
                            self.phase = WebGpuSearchPhase::CpuFallback(cpu);
                            continue;
                        }
                        return Err(WasmExactSearchError::InvalidProblem(
                            match resolution.disposition() {
                                GpuFailureDisposition::Unavailable => "webgpu_backend_unavailable",
                                GpuFailureDisposition::TransientFailure => {
                                    "webgpu_transient_before_commit"
                                }
                                GpuFailureDisposition::Incomplete => "webgpu_resource_incomplete",
                                GpuFailureDisposition::InvalidRequest => "webgpu_invalid_request",
                                GpuFailureDisposition::RejectedMismatch => "webgpu_trust_mismatch",
                                GpuFailureDisposition::FatalInternal => "webgpu_fatal_internal",
                                GpuFailureDisposition::CpuFallback
                                | GpuFailureDisposition::CpuRerunAfterIncomplete => {
                                    "webgpu_fallback_resolution_inconsistent"
                                }
                            },
                        ));
                    }
                },
                WebGpuSearchPhase::Reducing(mut reduction) => {
                    let Some(exact) = self.exact.as_mut() else {
                        return Err(WasmExactSearchError::InvalidProblem(
                            "webgpu_exact_session_missing",
                        ));
                    };
                    let mut processed = 0usize;
                    while processed < work_budget.max(1) {
                        if control.is_cancelled() {
                            return Ok(ExactSearchAdvance::Cancelled);
                        }
                        if reduction.path_cursor.is_none() {
                            let Some(graph) = reduction.graphs.get(reduction.graph_cursor) else {
                                exact.mark_webgpu_execution(
                                    reduction.adapter_index,
                                    reduction.adapter_name,
                                    reduction.adapter_type,
                                    reduction.adapter_backend,
                                    reduction.peak_gpu_bytes,
                                    reduction.shader_hash,
                                    reduction.shader_version,
                                    reduction.warmup_performed,
                                    reduction.session_reused,
                                );
                                self.phase = WebGpuSearchPhase::Finished;
                                return exact.complete_external_geometry(
                                    reduction.expanded_records,
                                    reduction.peak_frontier_states,
                                );
                            };
                            reduction.path_cursor = Some(graph.graph.path_cursor());
                        }
                        let cursor = reduction.path_cursor.as_mut().ok_or(
                            WasmExactSearchError::InvalidProblem("webgpu_path_cursor_missing"),
                        )?;
                        match cursor.next_path() {
                            Ok(Some(path)) => {
                                let graph = &reduction.graphs[reduction.graph_cursor];
                                let target_index = graph
                                    .target_begin
                                    .checked_add(path.batch_index() as usize)
                                    .and_then(|index| u32::try_from(index).ok())
                                    .ok_or(WasmExactSearchError::InvalidProblem(
                                        "webgpu_target_index_overflow",
                                    ))?;
                                if let Some(outcome) = exact.process_external_candidate(
                                    target_index,
                                    path.operation_indices(),
                                    control,
                                )? {
                                    self.phase = WebGpuSearchPhase::Finished;
                                    return Ok(outcome);
                                }
                                processed += 1;
                            }
                            Ok(None) => {
                                reduction.graph_cursor += 1;
                                reduction.path_cursor = None;
                            }
                            Err(_) => {
                                return Err(WasmExactSearchError::InvalidProblem(
                                    "webgpu_solution_graph_invalid",
                                ));
                            }
                        }
                    }
                    self.phase = WebGpuSearchPhase::Reducing(reduction);
                    control.report_progress("webgpu-buildup", processed as u64, None);
                    return Ok(ExactSearchAdvance::Pending);
                }
                WebGpuSearchPhase::CpuFallback(mut cpu) => {
                    let outcome = cpu.advance(work_budget, control)?;
                    if matches!(outcome, ExactSearchAdvance::Pending) {
                        self.phase = WebGpuSearchPhase::CpuFallback(cpu);
                    } else {
                        // Preserve the completed/final CPU session so the
                        // terminal memory authority keeps its admission lease
                        // alive through public post-processing.
                        self.exact = Some(cpu);
                    }
                    return Ok(outcome);
                }
                WebGpuSearchPhase::Finished => {
                    return Err(WasmExactSearchError::InvalidProblem(
                        "wasm_search_session_already_finished",
                    ));
                }
            }
        }
    }

    pub(crate) fn validate_public_result_memory_with_future(
        &self,
        result: &crate::CoreExecutionResult,
        checked_future_bytes: u128,
    ) -> Result<(), WasmExactSearchError> {
        self.exact
            .as_ref()
            .ok_or(WasmExactSearchError::InvalidProblem(
                "webgpu_terminal_exact_session_missing",
            ))?
            .validate_public_result_memory_with_future(result, checked_future_bytes)
    }
}

pub(super) fn compile_batches(
    session: &WasmExactSearchSession,
    policy: &PcExecutionPolicy,
) -> Result<Vec<WebGpuGeometryExactCoverBatch>, WasmExactSearchError> {
    let catalog = session.catalog();
    let gpu_catalog: Arc<dyn WebGpuExactCoverCatalog> =
        Arc::new(WasmGpuCatalog::from_catalog(&catalog)?);
    let targets = session
        .geometry_targets()
        .ok_or(WasmExactSearchError::InvalidProblem(
            "webgpu_target_groups_missing",
        ))?;
    let first_target = targets.first().ok_or(WasmExactSearchError::InvalidProblem(
        "webgpu_target_groups_empty",
    ))?;
    let goal_mask = catalog.initial_board() | catalog.required_cells();
    let first = WebGpuGeometryExactCoverBatch::from_catalog(
        catalog.width(),
        catalog.height(),
        catalog.initial_board(),
        goal_mask,
        catalog.required_cells(),
        0,
        first_target.key.counts(),
        gpu_catalog,
        policy.max_frontier_states(),
    )
    .map_err(|_| WasmExactSearchError::InvalidProblem("webgpu_batch_invalid"))?;
    let mut batches = Vec::new();
    batches
        .try_reserve_exact(targets.len())
        .map_err(|_| WasmExactSearchError::InvalidProblem("webgpu_batch_storage_unavailable"))?;
    batches.push(first);
    for target in targets.iter().skip(1) {
        batches.push(
            WebGpuGeometryExactCoverBatch::from_shared_geometry(
                &batches[0],
                target.key.counts(),
                policy.max_frontier_states(),
            )
            .map_err(|_| WasmExactSearchError::InvalidProblem("webgpu_batch_invalid"))?,
        );
    }
    Ok(batches)
}

impl WasmGpuCatalog {
    fn from_catalog(catalog: &GeometryCatalog) -> Result<Self, WasmExactSearchError> {
        let mut masks = Vec::new();
        let mut pieces = Vec::new();
        masks
            .try_reserve_exact(catalog.skeleton_count())
            .map_err(|_| {
                WasmExactSearchError::InvalidProblem("webgpu_catalog_storage_unavailable")
            })?;
        pieces
            .try_reserve_exact(catalog.skeleton_count())
            .map_err(|_| {
                WasmExactSearchError::InvalidProblem("webgpu_catalog_storage_unavailable")
            })?;
        for row_id in 0..catalog.skeleton_count() {
            let row = catalog.skeleton(row_id as u32);
            masks.push(row.cells);
            pieces.push(piece_code(row.piece));
        }
        let mut constraint_words = Vec::new();
        constraint_words
            .try_reserve_exact(4 + 7 * catalog.width() as usize)
            .map_err(|_| {
                WasmExactSearchError::InvalidProblem("webgpu_constraint_storage_unavailable")
            })?;
        let checker_flag = u32::from(
            catalog
                .projection_catalog()
                .standard_checker_rule_certified(),
        ) << 1;
        constraint_words.push(1 | checker_flag);
        constraint_words.push(catalog.separator_catalog().safe_column_bits() as u32);
        constraint_words.push((catalog.separator_catalog().safe_column_bits() >> 32) as u32);
        constraint_words.push(u32::from(catalog.width()));
        for piece in 0..7 {
            for column in 0..catalog.width() as usize {
                let (minimum, maximum) = catalog
                    .projection_catalog()
                    .piece_column_bounds(piece, column);
                constraint_words.push(u32::from(minimum) | (u32::from(maximum) << 8));
            }
        }
        Ok(Self {
            identity: WebGpuGeometryCatalogIdentity::from_words([
                catalog.identity_digest(),
                catalog.initial_board(),
                catalog.required_cells(),
                u64::from(catalog.width()),
                u64::from(catalog.height()),
                catalog.skeleton_count() as u64,
                catalog.realization_count() as u64,
                1,
            ]),
            masks: masks.into_boxed_slice(),
            pieces: pieces.into_boxed_slice(),
            support_offsets: catalog.support_offsets().to_vec().into_boxed_slice(),
            support_rows: catalog.support_rows().to_vec().into_boxed_slice(),
            constraint_words: constraint_words.into_boxed_slice(),
        })
    }
}

pub(super) async fn run_gpu(
    batches: Vec<WebGpuGeometryExactCoverBatch>,
    selection: WebGpuAdapterSelection,
    warmup_requested: bool,
    control: ExecutionControl,
) -> Result<GpuRunResult, GpuRunFailure> {
    if control.is_cancelled() {
        return Err(GpuRunFailure::Cancelled);
    }
    let connect_span = SearchStageSpan::begin(ExecutorSearchStage::PackingGpuConnect);
    let session_outcome = WebGpuGeometryExactCoverBackend::connect_selected(selection).await;
    connect_span.finish(1);
    let mut session = match session_outcome {
        WebGpuGeometryExactCoverSessionOutcome::Connected(session) => session,
        WebGpuGeometryExactCoverSessionOutcome::Unavailable(unavailable) => {
            return Err(GpuRunFailure::Execution(GpuExecutionFailure::unavailable(
                GpuExecutionFailureStage::CapabilityQuery,
                unavailable_reason(unavailable.reason()),
            )))
        }
    };
    let adapter_index = session.adapter().index();
    let session_reused = session.reused();
    let adapter_name = session.adapter().name().to_owned();
    let adapter_type = session.adapter().device_type().as_str();
    let adapter_backend = session.adapter().backend().to_owned();
    if warmup_requested {
        session
            .prepare_family(&batches)
            .map_err(|error| GpuRunFailure::Execution(input_failure(error)))?;
    }
    let mut graphs = Vec::new();
    let mut ranges = vec![(0usize, batches.len())];
    let mut peak_gpu_bytes = 0u64;
    let mut peak_frontier_states = 0usize;
    let mut shader_hash = String::new();
    let mut shader_version = "none";
    let mut expanded_records = 0usize;
    while let Some((begin, end)) = ranges.pop() {
        if control.is_cancelled() {
            return Err(GpuRunFailure::Cancelled);
        }
        let outcome = session
            .run_family_with_host_workers_and_control(&batches[begin..end], 1, &|| {
                control.is_cancelled()
            })
            .await
            .map_err(|error| GpuRunFailure::Execution(input_failure(error)))?;
        match outcome {
            WebGpuGeometryExactCoverOutcome::Connected(result) => {
                if !result.can_claim_exact() {
                    let reason = if result.cpu_confirmed_dispatches() == 0 {
                        "webgpu_trust_mismatch_no_confirmed_dispatch"
                    } else if result.cpu_confirmed_parents() == 0 {
                        "webgpu_trust_mismatch_no_confirmed_parent"
                    } else {
                        "webgpu_trust_mismatch_unconfirmed_result"
                    };
                    return Err(GpuRunFailure::TrustMismatch(reason));
                }
                let timings = result.timings();
                SearchStageSpan::record_elapsed(
                    ExecutorSearchStage::PackingGpuHostPrepareSubmit,
                    Some(std::time::Duration::from_nanos(
                        timings.host_prepare_submit_ns(),
                    )),
                    timings.layer_dispatch_count(),
                );
                SearchStageSpan::record_elapsed(
                    ExecutorSearchStage::PackingGpuDispatchCounterWait,
                    Some(std::time::Duration::from_nanos(
                        timings.dispatch_counter_wait_ns(),
                    )),
                    timings.layer_dispatch_count(),
                );
                SearchStageSpan::record_elapsed(
                    ExecutorSearchStage::PackingGpuPayloadReadback,
                    Some(std::time::Duration::from_nanos(
                        timings.payload_readback_ns(),
                    )),
                    timings.generated_record_count(),
                );
                SearchStageSpan::record_elapsed(
                    ExecutorSearchStage::PackingGpuHostReduce,
                    Some(std::time::Duration::from_nanos(
                        timings.exact_host_reduce_ns(),
                    )),
                    timings.generated_record_count(),
                );
                SearchStageSpan::record_elapsed(
                    ExecutorSearchStage::PackingGpuTraceEnumeration,
                    Some(std::time::Duration::from_nanos(
                        timings.trace_enumeration_ns(),
                    )),
                    timings.generated_record_count(),
                );
                peak_gpu_bytes = peak_gpu_bytes.max(result.peak_gpu_bytes());
                peak_frontier_states =
                    peak_frontier_states.max(result.solution_graph().peak_frontier_state_count());
                shader_hash = result.shader_hash().to_owned();
                shader_version = result.shader_version();
                expanded_records =
                    expanded_records.saturating_add(timings.generated_record_count() as usize);
                graphs.push(GpuGraph {
                    target_begin: begin,
                    graph: Arc::new(result.into_solution_graph()),
                });
            }
            WebGpuGeometryExactCoverOutcome::ResourceIncomplete(_) if end - begin > 1 => {
                let middle = begin + (end - begin) / 2;
                ranges.push((middle, end));
                ranges.push((begin, middle));
            }
            WebGpuGeometryExactCoverOutcome::ResourceIncomplete(_) => {
                return Err(GpuRunFailure::Execution(
                    GpuExecutionFailure::resource_incomplete(
                        GpuExecutionFailureStage::HostReduction,
                        GpuPartialResultDisposition::Discarded,
                    )
                    .unwrap_or_else(|_| {
                        GpuExecutionFailure::fatal_internal(GpuExecutionFailureStage::HostReduction)
                    }),
                ))
            }
            WebGpuGeometryExactCoverOutcome::Unavailable(unavailable) => {
                return Err(GpuRunFailure::Execution(GpuExecutionFailure::unavailable(
                    GpuExecutionFailureStage::MemoryReservation,
                    unavailable_reason(unavailable.reason()),
                )))
            }
            WebGpuGeometryExactCoverOutcome::Cancelled => return Err(GpuRunFailure::Cancelled),
            WebGpuGeometryExactCoverOutcome::RejectedInvalidResult { .. } => {
                return Err(GpuRunFailure::TrustMismatch(
                    "webgpu_trust_mismatch_invalid_result",
                ))
            }
            WebGpuGeometryExactCoverOutcome::RejectedTrustMismatch { mismatch_kind, .. } => {
                return Err(GpuRunFailure::TrustMismatch(
                    mismatch_kind.diagnostic_reason(),
                ))
            }
        }
    }
    graphs.sort_unstable_by_key(|graph| graph.target_begin);
    session.recycle();
    Ok(GpuRunResult {
        reduction: GpuReduction {
            graphs,
            graph_cursor: 0,
            path_cursor: None,
            adapter_index,
            adapter_name,
            adapter_type,
            adapter_backend,
            peak_gpu_bytes,
            peak_frontier_states,
            shader_hash,
            shader_version,
            warmup_performed: warmup_requested,
            session_reused,
            expanded_records,
        },
    })
}

fn input_failure(error: WebGpuGeometryExactCoverInputError) -> GpuExecutionFailure {
    match error {
        WebGpuGeometryExactCoverInputError::DevicePoll
        | WebGpuGeometryExactCoverInputError::ReadbackFailed => {
            GpuExecutionFailure::transient_before_commit(
                GpuExecutionFailureStage::Readback,
                GpuPartialResultDisposition::Discarded,
            )
            .unwrap_or_else(|_| {
                GpuExecutionFailure::fatal_internal(GpuExecutionFailureStage::Readback)
            })
        }
        WebGpuGeometryExactCoverInputError::LayerScratch
        | WebGpuGeometryExactCoverInputError::CatalogValidationAllocation => {
            GpuExecutionFailure::resource_incomplete(
                GpuExecutionFailureStage::MemoryReservation,
                GpuPartialResultDisposition::Discarded,
            )
            .unwrap_or_else(|_| {
                GpuExecutionFailure::fatal_internal(GpuExecutionFailureStage::MemoryReservation)
            })
        }
        WebGpuGeometryExactCoverInputError::StaticBufferCache
        | WebGpuGeometryExactCoverInputError::ReadbackAlignment => {
            GpuExecutionFailure::fatal_internal(GpuExecutionFailureStage::Readback)
        }
        WebGpuGeometryExactCoverInputError::InvalidBoard
        | WebGpuGeometryExactCoverInputError::InvalidBoardMask
        | WebGpuGeometryExactCoverInputError::InvalidPieceCounts
        | WebGpuGeometryExactCoverInputError::EmptyOperationTable
        | WebGpuGeometryExactCoverInputError::InvalidOperation
        | WebGpuGeometryExactCoverInputError::IncompatibleBatchFamily
        | WebGpuGeometryExactCoverInputError::CapacityOverflow
        | WebGpuGeometryExactCoverInputError::DimensionOverflow => {
            GpuExecutionFailure::invalid_request(GpuExecutionFailureStage::BatchPlanning)
        }
    }
}

fn unavailable_reason(reason: &str) -> SearchBackendFallbackReason {
    if reason.contains("IndexNotFound") || reason.contains("adapter_unavailable") {
        SearchBackendFallbackReason::GpuDeviceNotFound
    } else if reason.contains("shader") || reason.contains("pipeline") {
        SearchBackendFallbackReason::GpuKernelUnavailable
    } else if reason.contains("storage_buffer") || reason.contains("binding") {
        SearchBackendFallbackReason::GpuBindingUnavailable
    } else {
        SearchBackendFallbackReason::GpuBackendNotConnected
    }
}

pub(super) fn poll_once(future: &mut GpuRunFuture) -> Poll<Result<GpuRunResult, GpuRunFailure>> {
    let mut context = Context::from_waker(Waker::noop());
    future.as_mut().poll(&mut context)
}

pub(super) fn adapter_selection(selection: &GpuDeviceSelection) -> WebGpuAdapterSelection {
    match selection {
        GpuDeviceSelection::Auto => WebGpuAdapterSelection::Auto,
        GpuDeviceSelection::Index(index) => WebGpuAdapterSelection::Index(*index),
    }
}

const fn piece_code(piece: PieceKind) -> u32 {
    match piece {
        PieceKind::I => 1,
        PieceKind::O => 2,
        PieceKind::T => 3,
        PieceKind::S => 4,
        PieceKind::Z => 5,
        PieceKind::J => 6,
        PieceKind::L => 7,
    }
}

#[cfg(test)]
mod tests {
    use clearra_pc_graph::request::BackendFallbackPolicy;
    use clearra_webgpu::WebGpuGeometryExactCoverInputError;

    use crate::backend::{GpuExecutionFailureClass, GpuFailureDisposition};

    use super::input_failure;

    #[test]
    fn readback_failure_can_fallback_only_after_gpu_data_is_discarded() {
        let resolution = input_failure(WebGpuGeometryExactCoverInputError::ReadbackFailed)
            .resolve(BackendFallbackPolicy::Allow);

        assert_eq!(
            resolution.class(),
            GpuExecutionFailureClass::TransientBeforeCommit
        );
        assert_eq!(resolution.disposition(), GpuFailureDisposition::CpuFallback);
        assert!(resolution.discarded_partial_gpu_result());
    }

    #[test]
    fn malformed_gpu_result_is_never_reclassified_as_fallback() {
        let resolution = crate::backend::GpuExecutionFailure::trust_mismatch(
            crate::backend::GpuExecutionFailureStage::ExactConfirm,
        )
        .resolve(BackendFallbackPolicy::Allow);

        assert_eq!(
            resolution.disposition(),
            GpuFailureDisposition::RejectedMismatch
        );
        assert!(!resolution.fallback_used());
    }
}
