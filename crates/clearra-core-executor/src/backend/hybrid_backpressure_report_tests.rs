use super::*;

#[test]
fn hybrid_backpressure_reports_throttle_reason() {
    let report = HybridBackpressureReport::new(
        8,
        2,
        3,
        5,
        7,
        "gpu-packing",
        HybridThrottleReason::ReadbackPending,
    );

    assert_eq!(report.throttled_backend(), "gpu-packing");
    assert_eq!(report.throttle_reason().as_str(), "readback_pending");
}

#[test]
fn hybrid_backpressure_report_exposes_u2_contract_fields() {
    let report = HybridBackpressureReport::new(
        8,
        2,
        3,
        5,
        7,
        "gpu-packing",
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
    assert_eq!(report.memory_pressure_level(), "moderate");
}
