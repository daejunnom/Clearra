use crate::backend::HybridThrottleReason;

use super::{
    GpuWorkerBatchSizeDecision, GpuWorkerBatchSizer, GpuWorkerBudget, GpuWorkerMemoryPressure,
    GpuWorkerMemoryPressureLevel, GpuWorkerMetrics,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GpuWorkerAutotuneDecision {
    batch_size: GpuWorkerBatchSizeDecision,
    throttle_gpu_submission: bool,
    throttle_reason: HybridThrottleReason,
    memory_pressure: GpuWorkerMemoryPressure,
    reduce_trace_retention: bool,
    batch_scope_early_release: bool,
    throttle_coverage_row_emission: bool,
    count_only_mode_allowed: bool,
    partial_result_diagnostic: Option<&'static str>,
}

impl GpuWorkerAutotuneDecision {
    pub const fn selected_batch_size(self) -> u32 {
        self.batch_size.selected_batch_size()
    }
}
impl GpuWorkerAutotuneDecision {
    pub const fn prioritize_dedupe(self) -> bool {
        self.batch_size.prioritize_dedupe()
    }
}
impl GpuWorkerAutotuneDecision {
    pub const fn defer_low_priority_candidates(self) -> bool {
        self.batch_size.defer_low_priority_candidates()
    }
}
impl GpuWorkerAutotuneDecision {
    pub const fn throttle_gpu_submission(self) -> bool {
        self.throttle_gpu_submission
    }
}
impl GpuWorkerAutotuneDecision {
    pub const fn throttle_reason(self) -> HybridThrottleReason {
        self.throttle_reason
    }
}
impl GpuWorkerAutotuneDecision {
    pub const fn memory_pressure(self) -> GpuWorkerMemoryPressure {
        self.memory_pressure
    }
}
impl GpuWorkerAutotuneDecision {
    pub const fn reduce_trace_retention(self) -> bool {
        self.reduce_trace_retention
    }
}
impl GpuWorkerAutotuneDecision {
    pub const fn batch_scope_early_release(self) -> bool {
        self.batch_scope_early_release
    }
}
impl GpuWorkerAutotuneDecision {
    pub const fn throttle_coverage_row_emission(self) -> bool {
        self.throttle_coverage_row_emission
    }
}
impl GpuWorkerAutotuneDecision {
    pub const fn count_only_mode_allowed(self) -> bool {
        self.count_only_mode_allowed
    }
}
impl GpuWorkerAutotuneDecision {
    pub const fn partial_result_diagnostic(self) -> Option<&'static str> {
        self.partial_result_diagnostic
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GpuWorkerAutotune;

impl GpuWorkerAutotune {
    pub fn evaluate(
        budget: GpuWorkerBudget,
        metrics: GpuWorkerMetrics,
    ) -> GpuWorkerAutotuneDecision {
        let batch_size = GpuWorkerBatchSizer::select_batch_size(budget, metrics);
        let memory_pressure = GpuWorkerMemoryPressure::from_metrics(budget, metrics);
        let readback_high = metrics.gpu_readback_pending > budget.max_readback_pending;
        let coverage_pressure_high =
            metrics.coverage_row_buffer_pressure > budget.max_coverage_buffer_pressure;
        let memory_pressure_high = memory_pressure.level() == GpuWorkerMemoryPressureLevel::High;

        let throttle_reason = if readback_high {
            HybridThrottleReason::ReadbackPending
        } else if metrics.cpu_backlog() > budget.max_cpu_backlog {
            HybridThrottleReason::CpuWorkerQueueDepth
        } else if coverage_pressure_high {
            HybridThrottleReason::CoverageRowBufferPressure
        } else {
            HybridThrottleReason::None
        };

        let partial_result_diagnostic = if memory_pressure_high {
            Some("memory_pressure_truncated")
        } else if coverage_pressure_high {
            Some("coverage_row_buffer_pressure_truncated")
        } else {
            None
        };

        GpuWorkerAutotuneDecision {
            batch_size,
            throttle_gpu_submission: readback_high,
            throttle_reason,
            memory_pressure,
            reduce_trace_retention: memory_pressure_high,
            batch_scope_early_release: memory_pressure_high,
            throttle_coverage_row_emission: coverage_pressure_high,
            count_only_mode_allowed: coverage_pressure_high,
            partial_result_diagnostic,
        }
    }
}
