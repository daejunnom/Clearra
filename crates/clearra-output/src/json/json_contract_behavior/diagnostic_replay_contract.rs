use super::*;

mod case_diagnostic_contract_includes_evidence_and_suggested_next_step {
    use super::*;

    #[test]
    fn diagnostic_contract_includes_evidence_and_suggested_next_step() {
        let contract = JsonContract::from_render_message(
            "diagnostic",
            &[
                RenderField::new("diagnostic_count", RenderFieldValue::number("1")),
                RenderField::new(
                    "diagnostics",
                    RenderFieldValue::array([RenderFieldValue::object([
                        ("code", RenderFieldValue::string("E_CORE_FFI_BUFFER_BOUNDS")),
                        ("severity", RenderFieldValue::string("error")),
                        (
                            "message",
                            RenderFieldValue::string("native view exceeded C ABI buffer bound"),
                        ),
                        (
                            "location",
                            RenderFieldValue::string("core_ffi.build_variant"),
                        ),
                        (
                            "suggested_next_step",
                            RenderFieldValue::string("Reject the native view before copying."),
                        ),
                        (
                            "evidence",
                            RenderFieldValue::object([(
                                "kick_evidence_count",
                                RenderFieldValue::string("17"),
                            )]),
                        ),
                    ])]),
                ),
            ],
        );

        let JsonValue::Object(root) = contract.root() else {
            panic!("root object");
        };
        let contract = object_member(&root, "contract");
        let diagnostics = object_member(contract, "diagnostics");
        let items = array_member(diagnostics, "items");
        let JsonValue::Object(first) = &items[0] else {
            panic!("diagnostic item object");
        };
        let evidence = object_member(first, "evidence");

        assert_eq!(
            member_value(first, "code"),
            &JsonValue::string("E_CORE_FFI_BUFFER_BOUNDS")
        );
        assert_eq!(
            member_value(first, "suggested_next_step"),
            &JsonValue::string("Reject the native view before copying.")
        );
        assert_eq!(
            member_value(evidence, "kick_evidence_count"),
            &JsonValue::string("17")
        );
    }
}

mod case_replay_trace_contract_preserves_marker_line_clear_and_colored_ownership {
    use super::*;

    #[test]
    fn replay_trace_contract_preserves_marker_line_clear_and_colored_ownership() {
        let layout = Board64Layout::standard_10_by_lines(2).expect("layout");
        let input = BuildVariantReplayInput::new(
            "variant-1",
            layout,
            0,
            vec![BuildVariantOperation::new(
                PieceKind::O,
                RotationState::Zero,
                0,
                0,
            )],
        )
        .with_trace_marker(true, true);
        let trace = ReplayEngine::build_variant_to_trace(&input).expect("trace");
        let contract = JsonContract::from_replay_trace(&trace);

        let JsonValue::Object(root) = contract.root() else {
            panic!("root object");
        };
        let replay = object_member(&root, "replay_trace");
        let steps = array_member(replay, "steps");
        let events = array_member(replay, "events");
        let ownership = object_member(replay, "colored_cell_ownership");

        assert_eq!(
            member_value(replay, "variant_id"),
            &JsonValue::string("variant-1")
        );
        assert_eq!(
            member_value(replay, "representative"),
            &JsonValue::Bool(true)
        );
        assert_eq!(member_value(replay, "sample"), &JsonValue::Bool(true));
        assert_eq!(member_value(replay, "trace_steps"), &JsonValue::number("1"));
        assert!(matches!(
            member_value(replay, "canonical_key"),
            JsonValue::String(key) if key.starts_with("trk1:")
        ));
        assert_eq!(steps.len(), 1);
        assert_eq!(events.len(), 10);
        assert!(events.iter().any(|event| {
            matches!(
                event,
                JsonValue::Object(members)
                    if member_value(members, "type") == &JsonValue::string("drop")
            )
        }));
        assert!(events.iter().any(|event| {
            matches!(
                event,
                JsonValue::Object(members)
                    if member_value(members, "type") == &JsonValue::string("spin-basis")
            )
        }));
        assert!(events.iter().any(|event| {
            matches!(
                event,
                JsonValue::Object(members)
                    if member_value(members, "type") == &JsonValue::string("score-basis")
            )
        }));
        assert!(events.iter().any(|event| {
            matches!(
                event,
                JsonValue::Object(members)
                    if member_value(members, "type") == &JsonValue::string("lock")
                        && matches!(
                            member_value(members, "cleared_cell_owners"),
                            JsonValue::Array(owners) if owners.len() == 0
                        )
            )
        }));
        assert!(events.iter().any(|event| {
            matches!(
                event,
                JsonValue::Object(members)
                    if member_value(members, "type") == &JsonValue::string("board-snapshot")
                        && member_value(members, "phase") == &JsonValue::string("before-placement")
            )
        }));
        assert_eq!(
            member_value(ownership, "owned_cell_count"),
            &JsonValue::number("4")
        );
    }
}
