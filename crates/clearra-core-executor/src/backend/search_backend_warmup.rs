use clearra_pc_graph::request::GpuDeviceSelection;

/// Compatibility report for callers that previously prewarmed WebGPU.
/// The retired backend never opens an adapter, creates a device or runs CPU work.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GpuSearchWarmupReport;

impl GpuSearchWarmupReport {
    pub const fn connected(&self) -> bool {
        false
    }
    pub const fn device_index(&self) -> Option<u8> {
        None
    }
    pub fn device_name(&self) -> Option<&str> {
        None
    }
    pub fn unavailable_reason(&self) -> Option<&str> {
        Some("webgpu_backend_retired")
    }
    pub const fn session_reused(&self) -> bool {
        false
    }
    pub const fn initialization_elapsed_ns(&self) -> u64 {
        0
    }
}

/// Reports retirement without allocating backend resources.
pub fn prewarm_gpu_search(_device: GpuDeviceSelection) -> GpuSearchWarmupReport {
    GpuSearchWarmupReport
}

pub async fn prewarm_gpu_search_async(device: GpuDeviceSelection) -> GpuSearchWarmupReport {
    prewarm_gpu_search(device)
}
