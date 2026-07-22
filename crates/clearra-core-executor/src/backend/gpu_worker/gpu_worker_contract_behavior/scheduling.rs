use super::*;

mod case_gpu_worker_backpressure_reports_throttle_reason {

    use super::*;

    #[test]

    fn gpu_worker_backpressure_reports_throttle_reason() {
        let backpressure = GpuWorkerBackpressure::new(
            8,
            2,
            3,
            5,
            7,
            "gpu-worker-v0.1",
            HybridThrottleReason::ReadbackPending,
        );

        assert_eq!(
            backpressure.report().throttle_reason(),
            HybridThrottleReason::ReadbackPending
        );

        assert_eq!(backpressure.report().throttled_backend(), "gpu-worker-v0.1");
    }
}

mod case_autotune_reduces_batch_size_when_cpu_backlog_high {

    use super::*;

    #[test]

    fn autotune_reduces_batch_size_when_cpu_backlog_high() {
        let budget = GpuWorkerBudget::default();

        let metrics = GpuWorkerMetrics {
            cpu_confirm_queue_depth: 12,

            cpu_buildup_queue_depth: 8,

            ..Default::default()
        };

        let decision = GpuWorkerAutotune::evaluate(budget, metrics);

        assert!(decision.selected_batch_size() < budget.max_batch_size);

        assert!(decision.prioritize_dedupe());

        assert!(decision.defer_low_priority_candidates());

        assert_eq!(
            decision.throttle_reason(),
            HybridThrottleReason::CpuWorkerQueueDepth
        );
    }
}

mod case_autotune_throttles_when_readback_pending_high {

    use super::*;

    #[test]

    fn autotune_throttles_when_readback_pending_high() {
        let budget = GpuWorkerBudget::default();

        let metrics = GpuWorkerMetrics {
            gpu_readback_pending: budget.max_readback_pending + 1,

            ..Default::default()
        };

        let decision = GpuWorkerAutotune::evaluate(budget, metrics);

        assert!(decision.throttle_gpu_submission());

        assert_eq!(
            decision.throttle_reason(),
            HybridThrottleReason::ReadbackPending
        );
    }
}

mod case_autotune_reports_memory_pressure {

    use super::*;

    #[test]

    fn autotune_reports_memory_pressure() {
        let budget = GpuWorkerBudget::default();

        let metrics = GpuWorkerMetrics {
            memory_ticket_live_count: budget.max_memory_pressure + 5,

            pending_release_queue_depth: budget.max_memory_pressure + 1,

            ..Default::default()
        };

        let decision = GpuWorkerAutotune::evaluate(budget, metrics);

        assert_eq!(
            decision.memory_pressure().level(),
            GpuWorkerMemoryPressureLevel::High
        );

        assert!(decision.reduce_trace_retention());

        assert!(decision.batch_scope_early_release());
    }
}

mod case_autotune_never_drops_coverage_rows_silently {

    use super::*;

    #[test]

    fn autotune_never_drops_coverage_rows_silently() {
        let budget = GpuWorkerBudget::default();

        let metrics = GpuWorkerMetrics {
            coverage_row_buffer_pressure: budget.max_coverage_buffer_pressure + 1,

            ..Default::default()
        };

        let decision = GpuWorkerAutotune::evaluate(budget, metrics);

        assert!(decision.throttle_coverage_row_emission());

        assert!(decision.count_only_mode_allowed());

        assert_eq!(
            decision.partial_result_diagnostic(),
            Some("coverage_row_buffer_pressure_truncated")
        );
    }
}

mod case_partial_result_reports_truncation_reason {

    use super::*;

    #[test]

    fn partial_result_reports_truncation_reason() {
        let budget = GpuWorkerBudget::default();

        let metrics = GpuWorkerMetrics {
            memory_ticket_live_count: budget.max_memory_pressure + 1,

            ..Default::default()
        };

        let decision = GpuWorkerAutotune::evaluate(budget, metrics);

        assert_eq!(
            decision.partial_result_diagnostic(),
            Some("memory_pressure_truncated")
        );
    }
}
