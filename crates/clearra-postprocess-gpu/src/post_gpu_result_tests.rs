use crate::{
    BackendFallbackPolicy, BackendPolicy, GpuDeviceSelection, PostGpuCapabilityState,
    PostProcessGpuBackend,
};

use super::*;

fn connected_result() -> PostGpuResult {
    pollster::block_on(PostProcessGpuBackend::union_pattern_words(
        SearchBackendRequest::Cpu,
        &[vec![0b0011, 0b1000], vec![0b1100, 0b0100]],
    ))
    .expect("valid postprocess batch")
}

#[test]
fn post_gpu_result_has_trust_state() {
    let result = connected_result();

    assert_eq!(
        result.trust_state(),
        PostGpuTrustState::TrustedDeterministic
    );
    assert_eq!(result.union_words(), Some(&[0b1111, 0b1100][..]));
    assert!(result.can_claim_exact());
}

#[test]
fn postprocess_gpu_has_trust_state() {
    post_gpu_result_has_trust_state();
}

#[test]
fn post_gpu_fallback_reason_visible() {
    let result = PostGpuResult::unavailable(
        SearchBackendRequest::Gpu,
        "postprocess_gpu_adapter_unavailable",
    )
    .with_cpu_fallback();

    assert_eq!(result.search_backend_selected(), SearchBackendRequest::Gpu);
    assert_eq!(
        result.post_backend_selected(),
        Some(PostBackendRequest::Cpu)
    );
    assert_eq!(
        result.fallback_reason(),
        Some("postprocess_gpu_adapter_unavailable")
    );
    assert!(result.fallback_used());
}

#[test]
fn postprocess_gpu_fallback_reason_visible() {
    post_gpu_fallback_reason_visible();
}

#[test]
fn post_gpu_unconfirmed_not_exact() {
    let result = PostGpuResult::unavailable(
        SearchBackendRequest::Gpu,
        "postprocess_gpu_cpu_confirmation_missing",
    );

    assert!(result.cpu_confirm_required());
    assert!(!result.can_claim_exact());
}

#[test]
fn gpu_score_matrix_cpu_sample_confirm() {
    assert!(PostGpuTrustState::TrustedCpuSampleConfirmed.can_claim_exact());
    assert!(!PostGpuTrustState::Unavailable.can_claim_exact());
    assert!(!PostGpuTrustState::RejectedMismatch.can_claim_exact());
}

#[test]
fn search_backend_and_post_backend_are_separate() {
    let policy = BackendPolicy::new(
        SearchBackendRequest::Cpu,
        PostBackendRequest::Gpu,
        BackendFallbackPolicy::AllowWithDiagnostic,
        GpuDeviceSelection::Auto,
        true,
    );

    assert_eq!(policy.search_backend(), SearchBackendRequest::Cpu);
    assert_eq!(policy.post_backend(), PostBackendRequest::Gpu);
    assert!(policy.search_backend_and_post_backend_are_separate());
}

#[test]
fn post_backend_separate_from_search_backend() {
    search_backend_and_post_backend_are_separate();
}

#[test]
fn postprocess_gpu_failure_does_not_rewrite_search_backend() {
    let result =
        PostGpuResult::unavailable(SearchBackendRequest::Hybrid, "postprocess_gpu_device_lost")
            .with_cpu_fallback();

    assert_eq!(
        result.search_backend_selected(),
        SearchBackendRequest::Hybrid
    );
    assert_eq!(
        result.post_backend_selected(),
        Some(PostBackendRequest::Cpu)
    );
    assert_eq!(result.trust_state(), PostGpuTrustState::Unavailable);
}

#[test]
fn postprocess_gpu_capability_uses_stable_outcomes() {
    let result = connected_result();
    let capability = result.capability();

    assert_eq!(capability.state(), PostGpuCapabilityState::Connected);
    assert!(capability.runtime_connected());
    assert!(capability.exact_supported());
    assert_eq!(capability.unavailable_reason(), None);
}
