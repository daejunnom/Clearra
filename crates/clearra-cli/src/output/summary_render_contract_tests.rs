use clearra_output::json::json_contract::JsonValue;
use clearra_output::model::RenderMessage;

use super::*;

#[test]
fn contract_uses_exact_keys_without_suffix_inference() {
    let fields = SummaryRenderContract::render_fields(vec![
        ("queue_count".to_owned(), "001".to_owned()),
        ("queue_len".to_owned(), "7".to_owned()),
    ]);
    let message = fields
        .into_iter()
        .fold(RenderMessage::new("test"), |message, field| {
            message.with_value(field.key().to_owned(), field.value().clone())
        });
    let JsonValue::Object(root) = message.json_contract().root() else {
        panic!("root object");
    };
    let summary = object_member(&root, "summary");

    assert_eq!(
        member_value(summary, "queue_count"),
        &JsonValue::string("001")
    );
    assert_eq!(member_value(summary, "queue_len"), &JsonValue::number("7"));
}

#[test]
fn contract_exposes_retained_trace_keys_as_array() {
    let fields = SummaryRenderContract::render_fields(vec![(
        "retained_trace_keys".to_owned(),
        "trk1:a,trk1:b".to_owned(),
    )]);
    let message = fields
        .into_iter()
        .fold(RenderMessage::new("pc-scenario"), |message, field| {
            message.with_value(field.key().to_owned(), field.value().clone())
        });
    let JsonValue::Object(root) = message.json_contract().root() else {
        panic!("root object");
    };
    let summary = object_member(&root, "summary");

    assert_eq!(
        member_value(summary, "retained_trace_keys"),
        &JsonValue::array([JsonValue::string("trk1:a"), JsonValue::string("trk1:b")])
    );
}

#[test]
fn contract_preserves_solution_trace_mode_as_string() {
    let fields = SummaryRenderContract::render_fields(vec![(
        "solution_trace_mode".to_owned(),
        "sample-only".to_owned(),
    )]);
    let message = fields
        .into_iter()
        .fold(RenderMessage::new("pc"), |message, field| {
            message.with_value(field.key().to_owned(), field.value().clone())
        });
    let JsonValue::Object(root) = message.json_contract().root() else {
        panic!("root object");
    };
    let summary = object_member(&root, "summary");

    assert_eq!(
        member_value(summary, "solution_trace_mode"),
        &JsonValue::string("sample-only")
    );
}

#[test]
fn contract_preserves_not_calculated_solution_metadata_as_strings() {
    let fields = SummaryRenderContract::render_fields(vec![
        (
            "unique_solution_count".to_owned(),
            "not-calculated".to_owned(),
        ),
        (
            "normalized_solution_set_hash".to_owned(),
            "not-calculated".to_owned(),
        ),
        ("coverage_probability".to_owned(), "unavailable".to_owned()),
    ]);
    let message = fields
        .into_iter()
        .fold(RenderMessage::new("failed-queue"), |message, field| {
            message.with_value(field.key().to_owned(), field.value().clone())
        });
    let JsonValue::Object(root) = message.json_contract().root() else {
        panic!("root object");
    };
    let summary = object_member(&root, "summary");

    assert_eq!(
        member_value(summary, "unique_solution_count"),
        &JsonValue::string("not-calculated")
    );
    assert_eq!(
        member_value(summary, "normalized_solution_set_hash"),
        &JsonValue::string("not-calculated")
    );
    assert_eq!(
        member_value(summary, "coverage_probability"),
        &JsonValue::string("unavailable")
    );
}

#[test]
fn terminal_supply_projection_fields_keep_their_public_json_types() {
    let fields = SummaryRenderContract::render_fields(vec![
        ("projects_unplaced_lookahead".to_owned(), "true".to_owned()),
        (
            "projects_standard_bag_lookahead".to_owned(),
            "false".to_owned(),
        ),
        ("source_sequence_length".to_owned(), "7".to_owned()),
        (
            "normalized_unique_solution_count".to_owned(),
            "18".to_owned(),
        ),
        (
            "actual_normalized_unique_solution_count".to_owned(),
            "18".to_owned(),
        ),
    ]);
    let message = fields
        .into_iter()
        .fold(RenderMessage::new("pc-scenario"), |message, field| {
            message.with_value(field.key().to_owned(), field.value().clone())
        });
    let JsonValue::Object(root) = message.json_contract().root() else {
        panic!("root object");
    };
    let summary = object_member(&root, "summary");

    assert_eq!(
        member_value(summary, "projects_unplaced_lookahead"),
        &JsonValue::Bool(true)
    );
    assert_eq!(
        member_value(summary, "projects_standard_bag_lookahead"),
        &JsonValue::Bool(false)
    );
    assert_eq!(
        member_value(summary, "source_sequence_length"),
        &JsonValue::number("7")
    );
    assert_eq!(
        member_value(summary, "normalized_unique_solution_count"),
        &JsonValue::number("18")
    );
    assert_eq!(
        member_value(summary, "actual_normalized_unique_solution_count"),
        &JsonValue::number("18")
    );
}

fn member_value<'a>(
    members: &'a [clearra_output::json::json_contract::JsonMember],
    key: &str,
) -> &'a JsonValue {
    members
        .iter()
        .find_map(|member| (member.key() == key).then_some(member.value()))
        .expect("member exists")
}

fn object_member<'a>(
    members: &'a [clearra_output::json::json_contract::JsonMember],
    key: &str,
) -> &'a [clearra_output::json::json_contract::JsonMember] {
    let JsonValue::Object(nested) = member_value(members, key) else {
        panic!("object member");
    };
    nested
}
