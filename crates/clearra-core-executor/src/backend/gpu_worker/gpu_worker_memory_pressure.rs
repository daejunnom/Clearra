use super::{GpuWorkerBudget, GpuWorkerMetrics};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum GpuWorkerMemoryPressureLevel {
    #[default]
    Low,
    Moderate,
    High,
}

impl GpuWorkerMemoryPressureLevel {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Moderate => "moderate",
            Self::High => "high",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GpuWorkerMemoryPressure {
    level: GpuWorkerMemoryPressureLevel,
    memory_ticket_live_count: u32,
    pending_release_queue_depth: u32,
    pressure_score: u32,
}

impl GpuWorkerMemoryPressure {
    pub fn from_metrics(budget: GpuWorkerBudget, metrics: GpuWorkerMetrics) -> Self {
        let pressure_score = metrics.memory_pressure_score();
        let moderate_threshold = budget.max_memory_pressure / 2;
        let level = if pressure_score >= budget.max_memory_pressure {
            GpuWorkerMemoryPressureLevel::High
        } else if pressure_score >= moderate_threshold {
            GpuWorkerMemoryPressureLevel::Moderate
        } else {
            GpuWorkerMemoryPressureLevel::Low
        };

        Self {
            level,
            memory_ticket_live_count: metrics.memory_ticket_live_count,
            pending_release_queue_depth: metrics.pending_release_queue_depth,
            pressure_score,
        }
    }
}
impl GpuWorkerMemoryPressure {
    pub const fn level(self) -> GpuWorkerMemoryPressureLevel {
        self.level
    }
}
impl GpuWorkerMemoryPressure {
    pub const fn memory_ticket_live_count(self) -> u32 {
        self.memory_ticket_live_count
    }
}
impl GpuWorkerMemoryPressure {
    pub const fn pending_release_queue_depth(self) -> u32 {
        self.pending_release_queue_depth
    }
}
impl GpuWorkerMemoryPressure {
    pub const fn pressure_score(self) -> u32 {
        self.pressure_score
    }
}
