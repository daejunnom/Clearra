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

#[cfg(all(test, feature = "native-c-core"))]
std::thread_local! {
    static CPU_CATALOG_DISPATCH_ATTEMPTS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

pub(crate) fn execute_selected_buildable_packing(
    search_problem: &SearchProblem,
    source_pattern_bits: Option<&PatternBitSet>,
    selection: &PcBackendSelection,
    problem: &CPackingProblem,
    policy: &PcExecutionPolicy,
    cancellation: &ExecutionCancellationToken,
) -> Result<PackingBackendOutcome, PackingRunnerError> {
    let selected = selection.selected_backend();
    #[cfg(all(test, feature = "native-c-core"))]
    if matches!(
        selected,
        SelectedSearchBackend::CpuGeometryExactCover
            | SelectedSearchBackend::CpuParallelGeometryExactCover
    ) {
        CPU_CATALOG_DISPATCH_ATTEMPTS.with(|attempts| attempts.set(attempts.get() + 1));
    }
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

#[cfg(all(test, feature = "native-c-core"))]
mod tests {
    use clearra_core_domain::{
        pc::pc_target::PcTarget,
        piece::piece_kind::PieceKind,
        resource::{ExecutionAvailabilityReason, ExecutionAvailabilityState},
    };
    use clearra_pc_graph::request::{
        OpeningPcSearchQuery, PcExecutionPolicy, PcHoldPolicy, PcQueueInput, RequestedSearchBackend,
    };
    use clearra_problem::ProblemCompiler;
    use clearra_supply::queue::fixed_sequence::FixedSequence;

    use crate::packing::{PackingRunner, PackingRunnerError};

    use super::CPU_CATALOG_DISPATCH_ATTEMPTS;

    #[test]
    fn product_runner_fails_admission_before_native_catalog_dispatch() {
        let policy = PcExecutionPolicy::mvp_default()
            .with_requested_backend(RequestedSearchBackend::Cpu)
            .with_workers(4)
            .with_max_memory_mib(Some(0));
        let query = OpeningPcSearchQuery::new(PcTarget::two_lines())
            .with_queue(PcQueueInput::fixed_sequence(FixedSequence::new(vec![
                PieceKind::I,
                PieceKind::I,
                PieceKind::O,
                PieceKind::O,
                PieceKind::O,
            ])))
            .with_hold_policy(PcHoldPolicy::Disabled)
            .with_execution_policy(policy);
        let problem = ProblemCompiler::compile_opening_pc(&query).expect("problem");

        CPU_CATALOG_DISPATCH_ATTEMPTS.with(|attempts| attempts.set(0));
        let error = PackingRunner::run(&problem)
            .expect_err("zero memory budget must fail native product admission");
        let report = match error {
            PackingRunnerError::Native(clearra_core_ffi::NativeCoreError::PackingIncomplete {
                resource_report,
                ..
            }) => resource_report,
            other => panic!("unexpected admission failure: {other:?}"),
        };

        assert_eq!(CPU_CATALOG_DISPATCH_ATTEMPTS.with(std::cell::Cell::get), 0);
        assert!(!report.execution_started());
        assert!(!report.result_complete());
        assert_eq!(
            report.execution_availability().state(),
            ExecutionAvailabilityState::Exhausted
        );
        assert_eq!(
            report.execution_availability().reason(),
            Some(ExecutionAvailabilityReason::MemoryBudgetExceeded)
        );
    }
}
