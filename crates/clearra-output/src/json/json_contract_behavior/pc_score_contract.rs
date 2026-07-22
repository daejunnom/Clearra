use super::*;

mod case_pc_contract_exposes_scoring_post_processing_contract {
    use super::*;

    #[test]
    fn pc_contract_exposes_scoring_post_processing_contract() {
        let contract = JsonContract::from_render_message(
            "pc",
            &[
                RenderField::new("score_post_processing", RenderFieldValue::Bool(true)),
                RenderField::new("score_core_hot_path", RenderFieldValue::Bool(false)),
                RenderField::new("score_profile", RenderFieldValue::string("tetrio")),
                RenderField::new("score_profile_id", RenderFieldValue::string("tetrio")),
                RenderField::new("score_model_id", RenderFieldValue::string("tetrio")),
                RenderField::new("attack_model_id", RenderFieldValue::string("tetrio")),
                RenderField::new("spin_rule_id", RenderFieldValue::string("t-spins")),
                RenderField::new(
                    "spin_award_policy",
                    RenderFieldValue::string("t-spins-only"),
                ),
                RenderField::new(
                    "drop_score_policy",
                    RenderFieldValue::string("hard-drop-2-soft-drop-1"),
                ),
                RenderField::new("level_policy", RenderFieldValue::string("disabled")),
                RenderField::new("combo_policy", RenderFieldValue::string("linear")),
                RenderField::new("b2b_policy", RenderFieldValue::string("standard")),
                RenderField::new("pc_bonus_policy", RenderFieldValue::string("disabled")),
                RenderField::new(
                    "trace_requirement",
                    RenderFieldValue::string("full-drop-trace"),
                ),
                RenderField::new(
                    "score_accuracy_level",
                    RenderFieldValue::string("basic-approximation"),
                ),
                RenderField::new(
                    "score_accuracy_reason",
                    RenderFieldValue::string("profile-specific basic score table"),
                ),
                RenderField::new(
                    "score_profile_accuracy_mode",
                    RenderFieldValue::string("basic-approximation"),
                ),
                RenderField::new(
                    "score_profile_specific_exact",
                    RenderFieldValue::Bool(false),
                ),
                RenderField::new("score_event_basis", RenderFieldValue::string("c-replay")),
                RenderField::new(
                    "score_evaluation_trace_count",
                    RenderFieldValue::number("1"),
                ),
                RenderField::new("score_evaluation_complete", RenderFieldValue::Bool(true)),
                RenderField::new(
                    "score_evaluation_basis",
                    RenderFieldValue::string("all-traces"),
                ),
                RenderField::new("score_evaluation_scope", RenderFieldValue::string("full")),
                RenderField::new("score_best_score", RenderFieldValue::number("3200")),
                RenderField::new("score_best_attack", RenderFieldValue::number("10")),
                RenderField::new("score_event_count", RenderFieldValue::number("5")),
                RenderField::new("score_probability_before", RenderFieldValue::number("1.0")),
                RenderField::new("score_probability_after", RenderFieldValue::number("1.0")),
                RenderField::new(
                    "score_does_not_change_probability_union",
                    RenderFieldValue::Bool(true),
                ),
                RenderField::new("placement_event_available", RenderFieldValue::Bool(true)),
                RenderField::new("clear_event_available", RenderFieldValue::Bool(true)),
                RenderField::new("drop_event_basis_available", RenderFieldValue::Bool(true)),
                RenderField::new("spin_event_basis_available", RenderFieldValue::Bool(true)),
                RenderField::new("spin_target_id", RenderFieldValue::string("tsd")),
                RenderField::new(
                    "spin_target_request",
                    RenderFieldValue::string("SpinTargetRequest"),
                ),
                RenderField::new(
                    "spin_target_predicate",
                    RenderFieldValue::string("SpinTargetPredicate"),
                ),
                RenderField::new(
                    "target_probability_threshold",
                    RenderFieldValue::number("0.5"),
                ),
                RenderField::new("score_profile_id", RenderFieldValue::string("tetrio")),
                RenderField::new(
                    "trace_requirement",
                    RenderFieldValue::string("kick-evidence-required"),
                ),
                RenderField::new(
                    "spin_classifier_id",
                    RenderFieldValue::string("kick-sensitive-special"),
                ),
                RenderField::new("spin_classifier_exact", RenderFieldValue::Bool(false)),
                RenderField::new(
                    "spin_trace_completeness",
                    RenderFieldValue::string("missing-kick-evidence"),
                ),
                RenderField::new("spin_probability_complete", RenderFieldValue::Bool(false)),
                RenderField::new(
                    "spin_probability_diagnostic_code",
                    RenderFieldValue::string("W_SPIN_TARGET_PROBABILITY_INCOMPLETE"),
                ),
                RenderField::new(
                    "spin_coverage_reducer",
                    RenderFieldValue::string("PatternBitSet OR union"),
                ),
                RenderField::new(
                    "spin_coverage_row_kind",
                    RenderFieldValue::string("CoverageRowKind::SpinTarget"),
                ),
                RenderField::new("spin_probability", RenderFieldValue::number("0.5")),
                RenderField::new("spin_covered_pattern_count", RenderFieldValue::number("2")),
                RenderField::new("spin_pattern_count", RenderFieldValue::number("4")),
            ],
        );

        let JsonValue::Object(root) = contract.root() else {
            panic!("root object");
        };
        let contract = object_member(&root, "contract");
        let pc = object_member(contract, "pc");
        let scoring = object_member(pc, "scoring");
        let spin_target = object_member(pc, "spin_target");
        let objective = object_member(pc, "objective");
        assert_eq!(
            member_value(scoring, "score_post_processing"),
            &JsonValue::Bool(true)
        );
        assert_eq!(
            member_value(scoring, "score_core_hot_path"),
            &JsonValue::Bool(false)
        );
        assert_eq!(
            member_value(scoring, "score_model_id"),
            &JsonValue::string("tetrio")
        );
        assert_eq!(
            member_value(scoring, "attack_model_id"),
            &JsonValue::string("tetrio")
        );
        assert_eq!(
            member_value(scoring, "spin_rule_id"),
            &JsonValue::string("t-spins")
        );
        assert_eq!(
            member_value(scoring, "trace_requirement"),
            &JsonValue::string("full-drop-trace")
        );
        assert_eq!(
            member_value(scoring, "score_accuracy_level"),
            &JsonValue::string("basic-approximation")
        );
        assert_eq!(
            member_value(scoring, "score_profile_accuracy_mode"),
            &JsonValue::string("basic-approximation")
        );
        assert_eq!(
            member_value(scoring, "score_event_basis"),
            &JsonValue::string("c-replay")
        );
        assert_eq!(
            member_value(scoring, "score_evaluation_scope"),
            &JsonValue::string("full")
        );
        assert_eq!(
            member_value(scoring, "score_best_score"),
            &JsonValue::number("3200")
        );
        assert_eq!(
            member_value(scoring, "score_does_not_change_probability_union"),
            &JsonValue::Bool(true)
        );
        assert_eq!(
            member_value(scoring, "drop_event_basis_available"),
            &JsonValue::Bool(true)
        );
        assert_eq!(
            member_value(scoring, "spin_event_basis_available"),
            &JsonValue::Bool(true)
        );
        assert_eq!(
            member_value(spin_target, "spin_target_id"),
            &JsonValue::string("tsd")
        );
        assert_eq!(
            member_value(spin_target, "spin_target_predicate"),
            &JsonValue::string("SpinTargetPredicate")
        );
        assert_eq!(
            member_value(spin_target, "spin_probability_complete"),
            &JsonValue::Bool(false)
        );
        assert_eq!(
            member_value(spin_target, "spin_probability_diagnostic_code"),
            &JsonValue::string("W_SPIN_TARGET_PROBABILITY_INCOMPLETE")
        );
        assert_eq!(
            member_value(spin_target, "spin_coverage_reducer"),
            &JsonValue::string("PatternBitSet OR union")
        );
        assert_eq!(
            member_value(objective, "max_score_cover"),
            &JsonValue::string("connected-approximate")
        );
        assert_eq!(
            member_value(objective, "best_score_by_pattern_count"),
            &JsonValue::number("2")
        );
        assert_eq!(
            member_value(objective, "score_probability_no_double_count"),
            &JsonValue::Bool(true)
        );
        assert_eq!(
            member_value(objective, "score_does_not_modify_coverage_probability"),
            &JsonValue::Bool(true)
        );
    }
}
