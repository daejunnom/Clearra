use crate::{
    json::{JsonContract, JsonMember, JsonValue},
    model::{RenderField, RenderFieldValue},
};

#[test]
fn setup_contract_exposes_x3_raw_metrics_without_condition_summary() {
    let contract = JsonContract::from_render_message(
        "setup",
        &[
            RenderField::new(
                "shape_family_id",
                RenderFieldValue::string("setup-family-0"),
            ),
            RenderField::new(
                "setup_raw_metrics_schema_version",
                RenderFieldValue::number("2"),
            ),
            RenderField::new(
                "metrics_kind",
                RenderFieldValue::string("setup_raw_metrics"),
            ),
            RenderField::new("shape_family_count", RenderFieldValue::number("1")),
            RenderField::new("tiling_variant_count", RenderFieldValue::number("2")),
            RenderField::new("build_variant_count", RenderFieldValue::number("3")),
            RenderField::new("covered_pattern_count", RenderFieldValue::number("5")),
            RenderField::new("coverage_probability", RenderFieldValue::number("0.625")),
            RenderField::new("queue_prefix", RenderFieldValue::string("IOTS")),
            RenderField::new("queue_prefix_len", RenderFieldValue::number("4")),
            RenderField::new("hold_required", RenderFieldValue::bool(true)),
            RenderField::new("hold_piece", RenderFieldValue::string("T")),
            RenderField::new("bag_boundary_offsets", RenderFieldValue::string("0,7")),
            RenderField::new("bag_boundary_ambiguous", RenderFieldValue::bool(true)),
            RenderField::new("requires_180", RenderFieldValue::bool(false)),
            RenderField::new(
                "requires_180_evidence",
                RenderFieldValue::string("not-modeled"),
            ),
            RenderField::new(
                "rule_profile_evidence",
                RenderFieldValue::string("srs-plus"),
            ),
            RenderField::new("post_pc_solution_count", RenderFieldValue::number("13")),
            RenderField::new("score_basis", RenderFieldValue::string("retained-traces")),
            RenderField::new("score_aggregation_attached", RenderFieldValue::bool(true)),
            RenderField::new("backend_report", RenderFieldValue::string("attached")),
            RenderField::new(
                "raw_coverage_export_path",
                RenderFieldValue::string(
                    "inline://clearra/setup/raw-coverage/setup-family-0/union",
                ),
            ),
            RenderField::new("setup_raw_metrics", RenderFieldValue::string("attached")),
            RenderField::new(
                "setup_raw_coverage_export",
                RenderFieldValue::string("inline"),
            ),
            RenderField::new(
                "coverage_overlap_report",
                RenderFieldValue::string("overlap-visible"),
            ),
            RenderField::new(
                "build_variant_metrics",
                RenderFieldValue::string("per-build-variant"),
            ),
            RenderField::new("diagnostic_evidence", RenderFieldValue::string("attached")),
            RenderField::new("raw_coverage_schema_version", RenderFieldValue::number("2")),
            RenderField::new(
                "raw_coverage_export_kind",
                RenderFieldValue::string("setup_raw_coverage_export"),
            ),
            RenderField::new("pattern_universe_id", RenderFieldValue::number("1001")),
            RenderField::new("pattern_weight_model_id", RenderFieldValue::number("2001")),
            RenderField::new("pattern_count", RenderFieldValue::number("8")),
            RenderField::new(
                "rows",
                RenderFieldValue::string("machine-readable-coverage-rows"),
            ),
            RenderField::new(
                "family_unions",
                RenderFieldValue::string("machine-readable-family-unions"),
            ),
            RenderField::new("overlap_report", RenderFieldValue::string("visible")),
        ],
    );

    let JsonValue::Object(root) = contract.root() else {
        panic!("root object");
    };
    let contract = object_member(&root, "contract");
    let setup = object_member(contract, "setup");
    let search = object_member(setup, "search");
    let raw_metrics = object_member(setup, "raw_metrics");
    let raw_coverage = object_member(setup, "raw_coverage");

    assert_eq!(
        member_value(search, "shape_family_id"),
        &JsonValue::string("setup-family-0")
    );
    assert_eq!(
        member_value(raw_metrics, "schema_version"),
        &JsonValue::number("2")
    );
    assert_eq!(
        member_value(raw_metrics, "metrics_kind"),
        &JsonValue::string("setup_raw_metrics")
    );
    assert_eq!(
        member_value(raw_metrics, "tiling_variant_count"),
        &JsonValue::number("2")
    );
    assert_eq!(
        member_value(raw_metrics, "shape_family_count"),
        &JsonValue::number("1")
    );
    assert_eq!(
        member_value(raw_metrics, "build_variant_count"),
        &JsonValue::number("3")
    );
    assert_eq!(
        member_value(raw_metrics, "covered_pattern_count"),
        &JsonValue::number("5")
    );
    assert_eq!(
        member_value(raw_metrics, "queue_prefix"),
        &JsonValue::string("IOTS")
    );
    assert_eq!(
        member_value(raw_metrics, "queue_prefix_len"),
        &JsonValue::number("4")
    );
    assert_eq!(
        member_value(raw_metrics, "hold_required"),
        &JsonValue::Bool(true)
    );
    assert_eq!(
        member_value(raw_metrics, "hold_piece"),
        &JsonValue::string("T")
    );
    assert_eq!(
        member_value(raw_metrics, "bag_boundary_offsets"),
        &JsonValue::string("0,7")
    );
    assert_eq!(
        member_value(raw_metrics, "bag_boundary_ambiguous"),
        &JsonValue::Bool(true)
    );
    assert_eq!(
        member_value(raw_metrics, "requires_180"),
        &JsonValue::Bool(false)
    );
    assert_eq!(
        member_value(raw_metrics, "requires_180_evidence"),
        &JsonValue::string("not-modeled")
    );
    assert_eq!(
        member_value(raw_metrics, "rule_profile_evidence"),
        &JsonValue::string("srs-plus")
    );
    assert_eq!(
        member_value(raw_metrics, "post_pc_solution_count"),
        &JsonValue::number("13")
    );
    assert_eq!(
        member_value(raw_metrics, "score_basis"),
        &JsonValue::string("retained-traces")
    );
    assert_eq!(
        member_value(raw_metrics, "backend_report"),
        &JsonValue::string("attached")
    );
    assert_eq!(
        member_value(raw_metrics, "setup_raw_metrics"),
        &JsonValue::string("attached")
    );
    assert_eq!(
        member_value(raw_metrics, "setup_raw_coverage_export"),
        &JsonValue::string("inline")
    );
    assert_eq!(
        member_value(raw_metrics, "coverage_overlap_report"),
        &JsonValue::string("overlap-visible")
    );
    assert_eq!(
        member_value(raw_metrics, "build_variant_metrics"),
        &JsonValue::string("per-build-variant")
    );
    assert_eq!(
        member_value(raw_metrics, "diagnostic_evidence"),
        &JsonValue::string("attached")
    );
    assert_eq!(
        member_value(raw_coverage, "schema_version"),
        &JsonValue::number("2")
    );
    assert_eq!(
        member_value(raw_coverage, "export_kind"),
        &JsonValue::string("setup_raw_coverage_export")
    );
    assert_eq!(
        member_value(raw_coverage, "pattern_universe_id"),
        &JsonValue::number("1001")
    );
    assert_eq!(
        member_value(raw_coverage, "pattern_weight_model_id"),
        &JsonValue::number("2001")
    );
    assert_eq!(
        member_value(raw_coverage, "pattern_count"),
        &JsonValue::number("8")
    );
    assert_eq!(
        member_value(raw_coverage, "rows"),
        &JsonValue::string("machine-readable-coverage-rows")
    );
    assert_eq!(
        member_value(raw_coverage, "family_unions"),
        &JsonValue::string("machine-readable-family-unions")
    );
    assert_eq!(
        member_value(raw_coverage, "overlap_report"),
        &JsonValue::string("visible")
    );
    assert_eq!(
        member_value(raw_coverage, "raw_coverage_export_path"),
        &JsonValue::string("inline://clearra/setup/raw-coverage/setup-family-0/union")
    );
    assert!(!format!("{:?}", contract).contains("condition_summary"));
}

#[test]
fn setup_raw_metrics_no_condition_summary() {
    setup_contract_exposes_x3_raw_metrics_without_condition_summary();
}

fn member_value<'a>(members: &'a [JsonMember], key: &str) -> &'a JsonValue {
    members
        .iter()
        .find_map(|member| (member.key() == key).then_some(member.value()))
        .expect("member exists")
}

fn object_member<'a>(members: &'a [JsonMember], key: &str) -> &'a [JsonMember] {
    let JsonValue::Object(nested) = member_value(members, key) else {
        panic!("object member");
    };
    nested
}
