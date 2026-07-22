use super::{
    GpuWorkerBudget, GpuWorkerMemoryPressure, GpuWorkerMemoryPressureLevel, GpuWorkerMetrics,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GpuWorkerBatchSizeDecision {
    selected_batch_size: u32,
    reduced_for_cpu_backlog: bool,
    reduced_for_memory_pressure: bool,
    prioritize_dedupe: bool,
    defer_low_priority_candidates: bool,
}

impl GpuWorkerBatchSizeDecision {
    pub const fn selected_batch_size(self) -> u32 {
        self.selected_batch_size
    }
}
impl GpuWorkerBatchSizeDecision {
    pub const fn reduced_for_cpu_backlog(self) -> bool {
        self.reduced_for_cpu_backlog
    }
}
impl GpuWorkerBatchSizeDecision {
    pub const fn reduced_for_memory_pressure(self) -> bool {
        self.reduced_for_memory_pressure
    }
}
impl GpuWorkerBatchSizeDecision {
    pub const fn prioritize_dedupe(self) -> bool {
        self.prioritize_dedupe
    }
}
impl GpuWorkerBatchSizeDecision {
    pub const fn defer_low_priority_candidates(self) -> bool {
        self.defer_low_priority_candidates
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GpuWorkerBatchSizer;

impl GpuWorkerBatchSizer {
    pub fn select_batch_size(
        budget: GpuWorkerBudget,
        metrics: GpuWorkerMetrics,
    ) -> GpuWorkerBatchSizeDecision {
        let memory_pressure = GpuWorkerMemoryPressure::from_metrics(budget, metrics);
        let cpu_backlog_high = metrics.cpu_backlog() > budget.max_cpu_backlog;
        let memory_pressure_high = memory_pressure.level() == GpuWorkerMemoryPressureLevel::High;

        let mut selected_batch_size = budget.max_batch_size;
        if cpu_backlog_high {
            selected_batch_size /= 2;
        }
        if memory_pressure_high {
            selected_batch_size /= 2;
        }
        selected_batch_size = budget.clamp_batch_size(selected_batch_size);

        GpuWorkerBatchSizeDecision {
            selected_batch_size,
            reduced_for_cpu_backlog: cpu_backlog_high,
            reduced_for_memory_pressure: memory_pressure_high,
            prioritize_dedupe: cpu_backlog_high,
            defer_low_priority_candidates: cpu_backlog_high,
        }
    }
}
