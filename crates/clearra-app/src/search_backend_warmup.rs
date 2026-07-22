use clearra_pc_graph::request::GpuDeviceSelection;

pub use clearra_core_executor::backend::GpuSearchWarmupReport;

pub fn prewarm_search_backend(device: GpuDeviceSelection) -> GpuSearchWarmupReport {
    clearra_core_executor::backend::prewarm_gpu_search(device)
}
