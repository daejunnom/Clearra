#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum HybridThrottleReason {
    #[default]
    None,
    GpuQueueDepth,
    CpuWorkerQueueDepth,
    ReadbackPending,
    BuildVariantBufferPressure,
    CoverageRowBufferPressure,
}

impl HybridThrottleReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::GpuQueueDepth => "gpu_queue_depth",
            Self::CpuWorkerQueueDepth => "cpu_worker_queue_depth",
            Self::ReadbackPending => "readback_pending",
            Self::BuildVariantBufferPressure => "build_variant_buffer_pressure",
            Self::CoverageRowBufferPressure => "coverage_row_buffer_pressure",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct HybridBackpressureReport {
    gpu_queue_depth: u16,
    cpu_worker_queue_depth: u16,
    readback_pending_batches: u16,
    build_variant_buffer_pressure: u16,
    coverage_row_buffer_pressure: u16,
    throttled_backend: &'static str,
    throttle_reason: HybridThrottleReason,
    candidate_queue_len: u16,
    candidate_queue_capacity: u16,
    cpu_worker_backlog: u16,
    gpu_readback_backlog: u16,
    gpu_batch_in_flight: u16,
    backpressure_active: bool,
    deferred_batch_count: u16,
    truncated_batch_count: u16,
    memory_pressure_level: &'static str,
}

impl HybridBackpressureReport {
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
            gpu_queue_depth,
            cpu_worker_queue_depth,
            readback_pending_batches,
            build_variant_buffer_pressure,
            coverage_row_buffer_pressure,
            throttled_backend,
            throttle_reason,
            candidate_queue_len: gpu_queue_depth,
            candidate_queue_capacity: 0,
            cpu_worker_backlog: cpu_worker_queue_depth,
            gpu_readback_backlog: readback_pending_batches,
            gpu_batch_in_flight: readback_pending_batches,
            backpressure_active: !matches!(throttle_reason, HybridThrottleReason::None),
            deferred_batch_count: 0,
            truncated_batch_count: 0,
            memory_pressure_level: "low",
        }
    }
}
impl HybridBackpressureReport {
    pub const fn with_u2_contract(
        mut self,
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
        self.candidate_queue_len = candidate_queue_len;
        self.candidate_queue_capacity = candidate_queue_capacity;
        self.cpu_worker_backlog = cpu_worker_backlog;
        self.gpu_readback_backlog = gpu_readback_backlog;
        self.gpu_batch_in_flight = gpu_batch_in_flight;
        self.backpressure_active = backpressure_active;
        self.deferred_batch_count = deferred_batch_count;
        self.truncated_batch_count = truncated_batch_count;
        self.memory_pressure_level = memory_pressure_level;
        self
    }
}
impl HybridBackpressureReport {
    pub const fn throttle_reason(self) -> HybridThrottleReason {
        self.throttle_reason
    }
}
impl HybridBackpressureReport {
    pub const fn throttled_backend(self) -> &'static str {
        self.throttled_backend
    }
}
impl HybridBackpressureReport {
    pub const fn candidate_queue_len(self) -> u16 {
        self.candidate_queue_len
    }
}
impl HybridBackpressureReport {
    pub const fn candidate_queue_capacity(self) -> u16 {
        self.candidate_queue_capacity
    }
}
impl HybridBackpressureReport {
    pub const fn cpu_worker_backlog(self) -> u16 {
        self.cpu_worker_backlog
    }
}
impl HybridBackpressureReport {
    pub const fn gpu_readback_backlog(self) -> u16 {
        self.gpu_readback_backlog
    }
}
impl HybridBackpressureReport {
    pub const fn gpu_batch_in_flight(self) -> u16 {
        self.gpu_batch_in_flight
    }
}
impl HybridBackpressureReport {
    pub const fn backpressure_active(self) -> bool {
        self.backpressure_active
    }
}
impl HybridBackpressureReport {
    pub const fn deferred_batch_count(self) -> u16 {
        self.deferred_batch_count
    }
}
impl HybridBackpressureReport {
    pub const fn truncated_batch_count(self) -> u16 {
        self.truncated_batch_count
    }
}
impl HybridBackpressureReport {
    pub const fn memory_pressure_level(self) -> &'static str {
        self.memory_pressure_level
    }
}

#[cfg(test)]
#[path = "hybrid_backpressure_report_tests.rs"]
mod tests;
