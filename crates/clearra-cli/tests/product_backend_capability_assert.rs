use serde_json::Value;

pub fn backend_report(json: &Value) -> &serde_json::Map<String, Value> {
    json.pointer("/contract/pc/backend_report")
        .or_else(|| json.get("backend_report"))
        .and_then(Value::as_object)
        .expect("backend_report object must be present")
}

pub fn backend_report_string<'a>(json: &'a Value, field: &str) -> &'a str {
    backend_report(json)
        .get(field)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("backend_report.{field} string must be present"))
}

pub fn backend_report_bool(json: &Value, field: &str) -> bool {
    backend_report(json)
        .get(field)
        .and_then(Value::as_bool)
        .unwrap_or_else(|| panic!("backend_report.{field} bool must be present"))
}

pub fn assert_backend_report_has_u0_surface(json: &Value) {
    let report = backend_report(json);
    for field in [
        "backend_requested",
        "backend_selected",
        "candidate_backend",
        "buildup_backend",
        "gpu_available",
        "gpu_disabled_reason",
        "gpu_trust_state",
        "cpu_confirm_required",
        "cpu_reference_matched",
        "fallback_used",
        "fallback_backend",
        "backend_fallback_reason",
        "hybrid_status",
        "hybrid_disabled_reason",
        "memory_pressure_level",
        "backpressure",
    ] {
        assert!(
            report.contains_key(field),
            "backend_report.{field} must be present"
        );
    }
}

pub fn assert_u0_backend_capability_report(cpu: &Value, gpu_with_fallback: &Value, hybrid: &Value) {
    for value in [cpu, gpu_with_fallback, hybrid] {
        assert_backend_report_has_u0_surface(value);
    }

    assert_eq!(backend_report_string(cpu, "backend_requested"), "cpu");
    assert_eq!(backend_report_string(cpu, "backend_selected"), "cpu");
    assert_eq!(
        backend_report_string(cpu, "gpu_disabled_reason"),
        "not_requested"
    );
    assert!(!backend_report_bool(cpu, "gpu_available"));
    assert!(!backend_report_bool(cpu, "fallback_used"));
    assert_eq!(backend_report_string(cpu, "fallback_backend"), "none");
    assert_eq!(backend_report_string(cpu, "hybrid_status"), "not-requested");

    assert_eq!(
        backend_report_string(gpu_with_fallback, "backend_requested"),
        "gpu"
    );
    assert_eq!(
        backend_report_string(gpu_with_fallback, "backend_selected"),
        "cpu"
    );
    assert!(!backend_report_bool(gpu_with_fallback, "gpu_available"));
    assert_eq!(
        backend_report_string(gpu_with_fallback, "gpu_disabled_reason"),
        "gpu_kernel_unavailable"
    );
    assert!(backend_report_bool(gpu_with_fallback, "fallback_used"));
    assert_eq!(
        backend_report_string(gpu_with_fallback, "fallback_backend"),
        "cpu"
    );
    assert_eq!(
        backend_report_string(gpu_with_fallback, "backend_fallback_reason"),
        "gpu_kernel_unavailable"
    );

    assert_eq!(backend_report_string(hybrid, "backend_requested"), "hybrid");
    assert_eq!(backend_report_string(hybrid, "backend_selected"), "cpu");
    assert!(backend_report_bool(hybrid, "fallback_used"));
    assert_eq!(backend_report_string(hybrid, "fallback_backend"), "cpu");
    assert_eq!(backend_report_string(hybrid, "hybrid_status"), "disabled");
    assert_eq!(
        backend_report_string(hybrid, "hybrid_disabled_reason"),
        "gpu_kernel_unavailable"
    );
    assert_eq!(
        backend_report_string(hybrid, "backend_fallback_reason"),
        "gpu_kernel_unavailable"
    );
}
