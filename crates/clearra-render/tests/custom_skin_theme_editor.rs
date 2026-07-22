use clearra_render::{
    CustomSkinBackground, CustomSkinExportLimits, CustomSkinGridStyle,
    CustomSkinLineClearHighlight, CustomSkinOwnershipColorMode, CustomSkinPieceMapping,
    CustomSkinProvenance, CustomSkinThemeSchema, CustomSkinValidationError,
    CustomThemePreviewSource, UserImportedSkinAssetLocation,
};

const STANDARD_PIECES: [&str; 7] = ["I", "O", "T", "S", "Z", "J", "L"];

fn valid_schema() -> CustomSkinThemeSchema {
    CustomSkinThemeSchema::new(
        "user-skin",
        "palette-night",
        STANDARD_PIECES
            .iter()
            .map(|piece| CustomSkinPieceMapping::new(*piece, format!("{piece}-rect")))
            .collect(),
        CustomSkinGridStyle::new("#445566", 35),
        CustomSkinBackground::new("#101216"),
        CustomSkinLineClearHighlight::new("#f6d365", 65),
        CustomSkinOwnershipColorMode::PieceColor,
        CustomSkinExportLimits::new(1_048_576, 1920, 1080),
        Some(CustomSkinProvenance::new(
            "human reviewed user import",
            "user-confirmed-license",
            "private-user-cache-only",
            "manifest-user-skin-v1",
            "asset-import-report-user-skin-v1",
        )),
        UserImportedSkinAssetLocation::UserConfigDirectory,
        CustomThemePreviewSource::PngAtlas,
    )
}

#[test]
fn custom_skin_schema_validates() {
    let schema = valid_schema();

    assert_eq!(schema.validate(), Ok(()));
    assert_eq!(schema.preview_source().as_str(), "png-atlas");
    assert_eq!(schema.asset_location().as_str(), "user_config_directory");
    assert_eq!(
        schema.ownership_color_mode(),
        CustomSkinOwnershipColorMode::PieceColor
    );
}

#[test]
fn custom_skin_import_requires_provenance() {
    let mut schema = valid_schema();
    schema.clear_provenance_for_test();

    assert_eq!(
        schema.validate(),
        Err(CustomSkinValidationError::MissingProvenance)
    );
}

#[test]
fn custom_theme_preview_uses_png_atlas() {
    let mut schema = valid_schema();
    schema.set_preview_source_for_test(CustomThemePreviewSource::RawSvg);

    assert_eq!(
        schema.validate(),
        Err(CustomSkinValidationError::RawSvgPreviewForbidden)
    );
}

#[test]
fn raw_svg_not_passed_to_runtime_renderer() {
    let mut schema = valid_schema();
    schema.set_runtime_raw_svg_allowed_for_test(true);

    assert_eq!(
        schema.validate(),
        Err(CustomSkinValidationError::RawSvgPreviewForbidden)
    );
}

#[test]
fn user_imported_asset_is_not_repository_assets() {
    let mut schema = valid_schema();
    schema.set_asset_location_for_test(UserImportedSkinAssetLocation::RepositoryAssets);

    assert_eq!(
        schema.validate(),
        Err(CustomSkinValidationError::UserImportStoredInRepositoryAssets)
    );
}
