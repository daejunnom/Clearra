use serde_json::Value;

fn report(value: &Value) -> &serde_json::Map<String, Value> {
    value
        .pointer("/contract/pc/backend_report")
        .or_else(|| value.get("backend_report"))
        .and_then(Value::as_object)
        .expect("PC backend report")
}

pub(super) fn assert_cpu_route(value: &Value) {
    let backend = report(value);
    assert_eq!(backend["backend_requested"], "cpu");
    assert_eq!(backend["backend_selected"], "wasm-cpu");
    assert_eq!(backend["gpu_available"], false);
    assert_eq!(backend["gpu_disabled_reason"], "not_requested");
    assert_eq!(backend["fallback_used"], false);
    assert_eq!(backend["backend_fallback_reason"], Value::Null);
}

#[cfg(not(feature = "webgpu-search"))]
pub(super) fn assert_unavailable_gpu_fallback(value: &Value) {
    let backend = report(value);
    assert_eq!(backend["backend_requested"], "gpu");
    assert_eq!(backend["backend_selected"], "wasm-cpu");
    assert_eq!(backend["gpu_available"], false);
    assert_eq!(backend["fallback_used"], true);
    assert_eq!(backend["fallback_backend"], "wasm-cpu");
    assert_eq!(backend["backend_fallback_reason"], "gpu_kernel_unavailable");
    assert_eq!(
        backend["gpu_disabled_reason"],
        backend["backend_fallback_reason"]
    );
    assert_eq!(backend["gpu_trust_state"], "fallback-used");
}

#[cfg(not(feature = "webgpu-search"))]
pub(super) fn assert_cpu_selected_hybrid(value: &Value) {
    let backend = report(value);
    assert_eq!(backend["backend_requested"], "hybrid");
    assert_eq!(backend["backend_selected"], "wasm-cpu");
    assert_eq!(backend["gpu_available"], false);
    assert_eq!(backend["fallback_used"], false);
    assert_eq!(backend["backend_fallback_reason"], Value::Null);
    assert_eq!(backend["hybrid_status"], "cpu-selected");
    assert_eq!(backend["hybrid_disabled_reason"], "gpu_kernel_unavailable");
}
