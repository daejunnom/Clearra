mod backpressure_accessors {
    use super::HybridSchedulerReport;

    impl HybridSchedulerReport {
        pub fn gpu_worker_backpressure_gpu_queue_depth(self) -> u16 {
            self.backpressure.gpu_queue_depth
        }
    }
    impl HybridSchedulerReport {
        pub fn gpu_worker_backpressure_cpu_worker_queue_depth(self) -> u16 {
            self.backpressure.cpu_worker_queue_depth
        }
    }
    impl HybridSchedulerReport {
        pub fn gpu_worker_backpressure_readback_pending_batches(self) -> u16 {
            self.backpressure.readback_pending_batches
        }
    }
    impl HybridSchedulerReport {
        pub fn gpu_worker_backpressure_build_variant_buffer_pressure(self) -> u16 {
            self.backpressure.build_variant_buffer_pressure
        }
    }
    impl HybridSchedulerReport {
        pub fn gpu_worker_backpressure_coverage_row_buffer_pressure(self) -> u16 {
            self.backpressure.coverage_row_buffer_pressure
        }
    }
    impl HybridSchedulerReport {
        pub fn gpu_worker_backpressure_throttled_backend(self) -> &'static str {
            self.backpressure.throttled_backend
        }
    }
    impl HybridSchedulerReport {
        pub fn gpu_worker_backpressure_throttle_reason(self) -> &'static str {
            self.backpressure.throttle_reason
        }
    }
    impl HybridSchedulerReport {
        pub fn hybrid_candidate_queue_len(self) -> u16 {
            self.backpressure.candidate_queue_len
        }
    }
    impl HybridSchedulerReport {
        pub fn hybrid_candidate_queue_capacity(self) -> u16 {
            self.backpressure.candidate_queue_capacity
        }
    }
    impl HybridSchedulerReport {
        pub fn hybrid_cpu_worker_backlog(self) -> u16 {
            self.backpressure.cpu_worker_backlog
        }
    }
    impl HybridSchedulerReport {
        pub fn hybrid_gpu_readback_backlog(self) -> u16 {
            self.backpressure.gpu_readback_backlog
        }
    }
    impl HybridSchedulerReport {
        pub fn hybrid_gpu_batch_in_flight(self) -> u16 {
            self.backpressure.gpu_batch_in_flight
        }
    }
    impl HybridSchedulerReport {
        pub fn hybrid_backpressure_active(self) -> bool {
            self.backpressure.active
        }
    }
    impl HybridSchedulerReport {
        pub fn hybrid_deferred_batch_count(self) -> u16 {
            self.backpressure.deferred_batch_count
        }
    }
    impl HybridSchedulerReport {
        pub fn hybrid_truncated_batch_count(self) -> u16 {
            self.backpressure.truncated_batch_count
        }
    }
    impl HybridSchedulerReport {
        pub fn hybrid_memory_pressure_level(self) -> &'static str {
            self.backpressure.memory_pressure_level
        }
    }
}
mod backpressure_report {
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(super) struct HybridBackpressureReport {
        pub(super) gpu_queue_depth: u16,
        pub(super) cpu_worker_queue_depth: u16,
        pub(super) readback_pending_batches: u16,
        pub(super) build_variant_buffer_pressure: u16,
        pub(super) coverage_row_buffer_pressure: u16,
        pub(super) throttled_backend: &'static str,
        pub(super) throttle_reason: &'static str,
        pub(super) candidate_queue_len: u16,
        pub(super) candidate_queue_capacity: u16,
        pub(super) cpu_worker_backlog: u16,
        pub(super) gpu_readback_backlog: u16,
        pub(super) gpu_batch_in_flight: u16,
        pub(super) active: bool,
        pub(super) deferred_batch_count: u16,
        pub(super) truncated_batch_count: u16,
        pub(super) memory_pressure_level: &'static str,
    }
}
mod equivalence_accessors {
    use super::HybridSchedulerReport;

