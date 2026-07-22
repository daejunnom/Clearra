use clearra_app::AppResponse;

use super::super::{field_value, first_field};
use super::u32_value;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuiGpuBackpressureView {
    gpu_queue_depth: u32,
    readback_pending_batches: u32,
    cpu_confirm_queue_depth: u32,
    build_variant_buffer_pressure: u32,
    coverage_row_buffer_pressure: u32,
    throttle_reason: String,
}

impl GuiGpuBackpressureView {
    pub fn from_response(response: &AppResponse) -> Self {
        Self {
            gpu_queue_depth: u32_value(field_value(response, "gpu_backpressure_gpu_queue_depth")),
            readback_pending_batches: u32_value(first_field(
                response,
                &[
                    "gpu_backpressure_readback_pending_batches",
                    "readback_pending_batches",
                ],
            )),
            cpu_confirm_queue_depth: u32_value(first_field(
                response,
                &[
                    "gpu_backpressure_cpu_confirm_queue_depth",
                    "gpu_backpressure_cpu_worker_queue_depth",
                    "cpu_confirm_queue_depth",
                ],
            )),
            build_variant_buffer_pressure: u32_value(field_value(
                response,
                "gpu_backpressure_build_variant_buffer_pressure",
            )),
            coverage_row_buffer_pressure: u32_value(field_value(
                response,
                "gpu_backpressure_coverage_row_buffer_pressure",
            )),
            throttle_reason: field_value(response, "gpu_backpressure_throttle_reason")
                .unwrap_or_else(|| "none".to_owned()),
        }
    }
}
impl GuiGpuBackpressureView {
    pub const fn gpu_queue_depth(&self) -> u32 {
        self.gpu_queue_depth
    }
}
impl GuiGpuBackpressureView {
    pub const fn readback_pending_batches(&self) -> u32 {
        self.readback_pending_batches
    }
}
impl GuiGpuBackpressureView {
    pub const fn readback_pending(&self) -> bool {
        self.readback_pending_batches > 0
    }
}
impl GuiGpuBackpressureView {
    pub const fn cpu_confirm_queue_depth(&self) -> u32 {
        self.cpu_confirm_queue_depth
    }
}
impl GuiGpuBackpressureView {
    pub const fn build_variant_buffer_pressure(&self) -> u32 {
        self.build_variant_buffer_pressure
    }
}
impl GuiGpuBackpressureView {
    pub const fn coverage_row_buffer_pressure(&self) -> u32 {
        self.coverage_row_buffer_pressure
    }
}
impl GuiGpuBackpressureView {
    pub fn throttle_reason(&self) -> &str {
        &self.throttle_reason
    }
}
