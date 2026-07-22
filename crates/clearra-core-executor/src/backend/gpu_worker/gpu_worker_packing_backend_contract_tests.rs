use crate::backend::{
    gpu_worker::{
        GpuBackendCapability, GpuBackendKind, GpuFenceEpoch, GpuMemoryTicket,
        GpuWorkerBackpressure, GpuWorkerExactnessGate, GpuWorkerReduction, GpuWorkerResult,
        GpuWorkerResultReducer,
    },
    GpuTrustState, SearchBackendFallbackReason,
};

fn ticket() -> GpuMemoryTicket {
    GpuMemoryTicket::new(42, GpuFenceEpoch::new(3), 4096)
}

#[test]
fn gpu_backend_kind_split_reports_unavailable_backend_labels() {
    assert_eq!(
        GpuBackendCapability::for_kind(GpuBackendKind::NativeCompute).contract_label(),
        "native-gpu-unavailable"
    );
    assert_eq!(
        GpuBackendCapability::for_kind(GpuBackendKind::Disabled).contract_label(),
        "disabled"
    );
}

#[test]
fn unconfirmed_gpu_trust_state_cannot_source_exact_probability() {
    assert!(!GpuTrustState::GpuComputedUnconfirmed.can_source_exact_probability());
    assert!(!GpuTrustState::Unavailable.can_source_exact_probability());
    assert!(!GpuTrustState::FallbackUsed {
        reason: SearchBackendFallbackReason::GpuFeatureDisabled,
    }
    .can_source_exact_probability());
    assert!(!GpuTrustState::GpuComputedMismatch.can_source_exact_probability());
}

#[test]
fn cpu_confirmed_gpu_result_reduces_to_exact_candidate_source() {
    let result = GpuWorkerResult::new(
        7,
        3,
        GpuTrustState::GpuComputedCpuConfirmed,
        false,
        None,
        ticket(),
        GpuWorkerBackpressure::idle("native-gpu-cpu-confirmed"),
    );

    let reduction = GpuWorkerResultReducer::reduce(result);

    match reduction {
        GpuWorkerReduction::ExactCandidateSource { report, .. } => {
            assert_eq!(
                report.exactness_gate(),
                GpuWorkerExactnessGate::ExactCandidateSource
            );
            assert!(report.can_source_exact_probability());
            assert!(report.can_accept_build_variant());
        }
        other => panic!("expected CPU-confirmed exact source, got {other:?}"),
    }
}