    impl HybridSchedulerReport {
        pub fn gpu_assisted_buildup_reached(self) -> bool {
            self.equivalence.gpu_assisted_buildup_reached
        }
    }
    impl HybridSchedulerReport {
        pub fn gpu_only_packing_cpu_buildup_matches_cpu_reference(self) -> bool {
            self.equivalence.matches_cpu_reference
        }
    }
    impl HybridSchedulerReport {
        pub fn cpu_reference_candidate_count(self) -> u16 {
            self.equivalence.cpu_reference_candidate_count
        }
    }
    impl HybridSchedulerReport {
        pub fn hybrid_candidate_count(self) -> u16 {
            self.equivalence.hybrid_candidate_count
        }
    }
    impl HybridSchedulerReport {
        pub fn cpu_reference_build_variant_count(self) -> u16 {
            self.equivalence.cpu_reference_build_variant_count
        }
    }
    impl HybridSchedulerReport {
        pub fn hybrid_build_variant_count(self) -> u16 {
            self.equivalence.hybrid_build_variant_count
        }
    }
    impl HybridSchedulerReport {
        pub fn cpu_reference_coverage_row_count(self) -> u16 {
            self.equivalence.cpu_reference_coverage_row_count
        }
    }
    impl HybridSchedulerReport {
        pub fn hybrid_coverage_row_count(self) -> u16 {
            self.equivalence.hybrid_coverage_row_count
        }
    }
    impl HybridSchedulerReport {
        pub fn coverage_rows_from_enumerate_variants(self) -> bool {
            self.equivalence.coverage_rows_from_enumerate_variants
        }
    }
    impl HybridSchedulerReport {
        pub fn verify_first_used_for_coverage(self) -> bool {
            self.equivalence.verify_first_used_for_coverage
        }
    }
}
mod equivalence_report {
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(super) struct HybridEquivalenceReport {
        pub(super) gpu_assisted_buildup_reached: bool,
        pub(super) matches_cpu_reference: bool,
        pub(super) cpu_reference_candidate_count: u16,
        pub(super) hybrid_candidate_count: u16,
        pub(super) cpu_reference_build_variant_count: u16,
        pub(super) hybrid_build_variant_count: u16,
        pub(super) cpu_reference_coverage_row_count: u16,
        pub(super) hybrid_coverage_row_count: u16,
        pub(super) coverage_rows_from_enumerate_variants: bool,
        pub(super) verify_first_used_for_coverage: bool,
    }
}
mod gpu_worker_accessors {
    use super::HybridSchedulerReport;

    impl HybridSchedulerReport {
        pub fn gpu_worker_state(self) -> &'static str {
            self.gpu_worker.state
        }
    }
    impl HybridSchedulerReport {
        pub fn gpu_worker_trust_state(self) -> &'static str {
            self.gpu_worker.trust_state
        }
    }
    impl HybridSchedulerReport {
        pub fn gpu_worker_memory_ticket_id(self) -> u64 {
            self.gpu_worker.memory_ticket_id
        }
    }
    impl HybridSchedulerReport {
        pub fn gpu_worker_fence_epoch(self) -> u64 {
            self.gpu_worker.fence_epoch
        }
    }
    impl HybridSchedulerReport {
        pub fn gpu_worker_cpu_confirm_required(self) -> bool {
            self.gpu_worker.cpu_confirm_required
        }
    }
    impl HybridSchedulerReport {
        pub fn gpu_worker_can_source_exact_probability(self) -> bool {
            self.gpu_worker.can_source_exact_probability
        }
    }
}
mod gpu_worker_report {
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(super) struct HybridGpuWorkerReport {
        pub(super) state: &'static str,
        pub(super) trust_state: &'static str,
        pub(super) memory_ticket_id: u64,
        pub(super) fence_epoch: u64,
        pub(super) cpu_confirm_required: bool,
        pub(super) can_source_exact_probability: bool,
    }
}
mod unavailable_contract {
    use crate::backend::{BackendTrustReport, SelectedSearchBackend};

    use super::{
        backpressure_report::HybridBackpressureReport, equivalence_report::HybridEquivalenceReport,
        gpu_worker_report::HybridGpuWorkerReport, plan_report::HybridPlanReport,
        HybridSchedulerReport,
    };

    impl HybridSchedulerReport {
        pub const fn unavailable() -> Self {
            Self {
                enabled: false,
                plan: HybridPlanReport {
                    gpu_large_packing_batch: false,
                    cpu_small_irregular_buildup: false,
                    gpu_readback_cpu_buildup_overlap: false,
                    batch_buffer_reuse: false,
                    memory_epoch_managed: false,
                    backend_metrics_reported: false,
                },
                fallback_reason: "native_gpu_backend_not_built",
                gpu_worker: HybridGpuWorkerReport {
                    state: "unavailable",
                    trust_state: "unavailable",
                    memory_ticket_id: 0,
                    fence_epoch: 0,
                    cpu_confirm_required: true,
                    can_source_exact_probability: false,
                },
                backpressure: HybridBackpressureReport {
                    gpu_queue_depth: 0,
                    cpu_worker_queue_depth: 0,
                    readback_pending_batches: 0,
                    build_variant_buffer_pressure: 0,
                    coverage_row_buffer_pressure: 0,
                    throttled_backend: "none",
                    throttle_reason: "none",
                    candidate_queue_len: 0,
                    candidate_queue_capacity: 0,
                    cpu_worker_backlog: 0,
                    gpu_readback_backlog: 0,
                    gpu_batch_in_flight: 0,
                    active: false,
                    deferred_batch_count: 0,
                    truncated_batch_count: 0,
                    memory_pressure_level: "low",
                },
                equivalence: HybridEquivalenceReport {
                    gpu_assisted_buildup_reached: false,
                    matches_cpu_reference: false,
                    cpu_reference_candidate_count: 0,
                    hybrid_candidate_count: 0,
                    cpu_reference_build_variant_count: 0,
                    hybrid_build_variant_count: 0,
                    cpu_reference_coverage_row_count: 0,
                    hybrid_coverage_row_count: 0,
                    coverage_rows_from_enumerate_variants: false,
                    verify_first_used_for_coverage: false,
                },
            }
        }

