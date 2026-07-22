use super::*;

#[test]
fn custom_skin_theme_editor_schema_exposes_safe_theme_fields() {
    let schema = CustomSkinThemeEditorSchema::v1();

    assert_eq!(schema.schema_version(), 1);
    assert!(schema.editor_enabled());
    for key in [
        "skin_id",
        "palette_id",
        "piece_mapping",
        "grid_style",
        "background",
        "line_clear_highlight",
        "ownership_color_mode",
        "export_limits",
        "provenance",
    ] {
        assert!(schema.exposes_field(key), "missing {key}");
    }
    assert!(schema
        .fields()
        .iter()
        .all(CustomSkinThemeEditorFieldSchema::required_flag));
}

#[test]
fn custom_skin_theme_editor_uses_png_atlas_and_user_storage_only() {
    let schema = CustomSkinThemeEditorSchema::v1();

    assert_eq!(schema.runtime_preview_source(), "png-atlas");
    assert!(!schema.raw_svg_runtime_renderer_allowed());
    assert!(schema.manifest_and_provenance_required());
    assert!(!schema.repository_assets_allowed_for_user_imports());
    assert_eq!(
        schema.user_imported_asset_locations(),
        &["user_config_directory", "user_cache_directory"]
    );
}
