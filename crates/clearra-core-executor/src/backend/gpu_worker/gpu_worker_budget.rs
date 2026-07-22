#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GpuWorkerBudget {
    pub min_batch_size: u32,
    pub max_batch_size: u32,
    pub max_readback_pending: u32,
    pub max_cpu_backlog: u32,
    pub max_memory_pressure: u32,
    pub max_coverage_buffer_pressure: u32,
}

impl GpuWorkerBudget {
    pub const fn default_local() -> Self {
        Self {
            min_batch_size: 16,
            max_batch_size: 256,
            max_readback_pending: 2,
            max_cpu_backlog: 8,
            max_memory_pressure: 75,
            max_coverage_buffer_pressure: 75,
        }
    }
}
impl GpuWorkerBudget {
    pub const fn clamp_batch_size(self, requested: u32) -> u32 {
        if requested < self.min_batch_size {
            self.min_batch_size
        } else if requested > self.max_batch_size {
            self.max_batch_size
        } else {
            requested
        }
    }
}

impl Default for GpuWorkerBudget {
    fn default() -> Self {
        Self::default_local()
    }
}