        pub fn from_execution(
            actual_backend: SelectedSearchBackend,
            trust: BackendTrustReport,
            candidate_count: usize,
        ) -> Self {
            if !matches!(
                actual_backend,
                SelectedSearchBackend::Gpu | SelectedSearchBackend::Hybrid
            ) {
                return Self::unavailable();
            }

            let candidate_count = u16::try_from(candidate_count).unwrap_or(u16::MAX);
            let hybrid = actual_backend == SelectedSearchBackend::Hybrid;
            Self {
                enabled: hybrid,
                plan: HybridPlanReport {
                    gpu_large_packing_batch: false,
                    cpu_small_irregular_buildup: false,
                    gpu_readback_cpu_buildup_overlap: false,
                    batch_buffer_reuse: false,
                    memory_epoch_managed: false,
                    backend_metrics_reported: true,
                },
                fallback_reason: "none",
                gpu_worker: HybridGpuWorkerReport {
                    state: "completed",
                    trust_state: trust.state().as_str(),
                    memory_ticket_id: 0,
                    fence_epoch: 0,
                    cpu_confirm_required: true,
                    can_source_exact_probability: trust.can_source_exact_probability(),
                },
                backpressure: HybridBackpressureReport {
                    gpu_queue_depth: 0,
                    cpu_worker_queue_depth: 0,
                    readback_pending_batches: 0,
                    build_variant_buffer_pressure: 0,
                    coverage_row_buffer_pressure: 0,
                    throttled_backend: "none",
                    throttle_reason: "none",
                    candidate_queue_len: 0,
                    candidate_queue_capacity: 0,
                    cpu_worker_backlog: 0,
                    gpu_readback_backlog: 0,
                    gpu_batch_in_flight: 0,
                    active: false,
                    deferred_batch_count: 0,
                    truncated_batch_count: 0,
                    memory_pressure_level: "low",
                },
                equivalence: HybridEquivalenceReport {
                    gpu_assisted_buildup_reached: false,
                    matches_cpu_reference: trust.cpu_confirmed(),
                    cpu_reference_candidate_count: candidate_count,
                    hybrid_candidate_count: candidate_count,
                    cpu_reference_build_variant_count: 0,
                    hybrid_build_variant_count: 0,
                    cpu_reference_coverage_row_count: 0,
                    hybrid_coverage_row_count: 0,
                    coverage_rows_from_enumerate_variants: false,
                    verify_first_used_for_coverage: false,
                },
            }
        }
    }
}
mod model {
    use super::{
        backpressure_report::HybridBackpressureReport, equivalence_report::HybridEquivalenceReport,
        gpu_worker_report::HybridGpuWorkerReport, plan_report::HybridPlanReport,
    };

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct HybridSchedulerReport {
        pub(super) enabled: bool,
        pub(super) plan: HybridPlanReport,
        pub(super) fallback_reason: &'static str,
        pub(super) gpu_worker: HybridGpuWorkerReport,
        pub(super) backpressure: HybridBackpressureReport,
        pub(super) equivalence: HybridEquivalenceReport,
    }

    impl HybridSchedulerReport {
        pub fn enabled(self) -> bool {
            self.enabled
        }
    }
    impl HybridSchedulerReport {
        pub fn fallback_reason(self) -> &'static str {
            self.fallback_reason
        }
    }
}
mod plan_accessors {
    use super::HybridSchedulerReport;

    impl HybridSchedulerReport {
        pub fn gpu_large_packing_batch(self) -> bool {
            self.plan.gpu_large_packing_batch
        }
    }
    impl HybridSchedulerReport {
        pub fn cpu_small_irregular_buildup(self) -> bool {
            self.plan.cpu_small_irregular_buildup
        }
    }
    impl HybridSchedulerReport {
        pub fn gpu_readback_cpu_buildup_overlap(self) -> bool {
            self.plan.gpu_readback_cpu_buildup_overlap
        }
    }
    impl HybridSchedulerReport {
        pub fn batch_buffer_reuse(self) -> bool {
            self.plan.batch_buffer_reuse
        }
    }
    impl HybridSchedulerReport {
        pub fn memory_epoch_managed(self) -> bool {
            self.plan.memory_epoch_managed
        }
    }
    impl HybridSchedulerReport {
        pub fn backend_metrics_reported(self) -> bool {
            self.plan.backend_metrics_reported
        }
    }
}
mod plan_report {
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(super) struct HybridPlanReport {
        pub(super) gpu_large_packing_batch: bool,
        pub(super) cpu_small_irregular_buildup: bool,
        pub(super) gpu_readback_cpu_buildup_overlap: bool,
        pub(super) batch_buffer_reuse: bool,
        pub(super) memory_epoch_managed: bool,
        pub(super) backend_metrics_reported: bool,
    }
}
pub use model::HybridSchedulerReport;
