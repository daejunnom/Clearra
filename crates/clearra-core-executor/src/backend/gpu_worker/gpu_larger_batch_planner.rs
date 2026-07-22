use super::{
    GpuWorkerBudget, GpuWorkerMemoryPressure, GpuWorkerMemoryPressureLevel, GpuWorkerMetrics,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GpuLargerBatchPlan {
    planned_batch_size: u32,
    larger_batch_planner: bool,
    dominance_prefilter_enabled: bool,
    readback_compression_enabled: bool,
    cpu_exact_confirm_optimization_enabled: bool,
}

impl GpuLargerBatchPlan {
    pub const fn planned_batch_size(self) -> u32 {
        self.planned_batch_size
    }
}
impl GpuLargerBatchPlan {
    pub const fn larger_batch_planner(self) -> bool {
        self.larger_batch_planner
    }
}
impl GpuLargerBatchPlan {
    pub const fn dominance_prefilter_enabled(self) -> bool {
        self.dominance_prefilter_enabled
    }
}
impl GpuLargerBatchPlan {
    pub const fn readback_compression_enabled(self) -> bool {
        self.readback_compression_enabled
    }
}
impl GpuLargerBatchPlan {
    pub const fn cpu_exact_confirm_optimization_enabled(self) -> bool {
        self.cpu_exact_confirm_optimization_enabled
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GpuLargerBatchPlanner;

impl GpuLargerBatchPlanner {
    pub fn plan(
        budget: GpuWorkerBudget,
        metrics: GpuWorkerMetrics,
        candidate_count_hint: u32,
    ) -> GpuLargerBatchPlan {
        let memory_pressure = GpuWorkerMemoryPressure::from_metrics(budget, metrics);
        let low_pressure = memory_pressure.level() != GpuWorkerMemoryPressureLevel::High
            && metrics.cpu_backlog() <= budget.max_cpu_backlog
            && metrics.gpu_readback_pending <= budget.max_readback_pending;
        let requested = if low_pressure {
            candidate_count_hint.max(budget.max_batch_size)
        } else {
            candidate_count_hint.min(budget.max_batch_size / 2)
        };

        GpuLargerBatchPlan {
            planned_batch_size: budget.clamp_batch_size(requested),
            larger_batch_planner: true,
            dominance_prefilter_enabled: true,
            readback_compression_enabled: true,
            cpu_exact_confirm_optimization_enabled: true,
        }
    }
}
