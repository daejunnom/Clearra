use super::{
    GpuWorkerBackendReport, GpuWorkerBuildResultBridge, GpuWorkerBuildUpMode,
    GpuWorkerCoverageBridgeReport,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GpuWorkerProductReport {
    backend_report: GpuWorkerBackendReport,
    build_mode: GpuWorkerBuildUpMode,
    confirmed_candidate_count: u32,
    build_variant_count: u32,
    coverage_row_count: usize,
    count_complete: bool,
    trace_retained: bool,
    objective_ready: bool,
    verify_first_used_for_coverage: bool,
}

impl GpuWorkerProductReport {
    pub fn from_build_and_coverage(
        backend_report: GpuWorkerBackendReport,
        build: GpuWorkerBuildResultBridge,
        coverage: Option<GpuWorkerCoverageBridgeReport>,
    ) -> Self {
        let coverage_row_count = coverage.map_or(0, GpuWorkerCoverageBridgeReport::row_count);
        Self {
            backend_report,
            build_mode: build.mode(),
            confirmed_candidate_count: build.confirmed_candidate_count(),
            build_variant_count: build.build_variant_count(),
            coverage_row_count,
            count_complete: build.count_complete(),
            trace_retained: build.trace_retained(),
            objective_ready: build.can_source_coverage_rows() && coverage_row_count > 0,
            verify_first_used_for_coverage: build.verify_first_used_for_coverage(),
        }
    }
}
impl GpuWorkerProductReport {
    pub const fn backend_report(self) -> GpuWorkerBackendReport {
        self.backend_report
    }
}
impl GpuWorkerProductReport {
    pub const fn build_mode(self) -> GpuWorkerBuildUpMode {
        self.build_mode
    }
}
impl GpuWorkerProductReport {
    pub const fn confirmed_candidate_count(self) -> u32 {
        self.confirmed_candidate_count
    }
}
impl GpuWorkerProductReport {
    pub const fn build_variant_count(self) -> u32 {
        self.build_variant_count
    }
}
impl GpuWorkerProductReport {
    pub const fn coverage_row_count(self) -> usize {
        self.coverage_row_count
    }
}
impl GpuWorkerProductReport {
    pub const fn count_complete(self) -> bool {
        self.count_complete
    }
}
impl GpuWorkerProductReport {
    pub const fn trace_retained(self) -> bool {
        self.trace_retained
    }
}
impl GpuWorkerProductReport {
    pub const fn objective_ready(self) -> bool {
        self.objective_ready
    }
}
impl GpuWorkerProductReport {
    pub const fn verify_first_used_for_coverage(self) -> bool {
        self.verify_first_used_for_coverage
    }
}
