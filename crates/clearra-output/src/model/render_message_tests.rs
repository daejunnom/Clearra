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
fn path_json_stays_exhaustive_while_default_text_hides_internal_identity() {
    for (kind, public_kind) in [
        ("pc-path-family.v2", "perfect-clear replay paths"),
        ("build-path-family.v1", "build replay paths"),
    ] {
        let message = RenderMessage::new(kind)
            .with_value("problem_id", "problem-private")
            .with_value("materialized_pattern_count", "2")
            .with_value("witness_count", "2")
            .with_value("complete", true)
            .with_value("canonical_selection", "smallest-canonical-candidate-id")
            .with_value("canonical_witness", "candidate=1;pattern=2;trace=trace-a")
            .with_value("target_terminal_board_mask", "0x000000000000000f")
            .with_value(
                "witnesses",
                RenderFieldValue::array([
                    RenderFieldValue::string("candidate=1;operation=9;trace=trace-a"),
                    RenderFieldValue::string("candidate=2;operation=7;trace=trace-b"),
                ]),
            );

        let human = message.text_lines().join("\n");
        assert!(human.contains(&format!("kind: {public_kind}")));
        assert!(human.contains("materialized_pattern_count: 2"));
        assert!(human.contains("witness_count: 2"));
        assert!(human.contains("complete: true"));
        for private in [
            kind,
            "problem-private",
            "canonical_selection",
            "canonical_witness",
            "candidate=",
            "pattern=",
            "operation=",
            "trace=",
            "target_terminal_board_mask",
        ] {
            assert!(
                !human.contains(private),
                "default text leaked {private}: {human}"
            );
        }

        let verbose = message
            .text_lines_with_profile(TextOutputProfile::Verbose)
            .join("\n");
        assert!(verbose.contains(kind));
        assert!(verbose.contains("canonical_witness"));

        let contract = message.json_contract();
        let crate::json::json_contract::JsonValue::Object(root) = contract.root() else {
            panic!("root object");
        };
        let summary = object_member(&root, "summary");
        let crate::json::json_contract::JsonValue::Array(witnesses) =
            member_value(summary, "witnesses")
        else {
            panic!("witness array");
        };
        assert_eq!(witnesses.len(), 2);
    }
}

#[test]
fn product_human_text_is_fail_closed_while_machine_profiles_keep_identity_evidence() {
    for (kind, public_kind, public_key, source_value, human_value) in [
        (
            "pc-score-summary.v2",
            "perfect-clear field average score",
            "score_overall_score",
            "1234",
            "1234",
        ),
        (
            "build-coverage.v2",
            "build result",
            "union_probability",
            "0.75",
            "75%",
        ),
        (
            "setup-build-ranking.v2",
            "setup ranking",
            "candidate_count",
            "3",
            "3",
        ),
    ] {
        let message = RenderMessage::new(kind)
            .with_value("status", "searched")
            .with_value(public_key, source_value)
            .with_value("schema_id", "schema-private.v99")
            .with_value("problem_id", "problem-private")
            .with_value("pattern_id", "pattern-private")
            .with_value("candidate_id", "candidate-private")
            .with_value("trace_identity", "trace-private")
            .with_value("operation_id", "operation-private")
            .with_value("group_key", "group-private");

        let human = message.text_lines().join("\n");
        assert!(human.contains(&format!("kind: {public_kind}")), "{human}");
        assert!(
            human.contains(&format!("{public_key}: {human_value}")),
            "{human}"
        );
        for private in [
            kind,
            "schema-private.v99",
            "problem-private",
            "pattern-private",
            "candidate-private",
            "trace-private",
            "operation-private",
            "group-private",
        ] {
            assert!(
                !human.contains(private),
                "default {kind} text leaked {private}: {human}"
            );
        }

        for profile in [TextOutputProfile::Verbose, TextOutputProfile::Diagnostics] {
            let machine = message.text_lines_with_profile(profile).join("\n");
            assert!(machine.contains(kind), "{profile:?}: {machine}");
            assert!(
                machine.contains("schema-private.v99"),
                "{profile:?}: {machine}"
            );
            assert!(
                machine.contains("candidate-private"),
                "{profile:?}: {machine}"
            );
            assert!(machine.contains("trace-private"), "{profile:?}: {machine}");
        }

        let contract = message.json_contract();
        let crate::json::json_contract::JsonValue::Object(root) = contract.root() else {
            panic!("root object");
        };
        let summary = object_member(&root, "summary");
        assert_eq!(
            member_value(summary, "candidate_id"),
            &crate::json::json_contract::JsonValue::string("candidate-private")
        );
        assert_eq!(
            member_value(summary, "trace_identity"),
            &crate::json::json_contract::JsonValue::string("trace-private")
        );
    }
}

#[test]
fn failed_queue_contract_is_machine_evidence_not_default_human_text() {
    let message = RenderMessage::new("pc-failed-queue.v2")
        .with_value("status", "searched")
        .with_value("failed_pattern_count", 2usize)
        .with_value(
            "failed_queue_contract",
            "exact-build-coverage-complement.v1",
        );

    let human = message.text_lines().join("\n");
    assert!(human.contains("failed_pattern_count: 2"), "{human}");
    assert!(!human.contains("failed_queue_contract"), "{human}");
    assert!(
        !human.contains("exact-build-coverage-complement.v1"),
        "{human}"
    );

    for profile in [TextOutputProfile::Verbose, TextOutputProfile::Diagnostics] {
        let machine = message.text_lines_with_profile(profile).join("\n");
        assert!(machine.contains("failed_queue_contract"), "{machine}");
        assert!(
            machine.contains("exact-build-coverage-complement.v1"),
            "{machine}"
        );
    }

    let contract = message.json_contract();
    let crate::json::json_contract::JsonValue::Object(root) = contract.root() else {
        panic!("root object");
    };
    let summary = object_member(&root, "summary");
    assert_eq!(
        member_value(summary, "failed_queue_contract"),
        &crate::json::json_contract::JsonValue::string("exact-build-coverage-complement.v1",)
    );
}

#[test]
fn unknown_result_kinds_hide_kind_and_fields_in_default_text() {
    for kind in ["future-product.v99", "candidate_id=kind-private"] {
        let message = RenderMessage::new(kind)
            .with_value("status", "searched")
            .with_value("candidate_id", "future-private");

        let human = message.text_lines().join("\n");
        assert!(human.contains("kind: result"));
        assert!(human.contains("status: searched"));
        assert!(!human.contains(kind));
        assert!(!human.contains("future-private"));

        let verbose = message
            .text_lines_with_profile(TextOutputProfile::Verbose)
            .join("\n");
        assert!(verbose.contains(kind));
        assert!(verbose.contains("future-private"));

        if kind.ends_with(".v99") {
            let diagnostics = message
                .text_lines_with_profile(TextOutputProfile::Diagnostics)
                .join("\n");
            assert!(diagnostics.contains(kind));
            assert!(diagnostics.contains("future-private"));
        }
    }
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
