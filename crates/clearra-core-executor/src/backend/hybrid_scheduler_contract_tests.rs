use super::{
    GpuWorkerBatchSizer, GpuWorkerBudget, GpuWorkerMetrics, HybridBackpressureReport,
    HybridThrottleReason,
};

#[test]
fn memory_pressure_reduces_batch_size() {
    let budget = GpuWorkerBudget::default();
    let metrics = GpuWorkerMetrics {
        memory_ticket_live_count: budget.max_memory_pressure + 5,
        ..Default::default()
    };

    let decision = GpuWorkerBatchSizer::select_batch_size(budget, metrics);

    assert!(decision.selected_batch_size() < budget.max_batch_size);
    assert!(decision.reduced_for_memory_pressure());
}

#[test]
fn hybrid_backpressure_report_exposes_u2_scheduler_fields() {
    let report = HybridBackpressureReport::new(
        8,
        2,
        3,
        5,
        7,
        "hybrid-gpu-packing",
        HybridThrottleReason::ReadbackPending,
    )
    .with_u2_contract(9, 64, 2, 3, 4, true, 1, 0, "moderate");

    assert_eq!(report.candidate_queue_len(), 9);
    assert_eq!(report.candidate_queue_capacity(), 64);
    assert_eq!(report.cpu_worker_backlog(), 2);
    assert_eq!(report.gpu_readback_backlog(), 3);
    assert_eq!(report.gpu_batch_in_flight(), 4);
    assert!(report.backpressure_active());
    assert_eq!(report.deferred_batch_count(), 1);
    assert_eq!(report.truncated_batch_count(), 0);
    assert_eq!(report.memory_pressure_level(), "moderate");
}
