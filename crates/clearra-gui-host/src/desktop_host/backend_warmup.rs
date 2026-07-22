use clearra_pc_graph::request::GpuDeviceSelection;
use serde_json::json;

pub fn prewarm_search_backend(gpu_device: Option<u8>) -> String {
    let device = gpu_device
        .map(GpuDeviceSelection::Index)
        .unwrap_or(GpuDeviceSelection::Auto);
    let report = clearra_app::prewarm_search_backend(device);
    json!({
        "state": if report.connected() { "connected" } else { "unavailable" },
        "device_index": report.device_index(),
        "device_name": report.device_name(),
        "unavailable_reason": report.unavailable_reason(),
        "session_cached": report.connected(),
        "session_reused": report.session_reused(),
        "initialization_elapsed_ns": report.initialization_elapsed_ns()
    })
    .to_string()
}
