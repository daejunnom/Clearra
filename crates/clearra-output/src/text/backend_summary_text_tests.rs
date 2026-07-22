use super::*;

#[test]
fn text_default_summarizes_gpu_worker_without_internal_noise() {
    let fields = vec![
        JsonField::new("backend_selected", "cpu"),
        JsonField::new("gpu_unavailable_reason", "gpu_feature_disabled"),
        JsonField::new("memory_leak_report_clean", "true"),
        JsonField::new("gpu_memory_ticket_id", "42"),
    ];

    let lines = BackendSummaryText::default_lines(&fields);

    assert_eq!(
        lines,
        [
            "backend: cpu",
            "gpu: unavailable (gpu_feature_disabled)",
            "memory: clean"
        ]
    );
    assert!(!lines.join("\n").contains("gpu_memory_ticket_id"));
}

#[test]
fn text_verbose_includes_gpu_worker_backpressure() {
    let fields = vec![
        JsonField::new("gpu_worker_state", "unavailable"),
        JsonField::new("gpu_trust_state", "deterministic-reference-matched"),
        JsonField::new("gpu_memory_ticket_id", "42"),
        JsonField::new("gpu_fence_epoch", "3"),
        JsonField::new("gpu_backpressure_gpu_queue_depth", "0"),
    ];

    let text = BackendSummaryText::verbose_lines(&fields).join("\n");

    assert!(text.contains("gpu_worker_state: unavailable"));
    assert!(text.contains("gpu_trust_state: deterministic-reference-matched"));
    assert!(text.contains("gpu_memory_ticket_id: 42"));
    assert!(text.contains("gpu_backpressure_gpu_queue_depth: 0"));
}

#[test]
fn backend_report_present_in_verbose_text() {
    let fields = vec![
        JsonField::new("backend_requested", "gpu"),
        JsonField::new("backend_selected", "cpu"),
        JsonField::new("candidate_backend", "cpu-packing"),
        JsonField::new("buildup_backend", "cpu-buildup"),
        JsonField::typed("gpu_available", crate::json::JsonValue::Bool(false)),
        JsonField::new("gpu_disabled_reason", "gpu_feature_disabled"),
        JsonField::new("gpu_trust_state", "deterministic-reference-matched"),
        JsonField::typed("cpu_confirm_required", crate::json::JsonValue::Bool(false)),
        JsonField::typed("cpu_reference_matched", crate::json::JsonValue::Bool(true)),
        JsonField::typed("fallback_used", crate::json::JsonValue::Bool(true)),
        JsonField::new("fallback_backend", "cpu"),
        JsonField::new("backend_fallback_reason", "gpu_feature_disabled"),
        JsonField::new("hybrid_status", "not-requested"),
        JsonField::new("hybrid_disabled_reason", "not_requested"),
        JsonField::new("memory_pressure_level", "normal"),
        JsonField::new("backpressure", "none"),
    ];

    let text = BackendSummaryText::verbose_lines(&fields).join("\n");

    for expected in [
        "backend_requested: gpu",
        "backend_selected: cpu",
        "candidate_backend: cpu-packing",
        "buildup_backend: cpu-buildup",
        "gpu_available: false",
        "gpu_disabled_reason: gpu_feature_disabled",
        "gpu_trust_state: deterministic-reference-matched",
        "cpu_confirm_required: false",
        "cpu_reference_matched: true",
        "fallback_used: true",
        "fallback_backend: cpu",
        "backend_fallback_reason: gpu_feature_disabled",
        "hybrid_status: not-requested",
        "hybrid_disabled_reason: not_requested",
        "memory_pressure_level: normal",
        "backpressure: none",
    ] {
        assert!(text.contains(expected), "missing {expected}\n{text}");
    }
}

#[test]
fn gpu_default_summary_ignores_non_actionable_reason() {
    let fields = vec![
        JsonField::new("gpu_disabled_reason", "not_requested"),
        JsonField::new("backend_fallback_reason", "none"),
    ];

    let lines = BackendSummaryText::default_lines(&fields);

    assert_eq!(lines[1], "gpu: not-used");
}
