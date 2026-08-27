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
fn verify_probe_counts_are_json_numbers() {
    let fields = SummaryRenderContract::render_fields(vec![
        ("probes_attempted".to_owned(), "1".to_owned()),
        ("probes_passed".to_owned(), "1".to_owned()),
        ("probes_failed".to_owned(), "0".to_owned()),
    ]);
    let message = fields
        .into_iter()
        .fold(RenderMessage::new("verify"), |message, field| {
            message.with_value(field.key().to_owned(), field.value().clone())
        });
    let JsonValue::Object(root) = message.json_contract().root() else {
        panic!("root object");
    };
    let summary = object_member(&root, "summary");

    assert_eq!(
        member_value(summary, "probes_attempted"),
        &JsonValue::number("1")
    );
    assert_eq!(
        member_value(summary, "probes_passed"),
        &JsonValue::number("1")
    );
    assert_eq!(
        member_value(summary, "probes_failed"),
        &JsonValue::number("0")
    );
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
fn failed_queue_summary_keeps_existing_json_types_without_private_provenance_keys() {
    let fields = SummaryRenderContract::render_fields(vec![
        ("result_mode".to_owned(), "failed-queue".to_owned()),
        ("failed_pattern_count".to_owned(), "3".to_owned()),
        ("failed_pattern_limit".to_owned(), "9".to_owned()),
        (
            "failed_pattern_examples_materialized".to_owned(),
            "3".to_owned(),
        ),
        (
            "failed_pattern_examples_truncated".to_owned(),
            "false".to_owned(),
        ),
        ("failed_queue_probability".to_owned(), "0.25".to_owned()),
        ("failed_pattern_0".to_owned(), "IOT".to_owned()),
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

    for key in [
        "failed_pattern_count",
        "failed_pattern_limit",
        "failed_pattern_examples_materialized",
        "failed_queue_probability",
    ] {
        assert!(
            matches!(member_value(summary, key), JsonValue::Number(_)),
            "{key}"
        );
    }
    assert_eq!(
        member_value(summary, "failed_pattern_examples_truncated"),
        &JsonValue::Bool(false)
    );
    assert_eq!(
        member_value(summary, "failed_pattern_0"),
        &JsonValue::string("IOT")
    );
    for private_key in [
        "origin",
        "query",
        "problem_owner",
        "execution_authority",
        "memory_evidence",
    ] {
        assert!(!summary.iter().any(|member| member.key() == private_key));
    }
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

#[test]
fn existential_b2b_projection_keeps_counts_probability_and_completeness_typed() {
    let fields = SummaryRenderContract::render_fields(vec![
        (
            "b2b_preservation_selection".to_owned(),
            "existential".to_owned(),
        ),
        (
            "b2b_preservation_pattern_universe_count".to_owned(),
            "5040".to_owned(),
        ),
        ("b2b_preserving_pattern_count".to_owned(), "1260".to_owned()),
        ("b2b_preservation_probability".to_owned(), "0.25".to_owned()),
        (
            "b2b_preservation_probability_complete".to_owned(),
            "true".to_owned(),
        ),
        (
            "b2b_preservation_count_complete".to_owned(),
            "true".to_owned(),
        ),
        (
            "b2b_preservation_path_multiplicity_counted".to_owned(),
            "false".to_owned(),
        ),
        (
            "b2b_preservation_witness_available".to_owned(),
            "true".to_owned(),
        ),
        (
            "b2b_preserving_candidate_pattern_count".to_owned(),
            "1260".to_owned(),
        ),
        ("b2b_preserving_solution_count".to_owned(), "42".to_owned()),
        (
            "b2b_preservation_witness_pattern_index".to_owned(),
            "17".to_owned(),
        ),
    ]);
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
        member_value(summary, "b2b_preservation_selection"),
        &JsonValue::string("existential")
    );
    assert_eq!(
        member_value(summary, "b2b_preservation_pattern_universe_count"),
        &JsonValue::number("5040")
    );
    assert_eq!(
        member_value(summary, "b2b_preserving_pattern_count"),
        &JsonValue::number("1260")
    );
    assert_eq!(
        member_value(summary, "b2b_preservation_probability"),
        &JsonValue::number("0.25")
    );
    assert_eq!(
        member_value(summary, "b2b_preservation_probability_complete"),
        &JsonValue::Bool(true)
    );
    assert_eq!(
        member_value(summary, "b2b_preservation_count_complete"),
        &JsonValue::Bool(true)
    );
    assert_eq!(
        member_value(summary, "b2b_preservation_path_multiplicity_counted"),
        &JsonValue::Bool(false)
    );
    assert_eq!(
        member_value(summary, "b2b_preservation_witness_available"),
        &JsonValue::Bool(true)
    );
    assert_eq!(
        member_value(summary, "b2b_preserving_candidate_pattern_count"),
        &JsonValue::number("1260")
    );
    assert_eq!(
        member_value(summary, "b2b_preserving_solution_count"),
        &JsonValue::number("42")
    );
    assert_eq!(
        member_value(summary, "b2b_preservation_witness_pattern_index"),
        &JsonValue::number("17")
    );
}

#[test]
fn unavailable_b2b_witness_index_stays_a_typed_status_string() {
    let fields = SummaryRenderContract::render_fields(vec![
        (
            "b2b_preservation_witness_available".to_owned(),
            "false".to_owned(),
        ),
        (
            "b2b_preservation_witness_pattern_index".to_owned(),
            "not-materialized".to_owned(),
        ),
    ]);
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
        member_value(summary, "b2b_preservation_witness_available"),
        &JsonValue::Bool(false)
    );
    assert_eq!(
        member_value(summary, "b2b_preservation_witness_pattern_index"),
        &JsonValue::string("not-materialized")
    );
}

#[test]
fn pc_allspin_result_fields_keep_complete_values_typed_in_json() {
    let fields = SummaryRenderContract::render_fields(vec![
        ("pc_allspin_complete".to_owned(), "true".to_owned()),
        ("pc_allspin_count_complete".to_owned(), "true".to_owned()),
        (
            "pc_allspin_probability_complete".to_owned(),
            "true".to_owned(),
        ),
        (
            "pc_allspin_path_multiplicity_counted".to_owned(),
            "false".to_owned(),
        ),
        (
            "pc_allspin_initial_field_supplied".to_owned(),
            "true".to_owned(),
        ),
        (
            "pc_allspin_target_field_supplied".to_owned(),
            "false".to_owned(),
        ),
        ("pc_allspin_preserves_b2b".to_owned(), "true".to_owned()),
        ("pc_allspin_witness_available".to_owned(), "true".to_owned()),
        (
            "pc_allspin_witness_deterministic".to_owned(),
            "true".to_owned(),
        ),
        (
            "pc_allspin_preserving_queue_count".to_owned(),
            "1".to_owned(),
        ),
        ("pc_allspin_original_queue_count".to_owned(), "1".to_owned()),
        (
            "pc_allspin_preservation_probability".to_owned(),
            "1".to_owned(),
        ),
        (
            "pc_allspin_witness_pattern_index".to_owned(),
            "0".to_owned(),
        ),
    ]);
    let message = fields
        .into_iter()
        .fold(RenderMessage::new("pc"), |message, field| {
            message.with_value(field.key().to_owned(), field.value().clone())
        });
    let JsonValue::Object(root) = message.json_contract().root() else {
        panic!("root object");
    };
    let summary = object_member(&root, "summary");

    for key in [
        "pc_allspin_complete",
        "pc_allspin_count_complete",
        "pc_allspin_probability_complete",
        "pc_allspin_preserves_b2b",
        "pc_allspin_witness_available",
        "pc_allspin_witness_deterministic",
        "pc_allspin_initial_field_supplied",
    ] {
        assert_eq!(member_value(summary, key), &JsonValue::Bool(true), "{key}");
    }
    assert_eq!(
        member_value(summary, "pc_allspin_path_multiplicity_counted"),
        &JsonValue::Bool(false)
    );
    assert_eq!(
        member_value(summary, "pc_allspin_target_field_supplied"),
        &JsonValue::Bool(false)
    );
    for (key, value) in [
        ("pc_allspin_preserving_queue_count", "1"),
        ("pc_allspin_original_queue_count", "1"),
        ("pc_allspin_preservation_probability", "1"),
        ("pc_allspin_witness_pattern_index", "0"),
    ] {
        assert_eq!(
            member_value(summary, key),
            &JsonValue::number(value),
            "{key}"
        );
    }
}

#[test]
fn incomplete_pc_allspin_values_remain_status_strings_in_json() {
    let fields = SummaryRenderContract::render_fields(vec![
        ("pc_allspin_complete".to_owned(), "false".to_owned()),
        (
            "pc_allspin_preserves_b2b".to_owned(),
            "not-calculated".to_owned(),
        ),
        (
            "pc_allspin_preservation_probability".to_owned(),
            "not-calculated".to_owned(),
        ),
        (
            "pc_allspin_witness_pattern_index".to_owned(),
            "not-materialized".to_owned(),
        ),
    ]);
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
        member_value(summary, "pc_allspin_complete"),
        &JsonValue::Bool(false)
    );
    assert_eq!(
        member_value(summary, "pc_allspin_preserves_b2b"),
        &JsonValue::string("not-calculated")
    );
    assert_eq!(
        member_value(summary, "pc_allspin_preservation_probability"),
        &JsonValue::string("not-calculated")
    );
    assert_eq!(
        member_value(summary, "pc_allspin_witness_pattern_index"),
        &JsonValue::string("not-materialized")
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
