use super::*;

mod case_capacity_exceeded_never_complete {
    use super::*;

    #[test]
    fn capacity_exceeded_never_complete() {
        let contract = JsonContract::from_render_message(
            "pc-scenario",
            &[
                RenderField::new("resource_truncated", RenderFieldValue::bool(true)),
                RenderField::new(
                    "resource_truncation_reason",
                    RenderFieldValue::string("coverage_capacity_exceeded"),
                ),
                RenderField::new(
                    "resource_probability_complete",
                    RenderFieldValue::bool(true),
                ),
                RenderField::new("count_complete", RenderFieldValue::bool(true)),
                RenderField::new("probability_complete", RenderFieldValue::bool(true)),
            ],
        );

        let JsonValue::Object(root) = contract.root() else {
            panic!("root object");
        };
        let resource_report = object_member(&root, "resource_report");

        assert_eq!(
            member_value(resource_report, "truncated"),
            &JsonValue::Bool(true)
        );
        assert_eq!(
            member_value(resource_report, "truncation_reason"),
            &JsonValue::string("coverage_capacity_exceeded")
        );
        assert_eq!(
            member_value(resource_report, "probability_complete"),
            &JsonValue::Bool(false)
        );
    }
}

mod case_resource_cap_output_marked_incomplete {
    use super::*;

    #[test]
    fn resource_cap_output_marked_incomplete() {
        let contract = JsonContract::from_render_message(
            "pc-scenario",
            &[
                RenderField::new("count_complete", RenderFieldValue::bool(false)),
                RenderField::new(
                    "count_truncated_reason",
                    RenderFieldValue::string("buildup_enumeration_truncated"),
                ),
                RenderField::new("probability_complete", RenderFieldValue::bool(true)),
            ],
        );

        let JsonValue::Object(root) = contract.root() else {
            panic!("root object");
        };
        let resource_report = object_member(&root, "resource_report");

        assert_eq!(
            member_value(resource_report, "truncated"),
            &JsonValue::Bool(true)
        );
        assert_eq!(
            member_value(resource_report, "truncation_reason"),
            &JsonValue::string("buildup_enumeration_truncated")
        );
        assert_eq!(
            member_value(resource_report, "count_complete"),
            &JsonValue::Bool(false)
        );
        assert_eq!(
            member_value(resource_report, "probability_complete"),
            &JsonValue::Bool(false)
        );
    }
}

mod case_observed_truncated_universe_not_renormalized {
    use super::*;

    #[test]
    fn observed_truncated_universe_not_renormalized() {
        let contract = JsonContract::from_render_message(
            "percent",
            &[
                RenderField::new("queue_mode", RenderFieldValue::string("observed")),
                RenderField::new("supply_expansion_truncated", RenderFieldValue::bool(true)),
                RenderField::new("supply_probability_complete", RenderFieldValue::bool(false)),
                RenderField::new(
                    "supply_materialized_probability_mass",
                    RenderFieldValue::number("0.5"),
                ),
                RenderField::new("probability_complete", RenderFieldValue::bool(false)),
                RenderField::new("renormalized", RenderFieldValue::bool(false)),
            ],
        );

        let JsonValue::Object(root) = contract.root() else {
            panic!("root object");
        };
        let resource_report = object_member(&root, "resource_report");
        let contract = object_member(&root, "contract");
        let supply = object_member(contract, "supply");

        assert_eq!(
            member_value(resource_report, "truncated"),
            &JsonValue::Bool(true)
        );
        assert_eq!(
            member_value(resource_report, "truncation_reason"),
            &JsonValue::string("observed_universe_truncated")
        );
        assert_eq!(
            member_value(resource_report, "probability_complete"),
            &JsonValue::Bool(false)
        );
        assert_eq!(
            member_value(resource_report, "materialized_probability_mass"),
            &JsonValue::number("0.5")
        );
        assert_eq!(
            member_value(resource_report, "renormalized"),
            &JsonValue::Bool(false)
        );
        assert_eq!(
            member_value(supply, "materialized_probability_mass"),
            &JsonValue::number("0.5")
        );
        assert_eq!(
            member_value(supply, "probability_complete"),
            &JsonValue::Bool(false)
        );
    }
}

mod case_probability_not_calculated_is_not_resource_truncation {
    use super::*;

    #[test]
    fn probability_not_calculated_is_not_resource_truncation() {
        let contract = JsonContract::from_render_message(
            "pc",
            &[
                RenderField::new("resource_truncated", RenderFieldValue::bool(false)),
                RenderField::new("probability_calculated", RenderFieldValue::bool(false)),
                RenderField::new("probability_complete", RenderFieldValue::bool(false)),
                RenderField::new("supply_probability_complete", RenderFieldValue::bool(false)),
                RenderField::new(
                    "resource_probability_complete",
                    RenderFieldValue::bool(false),
                ),
            ],
        );

        let JsonValue::Object(root) = contract.root() else {
            panic!("root object");
        };
        let resource_report = object_member(&root, "resource_report");

        assert_eq!(
            member_value(resource_report, "truncated"),
            &JsonValue::Bool(false)
        );
        assert_eq!(
            member_value(resource_report, "truncation_reason"),
            &JsonValue::Null
        );
        assert_eq!(
            member_value(resource_report, "probability_complete"),
            &JsonValue::Bool(false)
        );
    }
}
