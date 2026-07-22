use super::*;

mod case_pc_contract_exposes_retained_trace_keys_as_array {
    use super::*;

    #[test]
    fn pc_contract_exposes_retained_trace_keys_as_array() {
        let contract = JsonContract::from_render_message(
            "pc-scenario",
            &[
                RenderField::new("retained_trace_key_count", RenderFieldValue::number("2")),
                RenderField::new(
                    "retained_trace_keys",
                    RenderFieldValue::string("trk1:first,trk1:second"),
                ),
            ],
        );

        let JsonValue::Object(root) = contract.root() else {
            panic!("root object");
        };
        let contract = object_member(&root, "contract");
        let pc = object_member(contract, "pc");
        let trace = object_member(pc, "trace");
        assert_eq!(
            member_value(trace, "retained_trace_keys"),
            &JsonValue::array([
                JsonValue::string("trk1:first"),
                JsonValue::string("trk1:second")
            ])
        );
    }
}

mod case_pc_contract_separates_scenario_replay_from_continuation {
    use super::*;

    #[test]
    fn pc_contract_separates_scenario_replay_from_continuation() {
        let contract = JsonContract::from_render_message(
            "pc-scenario",
            &[
                RenderField::new("continuation_token", RenderFieldValue::string("pc2:next")),
                RenderField::new(
                    "scenario_replay_token",
                    RenderFieldValue::string("sr2:again"),
                ),
                RenderField::new(
                    "replay_hint",
                    RenderFieldValue::string("clearra continue sr2:again"),
                ),
                RenderField::new("min_queue_consumed", RenderFieldValue::number("2")),
                RenderField::new("placed_piece_count", RenderFieldValue::number("2")),
                RenderField::new("best_remaining_queue_len", RenderFieldValue::number("5")),
                RenderField::new(
                    "continuation_available_complete",
                    RenderFieldValue::bool(true),
                ),
                RenderField::new("next_pc_available", RenderFieldValue::bool(true)),
                RenderField::new("continue_available", RenderFieldValue::bool(false)),
                RenderField::new(
                    "continuation_token_available",
                    RenderFieldValue::bool(false),
                ),
                RenderField::new(
                    "continuation_token_unavailable_reason",
                    RenderFieldValue::string(
                        "verified_kick_profile_not_encodable_as_opening_token",
                    ),
                ),
                RenderField::new(
                    "continuation_basis",
                    RenderFieldValue::string("best-remaining-frontier-state"),
                ),
                RenderField::new("continuation_queue_consumed", RenderFieldValue::number("1")),
            ],
        );

        let JsonValue::Object(root) = contract.root() else {
            panic!("root object");
        };
        let contract = object_member(&root, "contract");
        let pc = object_member(contract, "pc");
        let continuation = object_member(pc, "continuation");
        let replay = object_member(pc, "replay");
        let remaining = object_member(pc, "remaining");
        assert_eq!(
            member_value(continuation, "token"),
            &JsonValue::string("pc2:next")
        );
        assert_eq!(
            member_value(replay, "token"),
            &JsonValue::string("sr2:again")
        );
        let trace = object_member(pc, "trace");
        assert_eq!(
            member_value(trace, "min_queue_consumed"),
            &JsonValue::number("2")
        );
        assert_eq!(
            member_value(trace, "placed_piece_count"),
            &JsonValue::number("2")
        );
        assert_eq!(
            member_value(remaining, "best_remaining_queue_len"),
            &JsonValue::number("5")
        );
        assert_eq!(
            member_value(remaining, "continuation_available_complete"),
            &JsonValue::Bool(true)
        );
        assert_eq!(
            member_value(remaining, "next_pc_available"),
            &JsonValue::Bool(true)
        );
        assert_eq!(
            member_value(remaining, "continue_available"),
            &JsonValue::Bool(false)
        );
        assert_eq!(
            member_value(remaining, "continuation_token_available"),
            &JsonValue::Bool(false)
        );
        assert_eq!(
            member_value(remaining, "continuation_token_unavailable_reason"),
            &JsonValue::string("verified_kick_profile_not_encodable_as_opening_token")
        );
        assert_eq!(
            member_value(remaining, "continuation_basis"),
            &JsonValue::string("best-remaining-frontier-state")
        );
        assert_eq!(
            member_value(remaining, "continuation_queue_consumed"),
            &JsonValue::number("1")
        );
        assert_eq!(
            member_value(remaining, "replay_hint"),
            &JsonValue::string("clearra continue sr2:again")
        );
    }
}
