use super::*;
use crate::json::JsonField;

#[test]
fn json_backend_report_includes_gpu_worker_trust_state() {
    let fields = vec![
        JsonField::new("gpu_worker_state", "unavailable"),
        JsonField::new("gpu_trust_state", "deterministic-reference-matched"),
    ];

    let contract = backend_gpu_worker_contract(&fields);

    assert!(format!("{contract:?}").contains("deterministic-reference-matched"));
}

#[test]
fn json_gpu_worker_report_shows_connected_confirmed_state() {
    let fields = vec![
        JsonField::new("gpu_worker_state", "connected"),
        JsonField::new("gpu_trust_state", "gpu-computed-cpu-confirmed"),
        JsonField::typed("cpu_confirm_required", JsonValue::Bool(false)),
        JsonField::typed("gpu_can_source_exact_probability", JsonValue::Bool(true)),
    ];

    let JsonValue::Object(members) = backend_gpu_worker_contract(&fields) else {
        panic!("gpu worker object");
    };

    assert_eq!(
        member_value(&members, "state"),
        &JsonValue::string("connected")
    );
    assert_eq!(
        member_value(&members, "trust_state"),
        &JsonValue::string("gpu-computed-cpu-confirmed")
    );
    assert_eq!(
        member_value(&members, "cpu_confirm_required"),
        &JsonValue::Bool(false)
    );
    assert_eq!(
        member_value(&members, "can_source_exact_probability"),
        &JsonValue::Bool(true)
    );
}

#[test]
fn json_backend_report_includes_memory_ticket_and_fence_epoch() {
    let fields = vec![
        JsonField::new("gpu_memory_ticket_id", "42"),
        JsonField::new("gpu_fence_epoch", "3"),
        JsonField::new("gpu_scope_epoch", "3"),
        JsonField::new("gpu_byte_budget", "4096"),
    ];

    let contract = backend_gpu_worker_contract(&fields);

    assert!(format!("{contract:?}").contains("memory_ticket_id"));
    assert!(format!("{contract:?}").contains("fence_epoch"));
    assert!(format!("{contract:?}").contains("scope_epoch"));
    assert!(format!("{contract:?}").contains("byte_budget"));
}

#[test]
fn json_gpu_worker_report_shows_memory_ticket_and_fence() {
    let fields = vec![
        JsonField::new("gpu_memory_ticket_id", "42"),
        JsonField::new("gpu_fence_epoch", "3"),
        JsonField::new("gpu_scope_epoch", "3"),
        JsonField::new("gpu_byte_budget", "4096"),
    ];

    let JsonValue::Object(members) = backend_gpu_worker_contract(&fields) else {
        panic!("gpu worker object");
    };

    assert_eq!(
        member_value(&members, "memory_ticket_id"),
        &JsonValue::number("42")
    );
    assert_eq!(
        member_value(&members, "fence_epoch"),
        &JsonValue::number("3")
    );
    assert_eq!(
        member_value(&members, "scope_epoch"),
        &JsonValue::number("3")
    );
    assert_eq!(
        member_value(&members, "byte_budget"),
        &JsonValue::number("4096")
    );
}

#[test]
fn json_backend_report_includes_gpu_worker_unavailable_reason() {
    let fields = vec![JsonField::new(
        "gpu_worker_unavailable_reason",
        "gpu_feature_disabled",
    )];

    let JsonValue::Object(members) = backend_gpu_worker_contract(&fields) else {
        panic!("gpu worker object");
    };

    assert_eq!(
        member_value(&members, "unavailable_reason"),
        &JsonValue::string("gpu_feature_disabled")
    );
}

fn member_value<'a>(members: &'a [crate::json::JsonMember], key: &str) -> &'a JsonValue {
    members
        .iter()
        .find_map(|member| (member.key() == key).then_some(member.value()))
        .expect("member exists")
}
