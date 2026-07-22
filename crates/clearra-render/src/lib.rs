//! Skin, atlas, and bitmap render capability contracts for GUI/image output.

pub mod asset_import;
pub mod bitmap;
pub mod capability;
pub mod error;
pub mod export;
pub mod options;
pub mod scene;
pub mod skin;

#[cfg(feature = "asset-import")]
pub use asset_import::{
    rasterize_sanitized_svg, AssetImportBundle, AssetImportMetadata, AssetImportPipeline,
};
pub use asset_import::{
    sanitize_svg, AssetImportLimits, AssetImportReportValidator, RuntimeAssetGate,
    SvgSecurityScanner,
};
pub use bitmap::{ExactBitmapRenderer, RenderBoard, RenderCell};
pub use capability::{
    RenderCapability, RenderCapabilityReport, RenderExactnessGate, RenderFrameFormat,
    RenderUnsupportedReason,
};
pub use error::RenderError;
pub use export::RenderExportLimits;
pub use options::RenderOptions;
pub use scene::{RenderFramePhase, RenderScene, RenderSceneFrame, RenderTile};
pub use skin::{
    AtlasBoundsValidator, AtlasRect, CustomSkinBackground, CustomSkinExportLimits,
    CustomSkinGridStyle, CustomSkinLineClearHighlight, CustomSkinOwnershipColorMode,
    CustomSkinPieceMapping, CustomSkinProvenance, CustomSkinThemeSchema, CustomSkinValidationError,
    CustomThemePreviewSource, SkinAtlas, SkinManifest, SkinManifestValidator, SkinProvenance,
    SkinProvenanceValidator, UserImportedSkinAssetLocation,
};

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
