use clearra_core_domain::execution_cancellation::ExecutionCancellationToken;
use clearra_core_ffi::CPackingProblem;
use clearra_pc_graph::request::PcExecutionPolicy;

use crate::packing::PackingRunnerError;

use super::{
    BackendFallback, GpuFailureDisposition, PackingBackendOutcome, PcBackendSelection,
    SearchBackendExecutorResolver, SelectedSearchBackend,
};

pub(crate) fn execute_selected_packing(
    selection: &PcBackendSelection,
    problem: &CPackingProblem,
    policy: &PcExecutionPolicy,
    cancellation: &ExecutionCancellationToken,
    executors: &impl SearchBackendExecutorResolver,
) -> Result<PackingBackendOutcome, PackingRunnerError> {
    let selected_backend = selection.selected_backend();
    let executor = executors.executor_for(selected_backend).ok_or(
        PackingRunnerError::BackendExecutorUnavailable {
            backend: selected_backend,
            reason: "selected_backend_executor_unavailable",
        },
    )?;

    match executor.execute_packing(problem, policy, cancellation) {
        Ok(mut outcome) => {
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
            if selection.backend_fallback_used() && outcome.fallback.is_none() {
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
                    let mut cpu_outcome = executors.cpu_fallback_executor().execute_packing(
                        problem,
                        policy,
                        cancellation,
                    )?;
                    if !matches!(
                        cpu_outcome.actual_backend,
                        SelectedSearchBackend::CpuGeometryExactCover
                            | SelectedSearchBackend::CpuParallelGeometryExactCover
                    ) {
                        return Err(PackingRunnerError::BackendExecutionMismatch {
                            selected: SelectedSearchBackend::CpuGeometryExactCover,
                            actual: cpu_outcome.actual_backend,
                        });
                    }
                    if !cpu_outcome
                        .trust_report
                        .is_valid_for(cpu_outcome.actual_backend)
                    {
                        return Err(PackingRunnerError::BackendTrustMismatch {
                            backend: cpu_outcome.actual_backend,
                            trust_state: cpu_outcome.trust_report.state(),
                        });
                    }
                    cpu_outcome.attach_fallback(
                        BackendFallback::new(true, resolution.backend_fallback_reason()),
                        Some(resolution),
                    );
                    Ok(cpu_outcome)
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
