use clearra_core_domain::execution_cancellation::ExecutionCancellationToken;
use clearra_core_ffi::CPackingProblem;
use clearra_pc_graph::request::PcExecutionPolicy;

use crate::packing::PackingRunnerError;

use super::{
    BackendFallback, GpuExecutionFailure, GpuExecutionFailureStage, GpuFailureDisposition,
    PackingBackendOutcome, PcBackendSelection, SearchBackendFallbackReason, SelectedSearchBackend,
};

pub(crate) fn execute_selected_raw_geometry_packing(
    selection: &PcBackendSelection,
    problem: &CPackingProblem,
    policy: &PcExecutionPolicy,
    cancellation: &ExecutionCancellationToken,
) -> Result<PackingBackendOutcome, PackingRunnerError> {
    let selected = selection.selected_backend();
    let attempt = execute_backend(problem, policy, cancellation, selected);
    match attempt {
        Ok(mut outcome) => {
            validate_backend_outcome(&outcome, raw_actual_backend(selected))?;
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
                    let cpu_backend = SelectedSearchBackend::CpuGeometryExactCover;
                    let mut outcome = execute_backend(problem, policy, cancellation, cpu_backend)?;
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
    problem: &CPackingProblem,
    policy: &PcExecutionPolicy,
    cancellation: &ExecutionCancellationToken,
    backend: SelectedSearchBackend,
) -> Result<PackingBackendOutcome, PackingRunnerError> {
    match backend {
        SelectedSearchBackend::CpuGeometryExactCover
        | SelectedSearchBackend::CpuParallelGeometryExactCover => execute_cpu(
            problem,
            cancellation,
            raw_actual_backend(backend),
            raw_worker_count(problem, policy),
        ),
        SelectedSearchBackend::Gpu | SelectedSearchBackend::Hybrid => Err(
            PackingRunnerError::GpuExecution(GpuExecutionFailure::unavailable(
                GpuExecutionFailureStage::KernelExecution,
                SearchBackendFallbackReason::GpuBackendNotConnected,
            )),
        ),
        SelectedSearchBackend::None => Err(PackingRunnerError::BackendExecutorUnavailable {
            backend,
            reason: "selected_raw_geometry_backend_executor_unavailable",
        }),
    }
}

fn raw_actual_backend(selected_backend: SelectedSearchBackend) -> SelectedSearchBackend {
    match selected_backend {
        SelectedSearchBackend::CpuParallelGeometryExactCover => {
            SelectedSearchBackend::CpuGeometryExactCover
        }
        backend => backend,
    }
}

fn raw_worker_count(_problem: &CPackingProblem, _policy: &PcExecutionPolicy) -> usize {
    // The C sink applies a global candidate cap while candidates arrive. With
    // concurrent shards, scheduling would choose a non-deterministic retained
    // subset once that cap is reached. Raw production therefore stays serial
    // until shard-local results have a leased deterministic merge/cap owner.
    // The request and workers_requested remain intact. The execution outcome
    // reports the serial CPU exact-cover backend and this actual worker count.
    1
}

#[cfg(feature = "native-c-core")]
fn execute_cpu(
    problem: &CPackingProblem,
    cancellation: &ExecutionCancellationToken,
    backend: SelectedSearchBackend,
    workers_used: usize,
) -> Result<PackingBackendOutcome, PackingRunnerError> {
    use clearra_core_ffi::{CoreCNative, NativeCandidateReducer};

    use super::BackendTrustReport;

    if cancellation.is_cancelled() {
        return Err(PackingRunnerError::ExecutionCancelled);
    }
    let catalog = CoreCNative::compile_geometry_catalog_with_cancellation(problem, cancellation)
        .map_err(PackingRunnerError::Native)?;
    let mut reducer =
        NativeCandidateReducer::new(problem).map_err(PackingRunnerError::CandidateBatch)?;
    let streamed = catalog
        .stream_partition(
            problem,
            0,
            problem.piece_multiset_family.count,
            0,
            1,
            1,
            cancellation,
            &mut reducer,
        )
        .map_err(PackingRunnerError::Native)?;
    if cancellation.is_cancelled() {
        return Err(PackingRunnerError::ExecutionCancelled);
    }
    let candidates = reducer.into_candidates();
    let mut resource_report = streamed.resource_report;
    resource_report.observe_cpu_bytes(catalog.compile_resource_report().peak_cpu_bytes);

    Ok(PackingBackendOutcome::raw_geometry_exact(
        backend,
        candidates,
        resource_report,
        BackendTrustReport::cpu_exact(),
    )
    .with_workers_used(workers_used)
    .with_geometry_catalog(catalog)
    .with_pruning_ledger(streamed.pruning_ledger))
}

#[cfg(not(feature = "native-c-core"))]
fn execute_cpu(
    _problem: &CPackingProblem,
    _cancellation: &ExecutionCancellationToken,
    backend: SelectedSearchBackend,
    _workers_used: usize,
) -> Result<PackingBackendOutcome, PackingRunnerError> {
    Err(PackingRunnerError::BackendExecutorUnavailable {
        backend,
        reason: "native_raw_geometry_exact_cover_not_connected",
    })
}

#[cfg(test)]
mod tests {
    use clearra_core_ffi::CPackingProblem;
    use clearra_pc_graph::request::PcExecutionPolicy;

    use crate::backend::SelectedSearchBackend;

    use super::{raw_actual_backend, raw_worker_count};

    #[test]
    fn raw_execution_is_serial_until_deterministic_global_cap_ownership_exists() {
        let policy = PcExecutionPolicy::mvp_default().with_workers(4);
        let unbounded = CPackingProblem::default();
        let mut explicitly_bounded = CPackingProblem::default();
        explicitly_bounded.budget.has_max_memory_mib = 1;
        explicitly_bounded.budget.max_memory_mib = 64;

        assert_eq!(policy.workers_requested(), Some(4));
        assert!((1..=4).contains(&policy.workers()));
        assert_eq!(raw_worker_count(&unbounded, &policy), 1);
        assert_eq!(raw_worker_count(&explicitly_bounded, &policy), 1);
        assert_eq!(policy.workers_requested(), Some(4));
        assert_eq!(
            raw_actual_backend(SelectedSearchBackend::CpuParallelGeometryExactCover),
            SelectedSearchBackend::CpuGeometryExactCover
        );
    }
}
