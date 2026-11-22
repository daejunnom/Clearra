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
