#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GpuWorkerMetrics {
    pub gpu_batches_submitted: u32,
    pub gpu_batches_completed: u32,
    pub gpu_readback_pending: u32,
    pub cpu_confirm_queue_depth: u32,
    pub cpu_buildup_queue_depth: u32,
    pub candidate_buffer_pressure: u32,
    pub coverage_row_buffer_pressure: u32,
    pub memory_ticket_live_count: u32,
    pub pending_release_queue_depth: u32,
    pub average_batch_latency_ms: u32,
    pub average_cpu_confirm_latency_ms: u32,
}

impl GpuWorkerMetrics {
    pub const fn cpu_backlog(self) -> u32 {
        self.cpu_confirm_queue_depth
            .saturating_add(self.cpu_buildup_queue_depth)
    }
}
impl GpuWorkerMetrics {
    pub const fn memory_pressure_score(self) -> u32 {
        if self.memory_ticket_live_count > self.pending_release_queue_depth {
            self.memory_ticket_live_count
        } else {
            self.pending_release_queue_depth
        }
    }
}
