use clearra_pc_graph::request::{
    BackendFallbackPolicy, GpuDeviceSelection, PcExecutionPolicy, RequestedSearchBackend,
};

use super::{
    packing_problem_builder_error::to_u16, CBackendRequest, FfiProblemError, C_BACKEND_AUTO,
    C_BACKEND_CPU, C_BACKEND_FALLBACK_ALLOW, C_BACKEND_FALLBACK_DENY, C_BACKEND_GPU,
    C_BACKEND_HYBRID,
};

pub(crate) fn backend_descriptor(
    policy: &PcExecutionPolicy,
) -> Result<CBackendRequest, FfiProblemError> {
    let (gpu_device_kind, gpu_device_index) = match policy.gpu_device() {
        GpuDeviceSelection::Auto => (0, 0),
        GpuDeviceSelection::Index(index) => (1, *index),
    };

    Ok(CBackendRequest {
        requested_backend: backend_code(policy.requested_backend()),
        workers: to_u16(policy.workers(), |value| FfiProblemError::BudgetTooLarge {
            field: "workers",
            value,
        })?,
        deterministic: policy.deterministic() as u8,
        reserved_flags: 0,
        fallback_policy: match policy.backend_fallback() {
            BackendFallbackPolicy::Allow => C_BACKEND_FALLBACK_ALLOW,
            BackendFallbackPolicy::Deny => C_BACKEND_FALLBACK_DENY,
        },
        gpu_device_kind,
        gpu_device_index,
        reserved: 0,
    })
}

fn backend_code(backend: RequestedSearchBackend) -> u32 {
    match backend {
        RequestedSearchBackend::Auto => C_BACKEND_AUTO,
        RequestedSearchBackend::Cpu => C_BACKEND_CPU,
        RequestedSearchBackend::Gpu => C_BACKEND_GPU,
        RequestedSearchBackend::Hybrid => C_BACKEND_HYBRID,
    }
}
