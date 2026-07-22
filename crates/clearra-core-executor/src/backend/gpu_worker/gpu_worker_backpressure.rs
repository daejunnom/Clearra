use crate::backend::{HybridBackpressureReport, HybridThrottleReason};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GpuWorkerBackpressure {
    report: HybridBackpressureReport,
}

impl GpuWorkerBackpressure {
    pub const fn new(
        gpu_queue_depth: u16,
        cpu_worker_queue_depth: u16,
        readback_pending_batches: u16,
        build_variant_buffer_pressure: u16,
        coverage_row_buffer_pressure: u16,
        throttled_backend: &'static str,
        throttle_reason: HybridThrottleReason,
    ) -> Self {
        Self {
            report: HybridBackpressureReport::new(
                gpu_queue_depth,
                cpu_worker_queue_depth,
                readback_pending_batches,
                build_variant_buffer_pressure,
                coverage_row_buffer_pressure,
                throttled_backend,
                throttle_reason,
            ),
        }
    }
}
impl GpuWorkerBackpressure {
    pub const fn idle(throttled_backend: &'static str) -> Self {
        Self::new(0, 0, 0, 0, 0, throttled_backend, HybridThrottleReason::None)
    }
}
impl GpuWorkerBackpressure {
    pub const fn report(self) -> HybridBackpressureReport {
        self.report
    }
}
impl GpuWorkerBackpressure {
    pub const fn with_u2_contract(
        self,
        candidate_queue_len: u16,
        candidate_queue_capacity: u16,
        cpu_worker_backlog: u16,
        gpu_readback_backlog: u16,
        gpu_batch_in_flight: u16,
        backpressure_active: bool,
        deferred_batch_count: u16,
        truncated_batch_count: u16,
        memory_pressure_level: &'static str,
    ) -> Self {
        Self {
            report: self.report.with_u2_contract(
                candidate_queue_len,
                candidate_queue_capacity,
                cpu_worker_backlog,
                gpu_readback_backlog,
                gpu_batch_in_flight,
                backpressure_active,
                deferred_batch_count,
                truncated_batch_count,
                memory_pressure_level,
            ),
        }
    }
}
