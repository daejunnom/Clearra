use super::{
    require_non_empty, CustomSkinBackground, CustomSkinExportLimits, CustomSkinGridStyle,
    CustomSkinLineClearHighlight, CustomSkinOwnershipColorMode, CustomSkinPieceMapping,
    CustomSkinProvenance, CustomSkinValidationError, CustomThemePreviewSource,
    UserImportedSkinAssetLocation, STANDARD_PIECES,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CustomSkinThemeSchema {
    skin_id: String,
    palette_id: String,
    piece_mapping: Vec<CustomSkinPieceMapping>,
    grid_style: CustomSkinGridStyle,
    background: CustomSkinBackground,
    line_clear_highlight: CustomSkinLineClearHighlight,
    ownership_color_mode: CustomSkinOwnershipColorMode,
    export_limits: CustomSkinExportLimits,
    provenance: Option<CustomSkinProvenance>,
    asset_location: UserImportedSkinAssetLocation,
    preview_source: CustomThemePreviewSource,
    manifest_and_provenance_required: bool,
    runtime_raw_svg_allowed: bool,
}

impl CustomSkinThemeSchema {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        skin_id: impl Into<String>,
        palette_id: impl Into<String>,
        piece_mapping: Vec<CustomSkinPieceMapping>,
        grid_style: CustomSkinGridStyle,
        background: CustomSkinBackground,
        line_clear_highlight: CustomSkinLineClearHighlight,
        ownership_color_mode: CustomSkinOwnershipColorMode,
        export_limits: CustomSkinExportLimits,
        provenance: Option<CustomSkinProvenance>,
        asset_location: UserImportedSkinAssetLocation,
        preview_source: CustomThemePreviewSource,
    ) -> Self {
        Self {
            skin_id: skin_id.into(),
            palette_id: palette_id.into(),
            piece_mapping,
            grid_style,
            background,
            line_clear_highlight,
            ownership_color_mode,
            export_limits,
            provenance,
            asset_location,
            preview_source,
            manifest_and_provenance_required: true,
            runtime_raw_svg_allowed: false,
        }
    }
}
impl CustomSkinThemeSchema {
    pub fn validate(&self) -> Result<(), CustomSkinValidationError> {
        require_non_empty(&self.skin_id, CustomSkinValidationError::MissingSkinId)?;
        require_non_empty(
            &self.palette_id,
            CustomSkinValidationError::MissingPaletteId,
        )?;
        self.validate_piece_mapping()?;
        self.grid_style.validate()?;
        self.background.validate()?;
        self.line_clear_highlight.validate()?;
        self.export_limits.validate()?;
        self.provenance
            .as_ref()
            .ok_or(CustomSkinValidationError::MissingProvenance)?
            .validate()?;
        if !self.manifest_and_provenance_required {
            return Err(CustomSkinValidationError::ManifestAndProvenanceRequired);
        }
        if self.asset_location == UserImportedSkinAssetLocation::RepositoryAssets {
            return Err(CustomSkinValidationError::UserImportStoredInRepositoryAssets);
        }
        if self.preview_source != CustomThemePreviewSource::PngAtlas {
            return Err(CustomSkinValidationError::RawSvgPreviewForbidden);
        }
        if self.runtime_raw_svg_allowed {
            return Err(CustomSkinValidationError::RawSvgPreviewForbidden);
        }
        Ok(())
    }
}
impl CustomSkinThemeSchema {
    pub fn preview_source(&self) -> CustomThemePreviewSource {
        self.preview_source
    }
}
impl CustomSkinThemeSchema {
    pub fn asset_location(&self) -> UserImportedSkinAssetLocation {
        self.asset_location
    }
}
impl CustomSkinThemeSchema {
    pub fn ownership_color_mode(&self) -> CustomSkinOwnershipColorMode {
        self.ownership_color_mode
    }
}
impl CustomSkinThemeSchema {
    pub fn set_runtime_raw_svg_allowed_for_test(&mut self, allowed: bool) {
        self.runtime_raw_svg_allowed = allowed;
    }
}
impl CustomSkinThemeSchema {
    pub fn set_preview_source_for_test(&mut self, source: CustomThemePreviewSource) {
        self.preview_source = source;
    }
}
impl CustomSkinThemeSchema {
    pub fn set_asset_location_for_test(&mut self, location: UserImportedSkinAssetLocation) {
        self.asset_location = location;
    }
}
impl CustomSkinThemeSchema {
    pub fn clear_provenance_for_test(&mut self) {
        self.provenance = None;
    }
}
impl CustomSkinThemeSchema {
    fn validate_piece_mapping(&self) -> Result<(), CustomSkinValidationError> {
        for piece in STANDARD_PIECES {
            let mapping = self
                .piece_mapping
                .iter()
                .find(|mapping| mapping.piece == piece)
                .ok_or(CustomSkinValidationError::MissingPieceMapping)?;
            require_non_empty(
                &mapping.atlas_rect_id,
                CustomSkinValidationError::MissingPieceMapping,
            )?;
        }
        Ok(())
    }
}
