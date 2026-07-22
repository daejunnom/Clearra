use clearra_core_domain::execution_cancellation::ExecutionCancellationToken;
use clearra_core_ffi::CPackingProblem;
use clearra_coverage::pattern::pattern_bitset::PatternBitSet;
use clearra_pc_graph::request::PcExecutionPolicy;
use clearra_problem::SearchProblem;

use crate::packing::PackingRunnerError;

#[cfg(feature = "native-c-core")]
use super::buildable_geometry_graph_executor::execute_cpu_buildable_geometry_graph;

use super::{
    BackendFallback, GpuFailureDisposition, PackingBackendOutcome, PcBackendSelection,
    SelectedSearchBackend,
};
#[cfg(not(all(feature = "webgpu-search", feature = "native-c-core")))]
use super::{GpuExecutionFailure, GpuExecutionFailureStage, SearchBackendFallbackReason};

pub(crate) fn execute_selected_buildable_packing(
    search_problem: &SearchProblem,
    source_pattern_bits: Option<&PatternBitSet>,
    selection: &PcBackendSelection,
    problem: &CPackingProblem,
    policy: &PcExecutionPolicy,
    cancellation: &ExecutionCancellationToken,
) -> Result<PackingBackendOutcome, PackingRunnerError> {
    let selected = selection.selected_backend();
    let attempt = execute_backend(
        search_problem,
        source_pattern_bits,
        problem,
        policy,
        cancellation,
        selected,
    );
    match attempt {
        Ok(mut outcome) => {
            validate_backend_outcome(&outcome, selected)?;
            if selection.backend_fallback_used() {
                outcome.attach_fallback(
                    BackendFallback::from_selection(selection),
                    selection.gpu_failure(),
                );
            }
            Ok(outcome)
        }
        Err(PackingRunnerError::GpuExecution(failure)) => {
            let resolution = failure.resolve(policy.backend_fallback());
            match resolution.disposition() {
                GpuFailureDisposition::CpuFallback
                | GpuFailureDisposition::CpuRerunAfterIncomplete => {
                    let cpu_backend = if effective_worker_count(policy) > 1 {
                        SelectedSearchBackend::CpuParallelGeometryExactCover
                    } else {
                        SelectedSearchBackend::CpuGeometryExactCover
                    };
                    let mut outcome = execute_backend(
                        search_problem,
                        source_pattern_bits,
                        problem,
                        policy,
                        cancellation,
                        cpu_backend,
                    )?;
                    validate_backend_outcome(&outcome, cpu_backend)?;
                    outcome.attach_fallback(
                        BackendFallback::new(true, resolution.backend_fallback_reason()),
                        Some(resolution),
                    );
                    Ok(outcome)
                }
                GpuFailureDisposition::Unavailable
                | GpuFailureDisposition::TransientFailure
                | GpuFailureDisposition::Incomplete
                | GpuFailureDisposition::InvalidRequest
                | GpuFailureDisposition::RejectedMismatch
                | GpuFailureDisposition::FatalInternal => {
                    Err(PackingRunnerError::GpuExecutionRejected(resolution))
                }
            }
        }
        Err(error) => Err(error),
    }
}

fn validate_backend_outcome(
    outcome: &PackingBackendOutcome,
    selected_backend: SelectedSearchBackend,
) -> Result<(), PackingRunnerError> {
    if outcome.actual_backend != selected_backend {
        return Err(PackingRunnerError::BackendExecutionMismatch {
            selected: selected_backend,
            actual: outcome.actual_backend,
        });
    }
    if !outcome.trust_report.is_valid_for(outcome.actual_backend) {
        return Err(PackingRunnerError::BackendTrustMismatch {
            backend: outcome.actual_backend,
            trust_state: outcome.trust_report.state(),
        });
    }
    Ok(())
}

fn execute_backend(
    search_problem: &SearchProblem,
    source_pattern_bits: Option<&PatternBitSet>,
    problem: &CPackingProblem,
    policy: &PcExecutionPolicy,
    cancellation: &ExecutionCancellationToken,
    backend: SelectedSearchBackend,
) -> Result<PackingBackendOutcome, PackingRunnerError> {
    match backend {
        SelectedSearchBackend::CpuGeometryExactCover => execute_cpu(
            search_problem,
            source_pattern_bits,
            problem,
            cancellation,
            backend,
            1,
        ),
        SelectedSearchBackend::CpuParallelGeometryExactCover => execute_cpu(
            search_problem,
            source_pattern_bits,
            problem,
            cancellation,
            backend,
            effective_worker_count(policy),
        ),
        SelectedSearchBackend::Gpu | SelectedSearchBackend::Hybrid => execute_gpu(
            search_problem,
            source_pattern_bits,
            problem,
            policy,
            cancellation,
            backend,
            effective_worker_count(policy),
        ),
        SelectedSearchBackend::None => Err(PackingRunnerError::BackendExecutorUnavailable {
            backend,
            reason: "selected_backend_executor_unavailable",
        }),
    }
}

#[cfg(feature = "native-c-core")]
fn execute_cpu(
    search_problem: &SearchProblem,
    source_pattern_bits: Option<&PatternBitSet>,
    problem: &CPackingProblem,
    cancellation: &ExecutionCancellationToken,
    backend: SelectedSearchBackend,
    worker_count: usize,
) -> Result<PackingBackendOutcome, PackingRunnerError> {
    execute_cpu_buildable_geometry_graph(
        search_problem,
        source_pattern_bits,
        problem,
        cancellation,
        backend,
        worker_count,
    )
}

#[cfg(not(feature = "native-c-core"))]
fn execute_cpu(
    _search_problem: &SearchProblem,
    _source_pattern_bits: Option<&PatternBitSet>,
    _problem: &CPackingProblem,
    _cancellation: &ExecutionCancellationToken,
    backend: SelectedSearchBackend,
    _worker_count: usize,
) -> Result<PackingBackendOutcome, PackingRunnerError> {
    Err(PackingRunnerError::BackendExecutorUnavailable {
        backend,
        reason: "native_geometry_exact_cover_not_connected",
    })
}

#[cfg(all(feature = "webgpu-search", feature = "native-c-core"))]
fn execute_gpu(
    search_problem: &SearchProblem,
    source_pattern_bits: Option<&PatternBitSet>,
    problem: &CPackingProblem,
    policy: &PcExecutionPolicy,
    cancellation: &ExecutionCancellationToken,
    backend: SelectedSearchBackend,
    worker_count: usize,
) -> Result<PackingBackendOutcome, PackingRunnerError> {
    super::native_webgpu_packing_executor::execute_webgpu_buildable_unique(
        search_problem,
        source_pattern_bits,
        problem,
        policy,
        cancellation,
        backend,
        worker_count,
    )
}

#[cfg(not(all(feature = "webgpu-search", feature = "native-c-core")))]
fn execute_gpu(
    _search_problem: &SearchProblem,
    _source_pattern_bits: Option<&PatternBitSet>,
    _problem: &CPackingProblem,
    _policy: &PcExecutionPolicy,
    _cancellation: &ExecutionCancellationToken,
    _backend: SelectedSearchBackend,
    _worker_count: usize,
) -> Result<PackingBackendOutcome, PackingRunnerError> {
    Err(PackingRunnerError::GpuExecution(
        GpuExecutionFailure::unavailable(
            GpuExecutionFailureStage::KernelExecution,
            SearchBackendFallbackReason::GpuKernelUnavailable,
        ),
    ))
}

fn effective_worker_count(policy: &PcExecutionPolicy) -> usize {
    policy.workers()
}
