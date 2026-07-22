#[cfg(feature = "gpu-backend")]
use clearra_webgpu::{
    enumerate_adapter_summaries, select_adapter_summary, WebGpuAdapterDeviceType,
    WebGpuAdapterSelection,
};
#[cfg(feature = "gpu-backend")]
use serde::Serialize;

#[cfg(feature = "gpu-backend")]
use clearra_pc_graph::request::GpuDeviceSelection;

#[cfg(feature = "gpu-backend")]
#[derive(Serialize)]
struct GpuInventoryEntry {
    index: u8,
    name: String,
    device_type: String,
    backend: String,
    pci_bus_id: String,
    selectable: bool,
    auto_selected: bool,
}

#[cfg(feature = "gpu-backend")]
#[derive(Serialize)]
struct GpuWarmupProbeEntry {
    phase: &'static str,
    connected: bool,
    device_index: Option<u8>,
    device_name: Option<String>,
    session_or_context_reused: bool,
    initialization_elapsed_ns: u64,
    unavailable_reason: Option<String>,
}

#[cfg(feature = "gpu-backend")]
pub fn print_gpu_inventory() -> Result<(), String> {
    let adapters = pollster::block_on(enumerate_adapter_summaries())
        .map_err(|error| format!("GPU adapter enumeration failed: {error}"))?;
    let auto_selected = pollster::block_on(select_adapter_summary(WebGpuAdapterSelection::Auto))
        .ok()
        .map(|adapter| adapter.index());
    let entries = adapters
        .into_iter()
        .map(|adapter| GpuInventoryEntry {
            index: adapter.index(),
            name: adapter.name().to_owned(),
            device_type: adapter.device_type().as_str().to_owned(),
            backend: adapter.backend().to_owned(),
            pci_bus_id: adapter.pci_bus_id().to_owned(),
            selectable: adapter.device_type() != WebGpuAdapterDeviceType::Cpu,
            auto_selected: auto_selected == Some(adapter.index()),
        })
        .collect::<Vec<_>>();
    println!(
        "{}",
        serde_json::to_string_pretty(&entries)
            .map_err(|error| format!("GPU inventory serialization failed: {error}"))?
    );
    Ok(())
}

#[cfg(feature = "gpu-backend")]
pub fn print_gpu_warmup_probe(device: GpuDeviceSelection) -> Result<(), String> {
    let cold = clearra_core_executor::backend::prewarm_gpu_search(device.clone());
    let warm = clearra_core_executor::backend::prewarm_gpu_search(device);
    let entries = [warmup_entry("cold", &cold), warmup_entry("warm", &warm)];
    println!(
        "{}",
        serde_json::to_string_pretty(&entries)
            .map_err(|error| format!("GPU warmup probe serialization failed: {error}"))?
    );
    Ok(())
}

#[cfg(feature = "gpu-backend")]
pub fn prewarm_gpu(device: GpuDeviceSelection) -> Result<(), String> {
    let report = clearra_core_executor::backend::prewarm_gpu_search(device);
    if !report.connected() {
        eprintln!(
            "GPU prewarm unavailable: {}",
            report.unavailable_reason().unwrap_or("gpu_unavailable")
        );
    }
    Ok(())
}

#[cfg(feature = "gpu-backend")]
fn warmup_entry(
    phase: &'static str,
    report: &clearra_core_executor::backend::GpuSearchWarmupReport,
) -> GpuWarmupProbeEntry {
    GpuWarmupProbeEntry {
        phase,
        connected: report.connected(),
        device_index: report.device_index(),
        device_name: report.device_name().map(str::to_owned),
        session_or_context_reused: report.session_reused(),
        initialization_elapsed_ns: report.initialization_elapsed_ns(),
        unavailable_reason: report.unavailable_reason().map(str::to_owned),
    }
}

#[cfg(not(feature = "gpu-backend"))]
pub fn print_gpu_inventory() -> Result<(), String> {
    Err(
        "gpu_inventory_not_compiled; rebuild clearra-pc-artifact with --features gpu-backend"
            .to_owned(),
    )
}

#[cfg(not(feature = "gpu-backend"))]
pub fn print_gpu_warmup_probe(
    _device: clearra_pc_graph::request::GpuDeviceSelection,
) -> Result<(), String> {
    Err(
        "gpu_warmup_probe_not_compiled; rebuild clearra-pc-artifact with --features gpu-backend"
            .to_owned(),
    )
}

#[cfg(not(feature = "gpu-backend"))]
pub fn prewarm_gpu(_device: clearra_pc_graph::request::GpuDeviceSelection) -> Result<(), String> {
    Err(
        "gpu_prewarm_not_compiled; rebuild clearra-pc-artifact with --features gpu-backend"
            .to_owned(),
    )
}
