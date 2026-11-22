//! Legacy diagnostic options fail explicitly after the WebGPU backend retirement.

pub fn print_gpu_inventory() -> Result<(), String> {
    Err("webgpu_backend_retired".to_owned())
}

pub fn print_gpu_warmup_probe(
    _device: clearra_pc_graph::request::GpuDeviceSelection,
) -> Result<(), String> {
    Err("webgpu_backend_retired".to_owned())
}

pub fn prewarm_gpu(_device: clearra_pc_graph::request::GpuDeviceSelection) -> Result<(), String> {
    Err("webgpu_backend_retired".to_owned())
}
