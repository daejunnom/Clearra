use super::*;

mod case_render_message_contract_uses_explicit_typed_summary_values {
    use super::*;

    #[test]
    fn render_message_contract_uses_explicit_typed_summary_values() {
        let contract = JsonContract::from_render_message(
            "pc",
            &[
                RenderField::new("solution_found", RenderFieldValue::Bool(true)),
                RenderField::new("total_solution_count", RenderFieldValue::number("12")),
                RenderField::new("queue_mode", RenderFieldValue::string("fixed")),
            ],
        );

        let JsonValue::Object(root) = contract.root() else {
            panic!("root object");
        };
        assert!(root
            .iter()
            .any(|member| member.key() == "schema_version"
                && member.value() == &JsonValue::number("2")));
        assert!(root
            .iter()
            .any(|member| member.key() == "kind" && member.value() == &JsonValue::string("pc")));
    }
}

mod case_render_message_contract_does_not_infer_numeric_looking_strings {
    use super::*;

    #[test]
    fn render_message_contract_does_not_infer_numeric_looking_strings() {
        let contract = JsonContract::from_render_message(
            "pc",
            &[RenderField::new("continuation_token", "001")],
        );

        let JsonValue::Object(root) = contract.root() else {
            panic!("root object");
        };
        let summary = object_member(&root, "summary");
        assert_eq!(
            member_value(summary, "continuation_token"),
            &JsonValue::string("001")
        );
    }
}
