#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CustomSkinPieceMapping {
    pub(crate) piece: String,
    pub(crate) atlas_rect_id: String,
}

impl CustomSkinPieceMapping {
    pub fn new(piece: impl Into<String>, atlas_rect_id: impl Into<String>) -> Self {
        Self {
            piece: piece.into(),
            atlas_rect_id: atlas_rect_id.into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CustomSkinGridStyle {
    line_color: String,
    opacity_percent: u8,
}

impl CustomSkinGridStyle {
    pub fn new(line_color: impl Into<String>, opacity_percent: u8) -> Self {
        Self {
            line_color: line_color.into(),
            opacity_percent,
        }
    }
}
impl CustomSkinGridStyle {
    pub(crate) fn validate(&self) -> Result<(), CustomSkinValidationError> {
        require_non_empty(
            &self.line_color,
            CustomSkinValidationError::MissingGridStyle,
        )?;
        require_percent(self.opacity_percent)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CustomSkinBackground {
    color: String,
}

impl CustomSkinBackground {
    pub fn new(color: impl Into<String>) -> Self {
        Self {
            color: color.into(),
        }
    }
}
impl CustomSkinBackground {
    pub(crate) fn validate(&self) -> Result<(), CustomSkinValidationError> {
        require_non_empty(&self.color, CustomSkinValidationError::MissingBackground)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CustomSkinLineClearHighlight {
    color: String,
    opacity_percent: u8,
}

impl CustomSkinLineClearHighlight {
    pub fn new(color: impl Into<String>, opacity_percent: u8) -> Self {
        Self {
            color: color.into(),
            opacity_percent,
        }
    }
}
impl CustomSkinLineClearHighlight {
    pub(crate) fn validate(&self) -> Result<(), CustomSkinValidationError> {
        require_non_empty(
            &self.color,
            CustomSkinValidationError::MissingLineClearHighlight,
        )?;
        require_percent(self.opacity_percent)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CustomSkinOwnershipColorMode {
    PieceColor,
    PlayerColor,
    Disabled,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CustomSkinExportLimits {
    max_export_bytes: u64,
    max_frame_width: u32,
    max_frame_height: u32,
}

impl CustomSkinExportLimits {
    pub fn new(max_export_bytes: u64, max_frame_width: u32, max_frame_height: u32) -> Self {
        Self {
            max_export_bytes,
            max_frame_width,
            max_frame_height,
        }
    }
}
impl CustomSkinExportLimits {
    pub(crate) fn validate(&self) -> Result<(), CustomSkinValidationError> {
        if self.max_export_bytes == 0 || self.max_frame_width == 0 || self.max_frame_height == 0 {
            return Err(CustomSkinValidationError::InvalidExportLimits);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CustomSkinProvenance {
    source_label: String,
    license: String,
    redistribution: String,
    manifest_id: String,
    import_report_id: String,
}

impl CustomSkinProvenance {
    pub fn new(
        source_label: impl Into<String>,
        license: impl Into<String>,
        redistribution: impl Into<String>,
        manifest_id: impl Into<String>,
        import_report_id: impl Into<String>,
    ) -> Self {
        Self {
            source_label: source_label.into(),
            license: license.into(),
            redistribution: redistribution.into(),
            manifest_id: manifest_id.into(),
            import_report_id: import_report_id.into(),
        }
    }
}
impl CustomSkinProvenance {
    pub(crate) fn validate(&self) -> Result<(), CustomSkinValidationError> {
        require_non_empty(
            &self.source_label,
            CustomSkinValidationError::MissingProvenance,
        )?;
        require_non_empty(&self.license, CustomSkinValidationError::MissingProvenance)?;
        require_non_empty(
            &self.redistribution,
            CustomSkinValidationError::MissingProvenance,
        )?;
        require_non_empty(
            &self.manifest_id,
            CustomSkinValidationError::ManifestAndProvenanceRequired,
        )?;
        require_non_empty(
            &self.import_report_id,
            CustomSkinValidationError::ManifestAndProvenanceRequired,
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UserImportedSkinAssetLocation {
    UserConfigDirectory,
    UserCacheDirectory,
    RepositoryAssets,
}

impl UserImportedSkinAssetLocation {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::UserConfigDirectory => "user_config_directory",
            Self::UserCacheDirectory => "user_cache_directory",
            Self::RepositoryAssets => "repository_assets",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CustomThemePreviewSource {
    PngAtlas,
    RawSvg,
}

impl CustomThemePreviewSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PngAtlas => "png-atlas",
            Self::RawSvg => "raw-svg",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CustomSkinValidationError {
    MissingSkinId,
    MissingPaletteId,
    MissingPieceMapping,
    MissingGridStyle,
    MissingBackground,
    MissingLineClearHighlight,
    MissingProvenance,
    ManifestAndProvenanceRequired,
    UserImportStoredInRepositoryAssets,
    RawSvgPreviewForbidden,
    InvalidExportLimits,
}

pub(crate) fn require_non_empty(
    value: &str,
    error: CustomSkinValidationError,
) -> Result<(), CustomSkinValidationError> {
    if value.trim().is_empty() {
        Err(error)
    } else {
        Ok(())
    }
}

fn require_percent(value: u8) -> Result<(), CustomSkinValidationError> {
    if value > 100 {
        Err(CustomSkinValidationError::InvalidExportLimits)
    } else {
        Ok(())
    }
}
