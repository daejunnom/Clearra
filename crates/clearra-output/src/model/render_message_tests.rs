use super::*;

#[test]
fn with_field_preserves_numeric_looking_values_as_strings_for_json() {
    let message = RenderMessage::new("pc").with_field("token", "001");
    let contract = message.json_contract();

    let crate::json::json_contract::JsonValue::Object(root) = contract.root() else {
        panic!("root object");
    };
    let summary = object_member(&root, "summary");
    assert_eq!(
        member_value(summary, "token"),
        &crate::json::json_contract::JsonValue::string("001")
    );
}

#[test]
fn with_value_carries_explicit_typed_values_to_json() {
    let message = RenderMessage::new("pc")
        .with_value("lines", 2usize)
        .with_value("solution_found", true);
    let contract = message.json_contract();

    let crate::json::json_contract::JsonValue::Object(root) = contract.root() else {
        panic!("root object");
    };
    let summary = object_member(&root, "summary");
    assert_eq!(
        member_value(summary, "lines"),
        &crate::json::json_contract::JsonValue::number("2")
    );
    assert_eq!(
        member_value(summary, "solution_found"),
        &crate::json::json_contract::JsonValue::Bool(true)
    );
}

#[test]
fn fumen_pages_keep_trace_summary_without_verbose_json_contract_fields() {
    let message = RenderMessage::new("pc")
        .with_value("status", "searched")
        .with_value("route", "search-problem-core-executor")
        .with_value("search_execution_report", "attached")
        .with_value("retained_trace_keys", "trk1:very-long-key");
    let pages = message.fumen_pages();

    assert_eq!(pages.len(), 1);
    assert!(pages[0].contains("kind=pc"));
    assert!(pages[0].contains("status=searched"));
    assert!(pages[0].contains("route=search-problem-core-executor"));
    assert!(!pages[0].contains("search_execution_report=attached"));
    assert!(!pages[0].contains("retained_trace_keys=trk1:very-long-key"));
}

#[test]
fn default_text_uses_human_summary_field_policy() {
    let message = RenderMessage::new("pc")
        .with_value("status", "searched")
        .with_value("lines", 2usize)
        .with_value("queue_len", 7usize)
        .with_value("trace_retention_reason", "none")
        .with_value("executor_flow", "rust-shell-to-core")
        .with_value("compact_problem_descriptor", "compact")
        .with_value("gpu_backend_scope", "disabled")
        .with_value("score_event_basis", "sample")
        .with_value("coverage_row_view", "raw");

    let lines = message.text_lines();
    let rendered = lines.join("\n");

    assert!(rendered.contains("kind: pc"));
    assert!(rendered.contains("status: searched"));
    assert!(rendered.contains("lines: 2"));
    assert!(rendered.contains("queue_len: 7"));
    assert!(rendered.contains("trace_retention_reason: none"));
    assert!(!rendered.contains("executor_flow"));
    assert!(!rendered.contains("compact_problem_descriptor"));
    assert!(!rendered.contains("gpu_backend_scope"));
    assert!(!rendered.contains("score_event_basis"));
    assert!(!rendered.contains("coverage_row_view"));
}

#[test]
fn verbose_text_preserves_full_render_fields() {
    let message = RenderMessage::new("pc")
        .with_value("lines", 2usize)
        .with_value("trace_retention_reason", "none")
        .with_value("executor_flow", "rust-shell-to-core");

    let rendered = message
        .text_lines_with_profile(TextOutputProfile::Verbose)
        .join("\n");

    assert!(rendered.contains("lines: 2"));
    assert!(rendered.contains("trace_retention_reason: none"));
    assert!(rendered.contains("executor_flow: rust-shell-to-core"));
}

#[test]
fn text_default_summarizes_gpu_worker_without_internal_noise() {
    let message = RenderMessage::new("pc")
        .with_field("backend_selected", "cpu")
        .with_field("gpu_unavailable_reason", "gpu_feature_disabled")
        .with_field("memory_leak_report_clean", "true")
        .with_field("gpu_memory_ticket_id", "42");

    let text = message.text_lines().join("\n");

    assert!(text.contains("backend: cpu"));
    assert!(text.contains("gpu: unavailable (gpu_feature_disabled)"));
    assert!(text.contains("memory: clean"));
    assert!(!text.contains("gpu_memory_ticket_id"));
}

#[test]
fn text_verbose_includes_gpu_worker_backpressure() {
    let message = RenderMessage::new("pc")
        .with_field("backend_selected", "cpu")
        .with_field("gpu_worker_state", "unavailable")
        .with_field("gpu_trust_state", "deterministic-reference-matched")
        .with_field("gpu_memory_ticket_id", "42")
        .with_field("gpu_fence_epoch", "3")
        .with_field("gpu_backpressure_gpu_queue_depth", "0");

    let text = message
        .text_lines_with_profile(TextOutputProfile::Verbose)
        .join("\n");

    assert!(text.contains("gpu_worker_state: unavailable"));
    assert!(text.contains("gpu_trust_state: deterministic-reference-matched"));
    assert!(text.contains("gpu_memory_ticket_id: 42"));
    assert!(text.contains("gpu_backpressure_gpu_queue_depth: 0"));
}

fn member_value<'a>(
    members: &'a [crate::json::json_contract::JsonMember],
    key: &str,
) -> &'a crate::json::json_contract::JsonValue {
    members
        .iter()
        .find_map(|member| (member.key() == key).then_some(member.value()))
        .expect("member exists")
}

fn object_member<'a>(
    members: &'a [crate::json::json_contract::JsonMember],
    key: &str,
) -> &'a [crate::json::json_contract::JsonMember] {
    let crate::json::json_contract::JsonValue::Object(nested) = member_value(members, key) else {
        panic!("object member");
    };
    nested
}
