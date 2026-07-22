use super::*;

#[test]
fn gui_schema_exposes_backend_trust_state() {
    let schema = GuiEditorSchemaV2::v2();
    let backend_options = schema
        .backend_options()
        .options()
        .iter()
        .map(|option| option.value())
        .collect::<Vec<_>>();

    assert_eq!(backend_options, ["auto", "cpu", "gpu", "hybrid"]);
    assert!(schema.backend_result_exposes("backend_fallback_reason"));
    assert!(schema.backend_result_exposes("gpu_trust_state"));
    assert!(schema.exposes_required_field("gpu_trust_state"));
    assert!(schema.exposes_required_field("packing_candidate_count"));
    assert!(schema.exposes_required_field("build_variant_count"));
    assert!(schema.exposes_required_field("total_solution_count"));
    assert!(schema.exposes_required_field("retained_trace_count"));
    assert!(schema.exposes_required_field("coverage_probability"));
}

#[test]
fn gui_schema_exposes_raw_setup_metrics() {
    let schema = GuiEditorSchemaV2::v2();
    let raw = schema.setup_explorer().setup_raw_metrics_schema();

    assert!(raw.requires_field("setup_raw_metrics"));
    assert!(raw.requires_field("raw_coverage_export_path"));
    assert!(raw.requires_field("build_variant_metrics"));
    assert!(schema.exposes_required_field("raw_coverage_export_path"));
    assert!(schema
        .setup_explorer()
        .setup_raw_coverage_export_schema()
        .requires_field("rows"));
}

#[test]
fn gui_schema_exposes_score_accuracy_level() {
    let schema = GuiEditorSchemaV2::v2();

    assert!(schema.score_result_exposes("score_accuracy_level"));
    assert!(schema.score_result_exposes("score_event_basis"));
    assert!(schema.exposes_required_field("score_basis"));
    assert!(schema.exposes_required_field("score_accuracy_level"));
}

#[test]
fn gui_schema_exposes_exact_renderer_asset_status() {
    let schema = GuiEditorSchemaV2::v2();

    assert!(schema
        .render_options()
        .exposes_capability_field("renderer_capability"));
    assert!(schema
        .render_options()
        .exposes_capability_field("skin_manifest_valid"));
    assert!(!schema.render_options().unsupported_reason_required());
    assert!(schema
        .diagnostic_panel()
        .exposes_reason_field("render_asset_invalid"));
    assert!(schema.exposes_required_field("unsupported_reason"));
    assert!(schema.exposes_required_field("renderer_capability"));
}

#[test]
fn gui_schema_does_not_localize_json_keys() {
    let schema = GuiEditorSchemaV2::v2();

    assert!(!schema.json_contract_keys_localized());
    assert!(!schema.diagnostic_panel().json_contract_keys_localized());
    for field in schema.required_display_fields() {
        assert_ne!(field.contract_key(), field.localized_label().key().as_str());
        assert!(field
            .localized_label()
            .key()
            .as_str()
            .starts_with("ui.gui.v2.field."));
    }
}
